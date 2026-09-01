# fixbolt — Developer Guide

**Who this is for: somebody embedding fixbolt in their own application.** Not somebody
building fixbolt — that is [DESIGN.md](DESIGN.md), and *what* it must do is
[PRD.md](PRD.md).

This is a **framework**: it calls your code on its hot path, on a thread it owns, under
constraints it cannot enforce for you. Everything below is a constraint that shows up as
latency or as lost messages rather than as a compile error. **Where a rule is not enforced by
the type system or a test, this page says so.**

> **Every number on this page names its machine.** `[measured 2026-08-30]` means it was run
> here on the §9 machine — AMD Ryzen 7 3700X, Linux 7.0.0-30, `scripts/check-machine.sh`
> reporting `pass 10 fail 0` — and read, not estimated. Yours will differ; the *ratios* are
> what transfer.

---

## 0. First decide your mode

`[ADR-0013](decisions/ADR-0013-two-modes-standard-and-hft.md)` — and if you say nothing you get
`standard`, which is almost certainly what you want.

| | **`standard`** — the default | **`hft`** — opt-in |
|---|---|---|
| When idle | **blocks on readiness, gives the core back** | spins; **burns a core, permanently, per polling thread** |
| Wakeup cost | `epoll`-class, 2–5 µs | `[measured 2026-08-31]` **449 ns per session per turn** — ~670 ns if the core carries `nohz_full`, which §9 no longer asks for |
| Runs on | any OS, any hardware, a container, a shared box | Linux, on a machine that satisfies `DESIGN.md` §9 |
| Core pinning | none | required, and it refuses to start without it |
| Choose it when | you are not counting microseconds, or you share the machine | one session matters more than the core it costs |

**Do not choose `hft` because it sounds better.** It pins a core at 100% for as long as the
process lives. On a shared machine, in a container, or on a laptop that is not a bug you will
enjoy diagnosing — it is the engine doing exactly what you asked.

**A `standard` number and an `hft` number are not comparable.** When you publish one, say which.

`[2026-08-30]` **`standard` is built.** `serve(addr, cfg, app, capacity)` is `standard` and is
what you get if you say nothing; `serve_hft` is the spinning one. Everything below describes the
design both modes share, and says where a mode changes it.

**The `hft` figures on this page have not moved and none of them describes `standard`**, whose
own numbers do not exist yet on a machine worth quoting — `[measured 2026-08-30]` a shared
container gave `standard` p50 29.0 µs against `hft` 17.7 µs, which is a ratio on one untuned box
and not a figure. What that run does establish is that `standard` is woken by the data rather
than by its own 100 ms timeout, three orders of magnitude apart.

---

## 1. The one thing that decides your latency

**Sessions per polling thread**, and this section is about **`hft`**. Nothing else on this page
comes close for that mode. In `standard` the thread blocks rather than sweeping, so the term
below is replaced by the wakeup — different arithmetic, and **unmeasured**.

An idle turn of the engine is one non-blocking `read` per connection. `[measured 2026-08-31]`
a whole `Engine::turn` costs **449 ns per session on a core set up to `DESIGN.md` §9**
and **~670 ns if that core carries `nohz_full`**, which §9 no longer asks for
([ADR-0021](decisions/ADR-0021-nohz-full-leaves-section-9.md)), flat from 1 to 16 sessions
within 2%. Of the 449, **~420 ns is
the `recv` syscall and about 30 ns is everything the engine itself does** — measured in the same
run, so that subtraction is not across two programs. So the sweep is `N ×` that figure, and a
message arriving just after its socket was polled waits a whole sweep before anyone looks at it.

**The isolated core is the expensive one**, by 36%, and that is the machine this engine
recommends. It is a trade against a tail nobody has measured yet, not a free win —
[measured-costs.md](reference/measured-costs.md).

| Sessions on one thread | Added latency, worst case | Against the engine's own parse at 125.5 ns |
|---|---|---|
| 1 | 0.70 µs | 5.6× the parse, just to find the message |
| 2 | 1.41 µs | exceeds `DESIGN.md` §8's entire user-space budget |
| 16 | 11.2 µs | comparable to the whole kernel-TCP floor |
| 128 | 90 µs | |

**If you care about latency, run one session per thread and pin that thread to an isolated
core.** If you are building a gateway for many clients, you are in the `density` shape — that
is supported and reasonable, and you should plan against `N × 449 ns` on a §9 core
rather than against this project's headline figures ([ADR-0012](decisions/ADR-0012-latency-first-and-one-session-per-polling-thread.md)).

**Nothing enforces this.** The engine will happily carry 500 sessions on one thread and will
not warn you.

**And kernel bypass does not rescue it.** `[measured 2026-08-31]` removing the syscall — Onload,
`ef_vi`, DPDK — takes **~420 ns of the 449** down to the cost of a memory read, which is real and
worth having.
Two terms survive it:

- **Cache.** One `Connection` is **53.3 KiB**; `L1d` on the test machine is 32 KiB, so *one
  connection does not fit in L1*. Random access costs **1.05 ns** in L1 and **78.5 ns** from
  RAM — **75×** — and that applies to every access the engine makes, not just to polling.
- **Head-of-line blocking.** One thread serialises. Per-message work here is ~465 ns
  (`parse` 125.5 + `encode` 240.0 + the session step), so `k` sessions holding a message at
  once make the last one wait `(k-1) × 465 ns`. Nothing removes this except fewer sessions.

Full working: [reference/measured-costs.md](reference/measured-costs.md).

---

## 1a. Running many sessions: shard across threads, do not stack on one

A gateway with a hundred sessions on a sixteen-core server is a perfectly good deployment, and
the arithmetic works — **as long as it is *shard*, not *stack***. The two are different
architectures and the difference is the whole of §1:

| Shape | 100 sessions | Sweep |
|---|---|---|
| **Stack** — one engine, one thread | 100 on one thread | `100 × 449 ns` = **45 µs** |
| **Shard** — 8 engines, 8 pinned threads | ~13 each | `13 × 449 ns` = **5.8 µs** |

Nine microseconds sits under the 10–20 µs kernel-TCP floor, so for a gateway it is not the
dominant term any more. Sharding is what makes "many sessions" reasonable — **not the core
count by itself**, because a session only benefits from a core its own polling thread is on.

`[2026-08-31]` **`fixbolt_engine::shard` now does some of this**, behind the
`affinity` feature: `Shards::start` validates a `ShardPlan`, starts one pinned thread per core,
waits for every one of them to confirm its own pin, and hands accepted connections across a
channel. `serve_sharded_hft` is the whole loop.

`[2026-09-01]` **the shard you land on is decided by your identity, not by accept order.**
`serve_sharded_hft` holds each socket until its `Logon` arrives, reads `49=`/`56=` off it, and
routes on a **stable** hash of the pair — so both connections claiming one identity reach the
same engine and the single-logon rule can see them both.

> **This was a defect until 2026-09-01, and the fix is why the API changed.** An `Engine`
> carries one `Config`, so it serves **one FIX identity**, and it enforces *"that identity is
> already logged on"* by looking at the other connections **it** holds. Splitting those across
> engines left the rule nothing to look at: `[measured 2026-08-31]` the acceptance corpus
> scored **59 through one shard and 57 through two**, failing exactly
> `1b_DuplicateIdentity.def` and `AlreadyLoggedOn.def`. `[measured 2026-09-01]` it is **59
> through two** — [ADR-0020](decisions/ADR-0020-a-pre-session-stage-owns-the-socket-until-logon.md).
> `Assign` and `RoundRobin` are **gone**: `Assign` was asked at accept time, when nothing knew
> whose socket it was, and round-robin is the policy that produced the defect.

**Two limits you must choose, because there is no default for either.**
`serve_sharded_hft` takes a `presession::Limits`, and `Limits::new(pending, logon_ms)` refuses
a zero in both places:

| Limit | What happens without a sensible one |
|---|---|
| `logon_ms` — how long a connection has to send its `Logon` | A counterparty that opens a socket and says nothing holds a slot until you restart. This is a denial-of-service hole, not a tuning knob |
| `pending` — how many may wait at once | The table has no ceiling and neither does the memory behind it. When it is full the **next** connection is refused immediately rather than queued, which is the behaviour you want under attack |

Neither number can be picked by somebody who has not seen your deployment, which is why the
API will not pick one for you — the same reason `ShardPlan` makes you name your cores.

**Replacing the routing policy.** `Shards::with_route(Box<dyn Route>)` takes an
`Identity<'_>` and the shard count. Real deployments shard by counterparty deliberately; the
hash is a sensible default and not the final answer. Whatever you write must be **stable
across processes** — the same counterparty has to reach the same shard after a restart, or the
single-logon rule breaks again in a way that every test passes. `DefaultHasher` is seeded per
process and is the trap here.

**Everything below is what the engine still does not do for you.**

**The engine does not shard for you.** `[2026-08-30]` `Engine` holds a flat
`Vec<Connection>`, `turn()` sweeps all of them and `run()` is `loop { turn() }`. The only
thread the crate spawns is the journal's async writer. **You build the sharding**, and the
pieces are there because `Acceptor` and `Engine` are separate:

- `Acceptor::bind(addr)` / `accept() -> Option<TcpTransport>` — one listener
- `Engine::add(transport) -> ConnId` — hand a socket to whichever engine owns that shard
- one `Engine` per thread, each pinned to its own core, each running `turn()` in its own loop

`serve()` is the single-threaded convenience that composes those three. It is the right
starting point and the wrong production shape for a gateway; read it as an example rather than
as the API.

**What you own once you shard.** One of these is now partly provided; the rest are not.

- **Which shard a session lands on.** Round-robin is fine until sessions are unequal; there is
  no rebalancing, and a `ConnId` never moves between engines.
- ~~**Handing the socket across the thread boundary.**~~ `[2026-08-31]` **the runtime does this
  now**: `Shards::hand` sends the `TcpTransport` to the owning thread over a channel that
  `[measured]` makes no syscall and no allocation, and `serve_sharded_hft` is the whole accept
  loop. What is still yours is the line above it — *which* shard.
- **Pinning — the engine now does the pinning, you still choose the core.** `[2026-08-31]`
  `fixbolt_engine::affinity` is behind the `affinity` feature, Linux only. Call
  `pin_current_thread(CoreId(6))` **from inside the thread, as its first act**, and check the
  `Result`: it asks the kernel back with `sched_getaffinity` and returns
  `ReadbackMismatch` if the answer disagrees, so a success really is a success. The engine
  never picks a core for you, and it never will — the OS's idea of a free core does not know
  about `isolcpus`, your NIC's interrupts, or SMT siblings
  ([ADR-0015](decisions/ADR-0015-explicit-cores-pinned-from-inside-and-read-back.md)).
  **Say which cores you mean before you start any thread**, and let the engine refuse the plan:

  ```rust
  use fixbolt_engine::affinity::{CoreId, ShardPlan};
  ShardPlan::new(vec![CoreId(6), CoreId(7)])
      .with_journal_core(CoreId(0))
      .validate()?;                       // before a single thread exists
  ```

  It refuses a core that is absent, offline, named twice, or an SMT sibling of another core in
  the plan, and — for shard cores — one that is not in `isolcpus`.

  **`CoreId(0), CoreId(1)` is the natural first guess and it is wrong on any machine with SMT
  on.** `[measured 2026-08-31]` a GitHub runner reports `cpu0` and `cpu1` as two threads of one
  physical core, and that plan was refused — correctly, on the first machine that could show it,
  since `DESIGN.md` §9 requires SMT off and the reference desktop never can. Use
  `Topology::siblings_of` to take one id per physical core:

  ```rust
  let t = Topology::read()?;
  let mut cores = Vec::new();
  for c in t.online() {
      if cores.iter().any(|taken| t.siblings_of(*taken).contains(c)) { continue; }
      cores.push(*c);
  }
  ``` `allow_unisolated()` waives
  **only** that last rule; a development box needs it and CI needs it. `[measured 2026-08-31]`
  the reason `NotOnline` is a rule of its own: on the tuned reference machine
  `/sys/devices/system/cpu/isolated` reads `6-7,14-15` while `online` reads `0-7`, because
  turning SMT off took 8–15 offline. A plan that trusted `isolcpus` alone would have pinned a
  shard to a CPU the kernel will not schedule.

  **The threads that are not engine threads.** `[2026-08-31]` `ShardPlan` validates their cores
  — a journal writer or ring consumer sharing a core, or a physical core, with a shard is
  refused before anything starts. Putting them there is split:
  `FileJournal::open_pinned(path, Durability::Async, core)` pins the journal's writer and reads
  back the core it landed on (`writer_core()`); the ring consumer is **your** thread — the one
  that calls `RingApp::pump` — so pin it with `affinity::spawn_pinned` or, from inside it,
  `affinity::pin_current_thread`. `Durability::Fsync` has no writer thread and `open_pinned`
  refuses it rather than accepting a core it would ignore.
- **The biggest number on this page is not one you can tune, and you should know it exists.**
  `[measured 2026-09-01]` **the CPU speculation mitigations cost 59–63% of every syscall this
  engine performs.** A turn is **448.9 ns** on a mitigated machine and **175.2 ns** on the same
  machine with them off; thirteen pure user-space benchmarks do not move at all. On this AMD
  Zen 2 box all of it is `retbleed`'s untrained return thunk plus `spec_rstack_overflow`'s Safe
  RET — not `vmscape`, and not the retpolines, which together cost under 1%.

  **This is stated so you can plan against it, not so you turn them off.** Whether a given
  deployment runs mitigated is a security decision for whoever owns the machine and its threat
  model, and this project makes no recommendation about it. What it does say is that a latency
  figure from an unmitigated machine is **not comparable** to any figure in this repository,
  and `scripts/check-machine.sh` will refuse to call such a machine §9-satisfied
  ([ADR-0023](decisions/ADR-0023-section-9-records-the-cpu-mitigations.md)).

  The practical consequence for you: **whichever way your machines are configured, keep them
  the same as the machine your baselines came from.** A fleet that is mixed will show a 2.5×
  spread in syscall cost that has nothing to do with your code.

- **Isolating those cores: `isolcpus` and `rcu_nocbs` — and NOT `nohz_full`.** The first two keep
  other tenants and RCU callbacks off your engine cores and `[measured 2026-08-31]` cost nothing:
  a turn is 494.8 ns per session on an `isolcpus` core, 498.2 on an `rcu_nocbs` one and 501.8 on
  an untouched one.

  **`nohz_full` is the one to leave off, and this is the number to leave it off by.** It adds
  **160 ns to every kernel entry**, which takes a turn to **670.7 ns** — and this engine's idle
  turn is one non-blocking `read` per session, so it is nothing but kernel entries. It is behind
  at p50 (376 ns against 216), at p99 (376 against 224) **and at p99.9** (384 against 224). It
  pulls ahead only from **p99.99** outward, where it is genuinely good: 504 ns against 2848,
  because what it removes is the timer tick and nothing else.

  So: take `nohz_full` **only** if your objective is stated at p99.99 or beyond. If your target
  is a p99 — as this project's own §6 gate is — it costs you on every message to protect one in
  two thousand. [ADR-0021](decisions/ADR-0021-nohz-full-leaves-section-9.md),
  [measured-costs.md](reference/measured-costs.md).

  One thing this does **not** tell you: what `isolcpus` buys under load. It was measured on a
  quiet machine, where there was nothing for it to keep away. Keep it — it is free — but do not
  read a benefit into it that nobody here has measured.

---

## 2. The engine calls you on its hot path

`InlineDispatch` — the default — runs your handler **on the engine thread**, between the read
and the reply. Everything you do there is added to every message's latency, and anything that
blocks stops the session layer.

**Do not, in a handler under `InlineDispatch`:**

| Don't | Why | Costs |
|---|---|---|
| Allocate | Non-negotiable 1 exists because allocation is unbounded, not because it is slow | `[measured]` a `format!` on an error path showed as 30 000 bytes in `benches/alloc.rs` |
| Log, print, or format | Same reason, plus I/O | The engine never logs on its hot path; neither should you |
| Take a lock shared with another thread | The engine thread must not block | Unbounded |
| Do file or network I/O | Every syscall is ~354 ns before it does anything | see §1 |
| `sleep`, or wait on a channel | Non-negotiable 4: the engine thread never sleeps in the kernel | Unbounded, and `scripts/check-no-kernel-sleep.sh` will fail |

**If your handler might do any of those, use `RingDispatch` instead** — your code moves to its
own thread and the engine hands messages across an SPSC ring. You pay for the hop
(`[measured 2026-08-30]` **259.6 ns** one way at N=1 on the §9 machine, against **6.3 ns** for
inline) and you stop being able to stall the session.

**Read [§4](#4-when-the-ring-fills-you-lose-the-connection) before choosing the ring.** It has
a failure mode that inline does not.

---

## 3. `MessageView` borrows the engine's buffer

The view handed to your handler points into the engine's own read buffer. It is 24 bytes and
`Copy`, and it is **valid only for the duration of the call**.

- **Do not store it.** The borrow checker stops you keeping it past the call — that one *is*
  enforced.
- **To keep a message, copy the bytes you need out of it, into storage you own.** Copy fields,
  not the whole message, unless you need the whole message.
- **Do not assume field order** when reading. Ask the view for a tag; do not index positions.
  Field ordering on the *write* path comes from generated tables and never from a call site
  (non-negotiable 5) — the same discipline is worth having on the read path.

---

## 4. When the ring fills, you lose the connection

Under `RingDispatch`, if your thread stops draining, the ring fills — and **the connection is
then ended, deliberately**. `[2026-08-31]`
[ADR-0011](decisions/ADR-0011-a-full-ring-disconnects.md) is implemented.

A message the ring refuses is one the session has already **accepted, numbered, journalled and
acknowledged by sequence number**, and that your application never saw. For order flow that is
not backpressure, it is silent loss. So the engine does not carry on: it sends the counterparty
a `Logout` whose `58=` reads **`slow application`** — deliberately not D10's `slow consumer`,
because your counterparty is behaving perfectly and the fault is on this side — and drops the
session. They can reconnect and reconcile by sequence number, which is something they cannot do
about a message they were told had arrived.

**Three things follow, and the compiler cannot enforce any of them.**

- **A stall is now an outage, not a lag.** An application that pauses under GC or a lock for
  longer than the ring holds drops the session. `[measured 2026-08-30]` at 4 MiB —
  `ring::DEFAULT_CAPACITY`, and the default since ADR-0011 — `[measured 2026-08-31]` that is
  **5.05–5.36 ms** over four runs on one tuned Linux desktop, against **47.7 µs** at the old
  64 KiB. **Nobody has measured a real application's worst pause**, so that is a budget, not a
  guarantee, and it is one machine's number: a faster box fills the ring faster and has less
  slack, not more.
- **The `Logout` is queued, not sent, on the turn the ring refuses.** It goes out on the next
  flush, exactly as D10's path does. If you drive `turn()` yourself and stop the moment a
  connection looks doomed, **you never send it** and the counterparty learns nothing. Keep
  turning until `connections()` drops.
- **Watch `Engine::refused_connections()`.** Non-zero means sessions were dropped because your
  side could not keep up. Wire it to something a human sees — a metric, an alert, a log on the
  *cold* path. `RingDispatch::refused()` counts the same events from the dispatch's side.

And still: **size the ring for your stall, not your throughput.** The question is not "how many
messages per second" but "what is the longest my consumer can be away".

---

## 5. Time enters as a tick, and it is yours to supply

The session layer takes no clock (D1). Time arrives as `Input::Tick`, in milliseconds since
`0000-01-01`, and the session judges `SendingTime` against **the last tick it was given**.

- **A session that has never ticked holds zero**, and will refuse the first message it sees for
  clock skew. The engine ticks before it reads for exactly this reason.
- **If you drive the engine yourself** — as `crates/engine/tests/wire.rs` does — you own that
  ordering. Tick first, then read.
- **Do not format a timestamp per message.** The outbound path patches a cached one;
  `[measured 2026-08-30]` `SendingTime from the cache` is **4.9 ns**. Building one from scratch
  is orders of magnitude more and is a hot-path allocation waiting to happen.

---

## 6. Journalling: pick the policy deliberately

Three policies — `None`, `Async`, `Fsync` — and the difference is which failure they survive:

| Policy | Cost | Survives |
|---|---|---|
| `None` | zero | nothing |
| `Async` | one `write` per message | **a process crash**, not power loss |
| `Fsync` | a disk sync per message | power loss |

`[2026-08-30]` for reference, QuickFIX's `FileStore` flushes without `fsync` — its durability
class is `Async`, in both directions. See
[reference/session-lifecycle-prior-art.md](reference/session-lifecycle-prior-art.md).

**`Fsync` puts a disk on your hot path.** That is a deliberate trade and sometimes the right
one; it is not a default to reach for without measuring what it costs you. `[2026-08-31]` it now
costs on **both** directions: since ADR-0017 the journal also records which inbound sequence
numbers have been consumed, so under `Fsync` receiving a message pays a `sync_data` too.
Nothing here has measured that yet, and it is stated so you are not surprised by it rather than
because a number exists.

### 6a. Your application must be idempotent per sequence number

**The engine can deliver the same message twice, and after a restart it sometimes will.**
[ADR-0017](decisions/ADR-0017-the-inbound-count-is-persisted-after-delivery.md): the inbound
count is written down **after** your handler has seen a message, not before. A crash in that
window means the count on disk is behind what your handler actually processed, so on restart the
session asks for a resend and your handler sees it again.

That is the deliberate choice, and the alternative is worse. Writing the count first would close
that window by opening a bigger one: the message would be **lost** instead — this end would have
counted it, so it would never ask for a resend, and your counterparty would believe it arrived.
FIX gives you a way to detect the duplicate and none at all to detect the loss.

**What you must do:**

- **Key on the sequence number, not on arrival.** The `seq` your handler is given is the
  counterparty's, and it is stable across a replay. Deduplicate on it if a repeat would be
  harmful — a duplicate order, a duplicate cancel.
- **A replayed message carries `43=Y`.** If you only need to *notice* a repeat rather than
  suppress it, that flag is the signal, because the second copy arrives in answer to a
  `ResendRequest` this engine issued.
- **The window is moved, not closed, and no engine can close it.** Nothing spans your handler's
  side effects and this engine's disk atomically. Anyone who tells you otherwise has moved the
  problem into your database's transaction, which is a fine place for it — but it is your
  transaction, not the engine's.

---

## 7. The machine is part of your latency, and mostly it is not the tuning

`scripts/check-machine.sh` reads `DESIGN.md` §9 off the running box and tells you which rows
are not in force. Run it before you believe any number, yours or ours.

`[measured 2026-08-30]`, in order of how much they actually moved a benchmark here:

| Factor | Effect on the ring-hop median |
|---|---|
| **Anything else running on the machine** | **+71%** — 262 ns to 449 ns |
| Every §9 tuning row combined — governor, boost, SMT, THP, `busy_poll` | **0.8%** |

**The biggest one is not on the checklist by default and is free: make the machine quiet.**
A box that satisfies every tuning row and shares a core with a build is worse than an untuned
idle one.

Two more, both traps rather than settings:

- **A VM cannot satisfy §9.** Governor, turbo, C-states, SMT and NIC IRQ affinity are *host*
  properties. A guest does not fail those rows loudly — the `/sys` files are simply absent.
  Develop on a VM if you like; do not measure on one.
- **`scaling_cur_freq` lies on an isolated core.** With `nohz_full` the governor's tick stops,
  the file freezes, and it read **41% low** here while the core ran at full speed. Measure work
  per unit time instead.

---

## 8. How to benchmark this engine without fooling yourself

Everything in this section was paid for here, on this repository, in one day.

1. **Run your sample twice before you write down a rate.** `[measured 2026-08-30]` a
   pass-rate comparison read 8.3% against 3.3% at n=60 and **5.6% against 5.6% at n=250**.
   Medians reproduce at small n; *threshold crossings* do not, and a ceiling near the median
   turns a small shift into a coin flip.
2. **Plot the distribution before you explain it.** A histogram of 500 runs here showed **two
   discrete modes with an empty gap between them** — one value in 500 lay between the clusters.
   Eight hypotheses about the cause were proposed and refuted; the *shape* was what turned out
   to be actionable, and it cost one plot.
3. **Check the instrument against a known state before believing a surprising reading.** Four
   instruments failed here in one day: `ps %CPU` (a lifetime average, not current), a sampler
   slow enough to distort its own window, a "quietness" sampler whose wait loop was a busy
   spin, and `scaling_cur_freq` on a `nohz_full` core.
4. **A failed intervention still produces two clean-looking arms.** One A/B here compared a
   setting against itself because the command that was supposed to change it had errored. The
   arms agreed beautifully. Check that the variable actually moved.
5. **Measure a whole turn, including its syscalls.** Timing only the user-space part is how
   §8's budget came to exclude the syscall that dominates it — ~420 ns of a 449 ns turn.

Longer versions of all five, with the numbers:
[reference/measured-costs.md](reference/measured-costs.md).

---

## 9. What this engine does not do for you

Stated so you do not discover it in production:

- **It does not validate application-message semantics.** The dictionary validation is
  session-layer: required fields, types, enum values, structure. Whether a `NewOrderSingle`
  makes business sense is yours.
- **It does not resume sequence numbers across a restart yet.** `connect` resets
  unconditionally; [ADR-0010](decisions/ADR-0010-a-reconnect-is-not-a-restart.md) is the
  decision to change that — **`Accepted` 2026-08-30, not yet implemented**.
- **It has no session schedule.** Start time, end time and weekday resets are a known gap
  (`PRD.md`), so nothing ends a session on a clock.
- **It does not pin its own threads.** `DESIGN.md` D8 says the engine thread is pinned to an
  isolated core; `[2026-08-30]` nothing in the code does that — `STATUS.md` open item 21.
  **Pin it yourself**, with `taskset` or `sched_setaffinity`, or D8's premise does not hold.
- **It is not TLS-complete.** ADR-0005 is accepted on reasoning; the kTLS question is only now
  answerable.
