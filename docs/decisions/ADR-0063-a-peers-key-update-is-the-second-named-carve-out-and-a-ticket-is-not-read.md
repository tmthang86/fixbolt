# ADR-0063 — A peer's KeyUpdate is the second named carve-out, and a session ticket is not read

**Status:** Accepted (2026-09-13, the owner approved plan Sửa 7 as proposed — plan 7.10; built in step 6c-2) · **Date:** 2026-09-13 · **Plan:** docs/plans/2026-09-04-tls.md Sửa 7 ·
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

## Revision — 2026-09-13, on acceptance (step 6c-2)

Recorded as §5 of `CLAUDE.md` requires for a `Proposed` ADR revised in place. The decisions
above are unchanged; building them measured three things this text had as unknown or had
slightly wrong, all `[measured 2026-09-13]` on the §9 desk in desktop configuration,
`7.0.0-31-generic`, rustls 0.23.44 with `ring`, by `crates/engine/tests/tls_key_update.rs`:

- **`Resumption::disabled()` alone removes none of the sixteen ticket allocations.** With it
  set and the ticket handed back to rustls's `KernelConnection` (a reversal of decision 1's
  newtype), the ticket window read `Window { count: 16, largest: 354 }` — against
  `count: 16, largest: 355` with resumption enabled, measured the same day on the step-6c code
  (`da9fe6e`) as this step's red run. *Context* said the reduction
  was unmeasured; it is zero. The newtype is therefore the whole of the ticket fix, and the
  `Resumption::disabled()` line is what keeps the userspace fallback from resuming.
- **Which half of decision 1 holds the no-resumption rule, per path.** Kernel path: tickets
  arrive after the handover, so either half alone prevents resumption — reverting only the
  newtype stays `Full`, because rustls then stores into `NoClientSessionStorage`; reverting
  both reads `Some(Resumed)` on the second dial. Userspace fallback (`with_offload(false)`):
  rustls reads the tickets itself and only `Resumption::disabled()` stands between them and a
  resumption — reverting it alone reads `Some(Resumed)`. So decision 3's redial test has
  **two arms**, kernel and userspace, and each reversal turns exactly one of them red.
- **`tickets_ignored()` needs one allocation at the handover.** ktls-core 0.0.5's `Context`
  owns the session and exposes no accessor back to it (`src/context.rs`, public methods
  `new`, `state`, `buffer`, `buffer_mut`, `refresh_traffic_keys`, `handle_io_error`,
  `shutdown`, `is_closed`), so the counter is an `Arc<AtomicU32>` shared between the newtype
  and the transport, allocated once when a client's keys go into the kernel — inside
  ADR-0005 decision 1's handshake carve-out, beside the `Box` of the `Context` and the
  control-record buffer taken at the same point. A server takes no counter and no allocation.

The exact counts were stable: five consecutive runs of the test binary, each asserting
`count == 4`, `largest == 184` on both sides, `count == 0` over the ticket window and
`tickets_ignored` moving by exactly 2 inside it. The newtype is `tls::side::Ticketless`,
public in a private module — reachable only as `<Client as Side>::Kernel` — and the one new
public item is `TlsTransport<Client>::tickets_ignored(&self) -> u32`.

## Revision — 2026-09-13, after the senior review of the branch (plan Sửa 8)

A dated note, not a change of substance: no decision above is reversed. Two questions the
review raised are about **what the numbers in decisions 2 and 3 are numbers of**, and the
answers narrow the text rather than the decision. Plan Sửa 8 holds the sources and the step
that builds the test below; it awaits the owner's approval as this note is written.

- **The count is two boxes per direction rekeyed, and "four" is the `update_requested` case.**
  ktls-core 0.0.5 updates the receive secret on every peer KeyUpdate and the transmit secret
  only when the peer sets `update_requested` (`src/context.rs:436-475`); each direction is one
  `refresh_traffic_secret`, which is one `derive_next` and one `expand_secret`, one box each
  (`rustls-0.23.44/src/tls13/key_schedule.rs:565-580, 808-814`; `src/conn/kernel.rs:126-149`).
  So a peer's KeyUpdate costs **four** boxes under `update_requested` — the case measured and
  asserted by `a_key_update_allocates_only_the_boxes_rustls_key_schedule_forces` — and **two**
  under `update_not_requested`, which no library in the test bench can send on demand (rustls
  and ktls-core both always ask for `update_requested`; a peer sends `update_not_requested`
  spontaneously only by choice — OpenSSL's `SSL_key_update(…, SSL_KEY_UPDATE_NOT_REQUESTED)`,
  the JDK when its inbound side is closed). **`[measured 2026-09-13]`, step 6c-3** — no longer
  derived from source: a peer whose own TLS is on the kernel writes the five bytes itself,
  `send_tls_control_message(fd, Handshake, [24, 0, 0, 1, 0])`, and both ends read
  `Window { count: 2, largest: 184, sizes: [184, 184] }` — acceptor and initiator, in
  `crates/engine/tests/tls_key_update.rs::a_key_update_without_update_requested_rekeys_one_direction_and_allocates_two_boxes`,
  on the §9 desk in desktop configuration, `7.0.0-31-generic`, rustls 0.23.44 with `ring`
  0.17.14. The engine does not answer: `TlsRxRekeyReceived` and `TlsRxRekeyOk` each move by one,
  and `TlsTxRekeyOk` moves **by one, not by zero** — `/proc/net/tls_stat` counts a network
  namespace rather than a socket, and in this test both ends are on the kernel, so the peer's
  own send-side rekey, which RFC 8446 §4.6.3 obliges the sender of any KeyUpdate to perform,
  lands in the same counter the engine's would. Flipping the request byte to `update_requested`
  and changing nothing else reds exactly that assertion, at a delta of two, before the count is
  ever reached — so the test tells the two request bytes apart and does not merely count
  allocations. There is no third number: any other request value is refused by ktls-core with
  an alert.
- **184 bytes is `ring`'s layout, not rustls's.** `RingHkdfExpander { alg, prk }`
  (`src/crypto/ring/tls13.rs:302-305`): an 8-byte `&'static` plus `hkdf::Prk(hmac::Key)`, and
  `hmac::Key` is two `digest::BlockContext`s of 88 bytes each — a `DynState` enum sized for
  SHA-512's eight `u64`s (72 with its tag), a `u64` byte count, an `&'static Algorithm`
  (`ring-0.17.14/src/hmac.rs:155-158`, `src/digest.rs:39-49`, `src/digest/dynstate.rs:23-26`).
  8 + 2 × 88 = 184, which accounts for the measured size byte for byte. rustls requires
  `ring = "0.17"` (caret), so a `ring` bump alone can move this constant.
- **The engine does not pin `rustls` or `ring` with `=`, and this is a decision, not an
  omission.** *Consequences* already said the numbers are the lock's; the reasons the caret
  stays: an `=` in a library manifest propagates to every consumer, and one 0.23.x per graph
  means a consumer needing any other 0.23.x cannot resolve (the Cargo book's own warning
  against upper bounds below the next incompatible version); two security fixes have already
  landed inside 0.23.x (RUSTSEC-2024-0336, patched 0.23.5; RUSTSEC-2024-0399, patched 0.23.18),
  each of which an `=` pin turns into a manual bump under a red advisory; and the pin buys
  nothing the committed `Cargo.lock` does not already give this repository's tests — cargo does
  not move a lock on its own. **The promise, stated for the right reader:** this engine and
  ktls-core allocate nothing while a peer's KeyUpdate is handled; rustls's key schedule
  allocates two boxes per direction it rekeys, a box the `Hkdf` trait signature forces on
  every provider. The count and the size belong to the rustls and ring this repository's
  `Cargo.lock` resolves — 0.23.44 and 0.17.14 — and are asserted exactly so that a bump is
  noticed; they are not a promise to a build with another lock.
- **The test binds its constants to the lock, and CI asserts the lock.** Step 6c-3 added two
  strings, `DERIVED_FROM_RUSTLS = "0.23.44"` and `DERIVED_FROM_RING = "0.17.14"`, read against
  the workspace `Cargo.lock` *after* the count and size assertions, so that a bump which leaves
  both numbers unchanged still turns the test red with a sentence naming the old and new
  versions and this note — otherwise the version in this text goes stale in silence, the
  failure §4 of `CLAUDE.md` names. Step 6c-4 adds `cargo metadata --locked` as the first step
  of CI's `fmt · clippy · test` job and `--locked` to the `tls` job's `cargo test` lines: not
  against drift, which cargo does not do unasked, but against a manifest edit committed without
  its lock, where the desk and CI would build two different graphs and neither would say so.

The *Bad* consequence *"Every rustls bump now owes a re-derivation of two constants"* therefore
reads *every rustls **or ring** bump*, and the constant it re-derives for size is read from
`ring`'s `hmac.rs`, not rustls's `key_schedule.rs`.

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
| **`Resumption::disabled()` alone** | Does not reach zero: the kernel-mode client still parses, derives and clones before the no-op store drops the value. *Unmeasured when proposed; `[measured 2026-09-13]` in 6c-2 it removes none of the sixteen — see Revision* |
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

## Revision — 2026-09-15, rustls 0.23.44 → 0.23.45

A dated note, not a change of substance. `Cargo.lock` moved rustls from 0.23.44 to 0.23.45
because advisory [RUSTSEC-2026-0285](https://rustsec.org/advisories/RUSTSEC-2026-0285) (TLS 1.3
handshake messages accepted across encryption-level boundaries) failed CI's `cargo deny` job
on PR #73; ring stays 0.17.14. `crates/engine/tests/tls_key_update.rs` re-measured
`RUSTLS_BOXES_PER_DIRECTION` and `RUSTLS_HKDF_EXPANDER_BOX` against the new lock and both
assertions held unchanged; only its `DERIVED_FROM_RUSTLS` string moved, as the test's last
guard requires. No decision above changes.
