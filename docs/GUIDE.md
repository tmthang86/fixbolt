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

| Sessions on one thread | Added latency, worst case | Against a measured 16.0 µs round trip |
|---|---|---|
| 1 | 0.45 µs | 2.8% |
| 2 | 0.90 µs | exceeds `DESIGN.md` §8's entire user-space budget (~0.46 µs) |
| 16 | 7.2 µs | **45%** |
| 128 | 57 µs | 3.6× the whole round trip |

`[2026-09-02]` **This table used to be built on 703 ns and is now built on 449.** The older
figure was a C program's bare `read` on a `nohz_full` core; §9 stopped asking for `nohz_full`
([ADR-0021](decisions/ADR-0021-nohz-full-leaves-section-9.md)) and `Engine::turn` on the line
§9 now describes is **449 ns**. **The isolated core is not the expensive one** — that 36% was
`nohz_full`'s and only `nohz_full`'s: `isolcpus` reads 494.8 ns against 501.8 on an untouched
core, which is nothing.

**If you care about latency, run one session per thread and pin that thread to an isolated
core.** If you are building a gateway for many clients, you are in the `density` shape — that
is supported and reasonable, and you should plan against `N × 449 ns` on a §9 core
rather than against this project's headline figures ([ADR-0012](decisions/ADR-0012-latency-first-and-one-session-per-polling-thread.md)).

### And the pinning is the one thing here that only shows up in the tail

`[measured 2026-09-02]` **whether the engine thread sits on an `isolcpus` core is worth nothing
at p50 and 11× at p99.9**, measured wire-to-wire on a §9 desktop with one variable between the
arms:

| Where the engine thread ran | p50 | **p99.9** |
|---|---|---|
| pinned to an isolated core | 19 968 ns | **26 300 ns** |
| pinned to an ordinary core | 19 407 ns | **266 887 ns** |
| not pinned at all | 19 607 ns | **293 749 ns** |

The p50 column is why this is easy to get wrong: the isolated core is 2.9% **slower** at the
median, so a benchmark reporting medians says the tuning is pointless. What it prevents is the
scheduler putting something else on your core for a quarter of a millisecond, and **you will
only ever see that in a percentile long enough to contain it.**

**Nothing enforces this either.** `pin_current_thread` will pin you to a core the scheduler
shares and return `Ok` — it proves your thread went where you said, not that the core is yours.
Check `/proc/cmdline` for `isolcpus`, or run `scripts/check-machine.sh`.

**Nothing enforces this.** The engine will happily carry 500 sessions on one thread and will
not warn you.

**And kernel bypass does not rescue it.** `[measured 2026-08-31]` removing the syscall — Onload,
`ef_vi`, DPDK — takes **~420 ns of the 449** down to the cost of a memory read, which is real and
worth having.
Two terms survive it:

- **Cache.** One `Connection` is **53.3 KiB**; `L1d` on the test machine is 32 KiB, so *one
  connection does not fit in L1*. Random access costs **1.05 ns** in L1 and **78.5 ns** from
  RAM — **75×** — and that applies to every access the engine makes, not just to polling.
- **Head-of-line blocking.** One thread serialises. Per-message work here is ~460 ns
  (`parse` 122.6 + `encode` 239.1 + the session step), so `k` sessions holding a message at
  once make the last one wait `(k-1) × 460 ns`. Nothing removes this except fewer sessions.

Full working: [reference/measured-costs.md](reference/measured-costs.md).

---

## 1a0. Many counterparties: one registry, and nothing has a default

`[2026-09-01]` **This acceptor used to serve exactly one counterparty**, because every entry
point took one `Config` and a `Config` pins one `TargetCompID`. It takes a
**`presession::Table`** now — one entry per counterparty — and a socket is held until its
`Logon` says who it is
([ADR-0026](decisions/ADR-0026-a-counterparty-registry-in-the-pre-session-stage.md),
[ADR-0030](decisions/ADR-0030-one-engine-holds-many-counterparties.md)).

```rust
use fixbolt_engine::presession::{Limits, Table};
use fixbolt_session::Config;

let table = Table::with_capacity(2)
    .serving(Config::acceptor(b"FIX.4.4", b"US", b"ALPHA"))
    .serving(Config::acceptor(b"FIX.4.4", b"US", b"BETA"));

fixbolt_engine::serve(
    "0.0.0.0:9876",
    table,
    MyApp::default(),
    64,                                    // connection capacity
    Limits::new(64, 30_000).expect("neither is zero"),
)?;
```

**Three things about this that are decisions, not defaults.**

| | |
|---|---|
| **An empty table refuses every connection** | And `serve` refuses to start on one — `ServeError::NoCounterparties`. There is no wildcard entry and none is planned: an acceptor that admits an identity nobody configured is an open port, which is exactly what QuickFIX/J's `ANY_SESSION` template is |
| **A refused identity is told nothing** | The socket closes with no `Logout` and no `Reject`, which is what `1c_InvalidSenderCompID.def` and `1c_InvalidTargetCompID.def` expect. It is indistinguishable from a wrong password on purpose |
| **The refusal *is* the authentication hook** | There is no separate `AuthStrategy`. When a credential check on `553`/`554` arrives it goes in the `Entry`, behind the same `lookup` — two hooks answering *"may this counterparty in"* are two rules that will disagree |

**`Table` keys on the comp IDs, and that is one implementation of a trait.**
`Registry::lookup(Identity) -> Option<&Entry>` is the seam. If your counterparties are told
apart by `50=`/`57=`, or live in a file, or in a snapshot of a database, write your own — it
is about eight lines, and `crates/engine/tests/registry.rs` has a worked one. Two rules for
whatever you write:

- **It must not allocate.** `lookup` is on the connection path and `benches/alloc.rs` asserts
  the whole pre-session stage is zero. Borrow from what your registry already owns.
- **It must answer immediately.** It runs on the acceptor thread. A remote entitlements
  service is a denial-of-service surface no logon deadline closes; snapshot it out of band.
  This is where the design deliberately parts from Artio's `authenticateAsync`.

**One engine holds all of them.** The registry decides which *configuration* a connection
gets, not which engine — and *"this identity is already logged on"* is answered by comparing
identities, not by counting connections
([ADR-0030](decisions/ADR-0030-one-engine-holds-many-counterparties.md)). Two counterparties
on one engine are two sessions; two connections claiming one counterparty are one session and
a disconnect.

### The configuration file

`[2026-09-02]` you do not have to build the table in code
([ADR-0040](decisions/ADR-0040-a-configuration-file-refuses-what-it-does-not-understand.md)):

```ini
[DEFAULT]
BeginString=FIX.4.4
SenderCompID=US
StartTime=08:00:00
EndTime=17:00:00
Weekdays=Mon,Tue,Wed,Thu,Fri

[SESSION]
TargetCompID=ALPHA

[SESSION]
TargetCompID=BETA
HeartBtInt=60
StartTime=00:00:00
EndTime=23:59:59
```

```rust
let table = fixbolt_engine::settings::Settings::load("acceptor.cfg")?.into_table();
```

`[DEFAULT]` supplies every `[SESSION]` after it; a `[SESSION]` overrides its own. The shape is
QuickFIX's, on purpose. **The behaviour differs from QuickFIX's in three ways, and each is
deliberate:**

| | |
|---|---|
| **An unrecognised key is an error** | QuickFIX ignores what it does not know. Here a mistyped `Starttime` would fall back to `Schedule::always()`, and a session that should close at five would stay open all night with nothing saying so |
| **A file naming no counterparty is an error** | An empty `Table` refuses every connection, so a mistyped path behaves exactly like a firewall dropping your port |
| **A half-written schedule is an error** | `StartTime` with no `EndTime` is refused rather than completed with midnight. So is a `StartDay` with no hours — a key spelled correctly that has no effect |

Every error carries the line it is on and quotes what was written, because the person editing
this file does not read Rust:

```text
line 14: unknown key: Starttime
```

**A value longer than a `Config` can hold is refused, not truncated.** `Config` records an
over-long name as *not fitting*, and a name that does not fit matches nothing — so truncation
would give you an acceptor that starts cleanly and serves nobody. The limits are
`fixbolt_session::MAX_BEGIN_STRING_LEN` and `MAX_COMP_ID_LEN` if you need to check a value
before writing it.

**What the file still cannot say, and you will notice:** no credential (ADR-0026 decision 3
makes `lookup` the only authentication hook, and a password field would be a second one), no
per-counterparty journal path (that belongs to `Recovery` —
[ADR-0039](decisions/ADR-0039-a-fresh-journal-is-the-deployments-to-build.md) — not to the
registry), no `50=`/`57=`, and no reload while running: the table is read-only after startup.
There is also no `UtcOffsetMillis` key, deliberately — see §5a, where the reason is that a
fixed offset put in a settings file looks like a setting rather than like the hazard it is.

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

**Everything below is what is still yours to decide, and what the engine will not decide
for you.**

**Building it by hand, if `serve_sharded_hft` is not the shape you want.** `Engine` holds a
flat `Vec<Connection>`, `turn()` sweeps all of them and `run()` is `loop { turn() }`, so one
`Engine` is one shard. The pieces are separate on purpose:

- `Acceptor::bind(addr)` / `accept() -> Option<TcpTransport>` — one listener
- `Engine::add(transport) -> ConnId` — the engine's own `Config`, for a link with one
  counterparty. For an acceptor, `Engine::add_with_prefix_and_config(transport, cfg, prefix)`:
  the configuration the registry chose, and the bytes the pre-session stage already read
- one `Engine` per thread, each pinned to its own core, each running `turn()` in its own loop

**If you do compose them yourself, you inherit the defect `serve_sharded_hft` was changed to
fix**: routing a socket before its `Logon` is read cannot keep one identity on one engine, and
the acceptance corpus scores 57 rather than 59 when it does. Read `presession::identity_of`
and `Route` before writing an accept loop of your own.

`serve()` is the single-threaded convenience that composes those three. It is the right
starting point and the wrong production shape for a gateway; read it as an example rather than
as the API.

**What you still own once you shard.**

- **Which shard a session lands on, if the default hash is not what you want.** `HashRoute` is
  stable across processes and spreads by identity; it does **not** rebalance, it does not know
  that one counterparty sends a hundred times more than another, and a `ConnId` never moves
  between engines. `[2026-09-01]` **round-robin is not an option any more** — it is the policy
  that produced the single-logon defect above, and it was deleted rather than documented.
- ~~**Handing the socket across the thread boundary.**~~ `[2026-08-31]` **the runtime does this
  now**: `Shards::hand` sends the `TcpTransport` to the owning thread over a channel that
  `[measured]` makes no syscall and no allocation, and `serve_sharded_hft` is the whole accept
  loop.
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
  ```

  `allow_unisolated()` waives
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
- **Your `[profile.release]`, not ours.** Cargo honours a profile only from the top-level
  package being built, so **this project's profile does not reach you** — it is cargo's default
  and `benches/baselines.tsv` describes a default build
  ([ADR-0024](decisions/ADR-0024-the-workspace-keeps-the-default-release-profile.md)). Yours
  does reach your binary, and `[measured 2026-09-01]` on the §9 desktop it is worth:

  | Setting | syscall-bound path | pure computation | clean build |
  |---|---|---|---|
  | `lto = "thin"` | −2 … −3% | −8% … +1% | 5.2 s → 17.1 s |
  | `lto = "fat"` | **−3 … −5%** | −31% … +12% | → 15.9 s |
  | `codegen-units = 1` | −0 … −2% | −17% … +2% | → 5.2 s |
  | both | **−3 … −6%** | −30% … +12% | → 16.3 s |

  **Read the caveat before you plan against those numbers.** A benchmark is a separate crate
  calling into the library, so LTO inlines library internals into the *benchmark loop*; some of
  the win above is that boundary, and your application may not have it in the same place. The
  30% case is a small pure function that production already calls from within its own crate.
  **How much survives into a real application was not measured**, and this project will not
  claim a figure it does not have. The syscall-bound row is the more trustworthy half, because
  kernel time cannot be inlined away.

  Note also the two regressions in that table — `SendingTime from the cache` is **+12%** under
  fat LTO. Whole-program optimisation is not free in one direction.

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

## 1b. Two ways to write a handler, and the fast one is not the pretty one

`[2026-09-02]` there are now two, and **choosing between them is a latency decision, so make
it deliberately.**

### The short way: `fixbolt::Handler`

One crate in your `Cargo.toml`, a message already parsed, and a reply that writes the seven
fields you do not own:

```rust
use fixbolt::{Answer, Handler, Incoming, Reply};

struct Desk;

impl Handler for Desk {
    fn on_message(&mut self, msg: &Incoming<'_>, reply: Reply<'_>) -> Answer {
        if msg.msg_type() != b"D" {
            return reply.silent();
        }
        reply
            .message(b"8")
            .field(37, b"EXEC-1")
            .field(150, b"F")
            .field(11, msg.get(11).unwrap_or(b""))
            .send()
    }
}

fixbolt::serve(addr, table, fixbolt::app(Desk), 64, limits)?;
```

**What you did not write, and cannot get wrong.** `8`, `9` and `10` are the frame. `34` and
`52` are the session's — an application that regenerates `52` moves the body four bytes and
fails a test that says nothing about time. `49` and `56` are **reversed**: your sender is
their target. None of the seven is reachable from the API above, which is the point; naming
one anyway is ignored rather than merged, because two `34=` in one message is two sequence
numbers.

**And the order is not yours.** Everything you name goes through the generated tables:
`MsgType`, then header tags ascending, then body tags ascending. Name them in any order you
like.

### The fast way: `fixbolt::Application`

The raw seam. Bytes in, a `Range<usize>` out, and everything above is yours to do:

```rust
impl Application for Desk {
    fn on_message(&mut self, msg: &[u8], seq: u32, stamp: &[u8], out: &mut [u8])
        -> Option<Range<usize>> { /* ... */ }
}
```

`crates/conformance/src/echo.rs` is a worked example, and the comment at the top of it is the
list of traps you are now responsible for.

### What the short way costs

`[measured 2026-09-02]` on an **Intel Xeon @ 2.80GHz — a shared cloud VM that does NOT meet
§9**, so read the ratio and not the figure. One twelve-field `ExecutionReport`:

| | ns/op |
|---|---|
| Encode a `Template` you built **once** — [D9](DESIGN.md)'s shape | **40** |
| `App::on_message`: parse, build a template, encode | `[2026-09-02]` **956**, and it was 2 062 – 2 131 until `TemplateBuilder` stopped moving itself once per field ([ADR-0044](decisions/ADR-0044-a-builder-that-is-not-moved-per-field.md)) |
| …of which the second parse is | **146** |

**About 24×, and it was 50× this morning.** The parse is the small half. What is left is that a
`Template` is **materialised per message** — sorted, and its scratch laid out — where D9's shape
builds it once; `TemplateBuilder` no longer copies itself once per field, which was the other
half ([ADR-0044](decisions/ADR-0044-a-builder-that-is-not-moved-per-field.md)).
`crates/library/benches/cost.rs` is the benchmark and
[ADR-0041](decisions/ADR-0041-the-library-layer-buys-an-api-with-a-template-per-message.md) is
the decision that published the original ratio, including what would remove the rest.

**So:**

| If your deployment is | Use |
|---|---|
| `standard` mode, order entry, a few thousand messages a second | `Handler`. Two microseconds is not your problem |
| `hft` mode, one session on an isolated core | **`Application`**, with your own `Template` per message type, built at logon. Two microseconds is more than the rest of the message costs put together |

There is no third option where you get both, and pretending otherwise is what this section
exists to prevent.

### The two sizes, and the cliff

`Handler<N, P, S>` defaults to `256, 64, 1024`: `N` fields in the inbound index, `P` fields in
a reply, `S` bytes of them. A reply that exceeds `P` or `S` is **`Answer::Failed`, not a
slower success** — nothing goes on the wire, and `App::failed_replies()` counts it. That
counter is the only way you will find out, so read it.

The defaults were measured rather than picked: 128/4096 costs 1.9× as much, and below `S=512`
the curve flattens. If you know your message, say so — `impl Handler<256, 32, 512> for Desk`.

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

## 5a. Session schedules, and the timezone trap

`[2026-09-02]` A FIX session is not open all the time, and when it closes **both ends start
again at `34=1` the next morning**. That is protocol, not housekeeping: get it wrong and you
spend the morning arguing sequence numbers with your counterparty.

```rust
use fixbolt_session::schedule::{Schedule, Weekdays};

let hours = Schedule::daily(8 * 3_600, 17 * 3_600)   // seconds since midnight, UTC
    .expect("08:00 is before 17:00")
    .with_weekdays(Weekdays::WEEKDAYS)
    .expect("Monday to Friday");

let cfg = Config::acceptor(b"FIX.4.4", b"ISLD", b"TW44").with_schedule(hours);
```

Without `with_schedule` a session is open forever and never resets, which is what every
session did before this existed. That default is deliberate and exactly neutral.

### The trap: these are UTC, and a fixed offset is not daylight saving

**`fixbolt` has no timezone database and never will** —
[ADR-0033](decisions/ADR-0033-a-schedule-is-utc-arithmetic-and-the-calendar-stays-outside.md).
The session layer is pure (D1), and an IANA database is a dependency that allocates.

So if your venue says *"17:00 America/New_York"*:

1. Resolve that to a UTC offset **with your own zone library**, for the date in question.
2. Build the `Schedule` with `with_utc_offset_ms`.
3. **Rebuild it when the offset changes.** New York is `-5h` in winter and `-4h` in summer.

```rust
// A venue seven hours east of UTC, which does not observe DST.
let jakarta = Schedule::daily(9 * 3_600, 16 * 3_600)
    .expect("legal")
    .with_utc_offset_ms(7 * 3_600 * 1_000)
    .expect("inside a day");
```

**A `Schedule` built from one DST offset is wrong for half the year**, and the failure is not
loud: it resets sequence numbers an hour early or an hour late, on exactly the two days a
counterparty is least forgiving. Nothing in the type system can catch it. If your venue
observes DST, something in your deployment must rebuild the `Schedule` twice a year, and that
something is yours.

### A session may run past midnight

`open > close` wraps and is legal — 22:00 to 06:00 is **one** session, so nothing resets at
midnight in the middle of it. A weekday filter selects the day a session **opens** on, so a
Friday-night window under `Weekdays::WEEKDAYS` runs into Saturday morning as it should.

For a week-long window use `Schedule::weekly`: Sunday 21:00 to Friday 21:00 is one interval,
and Tuesday night is inside it.

### You must persist *when*, not only *what*

This is the part that is easy to get half-right.

```rust
// Wrong across a boundary: carries the numbers, says nothing about the calendar,
// and therefore NEVER resets.
let s = Session::resume(cfg, next_out, next_in);

// Right: carries the numbers AND when they were last touched.
let s = Session::resume_at(cfg, next_out, next_in, last_active_ms);
```

`next_out = 41` tells you nothing about whether a trading day has ended since 41 was reached.
So persist `Session::last_active_ms()` beside the sequence numbers and hand it back. A session
resumed without it will not reset, ever — which is correct for `Schedule::always()` and wrong
for everything else.

`[2026-09-02]` **`Engine::add_resumed` is how you hand it over**, and it takes the journal
too:

```rust
let next_out = journal.highest().map_or(1, |h| h + 1);
let next_in  = journal.highest_in().map_or(1, |h| h + 1);
engine.add_resumed(transport, cfg, journal, next_out, next_in, Some(last_active_ms));
```

Pass `None` for the last argument and no boundary is ever noticed — right under
`Schedule::always`, wrong under anything else.

**If you use `serve` rather than driving the engine yourself**, hand it a `Recovery` instead:

```rust
use fixbolt_engine::recovery::{FromFn, Resumed};

let recovery = FromFn::new(|cfg: &Config| Some(Resumed {
    journal:        my_journal_for(cfg),          // yours to open
    next_out:       my_next_out_for(cfg),
    next_in:        my_next_in_for(cfg),
    last_active_ms: my_last_active_for(cfg),
}));
fixbolt_engine::serve_with_recovery(addr, table, app, capacity, limits, recovery)?;
```

It is asked **once per connection, after the registry has named the counterparty** — before
the `Logon` there is no identity to look anything up by. That happens on the acceptor thread,
which is allowed to block, so reading a file there is fine. **A network round trip is not**:
every connection behind it waits, and the only backstop is the pending deadline, which refuses
the socket without saying why.

Returning `None` starts that session fresh, which is exactly what plain `serve` does.

**Three limits, and they are the difference between this working and looking like it works.**
`serve_sharded_hft` has **no** recovery variant, so a sharded deployment cannot resume. The
serving loop fixes the journal type as `journal::Store`, so a per-counterparty `FileJournal`
through `serve_with_recovery` is not yet possible. And **nothing persists `last_active_ms` for
you** — save `Session::last_active_ms()` beside your sequence numbers, or the instant is gone
with the process and the boundary becomes undecidable.

### What the reset is decided by

A comparison, not a clock alarm: *do the last instant I remember and now fall in the same
interval?* That is the only question an engine which was asleep at midnight, or started at
06:00, can still answer — and those are precisely the times a reset matters.

An instant your schedule cannot place is **never** the same session as anything, so an engine
that cannot tell resets rather than carrying numbers across a boundary it could not see.
Resetting when your counterparty did not is a `Logon` argument you see at once; not resetting
when they did is a silent divergence you find much later.

### What is not here

No `ResetSeqTime` — the reset is tied to the interval boundary, not to a separate hour. No
timezone names. And the `Logout` sent when your window closes carries **no `58=` text**, so
your counterparty learns that you went away and not why; FIX makes the text optional and
QuickFIX sends none here either.

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

### 6a0. How big the resend ring has to be, and what it costs

`[2026-09-04]` [ADR-0046](decisions/ADR-0046-the-ring-is-the-resend-store-and-a-replay-goes-in-batches.md).
**The in-memory ring is the whole resend store.** Disk is for restart and audit; the engine
thread never reads a file to answer a `ResendRequest`, because that is a blocking `read` on the
thread non-negotiable 4 protects. Anything older than the ring is gap-filled — legal on the
wire, and gone as far as your counterparty is concerned.

So the size is yours to choose, and this is the arithmetic:

> **N ≥ the number of application messages you send during the longest disconnection you are
> willing to replay across** — for most desks, one trading day.

`Store` is `MemJournal<4096, 512>` and costs `N × (SLOT_LEN + 8)` ≈ **2 MiB per session**.
A gateway holding hundreds of sessions should pick a smaller N through the const generic; §1a is
about that trade. **The messages a resend cannot reach are not lost quietly:**

| What you see | What it means | What to do |
|---|---|---|
| `SessionSnapshot::resend_beyond_journal` non-zero, or `EventKind::ResendBeyondJournal { filled, oldest }` | a counterparty asked for `filled` messages the ring no longer held, and got gap fills. `oldest` is how far back it reached | raise N, or accept that disconnections longer than N messages lose data |
| `SessionSnapshot::puts_refused` non-zero, or `EventKind::JournalRefused { count }` | your replies are longer than `SLOT_LEN`. They went out; they can never be replayed | raise `SLOT_LEN`, and re-check `resend_batch × SLOT_LEN < TX` |

**`tools/jrnl` is how you get a message older than the ring** — by hand, from the file, off the
engine thread. That is deliberate and it is the only way.

**Two constraints the type system cannot hold for you:**

- **`resend_batch × SLOT_LEN` must stay under `TX`.** The default is 8 × 512 = 4 KiB against
  8 KiB. Raise `SLOT_LEN` or lower `TX` and this is the number to re-check — a batch that does
  not fit is the defect ADR-0046 fixed, arriving again through configuration.
- **In `hft`, pre-build journals and call `add_with_journal`.** Plain `Engine::add` builds
  `J::default()`, which is a ~2 MiB allocation and 512 page faults **on the engine thread**.
  `docs/best-practices-hft.md` §6.

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

### 6b. A journal on disk, through the serving loop

`[2026-09-02]` `serve_with_recovery` is generic over the journal, so one `FileJournal` per
counterparty is reachable without giving up the serving loop
([ADR-0039](decisions/ADR-0039-a-fresh-journal-is-the-deployments-to-build.md)).

```rust
impl Recovery<FileJournal<64, 4096>> for OnDisk {
    // Called when the counterparty left nothing. THIS is why it exists: the
    // engine cannot build a FileJournal for you — only you know the path.
    fn fresh(&mut self, cfg: &Config) -> FileJournal<64, 4096> { /* open it */ }

    fn recover(&mut self, cfg: &Config) -> Option<Resumed<FileJournal<64, 4096>>> {
        let journal = self.fresh(cfg);
        let next_out = journal.highest().map_or(1, |h| h + 1);
        Some(Resumed {
            next_in: journal.highest_in().unwrap_or(1),
            last_active_ms: journal.last_active(),   // ← the boundary question
            journal,
            next_out,
        })
    }
}
```

**`last_active()` is the field people skip, and it is the one that matters after a weekend.**
`next_out = 4812` says nothing about whether a trading day ended since 4812 was reached, so
without it §5a's boundary reset has nothing to compare against and your session silently keeps
yesterday's numbering. The engine records the instant when a session logs on and when an
ordered shutdown says goodbye — **not per message**, because that would be a disk write on the
hot path.

**Four things to know.**

1. **`None` from `last_active()` means *"this journal does not know"***, not *"the session was
   never active"*. An in-memory journal answers `None`, and so does a file written before this
   existed. Treating them alike restarts a session's numbering without meaning to.
2. **A process killed between logon and shutdown reports the logon instant**, which after a
   long session may be a day stale. There is no periodic mark.
3. **Nothing stops two processes opening the same file.** Both append and the records
   interleave. There is no lock; one journal, one process.
4. **`NoRecovery` and `FromFn` require `J: Default`**, so neither can carry a `FileJournal`.
   A file-backed deployment writes a named type — which it has to anyway, since only it knows
   which path belongs to which counterparty.

---

## 7. The machine is part of your latency, and mostly it is not the tuning

`scripts/check-machine.sh` reads `DESIGN.md` §9 off the running box and tells you which rows
are not in force. Run it before you believe any number, yours or ours.

`[measured 2026-08-30]`, in order of how much they actually moved a benchmark here:

| Factor | Effect on the ring-hop **median** |
|---|---|
| **Anything else running on the machine** | **+71%** — 262 ns to 449 ns |
| Every §9 tuning row combined — governor, boost, SMT, THP, `busy_poll` | **0.8%** |

**The biggest one is not on the checklist by default and is free: make the machine quiet.**
A box that satisfies every tuning row and shares a core with a build is worse than an untuned
idle one.

`[measured 2026-09-02]` **And one row moves nothing in that column and 11× in the one beside
it.** Pinning the engine thread to an `isolcpus` core is worth **−2.9% at p50** — it is
*slower* — and **10× at p99.9**, 26 µs against 267. So the table above, which is about medians,
cannot rank it at all; §1 has the figures. **If you tune for the tail, do not rank your
settings by their effect on the median**, which is the mistake this project made about
`isolcpus` for two days and about `nohz_full` in the opposite direction for one.

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

0. **Match the instrument's timescale to the mechanism you are asking about.**
   `[measured 2026-09-02]` this project measured an isolation setting as *free, with an
   unproven benefit* for two days, using a benchmark that timed a **500 ns** operation. The
   stall that setting prevents is **250 µs** long. It does not appear in that benchmark as a bad
   p99.9; it appears as one absurd sample in a distribution nobody reads to the end, or it gets
   discarded as an artifact. **A percentile is a property of the instrument as much as of the
   system**, and a micro-benchmark has no far tail to report. Ask how long the mechanism's
   stall is before choosing what to measure with.

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

## 8a. Watching a running engine

`[2026-09-01]` The engine tells you what it is doing **only when you ask**, and what asking
costs while nobody is asking is one relaxed load per turn. That is the whole shape
([ADR-0032](decisions/ADR-0032-observation-is-a-snapshot-taken-on-request.md)).

```rust
let mut engine = /* ... */;
let watch = engine.observer();          // the one allocation this mechanism makes

std::thread::spawn(move || {
    loop {
        if let Some(s) = watch.request() {
            for sess in s.sessions() {
                println!(
                    "conn {} logged_on={} out={} in={} skew={:?}ms pending={}",
                    sess.id(), sess.logged_on(), sess.next_out(),
                    sess.next_in(), sess.last_skew_ms(), sess.has_pending_output(),
                );
            }
        }
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
});
```

**Four things about `request()` that will otherwise surprise you.**

1. **It never blocks, in either direction.** It hands back the most recent snapshot the
   engine published and asks for a fresh one. A snapshot is therefore always a moment old.
   If you need one taken *after* your call, call twice.
2. **It returns `None` until the engine has published at least once.** On an idle engine
   that is one turn away; on a stopped one it is forever. `None` means *"not yet"*, never
   *"no sessions"* — an engine with no sessions publishes a `Snapshot` whose `sessions()` is
   empty.
3. **The engine may skip a publish** if you happen to hold the cell at that instant. It does
   not wait for you (non-negotiable 4), and your request stays standing, so the next turn
   does it.
4. **Do not poll it in a tight loop.** Every call makes the engine build a snapshot on its
   next turn. Once a second is an operator; ten thousand times a second is a load generator
   pointed at your own hot path.

### The one field people forget, and it is the one that saves you

`SessionSnapshot::last_skew_ms()` is **your clock minus theirs**, in milliseconds, from the
last inbound message whose `52=` could be read — **whether that message was accepted or
refused**.

`Config::max_skew_ms` refuses a message whose `SendingTime` is too far from your clock, and
before a `Logon` the protocol says to refuse **in silence**. So a box whose NTP has drifted
does not produce an error: the counterparty simply stops working, and nothing else in this
engine would ever tell you why. Watch this number, and alert on it long before it reaches
`max_skew_ms`.

### Health

`Snapshot::healthy()` is a pure function on the same data: at least one session, every one
of them logged on, and both should-be-zero counters at zero
(`refused_connections()` — a full ring dropped a session, ADR-0011 — and
`sources_missing()` — a `Transport` broke its own contract). Wire it to whatever probes your
process; there is no second mechanism to disagree with it.

`truncated()` is deliberately **not** unhealthy: `MAX_SESSIONS` is 64 and `standard` has no
session ceiling, so it means *"there were more than I can list"*, not *"something is wrong"*.

### Why a connection ended

`[2026-09-02]` A snapshot tells you what **is**. It cannot tell you what **happened** — by
the time you ask, the session that ended is gone from it. So endings are **pushed**: the
engine records one when a session's state changes, whether or not anybody is reading
([ADR-0035](decisions/ADR-0035-an-event-is-pushed-and-a-loss-is-counted.md)).

```rust
let mut events = Vec::new();            // yours, on your thread; it may allocate
loop {
    watch.events(&mut events);          // appends; returns how many were added
    for e in events.drain(..) {
        match e.kind() {
            EventKind::LoggedOn        => println!("conn {} on", e.id()),
            EventKind::Ended(why)      => println!("conn {} ended: {why:?}", e.id()),
            EventKind::EndedWithoutReason => println!("conn {} ended, cause unrecorded", e.id()),
        }
    }
    if watch.events_lost() > 0 { /* you fell behind — see below */ }
    std::thread::sleep(std::time::Duration::from_millis(200));
}
```

**`DropReason` is the point of this.** On the wire, a `Logon` refused for a bad clock and a
`Logon` refused because the venue is shut are the *same observable*: silence. They are also
different people's problems on different days — `SendingTimeOutOfRange` is your NTP,
`OutsideSchedule` is your venue calendar. Six of the 59 acceptance definitions expect no
response at all, so the conformance gate is blind to the difference and only this stream is
not.

**Three things to hold onto.**

1. **Read often enough.** The ring holds `EVENT_CAPACITY` (256) events and the engine never
   waits for you. When it overflows, `events_lost()` goes up — **check it**, because an
   event stream that loses silently is a source you would keep trusting. A mass reconnect is
   both the moment it can overflow and the moment you need it.
2. **`ConnId` is not an identity.** An event names the connection, not the counterparty. If
   you need to know *who*, take a snapshot while the session is up and keep the mapping;
   after the disconnect it is gone.
3. **`EndedWithoutReason` means the cause was not recorded**, not that there wasn't one.
   It is a variant rather than a guess on purpose — a diagnostic that invents the most likely
   answer is worse than one that admits it does not know.

Only three kinds exist today: logon, ended, ended-without-reason. Gap detected, resend
issued and reject sent are **not** here — they are message-rate, and D8 forbids anything
message-rate on the hot path until the cost has been measured.

## 8b. Speaking first: what an initiator can be told to say

`[2026-09-02]` An acceptor answers. **An initiator has to start things**, and six of the things
it starts cannot come from the protocol — nothing on the wire asks for a `Logout`, and no timer
produces one. So they are calls you make:

```rust
session.send_heartbeat(emit);                    // 35=0, keepalive
session.send_test_request(b"OPS-7", emit);       // 35=1, your 112=
session.send_resend_request(4, 9, emit);         // 35=2, your 7= and 16=
session.send_sequence_reset(4812, emit);         // 35=4, and become 4812
session.begin_logout(b"end of day", emit);       // 35=5, then wait for theirs
session.send_application(&msg, &mut journal, emit);
```

**Four constraints the compiler cannot hold for you:**

1. **They are silent before the Logon is agreed and after the Logout.** Each returns `false`
   (or `Link::Dropped`) and sends **nothing**. That is deliberate — a message offered to a
   session that is not up has not done anything wrong — but it means *"I called it"* is not
   *"it went out"*. **Read the return value.**
2. **You never write `34=`, `52=`, `49=`, `56=`, `8=`, `9=` or `10=`.** There is no function
   that takes whole message bytes, on purpose
   ([ADR-0042](decisions/ADR-0042-a-second-implementation-is-the-only-independent-opinion.md)).
   If you find yourself wanting one, the thing you want is `send_application`.
3. **`send_test_request` remembers nothing.** The counterparty answers with a `Heartbeat`
   echoing your `112=`, and **matching the answer is yours** — a session that waited for it
   would need a timeout, and a timeout is a clock this layer does not own (D1). Choose a
   `112=` you can recognise; the session has one of its own for the request it raises after
   silence, and yours must not collide with it.
4. **`16=0` means *and everything after*.** It is passed through, not refused. Asking for a
   range that runs backwards — `from` greater than `to` — is not refused either, and
   `[measured 2026-09-02]` a real counterparty answers it with a gap fill rather than an
   error, so a mistake there is **silent on both sides**. Check your own arithmetic.

**Reconnect, backoff and a schedule for an initiator are not here.** They are the engine's, and
no part of them is covered by the acceptance corpus or by the interop gate. `STATUS.md` carries
that as an open item.

## 8c. Dialling out, and coming back

`[2026-09-02]` `connect_and_serve` runs **one initiator session** and does not give up when the
connection ends:

```rust
use fixbolt_engine::reconnect::Policy;

let policy = Policy::new(1_000, 30_000)?;   // 1 s, doubling, capped at 30 s
fixbolt_engine::connect_and_serve::<MyApp, fixbolt_engine::journal::Store, _>(
    "venue.example:9823",
    Config::initiator(b"FIX.4.4", b"ME", b"VENUE").with_heart_bt_int(30),
    MyApp::new(),
    policy,
    fixbolt_engine::recovery::NoRecovery,   // ← read the next paragraph before shipping this
)?;
```

**Four things it will not do for you**
([ADR-0043](decisions/ADR-0043-backoff-without-jitter-and-a-reconnect-asks-recovery-every-time.md)):

1. **`NoRecovery` restarts your sequence numbers on every reconnect.** It is correct for an
   in-memory journal — that journal could not have replayed anything anyway — and it is **wrong
   for a counterparty that expects continuity**, which is most of them. If a reconnect must
   carry on from `34=N`, pass a `Recovery` backed by a journal on disk, the same one
   `serve_with_recovery` takes. This is the single easiest mistake to make here, because
   "reconnect" sounds like it implies continuity and the type system will not stop you.
2. **There is no jitter.** If you run many initiators against the same venue they will all come
   back at the same millisecond, at every rung of the ladder. One session per process is the
   shape this engine is built for; a fleet needs jitter, and it is not here.
3. **A schedule does not know when it next opens.** Give the policy `with_schedule(...)` and
   outside those hours it will not dial — it re-asks once a ceiling. It cannot wake exactly at
   the open.
4. **`standard` only.** There is no `connect_and_serve_hft`. An `hft` deployment that dials out
   drives `Engine` itself, as it did before this existed.

**Set the ceiling deliberately.** Without one, a long outage turns into an hour of silence and
your first sign of recovery is a phone call. 30 s is a reasonable default; the venue's own
reconnect guidance beats any default here.

### The 3 a.m. phone call

`[2026-09-02]` The counterparty rings and says their next number is 4812. `Engine::admin()`
hands you a second handle over the same mechanism — `Observer` looks, `Admin` changes
([ADR-0036](decisions/ADR-0036-one-mechanism-two-capabilities.md)). Give an `Observer` to
everything that watches and an `Admin` only to whatever takes that call.

```rust
let admin = engine.admin();     // Send + Sync, like the Observer
// `id` comes from a snapshot: SessionSnapshot::id().
admin.submit(Command::SetNextOut { id, n: 4812 });
```

**Pick the right one of the three, because two of them are silent:**

| | What goes on the wire | When you want it |
|---|---|---|
| `SetNextIn { id, n }` | nothing | You decide what you expect. Never a lie |
| `SetNextOut { id, n }` | **nothing** | The counterparty has **already told you** their number. Same behaviour as QuickFIX's `setNextSenderMsgSeqNum` |
| `SendSequenceReset { id, n }` | `35=4`, `123=N`, `36=n` | **You** are the one changing the number. The only honest form |

**`SetNextOut` is a lie until the counterparty is told.** They still expect the old number,
so the next message you send draws a `ResendRequest`, or is refused as too low. Reach for it
only when they have told you what to set; otherwise `SendSequenceReset` is the one you mean.

A reset that moves the number **down** is allowed and is a last resort: the counterparty will
accept numbers it has already seen, so anything it kept for a resend is now ambiguous.
Nothing stops you, because at 3 a.m. that is sometimes the instruction.

**Four things about `submit`.**

1. **`true` means queued, not done.** The engine applies it on its next turn, and the outcome
   arrives on the event stream as `EventKind::Administered { change, to, outcome }`.
2. **`Outcome::NoSuchConnection` is ordinary, not an error.** A command can race a
   disconnect, and that is knowable only on the engine thread — which is why `submit` cannot
   tell you the outcome itself.
3. **`false` means the queue is full and nothing was taken.** Unlike a dropped event, a
   dropped command is never silent. `COMMAND_CAPACITY` is 32, sized for a person rather than
   for a loop.
4. **Two identical commands produce two identical events.** There is no command id, so if you
   submit in a loop you cannot tell the outcomes apart. Submit one at a time and read the
   outcome.

### Answering "did you receive it?"

`[2026-09-02]` The counterparty asks about an order from three weeks ago. **Do not reach for
`FileJournal`** — it reloads the file into a ring of `N` messages, because its job is the next
`ResendRequest`, and the message being asked about left that ring long ago.

```
$ jrnl /var/lib/fixbolt/ISLD.journal --seq 4812
msg  4812  8=FIX.4.4|35=D|34=4812|11=ORDER-4812|...

$ jrnl /var/lib/fixbolt/ISLD.journal --count
messages 2000  inbound-marks 1  seq 1..2000  bytes 87794
```

`journal::Reader` is the same thing as a library, if you would rather build the answer into
your own tooling
([ADR-0037](decisions/ADR-0037-reading-a-journal-is-not-recovering-from-one.md)).

**Three things to know before you trust the answer.**

1. **Check the exit code, or read stderr.** A file whose tail is torn — a process killed
   mid-write — makes `jrnl` warn and exit **2**. Those bytes are not shown and not replayed,
   so *"no, we never received it"* drawn from a torn file **might be wrong**.
   `Reader::torn_tail_bytes()` is the same fact for a program.
2. **The whole file is read into memory.** Fine for a tool, and a real limit for a very large
   journal; there is no streaming reader.
3. **Do not read a file the engine is still appending to.** You will see a consistent prefix
   and probably a torn-tail warning. Nothing about that case is promised or tested.

`jrnl` does not decode FIX — it prints the bytes with `SOH` shown as `|` and leaves the rest
to `grep`. Interpreting a message needs the dictionary, and a program that reads a file has no
business pulling one in.

### 6c. The message log: both directions, refusals included

`[2026-09-04]` The journal answers *"what did we send, by sequence number"*. It cannot answer
*"what did we receive at 10:32:07, and what did we turn away"* — it holds outbound application
messages only, keyed by `seq`, and the frames that matter most in a dispute never got a `seq`.
The message log is the other file.

```
FileLogPath=/var/log/fixbolt/messages.log
```

`[DEFAULT]` only. One engine writes one file; `conn=` and `shard=` tell the counterparties
apart inside it. A `[SESSION]` block carrying the key is **refused at startup**, because two
counterparties asking for two files is a configuration that cannot be honoured and picking one
silently is worse.

```
# conn=1 shard=0 peer=10.4.2.9:51422 opened at 20260903-10:32:07.118
20260903-10:32:07.120 IN  shard=0 conn=1 peer=10.4.2.9:51422 8=FIX.4.4␁…␁35=A␁…
20260903-10:32:07.120 OUT shard=0 conn=1 peer=10.4.2.9:51422 8=FIX.4.4␁…␁35=A␁…
```

`grep -v '^#'` is the messages; lines starting with `#` are the writer's own notes.

**Seven things the type system cannot tell you.**

1. **`OUT` means *queued*, not *sent*.** The line is written when the message reaches the
   outbound buffer, which is the only moment the engine can name it. A socket that dies takes
   that buffer with it, and the log then claims a send that never left the machine.
   `EventKind::MessageLogUnsent { bytes }` says how many bytes at the tail of that connection's
   output are wrong. **Non-zero means read the end of that connection's lines with suspicion.**
2. **Every `OUT` line written during one engine turn carries the same millisecond.** A turn
   reads the clock once, on purpose — a second read on the hot path is not worth it. Order is
   the order of the lines in the file, never the timestamp column.
3. **Losses are dropped and counted, never waited for.** A full ring means the writer is behind
   the engine — a slow disk, a log on a network mount, a burst the ring was sized too small
   for. The log drops rather than block the engine. `Snapshot::log_lost` is the running total
   and `EventKind::MessageLogLost { count }` arrives without being asked. **Non-zero means the
   file has holes and is not a complete record.**
4. **A killed process leaves a torn last line, and it is marked rather than merged.** Reopening
   writes `# torn tail, N bytes, …` before appending, so two messages never become one line.
   `FileLog::torn_tail_bytes()` is the same fact for a program.
5. **`0x0A`, `0x0D` and `\` inside a DATA field are escaped** to `\n`, `\r` and `\\`, so one
   message is always one line and the line still decodes back to the exact bytes.
   `msglog::unescape` is the inverse.
6. **Rotation is yours.** `logrotate` with `copytruncate`, or move the file and restart. The
   engine never rotates, never compresses and never expires anything.
7. **It costs the engine thread a ring copy per message per direction** — roughly 340 ns for a
   200-byte message, so a request/reply pair pays it twice. `[unproven]` — that is arithmetic
   from `DESIGN.md` §6, not a measurement of this module. What **is** measured is that it
   allocates nothing: `benches/alloc.rs` cases `log-record`, `log-idle` and `log-busy`.

In `hft`, give the writer thread a core that is not the engine's — `FileLog::open_pinned`.
An unpinned writer can land on the very core the engine was isolated onto, which is the whole
of ADR-0015 decision 8.

### Stopping without lying to the counterparty

`[2026-09-02]` Killing the process is not a shutdown. To the other end it is a **dead line**,
so they reconnect — and any bytes still in `tx` are lost having already spent their sequence
numbers, so your next session shows a gap for messages that never went on the wire.

```rust
let admin = engine.admin();
std::thread::spawn(move || {
    wait_for_sigterm();
    admin.shutdown(30_000);     // 30 s of grace, on the engine's clock
});

let done = engine.run();        // returns when the shutdown finishes
if !done.clean() {
    eprintln!(
        "{} of {} never answered — check their sequence numbers before restarting",
        done.timed_out(), done.sessions(),
    );
}
```

**Read the report.** *"We stopped"* and *"we stopped while two counterparties never
answered"* are different facts
([ADR-0038](decisions/ADR-0038-an-ordered-shutdown-is-a-state-not-a-flag.md)), and only the
second means you may have to reconcile sequence numbers by hand.

**Five things the compiler cannot tell you.**

1. **`grace_ms` is on the engine's clock.** With `SystemClock` that is wall time. With a clock
   of your own that stops advancing, the deadline never arrives and the shutdown never ends.
2. **Drop the engine after `run` returns, before the process exits.** A
   `Durability::Async` journal joins its writer thread on `Drop`; exiting without that can
   leave the tail of the file unwritten — which `jrnl` will then report as torn.
3. **Nothing catches `SIGTERM` for you.** The engine is a library. Wiring a signal to
   `Admin::shutdown` is yours.
4. **`serve_sharded_hft` cannot be stopped.** It has no shutdown path at all.
5. **Your application is not consulted.** There is no *"let the dispatcher drain"* phase, so
   an out-of-band dispatcher can lose work it had already accepted.

### What is still not here

`STATUS.md` open item 30 is **closed** — this section covers all of it. What remains is in
that file's *Not proven*, and two entries matter to you here: **nothing authenticates the
holder of an `Admin`** — the engine has no idea who is on the phone, and who you pass that
handle to is the whole of the access control — and **nothing stops accepting during a
shutdown**, so a socket arriving in the grace period is dropped rather than told anything.

## 9. What this engine does not do for you

Stated so you do not discover it in production:

- **It does not validate application-message semantics.** The dictionary validation is
  session-layer: required fields, types, enum values, structure. Whether a `NewOrderSingle`
  makes business sense is yours.
- **It does not decide *when* to resume sequence numbers — you do.** `[2026-08-31]` the
  mechanism exists: `Session::resume(cfg, next_out, next_in)` carries numbers across a restart
  and `Session::next_out()` / `next_in()` are what you persist
  ([ADR-0010](decisions/ADR-0010-a-reconnect-is-not-a-restart.md)). What the engine will never
  do is guess: a session built with `Session::new` has persisted nothing and resets, so
  **reading the journal back and choosing `new` or `resume` is your call**, and getting it
  wrong is a sequence-number dispute with your counterparty rather than a compile error.
- **Recovery does not reach the sharded runtime.** `[2026-09-02]` `serve_with_recovery` and
  `serve_hft_with_recovery` exist; `serve_sharded_hft` has no variant, and the serving loop
  fixes the journal type. `STATUS.md` item 31.
- **Its session schedule stops at the timezone.** `[2026-09-02]` §5a: the hours, the weekday
  filter, the week-long window and the sequence-number reset all work, and they are **UTC**.
  Resolving a venue's local time — and rebuilding the `Schedule` when daylight saving moves
  it — is yours. **And the engine does not yet persist when a session was last active**, so a
  process restarting across a boundary keeps yesterday's numbers unless you resume the session
  yourself with `Session::resume_at`.
- **`serve_hft` pins nothing, and it is the one entry point that does not.** `[2026-08-31]`
  `fixbolt_engine::affinity` pins a thread and reads the core back, and `serve_sharded_hft`
  pins **every engine thread it starts** (§1a). `serve_hft` starts no thread at all — it runs
  the engine on the thread that called it — so **that thread is yours to pin**, with
  `affinity::pin_current_thread` before the call or `taskset` around the process. Skip it and
  D8's premise does not hold for that deployment, and §8's budget is not about your process.
  `STATUS.md` open item 21, narrowed to exactly this.
- **It is not TLS-complete.** `[2026-08-31]` ADR-0005's load-bearing question is **answered**:
  `ktls-core` can be driven from a plain non-blocking socket with no async runtime, under four
  conditions ([ADR-0018](decisions/ADR-0018-ktls-on-a-plain-socket-answers-adr-0005.md)). That
  is a spike, not a feature — **no TLS code is merged, no TLS latency number is published, and
  there is no plan yet**. Which kernel and which cipher suites are the floor, and what tells
  you a session fell back to the userspace path, are both still open.
