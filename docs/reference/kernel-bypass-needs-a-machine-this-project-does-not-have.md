# Kernel bypass needs a machine this project does not have — what that machine is, and what a rented one can do

`[researched 2026-09-26]` Kernel bypass (Onload over AF_XDP) was dropped from phase 4 at the probe's
gate G1 because the desk's Intel I211 (`igb`, `7.0.0-31-generic`) lacks `get_rxfh_key_size`
([ADR-0203](../decisions/ADR-0203-an-item-that-cannot-run-is-dropped-on-its-failing-gates-evidence-in-place-of-a-pair.md);
the trap itself is
[onload-af-xdp-needs-rss-key-ops-the-igb-driver-lacks](onload-af-xdp-needs-rss-key-ops-the-igb-driver-lacks.md)).
[ADR-0204](../decisions/ADR-0204-kernel-bypass-is-a-later-phase-candidate-that-reopens-on-a-named-machine.md)
records it as a candidate for a phase after 4. **This page is the one authored copy of the hardware
facts**: which NIC and which peer make the item runnable, and whether a rented VPS, cloud VM or
bare-metal server can stand in for them. Prices are deliberately not here: they go stale, and they
are not what decides the item.

## What Onload's AF_XDP path asks of a driver

Read from Onload `src/lib/efhw/af_xdp.c` on `master` (`51dbf861c3`, 2026-09-24):

| Need | Where in Onload | If missing |
|---|---|---|
| `ethtool_ops.get_rxfh_indir_size` **and** `get_rxfh_key_size` | `af_xdp_rss_get_support`, called by `af_xdp_nic_init_hardware` | `hardware init failed rc=-95` — the NIC never registers (the desk's failure) |
| `ethtool_ops.set_rxnfc` (n-tuple flow steering, `ethtool -K <nic> ntuple on`) | `af_xdp_filter_insert`, per bound socket | stack may build, but the socket's filter fails `-95` and traffic stays on the kernel; Onload issue [#62](https://github.com/Xilinx-CNS/onload/issues/62) (maintainer: *"ena does not support n-tuple filtering, which is required for Onload-over-AF_XDP"*), [#10](https://github.com/Xilinx-CNS/onload/issues/10) (82599 needs `ntuple on` first) |
| `set_rxfh` with RSS contexts, key length = `EFRM_RSS_KEY_LEN` | `af_xdp_rss_context_supported` | no `NIC_FLAG_RX_RSS`; not fatal for one stack on one queue |
| AF_XDP zero-copy in the driver (`ndo_xsk_wakeup`) | `EFHW_VI_RX_ZEROCOPY` → `XDP_ZEROCOPY` | copy mode only; ADR-0098 item 2's kill line requires zero-copy **bound** |
| `xdp_metadata_ops.xmo_rx_timestamp` | sets `NIC_FLAG_HW_RX_TIMESTAMPING` | no hardware RX stamp under Onload — ADR-0098 item 2's *Trap*; the pair must then be read from the counterparty only |
| A kernel inside Onload's range | [README](https://github.com/Xilinx-CNS/onload/blob/master/README.md): *"kernel.org Linux kernels 6.1 - 7.0"*; AF_XDP is *"a community-supported work in progress that is not currently at release quality"* | build failure, as attempt 1 on the desk |

## Which drivers pass

Read from `torvalds/linux` driver sources (`master` = `v7.3-rc4`, and the tags named), 2026-09-26.
"Key size" is `.get_rxfh_key_size`; "n-tuple" is `.set_rxnfc`; "ZC" is `ndo_xsk_wakeup`; "RX stamp"
is a reference to `xmo_rx_timestamp` in the driver.

| Driver (NICs) | Key size | n-tuple | ZC | RX stamp | Onload reports |
|---|---|---|---|---|---|
| `igb` (I210, **I211 — the desk**) | **only from `v7.3-rc1`** (`1ae67b2b28bc`; `git compare v7.2...1ae67b2b28bc` = diverged, `v7.3-rc1...` = behind) | yes | yes (6.14+) | no | dropped here (ADR-0201 *Result*) |
| `igc` (I225, I226) | **only from `v7.3-rc1`** (`f243be8edeab`, 2026-07-01; absent at `v7.0`, `v7.1`, `v7.2`) | yes | yes | yes | — |
| `ixgbe` (82599, X520, X540, X550) | yes | yes | yes | no | #10: works once `ntuple on` |
| `i40e` (X710, XL710) | yes | yes | yes | no | — |
| `ice` (E810) | yes | yes | yes | yes | — |
| `mlx5` (ConnectX-4 Lx, -5, -6) | yes | yes | yes | yes | [#83](https://github.com/Xilinx-CNS/onload/issues/83): ConnectX-5, bare metal, works when run as root; `tcp_pingpong` half-RTT median **11.04 µs kernel → 8.76 µs Onload**, 99 % 12.09 → 11.10 (2022, 1-byte, the reporter's own machine — someone else's claim, not this project's) |
| `ena` (AWS, every instance incl. `.metal`) | yes | **no** | not in-tree; Amazon's out-of-tree driver ≥ 2.13 | — | #62 (n-tuple missing), [#337](https://github.com/Xilinx-CNS/onload/issues/337) (open, 2026-08-06: registers, stack allocation fails `rc=-95`); [#139](https://github.com/Xilinx-CNS/onload/issues/139): where it ran on AWS, latency was worse than the kernel |
| `gve` (GCP gVNIC, VMs) | yes | yes | yes | yes | #337, 2026-09-21, an AF_XDP-code contributor (not the maintainer): *"it works on latest IDPF driver and GVE driver"* — **not reproduced here** |
| `idpf` (GCP bare-metal: C3 / X4 metal) | yes | yes | yes | yes | same #337 comment |
| `mana` (Azure) | yes | **no** | no | — | — |
| `mlx5` VF (Azure accelerated networking) | the `mlx5` row's ops; what a VF may use is not verified | | | | [#37](https://github.com/Xilinx-CNS/onload/issues/37): ConnectX-4 Lx VF on Azure registered only after `ethtool -K <vf> lro off` (2021) |
| `veth`, `vmxnet3`, `virtio_net` | `veth`: no RSS ops at all; `vmxnet3`: no key size ([#257](https://github.com/Xilinx-CNS/onload/issues/257)); `virtio_net`: key size yes, n-tuple no | | | | #257, [#270](https://github.com/Xilinx-CNS/onload/issues/270) |

**Native Onload on an AMD Solarflare NIC** (README lists SFN8522/8542/8042, X2522, X2522-25G, X2541,
X3522; AMD sells the X4 series, [X4 page](https://www.amd.com/en/products/ethernet-adapters/solarflare-x4.html))
needs none of the table above: Onload drives the card through `ef_vi`, at release quality, with
CTPIO transmit and hardware timestamps. What it changes: the bypass stops being "any Linux NIC" and
becomes one vendor's card; ADR-0098 Q5 (**no `ef_vi` card in phase 4**) and `PRD.md` §5 (*`ef_vi` …
only once a Solarflare / X2-class NIC exists here*) are what a purchase would move; the engine
still runs unchanged under `onload` (ADR-0099 decision 3's order: Onload first, `ef_vi` as a
`Transport` second).

**The zero-cost path is the desk itself, later**: `igb` has every operation from `v7.3-rc1`, but
Onload's README stops at 7.0 — both have to move. G0 (`ethtool -x <nic>` prints a key) tells
which side is still missing.

## What the measurement needs beyond the NIC

ADR-0099 decision 2 publishes a bypass figure only beside a kernel-TCP figure from **the same boot**;
non-negotiable 10 needs the committed benchmark, the machine and the §9 settings; ADR-0200 judges
both arms from the counterparty.

1. **The acceptor host is not a guest** (`DESIGN.md` §9, first row: *"Development may move to a
   cloud VM; measurement cannot"*; `check-machine.sh` FAILs a guest). It needs root, the grub line
   (`isolcpus`, `rcu_nocbs`, `processor.max_cstate=1`), the governor and C-state controls, IRQ
   affinity and `ethtool -C rx-usecs 0` on the bypass NIC — the knobs the Onload maintainer points
   at first (#83: *"`ethtool -C <intf> tx-usecs 0 rx-usecs 0` … adjusting interrupt affinity"*).
2. **A counterparty on a separate machine** (`DESIGN.md` §8, *a load generator on a separate
   machine*), on a **direct cable, no switch**: a switch adds a hop, a queue and a second device's
   jitter to the only instrument ADR-0200 trusts. A second port on the same host shares cores,
   caches and the quiet-machine row, so it is not a peer.
3. **A peer that does not drown the signal.** The Mac mini's counterparty p50 is **~232 µs** against
   the acceptor's 27–29 µs wire window (C-40); a 10 % counterparty-side gain is ~23 µs, 80–86 % of
   everything the acceptor does (ADR-0200 decision 4, which predicted DROP on that arithmetic). A
   Linux peer with the same class of NIC, pinned and busy-polling, is what gives the pair
   resolution. Changing the instrument is the reopening plan's decision, not this page's.

**Minimum** (runs the item, keeps ADR-0200's design as written): the desk, on its §9 line, plus one
PCIe NIC from the passing rows (`ixgbe`, `i40e`, `ice` or `mlx5`) on a kernel inside Onload's range,
G0 green, `ntuple on`; the Mac mini on a direct cable at a speed both ends speak. It inherits
ADR-0200's DROP prediction. Unverified: whether the desk's mini-ITX board has its one PCIe slot free.

**Recommended** (gives the pair resolution and a hardware stamp under Onload): two Linux hosts on
bare metal, each with the same passing NIC — `mlx5` (ConnectX-5/6) or `ice` (E810), the two rows
with an RX stamp — on one direct cable (DAC for SFP ports), both on the §9 line, the peer running the
generator busy-polling.

**Native alternative**: a pair of Solarflare cards (X2522 or later) in the same two-host shape; needs
the owner to reverse ADR-0098 Q5 for that phase.

## Can a rented VPS, cloud VM or bare-metal server do it?

Three uses, three verdicts.

| Use | Verdict | Why |
|---|---|---|
| **(i) Develop and functionally test** — install Onload, register a NIC, run the 59-definition corpus under `onload` (ADR-0200 decision 5), prove `zc:1` and the segment-counter check | **Yes on a machine with a passing driver, which rules out most VMs.** GCP (gVNIC VMs, IDPF bare metal) is the candidate, on one contributor's word (#337, 2026-09-21) — reproduce before relying on it. A rented bare-metal server with an `ixgbe`/`i40e`/`ice`/`mlx5` NIC works by the table; the NIC model varies per offer, so `ethtool -i` and G0 are read before committing. **AWS: no, `.metal` included** — the NIC is `ena`, which has no n-tuple (#62, #337). **Azure: `mana`, no**; the `mlx5` VF registered once in 2021 (#37), unconfirmed since. Generic VPS (`virtio_net`, `vmxnet3`): no |
| **(ii) CI** | **No Onload in CI; none is needed.** GitHub-hosted runners are VMs whose NIC nobody here chooses and whose loopback/`veth` Onload cannot register (no RSS ops). The item is *no engine code* (ADR-0098 item 2), so what CI must hold is the pure parts — a verdict script's fixture test, the machine-check verdict functions — which run anywhere. An Onload job would need a self-hosted runner on hardware from use (i), and a runner on a rented box is a secret-bearing machine outside this repository's control |
| **(iii) A publishable figure** under ADR-0099, non-negotiable 10, `DESIGN.md` §9 | **Never from a VM**: a guest is a §9 FAIL, a virtualised NIC is not the NIC the row names, steal time and noisy neighbours are not controllable. **A dedicated bare-metal rental is not excluded by the rules** if `check-machine.sh` passes on it, both arms run on one boot, and the row names the machine, the NIC and driver, the Onload version, the XDP mode read back, and the path to the peer. Its limits: the peer sits behind the provider's switch fabric, not a direct cable, and that path is shared with other tenants; a figure from it is a different instrument from the desk's C-40 and is not comparable to it. AWS `.metal` still fails at the NIC |

## Sources

Read 2026-09-26: Onload
[README](https://github.com/Xilinx-CNS/onload/blob/master/README.md) and `src/lib/efhw/af_xdp.c`
(`master`, `51dbf861c3`); Onload issues
[#10](https://github.com/Xilinx-CNS/onload/issues/10),
[#37](https://github.com/Xilinx-CNS/onload/issues/37),
[#62](https://github.com/Xilinx-CNS/onload/issues/62),
[#83](https://github.com/Xilinx-CNS/onload/issues/83),
[#139](https://github.com/Xilinx-CNS/onload/issues/139),
[#257](https://github.com/Xilinx-CNS/onload/issues/257),
[#270](https://github.com/Xilinx-CNS/onload/issues/270),
[#337](https://github.com/Xilinx-CNS/onload/issues/337) with its comments; Linux
`drivers/net/ethernet/{intel/{igb,igc,ixgbe,i40e,ice,idpf},mellanox/mlx5/core,amazon/ena,google/gve,microsoft/mana}`
and `drivers/net/{veth,virtio_net}.c` at `master`, `v7.0`, `v6.18`, `v6.8` (and `igc` at `v7.1`,
`v7.2`, `v7.3-rc1`); Linux commits `1ae67b2b28bc`, `f243be8edeab`; Google Cloud
[IDPF](https://docs.cloud.google.com/compute/docs/networking/using-idpf) and
[bare-metal instances](https://docs.cloud.google.com/compute/docs/instances/bare-metal-instances);
[Azure accelerated networking](https://learn.microsoft.com/en-us/azure/virtual-network/accelerated-networking-how-it-works);
[AWS user-provided kernels](https://docs.aws.amazon.com/AWSEC2/latest/UserGuide/UserProvidedKernels.html);
Amazon [ENA release notes](https://github.com/amzn/amzn-drivers/blob/master/kernel/linux/ena/RELEASENOTES.md).
**Searched, not verified:** which NIC a given Hetzner or OVH dedicated offer carries; any published
Onload-on-gVNIC or -IDPF latency figure; whether Azure's `mlx5` VF still registers today.
