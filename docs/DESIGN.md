# nanofixengine — Design

A FIX 4.4 engine in Rust, **bidirectional** — acceptor and initiator on one session core,
parameterised by role ([ADR-0004](decisions/ADR-0004-bidirectional-engine.md)) — built so that
latency is a property the design guarantees rather than one it hopes for.

**Positioning, stated plainly:** the fastest FIX acceptor that can be built **on kernel TCP**.
The acceptor stays the headline because that is where the gap is: as of 2026-08-27 the Rust
ecosystem has no production-proven FIX acceptor, while it already has two initiators
([reference/prior-art.md](reference/prior-art.md)). The initiator ships in the same phase, held
to the same gates — it is simply not the differentiator.

Not an HFT client, not kernel-bypass. FIX tag=value over the kernel stack has a latency floor
of roughly **10–20 µs** wire-to-wire that no codec can move — the figure §8 derives, and the
only one this repository uses. The job is to make everything *above* that floor disappear, and
to measure the floor honestly.

What must be built and in which phase is [PRD.md](PRD.md); this document is *how*.

> **`nanofixengine` is a placeholder name.** It exists to get off the collision with
> `matthart1983/nanofix`. The shortlist of clean replacements is in [STATUS.md](../STATUS.md).

Settled decisions live in [decisions/](decisions/). What the landscape looks like is in
[reference/prior-art.md](reference/prior-art.md). Numbers that were actually measured — and
what they cost the designs that ignored them — are in
[reference/measured-costs.md](reference/measured-costs.md).

---

## 1. The finding this architecture is built around

A mature C++ FIX engine (fix8, 68% faster than QuickFIX) encodes a `NewOrderSingle` in
**2.1 µs** on production hardware, and says of itself that **1.4 µs** of that remains with
the framework stripped out. A Rust flyweight parser measured on an Apple M5 on 2026-08-27
parsed the same message shape in **139 ns**.

The gap is not the bytes. It is the framework: object models, dictionary lookups at runtime,
virtual dispatch, mandatory validation. Confirmation from the other direction: `hffix`
deletes the framework entirely — parse in place, no session layer — and is the fastest thing
in the survey.

**The organising principle: keep the framework off the hot path.** Every layer below is
shaped by it.

A second principle, learned from reviewing the first draft of this document: **the codec is
~1% of the wire-to-wire budget on kernel TCP.** A design that optimises the codec and says
nothing about I/O strategy, outbound encoding, or the OS underneath has optimised the wrong
1%. §4 D8–D10, §8 and §9 exist because of that review.

## 2. Layers

```
┌─────────────────────────────────────────────────────────────┐
│  Application — implements SessionHandler                    │
└──────────────────────────▲──────────────────────────────────┘
                           │
     ┌─────────────────────┴──────────────────────┐
     │  InlineDispatch — same thread as the       │  ← THE DEFAULT (D4)
     │  session machine, zero hops, the borrowed  │
     │  MessageView handed straight through       │
     │                  ── or ──                  │
     │  RingDispatch — SPSC ring, library on its  │  ← the option: an application
     │  own thread. In-process now, shared        │    that may block cannot stall
     │  memory later, without an API change       │    the session layer
     └─────────────────────┬──────────────────────┘
┌──────────────────────────┴──────────────────────────────────┐
│ L4  library    session ownership, dispatch to business code │
├─────────────────────────────────────────────────────────────┤
│ L3  engine     TCP accept AND connect (ADR-0004), drives    │
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

Added one at a time, each behind an approved plan.

| Crate | Layer | Owns | Depends on |
|---|---|---|---|
| `codec` | L1 | Parse and serialise. The hot path. Target: `no_std`-compatible, zero dependencies | — |
| `dict` | build | Code generation from FIX XML: tag constants, message shapes, required-field tables, **field ordering**, group delimiters and members, and the four validation tables — defined tags, message types, per-message tag sets, field types and enum values | `codec` — it implements `codec::Dictionary` |
| `session` | L2 | The FIX session state machine. Pure. No I/O. `Role`-parameterised. Time enters as `Tick`, in **milliseconds since 0000-01-01** — see D13 | `codec`, `dict` |
| `transport` | L0 | `Transport` trait + TCP implementation; TLS behind a feature flag (D11) | — |
| `engine` | L3 | TCP **acceptor and connector**, drives session machines, owns the journal | `session`, `transport` |
| | | `[2026-08-30]` step 1 of six exists: `Transport`, `TcpTransport`, `Loopback`, `Waiting`. `transport` is a module here rather than its own crate until something needs it to be otherwise | |
| `library` | L4 | The application-facing API | `engine` |
| `conformance` | dev | The `.def` acceptance runner, both roles. Also owns the corpus loader and the echo application the corpus assumes — **built before `session`**, so the gate exists before the thing it gates | `codec`, `dict` |

## 4. The decisions that shape it

### D1 — The session layer is a pure state machine with no I/O

```rust
pub enum Input<'a> { Message(MessageView<'a>), Tick(Timestamp), Disconnected }

/// Owns a scratch buffer. A generated Reject or Heartbeat is *written into it*, not borrowed
/// from the input — the first draft of this sketch had `Send(&'a [u8])` and could not have
/// compiled for any message the session itself originates.
pub struct ActionBuf { scratch: [u8; MAX_OUT], actions: [Action; MAX_ACTIONS], len: u8 }
pub enum Action { Send { range: Range<u16> }, Deliver, Store { seq: SeqNum, range: Range<u16> }, Disconnect }

fn step(&mut self, input: Input<'_>, out: &mut ActionBuf) -> Result<(), SessionError>;
```

```rust
pub trait Role: sealed::Sealed { const SPEAKS_FIRST: bool; }
pub struct Acceptor;   // SPEAKS_FIRST = false
pub struct Initiator;  // SPEAKS_FIRST = true
```

**Written, and it came out narrower than the sketch.** `[measured 2026-08-29]` `Role` is a
sealed trait with two marker types rather than an enum, so the branch is resolved at compile
time and costs nothing at run time. `ActionBuf` does not exist: the four inputs are four
methods — `connect`, `disconnect`, `tick`, `received` — each taking an `emit` closure the
caller supplies, and each answering `Link::{Up, Dropped}`. One input may call `emit` up to five
times; `[measured]` two files in the corpus need five. The buffer the messages are written into
is the session's own, so nothing is borrowed from the input. The rest of this section stands.

**`Deliver` became a trait, and it is the only other public API.** `[measured 2026-08-29]` the
session owns the seven administrative message types (`0 1 2 3 4 5 A`) and hands everything else
to the application:

```rust
pub trait Application {
    fn on_message(&mut self, msg: &[u8], seq: u32, stamp: &[u8], out: &mut [u8])
        -> Option<Range<usize>>;
}
```

It is given the two things an application does not own — the outbound sequence number and the
clock — writes its reply into a buffer the session lends it, and returns the range it used, or
`None` to say nothing. **`None` spends no sequence number.** `received` keeps its signature and
calls `received_with` with an application that never answers, so a session used as a pure
protocol machine is unchanged.

**`Store` became a trait, and the debt is paid.** `[measured 2026-08-30]` a resend has to
replay application messages this end already sent. The session no longer keeps them: it is
handed a `journal::Journal` — `put(seq, bytes)` and `get(seq)` — exactly as it is handed an
`Application`, and `Session::received` supplies `NoJournal` so a pure protocol machine is
unchanged. An emitted `Action::Store` could not have worked, because a resend has to *read*
and an action is one-way; [ADR-0008](decisions/ADR-0008-journal-is-a-trait.md) records that
and what else differs from the sketch. The three D7 tiers are three types in `engine`.

**One machine, both roles.** The acceptor waits for `Logon` and answers; the initiator sends
`Logon` and waits. Sequence handling, resend, heartbeat, test-request and logout are the same
protocol read from the other end. [ADR-0004](decisions/ADR-0004-bidirectional-engine.md)
measured how much differs — 51 of the 59 acceptance definitions mirror unchanged — and
concluded that a session core which cannot invert is a rewrite later, not an extension.

No socket, no clock, no allocation inside. Time arrives as `Tick`. `SessionError` is a
fieldless enum — no `String`, no `format!`, nothing that allocates on an error path.

**Why this is the highest-leverage decision in the design:** the 59 QuickFIX acceptance
definitions become *unit tests*. No listening socket, no timing window, no flake, and they
run in CI in milliseconds. A session layer that is entangled with I/O can only be tested
through a socket, and socket tests are the ones that get muted.

It also makes the engine replaceable without touching protocol correctness.

### D2 — The field index is separate from the message view

Measured, not assumed. Full detail in
[ADR-0003](decisions/ADR-0003-message-representation.md) and
[reference/measured-costs.md](reference/measured-costs.md).

```rust
#[repr(C)]                       // 12 bytes, natural alignment 4. NOT align(16)
pub struct FieldEntry { tag: u32, offset: u32, length: u16, _pad: u16 }

pub struct FieldIndex<const N: usize> { count: u16, fields: [FieldEntry; N] }  // reusable, no lifetime
pub struct MessageView<'a, const N: usize> { buf: &'a [u8], idx: &'a FieldIndex<N> }

/// Incomplete is Ok, not Err: TCP delivers bytes, not messages. Folding "wait for more"
/// into the error branch makes every call site pay to tell it apart from "session is broken".
pub enum Parsed { Complete { consumed: usize }, Incomplete }

pub fn parse_into<D: Dictionary, const N: usize>(
    buf: &[u8], idx: &mut FieldIndex<N>, v: Validation,
) -> Result<Parsed, ParseError>;
```

The caller owns one `FieldIndex` and reuses it for every message on that connection. The
parser never constructs or returns a large struct.

**`MessageView` is 24 bytes, not 16.** `&[u8]` is a fat pointer — 16 bytes — plus 8 for the
index reference. `[measured]` verified with `rustc -O` on 2026-08-27; the earlier "two words"
claim in the first draft of this document and in ADR-0003 was wrong, and both are corrected in
place. The consequence is an ABI one: on x86-64 SysV and AArch64 a struct over 16 bytes is
passed **indirectly**, so any function taking a `MessageView` by value on the hot path carries
`#[inline]` and `crates/codec/src/index.rs` carries
`const _: () = assert!(size_of::<MessageView<64>>() == 24);` so that growing it fails to
compile rather than silently costing a spill.

`N` is a const generic, so the caller chooses: `FieldIndex<64>` for order flow,
`FieldIndex<512>` for a market-data snapshot, same code, no runtime cost. Overflow is
`ParseError::TooManyFields`, never silent truncation. The 64 default is a measurement, not a
preference — see ADR-0003.

**Repeating groups do not change the index, and that is the decision.** The index stays flat:
`parse_into` records tags in wire order and knows nothing about groups. A group is resolved
only when asked for, by `MessageView::group(msg_type, counter)`, which walks the flat entries
and returns a pair of positions.

```rust
pub struct GroupIter<'a, D: Dictionary, const N: usize>;  // Iterator<Item = GroupEntry<'a, N>>
pub struct GroupEntry<'a, const N: usize>;                // get(tag), group::<D>(msg_type, counter)
```

Three consequences, each of which is why it is shaped this way:

- **A message with no group pays nothing**, because none of this runs unless it is called.
  `[measured]` parse is unchanged at 77 ns; walking a group is a separate 29–145 ns depending
  on depth (`benches/groups.rs`).
- **Nothing is built and nothing is allocated.** `benches/alloc.rs` walks four nesting levels
  and reports 0.
- **The scan steps over nested regions.** A group ends at the first tag outside its member
  set, and a nested group's members are not members of the group around it, so a scanner that
  does not skip stops inside the first nested group. `[measured]` 235 of the 731 group
  positions in FIX 4.4 contain a nested group — 32%.

`declared()` (what the counter field says) and `counted()` (what is on the wire) are reported
separately and never reconciled by the codec. Whether a mismatch is a `Reject 373=16` is the
session layer's decision, and it needs the counter tag for `371=`.

### D3 — Field ordering comes from generated tables, never from hand-written code

The QuickFIX acceptance comparator compares fields **positionally**: a correct FIX message
whose fields are in a different order fails. This is recorded as a trap in
[reference/quickfix-acceptance-def-format.md](reference/quickfix-acceptance-def-format.md).

So the serialiser emits in an order derived from the dictionary at build time. Ordering is
never a judgement made at a call site.

**Inside a repeating group the ascending-tag rule does not apply**, and this is the place D3
actually bites. `MsgType` first, then header tags ascending, then body tags ascending governs
the message; a group entry is written in the dictionary's **declaration** order, delimiter
first — `269` before `270`, `279` before `285`. The counter tag itself sorts among the body
tags like any other field; nothing after it sorts at all.

`Template::encode_with::<D>` therefore walks `D::group_order(msg_type, counter)` and never the
order the caller supplied. `[measured]` `crates/codec/tests/group_roundtrip.rs` hands every
entry over in reverse and round-trips 357 top-level positions byte-for-byte.

**A round-trip against your own table proves stability, not correctness**, so the order is
checked against a second implementation: QuickFIX's generated C++ for FIX 4.4, which carries
each group as `FIX::Group(counter, delimiter, message_order(...))`. `[measured]` delimiter
agrees on 730/730 groups and QuickFIX's order is an exact subsequence of this crate's on
730/730 (`crates/dict/tests/interop_quickfix_order.rs`). Swapping two adjacent members in
every group leaves the round-trip green and turns that test red — which is why it exists.

**A DATA field is written immediately behind its length field, and the encoder writes that
length.** A DATA value may legally contain `0x01`, so a reader takes its length from the field
in front and from nowhere else. Two rules follow, and both are refusals rather than advice:
a DATA field declared without its length field fails at `TemplateBuilder::build` — once, at
startup — with `EncodeError::DataWithoutLength`, and inside a repeating group the same case
fails in `encode_with` before a byte is written; and the length's value is computed from the
data rather than taken from the caller, because a caller who can state it can state it wrongly.
`[measured 2026-08-30]` fifteen of FIX 4.4's sixteen DATA pairs have `length == data - 1`, so
ordering by ascending tag was right by accident; `Signature(89)` takes `SignatureLength(93)`
and was emitted before its length. Held by `crates/codec/tests/data_encode.rs`, and by
`group_roundtrip.rs`, which writes **508 DATA members** with a separator inside every value.

### D4 — Dispatch is a trait; inline is the default, the ring buffer is the option

Taken from [Artio](https://github.com/artiofix/artio), which separates `FixEngine` (owns TCP
connections and session lifecycle) from `FixLibrary` (runs business logic) — **but not
adopted as the default.** Full reasoning and the reversal in
[ADR-0002](decisions/ADR-0002-engine-library-split.md).

```rust
pub trait Dispatch { fn deliver<const N: usize>(&mut self, session: SessionId, msg: MessageView<'_, N>) -> Flow; }

pub struct InlineDispatch<H: SessionHandler>(H);   // same thread, zero hops — the default
pub struct RingDispatch { ring: Spsc<Frame> }      // library on its own thread
```

- **`InlineDispatch`** — the handler runs on the engine thread, directly after the session
  machine. Zero handoff, zero copy, the borrowed `MessageView` is handed straight through.
  This is the HFT-standard shape: `recv → parse → decide → encode → send` on one core.
- **`RingDispatch`** — bytes are copied into an SPSC ring and a library thread consumes them.
  Costs a hop (measured, not estimated — see §6) and ends zero-copy at the boundary. Buys the
  one thing inline cannot: **an application that blocks does not stall the session layer.**

The application picks. A simulator serving a QA app that may stall for 40 ms wants the ring.
A gateway that answers in nanoseconds wants inline. Both are the same engine.

**Why the first draft got this wrong:** Artio's split is justified largely by the JVM —
process isolation contains GC pauses. Rust has no GC, so half the motivation does not
transfer, and what remains is a property some applications need and others pay for.

**As built.** `Dispatch` carries a `const OUT_OF_BAND: bool`, and it is `false` for
`InlineDispatch` — so the engine's "collect what the other thread produced" block is behind a
constant and compiles away entirely on the default engine. A reply from the ring comes back
through `Session::send_application`, which means the sequence number and `SendingTime` are the
session's own: an application on another thread cannot get either wrong, because it is never
told them.

A reply is routed by a **connection id, never an index** — the engine drops a dead connection
with `swap_remove`, so an index is stale the moment anything hangs up, and a reply for a
connection that has gone is dropped rather than delivered to whoever took its slot.
`crates/engine/tests/dispatch.rs` asserts that, and asserts the thing that makes the whole
trait worth having: **the same message produces the same bytes on the wire under either
dispatch.** The dispatch chooses a thread, not a protocol.

The ring itself is `Box<[AtomicU8]>` — safe Rust, no dependency, and a byte-at-a-time copy
whose price is published rather than hidden. [ADR-0007](decisions/ADR-0007-spsc-ring-without-unsafe.md).

### D5 — Transport is a trait; TCP is the only implementation that ships by default

```rust
pub trait Transport { fn recv(&mut self, buf: &mut [u8]) -> io::Result<usize>; fn send(&mut self, buf: &[u8]) -> io::Result<usize>; }
```

Two rules, both learned from reading `matthart1983/nanofix`:

1. **A feature flag must gate the module declaration itself** — `#[cfg(feature = "aeron")] mod aeron;`. In that project the flag exists in `Cargo.toml` while `src/lib.rs:1` reads `mod aeron_c;` unconditionally, so `cargo test --no-default-features` fails to link for everyone who does not have Aeron installed.
2. **`build.rs` must not invoke an external toolchain unless that feature is on.** In that project it panics regardless, with the author's own home directory as a fallback search path.

Together those two make the crate unbuildable for anyone but its author. That is the failure
mode to design against, and it costs nothing to avoid.

### D6 — No `panic!`, `unwrap()` or `expect()` in any library crate

Enforced by a workspace clippy lint, not by discipline. The reference implementation carries
**276** of them in `src/`; discipline alone demonstrably does not hold this line.

### D7 — Persistence is a policy, and it is off the hot path

QuickFIX's `FileStore` calls `Sync()` on every write across three files. That single choice
is the dominant latency source in its default configuration.

The session machine emits `Action::Store(seq, bytes)` and knows nothing about how it is
satisfied. The engine appends to a memory-mapped journal under a policy the user chooses:

| Policy | Meaning | Default for |
|---|---|---|
| `None` | Nothing persisted. Resend is impossible | Tests, simulators |
| `Async` | Appended and flushed by a background thread | **The default** |
| `Fsync` | `fsync` before the message is acknowledged | Regulated deployments that require it |

**As built.** Three types rather than three branches:
`nanofix_session::journal::NoJournal` is `None`, `engine::journal::MemJournal` is a ring that
keeps but does not persist, and `engine::journal::FileJournal` carries
`Durability::{Async, Fsync}`. A `FileJournal` **also** keeps the ring and answers `get` from
it: reading a replay back off disk would be a blocking `read` on the thread non-negotiable 4
protects.

Two deviations from the paragraph above, both in
[ADR-0008](decisions/ADR-0008-journal-is-a-trait.md). The journal is **a trait the session is
handed**, not an action it emits — a resend has to *read*, and an action cannot answer. And
the file is **appended, not memory-mapped**: `mmap` means a dependency or `unsafe`, and the
engine plan authorises neither, the same fork [ADR-0007](decisions/ADR-0007-spsc-ring-without-unsafe.md)
documents for the dispatch ring.

**A journal is written and never read back.** Nothing recovers a session from the log on
startup, so `Fsync` today is an audit trail rather than a recovery mechanism. That needs a
session constructible *from* a journal, which is its own plan — STATUS open item 16.

### D8 — The engine thread busy-polls; it never sleeps in the kernel

The engine thread is pinned to an isolated core and spins on non-blocking sockets. No
`epoll_wait`, no condition variables, no futex on the hot path.

**Why:** an `epoll` wakeup costs 2–5 µs *and* brings scheduler jitter with it. On a design
whose entire user-space path is under 1 µs, a blocking wait is the single largest cost the
engine controls. It burns a core — that is the price, and it is the standard price.

A `Waiting` strategy is a trait so tests and low-priority deployments can use a blocking
variant. The default ships as spin.

**As built.** `Engine::turn` is one non-blocking pass over every connection — flush what is
queued, **tick the clock**, read once, cut whole messages out, judge them, flush again — and
`Engine::run` is `loop { if !turn() { wait.idle() } }` and nothing else. Reading *once* per
turn rather than until the socket is empty is deliberate: a counterparty that writes faster
than this end processes must not be able to starve the other connections on the thread.

**The tick comes before the read, and that is a correctness ordering rather than a taste.**
`Session::received_with` takes no clock — D1 — so it judges `SendingTime` against the last
instant a `tick` gave it, and a session that has never ticked holds zero. Reading first means
the very first message on a connection is judged against 0000-01-01 and refused for skew.
`[measured 2026-08-30]` moving the tick left the wire gate at 59 / 59.

Keeping the pass separate from the loop is what lets the 59 acceptance definitions run
**through a real socket** with no background thread, no sleep and no timing window —
`crates/engine/tests/wire.rs` drives `turn` by hand and is as deterministic as the in-process
gate.

**Non-negotiable 4 has no machine check yet, and that is stated rather than glossed.**
`dtruss` is refused by macOS SIP, and the substitute — reading undefined symbols out of the
compiled rlib — fails its own reversal, because `Engine` and `serve` are generic and are
therefore never code-generated into the library at all. **Closed 2026-08-30**:
`scripts/check-no-kernel-sleep.sh` traces `tools/w2w` — a concrete binary, on Linux, with the
syscalls attributed to the engine thread by tid — and the script's own second run swaps
`wait::Spin` for `wait::Park` and fails if that does not trip it. §6 has the row.

### D9 — Outbound messages are templates: a pre-sorted parts list, patched, not built

An `ExecutionReport` from a given session has a fixed skeleton: `BeginString`, `SenderCompID`,
`TargetCompID`, `MsgType`, and the field order (D3). That skeleton is encoded **once per
session per message type**, into a scratch buffer **the template owns**. It cannot borrow one:
the bytes are per-session and must outlive any single send.

```rust
enum Part { Static(Range<u16>), Slot(u32) }        // ranges into the template's own scratch
pub struct Template<const P: usize, const S: usize> {
    scratch: [u8; S], parts: [Part; P], len: u8,
}
pub fn encode(&self, out: &mut [u8], slots: &[(u32, &[u8])]) -> Result<Range<usize>, EncodeError>;
```

Three properties that are not obvious, and that the first sketch of this decision got wrong:

- **The parts are sorted at build time** (D3), so `encode` walks them in order and never makes
  an ordering judgement. A slot the caller does not supply is skipped, so one template serves
  messages that differ in their optional fields.
- **The body is written first; the prefix is then written right-aligned in front of it.**
  `BodyLength` is variable-width, so writing the prefix first would mean shifting the whole
  body once its width is known. That is why `encode` returns a `Range` and not a length — the
  message does not begin at `out[0]`.
- **`SendingTime` is the hidden cost.** Naive formatting is 50–100 ns, as much as a whole
  parse. The `YYYYMMDD-HH:MM` prefix is cached and re-derived once a minute; only `SS.sss` is
  formatted per message.

This is how the fastest commercial engines reach tens of nanoseconds per serialise, and it is
why the published serialise target in §6 is 60 ns, not 150.

### D10 — TCP send backpressure has a stated policy

A slow counterparty fills the socket send buffer, and `send` returns `EAGAIN`. At 50,000
`ExecutionReport`/s against a QA application this **will** happen. The engine must not block
the session machine and must not drop protocol messages silently.

Policy, per session, chosen in configuration:

| Policy | Behaviour |
|---|---|
| `Queue { max_bytes }` | Buffer in a per-session outbound ring up to a bound, then… |
| `Disconnect` | …drop the session with a `Logout(text="slow consumer")`. **The default** — a FIX counterparty that cannot keep up is a broken counterparty |
| `Block` | …spin until the socket drains. Available for tests; never the default |

The queue is its own storage. It is tempting to say the queued bytes are the ones the journal
(D7) already holds — but under `JournalPolicy::None` the journal holds nothing, and that is the
policy simulators and tests run. The queue owns a per-session buffer, sized at startup.

**As built.** `Backpressure` on `Engine` or on a single `Connection`; the queue is the
connection's `TX` buffer and `Queue { max_bytes }` only tightens the bound. Three rules the
code makes explicit and `crates/engine/tests/backpressure.rs` holds:

- **A message goes in whole or not at all.** The session emits one message per `emit` call, so
  a refusal is always at a message boundary; a queue that wrote as much as would fit would put
  a frame on the wire that the counterparty cannot recover from.
- **The Logout that says `58=slow consumer` is not subject to `max_bytes`.** It is written into
  the whole `TX` buffer after the queue is discarded — the queued messages are for a
  counterparty that stopped reading, and the one message that matters must not be the one that
  cannot be sent.
- **A socket that has died ends the connection even with bytes queued.** `[measured 2026-08-30]`
  before this, killing the socket mid-write left the connection `Up` for as long as it was
  turned, because "finished" was defined as *closing and the queue is empty*.

`Block` spins rather than sleeping, so D8 still holds for the thread — but one slow
counterparty then stops every other session on it, which is why it is never a default.

### D11 — TLS is a transport implementation, and the guarantee is stated per mode

Decided in [ADR-0005](decisions/ADR-0005-tls.md). It needs a decision at all because of one
collision: the codec parses in place at the I/O buffer, and **encrypted bytes cannot be parsed
in place.** Userspace TLS reintroduces exactly the copy
[ADR-0003](decisions/ADR-0003-message-representation.md) spent its length removing.

| Mode | When | Hot-path guarantee |
|---|---|---|
| Handshake — `rustls`, userspace | Once per session, before any message flows | **Allocation permitted.** A named, bounded carve-out from non-negotiable 1 — to the handshake, not to the connection |
| Steady state — **kTLS** | Linux, and a cipher suite the kernel carries | **Met.** The kernel delivers plaintext into the read buffer; the D8 spin loop and parse-in-place both survive unchanged |
| Steady state — userspace `rustls` | macOS, older kernels, unsupported suites | **Not met, and the documentation says so in those words.** One copy each way, and it allocates. A number measured in this mode is never quoted as the engine's number |

`cargo build --no-default-features` produces a binary with no TLS code and no crypto
dependency at all (D5, and CI proves it on a machine with neither installed).

**Unverified and load-bearing:** whether `ktls-core` can be driven from a plain non-blocking
socket with no async runtime. Its documented usage is `tokio-rustls`-shaped, this engine has no
runtime and will not acquire one, and the question cannot be answered on a macOS laptop. It is
[STATUS.md](../STATUS.md) open item 10. **No TLS plan is written until it is answered**, and if
the answer is no, ADR-0005 is superseded rather than patched.

### D13 — `Tick` counts milliseconds from year zero, not from the Unix epoch

D1 says time reaches the session only as `Input::Tick`. What that number *means* had been left
open, and the obvious answer is wrong.

`SendingTime` is `YYYYMMDD-HH:MM:SS[.sss]` — four year digits, so the wire can name any instant
from 0000 to 9999. **Counted from 1970 in a `u64`, more than a fifth of that range does not
exist.** A counterparty sending `52=19600101-00:00:00` would wrap the skew subtraction into a
difference of half a billion years: it fails no check, but it crosses one, and the failure is
silent.

So `Tick` and every parsed `SendingTime` are **milliseconds since 0000-01-01T00:00:00Z**,
proleptic Gregorian. Every timestamp FIX can express is then a non-negative `u64`, the skew is
a plain `abs_diff` that cannot wrap, and the session needs no signed arithmetic. The engine
converts once at the edge: `tick = unix_millis + clock::MILLIS_YEAR_ZERO_TO_EPOCH`.

The cost is one added constant at the edge, and one asymmetry to remember:
`codec::TimestampCache` still takes Unix milliseconds, because it is `no_std` and shared with
callers that have no session. Bridging the two is the session's job, not the codec's.

## 5. Non-goals for v1

Stated so that scope creep has to argue with a document. The full list with phases is
[PRD.md §5](PRD.md); this is the subset that shapes the architecture.

- FIX 5.0 / FIXT 1.1 — phase 2, and it arrives together with SBE, because SBE messages are
  versioned by `ApplVerID`.
- SBE, FAST, FIXML — phase 2, and **an encoding ADR comes first**: `MessageView` presupposes
  tags on the wire and SBE has none, so the question is whether there is one view type or
  several (PRD §2).
- Kernel bypass — DPDK, OpenOnload, `ef_vi`. Not before an ordinary TCP path has been measured
  and found to be the limit. §8 puts that limit at 10–20 µs. The path is decided even though
  the work is not: Onload first, because D8's spin loop and the socket API survive unchanged;
  `ef_vi` second, as an `impl Transport` behind a D5-style flag; DPDK never, because it has
  no TCP stack. It is plaintext only, so it and D11 exclude each other.
  [STATUS.md](../STATUS.md) open item 14.
- SIMD delimiter scan and checksum. Not until `benches/parse.rs` on Linux shows the parse on
  the critical path — `matthart1983/nanofix` has SIMD and parses 4–6× slower, because layout
  beat it ([reference/measured-costs.md](reference/measured-costs.md)).
  [STATUS.md](../STATUS.md) open item 12.
- Clustering, HA, replication.
- Metrics dashboards and web UIs.
- Matching engine, order book, risk. This is a protocol engine.

**No longer a non-goal:** the initiator side.
[ADR-0004](decisions/ADR-0004-bidirectional-engine.md) moved it into phase 1 on the finding
that the two roles differ by about one enum's worth of behaviour, and that a session core
which cannot invert is a rewrite later rather than an extension.

## 6. Gates

Each is a committed benchmark or test, named. **A target without a runnable gate is a wish.**

| Gate | Target | Proven by |
|---|---|---|
| Parse `NewOrderSingle` | ≤ 150 ns **published**. `[measured]` **77.0 ns**, 2026-08-28 | `benches/parse.rs`, asserting a 150 ns regression ceiling |
| Serialise `ExecutionReport` (template, D9) | ≤ 60 ns **published**. `[measured]` **93.8 ns — the target is NOT met** | `benches/serialize.rs`, asserting a 190 ns regression ceiling |
| `RingDispatch` hop vs `InlineDispatch` | measured and published, whatever it is. `[measured 2026-08-30]` inline **2.7 ns**; ring **128.0 ns** one way and **242.5 ns** round trip, on a 163-byte `NewOrderSingle`, Apple M5, macOS 25.6, unpinned — **the ring hop is ~50x the inline call**, and ~0.8 ns of every byte of it is the `AtomicU8` copy ([ADR-0007](decisions/ADR-0007-spsc-ring-without-unsafe.md)) | `crates/engine/benches/dispatch.rs`, asserting ceilings of 15 / 260 / 500 ns |
| Allocations on the hot path — codec | **0** | `crates/codec/benches/alloc.rs`, counting allocator |
| Allocations on the hot path — session | **0**, counted separately on thirteen paths: accept, refuse, tick, beat, answer, gap, fill, deliver, resend, logon_out, originate, clock, text | `crates/session/benches/alloc.rs`. The refusal path is counted apart because it is the one a hostile counterparty controls, and it is where a `format!` is easiest to reach for. `beat` and `answer` are the two the session *originates* — a heartbeat nothing asked for, and a reply to a `TestRequest` |
| Every `373` code the corpus asks for is actually produced | **12 / 12**, read out of the corpus's own `E` lines | `crates/session/tests/score.rs`. The file count cannot say this: `14a_BadField.def` holds four cases and a session answering all four with the same code still passes the file |
| The session rules the corpus cannot tell apart | each has a test of its own | `crates/session/tests/logon.rs`, `tests/reject.rs` and `tests/heartbeat.rs`. `[measured]` seven so far. Three from steps 1–3: deleting the "first message must be a Logon" check leaves the score unchanged, because `1e_NotLogonMessage.def` also carries a wrong `56=`; stamping `52=` from a constant leaves it unchanged, because `52` is one of the five tags `fields.fmt` matches by shape; a Reject that gives the inbound sequence number back leaves it unchanged, because the *too high* branch does not exist yet. Four from step 4: all three heartbeat thresholds, which the harness's whole-interval ticks cannot see; and that a garbled frame is fatal only when it claims to be a Logon, which the corpus states once from each side in different files. Five from step 5, in `tests/resend.rs`: every file that opens a gap ends before opening a second one, so closing a filled gap, replaying held messages in sequence order, and what happens when there is no room to hold one are all invisible to the score |
| Session conformance, acceptor | **59 / 59** | `cargo test -p nanofix-session --test score`, in-process, no socket. `[measured 2026-08-29]` **59 / 59** — the session plan is closed |
| The journal keeps what a resend needs, under each D7 policy | `None` fills over everything; `MemJournal` and `FileJournal` replay; a message longer than a slot is refused rather than truncated | `crates/engine/tests/journal.rs`, seven tests. Reversal: making `put` keep nothing turns four of them red **and drops the acceptance score**, which is what proves the score depends on the journal |
| Session conformance, acceptor, **through a real socket** | **59 / 59, on every machine** | `cargo test -p nanofix-engine --test wire`. The same files over TCP: kernel sockets, the real framer, the real session, the real application. The only injected part is the clock, because every `I` line in the corpus carries a fixed instant. `[measured 2026-08-30]` **59 / 59 on the M5 and on Linux x86_64** — **met**. It read 39 / 59 on Linux until the harness's client socket was given `TCP_NODELAY`, which the engine's own sockets have always had; the gate is now flat across a 20× span of its timing bounds, which is what makes the figure mean something |
| **The engine thread never sleeps in the kernel** | no blocking syscall on that thread | `scripts/check-no-kernel-sleep.sh`. Traces `tools/w2w` with `strace -f` and attributes calls to the engine thread by tid — the client blocks on purpose and would mask everything. `[measured 2026-08-30]` Linux 6.18 x86_64: `accept4`, `recvfrom`, `sendto` and **zero** of `epoll_wait`/`poll`/`select`/`futex`/`nanosleep`/`sched_yield`. **The script runs the binary again with `wait::Park` and fails if that run does *not* trip it** — non-negotiable 4 had two machine checks before this one and both were green with a sleep present |
| Allocations on the hot path — engine | **0**, counted separately on seven paths: idle, send, recv, frame, turn, busy, ring | `crates/engine/benches/alloc.rs`, counting allocator. `busy` is a whole turn carrying a message in and a reply out, and it asserts the session is still logged on at the end of the count — `[cost]` an earlier version measured a connection that had been dropped at message two and reported the test double's queue growth as the engine's |
| The conformance runner can tell right from wrong | a fake that replays each file's own expected output scores **59 / 59** | `crates/conformance/tests/fix44.rs`. Without it `0 / 59` would also be what a broken runner reports |
| Session conformance, initiator | **51 / 51** mirrored definitions, **plus** interop green against `libquickfix` | `conformance` runner + a CI interop job (ADR-0004) |
| Repeating groups — read | every group **found**, to the full nesting depth of 4, at all **731** positions the dictionary declares | `crates/codec/tests/groups.rs` — reading is done; writing is not |
| Repeating groups — written | parse → encode **byte-identical** at all **357** top-level positions, exercising all **59** counters and nesting to depth 4 | `crates/codec/tests/group_roundtrip.rs` |
| Every tag number matches another implementation | **912 / 912** against QuickFIX's `FixFieldNumbers.h`, **and** 5 168 field names whose tag FIX 4.4 does not define are refused | `crates/dict/tests/interop_quickfix_fields.rs`. The negative half is what stops `is_defined_tag` being `true` for everything |
| Every field type matches another implementation | **898 / 912** exact, **14** differences each named by tag with both spellings | same test. QuickFIX's `FixFields.h` is shared across versions and carries later refinements; the XML is the source of truth (ADR-0001) |
| Every (message, tag) pair matches another implementation | **12 524 / 12 524** body pairs, checked as **84 816** answers — every message against every tag, both directions | `crates/dict/tests/interop_quickfix_messages.rs`. One acceptance definition covers this table; 84 816 answers do |
| Every enum value is one QuickFIX also knows | **245 / 245** fields, **1 708 / 1 708** values, zero exceptions | `crates/dict/tests/enums.rs`. One-directional by construction: QuickFIX lists every version's values, so it can only confirm, never forbid |
| What each of the 23 field types accepts | at least one accepted and one refused value per type | `crates/dict/tests/field_types.rs`. **These cases are invented** — the corpus supplies two, and 23 types with 2 real cases is not coverage |
| In-group field order matches another implementation | delimiter exact on all **730** groups, and QuickFIX's `message_order` an exact subsequence of this crate's member list on all 730 | `crates/dict/tests/interop_quickfix_order.rs`, read out of QuickFIX's generated C++. Exists because the round-trip test reads the same table the encoder does and is blind to a wrong order |
| **Wire-to-wire, NIC to NIC** | p50 / p99 / p99.9 published; p99 ≤ 50 µs on kernel TCP | `tools/w2w` — `SO_TIMESTAMPING`, HdrHistogram, load generator on a **separate machine** |
| Which TLS mode is actually in force | a session that fell back to the userspace path is **detected**, not assumed | ADR-0005 open question 3 — **no gate exists yet, and that is a known hole** |
| `parse_into` never panics on hostile input | `[measured]` 304,230,294 executions, 0 crashes, 2026-08-28 | `fuzz/fuzz_targets/parse.rs`, `cargo +nightly fuzz run parse` |
| The lint config denies `unwrap` / `expect` / `panic` | red on a crate carrying all three, green once they are gone | `scripts/check-lint-config.sh`, run in CI on every push |
| Builds with nothing optional installed | `--no-default-features` on a clean runner (non-negotiable 6) | `.github/workflows/ci.yml`, its own job |
| No documentation link points at a missing file | 155 internal links resolve | `scripts/check-links.py`, run in CI |
| `unsafe` blocks | each names what proves it sound | code review + Miri |

The wire-to-wire row is the only one that measures what a counterparty experiences. Every
other row is an internal number; without this one they are unfalsifiable.

**Most of these rows run today. The rest cannot, and CI says so out loud.** The workspace has
no crates, so `fmt`, `clippy` and `test` have nothing to check — the CI job emits a warning
annotation saying it *skipped* rather than passing, because a green tick that means "there was
nothing to look at" is the exact failure `CLAUDE.md` §10 names.

The 150 ns targets are anchored to a real measurement — 139 ns for a `NewOrderSingle` on an
Apple M5 on 2026-08-27, in the harness described in
[reference/measured-costs.md](reference/measured-costs.md). They are **a reference point on
one machine, not a promise about any other.**

`[measured]` **The serialise target is missed by 56%.** 93.8 ns against a published 60 ns, on
an `ExecutionReport` with 3 fixed fields and 14 slots. The cause is visible rather than
mysterious: `encode` looks each slot up with a linear scan of the supplied list, so the cost
grows with slots × parts. It is recorded here rather than optimised, because
`reference/measured-costs.md` exists to stop exactly the reverse — optimising before
measuring on the machine that matters. The number to beat is the Linux one, at the `engine`
step.

**Published target and asserted ceiling are deliberately different numbers.** The benchmark
asserts a regression ceiling of roughly 1.5–2× the baseline measured on the machine at hand,
not 150 / 60 ns. The reason is arithmetic: 139 ns sits 8% under 150 ns, and an unpinned laptop
varies by more than 8%, so a hard assert would go red at random — and a gate that goes red at
random is a gate somebody switches off, which is worse than having none. The 150 / 60 ns
figures stay as the published targets, to be confirmed on Linux at the `engine` step.

## 7. Build order

Each step is a plan, a branch, and a merge. Nothing starts before its predecessor is green.

1. **`codec` + `dict`** — parse, serialise, generated tables. Gated by the parse/serialise and
   allocation benchmarks. [Plan](plans/2026-08-27-codec-dict.md), approved.
2. **Repeating groups** — `GroupIter` over the flat index, plus `<component>` recursion in
   `dict`. [Plan](plans/2026-08-27-repeating-groups.md), approved. Immediately after step 1,
   because without it the codec cannot read an application message.
3. **`conformance`** — the `.def` runner. 1–2 days; needed regardless of anything else.
4. **`session`, acceptor role** — the pure state machine, driven to **59/59**.
5. **`session`, initiator role** — `Role::Initiator` against the mirrorable definitions, then
   interop against `libquickfix` in CI. A separate step because the *oracle* differs, not
   because the code does (ADR-0004). **Paused after its own step 2, 2026-08-30**, and step 6
   taken first: the mirrored gate turned out to top out at 45 of 50 and to measure framing
   rather than protocol wherever the harness has to play the operator. The initiator speaks
   first and can originate; the rest waits. See
   [the plan](plans/2026-08-29-session-initiator.md) Sửa 2 and `STATUS.md`.
6. **`engine`** — busy-poll TCP acceptor **and connector** (D8), journal, both dispatchers
   (D4), backpressure (D10).
7. **`tools/w2w`** — the wire-to-wire harness, run against step 6 on Linux **before** step 8.
8. **`library`** — the public API and the first end-to-end example.

Step 3 before step 4 is deliberate: the gate exists before the thing it gates. Step 7 before
step 8 for the same reason.

**TLS (D11) has no step here, because it has no plan** — it is blocked on ADR-0005 open
question 1. When it lands it belongs beside step 6, in `transport`.

## 8. Latency budget on kernel TCP

Where the time goes for one inbound `NewOrderSingle` → outbound `ExecutionReport`, Linux,
kernel TCP, no bypass. **Typical figures from the literature, not measured here** — the
`tools/w2w` harness replaces this table with real numbers.

| Stage | Typical | Who controls it |
|---|---|---|
| NIC → kernel → socket buffer | 3–8 µs | Kernel, IRQ affinity, driver |
| TLS record decrypt, **if enabled** — kTLS **vs** userspace (D11) | in-kernel with AES-NI, no extra copy **vs** one copy each way plus allocation | **This design**, and the kernel |
| Wakeup — `epoll` **vs** busy-poll (D8) | 2–5 µs **vs** ~0 | **This design** |
| Parse (D2) | ~0.14 µs | This design |
| Session machine (D1) | ~0.1 µs | This design |
| Dispatch — inline **vs** ring (D4) | ~0 **vs** 0.2–0.5 µs | Application's choice |
| Serialise — template (D9) | ~0.05 µs | This design |
| `send` syscall → NIC | 3–10 µs | Kernel |
| **Floor** | **~10–20 µs** | Kernel |
| **Everything this design controls** | **< 1 µs** | |

The TLS row has no number in it on purpose: none has been measured here, and none is quoted
from elsewhere either. It gets filled in when `tools/w2w` runs the same load three ways — TLS
off, kTLS, userspace `rustls` — on the same Linux box (ADR-0005 decision 5).

Two readings of this table:

1. On kernel TCP, this engine's user-space path is **under 5% of the total**. The design
   makes that 5% as small as it can be, and — through D8 — removes the one kernel cost it can.
2. Going below the floor means kernel bypass (OpenOnload, DPDK, `ef_vi`). That is L0's
   job, behind a feature flag that actually gates (D5), and it is **not v1**.

## 9. Deployment — the OS is part of the design

"Rust has no GC" does not mean "no jitter". p99.9 on a correct engine is usually lost to
the machine, not the code. None of this is optional for a latency measurement to mean
anything:

| Setting | Why |
|---|---|
| `isolcpus` + `nohz_full` for the engine core | No scheduler ticks, no other tenants |
| IRQ affinity: NIC queue → a core that is *not* the engine core | The engine never takes an interrupt |
| `mlockall` + pre-faulted buffers | No page fault on the hot path. The reference project's `pool.rs` touches every page at startup — copy that |
| Transparent huge pages **off** | THP compaction stalls are multi-millisecond |
| CPU frequency governor `performance`, C-states off | A core waking from C6 costs ~100 µs |
| `SO_BUSY_POLL` / `net.core.busy_poll` | Lets the kernel's own receive path spin instead of sleeping |
| **If TLS is on:** a kernel that carries the negotiated cipher suite in kTLS | kTLS support is narrower than what `rustls` will happily negotiate. A session that negotiates outside it drops silently to the userspace path and off the hot-path guarantee (D11). Which kernel version and which suites is ADR-0005 open question 2 — **unanswered** |

A latency number published without stating which of these were set is not a number.
