# ADR-0060: A deployment that requires the kernel is refused twice, because there are two ways to not get it

- **Status:** Accepted — 2026-09-10, approved by the owner before the code was written
- **Date:** 2026-09-10
- **Plan:** [tls](../plans/2026-09-04-tls.md), step 4b
- **Answers** [ADR-0005](ADR-0005-tls.md) **open question 3** — *what asserts which mode is
  actually active* — which has been open since 2026-08-27 and is the question this repository
  has repeatedly said must not be answered by a log line
- **Supplements** [ADR-0005](ADR-0005-tls.md) decision 3 (*the userspace fallback leaves the
  hot-path guarantee and must be named rather than discovered*) and
  [ADR-0018](ADR-0018-ktls-on-a-plain-socket-answers-adr-0005.md). Neither is reversed
- **Constrained by** non-negotiable 10 (no performance number without its benchmark), and by
  ADR-0013 decision 4 (a claim that does not name its mode is incomplete)

## Context

`serve_tls` exists (step 4a) and **nothing reads `TlsMode`**. A session that negotiated its way
onto userspace `rustls` serves correctly, looks identical from outside, and publishes latency
figures that describe a different code path. That is precisely the outcome ADR-0005 decision 3
says must be impossible to reach by accident, and it is reachable today.

The obvious fix is a configuration key, `TlsRequireKernel=Y`, and the phrase already written into
`TlsMode::keeps_the_hot_path`'s documentation says it *"refuses at startup"*.

**That phrase was written before anybody checked whether it was implementable, and on its own it
is not.** The mode is decided per connection, during the handshake, after the counterparty has
already arrived. At the moment `serve_tls` is called there is no connection to refuse.

### Two different things can deny you the kernel

Reading `TlsTransport::hand_over` makes the shape plain, and it is **two** failures rather than
one:

| What went wrong | When it is knowable | What it means operationally |
|---|---|---|
| **The kernel has no TLS ULP at all** — built without `CONFIG_TLS`, or the module is not loadable | **Before any connection**, by asking `setup_ulp` on a throwaway socket | The **host is misconfigured**. Every session will fall back. Nothing about the counterparty is involved |
| **The kernel is capable, and this handshake still landed in userspace** — a suite the kernel does not carry | **Per connection**, after the handshake | This **counterparty** negotiated something this kernel cannot offload. Other sessions on the same process may be fine |

`[measured 2026-09-10]` a third case exists and is **not** a fallback at all: with
`enable_secret_extraction` off, `dangerous_into_kernel_connection` refuses *after* consuming the
rustls connection, so nothing can fall back and the connection is dropped. `serve_tls` builds its
own `ServerConfig` precisely so a caller cannot reach that state, and it is out of scope here.

## Decision

**1. `TlsRequireKernel=Y` refuses at both layers, and they are separate refusals with separate
messages.**

- **At startup**, `serve_tls` probes this kernel once. If it cannot offload TLS and the
  deployment said `TlsRequireKernel=Y`, `serve_tls` returns `Err` and **never binds**. The
  operator learns their host is wrong **before a counterparty is affected**, and they learn it
  from a sentence about the kernel.
- **Per connection**, a handshake that completes in `TlsMode::Userspace` is **ended**, with the
  reason on the event stream. This is the case a startup probe cannot see, because the kernel
  was capable and the suite was not.

**2. `TlsRequireKernel=N` — the default — refuses nothing and reports everything.** The fallback
happens, the session serves, and `EventKind::TlsFellBackToUserspace` is raised **once per
connection**. Silence stops being the answer whether or not anybody chose to be strict.

**3. The mode reaches the engine through the `Transport` trait, with a default.**

```rust
fn tls_mode(&self) -> TlsMode { TlsMode::Plain }
```

Every existing transport is unchanged and answers `Plain`; `TlsTransport` overrides it. No
downcast, no `Any`, no second generic parameter on `Engine`, and no breaking change.

**4. No latency number is published by any of this.** Reporting the mode makes a future figure
*legible*; it does not make one. §8's TLS row stays empty until step 6 measures it.

## Alternatives considered

**Per connection only.** Simpler, and it catches everything eventually — the first counterparty
to connect is dropped and the operator investigates. Rejected because *eventually* means **in
production, against a real counterparty**, for a fault that has nothing to do with that
counterparty. A host built without `CONFIG_TLS` is knowable in one syscall before the listener
opens, and discovering it through a dropped session sends the operator to the wrong place first —
the same reasoning that gave `ServeError::LogPath` its own variant instead of folding it into
`Io`.

**Startup probe only.** Cheapest, and wrong in the direction that matters. A capable kernel does
not guarantee *this* handshake was offloaded: a suite mismatch still lands in userspace, and
under a startup-only check nothing would notice. That is ADR-0005 open question 3 left open while
appearing to be answered, which is worse than leaving it open, and it is the exact shape
`CLAUDE.md` §10 calls *a cause accepted because a knob moved with it*.

**A log line rather than an event.** Rejected by ADR-0005 open question 3's own wording — *"a
gate is needed, not a log line"* — and by non-negotiable 4: the engine thread does not log.

**An `Any`-based downcast in the engine.** Rejected: it puts a runtime type test on a path that
has a compile-time answer, and it would let a transport lie by omission. A trait method with a
default makes the answer total.

## Consequences

**Good.**

- ADR-0005 open question 3 is answered by something that **fails**, not by something that prints.
- The two failures an operator can hit are told apart, at the moment each is knowable, in the
  words of the thing that actually went wrong.
- `TlsMode` becomes readable from any transport, so `SessionSnapshot` and `tools/w2w` can carry
  it later without another trait change — which is what makes a future §8 TLS figure quotable
  under non-negotiable 10 instead of ambiguous.
- The default stays permissive. A deployment that has not thought about kTLS still runs, and
  still gets told.

**Bad, and none of it is small.**

- **`TlsMode` is now named in `transport.rs`, which is not behind the `tls` feature.** The enum
  is defined in `tls.rs` and that module *is* gated, so the type must move or be re-exported
  unconditionally. This puts a TLS concept into the core transport vocabulary of an engine whose
  `codec` crate has zero dependencies and whose `no_std` goal is live. It is the cost of not
  downcasting, and it is a real one.
- **`serve_tls` gains a syscall at startup** — one `socket` and one `setsockopt` on a throwaway
  descriptor. Irrelevant to latency, and it is still a side effect in a constructor.
- **The startup probe can disagree with reality.** It answers *this kernel can offload*, not
  *this kernel will offload for the suite your counterparty picks*. Decision 1's second half
  exists because of that gap, and anybody reading only the startup half will over-trust it.
  This ADR is where that is written down.
- **A connection dropped for `TlsRequireKernel` looks like a network fault to the counterparty.**
  There is no session yet, so there is no `Logout` and no text to carry a reason. The reason
  exists only on this side's event stream — the same one-sided silence that
  `[measured 2026-09-10]` made a failed handover read as `ECONNRESET`.
- **Two knobs' worth of behaviour under one key name.** An operator who reads
  `TlsRequireKernel=Y` as "check my kernel" will be surprised by a dropped session, and one who
  reads it as "drop bad sessions" will be surprised by a refusal to start. `CONFIGURATION.md`
  has to say both, and a single row will not do it.
