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
   `sudo -n strace -f -u "$USER" -o <file> target/release/w2w …` at the desk. Before and after each
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
