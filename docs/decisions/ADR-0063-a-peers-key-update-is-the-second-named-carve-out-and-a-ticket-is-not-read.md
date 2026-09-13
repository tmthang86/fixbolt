# ADR-0063 — A peer's KeyUpdate is the second named carve-out, and a session ticket is not read

**Status:** Proposed · **Date:** 2026-09-13 · **Plan:** docs/plans/2026-09-04-tls.md Sửa 7 ·
**Amends the scope of:** [ADR-0005](ADR-0005-tls.md) decision 1 (its text is not changed)

## Context

ADR-0005 decision 1 carves exactly one thing out of non-negotiable 1 (*no heap allocation on
the hot path*): **the handshake**. Its *Consequences* warned that *"the next carve-out will
cite this one"* and asked that steady state under kTLS still assert zero. Step 6c of the `tls`
plan (commit `2d33d2a`) fixed a live defect — a counterparty's TLS 1.3 KeyUpdate killed the
session — and then could not make the plan's allocation test assert zero without lying.

What was measured, on the §9 desk in desktop configuration, `7.0.0-31-generic`, with a
counting allocator and a backtrace per allocation (`[measured 2026-09-13]`, plan 7.1):

- **Per KeyUpdate from the peer, rustls allocates exactly four boxes of 184 bytes.**
  `KernelConnection::update_{rx,tx}_secret` → `KeyScheduleTraffic::refresh_traffic_secret`
  → `expander_for_okm` twice per direction (`derive_next`, then `expand_secret`), each
  returning a `Box<dyn HkdfExpander>` (`rustls-0.23.44/src/tls13/key_schedule.rs:565-580,
  623-628, 808-814`). The `Hkdf` trait's signature returns the box
  (`src/crypto/tls13.rs:134-168`), so every `CryptoProvider` — `ring`, `aws-lc-rs`, or one
  written here — pays it. A `Box` of a zero-sized type would not allocate, but an expander
  holds a 32-byte PRK; moving that into a thread-local so the type can be zero-sized is key
  material in a global and was rejected unread.
- **A client reads two session tickets after the handover on every connection**, and rustls
  allocates sixteen times storing them — on the engine thread, because an initiator's engine
  thread is also its dialling thread. `Resumption::disabled()` does not remove this: the
  rustls client always offers `psk_key_exchange_modes` (`src/client/hs.rs:322-327`), a rustls
  server sends two tickets on that alone (`src/server/tls13.rs:320-340`, `builder.rs:123`),
  OpenSSL sends them regardless ([s2n-tls#4124](https://github.com/aws/s2n-tls/issues/4124)),
  and the kernel-mode client still parses the ticket, derives its PSK and clones the peer's
  certificate chain before a no-op store drops the result (`src/conn/kernel.rs:232`,
  `src/client/tls13.rs:1481-1525`, `src/client/handy.rs:10-30`).
- **The 64 KiB control-record buffer growth is fixed** — pre-sized at the handover, inside
  the existing carve-out.

How often a KeyUpdate arrives is the **peer's** property, not this engine's: rustls sends one
at `confidentiality_limit = 1 << 24` records for AES-GCM (`src/crypto/ring/tls13.rs:28-78`,
`common_state.rs:262, 367`); OpenSSL never sends one automatically
([openssl#23566](https://github.com/openssl/openssl/issues/23566), open) and caps a connection
at 32 ([openssl#8068](https://github.com/openssl/openssl/issues/8068)); Java's JSSE sends at
its key limit ([JDK-8329548](https://bugs.openjdk.org/browse/JDK-8329548)). RFC 8446 §5.5
puts the AES-GCM ceiling at 2^24.5 full-size records. In FIX terms, one record per message:
a rustls peer at 10 000 msg/s rekeys after ~28 minutes, at 1 000 msg/s after ~4.7 hours.

The work cannot move off the engine thread: on a KeyUpdate the kernel stops decrypting until
the new RX key is installed (`Documentation/networking/tls.rst`), so every message behind it
waits, and a hand-off to another thread would add to exactly the path that is blocked.

## Decision

1. **A session ticket is never read.** `tls::client_config` sets
   `ClientConfig::resumption = Resumption::disabled()`, and the kernel-side session for
   `tls::Client` is a newtype implementing `ktls_core::TlsSession` that forwards
   `peer`, `protocol_version`, `update_tx_secret` and `update_rx_secret` to
   `rustls::kernel::KernelConnection<ClientConnectionData>` and answers
   `handle_new_session_ticket` with `Ok(())` without touching the payload, counting the
   ticket. `TlsTransport<Client>::tickets_ignored()` exposes the count — a no-op cannot
   otherwise prove it ran. **Consequence stated as a rule: the initiator does not resume TLS
   sessions. Every dial is a full handshake.** Tickets therefore allocate **zero** and need no
   carve-out.

2. **A peer's KeyUpdate is the second named carve-out from non-negotiable 1**, and it is
   narrower than "control events": after the handover, **this engine and ktls-core allocate
   nothing; rustls allocates exactly the boxes the `Hkdf::expander_for_okm` signature forces —
   today four of 184 bytes per KeyUpdate — and nothing else.** The bound is the peer's rekey
   rate: at least 2^24 records apart from a rustls peer, never from an unprompted OpenSSL
   peer. A peer that rekeys faster is exercising a right RFC 8446 grants it, and what it
   costs this engine is two `setsockopt` calls per event, not the four mallocs.

3. **The test asserts the count exactly, not as a ceiling.**
   `tls_key_update.rs::a_key_update_allocates_only_the_boxes_rustls_key_schedule_forces`
   asserts `count == 4` and `largest == 184` on both sides, each constant commented with the
   rustls source line it comes from. A rustls upgrade that changes either number is a red
   that says *re-derive and update this ADR*, not a green that quietly read 0 or 6.
   `a_session_ticket_after_the_handover_allocates_nothing` asserts `count == 0` over the
   ticket window **and** `tickets_ignored() == 2`, so an absent ticket cannot pass as an
   unallocating one. `a_redial_does_a_full_handshake_not_a_resumption` asserts decision 1's
   rule from the server's `handshake_kind()`.

4. **`tools/w2w --tls ktls` asserts `allocs 0` over its timed window on both threads.** This
   is consistent with decision 2, not in tension with it: the engine under test is an
   acceptor, which receives no tickets; neither side initiates a KeyUpdate (the engine by
   decision, the w2w client because a `KernelConnection` never counts records); and the
   client reads its two tickets while reading the Logon reply that follows them on the
   ordered stream, before arming. A non-zero reading in that arm is a finding to name, never
   a reason to loosen the assertion.

5. **The engine still never initiates a KeyUpdate** (plan 6.12 item 7). Unchanged here.

6. **Upstream, once 6c-2 is green**: an issue on `rustls/rustls` with the measurement above,
   asking for a non-boxing path for `KernelConnection`'s rekey. Nothing in this repository
   waits for the answer.

## Why

- **Zero where zero costs forty lines; a counted exception where zero costs a fork.** The
  ticket path has an engine-owned seam (`ktls_core::TlsSession` is public and
  `Context<C: TlsSession>` takes any implementor), so zero is cheap and honest. The rekey
  path's allocation lives in a trait signature of a security crate; the only routes to zero
  are a fork or key material in a global, and ADR-0001 already drew the line against carrying
  a fork.
- **A carve-out is only useful if a reviewer can check it.** ADR-0005 said so of the
  handshake. "Control events may allocate" is not checkable; "four boxes of 184 bytes, from
  these lines, per peer KeyUpdate" is — and the test checks it exactly.
- **Resumption buys an initiator nothing it needs.** A FIX initiator redials a few times a
  day behind a 50–200 ms backoff ladder; saving one round trip and one signature verification
  at that moment is below the noise of the ladder, and the saving would sit inside the
  handshake carve-out anyway. Against it stands sixteen allocations on the engine thread on
  every connection.
- **The rate argument is the peer's, and is written down as such.** The engine cannot bound
  how often a peer rekeys; it can say what one rekey costs and what well-known peers do.

## Consequences

**Good**

- Ticket handling is zero-allocation by construction, and a test proves the path ran.
- The exception is the smallest true statement about the rekey path, with the source line
  and the measurement beside it; a dependency bump that moves it is a red, not a drift.
- The w2w kTLS gate keeps its hard `allocs 0`, with the ordering argument recorded so the
  next reader does not have to rediscover why it holds.
- No new dependency, no version bump, no change outside `crates/engine/src/tls.rs` and its
  tests.

**Bad — and these are real**

- **Non-negotiable 1 now has two named exceptions.** ADR-0005 predicted this ADR. The next
  one will cite both. The mitigation is the same: each exception is counted by a test, and
  "counted" here means an exact number.
- **The initiator will never resume a TLS session**, and no configuration key turns that
  back on. A venue that relies on resumption for reconnect storms sees full handshakes from
  this engine. Reversing this is a new ADR.
- **The exact numbers are rustls's, pinned by `Cargo.lock` at 0.23.44.** Every rustls bump
  now owes a re-derivation of two constants. That is the intended cost, but it is a cost.
- **Whether four mallocs beside two `setsockopt` calls are measurable in §8 is reasoning, not
  measurement.** Nothing here has timed a rekey on the engine thread. Plan 6-M measures
  steady state; a rekey's latency cost stays *not proven* until something times it.
- **`Cargo.toml` asks for 0.23.43 and the lock resolves 0.23.44**; the commit message of 6c
  and the test's rustdoc said 0.23.43. The rustdoc is corrected in 6c-2; the manifest is
  left as a minimum, which is what a caret requirement means.

## Alternatives rejected

| Alternative | Why rejected |
|---|---|
| **Widen the carve-out to "connection-lifetime control events: handshake, tickets, KeyUpdate"** | Admits sixteen allocations per connection for tickets when a forty-line newtype makes them zero. A carve-out wider than necessary is the failure ADR-0005 warned about |
| **`Resumption::disabled()` alone** | Reduces the ticket count by an unmeasured amount and does not reach zero: the kernel-mode client still parses, derives and clones before the no-op store drops the value |
| **A custom `CryptoProvider` with a non-boxing HKDF** | Impossible in the trait as published; `expander_for_okm` returns `Box<dyn HkdfExpander>` for every provider. [rustls#1551](https://github.com/rustls/rustls/pull/1551) introduced the shape and measured its own boxes |
| **Fork rustls** | A maintenance debt on a security crate, the line ADR-0001 drew against FFI and ADR-0005 held against OpenSSL |
| **Rekey on a helper thread** | The kernel has already paused decryption; a hand-off lengthens the exact path that is blocked |
| **Assert `count <= 4`** | What 6c shipped under a name that said "nothing". A ceiling is green when the number silently changes in either direction; an exact count is the only assertion that forces the re-read |
| **Demand literal zero, whatever it takes** | The two routes are a fork or a PRK in a thread-local. Neither passes review here, and the rule's purpose — no allocation where latency is measured — is met |

## Sources

- rustls 0.23.44 source (`~/.cargo/registry`, the version `Cargo.lock` resolves): `src/crypto/tls13.rs`,
  `src/tls13/key_schedule.rs`, `src/conn/kernel.rs`, `src/client/{hs,tls13,handy,client_conn}.rs`,
  `src/server/{tls13,builder}.rs`, `src/crypto/ring/tls13.rs`, `src/common_state.rs`, `src/conn.rs`.
- ktls-core 0.0.5 source: `src/tls.rs:402-445` (`TlsSession`), `src/context.rs:21-36, 493-513`, `src/lib.rs:20-24`.
- [rustls `kernel` module docs](https://docs.rs/rustls/latest/rustls/kernel/index.html) — the user of the
  API tracks message counts.
- [RFC 8446 §5.5 Limits on Key Usage](https://www.rfc-editor.org/rfc/rfc8446#section-5.5); §4.2.9 on
  `psk_key_exchange_modes` and NewSessionTicket.
- [rustls#1551 Rework KDF interface](https://github.com/rustls/rustls/pull/1551) — merged 2023-10-26.
- [aws/s2n-tls#4124](https://github.com/aws/s2n-tls/issues/4124) — OpenSSL and s2n send tickets without the extension.
- [openssl#23566 Lack of automatic key update](https://github.com/openssl/openssl/issues/23566);
  [openssl#8068](https://github.com/openssl/openssl/issues/8068) — 32 KeyUpdates per connection.
- [JDK-8329548](https://bugs.openjdk.org/browse/JDK-8329548) — JSSE's automatic KeyUpdate.
- [Linux `tls.rst`](https://docs.kernel.org/networking/tls.html) — RX pauses on KeyUpdate until rekeyed.
- Plan `docs/plans/2026-09-04-tls.md` 6.3, 6.9, Sửa 7; commit `2d33d2a`.
