# ADR-0151 — A TLS handshake this end refuses sends its alert and is counted as an event; a peer that simply leaves is not

- **Status**: **Accepted — 2026-09-23** (manager, owner's standing mandate, with the phase-3-found-defects plan). Proposed 2026-09-23. Written by the architect (Opus) for defect D3 of
  [docs/plans/2026-09-23-phase-3-found-defects.md](../plans/2026-09-23-phase-3-found-defects.md).
  Accepting that plan accepts this ADR.
- **Date**: 2026-09-23
- **Deciders**: proposed by the architect; accepted by the manager under the owner's standing
  mandate, or by the owner.
- **Related**: [ADR-0005](ADR-0005-tls.md) (the handshake runs in userspace `rustls`);
  [ADR-0060](ADR-0060-a-deployment-that-requires-the-kernel-is-refused-twice.md) decision 3 (a
  TLS fact asked of any transport through a defaulted method on the core trait — the precedent
  this follows); `DESIGN.md` D5, D11; the finding
  `docs/reference/a-tls-handshake-with-no-common-suite-closes-without-a-word.md` on branch
  `plan/p3-quickfixj-interop` (ADR-0130 decision 6, reversal E — both on that branch,
  not yet on `main`).

## Context

### 1. What was seen — `[measured 2026-09-23]` by the interop plan's reversal E

A QuickFIX/J initiator offering only `TLS_AES_256_GCM_SHA384` against fixbolt's acceptor (which
offers only `TLS13_AES_128_GCM_SHA256`, `crates/engine/src/tls.rs:821-825`): QuickFIX/J logged
`Disconnecting: Encountered END_OF_STREAM`, not a TLS alert; fixbolt's observer received **no
event at all** for the connection.

### 2. Why there is no alert — `[read in source 2026-09-23]`

- `rustls` 0.23.45, on a `ClientHello` with no suite in common, calls
  `send_fatal_alert(AlertDescription::HandshakeFailure, NoCipherSuitesInCommon)`
  (`rustls/src/server/hs.rs:471`, error made at `:642`), which **queues** the alert into
  `common_state.sendable_tls` (`rustls/src/common_state.rs:562-572`, `:436-439`) and returns the
  error.
- In the unbuffered API the error comes back as `UnbufferedStatus { state: Err(_) }`. The queued
  alert is handed out only by **a further call** to `process_tls_records`, whose loop pops
  `sendable_tls` as `EncodeTlsData` before it looks at the stored error again
  (`rustls/src/conn/unbuffered.rs:42-175`, the `sendable_tls.pop()` arm precedes the
  `core.state` check).
- fixbolt's handshake driver returns `Step::Failed(InvalidData)` on the first `Err(_)`
  (`crates/engine/src/tls.rs:502`) and the socket is dropped. **The alert rustls prepared is
  never asked for.** The docs.rs pages for `UnbufferedStatus` and `rustls::unbuffered` say nothing
  about this (read 2026-09-23); the behaviour is read from the source, and the plan's failing test
  is what will prove it.
- RFC 8446 §6.2: an implementation that meets a fatal error condition SHOULD send the appropriate
  fatal alert, then close (<https://datatracker.ietf.org/doc/html/rfc8446#section-6.2>).

### 3. Why there is no event — `[read in repo 2026-09-23]`

The failed transport answers `recv` with `Io::Failed(_)` (`tls.rs:1421-1423`). The pre-session
stage treats every `Closed`/`Failed` as `Step::Gone` (`crates/engine/src/presession.rs:797`) and
counts it in `Progress::gone`, which the acceptor loop reads for nothing
(`crates/engine/src/lib.rs:3285-3289` passes on only `unframeable`). A connection has no
`ConnId` before its `Logon`, so the per-connection events cannot name it either.

The same driver serves both ends (`Handshake<S: Side>`), so the initiator loses its alert the
same way; its dial loop answers `Admission::Failed` with `policy.dropped` and no event
(`lib.rs:2673`).

`TlsRequireKernel=N` takes the same path: the handshake always runs in `rustls` before the
offload is considered (`tls.rs:1250-1262`), which answers the finding's third open question by
reading.

### 4. How others surface it — `[documented]`

QuickFIX/J surfaces a failed TLS handshake as an `SSLHandshakeException` in its session log
(<https://www.quickfixj.org/jira/browse/QFJ-949>, read 2026-09-23). A health-check probe that
connects and closes is ordinary in front of any TCP service, which is why the count below
excludes it.

## Decision

1. **Send the alert rustls prepared.** On `Err(_)` from `process_tls_records`, the handshake
   driver applies `discard`, then calls `process_tls_records` again **a bounded number of times
   (at most 4)**, encoding every `EncodeTlsData` into its existing 32 KiB outgoing buffer and
   treating `TransmitTlsData` as a flush; then it makes **one non-blocking flush attempt** and
   returns. If the socket says `Idle`, the alert is dropped — the connection is ending either way,
   and waiting for it would be a blocking close.
2. **A handshake this end refused is its own outcome.** The driver returns a new
   `Step::Refused` for that case; `Step::Failed(kind)` keeps meaning *"the socket failed or the
   peer left"*. `TlsTransport` remembers which it was.
3. **Any transport can be asked**, through a defaulted method on the core trait,
   `Transport::handshake_refused(&self) -> bool` (default `false`), as ADR-0060 did for
   `tls_mode`. `TlsTransport` answers `true` only after `Step::Refused`.
4. **It is counted, not logged.** `presession::Progress` gains `tls_refused: usize`: a socket
   that ends while its transport answers `handshake_refused()` is counted there instead of in
   `gone`. The acceptor loop hands the count to the engine as the unframeable count is handed,
   and the engine emits **`EventKind::TlsHandshakeRefused { count: u64 }`** under
   `ConnId::MAX`, once per turn that saw any — the shape of `OriginationUndeliverable`. The
   initiator's dial loop emits the same event (`count: 1`) when `Admission::Failed` comes from a
   refused handshake.
5. **A peer that connects and leaves is not a refusal.** It stays in `Progress::gone` and raises
   nothing, so health checks do not flood the event stream.

## Options not taken

- **A `tracing` line.** Logging sits behind a feature that is off by default; an operator
  without it would still see nothing. Rejected as the only signal; it may be added beside the
  event later.
- **An event per connection carrying the `rustls::Error`.** No `ConnId` exists before `Logon`,
  and `rustls::Error` carries `String`s in some variants — not a fieldless value an event can
  hold without allocating. A fieldless reason could be added to the variant later.
- **Folding it into `Ended(DropReason)`.** `DropReason` is the session layer's, and no session
  exists yet.
- **Counting every handshake that did not finish.** Would make a load balancer's TCP health
  check indistinguishable from a counterparty with the wrong cipher suite.

## Consequences

**Good**

- A counterparty with no suite in common is told `handshake_failure`, as RFC 8446 asks, and its
  own logs name the cause instead of `END_OF_STREAM`.
- The operator of either end sees a named event; the count says how many.
- Both ends gain it from one driver change; `TlsRequireKernel` does not change the answer.

**Bad, and these are the price**

- **Public surface grows**: a variant on `EventKind` (non-exhaustive, additive), a defaulted
  method on `Transport`, a field on `Progress` (not `#[non_exhaustive]`, so a caller building one
  with a struct literal breaks) and a variant on `tls::Step` (not `#[non_exhaustive]`; an
  exhaustive `match` breaks). `CHANGELOG.md` names each.
- **The alert is best-effort.** A full socket buffer at that instant drops it; nothing retries.
- **The decision rests on undocumented `rustls` behaviour** — that a second call hands out the
  queued alert. A `rustls` upgrade could change it; the transport-level test in the plan is the
  guard, and it runs in the `tls` CI job.
- **No reason is carried.** The operator learns *that* handshakes are refused, not *why*; the
  peer learns why from the alert. Adding a fieldless reason is a later, additive change.
- The shard acceptor has no TLS door (`crates/engine/src/shard.rs` names no TLS type), so
  nothing is added there; when one lands it must hand `tls_refused` on too.
