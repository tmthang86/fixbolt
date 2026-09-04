# Best Practices for `hft` Mode

`hft` is the **opt-in** mode. The engine thread never sleeps in the kernel on the hot path: no
`epoll_wait`, no futex, no blocking `read`. It burns a core to do that. This page is
operational advice for that mode. The OS tuning it depends on is in the
[HFT playbook](hft-playbook.md), and the default mode is in
[best-practices-standard.md](best-practices-standard.md). The split is a rule, not a
convenience ([ADR-0013](decisions/ADR-0013-two-modes-standard-and-hft.md), non-negotiable 4).

---

## 1. One session per polling thread

The `hft` deployment shape is **one session per polling thread**
([ADR-0012](decisions/ADR-0012-latency-first-and-one-session-per-polling-thread.md)). The
per-turn cost grows linearly with the sockets on a thread: `[measured 2026-08-31]`
`Engine::turn` is **~449 ns × N**, N being the sockets on that thread, and the core is burned
whatever N is ([DESIGN.md §8](DESIGN.md), `benches/turn.rs`,
[measured-costs.md](reference/measured-costs.md)).

- One session per thread keeps N = 1 and the polling cost at its floor.
- A second session on an `hft` thread doubles the idle cost every session pays. `density`
  belongs in `standard`.

---

## 2. Inline dispatch is the default, and usually right

`InlineDispatch` runs the application on the engine thread with no hand-off
([ADR-0002](decisions/ADR-0002-engine-library-split.md)). In `hft` a ring hand-off is a queue,
and a queue is latency: `[measured 2026-09-01]` the ring hop is **267 ns** one way against
**8.5 ns** inline on the tuned desktop. Reach for `RingDispatch` only when the application
genuinely cannot be bounded on the hot path, and know that a full ring disconnects the
session rather than blocking the engine ([ADR-0011](decisions/ADR-0011-a-full-ring-disconnects.md)).

**Size a ring by the longest stall you must survive, not by throughput.** The ring exists to
absorb the worst pause the application takes, so its capacity is that pause times the message
rate, not the average.

---

## 3. Cores: named by you, pinned from inside, read back

Core placement is explicit and verified, never assumed
([ADR-0015](decisions/ADR-0015-explicit-cores-pinned-from-inside-and-read-back.md)):

- **You name the cores.** The engine does not guess. A build without the `affinity` feature
  makes naming a core a hard error, not a flag that is accepted and does nothing.
- **Each thread pins itself from inside**, reads the mask back, and reads the scheduler's own
  answer from `/proc/thread-self/stat`. A pin that did not take is a failure, not a silent
  no-op.
- **The plan is checked before any thread exists.** `Topology` / `ShardPlan` refuse a core
  that is absent, offline, named twice, an **SMT sibling** of another core in the plan, or,
  for shard cores, outside `isolcpus`. Two SMT siblings sharing one physical core is the
  common mistake, and it is refused up front rather than found under load
  ([DESIGN.md §7](DESIGN.md)).
- **`serve_hft` pins nothing.** It runs the engine on the thread that called it, so that
  thread is yours to pin, with `affinity::pin_current_thread` before the call or `taskset`
  around the process. `serve_sharded_hft` pins every engine thread it starts.

`isolcpus` and `rcu_nocbs` stay in the checklist. `[measured 2026-08-31]` they are free
(`Engine::turn` 494.8 ns on an `isolcpus` core, 498.2 on `rcu_nocbs`, 501.8 untouched), and
`[measured 2026-09-02]` isolation is worth **11× at p99.9** wire-to-wire on the application
path and nothing at p50 ([DESIGN.md §9](DESIGN.md)).

---

## 4. The wait strategy: busy-poll, and why `Yield` fails

`hft` busy-polls. `wait::Yield`, which calls `sched_yield`, **fails both mode gates**:
`[measured 2026-08-30]` it uses 99.7% CPU, so it is not sleeping and fails the `standard`
gate, yet it yields the core, so it is not the tight poll `hft` needs. It is the one strategy
shown, not asserted, to belong to neither mode. Do not select it as a gentler `hft`.

---

## 5. What is measured here, and what is not

**Measured:** the per-turn cost (`~449 ns × N`); the round trip on a tuned machine (`hft`
administrative p50 16.0 µs, application p50 19.9 µs, [DESIGN.md §8](DESIGN.md)); the
`isolcpus` p99.9 benefit; that `Yield` fails both gates.

**Not measured:** any number on hardware you have not run the playbook against. The 16 µs
figure is one machine (AMD Ryzen 7 3700X, pinned, mitigations on) over loopback. It is a floor
kernel TCP imposes, not a promise your box will hit. Run `scripts/check-machine.sh` and
`scripts/bench.sh --strict`, and read the [HFT playbook](hft-playbook.md), before quoting a
number as yours.

---

## 6. The resend ring in `hft` mode

`[2026-09-04]` ([ADR-0046](decisions/ADR-0046-the-ring-is-the-resend-store-and-a-replay-goes-in-batches.md))

**`Engine::add` allocates, and in this mode that matters.** The ring is a `Box<[Slot]>` built
in `MemJournal::new`, so accepting a connection costs one ~2 MiB allocation and 512 first-touch
page faults **on the engine thread**, the thread this mode exists to keep out of the kernel.

- **Pre-build the journals at startup and call `add_with_journal`.** It does everything `add`
  does except construct the journal, and startup is where every buffer is pre-faulted anyway
  (DESIGN.md D8). `benches/alloc.rs` measures it that way for the same reason.
- **A resend answers in batches** of `Config::resend_batch` messages per turn, default 8. A
  turn is ~449 ns, so a hundred-message replay is thirteen turns and under a millisecond. The
  behaviour it replaced was one turn followed by `Logout 58=slow consumer`.
- **Nothing on the replay path allocates or reads a file.** The cursor is two `u32`s, the ring
  answers `get` in one index and one comparison, and disk is never read to answer a
  ResendRequest (non-negotiable 4; ADR-0046 decision 5).

---

## 7. The message log

`[2026-09-04]` **Off by default, and that is the right default here.** In `hft` the log costs
the engine thread one ring copy per message **per direction**, about 340 ns for a 200-byte
message, so a request/reply pair pays twice. `[unproven]`: that is arithmetic from
[DESIGN.md §8](DESIGN.md), not a measurement of this module. Against a p50 round trip of
~16 µs that is real, and it is yours to decide.

If you turn it on, **give the writer a core that is not the engine's**: `FileLog::open_pinned`,
or `serve_sharded_hft`'s `log_path`, which opens one file per shard before any shard thread
starts. An unpinned writer can land on the very core you isolated, which is what ADR-0015
decision 8 exists to prevent, and it will not look like a logging problem when it happens.

Sharded deployments get `messages.log.0`, `.1`, …, one per shard. Every engine numbers its
connections from zero, so one shared file would write `conn=0` for as many sockets as there
are shards.
