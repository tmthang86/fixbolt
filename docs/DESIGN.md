# fixbolt — Design

How the engine is built, and the latency budget it is built against. What it must do and in
which phase is [PRD.md](PRD.md); how to embed it is [GUIDE.md](GUIDE.md); the reasons behind
each decision, with their costs, are the ADRs in [decisions/](decisions/); the measurements
that justified them are in [reference/measured-costs.md](reference/measured-costs.md).

**What it is.** A FIX 4.4 engine in Rust, **bidirectional**: acceptor and initiator on one
session core, chosen by a type parameter
([ADR-0004](decisions/ADR-0004-bidirectional-engine.md)), built so that latency is a property
the design guarantees rather than one it hopes for.

**Positioning.** The fastest FIX acceptor that can be built **on kernel TCP**. The acceptor is
the headline because that is where the gap is: as of 2026-08-27 the Rust ecosystem has no
production-proven FIX acceptor and already has two initiators
([reference/prior-art.md](reference/prior-art.md)). The initiator ships in the same phase,
held to the same gates.

**What it is not.** Not an HFT client, not kernel bypass. FIX over the kernel stack has a
floor of roughly **10–20 µs** wire-to-wire that no codec can move (§8). The job is to make
everything above that floor disappear, and to measure the floor honestly.

**The shape it assumes.** One session on one isolated polling thread
([ADR-0012](decisions/ADR-0012-latency-first-and-one-session-per-polling-thread.md)). Inside
`hft`, latency beats session density, and reversing that needs its own ADR. Many sessions on
one thread is supported and named **`density`**; it costs `[measured 2026-08-31]`
**`N × 449 ns`** of polling per turn on a core set up to §9 and does not inherit the latency
figures on this page. Every figure here names its `N`.

**Sections.** §1 the finding the architecture is built around · §2 layers · §3 crates ·
§4 decisions D1–D15 · §5 non-goals · §6 gates · §7 build order · §8 latency budget · §9 the
OS checklist.

---

## 1. The finding this architecture is built around

A mature C++ FIX engine (fix8, 68% faster than QuickFIX) encodes a NewOrderSingle in
**2.1 µs** on production hardware, and says that **1.4 µs** of that remains with the framework
stripped out. A Rust flyweight parser on an Apple M5 parsed the same message shape in
**139 ns** on 2026-08-27.

The gap is not the bytes. It is the framework: object models, dictionary lookups at runtime,
virtual dispatch, mandatory validation. `hffix` confirms it from the other side: it deletes
the framework entirely (parse in place, no session layer) and is the fastest thing in the
survey.

Three principles follow, each learned in order:

1. **Keep the framework off the hot path.** Every layer below is shaped by it.
2. **The codec is about 1% of the wire-to-wire budget on kernel TCP.** A design that
   optimises the codec and says nothing about I/O strategy, outbound encoding, or the OS
   underneath has optimised the wrong 1%. D8–D10, §8 and §9 exist because of that.
3. **Price the I/O strategy too.** `[measured 2026-08-31]` the strategy D8 chose, one
   non-blocking `read` per connection per turn, costs a whole `Engine::turn` of **449 ns per
   session**, of which about **420 ns is the syscall** and about 30 ns is everything the
   engine itself does. This document's own parse is 122.6 ns on the same machine, so the
   syscall that discovers there is nothing to parse costs 3.8× the parse. The codec had been
   priced and the I/O strategy had not. [ADR-0012](decisions/ADR-0012-latency-first-and-one-session-per-polling-thread.md)
   is the consequence: latency wins over session density, the budget is stated end to end
   including syscalls, and every figure names its `N`.

## 2. Layers

```
┌─────────────────────────────────────────────────────────────┐
│  Application — implements Handler (library) or Application  │
└──────────────────────────▲──────────────────────────────────┘
                           │
     ┌─────────────────────┴──────────────────────┐
     │  InlineDispatch — same thread as the       │  ← the default (D4)
     │  session machine, zero hops, the borrowed  │
     │  MessageView handed straight through       │
     │                  ── or ──                  │
     │  RingDispatch — SPSC ring, application on  │  ← the option: an application
     │  its own thread                            │    that may block cannot stall
     └─────────────────────┬──────────────────────┘    the session layer
┌──────────────────────────┴──────────────────────────────────┐
│ L4  library    the application-facing API, package fixbolt  │
├─────────────────────────────────────────────────────────────┤
│ L3  engine     TCP accept and connect (ADR-0004), drives    │
│                the session machines, owns the journal       │
├─────────────────────────────────────────────────────────────┤
│ L2  session    FIX session protocol as a PURE state machine │
│                — no sockets, no clock, no I/O.              │
│                Role { Acceptor, Initiator } is a parameter  │
├─────────────────────────────────────────────────────────────┤
│ L1  codec      parse / serialise in place, zero allocation  │
├─────────────────────────────────────────────────────────────┤
│ L0  transport  trait. TCP is the default; TLS is a second   │
│                implementation behind a feature flag (D11)   │
└─────────────────────────────────────────────────────────────┘
```

## 3. Crates

Added one at a time, each behind an approved plan. All of them exist.

| Crate | Layer | Owns | Depends on |
|---|---|---|---|
| `codec` | L1 | Parse and serialise in place. The hot path. `no_std`-compatible is the goal; zero dependencies is the rule | — |
| `dict` | build | Code generation from the FIX XML: tag constants, message shapes, required-field tables, **field ordering**, group delimiters and members, and the validation tables (defined tags, message types, per-message tag sets, field types, enum values) | `codec`; it implements `codec::Dictionary` |
| `session` | L2 | The FIX session state machine. Pure, no I/O, `Role` as a type parameter. Time enters as `Tick` in milliseconds since 0000-01-01 (D13). Module `schedule` holds when a session is open and when both ends restart at `34=1` (ADR-0033) | `codec`, `dict` |
| `engine` | L3 | TCP acceptor and connector, drives the session machines, owns the journal and the message log. `transport` is a module here until something needs it to be a crate | `session`; `libc` **only** under the `standard` or `affinity` feature |
| `library` | L4 | The application-facing API, package **`fixbolt`**: `Handler`, `Incoming`, `Reply`, `App`, and a curated re-export of what an application needs (`serve`, `Config`, `Table`, `Limits`, `Settings`, `Handles`, `Observer`, `Admin`, `Recovery`, `FileJournal`, `FileLog`, …). `Engine`, `Dispatch`, `Transport`, `wait`, `shard`, `affinity`, `frame` and `ring` are deliberately absent; reaching for one means naming `fixbolt-engine` yourself | `engine` |
| `conformance` | dev | The `.def` acceptance runner for both roles, the corpus loader, and the echo application the corpus assumes. Built **before** `session`, so the gate existed before the thing it gates | `codec`, `dict` |
| `tools/w2w` | tool | The wire-to-wire harness, and the binary the two mode checks trace. Counts its own allocations on both threads over the timed window and asserts zero. Has its own `[features]` block, because a `cfg` never reaches into a dependency's features | `engine`, `session`, `codec`, `dict` |
| `tools/jrnl` | tool | Reads a journal file from outside the process that wrote it; warns on a torn tail or a bad checksum with exit code 2. Takes `engine` with `default-features = false` | `engine` |
| `tools/interop` | tool | Both roles against a real `libquickfix` over kernel TCP. The C++ counterparties are built by `scripts/interop.sh`, never by cargo | `library` (as `fixbolt`, no default features), `session` |

### What `engine` contains

The crate is the largest, and its modules are the record of what was built when:

| Module | What it is | Decision |
|---|---|---|
| `transport` | `Transport`, `TcpTransport`, `Loopback`, `Waiting` | D5 |
| `poll`, `block`, `waker` | `poll(2)` and `standard`'s idle turn, behind `#[cfg(all(feature = "standard", unix))]`. The crate's first external dependency and first `unsafe`, both behind that feature | [ADR-0014](decisions/ADR-0014-standard-mode-blocks-on-poll.md) |
| `affinity` | `CoreId`, `pin_current_thread` (reads the mask back), `running_on` (reads `/proc/thread-self/stat`), `Topology`, `ShardPlan`, `spawn_pinned`. Behind `#[cfg(all(feature = "affinity", target_os = "linux"))]`, off by default. Two `unsafe` blocks, each naming its test | [ADR-0015](decisions/ADR-0015-explicit-cores-pinned-from-inside-and-read-back.md), [ADR-0019](decisions/ADR-0019-two-unsafe-blocks-and-an-error-the-enum-can-hold.md) |
| `shard` | `Shards`, `Shardable`, `serve_sharded_hft`: one pinned thread per shard, each confirming its own pin before any of them serves. The acceptor thread blocks, because it is not an engine thread | [ADR-0020](decisions/ADR-0020-a-pre-session-stage-owns-the-socket-until-logon.md), [ADR-0022](decisions/ADR-0022-the-pre-session-stage-enforces-two-definitions.md) |
| `presession` | `Identity`, `identity_of`, `is_logon` (reads `49=` / `56=` / `35=` by field scan, no dictionary); `Limits`, `PendingSet` (owns a socket until its first whole message, under a deadline and a ceiling with no defaults); `Registry`, `Entry`, `Table` (which counterparty, decided before a session exists; a trait, and `None` from `lookup` is the authentication hook); `Route`, `HashRoute` (the shard a socket goes to, decided after its Logon). Everything allocated once, to the ceiling | [ADR-0020](decisions/ADR-0020-a-pre-session-stage-owns-the-socket-until-logon.md), [ADR-0026](decisions/ADR-0026-a-counterparty-registry-in-the-pre-session-stage.md), [ADR-0029](decisions/ADR-0029-the-pre-session-stage-enforces-four-definitions.md), [ADR-0030](decisions/ADR-0030-one-engine-holds-many-counterparties.md) |
| `journal` | `MemJournal` (the resend ring), `FileJournal` (append-only file with `Durability::{Async, Fsync}`, reloaded into a ring on open), `Reader` (the whole file, for a tool; allows itself to allocate), `Store = MemJournal<4096, 512>`. Records carry a CRC32 from format version 1 | [ADR-0008](decisions/ADR-0008-journal-is-a-trait.md), [ADR-0017](decisions/ADR-0017-the-inbound-count-is-persisted-after-delivery.md), [ADR-0037](decisions/ADR-0037-reading-a-journal-is-not-recovering-from-one.md), [ADR-0039](decisions/ADR-0039-a-fresh-journal-is-the-deployments-to-build.md), [ADR-0046](decisions/ADR-0046-the-ring-is-the-resend-store-and-a-replay-goes-in-batches.md) |
| `recovery` | `Recovery` (asked once per connection, after the registry names the counterparty, on the acceptor thread), `Resumed`, `NoRecovery`, `FromFn`; `serve_with_recovery`, `serve_hft_with_recovery`, generic over the journal. `Engine::add_resumed` is the seam for a caller driving the engine | [ADR-0034](decisions/ADR-0034-recovery-is-asked-once-the-counterparty-is-known.md), [ADR-0039](decisions/ADR-0039-a-fresh-journal-is-the-deployments-to-build.md) |
| `observe` | `Handles` (the one cell, made before the engine, adopted by it — [ADR-0054](decisions/ADR-0054-the-handles-are-made-before-the-engine-and-the-engine-adopts-them.md)); `Observer` / `Snapshot` / `SessionSnapshot` (on request; one relaxed load per turn while nobody asks; a fixed `[SessionSnapshot; 64]` plus `truncated`); `Event` / `EventKind` (pushed on a state change, never per message, losses counted); `Admin` / `Command` (the 3 a.m. operation, applied at the top of a turn; a refused `try_lock` loses nothing, a full queue refuses at the call) | [ADR-0032](decisions/ADR-0032-observation-is-a-snapshot-taken-on-request.md), [ADR-0035](decisions/ADR-0035-an-event-is-pushed-and-a-loss-is-counted.md), [ADR-0036](decisions/ADR-0036-one-mechanism-two-capabilities.md), [ADR-0054](decisions/ADR-0054-the-handles-are-made-before-the-engine-and-the-engine-adopts-them.md) |
| `settings` | `Settings`, `SettingsError`, `Problem`: the QuickFIX-shaped configuration file, no dependency, strict. An unrecognised key, a file with no `[SESSION]`, and a half-written schedule are each errors carrying a line number | [ADR-0040](decisions/ADR-0040-a-configuration-file-refuses-what-it-does-not-understand.md) |
| `reconnect` | `Policy`: doubling backoff to a ceiling, no jitter; `connect_and_serve` | [ADR-0043](decisions/ADR-0043-backoff-without-jitter-and-a-reconnect-asks-recovery-every-time.md) |
| shutdown | `Admin::shutdown`, `Shutdown`, `Session::begin_logout`, `State::LoggingOut`, `DropReason::EngineShutdown`; `run`, `serve` and `serve_hft` **return** | [ADR-0038](decisions/ADR-0038-an-ordered-shutdown-is-a-state-not-a-flag.md) |
| `msglog` | `MessageLog`, `NoLog` (compiles away), `FileLog`, `FileLog::open_pinned`: both directions, refusals included, one line per message | D14 |
| `origin` | `Sender` (`Send + Sync`, a third capability on `observe`'s `Arc`), the fixed origination queue, `ORIGIN_CAPACITY` and `ORIGIN_LEN`. Drained at the top of a turn beside `Command`; `Engine::speak_first` is the other door and lives in `lib.rs` | D15, [ADR-0048](decisions/ADR-0048-an-engine-that-can-speak-first-has-two-doors.md) |
| `ring`, `dispatch` | `RingDispatch` over an SPSC ring of `AtomicU8`, safe Rust, no dependency; `InlineDispatch` | D4, [ADR-0007](decisions/ADR-0007-spsc-ring-without-unsafe.md), [ADR-0011](decisions/ADR-0011-a-full-ring-disconnects.md) |

## 4. The decisions that shape it

### D1 — The session layer is a pure state machine with no I/O

The session owns the seven administrative message types (`0 1 2 3 4 5 A`) and hands
everything else to the application. It has no socket, no clock and no allocation. Time arrives
as `Tick`. Errors are fieldless enums: no `String`, no `format!`, nothing that allocates on an
error path.

```rust
pub trait Role: sealed::Sealed { const SPEAKS_FIRST: bool; }
pub struct Acceptor;   // SPEAKS_FIRST = false
pub struct Initiator;  // SPEAKS_FIRST = true

pub trait Application {
    fn on_message(&mut self, msg: &[u8], seq: u32, stamp: &[u8], out: &mut [u8])
        -> Option<Range<usize>>;
}
```

**Why it is the highest-leverage decision in the design:** the 59 QuickFIX acceptance
definitions become unit tests. No listening socket, no timing window, no flake, and they run
in milliseconds. A session layer entangled with I/O can only be tested through a socket, and
socket tests are the ones that get muted. It also makes the engine replaceable without
touching protocol correctness.

**As built.** The four inputs are four methods, `connect`, `disconnect`, `tick` and
`received`, each taking an `emit` closure the caller supplies and each answering
`Link::{Up, Dropped}`. One input may call `emit` up to five times (`[measured]` two corpus
files need five). `Role` is a sealed trait with two marker types, so the branch resolves at
compile time. The `Application` is given the two things it does not own, the outbound
sequence number and the clock, writes its reply into a buffer the session lends it, and
returns the range it used; `None` spends no sequence number.

**One machine, both roles.** The acceptor waits for a Logon and answers; the initiator sends
one and waits. Sequence handling, resend, heartbeat, test request and logout are the same
protocol read from the other end; [ADR-0004](decisions/ADR-0004-bidirectional-engine.md)
found that 51 of the 59 definitions mirror unchanged. The two roles are asymmetric in one
place the corpus could not show: `[measured 2026-09-02]` the inbound-Logon handler answered
with a Logon for both roles, which for an initiator starts a second handshake on a session
that already has one. It was found on the first run of `scripts/interop.sh`, by `libquickfix`
dropping the connection without a word, and the reply is now behind `!R::SPEAKS_FIRST`
([reference/a-role-can-be-wrong-in-a-direction-no-gate-runs.md](reference/a-role-can-be-wrong-in-a-direction-no-gate-runs.md)).

**An initiator needs one thing a pure machine cannot give it: intent.** `[measured
2026-08-30]` 46 of the 50 mirrorable definitions require this end to send a message nothing
on the wire asks for and no clock produces, 42 of them a Logout. So the layer has six
functions that take an operator's intent:

```rust
send_heartbeat(emit)                  // 35=0, carrying no 112=
send_test_request(id, emit)           // 35=1, the caller's 112=
send_resend_request(from, to, emit)   // 35=2, the caller's 7= and 16=
send_sequence_reset(n, emit)          // 35=4 with 123=N, and become n
begin_logout(text, emit)              // 35=5, then wait for theirs
send_application(msg, journal, emit)  // anything the session does not own
```

None of them takes whole message bytes, and that boundary is a decision
([ADR-0042](decisions/ADR-0042-a-second-implementation-is-the-only-independent-opinion.md)).
A caller supplies the fields it owns; the session builds the message from its own `Template`
and keeps `8`, `9`, `34`, `49`, `52`, `56` and `10`. `benches/alloc.rs` reads `ordered 0`.

**A connection and a session are different things.**
[ADR-0010](decisions/ADR-0010-a-reconnect-is-not-a-restart.md): FIX 4.4 numbers a
**session**, not a connection, so a session that outlives its process must keep counting.
`Session::resume(cfg, next_out, next_in)` builds one that already carries numbers and
`connect` leaves them alone; a session from `Session::new` has persisted nothing and resets
on every connection. The corpus keeps its meaning by construction: the runner builds a session
per scenario with `new`, so all seven reconnects in the corpus expect `34=1` and the score is
59 / 59 unchanged. `[measured 2026-08-31]` forcing `connect` to never reset drops it to 56 /
59, which proves the corpus exercises that branch. Recovering the numbers is the engine's job;
`Session::next_out()` and `next_in()` exist so it can persist them.

**Both counts survive, and the inbound one is written after delivery.**
[ADR-0017](decisions/ADR-0017-the-inbound-count-is-persisted-after-delivery.md): the journal
carries `mark_in(seq)` and `highest_in()`, so one file holds both directions. The session
calls `mark_in` at the end of `received_with`, after judging and after draining held
messages. **The ordering is the decision.** Writing the mark *before* delivery would mean an
ill-timed crash loses the message: this end has counted it and will never ask for a resend,
while the counterparty believes it arrived. Writing it *after* means the message is delivered
twice, and the second copy carries `43=Y`. FIX has a flag for the second failure and nothing
for the first. The cost: under `Durability::Fsync` the inbound path pays a sync per message,
and an application behind this engine must be idempotent per sequence number
([GUIDE.md §6a](GUIDE.md)).

**The journal is a trait the session is handed**, not an action it emits: a resend has to
*read*, and an action is one-way. `Session::received` supplies `NoJournal` so a pure protocol
machine is unchanged ([ADR-0008](decisions/ADR-0008-journal-is-a-trait.md)).

### D2 — The field index is separate from the message view

Measured, not assumed; full detail in
[ADR-0003](decisions/ADR-0003-message-representation.md) and
[reference/measured-costs.md](reference/measured-costs.md).

```rust
#[repr(C)]                       // 12 bytes, natural alignment 4. NOT align(16)
pub struct FieldEntry { tag: u32, offset: u32, length: u16, _pad: u16 }

pub struct FieldIndex<const N: usize> { count: u16, fields: [FieldEntry; N] }  // reusable, no lifetime
pub struct MessageView<'a, const N: usize> { buf: &'a [u8], idx: &'a FieldIndex<N> }

/// Incomplete is Ok, not Err: TCP delivers bytes, not messages.
pub enum Parsed { Complete { consumed: usize }, Incomplete }

pub fn parse_into<D: Dictionary, const N: usize>(
    buf: &[u8], idx: &mut FieldIndex<N>, v: Validation,
) -> Result<Parsed, ParseError>;
```

The caller owns one `FieldIndex` and reuses it for every message on that connection. The
parser never constructs or returns a large struct.

**`MessageView` is 24 bytes**: a fat `&[u8]` (16) plus a reference to the index (8).
`[measured]` verified with `rustc -O` on 2026-08-27. On x86-64 SysV and AArch64 a struct over
16 bytes is passed indirectly, so any hot-path function taking a `MessageView` by value
carries `#[inline]`, and `crates/codec/src/index.rs` carries
`const _: () = assert!(size_of::<MessageView<64>>() == 24);` so that growing it fails to
compile rather than silently costing a spill.

`N` is a const generic, so the caller chooses: `FieldIndex<64>` for order flow,
`FieldIndex<512>` for a market-data snapshot. Overflow is `ParseError::TooManyFields`, never
silent truncation.

**Repeating groups do not change the index.** The index stays flat: `parse_into` records tags
in wire order and knows nothing about groups. A group is resolved only when asked for, by
`MessageView::group(msg_type, counter)`, which walks the flat entries. Three consequences:

- **A message with no group pays nothing.** `[measured]` parse is unchanged at 77 ns; walking
  a group is a separate 29–145 ns depending on depth (`benches/groups.rs`).
- **Nothing is allocated.** `benches/alloc.rs` walks four nesting levels and reports 0.
- **The scan steps over nested regions.** A group ends at the first tag outside its member
  set, and a nested group's members are not members of the group around it. `[measured]` 235
  of the 731 group positions in FIX 4.4 contain a nested group.

`declared()` (what the counter says) and `counted()` (what is on the wire) are reported
separately and never reconciled by the codec. Whether a mismatch is a `Reject 373=16` is the
session layer's decision.

### D3 — Field ordering comes from generated tables, never from hand-written code

The QuickFIX acceptance comparator compares fields **positionally**: a correct FIX message
whose fields are in a different order fails
([reference/quickfix-acceptance-def-format.md](reference/quickfix-acceptance-def-format.md)).
So the serialiser emits in an order derived from the dictionary at build time, and ordering is
never a judgement made at a call site.

**Inside a repeating group the ascending-tag rule does not apply.** `MsgType` first, then
header tags ascending, then body tags ascending governs the message; a group entry is written
in the dictionary's **declaration** order, delimiter first (`269` before `270`, `279` before
`285`). `Template::encode_with::<D>` walks `D::group_order(msg_type, counter)` and never the
order the caller supplied. `[measured]` `crates/codec/tests/group_roundtrip.rs` hands every
entry over in reverse and round-trips 357 top-level positions byte for byte.

**A round trip against your own table proves stability, not correctness**, so the order is
checked against QuickFIX's generated C++: `[measured]` the delimiter agrees on 730 / 730 groups
and QuickFIX's order is an exact subsequence of this crate's on 730 / 730
(`crates/dict/tests/interop_quickfix_order.rs`). Swapping two adjacent members in every group
leaves the round trip green and turns that test red, which is why it exists.

**A DATA field is written immediately behind its length field, and the encoder writes that
length.** A DATA value may legally contain `0x01`, so a reader takes its length from the field
in front. A DATA field declared without its length field fails at `TemplateBuilder::build`
with `EncodeError::DataWithoutLength`; inside a group the same case fails in `encode_with`
before a byte is written; and the length is computed from the data, never taken from the
caller. `[measured 2026-08-30]` fifteen of FIX 4.4's sixteen DATA pairs have
`length == data − 1`, so ascending-tag order was right by accident; `Signature(89)` takes
`SignatureLength(93)` and was emitted before its length. Held by
`crates/codec/tests/data_encode.rs` and by `group_roundtrip.rs`, which writes 508 DATA members
with a separator inside every value.

### D4 — Dispatch is a trait; inline is the default, the ring buffer is the option

Taken from [Artio](https://github.com/artiofix/artio), which separates the engine (owns
connections and session lifecycle) from the library (runs business logic), **but not adopted
as the default**. Artio's split is justified largely by the JVM: process isolation contains GC
pauses. Rust has no GC, so what remains is a property some applications need and others pay
for ([ADR-0002](decisions/ADR-0002-engine-library-split.md)).

- **`InlineDispatch`**: the handler runs on the engine thread, directly after the session
  machine. Zero hand-off, zero copy, the borrowed `MessageView` handed straight through. The
  HFT-standard shape: `recv → parse → decide → encode → send` on one core.
- **`RingDispatch`**: bytes are copied into an SPSC ring and an application thread consumes
  them. Costs a hop (§6) and buys the one thing inline cannot: an application that blocks
  does not stall the session layer.

**As built.** `Dispatch` carries a `const OUT_OF_BAND: bool`, `false` for `InlineDispatch`, so
the engine's "collect what the other thread produced" block compiles away on the default
engine. A reply from the ring comes back through `Session::send_application`, so the sequence
number and SendingTime are the session's own; an application on another thread cannot get
either wrong because it is never told them. A reply is routed by a **connection id, never an
index**: the engine drops a dead connection with `swap_remove`, so an index is stale the
moment anything hangs up. `crates/engine/tests/dispatch.rs` asserts that, and asserts the
property that makes the trait worth having: **the same message produces the same bytes on the
wire under either dispatch.** The ring itself is `Box<[AtomicU8]>`, safe Rust, no dependency,
a byte-at-a-time copy whose price is published ([ADR-0007](decisions/ADR-0007-spsc-ring-without-unsafe.md)).

### D5 — Transport is a trait; TCP is the only implementation that ships by default

```rust
pub trait Transport { fn recv(&mut self, buf: &mut [u8]) -> io::Result<usize>; fn send(&mut self, buf: &[u8]) -> io::Result<usize>; }
```

Two rules, both learned from reading `matthart1983/nanofix`:

1. **A feature flag gates the module declaration itself**: `#[cfg(feature = "aeron")] mod aeron;`.
   In that project the flag exists in `Cargo.toml` while `src/lib.rs` declares the module
   unconditionally, so `cargo test --no-default-features` fails to link for everyone without
   Aeron installed.
2. **`build.rs` invokes no external toolchain unless that feature is on.** In that project it
   panics regardless, with the author's home directory as a fallback search path.

Together those make a crate unbuildable for anyone but its author. Avoiding it costs nothing.

### D6 — No `panic!`, `unwrap()` or `expect()` in any library crate

Enforced by a workspace clippy lint, not by discipline. The reference implementation carries
**276** of them; discipline alone demonstrably does not hold this line.

### D7 — Persistence is a policy, and it is off the hot path

QuickFIX's `FileStore` calls `Sync()` on every write across three files, which is the dominant
latency source in its default configuration. Here the policy is the user's:

| Policy | Type | Meaning |
|---|---|---|
| none | `fixbolt_session::journal::NoJournal` | nothing kept; resend is impossible. Tests, simulators |
| in memory | `engine::journal::MemJournal` | a ring that keeps but does not persist |
| `Async` | `engine::journal::FileJournal` | appended, flushed by a background thread. Survives a process crash |
| `Fsync` | `engine::journal::FileJournal` | synced before the message is acknowledged. Survives a power loss. Regulated deployments |

**The in-memory ring is the whole resend store, in every policy.** A `FileJournal` keeps the
ring too and answers `get` from it: reading a replay back off disk would be a blocking `read`
on the thread non-negotiable 4 protects. Anything older than the ring is gap-filled, which is
legal and invisible to the counterparty's engine
([ADR-0046](decisions/ADR-0046-the-ring-is-the-resend-store-and-a-replay-goes-in-batches.md)),
and since 2026-09-04 it is **counted and emitted**: `EventKind::ResendBeyondJournal { filled,
oldest }` in messages, and `JournalRefused { count }` for a reply longer than a slot.

- **`SLOTS` is 4096** (it was 8 until 2026-09-04: the smallest power of two above what the
  corpus asks for, and an acceptor that had sent a hundred ExecutionReports replayed eight of
  them). `N × (LEN + 8)` ≈ 2 MiB per session; `[measured 2026-09-04]` `tools/w2w` reads
  **+2 195 456 bytes** of resident set against `SLOTS = 8`.
- **`get` is one index and one comparison**, addressed by `seq % N`. The scan it replaced
  returned the *first* slot carrying a number, which after `Admin::SetNextOut` wound the count
  back was the stale copy: correctly numbered, correctly checksummed, wrong message.
- **A replay goes out in batches** of `Config::resend_batch` messages (default 8), continued
  from `Session::tick_with` and from each judged message. Before this the whole range went out
  in one call, and a resend larger than `TX` tripped D10 and ended the session as a *slow
  consumer*. The corpus cannot see it, because no definition asks for more than three
  messages; `crates/engine/tests/backpressure.rs` does.

**The file is appended, not memory-mapped**: `mmap` means a dependency or `unsafe`, and the
engine plan authorised neither ([ADR-0008](decisions/ADR-0008-journal-is-a-trait.md)). A
record carries its own length, `seq(4) || len(4) || bytes`, and from format version 1 a CRC32,
so a torn tail and a flipped byte both stop the read rather than being replayed to a
counterparty. **Three header shapes are not messages**: `len == 0` is ADR-0017's inbound mark;
`seq == 0 && len == 8` is ADR-0039's activity mark, eight little-endian milliseconds saying when
the session was last alive, written at logon and at an ordered shutdown, never per message; and
`seq == 0 && len == 4` is
[ADR-0053](decisions/ADR-0053-the-journal-answers-two-questions-and-the-second-is-a-number.md)'s
outbound mark, the highest `34=` this session has spent. `34=0` is not a sequence number FIX has,
so none of the three cost a format change — and **the third is the last that gets that escape**:
a fourth shape would be a format only its own history can read, so the next one lifts the
version to v2.

**A journal answers two questions, and only the first is about bytes.** *What can you
replay?* — `get`, `highest`, `oldest`. *How far have you counted?* — `highest_out` and
`highest_in`. `[measured 2026-09-05]` **treating the first as an answer to the second is a
defect that reaches the wire**: `highest()` is the highest message *held*, and a `Logon`, a
`Heartbeat` and a `Logout` each spend a `34=` no journal holds bytes for, so a restart deriving
`next_out` from `highest()` is short by every administrative message since the last application
one. A real `libquickfix` refused a resumed session over a difference of exactly one. So the
session tells the journal the count it has spent — `mark_out`, a high-water mark — and
`Resumed::from_journal` does the arithmetic once (ADR-0053). `FileJournal::open` reads the file
before appending, `Session::resume` and `Engine::add_resumed` carry the numbers into a running
engine, and `Recovery` is the seam the serving loop asks
([ADR-0034](decisions/ADR-0034-recovery-is-asked-once-the-counterparty-is-known.md),
[ADR-0039](decisions/ADR-0039-a-fresh-journal-is-the-deployments-to-build.md)). Held by
`crates/engine/tests/recovery.rs`, `tests/on_disk.rs` and
`crates/session/tests/numbering.rs`.

**The journal is not a message log.** It keeps outbound application **messages** for resend and,
of everything else, only **numbers** — one inbound, one outbound. No administrative traffic and
no refused frame is in it; that is D14.

### D8 — In `hft` the engine thread busy-polls; in `standard` it blocks

Mode-scoped, and `standard` is the default
([ADR-0013](decisions/ADR-0013-two-modes-standard-and-hft.md)):

| | `standard` (default) | `hft` (opt-in, Linux only) |
|---|---|---|
| Idle behaviour | blocks on readiness with a timeout, and gives the core back | spins on non-blocking sockets, never enters the kernel |
| Cost of a wakeup | `epoll`-class, 2–5 µs | `[measured 2026-08-31]` one turn at **449 ns per session** on a §9 core |
| Pinning | none | the polling thread is pinned to an isolated core |
| Runs on | any OS, any hardware, a container, a laptop | a machine that satisfies §9 |
| Rule 4 says | it **must** block | it must **not** sleep |

**Why `hft` spins.** An `epoll` wakeup costs 2–5 µs and brings scheduler jitter with it. On a
design whose entire user-space path is under 1 µs, a blocking wait is the single largest cost
the engine controls. `[measured 2026-09-02]` end to end the spin is worth **3 437 ns, 17.7%**,
at p50 against `standard` on the identical path (§8).

**Why `standard` exists and is the default.** An engine whose out-of-the-box configuration
pins a core at 100% looks broken to most people who try it. And the spin is not free: at
449 ns per session per turn, the trade wins at N = 1 and loses by N = 8, because 8 × 449 ns is
3.6 µs and clears the top of `epoll`'s range. `standard` is the honest default for everything
that is not one session on an isolated core.

**As built, the shared loop.** `Engine::turn` is one non-blocking pass over every connection:
flush what is queued, **tick the clock**, read once, cut whole messages out, judge them, flush
again. `Engine::run` is `loop { if !turn() { wait.idle() } }`. Reading *once* per turn rather
than until the socket is empty is deliberate: a counterparty that writes faster than this end
processes must not starve the other connections on the thread. **The tick comes before the
read** because the session judges SendingTime against the last tick it was given, and a
session that has never ticked holds zero; reading first would refuse the first message on
every connection for skew. Keeping the pass separate from the loop is what lets the 59
definitions run through a real socket with no background thread and no timing window:
`crates/engine/tests/wire.rs` drives `turn` by hand.

**As built, `standard`** ([ADR-0014](decisions/ADR-0014-standard-mode-blocks-on-poll.md)).
`Waiting::idle` is handed the source list and `Transport` names its own descriptor. The
mechanism is `poll(2)` through `libc` behind the default-on `standard` feature; `epoll` is
O(1) where this is O(N) and is a later ADR with numbers. `Block` blocks at a **100 ms**
timeout, which is a correctness parameter rather than a knob: in `standard` that timeout is
what delivers `Input::Tick` to a session with no clock. The source list is one interest per
connection, readable always and writable only while bytes are queued, rebuilt every turn
because a `Source` borrows a descriptor that may have been reissued. `serve` hands the
listener to the poller, so a connection is accepted on the connect rather than on the next
timeout. A self-pipe wakes the poller for a reply produced on the application's thread, and
the engine drains it after every wait, because an undrained pipe makes every subsequent `poll`
return instantly: a working engine, burning a core. Pairing a blocking strategy with a
transport that cannot name a source does not compile.

**`wait::Yield` is neither mode.** It is `std::thread::yield_now()`, which yields the scheduler
and does not block, so it burns its core without giving `hft` its tight poll. Its rustdoc says
it fails both gates, and §6's `standard` gate demonstrates it.

**Pinning is something the code does, not something this paragraph asserts.**
`affinity::pin_current_thread` pins the calling thread and confirms it with
`sched_getaffinity`; `tests/affinity.rs` watches the scheduler's own `processor` field while
the thread works, and with the pin removed the same thread was observed on cpu0, cpu4 and cpu5
in one run. `Topology` and `ShardPlan::validate()` refuse a bad plan before any thread exists,
and `shard::Shards` runs one pinned engine per core, routing each socket by the identity in
its Logon. **One entry point pins nothing: `serve_hft`.** It spawns no thread of its own, so
the thread that calls it is the caller's to pin ([GUIDE.md §9](GUIDE.md), STATUS item 21).

**Both halves of rule 4 are machine-checked.** `scripts/check-no-kernel-sleep.sh` traces
`tools/w2w` with `strace -f` on Linux, attributes syscalls to the engine thread by tid, and
runs the binary a second time in `standard` mode requiring that run to trip the check.
`scripts/check-standard-gives-the-core-back.sh` asserts four things at once, because CPU near
zero is passable by three different broken engines (§6). `[measured 2026-08-30]` the 59
definitions pass in `standard` too, with the engine blocking between steps.

### D9 — Outbound messages are templates: a pre-sorted parts list, patched, not built

An ExecutionReport from a given session has a fixed skeleton: BeginString, SenderCompID,
TargetCompID, MsgType, and the field order (D3). That skeleton is encoded **once per session
per message type** into a scratch buffer the template owns.

```rust
enum Part { Static(Range<u16>), Slot(u32) }        // ranges into the template's own scratch
pub struct Template<const P: usize, const S: usize> {
    scratch: [u8; S], parts: [Part; P], len: u8,
}
pub fn encode(&self, out: &mut [u8], slots: &[(u32, &[u8])]) -> Result<Range<usize>, EncodeError>;
```

Three properties the first sketch got wrong:

- **The parts are sorted at build time** (D3), so `encode` walks them in order and never makes
  an ordering judgement. A slot the caller does not supply is skipped, so one template serves
  messages that differ in their optional fields.
- **The body is written first; the prefix is then written right-aligned in front of it.**
  `BodyLength` is variable-width, so writing the prefix first would mean shifting the body once
  its width is known. That is why `encode` returns a `Range` and not a length.
- **SendingTime is the hidden cost.** Naive formatting is 50–100 ns, as much as a parse. The
  `YYYYMMDD-HH:MM` prefix is cached and re-derived once a minute; only `SS.sss` is formatted per
  message. `[measured 2026-08-31]` 4.9 ns from the cache.

This shape is how the fastest commercial engines are reported to reach tens of nanoseconds per
serialise. That figure was once §6's published target, 60 ns, and
[ADR-0016](decisions/ADR-0016-per-machine-baselines-replace-absolute-targets.md) withdrew it:
it described other people's software, no machine here came within 1.5× of it, and
`[measured 2026-08-31]` the floor of this `Part` shape is about 116 ns even with the slot scan
removed. The cached-timestamp design is still right; what was wrong was borrowing somebody
else's number to grade it against.

### D10 — TCP send backpressure has a stated policy

**Two ends can fall behind, and they are different questions.** D10 is about the counterparty
on the wire; D10b is about our own application behind the ring. On the wire, a counterparty
that cannot keep up is broken; behind the ring, the counterparty is faultless and we are the
ones who stopped reading.

A slow counterparty fills the socket send buffer and `send` returns `EAGAIN`. At 50 000
ExecutionReports per second against a QA application this **will** happen. The engine must
not block the session machine and must not drop protocol messages silently.

| Policy | Behaviour |
|---|---|
| `Queue { max_bytes }` | buffer in the connection's `TX` up to a bound, then… |
| `Disconnect` | …drop the session with `Logout 58=slow consumer`. **The default**: a FIX counterparty that cannot keep up is a broken counterparty |
| `Block` | …spin until the socket drains. For tests; never the default, because one slow counterparty then stops every other session on the thread |

**As built.** `Backpressure` on `Engine` or on a single `Connection`; the queue is the
connection's `TX` buffer and `Queue { max_bytes }` only tightens the bound. Three rules
`crates/engine/tests/backpressure.rs` holds: a message goes in whole or not at all (a partial
frame is unrecoverable at the other end); the Logout is not subject to `max_bytes`, because
the queue is discarded first so the one message that matters has room; and a socket that has
died ends the connection even with bytes queued (`[measured 2026-08-30]` before this, killing
the socket mid-write left the connection `Up` for as long as it was turned).

### D10b — A full ring to the application ends the connection

[ADR-0011](decisions/ADR-0011-a-full-ring-disconnects.md). Under `RingDispatch` the
application is on another thread. If it stops draining, the ring fills, and until 2026-08-31
the answer was a counter nobody read. A message counted there is one the session accepted,
numbered, journalled and acknowledged by sequence number, that the application never saw:
for order flow that is silent loss, not backpressure.

| | D10, the wire | D10b, the ring |
|---|---|---|
| Whose fault | the counterparty's | ours |
| `58=` text | `slow consumer` | `slow application`, a different constant so neither side is told the wrong one is at fault |
| The queue | discarded first, so the Logout has room | kept, because the socket is draining perfectly |
| `Block` offered | yes, for tests | no: spinning until an application thread drains makes the engine's progress depend on code it does not control, and the rule-4 gate cannot tell a spin that finishes from one that does not |
| Default capacity | `TX`, the caller's | `ring::DEFAULT_CAPACITY`, 4 MiB |

The signal is a defaulted method on `Dispatch`, `fn take_refusal(&mut self) -> bool { false }`,
because `deliver` is reached through the pure session layer's `Application::on_message`,
which cannot carry it. The engine asks immediately after one connection's turn, so a `true`
belongs to that connection. `InlineDispatch` takes the default and the branch folds away.

**Two costs, stated.** 4 MiB resident per ring; and an application that pauses longer than the
ring holds now drops the session. `[measured 2026-08-31]` 4 MiB gives **5.05–5.36 ms** of slack
over four runs on the §9 desktop, 22 550 messages, against 47.7 µs at the old 64 KiB. That is
above the 1.6–3.6 ms ADR-0011 derived by scaling, because the per-message cost goes 135 →
~230 ns once the buffer stops fitting in cache: a ring that fills more slowly gives the
application more time. Still true: no real application has ever stalled against this ring, so
the policy and the capacity come from one synthetic saturation run plus reasoning.

### D11 — TLS is a transport implementation, and the guarantee is stated per mode

Decided in [ADR-0005](decisions/ADR-0005-tls.md). It needs a decision because of one
collision: the codec parses in place at the I/O buffer, and encrypted bytes cannot be parsed
in place. Userspace TLS reintroduces exactly the copy ADR-0003 spent its length removing.

| Mode | When | Hot-path guarantee |
|---|---|---|
| Handshake, `rustls`, userspace | once per session, before any message flows | **allocation permitted**: a named, bounded carve-out from non-negotiable 1 |
| Steady state, **kTLS** | Linux, and a cipher suite the kernel carries | **met.** The kernel delivers plaintext into the read buffer; the D8 loop and parse-in-place survive unchanged |
| Steady state, userspace `rustls` | macOS, older kernels, unsupported suites | **not met, and the documentation says so.** One copy each way, and it allocates. A number measured in this mode is never quoted as the engine's |

`cargo build --no-default-features` produces a binary with no TLS code and no crypto
dependency (D5).

**Verified 2026-08-31, and it was load-bearing.** `ktls-core` *can* be driven from a plain
non-blocking socket with no async runtime: `strace -f` over 1000 round trips shows `recvfrom`
and `sendto` and nothing else. It costs four conditions: every read error goes to
`ktls_core::Context::handle_io_error`, the transport never reads the socket outside the
offload, the handshake hands over with an empty buffer, and `setup_ulp` needs an
`ESTABLISHED` socket ([reference/ktls-on-a-plain-socket.md](reference/ktls-on-a-plain-socket.md),
[ADR-0018](decisions/ADR-0018-ktls-on-a-plain-socket-answers-adr-0005.md),
`scripts/check-ktls-on-a-plain-socket.sh`).

**Still not built.** No TLS code is merged; a plan is drafted
([plans/2026-09-04-tls.md](plans/2026-09-04-tls.md)). Still unverified: which kernel version
and cipher suites are the floor (ADR-0005 open question 2), whether a session survives a
TLS 1.3 key update under kTLS (question 6), and what asserts which of the three modes is live
(question 3). The §8 TLS row stays empty.

### D13 — `Tick` counts milliseconds from year zero, not from the Unix epoch

SendingTime is `YYYYMMDD-HH:MM:SS[.sss]`, four year digits, so the wire can name any instant
from 0000 to 9999. Counted from 1970 in a `u64`, more than a fifth of that range does not
exist: a counterparty sending `52=19600101-00:00:00` would wrap the skew subtraction into a
difference of half a billion years, failing no check and crossing one, silently.

So `Tick` and every parsed SendingTime are **milliseconds since 0000-01-01T00:00:00Z**,
proleptic Gregorian. Every timestamp FIX can express is a non-negative `u64`, the skew is a
plain `abs_diff` that cannot wrap, and the session needs no signed arithmetic. The engine
converts once at the edge: `tick = unix_millis + clock::MILLIS_YEAR_ZERO_TO_EPOCH`.
`codec::TimestampCache` still takes Unix milliseconds because it is `no_std` and shared with
callers that have no session; bridging the two is the session's job.

### D14 — The message log is a second file, written by the journal's pattern, and it records refusals

The journal answers *"what did we send, numbered `seq`"*, which is what a ResendRequest needs.
It cannot answer the first question a desk asks in a dispute: *"at 10:32:07, what did we
receive, and what did we turn away?"* Inbound frames are kept as a number and no bytes
(ADR-0017), and a frame refused before the session saw it (a wrong `56=`, a duplicate identity,
garbage) disappears the moment the connection ends.

**It is a second file, not an extension of the journal, because the journal's key is `seq`.**
The three things this file exists for have no sequence number. `Journal` is also a `session`
trait, and a pre-session refusal is bytes the session never sees, so merging would put in
`session` something D1 forbids it to know. And `Durability::Fsync` blocks the engine thread
deliberately while a diagnostic must never, and one file cannot serve two durability policies
without branching on record kind inside the loop that must not branch. The full argument is
[reference/why-the-message-log-is-not-the-journal.md](reference/why-the-message-log-is-not-the-journal.md).

**The mechanism is ADR-0007's, unchanged**: one `Producer::push` per message per direction
into a ring, a writer thread that formats and appends, and losses dropped and counted rather
than waited for. The writer is allowed to allocate, for the reason `journal::Reader` is.

**`NoLog` is the default and it compiles away.** `MessageLog::LOGS` is an associated
constant, so an engine never given a log carries no branch, no field and no cost.

### D15 — An application can speak first, through two doors, and neither is told a sequence number

`[added 2026-09-05]` Until this decision **every application message this engine could send was
a reply.** `Handler::on_message` answered one inbound message; `Admin::Command` moved sequence
numbers. So there was no `ExecutionReport` for a fill that lands a second after the order, no
quote stream, no out-of-band `35=j`, and nothing to say to a counterparty that is connected and
quiet ([ADR-0048](decisions/ADR-0048-an-engine-that-can-speak-first-has-two-doors.md),
`STATUS.md` item 46).

**The primitive already existed.** `Session::send_application` takes a whole message, rewrites
`8=`, `9=`, `34=`, `52=` and `10=`, orders the rest from the generated tables, journals it and
spends the number. Its only caller was D4's `OUT_OF_BAND` block, which is `false` for
`InlineDispatch` — so it was reachable only by an application that had already moved to another
thread. What was missing was not a mechanism but a door.

| Door | Where the application is | For |
|---|---|---|
| `Handler::on_logon` | the engine thread, once per session | anything that must be said *as the session opens* — a subscription, a state dump, the two `35=B` the interop gate wants |
| `Sender` | any thread, any time | a fill that lands later, a quote stream, an out-of-band `35=j` |

**Neither door is told a sequence number or a clock**, so an application cannot get either
wrong: the session writes both on the way out and ignores whatever was there. That is `Reply`'s
existing rule extended to the message nobody asked for.

**`on_logon` is asked repeatedly and the engine owns the loop**: `nth = 0, 1, 2, …` until the
application answers `None`, each message sent as it comes, bounded by `MAX_ON_LOGON` (16) so a
handler that never stops cannot hold the engine thread. Reaching the bound emits
`EventKind::SpokeFirstToTheBound` rather than passing in silence.

**`[amended 2026-09-05, ADR-0054]` and all three are reachable from the front door.** Until
item 47 they were not: every one came off an `Engine`, and `serve` builds its engine inside
itself and returns only a `Shutdown`. So door 2 existed and a `serve` deployment could not open
it. `observe::Handles` is the same cell made **before** the engine, which then adopts it; the ten
`serve*`/`connect_and_serve*` entry points take one as their last argument. See D15's own row in
§3 and [ADR-0054](decisions/ADR-0054-the-handles-are-made-before-the-engine-and-the-engine-adopts-them.md).

**`Sender` rides the `Arc` that `Observer` and `Admin` already ride**, with a third capability
and its own fixed queue — `ORIGIN_CAPACITY` (64) slots of `ORIGIN_LEN` (512) bytes, filled once.
It copies `Commands` where `Commands` was right: `send` answers `false` at the call when the
queue is full or the message is too long, so a loss is never silent; the engine drains with
`try_lock` and never `lock`; and a relaxed load comes before the lock is attempted, so an engine
nobody sends through pays one load per turn. `Sender::drains()` is what keeps that falsifiable —
`[measured 2026-09-05]` removing the load makes it read 20 over 20 turns instead of 0.

**The gate is deliberately not one of this repository's own runners.** `scripts/interop.sh`'s
acceptor role has two steps, `news` and `resend`, that were red **by design** because the
acceptor could not originate; the tool said so in a comment. Both are green now, and the
exemption is gone from the code, so a red there is a red. `[measured 2026-09-05]` the two roles
pointed at each other read `PASS 7/7`, up from 5/7 before this work and 6/7 with the door open
but the message malformed — see
[reference/a-message-on-the-wire-is-not-a-message-delivered.md](reference/a-message-on-the-wire-is-not-a-message-delivered.md).

**What it costs**, and the ADR prices all of it: `crates/session` gains an
`Application::on_logon` with a default body that nothing in that crate calls, plus three `Config`
getters; there is a second bounded queue and a fifth buffer ceiling; and `MAX_ON_LOGON` is a
number with no measurement behind it, labelled as a guard rather than a knob.

**What it costs the engine thread is not measured yet**: one ring copy per message per
direction, about 1.7 ns/byte, so about 340 ns for a 200-byte message and 680 ns for a
request/reply pair, `[unproven]`, arithmetic from §6's dispatch row. What is measured is that
it allocates nothing on that thread: `benches/alloc.rs` cases `log-record`, `log-idle` and
`log-busy`.

**`OUT` means queued, not sent, and the gap is counted.** The line is written when a message
reaches the outbound buffer, and a dying socket discards that buffer.
`EventKind::MessageLogUnsent { bytes }` says how much of that connection's tail the file is
wrong about.

## 5. Non-goals for v1

The full list is [PRD.md §5](PRD.md); this is the subset that shapes the architecture.

- **FIX 5.0 / FIXT 1.1**: phase 2, together with SBE, because SBE messages are versioned by
  `ApplVerID`.
- **SBE, FAST, FIXML**: phase 2, and an encoding ADR comes first, because `MessageView`
  presupposes tags on the wire and SBE has none.
- **Kernel bypass**: not before an ordinary TCP path has been measured and found to be the
  limit, which §8 puts at 10–20 µs. If it ever happens: Onload first (the D8 loop and the
  socket API survive unchanged), `ef_vi` second as an `impl Transport` behind a D5-style flag,
  DPDK never (no TCP stack). Plaintext only, so it and D11 exclude each other. STATUS item 14.
- **SIMD delimiter scan and checksum**: declined by
  [ADR-0045](decisions/ADR-0045-parse-is-under-one-percent-of-the-wire-and-simd-is-declined.md),
  because parse is 0.62% of a round trip. `matthart1983/nanofix` has SIMD and parses 4–6×
  slower, because layout beat it.
- **Clustering, HA, replication. Metrics dashboards and web UIs. Matching engine, order book,
  risk.** This is a protocol engine.

**No longer a non-goal:** the initiator. [ADR-0004](decisions/ADR-0004-bidirectional-engine.md)
moved it into phase 1 on the finding that the two roles differ by about one enum's worth of
behaviour, and that a session core which cannot invert is a rewrite later.

## 6. Gates

Each is a committed benchmark or test, named. **A target without a runnable gate is a wish.**
Timing gates are judged against this machine's own recorded baseline, never an absolute
number ([ADR-0016](decisions/ADR-0016-per-machine-baselines-replace-absolute-targets.md), see
below).

### Correctness

| Gate | Target | Proven by |
|---|---|---|
| Session conformance, acceptor, in process | **59 / 59** | `cargo test -p fixbolt-session --test score` `[measured 2026-08-29]` |
| Session conformance, acceptor, through a real socket | **59 / 59 on every machine** | `cargo test -p fixbolt-engine --test wire`: kernel sockets, the real framer, the real session, the real application; only the clock is injected, because every `I` line in the corpus carries a fixed instant. `[measured 2026-08-30]` 59 / 59 on the M5 and on Linux. It read 39 / 59 on Linux until the harness's client socket was given `TCP_NODELAY`; the gate is now flat across a 20× span of its timing bounds |
| Session conformance, acceptor, in `standard` mode | **59 / 59** with the engine blocking between steps | the same wire test, second case. The only place the corpus meets `standard`. It proves the protocol, not the wiring: `[measured 2026-08-30]` with `Block` made to ignore readiness the run took 3.30 s against a 3.28 s baseline, because one block satisfies the settle criterion either way |
| Session conformance, acceptor, through the shard runtime | **59 / 59 through one shard and through two** | `cargo test -p fixbolt-engine --features affinity --test shard_wire`. `[measured 2026-08-31]` it read 57 through two; `[measured 2026-09-01]` 59 ([ADR-0020](decisions/ADR-0020-a-pre-session-stage-owns-the-socket-until-logon.md)). The test also counts how the pre-session stage disposed of every socket, because a dropped connection is indistinguishable from a refused duplicate ([ADR-0022](decisions/ADR-0022-the-pre-session-stage-enforces-two-definitions.md)) |
| Session conformance, initiator, mirrored corpus | **10 / 50** `[measured 2026-09-02]`, ceiling of 45 in doubt, and the harness's own drive count asserted beside it | `cargo test -p fixbolt-session --test mirror`. The secondary gate. It asserted `passed == 0` for three days, and a gate pinned at a constant reports nothing about the code under it. The jump to 10 was two real defects: a session that said goodbye first answered the acknowledgement with a third Logout, and `begin_logout(b"")` wrote an empty `58=`. `Report::driven` counts every time the harness played the operator, by MsgType, because a score a harness can raise by driving harder is not a score. Reversals: `make_receivable` neutered takes it to 0 / 50; letting the acceptor corpus be driven makes `tests/score.rs` panic |
| Every `373` code the corpus asks for is produced | **12 / 12**, read from the corpus's `E` lines | `crates/session/tests/score.rs`. The file count cannot say this: `14a_BadField.def` holds four cases, and a session answering all four with one code still passes the file |
| The session rules the corpus cannot tell apart | each has its own test | `crates/session/tests/logon.rs`, `reject.rs`, `heartbeat.rs`, `resend.rs`. Examples: deleting the "first message must be a Logon" check leaves the score unchanged, because `1e_NotLogonMessage.def` also carries a wrong `56=`; stamping `52=` from a constant leaves it unchanged, because `52` is matched by shape; all three heartbeat thresholds are invisible to whole-interval ticks; every file that opens a gap ends before opening a second one |
| The conformance runner can tell right from wrong | a fake that replays each file's own expected output scores **59 / 59** | `crates/conformance/tests/fix44.rs`. Without it, 0 / 59 is also what a broken runner reports |
| The journal keeps what a resend needs, under each policy | no journal fills over everything; `MemJournal` and `FileJournal` replay; a message longer than a slot is refused, not truncated | `crates/engine/tests/journal.rs`. Reversal: making `put` keep nothing turns four tests red **and drops the acceptance score**, which proves the score depends on the journal |
| A resend larger than the transmit buffer does not end the session | 100 messages of ~200 bytes, `TX` 8 KiB, `Backpressure::Disconnect`: all 100 come back with `43=Y`, in order, no `58=slow consumer` | `crates/engine/tests/backpressure.rs::a_resend_larger_than_tx_does_not_end_the_session` (ADR-0046). Reversal: `with_resend_batch(10_000)` ends the session on turn 1, **and `--test score` stays 59 / 59**, which says the corpus is blind to this |
| A resend past the ring is counted and reaches an operator | 20 orders through an 8-slot ring, `7=2 16=0`: one `ResendBeyondJournal { filled: 12, oldest: Some(14) }`, `events_lost() == 0` | `crates/engine/tests/events.rs::a_resend_past_the_ring_is_an_event_with_the_numbers`. One event per turn that changed, not one per message |
| A journal on disk says when its session was last alive | the instant survives the process; the latest mark answers; a file with no mark reads as before | `crates/engine/tests/on_disk.rs`, six tests, one through a real socket and the serving loop. Reversal: `mark_active` writing nothing turns three red ([a-reversal-that-must-not-compile](reference/a-reversal-that-must-not-compile.md)) |
| The serving loop does not require the journal to have a `Default` | putting the bound back **must not compile** | `crates/engine/tests/on_disk.rs::serving`, which uses a `FileJournal`. `[measured 2026-09-02]` restoring `J::default()` in `pump` gives `error[E0599]`. No runnable test can hold this claim |
| Counterparties come out of a configuration file | two named only in a file both log on through a real socket; one the file does not name gets nothing; one whose window closed is refused while one open now is served | `crates/engine/tests/settings.rs` (30 tests) and `tests/settings_wire.rs` (4, one a `#[should_panic]` control proving the harness can tell a closed socket from a hung one). Reversals: ignoring an unknown key turns 1 red, accepting a file with no `[SESSION]` turns 2, letting `[DEFAULT]` win over `[SESSION]` turns 1, dropping the schedule turns 5. One reversal not in the plan, keeping only the first counterparty, left all three wire tests green, because an unserved identity and a closed window are the same silence; the fix asserts the registry's length ([two-time-rules-share-one-observable](reference/two-time-rules-share-one-observable.md)) |
| An initiator comes back after its counterparty hangs up | the loop dials again; a policy that says stop opens no socket | `crates/engine/tests/reconnect_wire.rs`, over a real listener that answers one Logon and closes. Two orthogonal reversals. `[measured 2026-09-02]` the first originally made the suite hang, so the control now runs on a thread with a deadline ([a-reversal-can-fail-by-hanging](reference/a-reversal-can-fail-by-hanging.md)) |
| The reconnect loop, against a real `libquickfix` that dies and comes back | **6 / 6 × 3 scenarios.** `SIGKILL`: nobody said goodbye · it came back unprompted · **at one past the number it last sent** · the venue's next messages were *delivered to the application*, not gap-filled · no `35=2`, `141=Y` or `MsgSeqNum too low` anywhere · and the journal is never behind what an `Observer` saw spent. `SIGTERM`: the same five, with *the goodbye was answered* in place of the first. `HeartBtInt=1` with a pause before the kill: the `SIGKILL` five again, with an administrative message guaranteed after the last application one | `scripts/interop.sh` §4d/4e/4e-bis and the blocking `interop` CI job. `STATUS.md` items 38, 48, 47 — until 2026-09-05 every test of `connect_and_serve` was this repository's own reading, which ADR-0043's own *Consequences* said out loud. The counterparty's `FileStore` is what remembers the numbering, and its three `ResetOn*` are `N`: under the `Y` the other two directions use, both ends restart at 1 and a broken engine passes. `[measured 2026-09-05]` the first run refused the resumed session and named the cause in English ([a-journal-holds-messages-not-numbering](reference/a-journal-holds-messages-not-numbering.md)); the `SIGTERM` scenario asserted **3 / 3** with a pinned `known_gap` until ADR-0053 closed it, and the sixth assertion needs ADR-0054's handles to exist at all |
| The reconnect ladder, no I/O, no clock | doubling to a ceiling that holds; `logged_on` resets it; a shut venue outranks it | `crates/engine/tests/reconnect.rs`, 8 cases ([ADR-0043](decisions/ADR-0043-backoff-without-jitter-and-a-reconnect-asks-recovery-every-time.md)). **Every case is invented**: no corpus here covers reconnect. The ordering reversal was a no-op until the assertion moved to an instant where the two orderings disagree ([a-reversal-needs-an-input-where-the-answers-differ](reference/a-reversal-needs-an-input-where-the-answers-differ.md)) |
| The initiator, against a real `libquickfix` | **7 / 7**: logon · application messages in · an unprompted heartbeat · a TestRequest with this end's own `112=` · a ResendRequest answered by replay at the numbers asked for · a gap this end opens and gap-fills · logout | `scripts/interop.sh` and the blocking `interop` CI job. Phase 1 exit criterion 4 ([ADR-0042](decisions/ADR-0042-a-second-implementation-is-the-only-independent-opinion.md)). Builds QuickFIX at the same commit `fetch-quickfix-assets.sh` pins and refuses to run if the pins drift. Reads the transcript, not the exit code. `[measured 2026-09-02]` its first run found the initiator answering a Logon with a Logon. Reversal 2 was a no-op until the resend step named the sequence numbers it wanted ([a-resend-answer-has-two-legal-shapes](reference/a-resend-answer-has-two-legal-shapes.md)) |
| The acceptor can originate, and a second implementation says so | `news` and `resend` in the acceptor role of `scripts/interop.sh`, **red by design until 2026-09-05** and green now, with the exemption removed from `tools/interop` so a red is a red | D15, [ADR-0048](decisions/ADR-0048-an-engine-that-can-speak-first-has-two-doors.md). `[measured 2026-09-05]` this repository's two roles pointed at each other read **PASS 7/7**, from 5/7. The step in between, 6/7, is its own lesson: the door was open and the message was still refused, for a required group nobody had read off the XML ([a-message-on-the-wire-is-not-a-message-delivered](reference/a-message-on-the-wire-is-not-a-message-delivered.md)) |
| The acceptor, against a real `libquickfix` | **7 / 7**: logon with `141=Y` echoed · two `35=D` answered by two `35=8` paired on `11=` · an unprompted heartbeat with no `112=` · a TestRequest · a ResendRequest answered by replay at the two numbers asked for · a gap the counterparty opens · logout | the same script and CI job. **The differentiator's first independent opinion**: until 2026-09-04 the acceptor's whole evidence was 59 `.def` files read by this repository's own runner. Under test is the whole stack: `fixbolt::serve`, the poller, the pre-session table, the settings file, the library `Handler`. `[measured 2026-09-04]` its first run was red on `gapfill`, and the red was the test's ([a-gap-fill-can-swallow-the-question](reference/a-gap-fill-can-swallow-the-question.md)) |
| Repeating groups, read | every group found, to depth 4, at all **731** positions | `crates/codec/tests/groups.rs` |
| Repeating groups, written | parse → encode byte-identical at all **357** top-level positions, all 59 counters, depth 4 | `crates/codec/tests/group_roundtrip.rs` |
| Every tag number matches another implementation | **912 / 912** against QuickFIX's `FixFieldNumbers.h`, and 5 168 names FIX 4.4 does not define are refused | `crates/dict/tests/interop_quickfix_fields.rs`. The negative half stops `is_defined_tag` being `true` for everything |
| Every field type matches another implementation | **898 / 912**, 14 differences each named by tag | same test. QuickFIX's `FixFields.h` is shared across versions; the XML is the source of truth |
| Every (message, tag) pair matches another implementation | **12 524 / 12 524**, checked as 84 816 answers | `crates/dict/tests/interop_quickfix_messages.rs` |
| Every enum value is one QuickFIX also knows | **245 / 245** fields, **1 708 / 1 708** values | `crates/dict/tests/enums.rs`. One-directional: QuickFIX lists every version's values, so it can confirm but never forbid |
| What each of the 23 field types accepts | at least one accepted and one refused value per type | `crates/dict/tests/field_types.rs`. **Invented cases**: the corpus supplies two |
| In-group field order matches another implementation | delimiter exact on all **730** groups; QuickFIX's `message_order` an exact subsequence of this crate's on all 730 | `crates/dict/tests/interop_quickfix_order.rs`. Exists because the round-trip test reads the same table the encoder does |
| `parse_into` never panics on hostile input | `[measured 2026-08-28]` 304 230 294 executions, 0 crashes | `fuzz/fuzz_targets/parse.rs`, `cargo +nightly fuzz run parse` |

### Allocation

| Gate | Target | Proven by |
|---|---|---|
| Allocations on the hot path, codec | **0** | `crates/codec/benches/alloc.rs`, counting allocator |
| Allocations on the hot path, session | **0** on sixteen paths: accept, refuse, tick, beat, answer, gap, fill, deliver, resend, logon_out, originate, ordered, clock, text, schedule-open, schedule-shut | `crates/session/benches/alloc.rs`. The refusal path is counted apart because a hostile counterparty controls it and a `format!` is easiest to reach for there. `[measured 2026-09-02]` injecting one into `ordered` reads 10 000 |
| Allocations on the hot path, engine | **0** on twenty-seven paths: idle, send, recv, frame, turn, shard-turn, busy, ring, interests, pending-idle, pending-busy, pending-cycle, registry-lookup, observe-idle, observe-asked, events-idle, events-busy, admin-idle, admin-busy, shutdown, reconnect, log-record, log-idle, log-busy, **origin-idle, origin-busy, logon-first** | `crates/engine/benches/alloc.rs`. `busy` asserts the session is still logged on at the end of the count, because an earlier version measured a connection dropped at message two. `log-record` calls `MessageLog::record` a thousand times with no engine in the window; `[measured 2026-09-04]` making it allocate once reads 1000. `[measured 2026-09-05]` the two ADR-0048 cases read **2000** and **16** under an injected `format!`; `logon-first` is sixteen exact calls rather than thousands because `speak_first` runs once per session and the fixture cannot cycle sessions — one `Config` means a second concurrent session is refused as a duplicate, and a dropped `Loopback` peer signals no EOF, so an early version of that case read `1 sends over 500 sessions`. What no bench here proves is that the writer thread allocates nothing while the engine runs; `tools/w2w` is where a both-threads number belongs |

### Mode and machine

| Gate | Target | Proven by |
|---|---|---|
| The engine thread never sleeps in the kernel (`hft`) | no blocking syscall on that thread | `scripts/check-no-kernel-sleep.sh`: traces `tools/w2w` with `strace -f` and attributes calls to the engine thread by tid. `[measured 2026-08-30]` Linux 6.18: `accept4`, `recvfrom`, `sendto` and zero of `epoll_wait` / `poll` / `select` / `futex` / `nanosleep` / `sched_yield`. Runs the binary again in `standard` mode and fails if that run does not trip it: rule 4 had two machine checks before this one and both were green with a sleep present |
| A `standard` engine gives the core back | engine-thread CPU under 5% over a wall-clock window, found sleeping rather than running, **and** a round-trip p50 far below the poll timeout | `scripts/check-standard-gives-the-core-back.sh`. Four assertions, because CPU near zero is also what a dead thread, a run that never reached the mode, and an engine woken by its own timeout report. `[measured 2026-08-30]` a `Block` made to ignore readiness reads 0% CPU, sleeping 20 / 20, p50 99 046 599 ns; only the p50 catches it. Requires `hft` and `yield` to trip it |
| kTLS can be driven from a plain non-blocking socket | 15 assertions green, and the offloaded data path makes no blocking syscall | `scripts/check-ktls-on-a-plain-socket.sh` (D11, [ADR-0018](decisions/ADR-0018-ktls-on-a-plain-socket-answers-adr-0005.md)). `[measured 2026-08-31]` `recvfrom` 3033 + `sendto` 1000 over 1000 round trips and nothing else. Runs a second time with `poll(2)` in the loop and fails if that does not trip it. Skips with exit 2, not a pass, on a kernel that cannot offload |
| Which TLS mode is actually in force | a session that fell back to the userspace path is detected, not assumed | **no gate exists yet** (ADR-0005 open question 3) |
| The lint config denies `unwrap` / `expect` / `panic` | red on a crate carrying all three, green once they are gone | `scripts/check-lint-config.sh`, in CI on every push |
| Builds with nothing optional installed | `--no-default-features` on a clean runner | `.github/workflows/ci.yml`, its own job. `[measured 2026-08-30]` the workspace-wide command alone is not enough: cargo unifies features across one invocation, so a sibling crate switched the flag back on ([feature-flags-unify-across-a-workspace](reference/feature-flags-unify-across-a-workspace.md)) |
| An optional dependency is really optional | absent from the crate's graph with no features on, and the crate still builds and tests that way | `scripts/check-no-optional-deps.sh`, per crate. Reversal: removing `optional = true` from `libc` turns it red with the graph printed |
| No documentation link points at a missing file | every internal link resolves | `scripts/check-links.py`, in CI |
| `unsafe` blocks | each names what proves it sound | code review + Miri |

### Timing, per machine

| Gate | Result on the §9 desktop | Proven by |
|---|---|---|
| Parse NewOrderSingle | `[measured 2026-09-05]` **120.4 ns** validated, 113.5 raw, **60.9** for a Heartbeat. The Heartbeat figure was 56.3 until its fixture was corrected — it declared `9=49` against a body of 51 and the parse returned before its own checksum ([a benchmark parsed a message the parser rejects](reference/a-benchmark-parsed-a-message-the-parser-rejects.md)) | `benches/parse.rs` against `benches/baselines.tsv`, and the fixture is asserted valid before anything is timed |
| Dictionary pass, per inbound message | `[measured 2026-09-05]` **897.3 ns** for the `NewOrderSingle` `tools/w2w` sends, **218.4 ns** for its `TestRequest`; 882.1 and 169.5 for the two shapes `parse.rs` uses. **Seven times the parse it follows**, and the largest single piece of user-space work in §8 | `crates/session/benches/validate.rs` through `fixbolt_session::validate` ([ADR-0050](decisions/ADR-0050-the-dictionary-pass-is-public-so-it-can-be-timed.md)), against `baselines.tsv`. Proven by reversal: `validate` returning `None` immediately reads 1.1 ns |
| Serialise ExecutionReport (template, D9) | `[measured 2026-09-05]` **237.6 ns**, alignment pinned (ADR-0049); the 239.1 recorded 2026-08-31 was the same encoder at a different address. The 60 ns absolute target is withdrawn (ADR-0016): 93.8 (M5) · 177.6–199.4 (container) · 239.1 (desktop), and none came close | `benches/serialize.rs` against `baselines.tsv` |
| `RingDispatch` hop vs `InlineDispatch` | `[measured 2026-09-01]` inline **8.5 ns**, ring **267.4 ns** one way, **515.7 ns** round trip, on a 163-byte NewOrderSingle: about 31×, and about 1.7 ns of every byte is the `AtomicU8` copy. The inline figure was published as 1.3 ns for a day; that was the optimiser deleting the 163-byte copy, found by the arithmetic that 163 bytes in 1.3 ns is 125 GB/s from one core ([a-benchmark-can-delete-its-own-work](reference/a-benchmark-can-delete-its-own-work.md)) | `crates/engine/benches/dispatch.rs` against `baselines.tsv` |
| Per-message cost at N sessions on one thread | `[measured 2026-09-05]` **1 659.8 ns at N=1, 1 890.8 at N=64 — a ramp of 13.9%, and no step at the L2 edge.** That absence is the result: a message touching most of its 21 KiB connection would put a step at N ≈ 20. Converted through this machine's own latency-by-working-set table, a message touches **~2 to 4 KiB** | `crates/engine/benches/density.rs`, medians of 20 clean runs of 22, against `baselines.tsv`. Setup asserts each of the N sessions delivers exactly one order per turn, because a sweep whose sessions were dropped at logon is flat, fast and meaningless |
| Keeping a message for resend | `[measured 2026-09-05]` **8.9 ns** for a 191-byte `ExecutionReport` into `MemJournal<4096,512>`, walking the ring as the engine does; **6.3 ns** pinned to one slot. A 2 MiB ring is not a cache cost — 191 bytes at a 512-byte stride is what a prefetcher is for | `crates/engine/benches/journal.rs` against `baselines.tsv`, every case reading back what it wrote before anything is timed |
| What a bigger message costs the kernel | `[measured 2026-09-05]` **0.1443 ns per byte** written and read, from an 8 → 8192 byte lever. The two real `tools/w2w` sizes are cases of their own and **their difference is under this instrument's resolution**, which the module doc says where the number is | `crates/engine/benches/payload.rs` against `baselines.tsv`. Absolute figures here are environment-bound, not a round-trip claim — [a loopback write costs thirty-two syscalls](reference/a-loopback-write-costs-thirty-two-syscalls.md) |
| Wire-to-wire, loopback | `[measured 2026-09-02]` **met**: `pass 12 fail 0 unknown 1`, engine pinned to isolated `cpu6`, client to `cpu7`, medians of 20 runs of 20 000 round trips. `hft` **16 010 / 20 589 / 22 127 ns** administrative, **19 908 / 24 657 / 26 150** application; `standard` **19 447 / 24 106 / 25 609** and **20 920 / 25 618 / 27 092**. p99 ≤ 50 µs holds in all four arms. Allocations in the timed window 0 on both threads | `tools/w2w --features affinity`, driven by `scripts/w2w-baseline.sh`. Phase 1 exit criterion 6 |
| Wire-to-wire, NIC to NIC | **not met.** Loopback has no driver, no IRQ and no wire, which is why §9's NIC IRQ affinity row reads `unknown` beside every figure above | `tools/w2w` with `SO_TIMESTAMPING`, HdrHistogram, a load generator on a separate machine. STATUS item 40 |

The wire-to-wire row is the only one that measures what a counterparty experiences. Every
other row is an internal number; without this one they are unfalsifiable.

### Timing gates are per machine, not absolute

[ADR-0016](decisions/ADR-0016-per-machine-baselines-replace-absolute-targets.md),
[ADR-0031](decisions/ADR-0031-a-baseline-is-a-band.md). **There is no single published
nanosecond target.** Every timing row is judged against the figure this project measured for
that case on the CPU it is running on, inside a band `[baseline / margin, baseline × margin]`.
Both live in [`benches/baselines.tsv`](../benches/baselines.tsv), each line carrying its sample
size, its date, and the `check-machine.sh` verdict of the run that produced it.

Two findings retired the absolute column. The 60 ns serialise target was never a measurement
of this engine, only of what commercial engines are reported to reach, and no machine came
within 1.5× of it. And the ceilings had the same disease from the other side: `ring, one way`
measured 260.9 ns on a Ryzen 7 3700X, 270.7–272.9 on an EPYC 9V74 and 327.2–331.1 on an EPYC
7763, 21% between two machines of one vendor against ~1% within either, while the single
260 ns ceiling sat 0.3% *below the fastest of the three*. A ceiling no machine passes is a
ceiling somebody switches off.

**Recorded baselines**, medians of **20** qualifying `scripts/bench.sh` runs
`[measured 2026-09-05]`, on a box reading `pass 12 fail 0 unknown 1` for every run counted,
**with function alignment pinned** (ADR-0049). Every line was re-recorded that day: the
measurement changed, so what it is compared against changed with it. This machine is the only
one in `baselines.tsv`, so nothing else was invalidated.

`[2026-09-05, later the same day]` **four `validate` lines were added at n = 21, and one older
line was corrected rather than re-recorded.** `parse Heartbeat (validated)` goes **56.3 → 60.9
ns**: its fixture declared `9=49` against a body of 51, so `parse_into` returned
`Err(BadBodyLength)` on the line *before* the checksum block and the case had never summed its
own 51 bytes ([a benchmark parsed a message the parser
rejects](reference/a-benchmark-parsed-a-message-the-parser-rejects.md)). The two
`parse NewOrderSingle` lines were re-measured in the same runs, came in at 120.0 and 114.1
inside their bands, and **were not touched**.

| Case | AMD Ryzen 7 3700X | margin |
|---|---|---|
| parse NewOrderSingle (validated) | 120.4 ns | 1.10 |
| parse NewOrderSingle (no checks) | 113.5 ns | 1.15 |
| parse Heartbeat (validated) | 60.9 ns | 1.10 |
| encode ExecutionReport (template) | 237.6 ns | **1.15** |
| SendingTime from the cache | 4.9 ns | 1.10 |
| walk 1 group, 2 entries | 56.2 ns | 1.10 |
| walk 4 levels, 61-tag member list | 339.5 ns | 1.10 |
| `group_members` contains, 61 tags | 9.0 ns | 1.10 |
| encode 1 group, 2 entries | 108.8 ns | 1.10 |
| inline deliver + reply | 8.1 ns | 1.10 |
| ring, one way | 258.2 ns | 1.30 |
| ring, round trip | 496.0 ns | 1.20 |
| recv on a quiet socket | 418.5 ns | 1.10 |
| engine turn, 1 idle session | 481.0 ns | 1.10 |
| engine turn, 4 idle sessions | 1896.1 ns | 1.10 |
| engine turn, 16 idle sessions | 7694.8 ns | 1.10 |
| presession sweep, 1 quiet socket | 435.0 ns | 1.10 |
| presession sweep, 16 quiet sockets | 6819.5 ns | 1.10 |
| presession, read and route an identity | 186.5 ns | 1.10 |
| presession, registry lookup of 1 | 10.8 ns | 1.10 |
| presession, registry lookup of 40 | 100.8 ns | 1.10 |
| library, parse only | 159.6 ns | 1.10 |
| library, reply only | 804.1 ns | 1.10 |
| library, on_message | 1028.6 ns | 1.10 |
| validate NewOrderSingle | 882.1 ns | 1.10 |
| validate Heartbeat | 169.5 ns | 1.10 |
| validate TestRequest, w2w bytes | 218.4 ns | 1.10 |
| validate NewOrderSingle, w2w bytes | 897.3 ns | 1.10 |

**No other machine has a baseline, and none is invented.** The Apple M5 and the CI EPYCs have
figures scattered through this repository, but none was taken by the procedure above, so they
report `NO BASELINE`, which is counted on its own summary row and is not a pass.

**The margin is per case because one margin cannot work.** Nine of twelve cases hold inside
7.6% of their own median across a run set, while `ring, one way` draws a second mode at +24%
on roughly 1 run in 15. A single margin wide enough for that case would let `encode
ExecutionReport` drift 236 → 319 ns unnoticed.

**Bench builds pin function alignment, and the flag is read back**
([ADR-0049](decisions/ADR-0049-bench-builds-pin-function-alignment-and-the-flag-is-read-back.md)).
`scripts/bench.sh` exports `-C llvm-args=-align-all-functions=6` and then verifies, from the
built binaries, that it took — `scripts/check-bench-alignment.sh`, `[measured 2026-09-05]`
**23 of 23 own-crate text symbols on a 64-byte boundary pinned, 5 of 23 unpinned**.

It exists because `encode ExecutionReport (template)` was a 16% measurement of *where the
compiler put the code*. Its baseline was recorded at 239.1 ns and the same encoder read 280.4
four days later; the whole jump is one commit that touches no `crates/*/src/` file, and adding
inert functions the encoder never calls walks the case across 236.5–292.4 ns. Under the flag
the same perturbation holds inside 4.0%, which is why that one case carries 1.15 rather than
1.10 — pinning shrinks the layout term, it does not remove it.

**The cost, stated:** these figures are of a binary that is not the one that ships. That is
tolerable here because every §6 timing row is compared against *itself over time on one
machine*; it would not be tolerable for a published absolute. `tools/w2w` and §8's budget do
not come through `bench.sh` and are unaffected.

**Stretch, and not a gate:** serialise at about 116 ns on the §9 desktop, the measured floor of
the current `Part` shape. Reaching it needs the ~31 ns fixed prefix cost and the ~7 ns per
field in `put` attacked, not the slot scan.

### How the benchmarks are run

`scripts/bench.sh` runs every bench target and the `bench` CI job runs it on every push.
(Until 2026-08-30 nothing ran them: `cargo test --all` does not run a `harness = false` bench
target, and every ceiling above was an assertion no machine executed.) The script splits the
two kinds of benchmark by what decides their result:

| | What it measures | On a failure |
|---|---|---|
| **Invariant**: `alloc` × 3, `ring_full` | allocation counts, message counts | **CI red.** The answer is the same on every machine |
| **Timing**: `parse`, `serialize`, `groups`, `dispatch`, `turn`, `presession`, `validate`, `cost` | ns/op against this machine's own band, from a build with **function alignment pinned and read back** (ADR-0049) | **Reported, never red on a shared runner.** `bench.sh --strict`, which a §9 machine runs, is fatal on a case with no baseline for its CPU and on one that came in under its floor, because a benchmark that stops measuring reads far under its limit |

Timing ceilings are not enforced on a shared runner because they cannot be: `[measured
2026-08-30]` five runs on a 4 vCPU container gave a run-to-run spread of 5–232%, and on the
2-core CI EPYC `ring, one way` ranged 270.7–331.1 ns across four runs while single-threaded
cases held to ~3%. The spread follows whether a case crosses threads. A gate that goes red at
random gets switched off.

Every case is measured and printed before any is allowed to fail. The harness used to assert
inside each case, so the first one over its ceiling ended the process and a fourth `groups`
case over its ceiling was never seen. The script also fails when a target produces no
measurement: cargo had auto-discovered `benches/harness.rs`, a module with no case, as a ninth
bench target that reported `0 measured` and exited 0.

**A baseline must be taken through the path that will judge it.** `[measured 2026-08-31]` the
first attempt recorded medians from running the timing targets directly, and `encode
ExecutionReport` then went over its limit on the first `bench.sh` run, because `bench.sh` runs
eight targets and the case is not measured in the same state. Back-to-back runs leave the
previous suite inside `check-machine.sh`'s one-second window and disqualify their own
measurements.

## 7. Build order

Each step was a plan, a branch and a merge. **All eight are complete as of 2026-09-02.**

1. **`codec` + `dict`**: parse, serialise, generated tables ([plan](plans/2026-08-27-codec-dict.md)).
2. **Repeating groups**: `GroupIter` over the flat index, `<component>` recursion in `dict`
   ([plan](plans/2026-08-27-repeating-groups.md)).
3. **`conformance`**: the `.def` runner. Before step 4, so the gate existed before the thing
   it gates.
4. **`session`, acceptor role**: driven to 59 / 59.
5. **`session`, initiator role**: against the mirrored definitions, then interop against
   `libquickfix` in CI. A separate step because the oracle differs, not the code (ADR-0004).
   Paused after its own step 2 and resumed after step 6, because the mirrored gate measures
   framing rather than protocol wherever the harness plays the operator
   ([plan](plans/2026-08-29-session-initiator.md)).
6. **`engine`**: acceptor and connector (D8), journal, both dispatchers (D4), backpressure
   (D10).
7. **`tools/w2w`**: the wire-to-wire harness, run on Linux before step 8. `tools/jrnl` is
   beside it.
8. **`library`**: package `fixbolt`, with `examples/acceptor.rs` and `tests/end_to_end.rs`,
   which pulls in the same handler file with `#[path]` and drives it through a kernel socket.
   The example names nothing from `fixbolt_engine` or `fixbolt_session`, which is the facade's
   own test ([ADR-0041](decisions/ADR-0041-the-library-layer-buys-an-api-with-a-template-per-message.md)).

TLS (D11) has no step here. When it lands it belongs beside step 6, in `transport`.

## 8. Latency budget on kernel TCP

Where the time goes for one inbound NewOrderSingle → outbound ExecutionReport, Linux, kernel
TCP, no bypass.

### The round trip, measured

`[measured 2026-09-02]` on the §9 desktop, `check-machine.sh` `pass 12 fail 0 unknown 1`,
engine pinned to isolated `cpu6` and the client to `cpu7`, medians of 20 runs of 20 000 timed
round trips each, **over loopback**:

| Round trip | `hft` | `standard` |
|---|---|---|
| TestRequest → Heartbeat (no application) | p50 **16 010** · p99 **20 589** · p99.9 **22 127** ns | p50 **19 447** · p99 **24 106** · p99.9 **25 609** ns |
| NewOrderSingle → ExecutionReport (through an application) | p50 **19 908** · p99 **24 657** · p99.9 **26 150** ns | p50 **20 920** · p99 **25 618** · p99.9 **27 092** ns |
| if `FileLog` is on, added per message **per direction** | ~340 ns `[unmeasured]` | ~340 ns `[unmeasured]` |

`scripts/w2w-baseline.sh` is the procedure and
[reference/measured-costs.md](reference/measured-costs.md) the whole reading. **Loopback is
not a NIC**: §6's NIC-to-NIC row stays open, and these figures contain no driver and no
interrupt.

Three readings:

- **`hft` is worth 3 437 ns, 17.7%, against `standard` on the identical path.** That
  difference is D8's entire case. It also prices the wakeup row below at about 3.9 µs (the
  delta plus the ~449 ns sweep it replaces), inside the 2–5 µs this table has carried from the
  literature since it was written.
- **This engine's user-space path is 2.9% of the total**: 0.46 µs of a 16.0 µs `hft` round
  trip. The design makes that 2.9% as small as it can be, and through D8 trades `epoll`'s
  wakeup for a 449 ns poll. That trade wins at N = 1 and loses by N = 11.
- **The application round trip is 3 898 ns above the administrative one, and the committed
  benchmarks now account for about 1 094 ns of that — 28%.** `[measured 2026-09-05]` the
  dictionary pass, which STATUS item 39 named as the largest untimed candidate, is **679 ns of
  it, 17.4%**, and the gap is still **~2 804 ns unexplained**. The arithmetic is below the
  stage table. [ADR-0045](decisions/ADR-0045-parse-is-under-one-percent-of-the-wire-and-simd-is-declined.md)
  declines SIMD on this basis: parse is 0.62% of the application round trip.

### Stage by stage (`hft`, N = 1)

| Stage | Cost | Who controls it |
|---|---|---|
| NIC → kernel → socket buffer | 3–8 µs, from the literature | kernel, IRQ affinity, driver |
| TLS record decrypt, if enabled | kTLS: in-kernel with AES-NI, no extra copy. Userspace: one copy each way plus allocation (D11). **No number measured here** | this design, and the kernel |
| Wakeup, `standard` | 2–5 µs, `epoll`-class, from the literature; the core is given back | this design, D8 |
| Wakeup, `hft` | `[measured 2026-08-31]` `Engine::turn` **~449 ns × N**, N = sockets on the thread; a core is burned. ~670 ns on a core carrying `nohz_full`, which §9 no longer asks for (ADR-0021) | this design, D8, `benches/turn.rs` |
| Parse (D2) | `[measured 2026-09-05]` **0.12 µs** for a `NewOrderSingle`, **0.06 µs** for a `Heartbeat`: framing, field indexing, `9=` and `10=` only. `benches/parse.rs` parses with `NoDict`, so this row is **not** the dictionary pass — that is the row below. `[measured 2026-09-05]` the `Heartbeat` figure was 56.3 ns until its fixture was corrected; it declared `9=49` against a body of 51, so the parse returned before its own checksum ([a benchmark parsed a message the parser rejects](reference/a-benchmark-parsed-a-message-the-parser-rejects.md)) | this design |
| Dictionary pass (D1) | `[measured 2026-09-05]` **0.90 µs** for a `NewOrderSingle`, **0.22 µs** for a `TestRequest` — every field asked `is_defined_tag`, `field_type`, `allows`, `enum_allows` and `accepts`, a duplicate check that rescans the index, then `view.get(tag)` once per required header and body tag, each a linear scan. `crates/session/benches/validate.rs` through `fixbolt_session::validate` ([ADR-0050](decisions/ADR-0050-the-dictionary-pass-is-public-so-it-can-be-timed.md)). **This row is the single largest piece of user-space work in the table, and it is seven times the parse it follows** | this design |
| Session machine (D1), everything else | ~0.1 µs `[unmeasured]` | this design |
| Dispatch, inline vs ring (D4) | `[measured 2026-09-01]` **0.0085 µs** inline vs **0.27 µs** ring one way | the application's choice |
| Serialise, template (D9) | `[measured 2026-08-31]` **0.24 µs** (239.1 ns); carried as ~0.05 µs from the literature until ADR-0016 | this design |
| `send` syscall → NIC | 3–10 µs, from the literature | kernel |
| **Floor** | **~10–20 µs** | kernel |
| **User-space work only, application message** | `[measured 2026-09-05]` **~1.36 µs** at N = 1, inline: parse 0.120 + dictionary pass 0.897 + session ~0.1 + dispatch 0.0085 + serialise 0.233 | the half that was always cheap, and was not |
| **User-space work only, administrative message** | `[measured 2026-09-05]` **~0.4 µs**: parse 0.060 + dictionary pass 0.218 + session ~0.1 + the `Heartbeat` serialise, which has no committed case | |
| **Everything this design controls, N = 1** | **~1.81 µs** application, **~0.85 µs** administrative: the rows above plus one turn | |
| **Everything this design controls, N sessions** | **~1.36 µs + N × 449 ns** application | |

`[2026-09-05]` **The user-space total was ~0.46 µs on this page until the dictionary pass was
timed, and it was wrong in two ways at once**: it added a `NewOrderSingle`'s parse to an
`ExecutionReport`'s serialise and compared the sum against an *administrative* round trip, and
its `session ~0.1 µs` silently stood in for a pass that costs 0.90 µs on an application
message. The 2.9%-of-the-total reading above is the same arithmetic and now reads **8.5%** for
an application message on the application round trip — still small against a 10–20 µs floor,
which is why nothing in the design moves, but it is three times what this page said.

### The 3 898 ns, added back

`[measured 2026-09-05]` `--path app` costs 3 898 ns more than `--path admin` at p50. What
`--path app` **adds**, from committed benchmarks on this box, `pass 12 fail 0 unknown 1`:

| What the application path adds | ns | Case |
|---|---|---|
| inbound parse, a `NewOrderSingle` instead of a `TestRequest` | ~**+60** | `parse NewOrderSingle (validated)` 120.0 − `parse Heartbeat (validated)` 60.4. **Nearest committed cases, not `w2w`'s exact bytes** — 15 fields against 8, where `w2w` sends 16 against 9 |
| **the dictionary pass, the same substitution** | **+679** | `validate NewOrderSingle, w2w bytes` 897.3 − `validate TestRequest, w2w bytes` 218.4. These two cases *are* `w2w`'s exact bytes, copied field for field, which is the only reason the subtraction is allowed |
| dispatch to the application | +9 | `inline deliver + reply` 8.5 |
| the application's own parse | +114 | `parse NewOrderSingle (no checks)` 114.1 — `w2w`'s `Desk` re-parses with `Validation::NONE`, since the session already validated |
| the application's template patch and encode | +233 | `encode ExecutionReport (template)` 232.8 |
| **kernel copies of the bigger payload, both directions** | **+24.5** | `[measured 2026-09-05]` `TCP loopback` slope, `(14 890.9 − 12 528.8) / 16 368` = 0.1443 ns/byte, times the 170 extra bytes. **Read off the 8 → 8192 lever, not off the two real sizes** — those differ by tens of ns inside 12 600 and their direct difference read −4, +13 and +46 ns over three repetitions |
| **`Journal::put` of the reply into the ring** | **+8.9** | `journal put, 191 bytes, walking`. The administrative path never does it, so the whole figure counts |
| **measured subtotal** | **~1 128** | **28.9% of the gap** |
| the session's own `Heartbeat` serialise, which the application path does *not* do | −? | **no committed case**, so it is not subtracted |
| **still unattributed** | **~2 770** | **71.1%** |

**The largest candidate this page named turned out to be a sixth of the answer.** STATUS item
39 wrote the dictionary pass down as the leading explanation for the 3 898 ns and it is
**17.4%** of it. That is what the item asked for and it is not a cause; the arithmetic is here
so that nobody has to take the size of a number for an explanation again.

**Four candidates were named for the remainder. `[measured 2026-09-05]` two are now priced and
both are noise, and the remainder barely moved — ~2 804 to ~2 770 ns.**

| Candidate | Verdict |
|---|---|
| Two kernel copies of a larger payload each way, and the client's blocking `read` on a bigger message | **Dead: ~25 ns, 0.9%.** And three of the four byte counts this table used to quote were wrong — `strace` on the release binary reads **83/87** administrative and **149/191** application, not "79 and ~70" against "149 and ~200" |
| `Journal::put` of the outbound `ExecutionReport` | **Dead: 8.9 ns.** Confirmed in situ as well: one session, identical work, the ring swept 8 → 64 → 512 → 4 096 slots (4 KiB to 2 MiB) reads 1 659.8 / 1 635.5 / 1 654.8 / 1 657.7 — a 1.5% spread that is **not monotone** |
| The engine's framing and read-buffer management | **Open, and now holds almost all of it.** No benchmark isolates it |
| The session's own `Heartbeat` serialise on the administrative side | **Open.** No committed case, so it is not subtracted in either direction |

**Twice now the largest named candidate has not been the answer.** The dictionary pass is real,
is the biggest single row on this page, and is 17.4% of what it was nominated to explain; the
payload term was the intuitive one and is 0.9%. STATUS item 49.

**Which mode the stage table is about: `hft`.** `standard`'s round trip is measured and is in
the first table; what is unmeasured is its stage breakdown, because the wakeup is one opaque
term. A `standard` figure and an `hft` figure are not comparable and must not be quoted as if
they were; they are comparable as a *difference*, which is the one thing a difference is for
(ADR-0013 decision 4).

**The poll row is `Engine::turn`, and it carried a number nobody expected.** `[measured
2026-08-31]` `crates/engine/benches/turn.rs` measures the real sweep over real sockets: 449 ns
per session, flat from 1 to 16 within 2%, of which ~420 ns is the `recv` syscall. The same
benchmark under `taskset` found that the core §9 used to recommend was 36% slower at that
syscall, and a second boot separated the three isolation options: `isolcpus` 494.8 ns,
`rcu_nocbs` 498.2, untouched 501.8, `nohz_full` 670.7. Naming any CPU in `nohz_full` also costs
about 45 ns per kernel entry on every other CPU, so removing it moved the untouched cores too,
and the row settled at 449 ns with 24 fresh baseline runs (ADR-0021).

**Why serialise costing 239 ns rather than 60 changed nothing that matters.** The user-space
rows total ~0.46 µs against a floor of 10–20 µs, so 180 ns moves the wire-to-wire figure by
under 2% of the floor. That is why §6 could withdraw the target without the design changing.
**At N = 2 the polling sweep alone exceeds the whole user-space budget**, which is the sentence
this table did not contain until the poll was measured, and which ADR-0012 settles: one
session per polling thread is the shape this table describes, `density` carries the
`N × 449 ns` term, and no latency figure is published without its `N`.

The TLS row stays empty on purpose. It is filled in when `tools/w2w` runs the same load three
ways, TLS off, kTLS and userspace `rustls`, on one Linux box (ADR-0005 decision 5). Going below
the floor means kernel bypass, which is L0's job behind a feature flag that actually gates
(D5), and is not v1.

## 9. Deployment — the OS is part of the design

"Rust has no GC" does not mean "no jitter". p99.9 on a correct engine is usually lost to the
machine, not the code. None of this is optional for a latency measurement to mean anything.

| Setting | Why |
|---|---|
| **The machine is not a guest** | Governor, turbo, C-states, SMT and NIC IRQ affinity are **host** properties. A VM cannot set them and does not fail them loudly; the `/sys` files are simply absent. `check-machine.sh` reports `systemd-detect-virt` and steal time; a guest is a **FAIL**. Development may move to a cloud VM; measurement cannot |
| **Nothing else is running on the machine** | `[measured 2026-08-30]` the row that dominates every other row. On the Ryzen 7 3700X, all six tuning rows below move the `ring, one way` median by **0.8%** (260.6 → 259.7 ns); competing CPU load moves it by **71%** (262 → 449 ns) and takes a second mode near 324 ns from ~5% to 92% of samples. `check-machine.sh` reads CPU busy over a one-second window and **FAILs above 3%**, naming the processes |
| `isolcpus` + `rcu_nocbs` for the engine core, **and the engine thread pinned to it** | No other tenants and no RCU callbacks on the engine core. `[measured 2026-08-31]` free: 494.8 ns and 498.2 ns per turn against 501.8 untouched. `[measured 2026-09-02]` worth **11× at p99.9 and nothing at p50**, wire-to-wire, application path, one variable, both arms inside one CCD: p50 19 968 against 19 407 (the isolated core is 2.9% *slower*), p99.9 **26 300 against 266 887**, and 293 749 with no pinning at all. A 20 000-sample benchmark of a 500 ns operation could not see it; the excursion is 250 µs long |
| **CPU speculation mitigations IN FORCE** | `[measured 2026-09-01]` the single largest term in this design's budget. Turning them off makes every syscall **59–63%** cheaper: `Engine::turn` 448.9 → 175.2 ns, `recv` 420.5 → 156.9, while thirteen pure user-space benchmarks move −4.1% to +4.1% with no direction. All of it is `retbleed`'s untrained return thunk plus `spec_rstack_overflow`'s Safe RET; `vmscape` costs nothing. **This row requires them ON**, because `baselines.tsv` was recorded with them and a machine without them is not comparable. It is not advice to disable them ([ADR-0023](decisions/ADR-0023-section-9-records-the-cpu-mitigations.md)) |
| `nohz_full`: **NOT recommended** | `[measured 2026-08-31]` 160 ns on every kernel entry, 670.7 ns per turn against 494.8. What it buys is the far tail only: p50 376 against 216, p99 376 against 224, p99.9 384 against 224, and it wins from p99.99 outward (504 against 2 848). A busy `hft` engine makes ~2 000 000 kernel entries per second and this removes ~1 100 excursions of 3 µs: 0.32 s of tax against 0.0033 s of tail. Take it only for a p99.99 objective ([ADR-0021](decisions/ADR-0021-nohz-full-leaves-section-9.md)) |
| IRQ affinity: NIC queue → a core that is *not* the engine core | The engine never takes an interrupt |
| `mlockall` + pre-faulted buffers | No page fault on the hot path. The reference project's `pool.rs` touches every page at startup; copy that |
| Transparent huge pages **off** | THP compaction stalls are multi-millisecond |
| CPU frequency governor `performance`, C-states off | A core waking from C6 costs ~100 µs |
| `SO_BUSY_POLL` / `net.core.busy_poll` | Lets the kernel's own receive path spin instead of sleeping |
| **If TLS is on:** a kernel that carries the negotiated cipher suite in kTLS | kTLS support is narrower than what `rustls` will negotiate. A session that negotiates outside it drops silently to the userspace path and off the hot-path guarantee (D11). Which kernel and which suites: ADR-0005 open question 2, unanswered |

A latency number published without stating which of these were set is not a number.

### Checking the machine

`scripts/check-machine.sh` reads every row off the running machine and prints `PASS`, `FAIL`
or `? ? ?` for each, with the command that fixes a failing one. It reads only; applying these
is root, machine-specific, and belongs to the person at the box. `unknown` is deliberately
**not** a pass: a container that cannot read `/sys` must not be able to look like a tuned
host.

```
scripts/check-machine.sh          # what is in force, and how to fix what is not
scripts/bench.sh                  # counts and A/B comparisons, on any machine
scripts/bench.sh --strict         # refuses unless check-machine.sh is clean
```

`--strict` is what makes non-negotiable 10 real: it fails before it looks at a single ceiling
if the machine is not set up. Without `--strict` the run is still useful: allocation counts are
machine-independent, and an A/B comparison on the same box is valid whatever the box is.
`[measured 2026-08-30]` on the shared container this repository was developed in, the script
reports `pass 1 fail 5 unknown 3` and `bench.sh --strict` refuses, which is the correct answer
for that machine.
