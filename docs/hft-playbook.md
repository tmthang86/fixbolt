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
