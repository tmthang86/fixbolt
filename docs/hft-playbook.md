# HFT Playbook — an Ordered Tuning Procedure

This is the ordered procedure for taking a box from stock to a machine that produces a
latency number you can trust, in `hft` mode. It **supplements** [DESIGN.md](DESIGN.md)
§9, which holds the canonical OS checklist as a row-by-row table and whose gate is
`scripts/check-machine.sh`. **This page does not repeat that table** — it gives the
order to apply it in, the steps §9 does not (hardware, BIOS, NIC), and the two results
that run against industry habit. Where a step is an OS row, tune it in §9 first and
point here.

> Two things on this page are the opposite of what an HFT tuning guide usually says, and
> both are measured here, not borrowed: **`nohz_full` is not recommended**, and **the CPU
> speculation mitigations must stay on**. Sections 3 and the acceptance step explain why.

---

## 1. Hardware

- **CPU**: single-thread clock matters more than core count — the hot path is one thread
  on one core. Prefer a high sustained single-core frequency.
- **NIC**: a low-latency adapter with steerable receive queues.
- **NUMA**: keep the engine core, its memory, and the NIC on the same node.
- **RAM**: enough that nothing the engine touches is ever paged.

`[requirement, measured]` a core the engine can own. `[recommendation, not measured
here]` the specific CPU and NIC — this page names the shape, not a part number.

## 2. BIOS / firmware

Set what the OS cannot: disable deep C-states, fix the turbo/frequency policy so the
core does not down-clock when idle-polling, and decide SMT deliberately — an SMT sibling
of the engine core is refused by `ShardPlan` for a reason
([ADR-0015](decisions/ADR-0015-explicit-cores-pinned-from-inside-and-read-back.md)).

## 3. Kernel and boot

Apply the §9 rows; this section gives the order and the two counter-intuitive verdicts.

- **`isolcpus` and `rcu_nocbs` stay.** `[measured 2026-08-31]` free — `Engine::turn`
  494.8 ns on an `isolcpus` core against 501.8 untouched — and `[measured 2026-09-02]`
  worth **11× at p99.9** on the wire, nothing at p50
  ([ADR-0021](decisions/ADR-0021-nohz-full-leaves-section-9.md), [DESIGN.md](DESIGN.md) §9).
- **`nohz_full` is NOT recommended.** `[measured]` it adds **~200 ns to every kernel
  entry on the core that has it, and ~45 ns on every core that does not** — a hundred to
  save one. It is removed from the §9 recommendation, not forbidden
  ([ADR-0021](decisions/ADR-0021-nohz-full-leaves-section-9.md)). A guide that lists it
  by reflex is wrong for this engine.
- **The CPU speculation mitigations must stay ON.** `[measured 2026-09-01]` disabling
  them makes every syscall **59–63% cheaper** (`engine turn, 1 idle session` 448.9 →
  175.2 ns) — but a machine with them off **cannot be compared** to one with them on, and
  `bench.sh --strict` refuses it ([ADR-0023](decisions/ADR-0023-section-9-records-the-cpu-mitigations.md)).
  **This is not advice to disable them.** It is the number that says why you must not, if
  the goal is a figure anyone else can reproduce.

Confirm each row took: read the setting back, do not assume the kernel accepted it.

## 4. NIC and IRQ

Steer the NIC's receive queues onto cores that are **not** the engine core, keep its
interrupts off the isolated core, and enable kernel `busy_poll` on the socket. The
engine core should see nothing but its own session.

## 5. Application configuration and the build

- **Core map**: core → shard → session, one session per polling thread
  ([ADR-0012](decisions/ADR-0012-latency-first-and-one-session-per-polling-thread.md)).
- **Capacity, ring, `Durability`**: see [best-practices-hft.md](best-practices-hft.md) —
  size the ring by the longest stall, not throughput.
- **The `affinity` feature must be on**, or a build that names a core is a hard error
  rather than a flag that does nothing ([ADR-0015](decisions/ADR-0015-explicit-cores-pinned-from-inside-and-read-back.md)).
- **Build profile**: keep the workspace default. `[measured]` fat LTO is worth **−2.9%
  to −5.4%** at the median, and it is the **consumer's** to enable in their own binary,
  not the library's ([ADR-0024](decisions/ADR-0024-the-workspace-keeps-the-default-release-profile.md)).

## 6. Measure a number you can use

- `scripts/check-machine.sh` clean — the gate for §9.
- `scripts/bench.sh --strict` — refuses a machine with mitigations off (§3).
- `tools/w2w` for the wire-to-wire round trip, pinned via `--engine-core`/`--client-core`.
- **The five measurement traps this project already paid for** are in
  [GUIDE.md](GUIDE.md) §8 — read them, do not re-discover them. A score that moves with
  its own timeout is measuring the timeout, not the engine.

## 7. Acceptance, and what nobody has proven

- **The floor is kernel TCP's**, ~10–20 µs wire-to-wire; this engine owns ~2.9% of that
  round trip ([DESIGN.md](DESIGN.md) §8). You are tuning the box, not the protocol.
- **Every `DESIGN.md` §8 row is a number until you reproduce it.** The 16 µs figure is
  one machine (AMD Ryzen 7 3700X, pinned, mitigations on). It is not yours until your
  `bench.sh --strict` says so.
- **`standard` has no figure worth quoting**, and this engine **has never sent to a
  production FIX peer** — its independent check is 7 interop cases against a real
  `libquickfix`, not a venue. Treat any number as provisional until your own run
  confirms it, on your own hardware, with these settings recorded.
