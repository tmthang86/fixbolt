# nanofixengine — Design

A FIX 4.4 engine in Rust, **acceptor-first**, built so that latency is a property the design
guarantees rather than one it hopes for.

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
pub enum Action<'a> { Send(&'a [u8]), Deliver(MessageView<'a>), Store(SeqNum, &'a [u8]), Disconnect }

fn step(&mut self, input: Input<'_>, out: &mut ActionBuf) -> Result<(), SessionError>;
```

No socket, no clock, no allocation inside. Time arrives as `Tick`.

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

pub struct FieldIndex { count: u16, fields: [FieldEntry; MAX_FIELDS] }   // reusable, no lifetime
pub struct MessageView<'a> { buf: &'a [u8], idx: &'a FieldIndex }        // two words, free to copy

pub fn parse_into(buf: &[u8], idx: &mut FieldIndex) -> Result<usize, ParseError>;
```

The caller owns one `FieldIndex` and reuses it for every message on that connection. The
parser never constructs or returns a large struct. `MessageView` is 16 bytes and can be
passed by value anywhere.

`MAX_FIELDS = 64` to start, with overflow surfaced as `ParseError::TooManyFields` rather
than silently truncated. The number is a measurement, not a preference — see ADR-0003.

### D3 — Field ordering comes from generated tables, never from hand-written code

The QuickFIX acceptance comparator compares fields **positionally**: a correct FIX message
whose fields are in a different order fails. This is recorded as a trap in
[reference/quickfix-acceptance-def-format.md](reference/quickfix-acceptance-def-format.md).

So the serialiser emits in an order derived from the dictionary at build time. Ordering is
never a judgement made at a call site.

### D4 — Engine and library are split, with a ring buffer between them

Taken from [Artio](https://github.com/artiofix/artio), which separates `FixEngine` (owns TCP
connections and session lifecycle) from `FixLibrary` (runs business logic), communicating
over Aeron.

nanofixengine starts **in-process**: one binary, two threads, an SPSC ring buffer. The
ownership model and the message types are shaped so that moving the library to another
process is a transport swap, not a rewrite.

**Why do it now rather than later:** the cost today is designing an ownership boundary that
would need to exist anyway. The cost later is rewriting every place that assumed the
application runs on the session's thread. Artio's arrangement — session starts owned by the
engine, library requests ownership — is worth copying exactly.

The property this buys: **an application that blocks does not stall the session layer.**
Heartbeats keep flowing, sequence numbers keep advancing, and the counterparty does not
disconnect because a database call took 40 ms. QuickFIX does not have this, and it is the
main reason it is hard to run at load.

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
| Serialise `ExecutionReport` | ≤ 150 ns | `benches/serialize.rs` |
| Allocations on the hot path | **0** | `benches/alloc.rs`, counting allocator |
| Session conformance | **59 / 59** | `conformance` runner |
| `unsafe` blocks | each names what proves it sound | code review + Miri |

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
4. **`engine`** — TCP acceptor, journal, the ring buffer to the library.
5. **`library`** — the public API and the first end-to-end example.

Step 2 before step 3 is deliberate: the gate exists before the thing it gates.
