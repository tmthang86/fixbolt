# `rustls`' unbuffered API hands out a queued alert only on the next call

`[read 2026-09-23]` from the `rustls` 0.23.45 source (the version in `Cargo.lock`), and
`[measured 2026-09-23]` by the tests named below. **The `rustls` documentation does not
describe this**; the source does. Found while fixing defect D3 of
[docs/plans/2026-09-23-phase-3-found-defects.md](../plans/2026-09-23-phase-3-found-defects.md);
the decision it led to is
[ADR-0151](../decisions/ADR-0151-a-tls-handshake-this-end-refuses-sends-its-alert-and-is-counted-and-a-peer-that-leaves-is-not.md).

## The trap

When a handshake fails inside `rustls` — no cipher suite in common is the everyday case — it
does two things in one call: it **queues** the alert that explains the failure
(`handshake_failure` here: `server/hs.rs:471`, `common_state.rs:562-572`) and it **returns the
error**. With the buffered API (`ServerConnection`), `complete_io` makes a last-gasp
`write_tls` and the alert reaches the peer. With the **unbuffered** API, which this engine uses
so that nothing waits, `process_tls_records` returns `UnbufferedStatus { discard, state:
Err(e) }` and nothing else: the alert stays in `sendable_tls`. It is handed out only by the
**next** `process_tls_records` call, whose loop pops `sendable_tls` as `EncodeTlsData` *before*
it reaches the branch that re-reports the error (`conn/unbuffered.rs:42-175`).

A driver that treats the first `Err` as the end — which `Handshake::pump` did until ADR-0151 —
closes the socket with the alert still queued. The counterparty reads a bare close:
QuickFIX/J logged `END_OF_STREAM`, a `rustls` client reads `UnexpectedEof`. Nothing on either
side names the cause.

## What the engine does about it

`Handshake::refuse` (`crates/engine/src/tls.rs`): after the `Err`, apply `discard`, then call
`process_tls_records` again **at most four times**, encoding each `EncodeTlsData` into the
existing outgoing buffer and acknowledging `TransmitTlsData`; then **one** non-blocking flush,
and `Step::Refused`. `[measured 2026-09-23]` with a temporary print in `refuse`, on 0.23.45 the
sequence is exactly `EncodeTlsData`, `TransmitTlsData`, then `BlockedHandshake`, which ends the
loop — three calls, two of which do the work; by reading, `Err` takes the third's place if more
input was buffered. The fourth is headroom against a future `rustls` that answers differently.

## What guards it

- `crates/engine/tests/tls.rs::a_client_with_no_suite_in_common_is_sent_a_handshake_failure_alert`
  — a `rustls` client offering only `TLS13_AES_256_GCM_SHA384` must read
  `AlertReceived(HandshakeFailure)`. **Reversal:** the re-call loop made zero-length → the
  client reads `the acceptor closed without a TLS alert (0 bytes before EOF)`. This test runs in
  the `tls` CI job, so a `rustls` upgrade that changes the behaviour turns it red rather than
  silently restoring the bare close.
- `crates/engine/tests/tls_wire.rs::a_refused_handshake_is_an_event_not_silence` — the same
  through `serve_tls`, over the wire.

## A trap the plan named that does not bite here

The plan's trap table said that calling `process_tls_records` again **without** first applying
`discard` would make `rustls` return a different error or the wrong alert.
`[measured 2026-09-23]` it does not, on this path: with the `discard` step skipped the alert
test stays green, because the call that hands the alert out pops `sendable_tls` before it
deframes anything, and the call after that re-reports the stored error whatever the buffer
holds. The engine applies `discard` anyway — it is the unbuffered API's contract — but **no test
proves that it does**, and none can while `rustls` behaves this way.

## Also answered by reading

`TlsRequireKernel=N` behaves identically: the handshake always runs in `rustls` before the
offload is considered (`TlsTransport::advance`), so the refusal happens before the two settings
diverge.
