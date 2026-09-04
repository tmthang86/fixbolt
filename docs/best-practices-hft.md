# Best Practices — `hft` Mode

`hft` is the **opt-in** mode: the engine thread never sleeps in the kernel on the hot
path — no `epoll_wait`, no futex, no blocking `read`. It burns a core to do it. This
page is operational advice for that mode; the OS tuning it depends on lives in the
[HFT playbook](hft-playbook.md), and the default mode in
[best-practices-standard.md](best-practices-standard.md). The split is a rule, not a
convenience ([ADR-0013](decisions/ADR-0013-two-modes-standard-and-hft.md),
non-negotiable 4).

---

## 1. One session per polling thread

The default `hft` deployment shape is **one session per polling thread**
([ADR-0012](decisions/ADR-0012-latency-first-and-one-session-per-polling-thread.md)).
The per-turn cost scales linearly with the sockets on a thread: `[measured 2026-08-31]`
`Engine::turn` itself is **~449 ns × N**, N = sockets on that thread, and a core is
burned regardless of N ([DESIGN.md](DESIGN.md) §8, `benches/turn.rs`,
[measured-costs.md](reference/measured-costs.md)).

- One session per thread keeps N=1 and the polling cost at its floor.
- Putting a second session on an `hft` thread doubles the idle cost every session pays,
  so `density` belongs in `standard`, not here.

---

## 2. Inline dispatch is the default, and usually right

`InlineDispatch` runs the application on the engine thread with no hand-off
([ADR-0002](decisions/ADR-0002-engine-library-split.md), D4). In `hft` this is the
default because a ring hand-off is a queue, and a queue is latency. Reach for
`RingDispatch` only when the application genuinely cannot be bounded on the hot path —
and know that a full ring disconnects the session rather than blocking the engine
([ADR-0011](decisions/ADR-0011-a-full-ring-disconnects.md)).

**Size a ring by the longest stall you must survive, not by throughput.** A ring exists
to absorb the worst pause the application takes, so its capacity is that pause times the
message rate — not the average.

---

## 3. Pinning cores — named by you, pinned from inside, read back

Core placement is explicit and verified, never assumed
([ADR-0015](decisions/ADR-0015-explicit-cores-pinned-from-inside-and-read-back.md)):

- **You name the cores.** The engine does not guess. A build without the `affinity`
  feature is a hard error when a core is named, not a flag that is accepted and does
  nothing.
- **The thread pins itself from inside**, then reads the mask back and reads the
  scheduler's own answer out of `/proc/thread-self/stat` — so a pin that did not take is
  a failure, not a silent no-op.
- **The plan is checked before any thread is created.** `Topology`/`ShardPlan` refuses a
  core that is absent, offline, duplicated, an **SMT sibling** of another core in the
  plan, or — for shard cores — outside `isolcpus`. An SMT sibling sharing a physical
  core is the common mistake, and it is refused up front, not discovered under load
  ([DESIGN.md](DESIGN.md) §7).

`isolcpus` and `rcu_nocbs` stay in the checklist — `[measured 2026-08-31]` free
(`Engine::turn` 494.8 ns on an `isolcpus` core, 498.2 on `rcu_nocbs`, against 501.8
untouched) and `[measured 2026-09-02]` worth **11× at p99.9** wire-to-wire on the
application path, nothing at p50 ([DESIGN.md](DESIGN.md) §9).

---

## 4. The wait strategy — busy-poll, and why `Yield` fails

`hft` busy-polls. `wait::Yield` — which calls `sched_yield` — **fails both mode gates**:
`[measured 2026-08-30]` 99.7% CPU (so it is not sleeping, failing the `standard` gate)
yet it yields the core (so it is not the tight poll `hft` needs). It is the one strategy
shown, not asserted, to belong to neither mode. Do not select it thinking it is a
gentler `hft`; it is a defect in both.

---

## 5. What is measured here, and what is not

- **Measured**: the per-turn cost (`~449 ns × N`), the round trip on a §9 machine
  (`hft` admin p50 16.0 µs, application p50 19.9 µs — [DESIGN.md](DESIGN.md) §8), the
  `isolcpus` p99.9 benefit, and that `Yield` fails both gates.
- **Not measured here**: any number on hardware you have not run the playbook against.
  The 16 µs figure is one machine (AMD Ryzen 7 3700X, pinned, mitigations on); it is a
  floor kernel TCP imposes, not a promise your box will hit. Run
  `scripts/check-machine.sh` and `bench.sh --strict`, and read the
  [HFT playbook](hft-playbook.md), before quoting a number as yours.

---

## 6. The resend ring, in `hft` mode

`[2026-09-04]` [ADR-0046](decisions/ADR-0046-the-ring-is-the-resend-store-and-a-replay-goes-in-batches.md).

**`Engine::add` allocates, and in this mode that matters.** The ring is a `Box<[Slot]>` built
in `MemJournal::new`, so accepting a connection costs one ~2 MiB allocation and 512 first-touch
page faults **on the engine thread** — the thread this mode exists to keep out of the kernel.

- **Pre-build the journals at startup and call `add_with_journal`.** It does everything `add`
  does except construct the journal, and startup is where D8 already wants every buffer
  pre-faulted. `benches/alloc.rs` measures it that way for the same reason.
- **A resend answers in batches** of `Config::resend_batch` messages per turn, default 8. In
  `hft` a turn is ~449 ns, so a hundred-message replay is thirteen turns and under a
  millisecond — and the alternative it replaced was one turn followed by
  `Logout 58=slow consumer`.
- **Nothing on the replay path allocates or reads a file.** The cursor is two `u32`s, the ring
  answers `get` in one index and one comparison, and disk is never read to answer a
  `ResendRequest` — non-negotiable 4, and ADR-0046 decision 5 (a) is why that is a decision
  rather than an omission.
