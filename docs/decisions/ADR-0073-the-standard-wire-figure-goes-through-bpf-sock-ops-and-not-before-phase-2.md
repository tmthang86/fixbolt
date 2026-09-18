# ADR-0073 — `standard`'s wire figure goes through BPF sock_ops timestamping, and not before phase 2

- **Status**: **Accepted — 2026-09-18**, by the plan [closing-the-open-items](../plans/2026-09-18-closing-the-open-items.md), approved by the manager under the owner's 2026-09-18 mandate. Proposed the same day.
- **Date**: 2026-09-18
- **Deciders**: Tran Manh Thang
- **Related**: [ADR-0071](ADR-0071-a-skipped-tx-stamp-is-a-missing-sample-not-a-failed-run.md),
  [reference/a-transmit-timestamp-wakes-a-blocking-engine.md](../reference/a-transmit-timestamp-wakes-a-blocking-engine.md),
  [plans/2026-09-04-the-second-linux-desk.md](../plans/2026-09-04-the-second-linux-desk.md)
  Sửa 2 Điều 3 (options S1–S5), `DESIGN.md` §6 NIC row (*mode `hft` only*),
  `STATUS.md` open item 87

## Context

A TX timestamp requested on a socket lands on that socket's error queue, `poll(2)` reports
`POLLERR` whenever the queue is non-empty, and a `standard` engine asleep in `poll` therefore
spins `poll → recv EAGAIN` until the observer drains the queue. So `tools/w2w` refuses
`--mode standard --wire-timestamps` on a real NIC, and `DESIGN.md` §6's NIC row is `hft` only.

Sửa 2 Điều 3 named the way round it — BPF sock_ops timestamping — and left one thing
unverified: *"that a BPF-only request queues nothing on the error queue is inferred from how
the flags are split, not verified; read `__skb_tstamp_tx` first"*.

**Read on 2026-09-18, `net/core/skbuff.c` at tag `v6.16`** (the desk runs `7.0.0-31`; the
code was introduced in 6.15 and not reworked since):

```c
void __skb_tstamp_tx(struct sk_buff *orig_skb, const struct sk_buff *ack_skb,
		     struct skb_shared_hwtstamps *hwtstamps, struct sock *sk, int tstype)
{
	...
	if (skb_shinfo(orig_skb)->tx_flags & SKBTX_BPF)
		skb_tstamp_tx_report_bpf_timestamping(orig_skb, hwtstamps, sk, tstype);

	if (!skb_tstamp_tx_report_so_timestamping(orig_skb, hwtstamps, tstype))
		return;
	... /* everything below queues on sk_error_queue */
}

static bool skb_tstamp_tx_report_so_timestamping(struct sk_buff *skb,
		struct skb_shared_hwtstamps *hwtstamps, int tstype)
{
	switch (tstype) {
	case SCM_TSTAMP_SND:
		return skb_shinfo(skb)->tx_flags & (hwtstamps ? SKBTX_HW_TSTAMP_NOBPF :
						    SKBTX_SW_TSTAMP);
	...
```

and `include/linux/skbuff.h`: `#define SKBTX_HW_TSTAMP (SKBTX_HW_TSTAMP_NOBPF | SKBTX_BPF)`.
So a hardware stamp on an skb whose only flag is `SKBTX_BPF` reaches the BPF callback
(`BPF_SOCK_OPS_TSTAMP_SND_HW_CB`) and **returns before anything is queued on the error queue**.
The inference is now a read line of code. The driver still stamps such an skb, because
`igb` tests `SKBTX_HW_TSTAMP`, which includes `SKBTX_BPF`. The single-slot limit of ADR-0071
applies unchanged.

What it needs: a `BPF_PROG_TYPE_SOCK_OPS` program attached to the engine's cgroup that calls
`bpf_sock_ops_enable_tx_tstamp` (or sets `SK_BPF_CB_TX_TIMESTAMPING`) for the engine's socket
and forwards the hardware stamp plus the TCP byte key to a ring buffer the observer reads;
`CAP_BPF` and `CAP_NET_ADMIN` on the loader; a Rust BPF loader (a new dependency); and a way
to compile the program.

## Decision

1. **BPF sock_ops timestamping is the route for a `standard` wire figure.** Options S2
   (the engine reads its own error queue) and S4/S5 are rejected as Sửa 2 argued: S2 measures
   a different engine; S4 is hardware this project does not have; S5 is silence.
2. **It is not built in phase 1's closing plan.** The `standard` NIC row stays *`hft` only,
   generator's table for `standard`*, with this ADR as the reason. Cost 2–3 days of build plus
   a boot; benefit one row of one table for the mode whose users, by ADR-0013, are not the
   ones buying wire figures. It is scheduled as the first tooling step of phase 2's first
   §9 boot, after the encoding ADR, when the desk is booked for a full day anyway.
3. **The shape is fixed now so the later plan is briefable**: the program lives in
   `tools/w2w-bpf/` as C compiled with `clang -target bpf` by
   `scripts/build-bpf-tstamp.sh` (the object is gitignored, like `vendor/`); the loader is
   `aya` (pure Rust, no libbpf C link, the line ADR-0001 and ADR-0005 drew) behind a
   `bpf-tstamp` feature of `tools/w2w` that is **off by default and not built by CI**
   (`CLAUDE.md` §2 rule 6 by analogy — `tools/` is not a library crate, but the same rule keeps
   a clang out of every contributor's build). The dependency gets its own ADR when the plan
   is written, per `CLAUDE.md` §6.
4. **Pairing stays by TCP byte offset**: the BPF callback sees `skb_shinfo(skb)->tskey`
   (`sk_tskey_bpf_offset`), the same key `OPT_ID_TCP` gives the observer today, so the pairing
   code in `tools/w2w` does not change.

## Consequences

**Good**

- Item 87 closes as a decision whose one open precondition is now verified against the
  kernel source and quoted, so the future plan does not re-derive it.
- `standard` will eventually be measured as `standard` — the engine's socket is never
  touched, and the engine never sees `POLLERR`.
- The shape decided here keeps C toolchains and root out of the default build.

**Bad — and accepted**

- **`DESIGN.md` §6's NIC row stays half-met for the rest of phase 1**, and a reader who wants
  a `standard` wire number gets the counterparty's software clock instead. The row says so.
- **`aya` and a BPF object are a new class of dependency** — a program that runs in the
  kernel, loaded with `CAP_BPF`, on the measurement box only. It is measurement tooling, not
  the engine; but it is still code this repository ships and must keep building against
  kernel headers that move.
- **`clang` enters the desk's toolchain** for the object, even if not CI's.
- **The verification is of `v6.16`, not of the desk's `7.0`.** The plan that builds this must
  re-read `__skb_tstamp_tx` at the desk kernel's tag before relying on it; a moved line is a
  reason to stop, not to proceed.

## Sources

- `net/core/skbuff.c` at `v6.16`: `__skb_tstamp_tx`, `skb_tstamp_tx_report_so_timestamping`,
  `skb_tstamp_tx_report_bpf_timestamping` — fetched from
  <https://github.com/torvalds/linux> on 2026-09-18 and quoted above.
- `include/linux/skbuff.h` at `v6.16`: the `SKBTX_*` enum and `SKBTX_HW_TSTAMP` (same fetch).
- J. Xing, *net-timestamp: bpf extension to equip applications transparently* (v6.15) —
  <https://lists.openwall.net/netdev/2025/01/21/18>; *bpf: Support selective sampling for bpf
  timestamping* —
  <https://github.com/torvalds/linux/commit/59422464266f8baa091edcb3779f0955a21abf00>.
- eBPF docs, `BPF_PROG_TYPE_SOCK_OPS` —
  <https://docs.ebpf.io/linux/program-type/BPF_PROG_TYPE_SOCK_OPS/> (callbacks
  `BPF_SOCK_OPS_TSTAMP_SND_HW_CB` and siblings listed; read 2026-09-18).
- [a-transmit-timestamp-wakes-a-blocking-engine.md](../reference/a-transmit-timestamp-wakes-a-blocking-engine.md)
  `[2026-09-14]`.
