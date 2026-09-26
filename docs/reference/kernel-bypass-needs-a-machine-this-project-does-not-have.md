# Kernel bypass needs a machine this project does not have — what that machine is, and what a rented one can do

`[researched 2026-09-26]` Kernel bypass (Onload over AF_XDP) was dropped from phase 4 at the probe's
gate G1 because the desk's Intel I211 (`igb`, `7.0.0-31-generic`) lacks `get_rxfh_key_size`
([ADR-0203](../decisions/ADR-0203-an-item-that-cannot-run-is-dropped-on-its-failing-gates-evidence-in-place-of-a-pair.md);
the trap itself is
[onload-af-xdp-needs-rss-key-ops-the-igb-driver-lacks](onload-af-xdp-needs-rss-key-ops-the-igb-driver-lacks.md)).
[ADR-0204](../decisions/ADR-0204-kernel-bypass-is-a-later-phase-candidate-that-reopens-on-a-named-machine.md)
records it as a candidate for a phase after 4, on rented cloud VMs, and
[ADR-0205](../decisions/ADR-0205-a-latency-figure-from-a-cloud-vm-is-published-under-its-own-label-beside-a-same-boot-kernel-twin-and-never-compared-with-the-desk.md)
decides how a figure from them is published. **This page is the one authored copy of the hardware
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

1. **An acceptor whose §9 rows can be set, or printed as absent.** On bare metal: root, the grub
   line (`isolcpus`, `rcu_nocbs`, `processor.max_cstate=1`), governor and C-state controls, IRQ
   affinity and `ethtool -C rx-usecs 0` on the bypass NIC — the knobs the Onload maintainer points
   at first (#83: *"`ethtool -C <intf> tx-usecs 0 rx-usecs 0` … adjusting interrupt affinity"*). On
   a VM, the rows that cannot be set are printed as absent, row by row (`DESIGN.md` §9 *A figure
   from a cloud VM*, ADR-0205).
2. **A counterparty on a separate machine** (`DESIGN.md` §8, *a load generator on a separate
   machine*). On hardware, a direct cable with no switch; on a cloud, a second VM in the same zone,
   whose path to the acceptor is the provider's fabric. A second port on the same host shares cores,
   caches and the quiet-machine row, so it is not a peer.
3. **A peer that does not drown the signal.** The Mac mini's counterparty p50 is **~232 µs** against
   the acceptor's 27–29 µs wire window (C-40); a 10 % counterparty-side gain is ~23 µs, 80–86 % of
   everything the acceptor does (ADR-0200 decision 4, which predicted DROP on that arithmetic). A
   Linux peer pinned and busy-polling is what gives the pair resolution.

## The chosen platform — 2026-09-26

The owner chose **two rented cloud VMs, a commodity NIC over AF_XDP, no purchase, no Mac**, and
accepted a VM figure as publishable (ADR-0204 decisions 2–5, ADR-0205). By the driver table, the
candidate is **GCP with `gve`**; AWS `ena` and Azure `mana` are out. The step-by-step procedure for
renting and gating the pair is [hft-playbook.md §8](../hft-playbook.md).

What `gve` adds to the table, read from its source and Google's driver
[README](https://github.com/GoogleCloudPlatform/compute-virtual-ethernet-linux):

- **n-tuple and the RSS key are device options, per platform.** `gve_adminq.c` sets `NETIF_F_NTUPLE`
  only when the virtual device reports `max_flow_rules > 0` (and logs `FLOW STEERING device option
  enabled with max rule limit of N`), and takes `rss_key_size` from the device's RSS option. The
  README: *"Support for flow steering varies by VM platform, so it is best to check for support
  before attempting to use the feature."* The gate (ADR-0204 decision 3) is therefore per machine
  series.
- **XDP needs half the queues**: *"the number of RX and TX queues must be no more than half their
  maximum values"* before an XDP program attaches — `ethtool -L` before Onload registers.
- **Zero-copy** is advertised on both queue formats, GQI-QPL and DQO-RDA
  (`gve_set_netdev_xdp_features`). The RX metadata timestamp (`gve_xdp_rx_timestamp`) is in the
  DQO path; **transmit hardware timestamps are refused** (`HWTSTAMP_TX_OFF` only), so the acceptor
  has no NIC-stamped wire window and the pair is the counterparty's round trip only.
- **Placement**: a compact placement policy *"places compute instances close to each other in a
  zone, which reduces network latency"* (C3, C4, N2 and others); sole-tenant nodes remove other
  tenants from the host but *"you can't apply placement policies to sole-tenant instances"*;
  threads per core = 1 disables SMT on most series
  ([placement](https://docs.cloud.google.com/compute/docs/instances/placement-policies-overview),
  [sole-tenancy](https://docs.cloud.google.com/compute/docs/nodes/sole-tenant-nodes),
  [threads per core](https://docs.cloud.google.com/compute/docs/instances/set-threads-per-core)).

**Other shapes, not chosen, kept for the record.** *Hardware minimum*: the desk plus one PCIe NIC
from the passing rows (`ixgbe`, `i40e`, `ice`, `mlx5`), with the Mac on a cable — inherits ADR-0200's
DROP prediction. *Hardware recommended*: two bare-metal hosts with `mlx5` (ConnectX-5/6) or `ice`
(E810), the rows with an RX stamp, on one DAC cable. *Native*: a pair of Solarflare cards (X2522 or
later), which ADR-0098 Q5 and the owner's answer 1 rule out.

## Can a rented VPS, cloud VM or bare-metal server do it?

Three uses, three verdicts.

| Use | Verdict | Why |
|---|---|---|
| **(i) Develop and functionally test** — install Onload, register a NIC, run the 59-definition corpus under `onload` (ADR-0200 decision 5), prove `zc:1` and the segment-counter check | **Yes, on a GCP `gve` pair that passes ADR-0204's gate**; on a rented bare-metal server with an `ixgbe`/`i40e`/`ice`/`mlx5` NIC too. GCP rests on one contributor's report (#337, 2026-09-21), so the gate decides. **AWS: no, `.metal` included** — `ena` has no n-tuple (#62, #337). **Azure: no** — `mana` has no n-tuple; the `mlx5` VF registered once in 2021 (#37), unconfirmed since. Generic VPS (`virtio_net`, `vmxnet3`): no |
| **(ii) CI** | **No Onload in CI; none is needed.** GitHub-hosted runners are VMs whose NIC nobody here chooses and whose loopback/`veth` Onload cannot register (no RSS ops). The item is *no engine code* (ADR-0098 item 2), so what CI must hold is the pure parts — a verdict script's fixture test, the machine-check verdict functions — which run anywhere. A self-hosted runner on a rented VM would be a secret-bearing machine outside this repository's control |
| **(iii) A publishable figure** under ADR-0099, non-negotiable 10, `DESIGN.md` §9 | **Yes, from the rented pair, under ADR-0205** (accepted by the owner 2026-09-26): its own table, a kernel-TCP twin from the same VM boot beside the bypass row, both arms from the counterparty VM, arms alternating run by run, and a label naming both instances, the driver, the Onload commit, `check-machine.sh` verbatim (its `GUEST` FAIL included) and every §9 row as applied / applied at vCPU level / absent / not applicable. **Never compared with the desk** (C-40, §8) or with another instance. What it cannot remove: host neighbours and fabric noise (measured by steal only), several §9 rows absent or not applicable, no NIC timestamps on `gve` |

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
Amazon [ENA release notes](https://github.com/amzn/amzn-drivers/blob/master/kernel/linux/ena/RELEASENOTES.md); Linux `drivers/net/ethernet/google/gve/{gve_adminq.c, gve_ethtool.c, gve_main.c, gve_rx_dqo.c}`
at `master`; the `gve` driver [README](https://github.com/GoogleCloudPlatform/compute-virtual-ethernet-linux);
Google Cloud [placement policies](https://docs.cloud.google.com/compute/docs/instances/placement-policies-overview),
[sole-tenancy](https://docs.cloud.google.com/compute/docs/nodes/sole-tenant-nodes),
[threads per core](https://docs.cloud.google.com/compute/docs/instances/set-threads-per-core).
**Searched, not verified:** which GCP machine series' `gve` device offers flow steering (Google names none); which NIC a given Hetzner or OVH dedicated offer carries; any published
Onload-on-gVNIC or -IDPF latency figure; whether Azure's `mlx5` VF still registers today.
