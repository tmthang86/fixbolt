# ADR-0087 — Recovery reaches the sharded runtime, and the journal crosses the channel with the connection

- **Status**: Accepted — 2026-09-20. The owner delegated the technical decisions of
  [the-desk-free-residue](../plans/2026-09-20-the-desk-free-residue.md) to the architect on
  2026-09-20. **Nothing here is built**: the entry points, the enum and the test named below are
  *to be written* in step 5 of that plan.
- **Date**: 2026-09-20
- **Deciders**: Tran Manh Thang (mandate); written by the architect.
- **Related**: [ADR-0034](ADR-0034-recovery-is-asked-once-the-counterparty-is-known.md)
  decision 3 — **the deferral it recorded is lifted here, not contradicted**: it declined to
  touch `shard.rs` because the author could not run Linux; the machine this plan runs on is
  Linux. [ADR-0039](ADR-0039-a-fresh-journal-is-the-deployments-to-build.md) (`Recovery::fresh`,
  and the *Bad* bullet naming this gap), [ADR-0015](ADR-0015-explicit-cores-pinned-from-inside-and-read-back.md) (the shard
  runtime), [ADR-0030](ADR-0030-one-engine-holds-many-counterparties.md) (the pre-session stage
  chooses the `Config`), [ADR-0020](ADR-0020-a-pre-session-stage-owns-the-socket-until-logon.md)
  (the acceptor thread may block), `CLAUDE.md` §2 non-negotiables 1, 4, 6, 7.
- **Closes**: `STATUS.md` item 32 **(a)**, the recovery half. The ordered-shutdown half of the
  same sentence stays open — see *Consequences*.

## Context

`serve_with_recovery` and `serve_hft_with_recovery` (`crates/engine/src/lib.rs`) ask a
[`Recovery`] once per connection, on the acceptor thread, the moment the pre-session stage has
named the counterparty, and hand the answer to the engine through
`Engine::add_with_prefix_config_and_journal`. `serve_sharded_hft` and `serve_sharded_hft_with`
(`crates/engine/src/shard.rs`) do the same pre-session work and then send the settled `Pending`
down an `mpsc` channel to a shard thread, which calls `Shardable::add(transport, cfg, prefix)` —
and `Shardable`'s blanket implementation is `Engine::add_with_prefix_and_config`, which builds a
fresh `J::default()` journal every time. A sharded deployment therefore cannot resume a session
across a restart, and the `Shardable` bound `J: SessionJournal + Default` is the same bound
ADR-0039 removed from the single-threaded loop, still standing here.

The two moments that matter are on different threads. The counterparty is known on the acceptor
thread, which ADR-0020 allows to block, so that is where a `Recovery` may read a file. The
session is built on the shard thread, which in `hft` mode never blocks (`CLAUDE.md` §2
non-negotiable 4). Whatever recovery produced must therefore **travel** from the first thread to
the second, with the connection.

## Decision

### 1. The journal crosses the channel with the connection

A new type in `crates/engine/src/recovery.rs`:

```rust
/// How a connection's session begins: fresh, with a journal the deployment built, or
/// continued from what a `Recovery` found. Decided on the acceptor thread, carried to the
/// thread that builds the session.
pub enum Start<J> {
    Fresh(J),
    Resumed(Resumed<J>),
}
```

The acceptor thread asks `recovery.recover(&cfg)` and, on `None`, `recovery.fresh(&cfg)` —
both on the thread that may block — and sends `(Pending, Start<J>)` down the shard's channel
instead of `Pending` alone. Nothing about the journal is decided on the shard thread.

### 2. `Shardable` learns to start a session from a `Start<J>`

`Shardable` gains

```rust
fn add_started(&mut self, transport: TcpTransport, cfg: Config, prefix: &[u8], start: Start<J>) -> bool;
```

with the blanket implementation forwarding to `Engine::add_with_prefix_config_and_journal`.
The existing `add` stays and is implemented as `add_started(…, Start::Fresh(J::default()))`
so every current caller and every test in `tests/shard.rs` compiles unchanged. The
`J: Default` bound moves from the `impl Shardable for Engine` header onto `add` alone, which is
where ADR-0039 decision 2 says such a bound belongs — on the callers that want it.

`Shardable` therefore becomes generic over `J`. `Shards<PRE>` becomes `Shards<PRE, J = Store>`
so `Shards::<PRE>::start` in `tests/shard.rs` and `tests/shard_wire.rs` keeps meaning what it
means today.

### 3. Two new doors, and the old ones delegate through them

`serve_sharded_hft_with_recovery` and `serve_sharded_hft_with_recovery_with<N, RX, TX, APP, …>`
take a `recovery: V where V: Recovery<J>` and are the only sharded serving loop. `serve_sharded_hft`
and `serve_sharded_hft_with` become one-line calls through `NoRecovery` — the shape ADR-0034
decision 3 gave `serve` and `serve_with_recovery`, so there is **one** loop, and the fresh path
is the resumed path with `None` for an answer rather than a second copy of the loop.

Both new items carry `#[cfg(feature = "standard")]` exactly as `serve_sharded_hft` does, and
the test file carries the `#![cfg]` that `tests/shard_hft.rs` learned the hard way to carry.

### 4. What crosses the channel must be `Send`

`Start<J>` rides the channel, so the sharded doors require `J: Send + 'static`. `Store` is.
A `FileJournal` is expected to be (it owns a file handle); the plan's step 5 asserts it with a
compile-time `fn assert_send<T: Send>()` in the test rather than by reading the struct.

### 5. Not decided here: ordered shutdown of the sharded runtime

Item 32 (a)'s sentence also says the sharded door "cannot be stopped". That needs a `Shutdown`
that spans threads and a `Logout` sequence per shard, which is a design of its own and is
**left open**, in the same `STATUS.md` row, with this ADR named as the reason recovery is no
longer in the sentence.

## Consequences

**Good**

- A sharded `hft` deployment can resume every session after a restart with the same `Recovery`
  a single-threaded one uses; there is one recovery seam, asked in one place, on the thread
  allowed to block.
- One serving loop for sharded `hft`, not two. The `NoRecovery` path is exercised by the
  existing `serve_sharded_hft_serves_a_session` (`34=1` for a session nobody resumed), which
  stays green **unmodified** and so proves the fresh path did not change.
- The `J: Default` bound leaves the shard runtime's header, closing the same hole ADR-0039
  closed for `pump`.

**Bad — and accepted**

- **`Shardable` and `Shards` change shape in public.** A caller who implemented `Shardable` for
  a type of their own gains a required method; a caller who named `Shards<PRE>` is saved by the
  default type parameter, but one who wrote `Shards<PRE, _>`-style inference elsewhere is not.
  `CHANGELOG.md` says so.
- **`recover` and `fresh` still run on the acceptor thread unbounded.** A slow `Recovery`
  delays every pre-session connection behind it — the same *Bad* bullet ADR-0034 and ADR-0039
  carry, now for the sharded door too. Nothing bounds it and this ADR does not pretend to.
- **A journal is built for a connection that may be refused a shard.** `fresh` is called before
  `hand`, and `hand` can still fail on `NoIdentity` or `BadRoute`; the journal is dropped with
  the socket. For a `FileJournal` that may mean a file opened and closed for nothing. Ordering
  it the other way would call `fresh` on the shard thread, which may not block.
- **The channel carries more bytes per connection.** A `Pending` is a `PRE`-byte array moved by
  value; `Start<J>` adds the journal. Connection arrival is not the per-message hot path and the
  channel is the one `Pending` already rides, so `benches/alloc.rs` is unaffected, but a
  `Resumed` with a large in-memory `Store` is now moved across threads at logon.
- **The sharded runtime still cannot be stopped.** Item 32 stays open for that half.

## Alternatives rejected

| Alternative | Why not |
|---|---|
| Call `Recovery::recover` on the shard thread | It may read a file; the shard thread never blocks in `hft` (non-negotiable 4) |
| Keep `Pending` alone on the channel and ship a `Recovery` clone to every shard | Two threads asking one journal store about the same counterparty, and `Recovery` has no `Clone` bound; also moves the file read onto the shard thread |
| A second loop, `serve_sharded_hft_with_recovery`, beside the old one | Two copies of the pre-session loop to keep in step; ADR-0034 decision 3 chose one loop for the single-threaded door for the same reason |
| Change `serve_sharded_hft`'s signature in place | Every caller (`tests/shard_hft.rs`, `docs/GUIDE.md` §1a) breaks for a parameter most of them do not want |

[`Recovery`]: ../../crates/engine/src/recovery.rs
