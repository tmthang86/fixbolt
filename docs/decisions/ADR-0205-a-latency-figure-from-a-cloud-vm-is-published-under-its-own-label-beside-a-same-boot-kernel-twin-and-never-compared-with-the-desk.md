# ADR-0205 — A latency figure from a cloud VM is published under its own label, beside a kernel-TCP twin from the same VM boot, and never compared with the desk

- **Status**: **Accepted — 2026-09-26, by the owner** (in conversation, answering ADR-0204 decision 5:
  *"thuê — chấp nhận con số đo ở VPS, nếu cần thì viết lại ADR"*, i.e. rent, and a figure measured on
  a rented VM is accepted as publishable). It **supersedes, for the figures in its scope, the
  sentence of `DESIGN.md` §9's first row *"Development may move to a cloud VM; measurement cannot"***
  — that row's facts (governor, turbo, C-states, SMT and host IRQ affinity are host properties a
  guest cannot set) stand. It supersedes no ADR: that row was never an ADR's decision.
- **Date**: 2026-09-26
- **Deciders**: Tran Manh Thang. Written by the architect (Opus).
- **Related**: [ADR-0204](ADR-0204-kernel-bypass-is-a-later-phase-candidate-that-reopens-on-a-named-machine.md)
  (the item whose figures this governs);
  [ADR-0099](ADR-0099-kernel-tcp-stays-the-default-and-the-headline-and-a-bypass-figure-is-a-second-labelled-row.md)
  decisions 1 and 2; [ADR-0068](ADR-0068-a-published-figure-is-two-procedures-shown-side-by-side.md);
  [ADR-0200](ADR-0200-a-bypass-arm-is-judged-from-the-counterparty-against-a-same-boot-kernel-twin-and-its-kill-line-is-arithmetic-written-first.md)
  (Deprecated); `DESIGN.md` §8, §9 *A figure from a cloud VM*; `CLAUDE.md` §2 non-negotiable 10;
  `scripts/check-machine.sh` `virt_verdict`;
  [kernel-bypass-needs-a-machine-this-project-does-not-have](../reference/kernel-bypass-needs-a-machine-this-project-does-not-have.md).

## Context

The owner chose to take kernel bypass forward on rented cloud VMs, with a second rented VM as the
counterparty, and accepted a figure measured there as publishable (ADR-0204, revised). Until now
`DESIGN.md` §9 said a guest cannot measure, and `check-machine.sh` FAILs a guest with the words
*"no latency figure from it is publishable"*.

The reason for that rule is still true. Inside a VM the host's governor, turbo, C-states, SMT and the
physical NIC's interrupts are not visible and not settable; other tenants share the host and the
network fabric; the vCPU a guest pins is not a physical core it owns. What changes is not the facts
but what they forbid: a guest figure is allowed, provided nobody can mistake it for a desk figure and
every §9 row it could not meet is printed as absent.

Research (2026-09-26, sources on the reference page and below):

- GCP's `gve` driver exposes n-tuple steering and RSS configuration **only when the virtual device
  offers them** (`gve_adminq.c`: `NETIF_F_NTUPLE` set only when the device's flow-steering option
  reports `max_flow_rules > 0`; `rss_key_size` taken from the device's RSS option). Google's driver
  README says *"Support for flow steering varies by VM platform, so it is best to check for support
  before attempting to use the feature"*, and that an XDP program attaches only when the RX and TX
  queue counts are at most half their maximum. Both of `gve`'s queue formats (GQI-QPL, DQO-RDA) advertise AF_XDP zero-copy
  (`gve_set_netdev_xdp_features`).
- `gve` refuses hardware TX timestamps (`HWTSTAMP_TX_OFF` only) — the acceptor's NIC-stamped wire
  window (C-40's instrument) does not exist on it; only the counterparty's round trip does.
- A compact placement policy *"places compute instances close to each other in a zone, which
  reduces network latency"*; sole-tenant nodes remove other tenants from the host, but *"you can't
  apply placement policies to sole-tenant instances"*. Threads-per-core = 1 disables SMT at creation
  on most series.

## Decision

1. **Scope.** A latency figure measured on a cloud VM is publishable when it is a pair for the
   ADR-0204 item — the kernel-TCP arm and the bypass arm, measured from a counterparty VM — and it
   carries the label in decision 3. Any other figure from a VM (a codec benchmark, a kernel-TCP
   headline figure alone) is outside this ADR and stays unpublishable until an ADR of its own.
2. **Placement.** A VM figure lives in **its own table**, headed as a cloud-VM measurement, never in
   a table with the desk's rows (C-40, `DESIGN.md` §8's budget rows, `benches/baselines.tsv`). It is
   never compared with a desk figure, never used as a denominator for a desk figure, and never the
   headline (ADR-0099 decision 1 is unchanged: the headline is kernel TCP, measured on the desk).
   ADR-0099 decision 2 holds inside the VM table: the bypass row appears only beside a kernel-TCP row
   **from the same VM boot**, same mode, same procedure pair (ADR-0068), same pair of VMs.
3. **The label — how non-negotiable 10 holds on a VM.** Every VM table carries, verbatim:
   - **Benchmark**: the committed script and its commit; the sha256 of the binaries on both VMs.
   - **Machine, for both VMs**: provider, machine type, zone, placement policy (or sole-tenant node
     type), threads per core, vCPU count, CPU model as `/proc/cpuinfo` reads it, image, `uname -r`,
     `ethtool -i` (driver and version), the queue format the driver logged, `ethtool -l`, and for
     the bypass arm the Onload commit and the XDP mode read back (`ss --xdp`, `zc:`).
   - **§9 settings**: `scripts/check-machine.sh` output verbatim — its `GUEST` FAIL included, never
     trimmed — then every `DESIGN.md` §9 row marked *applied*, *applied at vCPU level*, *absent (host
     property)* or *not applicable*, per §9 *A figure from a cloud VM*.
   - **Noise read, per run**: steal time over the run's window, and the run's UTC start and end.
4. **Noise is handled by the procedure, not by assumption.** On one pair of VMs and one boot, the
   two arms alternate run by run, so neighbour noise lands on both; each run's steal is printed; the
   reopening plan writes, before the first run, the steal above which a run is disqualified. A VM
   figure is re-measured, not reused, after the VMs are recreated: a new instance is a new machine.
5. **`check-machine.sh` keeps telling the truth.** A guest stays a `GUEST` FAIL; what changes is
   that the FAIL is printed in the label instead of forbidding the figure. The `virt_verdict` text
   *"no latency figure from it is publishable"* is out of date for this ADR's scope and is changed by
   the plan that first publishes a VM figure (a `scripts/` change, not in this docs-only PR); until
   then this ADR governs where the two disagree (`STATUS.md` item 114).

## Consequences

**Good**

- The bypass item can run without buying hardware, and its result — kept or killed — can be shown.
- Non-negotiable 10 holds: a VM figure names its benchmark, its machine and exactly which §9 rows
  were in force, including the ones that were not.
- The desk's figures keep their meaning: no VM row can be read into C-40 or the §8 budget.

**Bad — and accepted**

- **A VM figure is not comparable with anything measured on the desk**, and not with another VM
  figure from a different instance, zone or day. It answers one question: on this pair of VMs, in this
  boot, did Onload beat the kernel.
- **Noisy neighbours are measured, not removed.** Steal time shows CPU contention; it does not show
  contention in the host's memory system or the provider's network fabric, which move the tail with
  nothing printed. Sole-tenancy removes host neighbours but gives up the compact placement policy.
- **§9 rows are absent or not applicable by construction** — the guest row itself; governor, turbo
  and C-states; the physical NIC's IRQ affinity; the host's mitigations; EEE (no PHY) — and
  `isolcpus` isolates a vCPU, not a core. A p99.9 from a VM will
  carry excursions the desk's rows exist to remove.
- **No NIC timestamps**: `gve` gives none on transmit, so the pair is the counterparty's round trip
  only, which includes the peer VM's own stack and the fabric between the two VMs.
- **Rules and tooling disagree until the first VM plan lands**: `check-machine.sh` still prints
  "not publishable" for a guest.
- **A published VM figure will be quoted without its label.** Decision 2 forbids that inside this
  repository and cannot forbid it outside, as ADR-0099 already accepted for the bypass row.

## Sources

Read 2026-09-26: Linux `drivers/net/ethernet/google/gve/{gve_adminq.c, gve_ethtool.c, gve_main.c,
gve_rx_dqo.c}` at `master`; Google Cloud
[placement policies](https://docs.cloud.google.com/compute/docs/instances/placement-policies-overview),
[sole-tenancy](https://docs.cloud.google.com/compute/docs/nodes/sole-tenant-nodes),
[sole-tenant best practices](https://docs.cloud.google.com/compute/docs/nodes/sole-tenant-best-practices),
[threads per core](https://docs.cloud.google.com/compute/docs/instances/set-threads-per-core),
the `gve` driver [README](https://github.com/GoogleCloudPlatform/compute-virtual-ethernet-linux)
(*Receive Flow Steering*, *XDP*); `scripts/check-machine.sh` `virt_verdict`;
Onload issue [#337](https://github.com/Xilinx-CNS/onload/issues/337).
