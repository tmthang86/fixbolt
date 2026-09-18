# ADR-0070 — kTLS stays the `hft` steady state for the guarantee, not for latency

- **Status**: **Accepted — 2026-09-18**, by the plan [closing-the-open-items](../plans/2026-09-18-closing-the-open-items.md), approved by the manager under the owner's 2026-09-18 mandate. Proposed the same day.
- **Date**: 2026-09-18
- **Deciders**: Tran Manh Thang
- **Related**: [ADR-0005](ADR-0005-tls.md) decision 2 (amended, not reversed),
  [ADR-0063](ADR-0063-a-peers-key-update-is-the-second-named-carve-out-and-a-ticket-is-not-read.md),
  [ADR-0068](ADR-0068-a-published-figure-is-two-procedures-shown-side-by-side.md),
  `DESIGN.md` §8 *The round trip under TLS, measured*, `docs/best-practices-hft.md` §9,
  `STATUS.md` open item 84
- **Answers**: item 84's design question — *does decision 2 still hold for `hft`; what would a
  NIC with TLS offload change; does the sidecar row of ADR-0005 survive the measured deltas*

## Context

ADR-0005 decision 2 chose kTLS as the `hft` steady state for four reasons: D8 preserved (no
blocking call enters the loop), parse-in-place preserved (no userspace copy), crypto on AES-NI
with NIC offload where the hardware has it, and the hot-path allocation guarantee met. Latency
was not one of the four, and the ADR said the number was literature until measured.

`[measured 2026-09-14]` on the §9 desk, loopback, TLS 1.3 `TLS13_AES_128_GCM_SHA256` at both
ends, two procedures (`DESIGN.md` §8): kTLS added **+9.0 to +9.5 µs** at p50 over plain TCP
in both `hft` paths; userspace `rustls` added **+1.4 to +4.7 µs**. kTLS was slower at p99 and
p99.9 too. The kTLS arm counted **0** allocations in the window; the userspace arm counted
**4 per round trip**. Nothing was isolated: both ends carried the same mode, so the figure is
two kTLS stacks or two `rustls` stacks, never one engine's cost.

What the search found, 2026-09-18:

- The kernel's own documentation says software kTLS runs the crypto synchronously on the
  calling CPU in the common case and that *"the use of accelerators introduces extra latency
  on socket reads (decryption only starts when a read syscall is made)"* —
  <https://docs.kernel.org/networking/tls-offload.html>. RX decryption on the software path
  happens **inside `recvmsg`**, after the whole record is in the receive queue: the receiver
  pays the record's AES-GCM at the moment it asks for the bytes, on the engine core, instead of
  having it done while the bytes were in flight. `rustls` does exactly the same work in
  userspace, so the difference is not *where* the crypto runs but the kernel's per-record
  machinery around it (strparser, skb cloning, the crypto API's request objects).
- Jakub Kicinski's account of the 5.20 RX rewrite records that the software RX path used to
  copy and CoW whole receive-queue skbs to decrypt, and that after three fixes it became
  *"comparable with the user space OpenSSL"* — comparable, not faster —
  <https://people.kernel.org/kuba/tls-1-3-rx-improvements-in-linux-5-20>.
- The case for kTLS in the literature is throughput and `sendfile`/zero-copy at 100 Gb/s, not
  the latency of a 200-byte record; no source found claims a latency win for small records in
  software mode. Search for a small-record latency comparison on a recent kernel found nothing
  beyond this repository's own PR #71.
- Hardware TLS offload (`tls-hw-tx-offload`) exists on Mellanox ConnectX-6 Dx and later and on
  some Chelsio parts; on those NICs the record is encrypted by the NIC on the way out and the
  software record layer is bypassed. Neither the §9 desk (I211) nor loopback has it (`ethtool
  -k` reads `off [fixed]`, 2026-09-14).

## Decision

1. **Decision 2 of ADR-0005 stands, with its reason restated: kTLS is the `hft` steady state
   because it is the only path that meets non-negotiable 1 and keeps parse-in-place**, and for
   no other reason. The sentence in ADR-0005 that reads *"the only one that meets the hot-path
   guarantee"* is the whole justification; the third reason's "AES-NI with NIC offload" is
   struck as a latency argument until a NIC with offload has been measured.
2. **Latency is now a documented cost of that choice, not a benefit.** `docs/best-practices-hft.md`
   §9 says, in numbers, that on a kernel without TLS offload kTLS costs ~9 µs a round trip at
   p50 against ~1.4–4.7 µs for userspace, both ends included, and that a deployment which
   prefers latency to the allocation guarantee may run `hft` over userspace `rustls` **only if
   it accepts four heap allocations per round trip on the engine thread** — that is the number
   `tools/w2w` counted, and the deployment is outside the hot-path guarantee in those words.
   `TlsRequireKernel=Y` stays the recommendation; the alternative is named, priced and marked
   as off the guarantee.
3. **The sidecar row of ADR-0005's *Alternatives* is not reopened.** It priced a loopback hop
   at the same order as the whole kernel-TCP floor (10–20 µs); a measured kTLS delta of 9 µs
   does not make a sidecar cheaper, it makes both expensive. Nothing changes there.
4. **What would reverse decision 2, named so the next measurement is the right one**: one
   procedure pair on a NIC with `tls-hw-tx-offload: on`, `hft`, both paths, kTLS versus
   userspace, per ADR-0068. If kTLS-with-offload is not within 5% of userspace at p50, p99 and
   p99.9, decision 2 is superseded by a new ADR that makes userspace the `hft` steady state
   and widens the non-negotiable 1 carve-out to the record layer. Until such a NIC exists here
   this is a stated condition, not a plan.
5. **The engine's own share is measured, and it is small.** `[measured 2026-09-18, boot C
   step C-84]` §9 desktop, `pass 16 fail 0 unknown 0`, commit `85460c1`'s code, loopback,
   `hft` admin, 10 runs × 20 000, two procedures in opposite order, every arm reproduced (max
   2.9% at p99.9); `tools/w2w` ran at listener cadence N = 1 (its own default, which did not
   follow `Limits`' 16 — ADR-0069 *Measured*), so the `off` reference is boot C's N = 1 arm,
   16 070 ‖ 16 080 ns. p50 ns, procedure 1 ‖ 2: kTLS/kTLS 25 503 ‖ 25 473 (+9.4 µs);
   userspace/userspace 20 824 ‖ 20 719 (+4.7 µs); **engine kTLS + client userspace 21 175 ‖
   21 250 (+5.1 µs)**; engine userspace + client kTLS 22 177 ‖ 22 147 (+6.1 µs). So, with the same userspace client, **the
   engine's kTLS costs 351 ‖ 531 ns more than its userspace `rustls`** — about 0.4 µs, ~2% of
   the round trip; the client's kTLS costs the client 1 353 ‖ 1 428 ns; and both-kTLS is
   **superadditive** by ~2.8–3.0 µs over the sum of the parts (a candidate, not a cause: two
   kernel record layers that each decrypt only on a complete record, serialising where two
   userspace layers pipeline). The 9 µs of decision 2's documentation is therefore mostly the
   *counterparty's* kTLS and the interaction, not this engine's. Decision 2's text stands; its
   price is restated in `best-practices-hft.md` §9 with the engine's share separated. The mixed
   arms print their allocations (4 001 on the client thread, the client's `rustls`) and do not
   assert them; the symmetric arms still do.

## Consequences

**Good**

- Item 84 closes as a decision with its condition for reversal written down, instead of
  staying a question every reader re-derives from the table.
- A reader of `best-practices-hft.md` gets a price for each choice and the rule that makes
  one of them off the guarantee — a sentence a reviewer can check.
- The next TLS measurement is defined before the hardware exists, so a NIC purchase can be
  judged against it.

**Bad — and accepted**

- **The recommended mode is the slower one on every machine this project owns** — by ~0.4 µs
  of the engine's own doing (decision 5), and by ~5–7 µs a round trip when the counterparty is
  kTLS too. A user who benchmarks `hft` under TLS against a kTLS peer will see the larger
  number; the document says which part is this engine's.
- **"Four allocations per round trip" is one measurement on one build.** The count may change
  with `rustls` versions; the guard is `tools/w2w`'s allocation counter on the TLS arms, which
  only the §9 procedure runs.
- **Decision 4 names a NIC nobody here has.** The condition can stay unmet indefinitely, and
  during that time the design carries a preference it cannot show a latency benefit for.
- **The mixed arms (decision 5) measured loopback**, so the engine's 0.4 µs is a software-kTLS
  figure and says nothing about offload; and the superadditive ~3 µs is a candidate mechanism
  nobody has isolated.

## Sources

- `DESIGN.md` §8 *The round trip under TLS, measured*; `docs/reference/measured-costs.md`
  *TLS on the wire* `[measured 2026-09-14]`; PR #71.
- Kernel TLS offload documentation — <https://docs.kernel.org/networking/tls-offload.html>
  (read 2026-09-18): sync crypto in the common case; accelerator latency on reads.
- J. Kicinski, *TLS 1.3 Rx improvements in Linux 5.20* —
  <https://people.kernel.org/kuba/tls-1-3-rx-improvements-in-linux-5-20> (read 2026-09-18).
- `ethtool -k lo` on the §9 desk, `tls-hw-tx-offload: off [fixed]`, read 2026-09-14.
