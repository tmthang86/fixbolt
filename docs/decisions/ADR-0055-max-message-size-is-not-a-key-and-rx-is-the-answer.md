# ADR-0055 — `MaxMessageSize` is not a configuration key in any engine, and `RX` is the answer

- **Status**: Accepted — 2026-09-05
- **Date**: 2026-09-05
- **Deciders**: Tran Manh Thang
- **Related**: [ADR-0040](ADR-0040-a-configuration-file-refuses-what-it-does-not-understand.md) —
  a file refuses what it does not understand, which is why an invented key is expensive ·
  [ADR-0046](ADR-0046-the-ring-is-the-resend-store-and-a-replay-goes-in-batches.md) — the ring
  that dominates a session's memory ·
  [ADR-0047](ADR-0047-the-four-buffer-sizes-are-the-callers-through-a-second-function.md) — `RX`
  is a parameter the caller already has ·
  [DESIGN.md](../DESIGN.md) §2, §9 · `STATUS.md` item 45 (wave B) ·
  [prior-art.md](../reference/prior-art.md) ·
  [plan](../plans/2026-09-04-settings-for-both-roles.md) step 0

## Context

`STATUS.md` item 45 listed `MaxMessageSize` among the things "a QuickFIX-family engine carries
that this one does not", and wave B's first plan was scoped to add it as a configuration key.
The source of that entry was `docs/reference/prior-art.md`, which called it a *"per-session
knob"* of QuickFIX.

**It is not a configuration key in any engine surveyed.** The claim came from reading a tag
name as a settings name, and it survived because nothing cross-checks a prior-art claim against
the source sitting in `vendor/`.

`[verified 2026-09-05, re-run from the checked-out vendor tree]`

- `vendor/quickfix-src/src/C++/SessionSettings.h` declares **113 configuration keys** (114
  `#define`/`const char` lines, one of which is the include guard). `MaxMessageSize` is not one
  of them.
- `vendor/quickfix-src/src/C++/FixFieldNumbers.h:61` — `const int MaxMessageSize = 383;`. It is
  **tag 383, an optional field of the `Logon` message** (`spec/FIX44.xml:284`), by which the two
  ends *tell each other* their limit. A protocol matter, not a file matter.
- QuickFIX/J's configuration reference has no such setting either.

So the question behind the key is real — what bounds an inbound message — but the shape the plan
assumed for the answer does not exist anywhere it was borrowed from.

## What the four engines actually do

`[surveyed 2026-09-04, sources read; the table is reproduced in prior-art.md]`

| Engine | Read buffer | Ceiling | Set where | Over-long |
|---|---|---|---|---|
| QuickFIX C++ | `std::string`, appended to, grows on the heap | **none** | — | waits forever, buffer grows on every read. `Parser::readFixMessage` takes an `int length` and checks only `< 0`, so `9=2000000000` is a denial-of-service surface |
| QuickFIX/J | no message-size setting | **none** | — | — |
| **Artio** | fixed `ByteBuffer`, 16 KiB default | 16 KiB | `receiverBufferSize(int)` at engine construction | records the message and **disconnects**, naming the cause |
| **fixbolt** | `[u8; RX]`, 4 KiB | 4 KiB | a type parameter, compile time | `Cut::Garbage` → the session decides; **except pre-session, which closes silently** |

Two things follow, and only one of them is a gap.

**fixbolt's bound is the better one and is already in the right place.** Artio is the closest
engine in philosophy, and the only engine of the four with a ceiling at all sets it *where the
engine is constructed* — which is exactly where `RX` is set here. Artio also waits until its
buffer is genuinely full (`offset == 0 && byteBuffer.remaining() == 0`) where fixbolt decides
from `9=` alone, so fixbolt refuses earlier and on better evidence. **This is a place fixbolt is
ahead of its prior art, not behind it.**

**Artio names the reason and fixbolt does not.** That is the gap, and it is not a config key.

## Decision

**1. There is no `MaxMessageSize` configuration key, and there will not be one.** No engine
surveyed has it, and under ADR-0040 every key in the file is a promise the parser must keep
forever. A key that only repeats a compile-time type parameter would be a promise about
something the file cannot change.

**2. `RX` is the answer to the question the key was standing in for**, and ADR-0047 already put
it in the caller's hands: `serve_with::<N, RX, TX, APP>` and its nine siblings take it, and
since ADR-0047 the pre-session buffer **is** the same `RX` rather than a second constant under a
comment promising it matched.

**3. Tag 383 is a separate, smaller piece of work and is named as open, not scheduled here.**
Sending `383=<RX>` in a `Logon` and reading the counterparty's is a `session` change: the
protocol already has the conversation this key was invented to have. It is not part of wave B.

**4. Closing a connection because a frame is longer than the buffer must have a name.** Today
the pre-session stage answers `Step::Gone` (`presession.rs:690`) and the socket closes with no
reason and no event, counted in the same `p.gone` as a peer that simply left. `conn.rs:348`
already argues this exact point for `DuplicateIdentity` — *"named, not merely closed"* — and the
argument was never applied here. **Wave B's first plan, step 6.**

**5. `RX = 4096` is not raised, and the reason is that it has never been measured — not that it
is expensive.** The owner has said that on the production server RAM is not the constraint. That
opens the question rather than closing it; what closes it for now is non-negotiable 10: `RX` is
on the hot path, so changing it is a measurement on the §9 machine (`benches/turn.rs` at two
values of `RX`), and this plan runs on a laptop. **Wave C.**

## What memory it would actually cost, since that is the first objection

`[measured 2026-09-05, this Linux development box, `size_of` is a compile-time fact so the
figures hold on any target with the same pointer width and layout]`
`cargo test -p fixbolt-engine --test connection_size -- --nocapture`

```text
Connection RX=4096  : 23 760 bytes
Connection RX=16384 : 36 048 bytes
```

| | inline | heap (journal ring) | total per session |
|---|---|---|---|
| `RX = 4096` | 23 760 | 2 129 920 | **2.054 MiB** |
| `RX = 16384` | 36 048 | 2 129 920 | **2.066 MiB** |

The difference is **12 288 bytes — exactly the buffer, with no padding introduced**, which is
what `connection_size.rs` asserts rather than states. Against `Store = MemJournal<4096, 512>` at
`4096 × (512 + 8)` on the heap, **quadrupling the receive buffer is +0.57% of a session.**

**And a larger `RX` buys capacity, not speed.** The mechanism was looked for and is not there:
`Framer::cut` and `take` run on `self.len`, never on `N` (`frame.rs`), and the scan touches one
cache line per connection at either size, so TLB reach does not move. Anyone raising `RX`
expecting nanoseconds is raising the wrong constant.

**Three real risks, none of them latency:**

1. ~~`PRE` must follow `RX`, and only a comment holds it~~ — **fixed by ADR-0047 before this
   plan reached it.** Both `const PRE: usize = 4096;` are gone; the pre-session buffer is the
   engine's `RX` as a type parameter. Recorded here because the plan carried it as work.
2. `Connection::new` is built on the stack and then pushed. Release builds normally elide it;
   nothing guarantees it, and `Engine::add` already allocates ~2 MiB on the engine thread.
3. **The ceiling only moves in one direction.** `SLOT_LEN = 512` bounds what the resend ring can
   hold, tied to `TX` through `resend_batch × SLOT_LEN < TX` (ADR-0046). Accepting 16 KiB while
   replaying 512 bytes is not a silent loss — `puts_refused` and `EventKind::JournalRefused`
   both say so — but it is an asymmetry. **Four constants travel together, not one.**

## Consequences

**Good**

- **A key that would have been fixbolt's own invention is not added.** Under ADR-0040 the file
  refuses what it does not understand, so every key is permanent; the cheapest one is the one
  not added.
- **`prior-art.md` is right about a competitor for the first time by checking rather than by
  recalling.** `[corrected 2026-09-04]` The correction is in place and names its source.
- **The real gap is smaller and better defined than the imagined one**: name the disconnect
  (decision 4), and say `383` on the wire (decision 3).
- **A number that three documents were doing arithmetic on is now asserted.**
  `crates/engine/tests/connection_size.rs` fails if a bigger receive buffer ever costs more than
  the buffer.

**Bad, and named**

- **fixbolt still has no way for an operator to change the message ceiling without a
  recompile**, and that is a genuine difference from Artio, whose `receiverBufferSize(int)` is a
  runtime argument. The type parameter is a deliberate design position (D2, `CLAUDE.md` §6: the
  caller picks `N`, no hidden constant), and it does mean a desk that meets a 20 KiB
  `SecurityList` at four in the afternoon rebuilds rather than edits.
- **`RX = 4096` is now explicitly an unmeasured default**, carried another wave. The acceptance
  corpus never exceeds ~200 bytes, so nothing in this repository's gates has an opinion on it.
- **Artio's default is four times ours.** That is a data point and not evidence: Artio's buffer
  is on the heap, so its ceiling is cheaper to raise than a `const` here.

**Found by doing it, and it was not in the plan**

- **The plan's memory figures were eight bytes stale, and the test written to hold them found a
  test that could not fail.** The published pair was 23 752 / 36 040, measured 2026-09-04 on a
  laptop; wave A added eight bytes to `Connection` and neither `CONFIGURATION.md` nor `GUIDE.md`
  had any way to notice. Both are corrected here.
  The second assertion drafted alongside — *the resend ring is on the heap* — **did not go red
  when reversed. It stopped the crate compiling**, because `journal.rs:110` already carries
  `const _: () = assert!(size_of::<Store>() <= 64);` under a comment reading *"a compile error,
  not a test"*. The test could never have failed while the crate existed. It was deleted and the
  finding written into the test file's own header; the generalised form is in
  [a-test-that-cannot-fail-reads-as-coverage](../reference/a-test-that-cannot-fail-reads-as-coverage.md),
  **`[to testing-skills]`**.

## Not decided here

- What `RX` should be. Wave C, on the §9 machine, with `benches/turn.rs` at two values.
- The other four constants (`SLOTS`, `SLOT_LEN`, `TX`, `RingDispatch::DEFAULT_CAPACITY`) —
  a defaults-for-a-production-server ADR, same trip.
- Whether tag 383 is honoured as well as sent: refusing a counterparty's stated ceiling is a
  session-layer policy question that decision 3 does not answer.
