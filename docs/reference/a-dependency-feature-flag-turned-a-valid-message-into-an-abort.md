# A dependency's feature flag turned a valid protocol message into an abort

> `[measured 2026-09-13]` — found while building the `tls` plan's step 6c,
> `docs/plans/2026-09-04-tls.md`, Sửa 6. **`[to testing-skills]`**

## The claim

A long-lived TLS session under kTLS survives a TLS 1.3 `KeyUpdate` from the counterparty.
`rustls` and OpenSSL both send one on their own, once a connection has carried enough records
— it is not an edge case a venue opts into, it is what RFC 8446 asks a conforming peer to do
once a confidentiality limit is reached. Until step 6c, nothing in this repository had ever
sent one: every acceptance definition, every wire test, every interop run against a real
`libquickfix` exchanges a handful of messages and closes long before any implementation's
rekey threshold.

## The dependency, and the flag nobody had looked for

The kernel-mode TLS path (D11) hands the connection to `ktls-core` 0.0.5 after the handshake.
Its `Context::process_tls_records` handles a `KeyUpdate` control message with two arms, gated
on its own Cargo feature, `tls13-key-update`:

```rust
#[cfg(not(feature = "tls13-key-update"))]
{
    crate::warn!("… TLS 1.3 key update support is disabled");
    return self.abort(
        socket,
        InvalidMessage::UnexpectedMessage("TLS 1.3 key update support is disabled"),
        AlertDescription::InternalError,
    );
}

#[cfg(feature = "tls13-key-update")]
{
    // install the new key …
}
```

(`ktls-core-0.0.5/src/context.rs:416-445`.) This engine had enabled `ktls-core` without that
feature — it is off by default in the dependency, and nothing in `fixbolt`'s own feature list
named it, because nobody writing the `tls` feature in `Cargo.toml` had reason to go looking for
a flag that gates a message type the test suite never sent. A session that reaches its peer's
rekey point does not negotiate anything or ask permission: the `KeyUpdate` simply arrives, and
without the flag the kernel-mode path's only two defined behaviours are "install the key" or
"send a fatal alert and abort," and it chose the second one, silently, for every build of this
engine that had ever existed.

## The exact red

Before the feature was enabled, a wire test that kept a session alive long enough to provoke a
rekey read:

```
no Heartbeat answered the TestRequest sent after a KeyUpdate:
Err(InvalidData: received fatal alert: InternalError)
```

Both ends reported the same shape: the connection did not close at the moment a message was
malformed, or a sequence number was wrong, or a field was missing — every other kind of session
death this repository's acceptance definitions and reject tests are built to catch. It closed
later, at a moment the message on the wire was completely valid, because a dependency three
layers below the session had decided, at compile time, that this category of valid message was
unsupported.

## The fix, and what it found on the way

Turning the feature on made the red Heartbeat assertion pass, with `/proc/net/tls_stat`'s
`TlsRxRekeyReceived`, `TlsRxRekeyOk` and `TlsTxRekeyOk` counters all moving to confirm the
kernel, not just the test, saw the rekey succeed. Two things the plan had assumed turned out to
be measured wrong once the fix was in:

- **The control-record buffer.** The plan had sized it at 16 KiB + 5 bytes, on the reasoning
  that this engine's own messages never need more. `ktls-core` always reserves
  `u16::MAX + 5` — **65 540 bytes** — for this buffer regardless (`ffi.rs:110`), so sizing it
  smaller did not shrink the real allocation, it just meant the buffer would have grown anyway
  on the first record that needed more than 16 KiB. The buffer is now taken at the handover,
  sized to what `ktls-core` actually reserves, once, rather than guessed at and grown later.
  Reversing the fix — setting the buffer to `None` instead — reads a **65 540-byte allocation**
  on the hot path, which is how this was caught rather than assumed.

- **`Resumption::disabled()` removes none of a client's sixteen ticket allocations.** This was
  a separate finding, made measuring the same carve-out one step later (6c-2), but it belongs
  beside this one because it is the same shape of mistake: a setting that looks like it should
  stop an allocation, unverified, shipped under a comment that said it did. `rustls`'s client
  always offers `psk_key_exchange_modes` regardless of the resumption setting, a server sends
  two tickets on that alone, and the kernel-mode client still parses each ticket, derives its
  PSK and clones the peer's certificate chain before a no-op store drops the result — sixteen
  allocations, measured with `Resumption::disabled()` set (`Window { count: 16, largest: 354 }`)
  against sixteen without it (`count: 16, largest: 355`), the one-byte difference being the
  session ID `rustls` still writes into the ticket request either way). The fix that actually
  reaches zero is a newtype, `tls::side::Ticketless`, that intercepts the ticket before `rustls`
  ever stores it — not the setting that reads as though it should. See
  [ADR-0063](../decisions/ADR-0063-a-peers-key-update-is-the-second-named-carve-out-and-a-ticket-is-not-read.md)
  for the full measurement and the decision built on it.

## The rule

**A feature flag inside a dependency can silently turn a conforming, valid input into a fatal
error, and a test suite that has never sent that input will never see it.** `cargo build`
succeeds, `cargo test` is green, the acceptance suite scores the same as always — every gate
this repository has is blind to a message shape nobody constructed. The defect was not in any
line this repository wrote; it was in a default this repository inherited without auditing the
dependency's own feature list against the protocol surface the dependency claims to implement.

`[to testing-skills]` — **the generalisation is not about TLS.** Any dependency that implements
a protocol or a format can carry its own internal feature flags, each changing behaviour for
some subset of valid inputs, and a consumer's test suite proves nothing about a flag it never
knew to look for. The defence is not "read the dependency's source" as a one-time audit — that
caught this one instance — but a standing question for any protocol dependency: *what inputs
does the spec require it to handle that my own tests have never constructed?* A corpus of real
captures (this repository's `.def` files, in FIX's case) is the partial answer, because a real
counterparty eventually sends the input an invented test suite did not think of; the residual
risk is exactly the input that is valid, required by the spec, but rare enough that no capture
at hand contains it either — which is what a TLS 1.3 rekey was here, until a long-enough-lived
test session finally triggered one.

**Regression test:** `crates/engine/tests/tls_key_update.rs`, the case that keeps a session
alive past a provoked rekey and asserts the Heartbeat answer it used to lose
(`a_key_update_allocates_only_the_boxes_rustls_key_schedule_forces` for the allocation claim
above, `a_session_ticket_after_the_handover_allocates_nothing` for the ticket claim).
