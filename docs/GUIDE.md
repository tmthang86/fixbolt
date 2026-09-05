# fixbolt — Developer Guide

**Who this is for:** somebody embedding fixbolt in their own application. If you are
changing fixbolt itself, read [DESIGN.md](DESIGN.md); what it must do is in [PRD.md](PRD.md).

fixbolt is a **framework**: it calls your code on its hot path, on a thread it owns, under
constraints it cannot enforce for you. Everything on this page is a constraint that shows up
as latency or as lost messages rather than as a compile error. Where a rule is not held by
the type system or by a test, this page says so.

> **Every number here names its machine.** `[measured 2026-08-31]` means it was run on the
> tuned Linux desktop [DESIGN.md §9](DESIGN.md) describes (AMD Ryzen 7 3700X, Linux 7.0.0-30,
> `scripts/check-machine.sh` clean) and read, not estimated. Your figures will differ; the
> *ratios* are what transfer.

**Contents**

| § | Topic |
|---|---|
| [0](#0-first-decide-your-mode) | `standard` or `hft` |
| [1](#1-the-one-thing-that-decides-your-latency) | Sessions per polling thread |
| [1a](#1a-running-many-sessions-shard-across-threads-do-not-stack-on-one) | Sharding, cores, the machine |
| [1b](#1b-two-ways-to-write-a-handler) | `Handler` versus `Application` |
| [1c](#1c-many-counterparties-one-registry-and-a-configuration-file) | Counterparties and the configuration file |
| [2](#2-the-engine-calls-you-on-its-hot-path) | What not to do in a handler |
| [3](#3-messageview-borrows-the-engines-buffer) | Borrowed messages |
| [4](#4-when-the-ring-fills-you-lose-the-connection) | `RingDispatch` and its failure mode |
| [5](#5-time-enters-as-a-tick) | Time |
| [5a](#5a-session-schedules-and-the-timezone-trap) | Schedules |
| [6](#6-journalling-pick-the-policy-deliberately) | Journal, resend ring, idempotency, disk, message log |
| [7](#7-the-machine-is-part-of-your-latency) | The machine |
| [8](#8-how-to-benchmark-this-engine-without-fooling-yourself) | Benchmarking |
| [8a](#8a-watching-a-running-engine) | Observation, health, events |
| [8b](#8b-speaking-first-what-an-initiator-can-be-told-to-say) | Initiator sends |
| [8c](#8c-dialling-out-and-coming-back) | Reconnect, administration, shutdown |
| [9](#9-what-this-engine-does-not-do-for-you) | What is yours |

---

## 0. First decide your mode

[ADR-0013](decisions/ADR-0013-two-modes-standard-and-hft.md). If you say nothing you get
`standard`, which is almost certainly what you want.

| | **`standard`** (default) | **`hft`** (opt-in) |
|---|---|---|
| When idle | blocks on readiness and gives the core back | spins; burns a core, permanently, per polling thread |
| Wakeup cost | `epoll`-class, 2–5 µs | `[measured 2026-08-31]` one turn of **449 ns per session** |
| Runs on | any OS, any hardware, a container, a shared box | Linux, on a machine that satisfies [DESIGN.md §9](DESIGN.md) |
| Core pinning | none | the polling thread must sit on an isolated core. `serve_sharded_hft` pins its threads and refuses a bad plan; `serve_hft` runs on the calling thread and leaves the pinning to you (§9) |
| Entry point | `serve` | `serve_hft`, `serve_sharded_hft` |
| Choose it when | you are not counting microseconds, or you share the machine | one session matters more than the core it costs |

**Do not choose `hft` because it sounds better.** It pins a core at 100% for as long as the
process lives. On a shared machine, in a container, or on a laptop that is the engine doing
exactly what you asked, and it is not a bug you will enjoy diagnosing.

**A `standard` number and an `hft` number are not comparable.** When you publish one, say
which. `[measured 2026-09-02]` on the tuned desktop the two differ by 3 437 ns at p50 on the
same path, which is what the burned core buys ([DESIGN.md §8](DESIGN.md)).

Everything below describes the design both modes share, and says where a mode changes it.

---

## 1. The one thing that decides your latency

**Sessions per polling thread.** This section is about `hft`; in `standard` the thread blocks
rather than sweeping, and the term below is replaced by the wakeup.

An idle turn of the engine is one non-blocking `read` per connection. `[measured 2026-08-31]`
a whole `Engine::turn` costs **449 ns per session** on a core set up to §9, flat from 1 to 16
sessions within 2%. Of the 449, about **420 ns is the `recv` syscall** and about 30 ns is
everything the engine itself does, measured in the same run. So a sweep costs `N × 449 ns`,
and a message arriving just after its socket was polled waits a whole sweep before anyone
looks at it.

| Sessions on one thread | Added latency, worst case | Against a measured 16.0 µs round trip |
|---|---|---|
| 1 | 0.45 µs | 2.8% |
| 2 | 0.90 µs | more than the entire user-space budget in [DESIGN.md §8](DESIGN.md) (~0.46 µs) |
| 16 | 7.2 µs | 45% |
| 128 | 57 µs | 3.6× the whole round trip |

(This table was built on 703 ns until 2026-09-02. That figure was a C program's bare `read` on
a `nohz_full` core; §9 no longer asks for `nohz_full`, and on the line it now describes the
turn is 449 ns. `isolcpus` itself costs nothing: 494.8 ns against 501.8 on an untouched core.
[ADR-0021](decisions/ADR-0021-nohz-full-leaves-section-9.md).)

**If you care about latency, run one session per thread and pin that thread to an isolated
core.** If you are building a gateway for many clients you are in the `density` shape, which
is supported and reasonable, and you should plan against `N × 449 ns` per shard rather than
against this project's headline figures
([ADR-0012](decisions/ADR-0012-latency-first-and-one-session-per-polling-thread.md)).

**Nothing enforces this.** The engine will carry 500 sessions on one thread and not warn you.

### Pinning shows up only in the tail

`[measured 2026-09-02]` whether the engine thread sits on an `isolcpus` core is worth
**nothing at p50 and 11× at p99.9**, wire-to-wire on the tuned desktop with one variable
between the arms:

| Where the engine thread ran | p50 | p99.9 |
|---|---|---|
| pinned to an isolated core | 19 968 ns | **26 300 ns** |
| pinned to an ordinary core | 19 407 ns | 266 887 ns |
| not pinned at all | 19 607 ns | 293 749 ns |

The isolated core is 2.9% *slower* at the median, so a benchmark that reports medians says
the tuning is pointless. What it prevents is the scheduler putting something else on your
core for a quarter of a millisecond, and you only see that in a percentile long enough to
contain it.

**Nothing enforces this either.** `pin_current_thread` will pin you to a core the scheduler
shares and return `Ok`; it proves your thread went where you said, not that the core is
yours. Check `/proc/cmdline` for `isolcpus`, or run `scripts/check-machine.sh`.

### Kernel bypass does not rescue session density

`[measured 2026-08-31]` removing the syscall (Onload, `ef_vi`, DPDK) takes about 420 ns of
the 449 down to the cost of a memory read. Two terms survive it:

- **Cache.** One `Connection` is **53.3 KiB**; L1d on the test machine is 32 KiB, so one
  connection does not fit in L1. Random access costs **1.05 ns** in L1 and **78.5 ns** from
  RAM, a 75× ratio, and that applies to every access the engine makes.
- **Head-of-line blocking.** One thread serialises. Per-message work is about 460 ns (parse
  122.6 + encode 239.1 + the session step), so `k` sessions holding a message at once make the
  last one wait `(k − 1) × 460 ns`. Only fewer sessions removes this.

Full working: [reference/measured-costs.md](reference/measured-costs.md).

---

## 1a. Running many sessions: shard across threads, do not stack on one

A gateway with a hundred sessions on a sixteen-core server is a good deployment, and the
arithmetic works as long as it is *shard*, not *stack*:

| Shape | 100 sessions | Sweep |
|---|---|---|
| **Stack**: one engine, one thread | 100 on one thread | `100 × 449 ns` = **45 µs** |
| **Shard**: 8 engines, 8 pinned threads | ~13 each | `13 × 449 ns` = **5.8 µs** |

Six microseconds sits under the 10–20 µs kernel-TCP floor, so for a gateway it is no longer
the dominant term. Sharding is what makes "many sessions" reasonable, not the core count by
itself, because a session only benefits from a core its own polling thread is on.

### The shard runtime

`fixbolt_engine::shard`, behind the `affinity` feature, Linux only. `Shards::start` validates
a `ShardPlan`, starts one pinned thread per core, waits for every one of them to confirm its
own pin, and hands accepted connections across a channel that `[measured]` makes no syscall
and no allocation. `serve_sharded_hft` is the whole loop.

**The shard a session lands on is decided by its identity, not by accept order.**
`serve_sharded_hft` holds each socket until its Logon arrives, reads `49=` / `56=` off it,
and routes on a **stable** hash of the pair, so two connections claiming one identity reach
the same engine and the single-logon rule can see them both
([ADR-0020](decisions/ADR-0020-a-pre-session-stage-owns-the-socket-until-logon.md)).

> This was a defect until 2026-09-01. An `Engine` enforces "that identity is already logged
> on" by looking at the connections it holds, and routing by accept order split one identity
> across engines: `[measured 2026-08-31]` the acceptance corpus scored 59 through one shard
> and **57 through two**. Routing after the Logon made it 59 through two, and the old
> `Assign` and `RoundRobin` policies were deleted rather than documented.

**Two limits you must choose, because neither has a default.** `Limits::new(pending,
logon_ms)` refuses a zero in either place:

| Limit | What happens without a sensible one |
|---|---|
| `logon_ms`: how long a connection has to send its Logon | a counterparty that opens a socket and says nothing holds a slot until you restart. A denial-of-service hole, not a tuning knob |
| `pending`: how many may wait at once | the pending table has no ceiling and neither does the memory behind it. When it is full the next connection is refused immediately rather than queued, which is what you want under attack |

**Replacing the routing policy.** `Shards::with_route(Box<dyn Route>)` takes an `Identity`
and the shard count. Whatever you write must be **stable across processes**: the same
counterparty has to reach the same shard after a restart, or the single-logon rule breaks
again in a way every test passes. `DefaultHasher` is seeded per process and is the trap.
`HashRoute` does not rebalance, does not know that one counterparty sends a hundred times
more than another, and a `ConnId` never moves between engines.

**Building it by hand.** `Engine` holds a flat `Vec<Connection>`, `turn()` sweeps them all,
and `run()` is `loop { turn() }`, so one `Engine` is one shard. The pieces are separate on
purpose: `Acceptor::bind(addr)` / `accept()` is one listener;
`Engine::add_with_prefix_and_config(transport, cfg, prefix)` adds a connection with the
configuration the registry chose and the bytes the pre-session stage already read; one
`Engine` per thread, each pinned, each running `turn()`. If you compose them yourself you
inherit the defect above: read `presession::identity_of` and `Route` before writing an accept
loop of your own. `serve()` is the single-threaded convenience and the wrong production shape
for a gateway; read it as an example.

### Cores: you name them, the engine pins and reads back

`fixbolt_engine::affinity`, behind the `affinity` feature, Linux only
([ADR-0015](decisions/ADR-0015-explicit-cores-pinned-from-inside-and-read-back.md)). Call
`pin_current_thread(CoreId(6))` **from inside the thread, as its first act**, and check the
`Result`: it asks the kernel back with `sched_getaffinity` and returns `ReadbackMismatch` if
the answer disagrees. The engine never picks a core for you: the OS's idea of a free core knows
nothing about `isolcpus`, your NIC's interrupts, or SMT siblings.

**Say which cores you mean before you start any thread**, and let the engine refuse the plan:

```rust
use fixbolt_engine::affinity::{CoreId, ShardPlan};
ShardPlan::new(vec![CoreId(6), CoreId(7)])
    .with_journal_core(CoreId(0))
    .validate()?;                       // before a single thread exists
```

It refuses a core that is absent, offline, named twice, an SMT sibling of another core in the
plan, or, for shard cores, not in `isolcpus`. `allow_unisolated()` waives only that last rule;
a development box and CI need it.

**`CoreId(0), CoreId(1)` is the natural first guess and it is wrong on any machine with SMT
on.** `[measured 2026-08-31]` a GitHub runner reports `cpu0` and `cpu1` as two threads of one
physical core, and that plan was refused. Use `Topology::siblings_of` to take one id per
physical core:

```rust
let t = Topology::read()?;
let mut cores = Vec::new();
for c in t.online() {
    if cores.iter().any(|taken| t.siblings_of(*taken).contains(c)) { continue; }
    cores.push(*c);
}
```

`NotOnline` is a rule of its own because `[measured 2026-08-31]` on the tuned desktop
`/sys/devices/system/cpu/isolated` reads `6-7,14-15` while `online` reads `0-7`: turning SMT
off took 8–15 offline, and a plan that trusted `isolcpus` alone would have pinned a shard to
a CPU the kernel will not schedule.

**The threads that are not engine threads.** `ShardPlan` validates their cores too: a journal
writer or ring consumer sharing a core, or a physical core, with a shard is refused before
anything starts. `FileJournal::open_pinned(path, Durability::Async, core)` pins the journal's
writer and reports the core it landed on (`writer_core()`). The ring consumer is **your**
thread, the one that calls `RingApp::pump`, so pin it with `affinity::spawn_pinned` or, from
inside it, `affinity::pin_current_thread`. `Durability::Fsync` has no writer thread and
`open_pinned` refuses it rather than accepting a core it would ignore.

### The machine settings that matter, and the one that does not

- **`isolcpus` and `rcu_nocbs`: keep them.** `[measured 2026-08-31]` free (494.8 ns and
  498.2 ns per turn against 501.8 untouched), and §1 shows what isolation buys in the tail.
  What `isolcpus` buys *under load* has not been measured; it was measured on a quiet machine.
- **`nohz_full`: leave it off.** `[measured 2026-08-31]` it adds 160 ns to every kernel entry,
  taking a turn to 670.7 ns, and this engine's idle turn is nothing but kernel entries. It is
  behind at p50 (376 ns against 216), p99 (376 against 224) and p99.9 (384 against 224), and
  ahead only from p99.99 outward (504 against 2 848). Take it only if your objective is stated
  at p99.99 or beyond ([ADR-0021](decisions/ADR-0021-nohz-full-leaves-section-9.md)).
- **The CPU speculation mitigations cost 59–63% of every syscall this engine performs**, and
  you cannot tune that away. `[measured 2026-09-01]` a turn is 448.9 ns mitigated and
  175.2 ns with them off, while thirteen pure user-space benchmarks do not move. On this AMD
  Zen 2 box it is `retbleed`'s untrained return thunk plus `spec_rstack_overflow`'s Safe RET.
  **This is stated so you can plan, not so you turn them off.** Whether a machine runs
  mitigated is a security decision for whoever owns it. What this project does say is that a
  figure from an unmitigated machine is not comparable to anything here, and
  `scripts/check-machine.sh` will not call such a machine §9-satisfied
  ([ADR-0023](decisions/ADR-0023-section-9-records-the-cpu-mitigations.md)). Keep your fleet
  configured the same way as the machine your baselines came from, or you will see a 2.5×
  spread in syscall cost that has nothing to do with your code.
- **Your `[profile.release]`, not ours.** Cargo honours a profile only from the top-level
  package being built, so this project's profile does not reach you
  ([ADR-0024](decisions/ADR-0024-the-workspace-keeps-the-default-release-profile.md)). Yours
  does, and `[measured 2026-09-01]` on the tuned desktop it is worth:

  | Setting | syscall-bound path | pure computation | clean build |
  |---|---|---|---|
  | `lto = "thin"` | −2 … −3% | −8% … +1% | 5.2 s → 17.1 s |
  | `lto = "fat"` | **−3 … −5%** | −31% … +12% | → 15.9 s |
  | `codegen-units = 1` | −0 … −2% | −17% … +2% | → 5.2 s |
  | both | **−3 … −6%** | −30% … +12% | → 16.3 s |

  Read the caveat before planning against those numbers. A benchmark is a separate crate
  calling into the library, so LTO inlines library internals into the benchmark loop, and
  your application may not have that boundary in the same place. How much survives into a
  real application was not measured. The syscall-bound column is the more trustworthy half,
  because kernel time cannot be inlined away. Note the regressions too: `SendingTime from the
  cache` is +12% under fat LTO.

---

## 1b. Two ways to write a handler

There are two, and choosing between them is a latency decision, so make it deliberately.

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

fixbolt::serve(addr, table, fixbolt::app(Desk), 64, limits, fixbolt::NoLog)?;
```

**What you did not write, and cannot get wrong.** `8`, `9` and `10` are the frame. `34` and
`52` are the session's; an application that regenerates `52` moves the body four bytes and
fails a test that says nothing about time. `49` and `56` are **reversed**: your sender is their
target. None of the seven is reachable from the API above, and naming one anyway is ignored
rather than merged, because two `34=` in one message is two sequence numbers.

**The order is not yours either.** Everything you name goes through the generated tables:
`MsgType`, then header tags ascending, then body tags ascending. Name them in any order.

### The fast way: `fixbolt::Application`

The raw seam. Bytes in, a `Range<usize>` out, and everything above is yours to do:

```rust
impl Application for Desk {
    fn on_message(&mut self, msg: &[u8], seq: u32, stamp: &[u8], out: &mut [u8])
        -> Option<Range<usize>> { /* ... */ }
}
```

`crates/conformance/src/echo.rs` is a worked example, and the comment at the top of it lists
the traps you are now responsible for.

### What the short way costs

`[measured 2026-09-05]` on the §9 desktop, AMD Ryzen 7 3700X, `pass 12 fail 0 unknown 1`,
medians of 20 `bench.sh` runs — committed benchmarks, all of them. One twelve-field
ExecutionReport:

| | ns/op |
|---|---|
| Encode a `Template` you built once (D9's shape) — `encode ExecutionReport (template)` | **237.6** |
| `library, reply only`: build a template, sort it, encode | **804.1** |
| `library, on_message`: the parse, the handler, and the reply | **1 028.6** |
| …of which the second parse is — `library, parse only` | 159.6 |

**About 3.4×, roughly 570 ns more per reply — 2.9% of a 19 908 ns application round trip on
the same box.** This table said *24×* against a *40 ns* encode until 2026-09-05; that 40 ns
came from an experiment that was never committed, and the committed benchmark for the same
shape read 177–199 ns on the same VM class
([ADR-0051](decisions/ADR-0051-item-34-is-a-third-of-the-size-it-was-recorded-at.md)). What
the 570 ns is: a `Template` built per message, sorted and laid out, where D9's shape builds it
once. `crates/library/benches/cost.rs` is the benchmark; ADR-0051 is where the owner decided
that 2.9% is not worth a codec hot-path change today.

| If your deployment is | Use |
|---|---|
| `standard` mode, order entry, a few thousand messages a second | `Handler`. A microsecond is not your problem |
| `hft` mode, one session on an isolated core | **`Application`**, with your own `Template` per message type, built at logon. A microsecond is more than the rest of the message costs put together |

There is no third option where you get both.

### The two sizes, and the cliff

`Handler<N, P, S>` defaults to `256, 64, 1024`: `N` fields in the inbound index, `P` fields
in a reply, `S` bytes of them. A reply that exceeds `P` or `S` is **`Answer::Failed`, not a
slower success**: nothing goes on the wire, and `App::failed_replies()` counts it. That
counter is the only way you will find out, so read it.

The defaults were measured rather than picked: 128 / 4096 costs 1.9× as much, and below
`S = 512` the curve flattens. If you know your message, say so:
`impl Handler<256, 32, 512> for Desk`.

---

## 1c. Many counterparties: one registry, and a configuration file

Every entry point takes a **`presession::Table`** with one entry per counterparty, and a
socket is held until its Logon says who it is
([ADR-0026](decisions/ADR-0026-a-counterparty-registry-in-the-pre-session-stage.md),
[ADR-0030](decisions/ADR-0030-one-engine-holds-many-counterparties.md)). (Until 2026-09-01
every entry point took one `Config`, so the engine served exactly one counterparty.)

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
    Limits::new(64, 30_000)?,              // pending sockets, and their deadline
    fixbolt_engine::msglog::NoLog,
)?;
```

Three things here are decisions, not defaults:

| | |
|---|---|
| **An empty table refuses every connection**, and `serve` refuses to start on one (`ServeError::NoCounterparties`) | There is no wildcard entry and none is planned. An acceptor that admits an identity nobody configured is an open port, which is what QuickFIX/J's `ANY_SESSION` template is |
| **A refused identity is told nothing** | The socket closes with no Logout and no Reject, as `1c_InvalidSenderCompID.def` and `1c_InvalidTargetCompID.def` expect. It is indistinguishable from a wrong password on purpose |
| **The refusal *is* the authentication hook** | There is no separate `AuthStrategy`. When a credential check on `553` / `554` arrives it goes in the `Entry`, behind the same `lookup`. Two hooks answering "may this counterparty in" are two rules that will disagree |

**`Table` keys on the comp IDs, and it is one implementation of a trait.**
`Registry::lookup(Identity) -> Option<&Entry>` is the seam. If your counterparties are told
apart by `50=` / `57=`, or live in a file or a database snapshot, write your own; it is about
eight lines, and `crates/engine/tests/registry.rs` has one. Two rules for whatever you write:

- **It must not allocate.** `lookup` is on the connection path and `benches/alloc.rs` asserts
  the whole pre-session stage is zero. Borrow from what your registry already owns.
- **It must answer immediately.** It runs on the acceptor thread. A remote entitlements service
  is a denial-of-service surface no logon deadline closes; snapshot it out of band. This is
  where the design parts from Artio's `authenticateAsync` deliberately.

**One engine holds all of them.** The registry decides which *configuration* a connection
gets, not which engine, and "this identity is already logged on" is answered by comparing
identities, not by counting connections. Two counterparties on one engine are two sessions;
two connections claiming one counterparty are one session and a disconnect.

### The configuration file

You do not have to build the table in code
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

`[DEFAULT]` supplies every `[SESSION]` after it; a `[SESSION]` overrides for itself. The
shape is QuickFIX's on purpose. The behaviour differs from QuickFIX in three ways, each
deliberate:

| | |
|---|---|
| **An unrecognised key is an error** | QuickFIX ignores what it does not know. Here a mistyped `Starttime` would fall back to `Schedule::always()`, and a session that should close at five would stay open all night with nothing saying so |
| **A file naming no counterparty is an error** | An empty table refuses every connection, so a mistyped path would behave exactly like a firewall dropping your port |
| **A half-written schedule is an error** | `StartTime` with no `EndTime` is refused rather than completed with midnight. So is a `StartDay` with no hours: a key spelled correctly that has no effect |

Every error carries its line and quotes what was written, because the person editing the file
does not read Rust:

```text
line 14: unknown key: Starttime
```

**A value longer than a `Config` can hold is refused, not truncated.** A truncated name
matches nothing, so truncation would give you an acceptor that starts cleanly and serves
nobody. The limits are `fixbolt_session::MAX_BEGIN_STRING_LEN` and `MAX_COMP_ID_LEN`.

**What the file cannot say**, and you will notice: no credential (ADR-0026 decision 3 makes
`lookup` the only authentication hook), no per-counterparty journal path (that belongs to
`Recovery`, [ADR-0039](decisions/ADR-0039-a-fresh-journal-is-the-deployments-to-build.md)),
no `50=` / `57=`, no reload while running (the table is read-only after startup), and no
`UtcOffsetMillis` key, deliberately: §5a explains why a fixed offset in a settings file is a
hazard dressed as a setting. Every key is listed in [CONFIGURATION.md](CONFIGURATION.md).

---

## 2. The engine calls you on its hot path

`InlineDispatch`, the default, runs your handler **on the engine thread**, between the read
and the reply. Everything you do there is added to every message's latency, and anything that
blocks stops the session layer.

**Do not, in a handler under `InlineDispatch`:**

| Don't | Why | What it costs |
|---|---|---|
| Allocate | non-negotiable 1 exists because allocation is unbounded, not because it is slow | `[measured]` one `format!` on an error path showed as 30 000 bytes in `benches/alloc.rs` |
| Log, print or format | same reason, plus I/O | the engine never logs on its hot path; neither should you |
| Take a lock shared with another thread | the engine thread must not block | unbounded |
| Do file or network I/O | every syscall is hundreds of nanoseconds before it does anything | see §1 |
| `sleep`, or wait on a channel | non-negotiable 4: the engine thread never sleeps in the kernel | unbounded, and `scripts/check-no-kernel-sleep.sh` fails |

**If your handler might do any of those, use `RingDispatch`.** Your code moves to its own
thread and the engine hands messages across an SPSC ring. You pay for the hop
(`[measured 2026-09-01]` **267 ns** one way against **8.5 ns** inline on the tuned desktop) and
you stop being able to stall the session.

**Read §4 before choosing the ring.** It has a failure mode inline does not.

---

## 3. `MessageView` borrows the engine's buffer

The view handed to your handler points into the engine's own read buffer. It is 24 bytes,
`Copy`, and **valid only for the duration of the call**.

- **Do not store it.** The borrow checker stops you keeping it past the call; that rule *is*
  enforced.
- **To keep a message, copy the bytes you need into storage you own.** Copy fields, not the
  whole message, unless you need the whole message.
- **Do not assume field order when reading.** Ask the view for a tag; do not index positions.
  Field ordering on the write path comes from generated tables and never from a call site
  (non-negotiable 5); the same discipline is worth having on the read path.

---

## 4. When the ring fills, you lose the connection

Under `RingDispatch`, if your thread stops draining, the ring fills, and **the connection is
then ended, deliberately** ([ADR-0011](decisions/ADR-0011-a-full-ring-disconnects.md)).

A message the ring refuses is one the session has already accepted, numbered, journalled and
acknowledged by sequence number, and that your application never saw. For order flow that is
not backpressure, it is silent loss. So the engine sends the counterparty a Logout whose
`58=` reads **`slow application`** (deliberately not D10's `slow consumer`, because the
counterparty is behaving perfectly and the fault is on this side) and drops the session. They
can reconnect and reconcile by sequence number, which they cannot do about a message they
were told had arrived.

**Three things follow, and the compiler cannot enforce any of them:**

- **A stall is now an outage, not a lag.** An application that pauses for longer than the ring
  holds drops the session. `[measured 2026-08-31]` at the 4 MiB default
  (`ring::DEFAULT_CAPACITY`) that is **5.05–5.36 ms** over four runs on the tuned desktop,
  against 47.7 µs at the old 64 KiB. Nobody has measured a real application's worst pause, so
  that is a budget, not a guarantee, and it is one machine's number: a faster box fills the
  ring faster and has less slack.
- **The Logout is queued, not sent, on the turn the ring refuses.** It goes out on the next
  flush. If you drive `turn()` yourself and stop the moment a connection looks doomed, you
  never send it and the counterparty learns nothing. Keep turning until `connections()`
  drops.
- **Watch `Engine::refused_connections()`.** Non-zero means sessions were dropped because your
  side could not keep up. Wire it to something a human sees. `RingDispatch::refused()` counts
  the same events from the dispatch's side.

**Size the ring for your stall, not your throughput.** The question is not "how many messages
per second" but "how long can my consumer be away".

---

## 5. Time enters as a tick

The session layer takes no clock (D1). Time arrives as `Input::Tick`, in milliseconds since
`0000-01-01` (D13), and the session judges `SendingTime` against **the last tick it was
given**.

- **A session that has never ticked holds zero** and will refuse the first message it sees for
  clock skew. The engine ticks before it reads for exactly this reason.
- **If you drive the engine yourself**, as `crates/engine/tests/wire.rs` does, you own that
  ordering. Tick first, then read.
- **Do not format a timestamp per message.** The outbound path patches a cached one;
  `[measured 2026-08-31]` `SendingTime from the cache` is **4.9 ns**. Building one from scratch
  is orders of magnitude more and a hot-path allocation waiting to happen.

---

## 5a. Session schedules, and the timezone trap

A FIX session is not open all the time, and when it closes **both ends start again at `34=1`
the next morning**. That is protocol, not housekeeping: get it wrong and you spend the morning
arguing sequence numbers with your counterparty.

```rust
use fixbolt_session::schedule::{Schedule, Weekdays};

let hours = Schedule::daily(8 * 3_600, 17 * 3_600)   // seconds since midnight, UTC
    .expect("08:00 is before 17:00")
    .with_weekdays(Weekdays::WEEKDAYS)
    .expect("Monday to Friday");

let cfg = Config::acceptor(b"FIX.4.4", b"ISLD", b"TW44").with_schedule(hours);
```

Without `with_schedule` a session is open forever and never resets. That default is exactly
neutral: `[measured 2026-09-02]` forcing `same_session` to `true` turns five schedule tests red
and leaves 59 / 59 green, which proves the corpus cannot see a schedule at all.

### The trap: these are UTC, and a fixed offset is not daylight saving

**fixbolt has no timezone database and never will**
([ADR-0033](decisions/ADR-0033-a-schedule-is-utc-arithmetic-and-the-calendar-stays-outside.md)).
The session layer is pure, and an IANA database is a dependency that allocates.

So if your venue says *"17:00 America/New_York"*:

1. Resolve that to a UTC offset **with your own zone library**, for the date in question.
2. Build the `Schedule` with `with_utc_offset_ms`.
3. **Rebuild it when the offset changes.** New York is −5 h in winter and −4 h in summer.

```rust
// A venue seven hours east of UTC that does not observe DST.
let jakarta = Schedule::daily(9 * 3_600, 16 * 3_600)
    .expect("legal")
    .with_utc_offset_ms(7 * 3_600 * 1_000)
    .expect("inside a day");
```

**A `Schedule` built from one DST offset is wrong for half the year**, and the failure is not
loud: it resets sequence numbers an hour early or an hour late, on exactly the two days a
counterparty is least forgiving. Nothing in the type system catches it. If your venue observes
DST, something in your deployment must rebuild the `Schedule` twice a year, and that something
is yours.

### A session may run past midnight

`open > close` wraps and is legal: 22:00 to 06:00 is **one** session, so nothing resets at
midnight in the middle of it. A weekday filter selects the day a session **opens** on, so a
Friday-night window under `Weekdays::WEEKDAYS` runs into Saturday morning as it should. For a
week-long window use `Schedule::weekly`: Sunday 21:00 to Friday 21:00 is one interval.

### Persist *when*, not only *what*

```rust
// Wrong across a boundary: carries the numbers, says nothing about the calendar,
// and therefore NEVER resets.
let s = Session::resume(cfg, next_out, next_in);

// Right: carries the numbers AND when they were last touched.
let s = Session::resume_at(cfg, next_out, next_in, last_active_ms);
```

`next_out = 41` says nothing about whether a trading day has ended since 41 was reached. So
persist `Session::last_active_ms()` beside the sequence numbers and hand it back. A session
resumed without it never resets, which is right for `Schedule::always()` and wrong for
everything else.

**`Engine::add_resumed` is how you hand it over**, and it takes the journal too:

```rust
// `highest_out`, NOT `highest` — see the box below.
let next_out = journal.highest_out().map_or(1, |h| h + 1);
let next_in  = journal.highest_in().map_or(1, |h| h + 1);
engine.add_resumed(transport, cfg, journal, next_out, next_in, Some(last_active_ms));
```

Pass `None` for the last argument and no boundary is ever noticed.

**If you use `serve` rather than driving the engine yourself**, hand it a `Recovery` instead:

```rust
use fixbolt_engine::recovery::{FromFn, Resumed};

let recovery = FromFn::new(|cfg: &Config| Some(Resumed {
    journal:        my_journal_for(cfg),          // yours to open
    next_out:       my_next_out_for(cfg),
    next_in:        my_next_in_for(cfg),
    last_active_ms: my_last_active_for(cfg),
}));
fixbolt_engine::serve_with_recovery(addr, table, app, capacity, limits, recovery, log)?;
```

It is asked **once per connection, after the registry has named the counterparty**, on the
acceptor thread, which is allowed to block, so reading a file there is fine. **A network
round trip is not**: every connection behind it waits, and the only backstop is the pending
deadline, which refuses the socket without saying why. Returning `None` starts that session
fresh, which is what plain `serve` does.

**Two limits.** `serve_sharded_hft` has no recovery variant, so a sharded deployment cannot
resume. And with an in-memory journal nothing persists `last_active_ms` for you; a
`FileJournal` records it at logon and at an ordered shutdown (§6b).

### What decides the reset

A comparison, not a clock alarm: *do the last instant I remember and now fall in the same
interval?* That is the only question an engine that was asleep at midnight, or started at
06:00, can still answer, and those are precisely the times a reset matters. An instant your
schedule cannot place is **never** the same session as anything, so an engine that cannot tell
resets rather than carrying numbers across a boundary it could not see. Resetting when your
counterparty did not is a Logon argument you see at once; not resetting when they did is a
silent divergence you find much later.

### What is not here

No `ResetSeqTime` (the reset is tied to the interval boundary, not a separate hour). No
timezone names. And the Logout sent when your window closes carries no `58=` text; FIX makes
the text optional and QuickFIX sends none here either.

---

## 6. Journalling: pick the policy deliberately

Three policies, and the difference is which failure they survive:

| Policy | Cost | Survives |
|---|---|---|
| no journal (`NoJournal`) | zero | nothing |
| `Durability::Async` | one `write` per message, flushed by a background thread | a process crash, not power loss |
| `Durability::Fsync` | a disk sync per message | power loss |

For reference, QuickFIX's `FileStore` flushes without `fsync`, so its durability class is
`Async` ([reference/session-lifecycle-prior-art.md](reference/session-lifecycle-prior-art.md)).

**`Fsync` puts a disk on your hot path.** That is sometimes the right trade; it is not a
default to reach for without measuring it. Since
[ADR-0017](decisions/ADR-0017-the-inbound-count-is-persisted-after-delivery.md) it costs in
**both** directions: the journal also records which inbound numbers have been consumed, so
under `Fsync` receiving a message pays a sync too. Nothing here has measured that yet.

### How big the resend ring has to be, and what it costs

[ADR-0046](decisions/ADR-0046-the-ring-is-the-resend-store-and-a-replay-goes-in-batches.md).
**The in-memory ring is the whole resend store.** Disk is for restart and audit; the engine
thread never reads a file to answer a ResendRequest, because that is a blocking `read` on the
thread non-negotiable 4 protects. Anything older than the ring is gap-filled: legal on the
wire, and gone as far as your counterparty is concerned.

So the size is yours to choose:

> **`N` ≥ the number of application messages you send during the longest disconnection you
> are willing to replay across.** For most desks, one trading day.

`Store` is `MemJournal<4096, 512>` and costs `N × (SLOT_LEN + 8)`, about **2 MiB per session**.
A gateway holding hundreds of sessions should pick a smaller `N` through the const generic
(§1a).

**That 2 MiB is the number to hold in mind when sizing anything else.** `[measured 2026-09-04]`
one `Connection` is 23 752 bytes at the default `RX = 4096`, and 36 040 at `RX = 16 384` — so
quadrupling the receive buffer is **0.57%** of what a session already costs. The four sizes in
[CONFIGURATION.md](CONFIGURATION.md) are rarely worth economising on; the journal ring is where
the memory actually goes.

The messages a resend cannot reach are not lost quietly:

| What you see | What it means | What to do |
|---|---|---|
| `SessionSnapshot::resend_beyond_journal` non-zero, or `EventKind::ResendBeyondJournal { filled, oldest }` | a counterparty asked for `filled` messages the ring no longer held and got gap fills; `oldest` is how far back it reached | raise `N`, or accept that disconnections longer than `N` messages lose data |
| `SessionSnapshot::puts_refused` non-zero, or `EventKind::JournalRefused { count }` | your replies are longer than `SLOT_LEN`. They went out; they can never be replayed | raise `SLOT_LEN`, and re-check `resend_batch × SLOT_LEN < TX` |
| **Nothing at all** — a counterparty says you never answered, and no counter moved | your reply did not fit `APP`, the application's layout scratch. `Application::on_message` returning `None` means *"nothing to say"*, so this is indistinguishable from silence by design | raise `APP` through `serve_with`; `[measured 2026-09-05]` the default is 1 KiB and it is the tightest ceiling here — [a-ceiling-has-more-than-one-floor](reference/a-ceiling-has-more-than-one-floor.md) |

**`tools/jrnl` is how you get a message older than the ring**: by hand, from the file, off the
engine thread (§8c).

Two constraints the type system cannot hold:

- **`resend_batch × SLOT_LEN` must stay under `TX`.** The default is 8 × 512 = 4 KiB against
  8 KiB. Raise `SLOT_LEN` or lower `TX` and this is the number to re-check.
- **In `hft`, pre-build journals and call `add_with_journal`.** Plain `Engine::add` builds
  `J::default()`, a ~2 MiB allocation and 512 page faults **on the engine thread**
  ([best-practices-hft.md §6](best-practices-hft.md)).

### 6a. Your application must be idempotent per sequence number

**The engine can deliver the same message twice, and after a restart it sometimes will.**
[ADR-0017](decisions/ADR-0017-the-inbound-count-is-persisted-after-delivery.md): the inbound
count is written **after** your handler has seen a message, not before. A crash in that window
leaves the count on disk behind what your handler processed, so on restart the session asks
for a resend and your handler sees the message again.

That is the deliberate choice, and the alternative is worse. Writing the count first would
close that window by opening a bigger one: the message would be **lost**, because this end
would have counted it and never ask for a resend, while your counterparty believes it arrived.
FIX gives you a way to detect the duplicate and none to detect the loss.

**What you must do:**

- **Key on the sequence number, not on arrival.** The `seq` your handler is given is the
  counterparty's and is stable across a replay. Deduplicate on it where a repeat would be
  harmful: a duplicate order, a duplicate cancel.
- **A replayed message carries `43=Y`.** If you only need to *notice* a repeat, that flag is
  the signal.
- **The window is moved, not closed, and no engine can close it.** Nothing spans your
  handler's side effects and this engine's disk atomically. Putting both in your database's
  transaction is a fine place for it, but it is your transaction.

### 6b. A journal on disk, through the serving loop

`serve_with_recovery` is generic over the journal, so one `FileJournal` per counterparty is
reachable without giving up the serving loop
([ADR-0039](decisions/ADR-0039-a-fresh-journal-is-the-deployments-to-build.md)):

```rust
impl Recovery<FileJournal<64, 4096>> for OnDisk {
    // Called when the counterparty left nothing. The engine cannot build a
    // FileJournal for you: only you know the path.
    fn fresh(&mut self, cfg: &Config) -> FileJournal<64, 4096> { /* open it */ }

    fn recover(&mut self, cfg: &Config) -> Option<Resumed<FileJournal<64, 4096>>> {
        // All three numbers, computed once, correctly. `None` means
        // "this counterparty left nothing" — the engine then asks `fresh`.
        Resumed::from_journal(self.fresh(cfg))
    }
}
```

**`last_active()` is the field people skip, and it is the one that matters after a weekend.**
The engine records the instant when a session logs on and when an ordered shutdown says
goodbye, **not per message**, because that would be a disk write on the hot path.

Four things to know:

1. **`None` from `last_active()` means "this journal does not know"**, not "the session was
   never active". An in-memory journal answers `None`, and so does a file written before this
   existed.
2. **A process killed between logon and shutdown reports the logon instant**, which after a
   long session may be a day stale. There is no periodic mark.
3. **Nothing stops two processes opening the same file.** Both append and the records
   interleave. One journal, one process.
4. **`NoRecovery` and `FromFn` require `J: Default`**, so neither can carry a `FileJournal`. A
   file-backed deployment writes a named type, which it has to anyway, since only it knows
   which path belongs to which counterparty.

**Journal files carry a CRC32 per record from 2026-09-04.** A record that does not match its
checksum stops the read exactly as a torn tail does; `corrupt_records()` is the count, on
`Reader` and on `FileJournal` alike. A file written before that, or any file that existed
when this version first opened it, has no checksums and never will: one file, one format.

### 6c. The message log: both directions, refusals included

The journal answers *"what did we send, by sequence number"*. It cannot answer *"what did we
receive at 10:32:07, and what did we turn away"*: it holds outbound application messages
only, keyed by `seq`, and the frames that matter most in a dispute never got a `seq`. The
message log is the other file (D14).

```ini
FileLogPath=/var/log/fixbolt/messages.log
```

Or in code: `fixbolt::FileLog::open(path)` as the last argument to `serve`. `[DEFAULT]` only.
One engine writes one file; `conn=` and `shard=` tell the counterparties apart inside it. A
`[SESSION]` block carrying the key is refused at startup, because two counterparties asking
for two files is a configuration that cannot be honoured.

```
# conn=1 shard=0 peer=10.4.2.9:51422 opened at 20260903-10:32:07.118
20260903-10:32:07.120 IN  shard=0 conn=1 peer=10.4.2.9:51422 8=FIX.4.4␁…␁35=A␁…
20260903-10:32:07.120 OUT shard=0 conn=1 peer=10.4.2.9:51422 8=FIX.4.4␁…␁35=A␁…
```

`grep -v '^#'` is the messages; lines starting with `#` are the writer's own notes.

**Seven things the type system cannot tell you:**

1. **`OUT` means *queued*, not *sent*.** The line is written when the message reaches the
   outbound buffer, which is the only moment the engine can name it. A socket that dies takes
   that buffer with it, and the log then claims a send that never left the machine.
   `EventKind::MessageLogUnsent { bytes }` says how many bytes at the tail of that
   connection's output are wrong. Non-zero means read the end of that connection's lines with
   suspicion.
2. **Every `OUT` line written during one engine turn carries the same millisecond.** A turn
   reads the clock once. Order is the order of the lines in the file, never the timestamp.
3. **Losses are dropped and counted, never waited for.** A full ring means the writer is
   behind the engine: a slow disk, a network mount, a burst the ring was sized too small for.
   `Snapshot::log_lost` is the running total and `EventKind::MessageLogLost { count }` arrives
   unasked. Non-zero means the file has holes.
4. **A killed process leaves a torn last line, and it is marked rather than merged.**
   Reopening writes `# torn tail, N bytes, …` before appending. `FileLog::torn_tail_bytes()`
   is the same fact for a program.
5. **`0x0A`, `0x0D` and `\` inside a DATA field are escaped** to `\n`, `\r` and `\\`, so one
   message is always one line and the line decodes back to the exact bytes.
   `msglog::unescape` is the inverse.
6. **Rotation is yours.** `logrotate` with `copytruncate`, or move the file and restart. The
   engine never rotates, compresses or expires anything.
7. **It costs the engine thread a ring copy per message per direction**: roughly 340 ns for a
   200-byte message, so a request/reply pair pays it twice. `[unproven]`: that is arithmetic
   from [DESIGN.md §6](DESIGN.md), not a measurement of this module. What **is** measured is
   that it allocates nothing: `benches/alloc.rs` cases `log-record`, `log-idle` and
   `log-busy`.

In `hft`, give the writer thread a core that is not the engine's: `FileLog::open_pinned`. An
unpinned writer can land on the very core the engine was isolated onto.

---

## 7. The machine is part of your latency

`scripts/check-machine.sh` reads [DESIGN.md §9](DESIGN.md) off the running box and reports
which rows are not in force. Run it before you believe any number, yours or ours.

`[measured 2026-08-30]`, in order of how much they actually moved a benchmark here:

| Factor | Effect on the ring-hop **median** |
|---|---|
| **Anything else running on the machine** | **+71%**, 262 ns to 449 ns |
| Every §9 tuning row combined: governor, boost, SMT, THP, `busy_poll` | **0.8%** |

**The biggest factor is free: make the machine quiet.** A box that satisfies every tuning row
and shares a core with a build is worse than an untuned idle one.

**And one row moves nothing in that column and 11× in the one beside it.** Pinning the engine
thread to an `isolcpus` core is −2.9% at p50 and 10× at p99.9 (§1). A table of medians cannot
rank it at all. **If you tune for the tail, do not rank your settings by their effect on the
median**, which is the mistake this project made about `isolcpus` for two days and about
`nohz_full` in the opposite direction for one.

Two more, both traps rather than settings:

- **A VM cannot satisfy §9.** Governor, turbo, C-states, SMT and NIC IRQ affinity are *host*
  properties. A guest does not fail those rows loudly; the `/sys` files are simply absent.
  Develop on a VM if you like; do not measure on one.
- **`scaling_cur_freq` lies on an isolated core.** With `nohz_full` the governor's tick stops,
  the file freezes, and it read 41% low here while the core ran at full speed. Measure work per
  unit time instead.

---

## 8. How to benchmark this engine without fooling yourself

Everything in this section was paid for in this repository.

0. **Match the instrument's timescale to the mechanism you are asking about.**
   `[measured 2026-09-02]` this project measured an isolation setting as *free, with an
   unproven benefit* for two days, using a benchmark that timed a 500 ns operation. The stall
   that setting prevents is 250 µs long. It does not appear in that benchmark as a bad p99.9;
   it appears as one absurd sample nobody reads, or is discarded as an artefact. A percentile
   is a property of the instrument as much as of the system.
1. **Run your sample twice before you write down a rate.** `[measured 2026-08-30]` a
   pass-rate comparison read 8.3% against 3.3% at n = 60 and 5.6% against 5.6% at n = 250.
   Medians reproduce at small n; threshold crossings do not.
2. **Plot the distribution before you explain it.** A histogram of 500 runs here showed two
   discrete modes with an empty gap between them. Eight hypotheses were proposed and refuted;
   the shape was what turned out to be actionable, and it cost one plot.
3. **Check the instrument against a known state before believing a surprising reading.** Four
   instruments failed here in one day: `ps %CPU` (a lifetime average), a sampler slow enough
   to distort its own window, a "quietness" sampler whose wait loop was a busy spin, and
   `scaling_cur_freq` on a `nohz_full` core.
4. **A failed intervention still produces two clean-looking arms.** One A/B here compared a
   setting against itself because the command meant to change it had errored. Check that the
   variable actually moved.
5. **Measure a whole turn, including its syscalls.** Timing only the user-space part is how
   §8's budget came to exclude the syscall that dominates it.

Longer versions with the numbers: [reference/measured-costs.md](reference/measured-costs.md).

---

## 8a. Watching a running engine

The engine tells you what it is doing **only when you ask**, and asking costs one relaxed
load per turn while nobody is asking
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

**Four things about `request()`:**

1. **It never blocks.** It hands back the most recent snapshot the engine published and asks
   for a fresh one, so a snapshot is always a moment old. If you need one taken *after* your
   call, call twice.
2. **It returns `None` until the engine has published at least once.** `None` means "not
   yet", never "no sessions"; an engine with no sessions publishes a `Snapshot` whose
   `sessions()` is empty.
3. **The engine may skip a publish** if you hold the cell at that instant. It does not wait
   for you, and your request stays standing for the next turn.
4. **Do not poll it in a tight loop.** Every call makes the engine build a snapshot on its
   next turn. Once a second is an operator; ten thousand times a second is a load generator
   pointed at your own hot path.

### The one field people forget, and it is the one that saves you

`SessionSnapshot::last_skew_ms()` is **your clock minus theirs**, in milliseconds, from the
last inbound message whose `52=` could be read, **whether that message was accepted or
refused**.

`Config::max_skew_ms` refuses a message whose SendingTime is too far from your clock, and
before a Logon the protocol says to refuse **in silence**. So a box whose NTP has drifted
produces no error: the counterparty simply stops working, and nothing else in this engine
would tell you why. Watch this number, and alert on it long before it reaches `max_skew_ms`.

### Health

`Snapshot::healthy()` is a pure function on the same data: at least one session, every one of
them logged on, and both should-be-zero counters at zero (`refused_connections()`, a full
ring dropped a session; and `sources_missing()`, a `Transport` broke its own contract). Wire
it to whatever probes your process. `truncated()` is deliberately **not** unhealthy:
`MAX_SESSIONS` is 64 and `standard` has no session ceiling, so it means "more than I can
list", not "something is wrong".

### Why a connection ended

A snapshot tells you what **is**. It cannot tell you what **happened**: by the time you ask,
the session that ended is gone from it. So endings are **pushed**: the engine records one
when a session's state changes, whether or not anybody is reading
([ADR-0035](decisions/ADR-0035-an-event-is-pushed-and-a-loss-is-counted.md)).

```rust
let mut events = Vec::new();            // yours, on your thread; it may allocate
loop {
    watch.events(&mut events);          // appends; returns how many were added
    for e in events.drain(..) {
        match e.kind() {
            EventKind::LoggedOn           => println!("conn {} on", e.id()),
            EventKind::Ended(why)         => println!("conn {} ended: {why:?}", e.id()),
            EventKind::EndedWithoutReason => println!("conn {} ended, cause unrecorded", e.id()),
            _ => {}
        }
    }
    if watch.events_lost() > 0 { /* you fell behind; see below */ }
    std::thread::sleep(std::time::Duration::from_millis(200));
}
```

**`DropReason` is the point of this.** On the wire, a Logon refused for a bad clock and a
Logon refused because the venue is shut are the *same observable*: silence. They are also
different people's problems: `SendingTimeOutOfRange` is your NTP, `OutsideSchedule` is your
venue calendar. Six of the 59 acceptance definitions expect no response at all, so the
conformance gate is blind to the difference and only this stream is not. Every variant is
listed in [SESSION-BEHAVIOUR.md §1](SESSION-BEHAVIOUR.md).

**Three things to hold onto:**

1. **Read often enough.** The ring holds `EVENT_CAPACITY` (256) events and the engine never
   waits for you. When it overflows, `events_lost()` goes up. Check it: an event stream that
   loses silently is a source you would keep trusting, and a mass reconnect is both the moment
   it can overflow and the moment you need it.
2. **`ConnId` is not an identity.** An event names the connection, not the counterparty. If
   you need to know *who*, take a snapshot while the session is up and keep the mapping.
3. **`EndedWithoutReason` means the cause was not recorded**, not that there was none. A
   diagnostic that invents the most likely answer is worse than one that admits it does not
   know.

The kinds today: `LoggedOn`, `Ended`, `EndedWithoutReason`, `Administered` (§8c),
`ResendBeyondJournal` and `JournalRefused` (§6), `MessageLogLost` and `MessageLogUnsent` (§6c).
Gap detected, resend issued and reject sent are **not** here: they are message-rate, and
nothing message-rate goes on the hot path until its cost has been measured.

---

## 8b. Speaking first: what an initiator can be told to say

An acceptor answers. **An initiator has to start things**, and six of the things it starts
cannot come from the protocol: nothing on the wire asks for a Logout, and no timer produces
one. So they are calls you make:

```rust
session.send_heartbeat(emit);                    // 35=0, keepalive
session.send_test_request(b"OPS-7", emit);       // 35=1, your 112=
session.send_resend_request(4, 9, emit);         // 35=2, your 7= and 16=
session.send_sequence_reset(4812, emit);         // 35=4, and become 4812
session.begin_logout(b"end of day", emit);       // 35=5, then wait for theirs
session.send_application(&msg, &mut journal, emit);
```

**Four constraints the compiler cannot hold:**

1. **They are silent before the Logon is agreed and after the Logout.** Each returns `false`
   (or `Link::Dropped`) and sends nothing. "I called it" is not "it went out". **Read the
   return value.**
2. **You never write `34=`, `52=`, `49=`, `56=`, `8=`, `9=` or `10=`.** There is no function
   that takes whole message bytes, on purpose
   ([ADR-0042](decisions/ADR-0042-a-second-implementation-is-the-only-independent-opinion.md)).
   If you want one, the thing you want is `send_application`.
3. **`send_test_request` remembers nothing.** The counterparty answers with a Heartbeat
   echoing your `112=`, and matching the answer is yours: a session that waited for it would
   need a timeout, and a timeout is a clock this layer does not own. Choose a `112=` you can
   recognise; the session has one of its own for the request it raises after silence, and
   yours must not collide with it.
4. **`16=0` means "and everything after".** It is passed through, not refused. A range that
   runs backwards is not refused either, and `[measured 2026-09-02]` a real counterparty
   answers it with a gap fill rather than an error, so a mistake there is silent on both sides.
   Check your own arithmetic.

**Reconnect, backoff and a schedule for an initiator are the engine's** (§8c), and no part of
them is covered by the acceptance corpus or the interop gate (STATUS item 38).

**That list is the layer below.** If you are writing a `Handler` and using `serve`, §8d is
your section: you never hold a `Session`.

---

## 8c. Dialling out, and coming back

`connect_and_serve` runs **one initiator session** and does not give up when the connection
ends:

```rust
use fixbolt_engine::reconnect::Policy;

let policy = Policy::new(1_000, 30_000)?;   // 1 s, doubling, capped at 30 s
fixbolt_engine::connect_and_serve::<MyApp, fixbolt_engine::journal::Store, _, _>(
    "venue.example:9823",
    Config::initiator(b"FIX.4.4", b"ME", b"VENUE").with_heart_bt_int(30),
    MyApp::new(),
    policy,
    fixbolt_engine::recovery::NoRecovery,   // read the next paragraph before shipping this
    fixbolt_engine::msglog::NoLog,
)?;
```

**Four things it will not do for you**
([ADR-0043](decisions/ADR-0043-backoff-without-jitter-and-a-reconnect-asks-recovery-every-time.md)):

1. **`NoRecovery` restarts your sequence numbers on every reconnect.** That is right for an
   in-memory journal and **wrong for a counterparty that expects continuity**, which is most
   of them. If a reconnect must carry on from `34=N`, pass a `Recovery` backed by a journal on
   disk, the same one `serve_with_recovery` takes. This is the easiest mistake to make here,
   because "reconnect" sounds like it implies continuity and the type system will not stop you.
2. **There is no jitter.** Many initiators against one venue all come back at the same
   millisecond, at every rung of the ladder. One session per process is the shape this engine
   is built for.
3. **A schedule does not know when it next opens.** With `with_schedule(...)` it will not
   dial outside those hours; it re-asks once a ceiling. It cannot wake exactly at the open.
4. **`standard` only.** There is no `connect_and_serve_hft`. An `hft` deployment that dials
   out drives `Engine` itself.
5. **Use `Resumed::from_journal`, and never derive `next_out` from `Journal::highest`.**
   `[measured 2026-09-05]` this is the quiet twin of mistake 1 and it is easier to make,
   because it looks careful. `journal.highest() + 1` is short by every **administrative**
   message you sent after your last application one: `Journal::put` is offered application
   messages only — the journal is the resend store — while a `Logon`, a `Heartbeat` and a
   `Logout` each spend a `34=` all the same. A clean logout is enough to do it: you answer the
   venue's `35=5`, that spends a number, and your next `Logon` is one too low. A real
   `libquickfix` acceptor answers `MsgSeqNum too low, expecting 4 but received 3` and will not
   let you on.

   Since 2026-09-05 the journal records that count itself (`highest_out`), and
   `Resumed::from_journal` reads all three fields off it:

   ```rust
   fn recover(&mut self, cfg: &Config) -> Option<Resumed<MyJournal>> {
       Resumed::from_journal(self.open_for(cfg))
   }
   ```

   **You still have to give the engine a durable journal.** `from_journal` over an in-memory
   one answers `None`, which is *start fresh* — correct, and not continuity.
   [ADR-0053](decisions/ADR-0053-the-journal-answers-two-questions-and-the-second-is-a-number.md),
   [a-journal-holds-messages-not-numbering](reference/a-journal-holds-messages-not-numbering.md)

**Set the ceiling deliberately.** Without one, a long outage turns into an hour of silence and
your first sign of recovery is a phone call. 30 s is reasonable; the venue's own guidance beats
any default.

### The 3 a.m. phone call

The counterparty rings and says their next number is 4812. `Engine::admin()` hands you a
second handle over the same mechanism as the observer: `Observer` looks, `Admin` changes
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
| `SetNextIn { id, n }` | nothing | you decide what you expect. Never a lie |
| `SetNextOut { id, n }` | nothing | the counterparty has **already told you** their number. Same as QuickFIX's `setNextSenderMsgSeqNum` |
| `SendSequenceReset { id, n }` | `35=4`, `123=N`, `36=n` | **you** are the one changing the number. The only honest form |

**`SetNextOut` is a lie until the counterparty is told.** They still expect the old number,
so your next message draws a ResendRequest or is refused as too low. Reach for it only when
they have told you what to set; otherwise `SendSequenceReset` is the one you mean. A reset
that moves the number **down** is allowed and is a last resort: anything the counterparty
kept for a resend becomes ambiguous.

**Four things about `submit`:**

1. **`true` means queued, not done.** The engine applies it on its next turn, and the outcome
   arrives on the event stream as `EventKind::Administered { change, to, outcome }`.
2. **`Outcome::NoSuchConnection` is ordinary.** A command can race a disconnect, which is
   knowable only on the engine thread.
3. **`false` means the queue is full and nothing was taken.** Unlike a dropped event, a
   dropped command is never silent. `COMMAND_CAPACITY` is 32, sized for a person.
4. **Two identical commands produce two identical events.** There is no command id. Submit one
   at a time and read the outcome.

### Answering "did you receive it?"

The counterparty asks about an order from three weeks ago. Do not reach for `FileJournal`: it
reloads the file into a ring of `N` messages because its job is the next ResendRequest, and
the message being asked about left that ring long ago.

```
$ jrnl /var/lib/fixbolt/ISLD.journal --seq 4812
msg  4812  8=FIX.4.4|35=D|34=4812|11=ORDER-4812|...

$ jrnl /var/lib/fixbolt/ISLD.journal --count
messages 2000  inbound-marks 1  seq 1..2000  bytes 87794
```

`journal::Reader` is the same thing as a library
([ADR-0037](decisions/ADR-0037-reading-a-journal-is-not-recovering-from-one.md)).

**Three things to know before you trust the answer:**

1. **Check the exit code, or read stderr.** A file whose tail is torn, or a record whose
   checksum does not match, makes `jrnl` warn and exit **2**. Those bytes are not shown, so
   "no, we never received it" drawn from a damaged file might be wrong.
   `Reader::torn_tail_bytes()` and `corrupt_records()` are the same facts for a program.
2. **The whole file is read into memory.** Fine for a tool; a real limit for a very large
   journal.
3. **Do not read a file the engine is still appending to.** You will see a consistent prefix
   and probably a torn-tail warning. Nothing about that case is promised or tested.

`jrnl` does not decode FIX. It prints the bytes with `SOH` shown as `|` and leaves the rest to
`grep`; interpreting a message needs the dictionary, and a file reader has no business
pulling one in.

### Stopping without lying to the counterparty

Killing the process is not a shutdown. To the other end it is a dead line, so they reconnect,
and any bytes still in `tx` are lost having already spent their sequence numbers, so your
next session shows a gap for messages that never went on the wire.

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

**Read the report.** "We stopped" and "we stopped while two counterparties never answered"
are different facts ([ADR-0038](decisions/ADR-0038-an-ordered-shutdown-is-a-state-not-a-flag.md)),
and only the second means you may have to reconcile sequence numbers by hand.

**Five things the compiler cannot tell you:**

1. **`grace_ms` is on the engine's clock.** With `SystemClock` that is wall time. With a clock
   of your own that stops advancing, the deadline never arrives.
2. **Drop the engine after `run` returns, before the process exits.** A `Durability::Async`
   journal joins its writer thread on `Drop`; exiting without that can leave the tail of the
   file unwritten, which `jrnl` then reports as torn.
3. **Nothing catches `SIGTERM` for you.** The engine is a library.
4. **`serve_sharded_hft` cannot be stopped.** It has no shutdown path.
5. **Your application is not consulted.** There is no "let the dispatcher drain" phase, so an
   out-of-band dispatcher can lose work it had already accepted.

Two more entries from STATUS's *Not proven* matter here: **nothing authenticates the holder
of an `Admin`** (who you pass that handle to is the whole of the access control), and
**nothing stops accepting during a shutdown**, so a socket arriving in the grace period is
dropped rather than told anything.

---

## 8d. Speaking first from an application: two doors

`[added 2026-09-05]` Until this existed, **everything your application could send was an
answer**. `Handler::on_message` replies to one inbound message and there was nothing else. So
there was no way to send an `ExecutionReport` for a fill that lands a second after the order,
no quote stream, and nothing to say to a counterparty that is connected and quiet
([ADR-0048](decisions/ADR-0048-an-engine-that-can-speak-first-has-two-doors.md), `DESIGN.md`
D15).

**Door 1 — `Handler::on_logon`.** Anything that has to be said as the session opens.

```rust
fn on_logon(&mut self, who: Peer<'_>, nth: u32, reply: Reply<'_, P, S>) -> Answer {
    match nth {
        0 => reply.message(b"B").field(148, b"desk is up").send(),
        _ => reply.silent(),   // this is how you stop
    }
}
```

**Door 2 — `Sender`.** Anything said later, from any thread.

```rust
let tx = engine.sender();          // Send + Sync + Clone
std::thread::spawn(move || {
    if !tx.send(conn_id, &bytes) { /* the queue was full, or it did not fit */ }
});
```

**Six constraints the compiler cannot hold:**

1. **`on_logon` runs on the engine thread**, exactly like `on_message`. Everything §2 says
   applies to it: a lock, a database call or a log flush there stalls the session layer. If
   the work is not instant, hand it to another thread and use `Sender`.
2. **`nth` is how you stop.** The engine asks `0, 1, 2, …` until you answer `reply.silent()`,
   and stops on its own at `MAX_ON_LOGON` (16). If you never say silent you get sixteen
   messages, an `EventKind::SpokeFirstToTheBound` on the event stream, and no more.
3. **`Sender::send` returning `true` means queued, not sent.** The engine takes it at the top
   of its next turn. A `false` means **nothing was taken** — either the queue was full
   (`ORIGIN_CAPACITY`, 64) or the message was empty or longer than `ORIGIN_LEN` (512). Read
   the return value; this is the one loss that is reported at the call.
4. **A `Sender` message for a connection that has gone is dropped**, on purpose: the session
   that owned its sequence numbers went with it. Watch `Sender::undeliverable()` or
   `EventKind::OriginationUndeliverable`, and route by an id you got from a `Snapshot`, not
   by one you cached across a disconnect.
5. **A session that is not logged on discards silently.** `Sender::send` answers `true` — it
   queued — and the session then has nothing to do with it. Check `SessionSnapshot::logged_on`
   if that matters to you.
6. **You still never write `34=` or `52=`.** `Reply` leaves them out for an origination and
   the session writes them on the way out. Writing them yourself is not an error and is not
   respected either.

**And one the compiler cannot hold that is not about this engine at all:** the message you
originate is validated by the *counterparty's* dictionary. `[measured 2026-09-05]` a `35=B`
News carrying only `148=` is legal-looking, reaches the wire, replays correctly on a resend —
and is refused by the receiver, because `FIX44.xml` also requires the `33`/`58` group. Read
the message definition before you originate a type you have not sent before
([reference/a-message-on-the-wire-is-not-a-message-delivered.md](reference/a-message-on-the-wire-is-not-a-message-delivered.md)).

**`serve` does not hand you a `Sender` yet.** Door 2 needs an `Engine` you drive yourself;
through `fixbolt::serve` only door 1 is reachable. So is `Observer` and so is `Admin` —
the gap is older than this feature and is `STATUS.md` item 47.

---

## 9. What this engine does not do for you

Stated so you do not discover it in production:

- **It does not validate application-message semantics.** The dictionary validation is
  session-layer: required fields, types, enum values, structure. Whether a NewOrderSingle
  makes business sense is yours.
- **It does not decide *when* to resume sequence numbers.** `Session::resume` carries numbers
  across a restart and `next_out()` / `next_in()` are what you persist
  ([ADR-0010](decisions/ADR-0010-a-reconnect-is-not-a-restart.md)). A session built with
  `Session::new` resets. Reading the journal back and choosing `new` or `resume` is your call
  (§5a, §6b), and getting it wrong is a sequence-number dispute rather than a compile error.
- **Recovery does not reach the sharded runtime.** `serve_with_recovery` and
  `serve_hft_with_recovery` exist; `serve_sharded_hft` has no variant (STATUS item 32 a).
- **Its session schedule stops at the timezone.** Hours, weekday filter, week-long windows
  and the reset all work, in UTC. Resolving a venue's local time and rebuilding the `Schedule`
  when daylight saving moves it is yours (§5a). With an in-memory journal, persisting when a
  session was last active is yours too.
- **`serve_hft` pins nothing.** It runs the engine on the thread that called it, so that thread
  is yours to pin, with `affinity::pin_current_thread` before the call or `taskset` around the
  process. Skip it and [DESIGN.md §8](DESIGN.md)'s budget is not about your process. STATUS
  item 21.
- **It is not TLS-complete.** The blocking question is answered: kTLS can be driven from a
  plain non-blocking socket with no async runtime, under four conditions
  ([ADR-0018](decisions/ADR-0018-ktls-on-a-plain-socket-answers-adr-0005.md)). That is a
  spike, not a feature: no TLS code is merged, no TLS latency number is published, and the
  plan is a draft. Which kernel and which cipher suites are the floor, and what tells you a
  session fell back to the userspace path, are open.
- **It cannot originate an application message.** `Handler::on_message` returns one reply to
  one inbound message, and the session's `send_application` is reachable only by driving the
  session yourself (STATUS item 46).
