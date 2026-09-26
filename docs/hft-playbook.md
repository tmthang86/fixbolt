# HFT Playbook: an Ordered Tuning Procedure

How to take a Linux box from stock to a machine that produces an `hft` latency number you can
trust. It **supplements** [DESIGN.md §9](DESIGN.md), which holds the OS checklist row by row and
whose gate is `scripts/check-machine.sh`. This page does not repeat that table. It gives the
order to apply it in, the steps §9 does not cover (hardware, BIOS, NIC), and the two results
that contradict common HFT advice.

> Two things here are the opposite of what tuning guides usually say, and both were measured
> in this repository rather than borrowed: **`nohz_full` is not recommended**, and **the CPU
> speculation mitigations must stay on**. §3 and §7 say why.

---

## 1. Hardware

- **CPU:** single-thread clock matters more than core count. The hot path is one thread on one
  core, so prefer a high sustained single-core frequency.
- **NIC:** a low-latency adapter with steerable receive queues.
- **NUMA:** keep the engine core, its memory and the NIC on one node.
- **RAM:** enough that nothing the engine touches is ever paged.

Requirement, measured: a core the engine can own. Recommendation, not measured here: the
specific CPU and NIC. This page names the shape, not a part number.

## 2. BIOS and firmware

Set what the OS cannot: disable deep C-states, fix the turbo and frequency policy so the core
does not down-clock while polling, and decide SMT deliberately. An SMT sibling of the engine
core is refused by `ShardPlan` for a reason
([ADR-0015](decisions/ADR-0015-explicit-cores-pinned-from-inside-and-read-back.md)).

## 3. Kernel and boot

Apply the §9 rows in order, and read these three verdicts first:

- **`isolcpus` and `rcu_nocbs` stay.** `[measured 2026-08-31]` free (`Engine::turn` 494.8 ns
  on an `isolcpus` core against 501.8 untouched) and `[measured 2026-09-02]` worth **11× at
  p99.9** on the wire, nothing at p50
  ([ADR-0021](decisions/ADR-0021-nohz-full-leaves-section-9.md), [DESIGN.md §9](DESIGN.md)).
- **`nohz_full` is not recommended.** `[measured 2026-08-31]` it adds about **200 ns to every
  kernel entry on the core that has it, and about 45 ns on every core that does not**. It
  wins only from p99.99 outward. It is removed from the §9 recommendation, not forbidden
  ([ADR-0021](decisions/ADR-0021-nohz-full-leaves-section-9.md)).
- **The CPU speculation mitigations must stay on.** `[measured 2026-09-01]` disabling them
  makes every syscall **59–63% cheaper** (`engine turn, 1 idle session` 448.9 → 175.2 ns).
  But a machine with them off **cannot be compared** to one with them on, and
  `bench.sh --strict` refuses it
  ([ADR-0023](decisions/ADR-0023-section-9-records-the-cpu-mitigations.md)). This is not
  advice to disable them. It is the number that says why you must not, if you want a figure
  anyone else can reproduce.

After each row, read the setting back. Do not assume the kernel accepted it.

## 4. NIC and IRQ

Steer the NIC's receive queues onto cores that are **not** the engine core, keep its
interrupts off the isolated core, and enable `busy_poll` on the socket. The engine core should
see nothing but its own session.

`scripts/check-machine.sh` reads this, in `hft` mode same as `standard`, once a NIC is
selected (`FIXBOLT_NIC=<nic>`, or auto-selected: the first physical NIC under
`/sys/class/net` in name order — a bus device, not virtual, not wireless; carrier only
breaks a tie between several):

```
FIXBOLT_NIC=<nic> scripts/check-machine.sh   # NIC IRQ affinity, coalescing, irqbalance, busy_read
echo <cpu> | sudo tee /proc/irq/<n>/smp_affinity_list   # steer one IRQ off the engine core
sudo ethtool -C <nic> rx-usecs 0                        # interrupt coalescing off
systemctl stop irqbalance                               # stop it moving IRQs back
```

**EEE (802.3az) must be off on the measurement NIC.** `[measured 2026-09-15]` one A/B on the §9
desktop, `enp9s0` (Intel I211, `igb`) cabled to a Mac mini, `hft` admin paced at 1 s, same hour:
**EEE on added +14.6 µs to wire p50**, close to the ~16.5 µs
1000BASE-T wake time ([measured-costs.md](reference/measured-costs.md), boot B — the raw pair is A/B only, never a figure). Check with
`ethtool --show-eee <nic>`; the row this project cares about reads `EEE status: disabled`.
`scripts/check-machine.sh` reads this as its `eee` row, once a NIC is selected, alongside the
block above. `ethtool --set-eee <nic> eee <on|off>` and `ethtool -A <nic> …` (pause negotiation)
both bounce an `igb` link for ~4 s — wait for `Link detected: yes` (`ethtool <nic>`) before
measuring again, and re-read `--show-eee` rather than trust the command that set it. Leave pause
frames on and read all four `*_flow_control_*` counters (`ethtool -S <nic> | grep flow_control`)
before and after: they must stay 0.

**Kernel bypass (Onload over AF_XDP) is dropped on this NIC** — on kernels before v7.3, `igb` answers the RSS
indirection-table query but not the RSS key query, and Onload's AF_XDP path needs both. Before
trying it on any NIC, run `ethtool -x <nic> | grep -A1 'RSS hash key'`; `Operation not supported`
means stop there. Full trap, evidence and reopen condition:
[onload-af-xdp-needs-rss-key-ops-the-igb-driver-lacks](reference/onload-af-xdp-needs-rss-key-ops-the-igb-driver-lacks.md).
Which NICs and drivers pass, and what a rented VM or bare-metal server can do:
[kernel-bypass-needs-a-machine-this-project-does-not-have](reference/kernel-bypass-needs-a-machine-this-project-does-not-have.md).

## 5. Application configuration and the build

- **Core map:** core → shard → session, one session per polling thread
  ([ADR-0012](decisions/ADR-0012-latency-first-and-one-session-per-polling-thread.md)).
- **Capacity, ring and `Durability`:** see [best-practices-hft.md](best-practices-hft.md).
  Size the ring by the longest stall, not by throughput.
- **The `affinity` feature must be on.** Without it, naming a core is a hard error rather
  than a flag that does nothing
  ([ADR-0015](decisions/ADR-0015-explicit-cores-pinned-from-inside-and-read-back.md)).
- **Build profile:** keep the workspace default. `[measured 2026-09-01]` fat LTO is worth
  **−2.9% to −5.4%** at the median on the syscall-bound path, and it is the **consumer's** to
  enable in their own binary, not the library's
  ([ADR-0024](decisions/ADR-0024-the-workspace-keeps-the-default-release-profile.md)).

## 6. Measure a number you can use

1. `scripts/check-machine.sh` clean: the gate for §9.
2. `scripts/bench.sh --strict`: refuses a machine with mitigations off.
3. `tools/w2w` for the wire-to-wire round trip, pinned with `--engine-core` and
   `--client-core`. `scripts/w2w-baseline.sh` is the committed 20-run procedure.
   `[2026-09-14]` For two machines, `--listen <addr>` runs the engine half and `--connect <addr>`
   the generator half. The engine half prints its mode, transport and engine-thread allocations
   and **no latency figure**; the generator's table is a round trip *as the counterparty sees
   it*, the generator's host and the wire included. `--connect` refuses `--engine-core` and
   `--mode`; `--listen` refuses `--client-core`, `--messages`, `--warmup` (except beside
   `--wire-timestamps`, item 4), `--hold-ms` and `--interval`; both refuse `--tls` other than `off`. `--interval <us>` spaces sends by
   spinning, so the generator's core is busy for the whole wait between sends, not asleep. A paced
   run whose last `52=` would already be older than the counterparty's `MaxLatency` is refused at
   startup, before any socket opens; `--max-skew-ms` declares that `MaxLatency` (default
   `fixbolt_session::DEFAULT_MAX_SKEW_MS`).
   `--journal mem|file-async` and `--log none|file` apply to the combined run and `--listen`
   (both default to today's behaviour); `--connect` refuses both, for the reason it refuses
   `--mode`. `file-async` opens the engine's `FileJournal` with `Durability::Async`, and `file`
   its `FileLog`, each in a file under `std::env::temp_dir()` removed when the run ends — this is
   the row `DESIGN.md` §8's "if FileLog is on, ~340 ns [unmeasured]" comes from, boot B's B5.
   `scripts/w2w-baseline.sh` drives both halves: `LISTEN=<addr>` starts the engine half here and
   the generator half either here too (`GENERATOR_SSH` empty, a loopback split for rehearsing the
   procedure before a cable is run) or on another host over `ssh` — `GENERATOR_W2W` names the
   `w2w` binary on that host's `PATH` (default `w2w`). The generator connects to the address the
   engine half prints on its `listening:` line, so `LISTEN=<ip>:0` works; with `GENERATOR_SSH`,
   `LISTEN` must name an address that host can reach (`0.0.0.0` is refused), and the generator
   there is **not pinned** by the script — the header and summary say so. A split arm's figures are printed and
   summarised labelled "as the counterparty sees it — not an acceptor wire figure", naming the
   generator host, never as a §8 wire-to-wire number. `WIRE_NIC=<ifname> OBSERVER_CORE=<cpu>`
   adds the acceptor's own wire figure (item 4) to a split run: the engine half gets
   `--wire-timestamps --nic $WIRE_NIC --observer-core $OBSERVER_CORE --warmup $WARMUP`, every
   run must read `hw-rx-missing 0` and `hw-tx-missing 0` or **the script FAILS** (not
   DISQUALIFIED: a missing stamp does not clear by waiting), and the summary prints the median
   `wire p50/p99/p99.9` under its own heading, separate from the counterparty table. Refused
   before anything runs: without `LISTEN`, without `OBSERVER_CORE`, with the observer on the
   engine's core (or the client's, for a loopback split), and for any `standard` arm — run
   `standard` in its own invocation without `WIRE_NIC`. An `ARMS` entry also grows a fourth,
   optional field — `mode:path:tls:interval`, the interval in microseconds passed on as
   `--interval <us>`; `0`, the default, adds no flag and no line, same as before this field
   existed. `FIXBOLT_NIC` reaches the script's `scripts/check-machine.sh` call the same way
   any other environment variable does — one call since 2026-09-15, whose printed block and
   whose `machine pass … fail … unknown …` verdict line are the same reading (before that fix
   they were two readings, and on 2026-09-15 they disagreed). None of this changes the command line when `LISTEN` is
   unset and no `ARMS` entry uses a fourth field — `ARMS="hft:admin"` still means what it always
   meant. `[2026-09-14]` ADR-0068 decision 5: the header now also prints the commit, tree state,
   uptime and the binary's sha256/mtime, `OUT_DIR` (default
   `target/w2w-baseline/<UTC timestamp>-<HEAD>`) keeps every run's raw output plus an appended
   `summary.txt`, each summary gains two-sided `min/median`/`max/median` dispersion at every
   published percentile beside the existing `spread`, and `W2W_EXTRA` (split on whitespace, so
   `--journal file-async` or `--log file` needs no bespoke script) reaches the combined run and the
   `--listen` half only — never `--connect`, which refuses both flags.
4. `[2026-09-14]` **Wire-in → wire-out at the acceptor, on one clock — `hft` only**: add
   `--wire-timestamps --nic <ifname> --observer-core <cpu>` to the engine's process (`--listen`
   on the NIC's address; the combined run is over loopback and a hardware NIC never carries it),
   and give `--listen` the same `--warmup <n>` as the generator so its cold requests stay out of
   the wire figures. The observer spins on its core, which must not be the engine's or the
   client's and need not be in `isolcpus`. **`--mode standard` with the flag on a hardware NIC is
   refused**: a queued TX stamp raises `POLLERR` and makes a blocking engine spin
   ([reference](reference/a-transmit-timestamp-wakes-a-blocking-engine.md)) — measure `standard`
   there without the flag, from the generator's table only. **After every build, once:**

   ```sh
   sudo -n setcap cap_net_raw,cap_net_admin+ep target/release/w2w
   ```

   `cap_net_raw` opens the `AF_PACKET` tap; `cap_net_admin` is what `SIOCSHWTSTAMP` requires
   (`net/core/dev_ioctl.c`) — `cap_net_raw` alone is refused on a hardware NIC. `cap_net_admin`
   lets that file change the machine's network configuration, which is acceptable only because
   the one user who runs it already has `sudo -n`, the capability is lost at every rebuild, and
   `w2w` touches only `SIOCSHWTSTAMP`, prints the configuration before and after and restores the
   previous one on exit — a Ctrl-C or a `kill` (SIGINT, SIGTERM) included; a second signal, a
   `kill -9` or a crash restores nothing. **So around every B6 session:** record
   `ethtool --get-hwtimestamp-cfg <nic>` before the first run and read it again after the last; the
   two must agree, and where they do not, put the recorded one back with
   `sudo -n ethtool --set-hwtimestamp-cfg <nic> tx <mode> rx-filter <filter>` before the next run
   reads the wrong configuration as its "before". Do not run `w2w` under `sudo`. **A `w2w` started under `strace` (or
   `gdb`) by an unprivileged user does not get these capabilities** and refuses to run: the gate
   scripts' `W2W_EXTRA` arms therefore run on `lo` inside `unshare -Urn`, and to trace the engine
   thread **on the real NIC**, which a namespace cannot see, run
   `sudo -n strace -f -u "$USER" -o <file> target/release/w2w …` at the desk.
   `scripts/check-no-kernel-sleep-by-ctxt.sh` ([ADR-0072](decisions/ADR-0072-a-tracer-free-check-that-the-hft-engine-thread-never-sleeps.md))
   needs no tracer and no capability — it reads `/proc/self/task/<tid>/status` from the main
   thread, so it runs on the real NIC directly, without `sudo` and without the namespace
   workaround `strace` needs. Before and after each
   run read `ethtool -S <nic> | grep tx_hwtstamp_skipped` and write it beside `hw-tx-missing`:
   `igb` keeps one TX stamp pending and counts each one it skips there; the two must agree or be
   explained. Publish a wire figure only from a run whose `hw-rx-missing` and `hw-tx-missing`
   both read `0`: a driver's read-back of its own configuration is not evidence, each sample's
   hardware stamp is. On `lo` both read the request count and no wire column is printed, by
   design.

   **The runbook to read before and after every such procedure**
   (`docs/plans/2026-09-04-the-second-linux-desk.md`, *Sửa 3*, Điều 3, "Runbook B6"):

   ```sh
   sudo -n ethtool --show-eee <nic>                     # EEE status: disabled
   sudo -n ethtool -a <nic>                             # Autonegotiate on, RX on, TX on
   ethtool -S <nic> | grep -E 'flow_control|hwtstamp'   # four pause counters 0; tx_hwtstamp_skipped
   ethtool --get-hwtimestamp-cfg <nic>                  # tx off, rx-filter none, as restored above
   cat /proc/irq/<n1..n4>/smp_affinity_list             # each IRQ still steered off the engine core
   ssh <counterparty> 'ifconfig <if> | grep media; shasum -a 256 <path-to-w2w>'
   ```

   `[measured 2026-09-15]` **an `igb` NIC cannot hold an interval-0 wire figure with this
   procedure.** At `--interval 0` a TX stamp is skipped within 1–4 runs of 20 000:
   `tx_hwtstamp_skipped` rose with every attempt across boot B (0 → 1 → 5 → 29 → 50 → 52) —
   `igb` holds one TX timestamp at a time and skips the next request while one is already
   pending (`igb_main.c`, `tx_hwtstamp_skipped++`). The script FAILs any run with a missing stamp
   (the rule above), so an interval-0 wire figure is not obtainable on an Intel I211 with this
   procedure. **Paced at 1 s — the only pacing run over the cable — every stamp arrived; what the
   least pacing is that avoids a skip was not measured**
   ([measured-costs.md](reference/measured-costs.md), boot B, section B6).

5. **A measurement campaign's driver lives in `scripts/`, on the plan's branch, before the
   reboot** — never in gitignored `target/`, which dies with the desk
   ([ADR-0093](decisions/ADR-0093-a-campaign-driver-is-committed-sudo-in-a-committed-script-names-what-root-can-find-and-a-timer-due-inside-the-window-is-a-fail-row.md)
   decision 1). Before a campaign longer than an hour, `scripts/check-machine.sh`'s `no timer
   due` row must read clean: it FAILs on any systemd timer due inside `FIXBOLT_TIMER_WINDOW`
   hours (default 12), naming the units, and `ab-rotation.sh` refuses to start on that FAIL
   ([a-quiet-machine-check-cannot-see-a-timer-that-has-not-fired](reference/a-quiet-machine-check-cannot-see-a-timer-that-has-not-fired.md)).
   Any `sudo` line a driver adds to `scripts/` is read by
   `scripts/check-sudo-names-what-root-can-find.sh`, which FAILs a command root's `secure_path`
   cannot resolve by name — the fix is an absolute path, never a bare `cargo`, `rustc` or
   toolchain shim after `sudo`
   ([perf-record-exits-zero-when-sudo-cannot-find-the-workload](reference/perf-record-exits-zero-when-sudo-cannot-find-the-workload.md)).

**The measurement traps this project already paid for** are in [GUIDE.md §8](GUIDE.md). Read
them rather than rediscover them. A score that moves with its own timeout is measuring the
timeout, not the engine.

## 7. Acceptance, and what nobody has proven

- **The floor is kernel TCP's**, about 10–20 µs wire-to-wire. This engine owns about 2.9% of
  the round trip ([DESIGN.md §8](DESIGN.md)). You are tuning the box, not the protocol.
- **Every DESIGN.md §8 row is a number until you reproduce it.** The 16 µs figure is one
  machine (AMD Ryzen 7 3700X, pinned, mitigations on) over loopback. It is not yours until
  your own `bench.sh --strict` and `w2w` runs say so.
- **The figures are loopback, not NIC to NIC.** No driver, no interrupt and no wire is in
  them; [DESIGN.md §6](DESIGN.md) keeps the stricter row open.
- **This engine has never sent to a production FIX peer.** Its independent check is 7 interop
  cases each way against a real `libquickfix`, not a venue. Treat any number as provisional
  until your own run, on your own hardware, with these settings recorded, confirms it.

## 8. Renting the GCP VM pair for the bypass item (ADR-0204, ADR-0205)

This section is procedure, not the decision: [ADR-0204](decisions/ADR-0204-kernel-bypass-is-a-later-phase-candidate-that-reopens-on-a-named-machine.md)
decision 3 is the gate a rented pair must pass before anything is built on it, and
[ADR-0205](decisions/ADR-0205-a-latency-figure-from-a-cloud-vm-is-published-under-its-own-label-beside-a-same-boot-kernel-twin-and-never-compared-with-the-desk.md)
is how a figure from it is labelled. The platform choice and the hardware facts behind it are
[kernel-bypass-needs-a-machine-this-project-does-not-have](reference/kernel-bypass-needs-a-machine-this-project-does-not-have.md).
No price appears below — prices go stale and this repository is public; use
[GCP's pricing calculator](https://cloud.google.com/products/calculator) for a current estimate
before renting.

### Two gVNICs per VM, and why

Every VM this section creates — the gate VM and both measurement-pair VMs — gets **two** gVNICs:
`nic0` on the `default` VPC (SSH, outbound internet, external IP) and `nic1` on a new custom VPC
`bypass-data` (subnet `bypass-data-sub`, `10.10.0.0/24`, MTU 1460, `no-address`), carrying only the
pair's FIX traffic. The ADR-0204 gate (8.6) runs `ethtool -L` (queue count at or below half the
maximum) and Onload attaches an XDP program to the interface it accelerates; either can reset the
link it touches. On a single-NIC VM that NIC also carries SSH — including a Claude Code or VS Code
Remote-SSH session — so the gate or the measurement can cut the operator off mid-run. Two NICs
confine the gate and Onload's XDP program to `nic1`; `nic0` and the SSH session on it are never
touched by either.

A few Google-documented facts this shape depends on:

- **Additional network interfaces (vNICs) can only be defined at instance creation**, not added to
  a running VM afterward: *"you can define additional vNICs only when you create a instance"*
  ([Update the network interfaces for an
  instance](https://docs.cloud.google.com/compute/docs/networking/update-network-interfaces)). Both
  interfaces are named in the single `create` command (8.5, below); there is no later step that adds
  `nic1`.
- **The simplest configuration puts each interface in its own VPC network** — Google's own
  multi-interface guide's examples each attach one interface per VPC ([Create VMs with multiple
  network interfaces](https://docs.cloud.google.com/vpc/docs/create-use-multiple-interfaces));
  sharing one VPC network across interfaces is possible only when `nic0` is also attached to that
  same network ([Multiple network
  interfaces](https://docs.cloud.google.com/vpc/docs/multiple-interfaces-concepts)), which does not
  apply here. `bypass-data` is `nic1`'s own network, separate from `default`.
- **A custom-mode VPC starts with no firewall rules of its own beyond the implied deny**: *"every
  network has an implied deny firewall rule for ingress traffic"* ([VPC firewall rules
  overview](https://docs.cloud.google.com/firewall/docs/firewalls)). Only the auto-mode `default`
  network is pre-populated with `default-allow-internal`, `default-allow-ssh`, `default-allow-rdp`
  and `default-allow-icmp` ([VPC firewall
  rules](https://docs.cloud.google.com/vpc/docs/firewalls)); `bypass-data`, being custom-mode, gets
  none of these, so it needs its own allow rule (below) before the pair can reach each other over
  `nic1`.
- **Google Cloud's default VPC MTU is 1,460 bytes**, and a network's MTU can be raised as far as
  8,896 bytes ([Maximum transmission unit](https://docs.cloud.google.com/vpc/docs/mtu)).
  `bypass-data` is created at the default 1460 and stays there — not needed for FIX message sizes,
  and raising it is not free on `gve`: the driver checks the interface MTU against its RX packet
  buffer size before letting XDP attach — `max_xdp_mtu = priv->rx_cfg.packet_buffer_size -
  sizeof(struct ethhdr)` (minus `GVE_RX_PAD` in one queue format), and refuses with `"XDP is not
  supported for mtu %d."` when the MTU is over that ceiling
  ([gve_main.c](https://github.com/torvalds/linux/blob/master/drivers/net/ethernet/google/gve/gve_main.c),
  the XDP-verify path, around lines 1780–1786). A jumbo MTU (8896) risks the gate's own `ss --xdp`
  step failing for a reason that has nothing to do with flow steering or RSS.

`bypass-data` and `bypass-data-sub` are created once, shared by the gate VM and the measurement
pair, and deleted in 8.7 when nothing needs the pair:
```
gcloud compute networks create bypass-data --subnet-mode=custom --mtu=1460

gcloud compute networks subnets create bypass-data-sub \
  --network=bypass-data --region=asia-southeast1 --range=10.10.0.0/24

gcloud compute firewall-rules create bypass-data-internal \
  --network=bypass-data --direction=INGRESS --action=ALLOW \
  --rules=tcp,udp,icmp --source-ranges=10.10.0.0/24
```
The existing SSH rule (8.5, `bypass-pair-ssh`, on `default`) is unchanged and keeps protecting
`nic0`. Separately, the `default` network's own pre-populated `default-allow-ssh` opens port 22 to
`0.0.0.0/0` — wider than `bypass-pair-ssh`'s `OWNER_IP/32` — and is redundant once the tagged rule
is in place:
```
gcloud compute firewall-rules update default-allow-ssh --disabled
```

### 8.1 Account and billing

1. Sign in with a Google account and create a project: `gcloud projects create PROJECT_ID`, or
   the console's *New Project* dialog.
2. Link a billing account. **A new Google Cloud customer gets a free-trial credit** — $300,
   valid 90 days, and no charge until the account is explicitly upgraded to a paid one — but
   this is a promotional term Google can change; check
   [cloud.google.com/free](https://cloud.google.com/free) and the
   [free-trial FAQ](https://cloud.google.com/signup-faqs) at signup time rather than trust a
   figure written here.
3. **Payment method**: Google's own list of accepted cards is Visa, Mastercard, American
   Express and Discover/JCB where applicable
   ([payment methods](https://docs.cloud.google.com/billing/docs/how-to/payment-methods)); a
   Vietnamese-issued Visa or Mastercard is on that list. **Unverified**: whether a *debit* card
   issued in Vietnam is accepted in practice — user reports of rejected VN debit cards exist
   online, but this project found no Google document naming Vietnam as excluded or restricted.
   If a card is rejected, Google's own fallback is a different card or a reseller offering
   invoiced billing; there is no project-specific workaround.
4. Enable the Compute Engine API: `gcloud services enable compute.googleapis.com`.
5. Install the `gcloud` CLI per Google's own instructions
   ([install docs](https://cloud.google.com/sdk/docs/install)) and authenticate:
   ```
   gcloud auth login
   gcloud config set project PROJECT_ID
   ```
6. Set the region and a zone. `asia-southeast1` (Singapore) is the nearest Google Cloud region
   to Vietnam:
   ```
   gcloud config set compute/region asia-southeast1
   gcloud config set compute/zone asia-southeast1-b
   ```
   Both VMs go in the **same zone** — ADR-0204 decision 2 names one zone for the pair, and a
   compact placement policy (8.4) is zone-scoped.

### 8.2 Quota check

A new billing account's CPU quota is often too low for a `c3-standard-44`-class pair (8.3).
Check before choosing a machine type:
```
gcloud compute regions describe asia-southeast1 \
  --format="table(quotas.metric,quotas.limit,quotas.usage)" | grep -w CPUS
```
If the limit is short of what two instances need, request an increase from the console's
*IAM & Admin → Quotas* page, or:
```
gcloud beta quotas preferences create \
  --project=PROJECT_ID --service=compute.googleapis.com \
  --quota-id=CPUS-per-project-region --dimensions=region=asia-southeast1 \
  --preferred-value=VALUE --contact-email=OWNER_EMAIL \
  --justification="kernel-bypass measurement pair, ADR-0204"
```
A quota increase is not instant; a new account should ask for it before picking a boot date.

### 8.3 Machine series

The reference page's driver table (`gve`) is necessary but not sufficient: n-tuple steering and
the RSS key are **device options gVNIC exposes only on some machine series**
(ADR-0204 decision 3; ADR-0205 *Context*), so the gate is what decides, not this table. What the
table narrows to is which series to *try first*.

- **Tier_1 is a bandwidth tier, not a flow-steering feature.** Nothing found on Google's
  `gve` source or documentation ties n-tuple steering or the RSS key to Tier_1 — those are
  device options the virtual NIC offers or does not, independent of the egress-bandwidth tier
  (ADR-0204 decision 3; ADR-0205 *Context*). Tier_1 buys bandwidth headroom, which this item does
  not need — the gate is about latency-relevant device options, and the measurement pair's
  concern (if any) is tail latency, not throughput. Do not require Tier_1 for the gate.
- **Two stages, two different VMs.**
  1. **Gate VM** — cheap, disposable, used only to run the ADR-0204 gate (8.6). No Tier_1
     needed. The smallest published C3 size, **`c3-standard-4`** (4 vCPU, 2 physical cores) —
     verify `--threads-per-core=1` is accepted on it at creation time; Google's docs name N4A
     and Tau T2D as the series that cannot disable SMT at all, but do not state that every size
     of every other series accepts the flag, so confirm on this specific size before relying on
     it. On-demand or spot is fine — if the gate fails, delete the VM (8.7) and try another
     series or size; nothing measured on it is kept.
  2. **Measurement pair** — sized by the reopening plan once a gate-passing series is known
     (ADR-0204 decision 3: "the series that passed is part of every figure's label"). Tier_1 is
     **optional**, not required: enable it only if the reopening plan's own bandwidth needs call
     for it, not because the gate does.
  `gve`'s driver README does say n-tuple and RSS support "varies by VM platform" and to check
  before use — which is exactly why the gate (8.6), not this series list, is the actual decision
  ([gve README](https://github.com/GoogleCloudPlatform/compute-virtual-ethernet-linux)).
- **Which series to try first (for the gate VM):** **C3** and **C4** are current, gVNIC-default
  series per Google's
  [network-optimized](https://docs.cloud.google.com/compute/docs/network-optimized-machines) and
  [general-purpose](https://docs.cloud.google.com/compute/docs/general-purpose-machines)
  machine-family pages; **N2** is a fallback if C3/C4 quota is unavailable. This ordering is
  about gVNIC being the default NIC and current generation, not about the gate's outcome, which
  none of these pages state.
- **Spot vs on-demand.** A spot VM can be preempted mid-run. ADR-0205 decision 4 already treats
  a recreated instance as a new machine whose figures are not reused; a preemption during a
  measurement boot is the same event, uninvited. Use on-demand for the boot that produces a
  published figure; spot is acceptable for the gate VM and for throwaway development, where a
  mid-run restart costs nothing.

### 8.4 Placement: sole-tenant node vs compact placement policy

ADR-0204 leaves this choice to the owner. Google's own sole-tenancy docs are explicit that
**the two cannot be combined**: *"you can't apply placement policies to sole-tenant
instances"* ([sole-tenancy](https://docs.cloud.google.com/compute/docs/nodes/sole-tenant-nodes)).
Pick one.

**Compact placement policy** (both VMs physically close in the zone, still multi-tenant):
```
gcloud compute resource-policies create group-placement bypass-pair-policy \
  --collocation=collocated --region=asia-southeast1
gcloud compute instances create bypass-acceptor \
  --resource-policies=bypass-pair-policy --zone=asia-southeast1-b …
gcloud compute instances create bypass-counterparty \
  --resource-policies=bypass-pair-policy --zone=asia-southeast1-b …
```

**Sole-tenant node** (both VMs alone on dedicated hardware, no compact placement). The node type
name is not guessed here — list what this project's zone actually offers and use that value:
```
gcloud compute sole-tenancy node-types list --filter="zone:asia-southeast1-b"
```
```
gcloud compute sole-tenancy node-templates create bypass-node-template \
  --node-type=<C3 node type from: gcloud compute sole-tenancy node-types list --filter="zone:asia-southeast1-b"> \
  --region=asia-southeast1
gcloud compute sole-tenancy node-groups create bypass-node-group \
  --node-template=bypass-node-template --target-size=1 --zone=asia-southeast1-b
gcloud compute instances create bypass-acceptor bypass-counterparty \
  --node-group=bypass-node-group --zone=asia-southeast1-b …
```
The node type must match the machine series chosen in 8.3; a sole-tenant node costs for the
whole node, not per instance, so this is the more expensive of the two shapes to leave running.

### 8.5 Create the two VMs

An Onload build needs a kernel inside the 6.1–7.0 range the README states
(reference page, *What Onload's AF_XDP path asks of a driver*). Two current GCP images are
candidates: **Debian 12 (`debian-12`)**, whose distribution ships kernel 6.1 by default, and
**Ubuntu 24.04 LTS (`ubuntu-2404-lts-amd64`)**, whose distribution ships kernel 6.8 by default —
these defaults come from Debian's and Canonical's own release documentation, not from a single
GCP page naming the exact kernel per image; Google's
[OS details page](https://docs.cloud.google.com/compute/docs/images/os-details) confirms the
image families exist and lists their interfaces but not their exact shipped kernel build.
**Treat both numbers as unconfirmed until `uname -r` on the booted VM says so** — which ADR-0204
decision 3 requires quoting anyway. **Ubuntu 22.04's GA kernel is 5.15, outside the range**; its
HWE kernel moves later but is not the image default, so prefer 24.04 over relying on an HWE
upgrade after boot.

**Networking, outbound and inbound.** All of this is about `nic0` — `nic1` (`bypass-data`, above)
is created `no-address` unconditionally, in both shapes below, and carries no SSH and no outbound
internet traffic; it exists only for the pair's FIX traffic. Both VMs need outbound internet on
`nic0` to fetch the Onload source, kernel headers and build tooling. The simplest arrangement for a
solo owner is an **external IP on `nic0`**, with the firewall doing the restricting (below); the
alternative — `no-address` on `nic0` too, plus
[Cloud NAT](https://docs.cloud.google.com/nat/docs/overview) for outbound and
[IAP TCP forwarding](https://docs.cloud.google.com/iap/docs/using-tcp-forwarding) for inbound SSH
— gives up no traffic capability but adds a Cloud Router and a NAT gateway to create, keep track
of and eventually delete alongside the VMs (8.7). This procedure uses the external-IP shape for
`nic0`; commands for the no-address alternative (also `nic0` only) follow it.

```
# gate VM — cheap, disposable, run only the ADR-0204 gate (8.6) on it
gcloud compute instances create bypass-gate \
  --zone=asia-southeast1-b --machine-type=c3-standard-4 \
  --image-family=debian-12 --image-project=debian-cloud \
  --network-interface=nic-type=GVNIC,network=default \
  --network-interface=nic-type=GVNIC,network=bypass-data,subnet=bypass-data-sub,no-address \
  --threads-per-core=1 \
  --tags=bypass-pair

# measurement pair — created only after the gate above passes, series/size per the reopening plan
gcloud compute instances create bypass-acceptor \
  --zone=asia-southeast1-b --machine-type=MACHINE_TYPE \
  --image-family=debian-12 --image-project=debian-cloud \
  --network-interface=nic-type=GVNIC,network=default \
  --network-interface=nic-type=GVNIC,network=bypass-data,subnet=bypass-data-sub,no-address \
  --threads-per-core=1 \
  --tags=bypass-pair \
  [--resource-policies=bypass-pair-policy | --node-group=bypass-node-group]

gcloud compute instances create bypass-counterparty \
  --zone=asia-southeast1-b --machine-type=MACHINE_TYPE \
  --image-family=debian-12 --image-project=debian-cloud \
  --network-interface=nic-type=GVNIC,network=default \
  --network-interface=nic-type=GVNIC,network=bypass-data,subnet=bypass-data-sub,no-address \
  --threads-per-core=1 \
  --tags=bypass-pair \
  [--resource-policies=bypass-pair-policy | --node-group=bypass-node-group]
```
Add `--network-performance-configs=total-egress-bandwidth-tier=TIER_1` to either instance only
if the reopening plan states a bandwidth reason for it (8.3) — it is not part of the gate.

**Firewall**, allowing only the pair's own traffic and SSH from the owner's IP — nothing else:
```
gcloud compute firewall-rules create bypass-pair-internal \
  --network=default --direction=INGRESS --action=ALLOW \
  --rules=tcp,udp --source-tags=bypass-pair --target-tags=bypass-pair

gcloud compute firewall-rules create bypass-pair-ssh \
  --network=default --direction=INGRESS --action=ALLOW \
  --rules=tcp:22 --source-ranges=OWNER_IP/32 --target-tags=bypass-pair
```

**The `no-address` alternative**, if a public IP on `nic0` is unwanted. `nic1` already has
`no-address` unconditionally above; add `no-address` to each VM's `nic0` `--network-interface`
too, then:
```
# outbound: a Cloud Router and NAT gateway in the same region
gcloud compute routers create bypass-nat-router --network=default --region=asia-southeast1
gcloud compute routers nats create bypass-nat \
  --router=bypass-nat-router --region=asia-southeast1 \
  --auto-allocate-nat-external-ips --nat-all-subnet-ip-ranges

# inbound SSH: source must be Google's own IAP range, not the owner's IP —
# IAP terminates the connection and re-originates it from this range
# (verified: docs.cloud.google.com/iap/docs/using-tcp-forwarding)
gcloud compute firewall-rules create bypass-pair-ssh-iap \
  --network=default --direction=INGRESS --action=ALLOW \
  --rules=tcp:22 --source-ranges=35.235.240.0/20 --target-tags=bypass-pair

gcloud compute ssh bypass-acceptor --zone=asia-southeast1-b --tunnel-through-iap
```
This is the more locked-down shape — neither VM is directly reachable from the internet — at the
cost of two more resources (router, NAT) to create and delete alongside the VMs. For a solo
owner running a short-lived measurement pair, external IP plus the restrictive firewall rules
above is the simpler choice and what the rest of this section assumes; switch to `no-address`
plus Cloud NAT and IAP if the exposure of a public IP, however firewalled, is unacceptable.

**Find each NIC's interface name, and confirm the pair reaches each other over `nic1`.** Do not
assume a name — GCP's interface naming can vary by image and by how many NICs a VM has:
```
ip -br addr
```
On a Debian 12 image, `nic0` is commonly `ens4` and `nic1` commonly `ens5`, but read this
command's own output rather than trust that pattern. List each VM's `nic1` address:
```
gcloud compute instances list --filter="tags.items=bypass-pair" \
  --format="table(name,networkInterfaces[1].networkIP)"
```
Then from one VM, ping the other over `nic1` specifically — not the default route, which is
`nic0`:
```
ping -I <nic1> <peer's nic1 IP from the listing above>
```
A reply confirms `bypass-data-sub` routes between the pair before the gate below or a measurement
run relies on it.

### 8.6 Run the ADR-0204 gate on the gate VM

Run on `bypass-gate` (8.5), not on the measurement pair — the pair is not created until this
gate passes. Every line below is ADR-0204 decision 3, in order, run against `nic1` (the interface
found above) — never `nic0`, which the gate must not touch. A pass on every line is what
lets the reopening plan proceed to size and create the measurement pair; **a fail on any line:
stop, delete the gate VM (8.7), and record which line failed and on which machine type** — do
not try to work around a failing line on the same series; try another series or size instead.

```
# machine type, zone, and confirm the kernel is in Onload's 6.1–7.0 range
gcloud compute instances describe bypass-gate --zone=asia-southeast1-b \
  --format='value(machineType,zone)'
uname -r                                           # e.g. 6.1.0-... or 6.8.0-...

# driver is gve, on nic1 (found above, e.g. ens5 — not nic0/ens4)
ethtool -i <nic1>                                  # pass: driver: gve

# flow steering is a device option this instance's series offers
dmesg | grep -i "FLOW STEERING"                    # pass: "... enabled with max rule limit of N", N > 0

# n-tuple steering can be turned on
sudo ethtool -K <nic1> ntuple on
ethtool -k <nic1> | grep ntuple                    # pass: ntuple-filters: on

# the device offers an RSS hash key (G0 from the reference page)
ethtool -x <nic1> | grep -A1 "RSS hash key"        # pass: a key is printed, not "Operation not supported"

# queue count at or below half the maximum before an XDP program can attach
ethtool -l <nic1>                                  # read "Combined" maximum
sudo ethtool -L <nic1> combined <max/2 or less>
ethtool -l <nic1>                                  # pass: current <= half of maximum

# register nic1 for Onload's AF_XDP path — gve is not an AMD Solarflare adapter, so this is the
# path Onload's own README documents for "any interfaces ... not AMD Solarflare interfaces"
# (github.com/Xilinx-CNS/onload README, "Onload with AF_XDP")
echo <nic1> | sudo tee /sys/module/sfc_resource/afxdp/register

# after Onload registers and binds the socket:
ss --xdp                                           # pass: zc:1 on the bound socket
```
A `dmesg` line absent, `ethtool -x` returning `Operation not supported`, or `ss --xdp` never
showing `zc:1` are each, individually, the stop condition — the same three ways the desk's I211
failed (ADR-0203). A failing series does not disqualify GCP; ADR-0204 decision 3 allows trying
another series, with the series that passed named in every figure's label thereafter.

If instead the kernel log shows `gve ...: XDP is not supported for mtu %d.`, the stop condition
is the MTU, not the series or the gate: `bypass-data` was raised above `gve`'s XDP-checked ceiling
(above, *Two gVNICs per VM*) — set it back to 1460 and rerun rather than trying another machine
series.

### 8.7 Cost control

- **Delete the gate VM** as soon as 8.6 has a verdict, pass or fail — it is disposable by
  design and nothing about it is reused: `gcloud compute instances delete bypass-gate --zone=asia-southeast1-b`.
- **Stop** a measurement-pair instance between sessions to release the vCPU/memory billing
  while keeping the disk and its configuration:
  `gcloud compute instances stop bypass-acceptor bypass-counterparty --zone=asia-southeast1-b`.
  A stopped sole-tenant node still bills for the node itself.
- **Delete** the measurement pair when the item is not being worked: this also ends node-group
  and disk billing. ADR-0205 decision 4 already treats a recreated instance as a new machine, so
  deleting between measurement campaigns costs nothing the record depends on.
  ```
  gcloud compute instances delete bypass-acceptor bypass-counterparty --zone=asia-southeast1-b
  gcloud compute sole-tenancy node-groups delete bypass-node-group --zone=asia-southeast1-b   # if used
  gcloud compute resource-policies delete bypass-pair-policy --region=asia-southeast1          # if used
  gcloud compute firewall-rules delete bypass-pair-internal bypass-pair-ssh
  gcloud compute firewall-rules delete bypass-pair-ssh-iap                                     # if the no-address alternative was used
  gcloud compute routers nats delete bypass-nat --router=bypass-nat-router --region=asia-southeast1   # if used
  gcloud compute routers delete bypass-nat-router --region=asia-southeast1                     # if used
  gcloud compute firewall-rules delete bypass-data-internal
  gcloud compute networks subnets delete bypass-data-sub --region=asia-southeast1
  gcloud compute networks delete bypass-data
  ```
  `bypass-data`, its subnet and its firewall rule are shared by the gate VM and the measurement
  pair (above); delete them once neither is left running, not after each VM individually.
- **Budget alert**, so an idle pair left running is caught before a bill surprises the owner:
  ```
  gcloud billing budgets create --billing-account=BILLING_ACCOUNT_ID \
    --display-name="bypass-pair-budget" --budget-amount=AMOUNTUSD \
    --threshold-rule=percent=0.5 --threshold-rule=percent=0.9 --threshold-rule=percent=1.0
  ```
  or the console's *Billing → Budgets & alerts → Create Budget*. A budget alert notifies; it
  does not stop the VMs by itself.
