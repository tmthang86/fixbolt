# nanofixengine — Design

A FIX 4.4 engine in Rust, **acceptor-first**, built so that latency is a property the design
guarantees rather than one it hopes for.

**Positioning, stated plainly:** the fastest FIX acceptor that can be built **on kernel TCP**.
Not an HFT client, not kernel-bypass. FIX tag=value over the kernel stack has a latency floor
of roughly 15–25 µs wire-to-wire that no codec can move; §8 puts numbers on it. The job is to
make everything *above* that floor disappear, and to measure the floor honestly.

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
                           │  library thread(s)
┌──────────────────────────┴──────────────────────────────────┐
│ L4  library    session ownership, dispatch to business code │
├─────────────────────────────────────────────────────────────┤
│         SPSC ring buffer — in-process now, shared memory     │
│         later, without an API change                         │
├─────────────────────────────────────────────────────────────┤
│ L3  engine     TCP accept, drives session machines, journal │
├─────────────────────────────────────────────────────────────┤
│ L2  session    FIX session protocol as a PURE state machine │
│                — no sockets, no clock, no I/O               │
├─────────────────────────────────────────────────────────────┤
│ L1  codec      parse / serialise in place, zero allocation  │
├─────────────────────────────────────────────────────────────┤
│ L0  transport  trait; TCP is the only default implementation│
└─────────────────────────────────────────────────────────────┘
```

## 3. Crates

Added one at a time, each behind an approved plan.

| Crate | Layer | Owns | Depends on |
|---|---|---|---|
| `codec` | L1 | Parse and serialise. The hot path. Target: `no_std`-compatible, zero dependencies | — |
| `dict` | build | Code generation from FIX XML: tag constants, message shapes, required-field tables, **field ordering** | — |
| `session` | L2 | The FIX session state machine. Pure. No I/O | `codec`, `dict` |
| `transport` | L0 | `Transport` trait + TCP implementation | — |
| `engine` | L3 | TCP acceptor, drives session machines, owns the journal | `session`, `transport` |
| `library` | L4 | The application-facing API | `engine` |
| `conformance` | dev | The `.def` acceptance runner | `codec` |

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
pub struct MessageView<'a, const N: usize> { buf: &'a [u8], idx: &'a FieldIndex<N> } // two words

pub fn parse_into<const N: usize>(buf: &[u8], idx: &mut FieldIndex<N>) -> Result<usize, ParseError>;
```

The caller owns one `FieldIndex` and reuses it for every message on that connection. The
parser never constructs or returns a large struct. `MessageView` is 16 bytes and can be
passed by value anywhere.

`N` is a const generic, so the caller chooses: `FieldIndex<64>` for order flow,
`FieldIndex<512>` for a market-data snapshot, same code, no runtime cost. Overflow is
`ParseError::TooManyFields`, never silent truncation. The 64 default is a measurement, not a
preference — see ADR-0003.

### D3 — Field ordering comes from generated tables, never from hand-written code

The QuickFIX acceptance comparator compares fields **positionally**: a correct FIX message
whose fields are in a different order fails. This is recorded as a trap in
[reference/quickfix-acceptance-def-format.md](reference/quickfix-acceptance-def-format.md).

So the serialiser emits in an order derived from the dictionary at build time. Ordering is
never a judgement made at a call site.

### D4 — Dispatch is a trait; inline is the default, the ring buffer is the option

Taken from [Artio](https://github.com/artiofix/artio), which separates `FixEngine` (owns TCP
connections and session lifecycle) from `FixLibrary` (runs business logic) — **but not
adopted as the default.** Full reasoning and the reversal in
[ADR-0002](decisions/ADR-0002-engine-library-split.md).

```rust
pub trait Dispatch { fn deliver(&mut self, session: SessionId, msg: MessageView<'_, 64>) -> Flow; }

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

### D8 — The engine thread busy-polls; it never sleeps in the kernel

The engine thread is pinned to an isolated core and spins on non-blocking sockets. No
`epoll_wait`, no condition variables, no futex on the hot path.

**Why:** an `epoll` wakeup costs 2–5 µs *and* brings scheduler jitter with it. On a design
whose entire user-space path is under 1 µs, a blocking wait is the single largest cost the
engine controls. It burns a core — that is the price, and it is the standard price.

A `Waiting` strategy is a trait so tests and low-priority deployments can use a blocking
variant. The default ships as spin.

### D9 — Outbound messages are templates, patched, not built

An `ExecutionReport` from a given session has a fixed skeleton: `BeginString`, `SenderCompID`,
`TargetCompID`, `MsgType`, the field order (D3). That skeleton is encoded **once per session
per message type**. At send time only the variable fields, `MsgSeqNum`, `SendingTime`,
`BodyLength` and `CheckSum` are written into pre-computed offsets.

`SendingTime` is the hidden cost: naive formatting is 50–100 ns, as much as a whole parse.
The `YYYYMMDD-HH:MM` prefix is cached and re-derived once a minute; only `SS.sss` is
formatted per message.

This is how the fastest commercial engines reach tens of nanoseconds per serialise, and it
is why the serialise gate in §6 is 60 ns, not 150.

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

The queued bytes are the same bytes the journal (D7) already holds, so queuing costs no
extra copy.

## 5. Non-goals for v1

Stated so that scope creep has to argue with a document.

- FIX 5.0 / FIXT 1.1. FIX 4.4 only, until something concrete needs more.
- SBE, FAST, FIXML.
- Kernel bypass — DPDK, OpenOnload. Not before an ordinary TCP path has been measured and
  found to be the limit.
- Clustering, HA, replication.
- Metrics dashboards and web UIs.
- The initiator side, beyond what the acceptance definitions require.

## 6. Gates

Each is a committed benchmark or test, named. **A target without a runnable gate is a wish.**

| Gate | Target | Proven by |
|---|---|---|
| Parse `NewOrderSingle` | ≤ 150 ns | `benches/parse.rs` |
| Serialise `ExecutionReport` (template, D9) | ≤ 60 ns | `benches/serialize.rs` |
| `RingDispatch` hop vs `InlineDispatch` | measured and published, whatever it is | `benches/dispatch.rs` |
| Allocations on the hot path | **0** | `benches/alloc.rs`, counting allocator |
| Session conformance | **59 / 59** | `conformance` runner |
| **Wire-to-wire, NIC to NIC** | p50 / p99 / p99.9 published; p99 ≤ 50 µs on kernel TCP | `tools/w2w` — `SO_TIMESTAMPING`, HdrHistogram, load generator on a **separate machine** |
| `unsafe` blocks | each names what proves it sound | code review + Miri |

The wire-to-wire row is the only one that measures what a counterparty experiences. Every
other row is an internal number; without this one they are unfalsifiable.

The 150 ns targets are anchored to a real measurement — 139 ns for a `NewOrderSingle` on an
Apple M5 on 2026-08-27, in the harness described in
[reference/measured-costs.md](reference/measured-costs.md). They are **a reference point on
one machine, not a promise about any other.**

## 7. Build order

Each step is a plan, a branch, and a merge. Nothing starts before its predecessor is green.

1. **`codec` + `dict`** — parse, serialise, generated tables. Gated by the parse/serialise
   and allocation benchmarks.
2. **`conformance`** — the `.def` runner. 1–2 days; needed regardless of anything else.
3. **`session`** — the pure state machine, driven to **59/59**.
4. **`engine`** — busy-poll TCP acceptor (D8), journal, both dispatchers (D4), backpressure
   (D10).
5. **`tools/w2w`** — the wire-to-wire harness, run against step 4 on Linux **before** step 6.
6. **`library`** — the public API and the first end-to-end example.

Step 2 before step 3 is deliberate: the gate exists before the thing it gates. Step 5 before
step 6 for the same reason.

## 8. Latency budget on kernel TCP

Where the time goes for one inbound `NewOrderSingle` → outbound `ExecutionReport`, Linux,
kernel TCP, no bypass. **Typical figures from the literature, not measured here** — the
`tools/w2w` harness replaces this table with real numbers.

| Stage | Typical | Who controls it |
|---|---|---|
| NIC → kernel → socket buffer | 3–8 µs | Kernel, IRQ affinity, driver |
| Wakeup — `epoll` **vs** busy-poll (D8) | 2–5 µs **vs** ~0 | **This design** |
| Parse (D2) | ~0.14 µs | This design |
| Session machine (D1) | ~0.1 µs | This design |
| Dispatch — inline **vs** ring (D4) | ~0 **vs** 0.2–0.5 µs | Application's choice |
| Serialise — template (D9) | ~0.05 µs | This design |
| `send` syscall → NIC | 3–10 µs | Kernel |
| **Floor** | **~10–20 µs** | Kernel |
| **Everything this design controls** | **< 1 µs** | |

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

A latency number published without stating which of these were set is not a number.
