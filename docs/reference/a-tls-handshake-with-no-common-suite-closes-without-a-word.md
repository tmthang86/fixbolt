# A TLS handshake with no cipher suite in common closes without an alert or an event

`[measured 2026-09-23]` found running reversal E of
[docs/plans/2026-09-23-p3-quickfixj-interop.md](../plans/2026-09-23-p3-quickfixj-interop.md)
row 3 against the `qfj-acceptor-tls` arm,
[ADR-0130](../decisions/ADR-0130-a-jvm-enters-ci-as-a-second-oracle-quickfixj-by-pinned-jar-and-our-own-judge.md)
decision 6. **This is an open question, not a closed one — no test guards the gap this page
describes, and that is stated plainly rather than implied by silence.**

## What was done

`scripts/interop-qfj.sh`'s `qfj_tls_lines` reads `INTEROP_QFJ_CIPHER` (default
`TLS_AES_128_GCM_SHA256`, the one suite fixbolt's TLS offers). Set to
`TLS_AES_256_GCM_SHA384` — a real TLS 1.3 suite, just not one fixbolt's `server_config` accepts
(`crates/engine/src/tls.rs` narrows to `TLS13_AES_128_GCM_SHA256` alone) — and run the
`qfj-acceptor-tls` arm alone:

```
INTEROP_QFJ_ARMS=acceptor-tls INTEROP_QFJ_CIPHER=TLS_AES_256_GCM_SHA384 scripts/interop-qfj.sh
```

## What was seen

Every step failed, as expected — there is no session. What matters is **how** it failed, on
each side:

**fixbolt's own log (the acceptor), in full:**

```
interop: fixbolt acceptor on 127.0.0.1:15662, 1 counterparties, TLS, TlsRequireKernel=Y
interop: listening on 127.0.0.1:15662
interop: stopping on "stop"
interop: events lost 0
interop: acceptor stopped: Shutdown { sessions: 0, said_goodbye: 0, acked: 0, timed_out: 0 }
```

No `interop: event <kind>` line at all for the failed connection — not
`TlsFellBackToUserspace`, not anything else. `tools/interop`'s background thread prints every
`Observer::events` entry it sees (`crates/engine/src/observe.rs`), so the absence is not the
thread failing to print; the engine raised nothing observable for this connection attempt.

**QuickFIX/J's log (the initiator):**

```
qfj: event MINA session created: local=/127.0.0.1:49760, class org.apache.mina.transport.socket.nio.NioSocketSession, remote=localhost/127.0.0.1:15662
qfj: event Disconnecting: Encountered END_OF_STREAM
```

`END_OF_STREAM`, not a TLS alert. If fixbolt's TLS layer had sent a `handshake_failure` alert
before closing — the ordinary TLS behaviour when a server finds no acceptable cipher suite in
the `ClientHello` — MINA would ordinarily surface that as a decode or an SSL exception, not a
plain end-of-stream. What was observed is consistent with the socket simply closing.

## Why this is worth recording rather than fixing here

This plan's own bounds say so directly: row 3's brief is explicit that if the engine's TLS door
is missing something the tool needs, the step **stops and reports**, and does not edit
`crates/`. Whether fixbolt's TLS layer should send an alert on a cipher-suite mismatch, and
whether that should raise an `EventKind`, is a decision about `crates/engine/src/tls.rs` and
`observe.rs` — outside this plan's *Không đụng* list, and outside `tools/interop-qfj`'s and
`scripts/interop-qfj.sh`'s reach even if it were not.

## What is not known

- Whether the silence is rustls closing the connection without an alert, or the alert being
  sent and lost somewhere between rustls and the raw socket, or something else in
  `serve_tls_requiring`'s error path swallowing it before an event is raised. Not investigated;
  would need reading `crates/engine/src/tls.rs`'s handshake error handling, which is out of
  scope here.
- Whether a real TLS client (not QuickFIX/J specifically) would report anything different from
  `END_OF_STREAM` for the same close — not tested against a second client library.
- Whether the acceptor's userspace fallback path (`TlsRequireKernel=N`) behaves the same way.
  Only the `TlsRequireKernel=Y` arm was run.

## What guards it

**Nothing, on purpose stated here rather than left silent** — `CLAUDE.md` §4: every recorded
trap gets a regression test, and where one does not exist yet, that is said plainly. This page
is the record; a test belongs to whichever plan next touches `crates/engine/src/tls.rs`'s
handshake failure path or `observe.rs`'s event set.
