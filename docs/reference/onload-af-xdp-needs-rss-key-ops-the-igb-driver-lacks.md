# Onload over AF_XDP needs RSS key ops the `igb` driver lacks

`[cost 2026-09-24]` Kernel bypass (phase 4 item 4, ADR-0098) was dropped on this desk before any
AF_XDP stack came up. The cause is one missing `ethtool` operation, and it is checkable in one
command before anything is built.

## The trap

Onload's AF_XDP path calls `af_xdp_rss_get_support` (`src/lib/efhw/af_xdp.c`) during hardware
registration. That function returns `-EOPNOTSUPP` unless the NIC driver implements **both**
`get_rxfh_indir_size` and `get_rxfh_key_size`. `af_xdp_nic_init_hardware` propagates that as a
hard failure — nothing loads, nothing is left half-registered, but the whole item is dead before
a single frame moves.

The desk's NIC is an Intel I211 on the `igb` driver, `enp9s0`, kernel `7.0.0-31-generic`. `igb`
answers the indirection-table query and refuses the key query — an asymmetry, not an absent RSS
feature: `ethtool -l enp9s0` and `ethtool -x enp9s0`'s indirection table both work, only the key
read fails. Linux added `get_rxfh_key_size` to `igb` in `dfaf57ef99cf` / `1ae67b2b28bc` / `e3c94e9782a7`
("igb: expose RSS key via ethtool get_rxfh"), first shipped in **`v7.3-rc1`** — this desk's
`7.0.0-31` predates it. This holds on every Onload version checked: `v9.0.2`, the `v9_2` branch
pin `174b947d0b9b7b77439463afbabf8a7e417b3706`, and `master` — the function is unchanged across
all three, so a newer Onload does not fix an older kernel.

## The evidence

Two attempts on this desk, both recorded in full in
[ADR-0201](../decisions/ADR-0201-onload-on-the-i211-runs-without-hardware-flow-filters-one-channel-count-holds-for-the-boot-and-the-control-path-leaves-the-cable.md)
*Result*, and summarised in [measured-costs.md](measured-costs.md), *Onload over AF_XDP:
dropped at G1, no latency pair exists*:

- `ethtool -x enp9s0` read **before either attempt**: `RSS hash key: Operation not supported`.
  This alone predicts the outcome below; nothing about the two attempts adds information the
  `ethtool` read did not already have.
- Attempt 1 (`v9.0.2`) never reached registration — it failed to build on this kernel
  (`sockaddr_unsized`, unrelated to RSS) and is not evidence about the RSS question either way.
- Attempt 2 (`v9_2` pin) built, installed, loaded, and failed registering `enp9s0`:
  `` af_xdp_rss_get_support: enp9s0 does not support `get_rxfh_key_size` operation `` then
  `[sfc efrm] ?: ERROR: hardware init failed rc=-95` — identically with default filters, with
  `enable_af_xdp_flow_filters=0`, and with `ethtool -L enp9s0 combined 1`. Three variants, one
  cause, confirmed by reading the source rather than guessed from the retries.

Searched Onload's issue tracker for `get_rxfh_key_size` and found nothing; the closest
"hardware init failed" reports are on virtual NICs (vmxnet3 #257, virtio-net #270), a different
cause (no RSS key concept at all on a virtual device).

## Reopen condition

This is dropped for **this NIC on this kernel**, not for kernel bypass in general. It reopens
when both of these hold:

1. **A measurement NIC whose driver implements both RSS key operations, on a kernel that has
   them** — for example `igb` itself, on a kernel **≥ 7.3** (where `get_rxfh_key_size` shipped).
   AF_XDP zero-copy support on that driver is also required; Onload's README lists kernels only
   up to 7.0 as tested, so the Onload side of the pin would need re-checking too.
2. **A rebuilt Onload against that kernel** — the `sockaddr_unsized` build fix from attempt 1
   (`268f1d4c8a`) is a separate, already-solved precondition; keep the `git merge-base
   --is-ancestor` guard when re-pinning.

Reopening is a new plan; ADR-0200 already holds the measurement design (generator-side pair
against a same-boot kernel twin) it would reuse.

## The guard that watches it

**The check costs nothing and runs before any build:**

```
ethtool -x <nic> | grep -A1 'RSS hash key'
```

A driver that cannot do this prints `Operation not supported` and Onload's AF_XDP path is dead
on arrival for it — do not attempt registration. A driver that prints an actual key is the
pre-check this item's reopen condition needs.

This is exactly the plan's own G1 probe (step 6.1 of
[plans/2026-09-24-p4-bypass-and-s9-boot.md](../plans/2026-09-24-p4-bypass-and-s9-boot.md)): it
reads `ethtool -x <nic>` before building anything, and treats `Operation not supported` there as
a gate failure rather than proceeding to install. Any future attempt at this item, on any NIC,
runs that same probe first.
