# ADR-0015 — Cores are named by the caller, pinned from inside the thread, and read back

> **Status:** **Accepted — 2026-08-31** · Step 1 of
> [threads-and-affinity](../plans/2026-08-30-threads-and-affinity.md).
>
> **Numbering.** That plan's own text says *"ADR-0013"* in four places and
> `STATUS.md` says ADR-0015. The plan was written before `standard-mode` took
> 0013 and 0014, and §5 forbids reusing a number. **0015 is the right one**, it
> was reserved for this step in [ADR-0018](ADR-0018-ktls-on-a-plain-socket-answers-adr-0005.md)'s
> header, and the plan is corrected in the same commit as this file.
>
> **Accepted by standing delegation.** `[2026-08-30]` the owner delegated
> plan-writing and plan approval to the agent working in this repository, and
> decided the same day that **the engine must let the user choose how many
> threads and which cores to pin**. That decision is the input to this ADR;
> nobody read the reasoning below on the owner's behalf.

- **Date**: 2026-08-31
- **Deciders**: Tran Manh Thang
- **Related**: [ADR-0012](ADR-0012-latency-first-and-one-session-per-polling-thread.md),
  [ADR-0013](ADR-0013-two-modes-standard-and-hft.md),
  [ADR-0014](ADR-0014-standard-mode-blocks-on-poll.md),
  [DESIGN.md §4 D8, §8, §9](../DESIGN.md), [GUIDE.md §1a](../GUIDE.md),
  `STATUS.md` open items 21 and 22

## Context

**`DESIGN.md` D8 says the engine thread is pinned to an isolated core. Nothing
pins it.** `[measured 2026-08-30]` a grep for `sched_setaffinity`, `affinity`,
`core_affinity` or `libc` across `crates/` and `tools/` returns nothing: no
dependency, no call, no test. §8's latency budget and §9's `isolcpus` row both
assume a pinned engine thread, so the design's central jitter defence is
**asserted in prose and absent from the code**. That is `STATUS.md` open item 21,
and `CLAUDE.md` §4's *"prose does not hold a constraint"* on the one claim where
it costs most.

The engine also does not shard. `Engine` holds a flat `Vec<Connection>`, `turn()`
sweeps all of them, `run()` is `loop { turn() }`, and `GUIDE.md` §1a currently
tells the reader that sharding, moving sockets between threads, pinning and core
isolation are **their** problem.

This ADR decides the shape. **It does not promise the engine gets faster.**
`[measured 2026-08-30]` pinning to an isolated core moved neither the median nor
the 324 ns second mode. What it changes is that a sentence in the design becomes
a checkable fact instead of an assumption.

### What the machine says, and why it changes two of the rules

`[measured 2026-08-31]` on the §9 desktop, tuned, `check-machine.sh` reading
`pass 10 fail 0`:

```
/sys/devices/system/cpu/present   0-15
/sys/devices/system/cpu/online    0-7
/sys/devices/system/cpu/offline   8-15
/sys/devices/system/cpu/isolated  6-7,14-15
/sys/devices/system/cpu/nohz_full 6-7,14-15
/sys/devices/system/cpu/smt/control  off
cpu6 thread_siblings_list  6
cpu7 thread_siblings_list  7
```

Two things fall out of that, and neither was in the plan.

**`isolated` names cores that cannot run anything.** `isolcpus=6,7,14,15` came
from the kernel command line; §9 then turns SMT off, which takes 8–15 offline.
The `isolated` file still lists 14 and 15. A validator that reads `isolated`
alone would accept core 14 and pin a thread onto a CPU that does not exist for
scheduling purposes. **The two files must be intersected, and `online` wins.**

**The SMT-sibling rejection can never fire on a correctly tuned machine.** §9
requires SMT off, and with it off every online CPU's `thread_siblings_list` is a
single entry. That is not an argument for dropping the rule — it is the shape of
every guard worth having: it fires on the machine that is *not* set up right,
which is the machine where the mistake gets made. It does mean the rule cannot be
tested for real here, and the test has to synthesise the topology rather than read
it.

## Decision

**1. The caller names explicit core ids. The engine never picks.**
The OS's idea of an idle core is wrong in this context: it does not know
`isolcpus`, it does not know where the NIC's interrupts land, and it does not know
that two ids can be two threads of one physical core. Auto-selection is how a
system that merely *looks* pinned gets built.

**2. Pinning happens from inside the thread, first thing, and is read back.**
`sched_setaffinity(0, …)` called by that thread before it does any work, then
`sched_getaffinity` compared against what was asked for. **A call returning `Ok`
is not evidence.** `CLAUDE.md` §10: a green result that was inferred rather than
observed is not a result.

**3. A failure stops startup. It never runs on unpinned.**
A process advertising itself as low-latency that quietly runs unpinned is worse
than one that refuses to start, because it looks fine. Errors are returned from
the constructor — no `panic!`, no `unwrap`, per non-negotiable 7.

**4. The error type carries the offending core.** `AffinityError` is **not**
fieldless. `CLAUDE.md` §6 asks for fieldless errors *where they sit on a hot
path*; this one sits at startup exactly once, and `NotIsolated(CoreId(3))` tells
the operator what to change where `NotIsolated` does not. The hot path stays
untouched.

**5. Four rejections, checked before any thread is created.**

| Rejection | Read from | Why |
|---|---|---|
| `NoSuchCore` | `present` | the id is not a CPU on this machine at all |
| `NotOnline` | `online` | **distinct from the above, and it is the one the §9 machine actually hits**: 8–15 are present and offline |
| `NotIsolated` | `isolated` ∩ `online` | the scheduler will put other work there; §9's `isolcpus` row exists for this |
| `SmtSiblingOf` | `topology/thread_siblings_list` | two shards on two threads of one physical core share it, and "a core each" becomes a lie |

`NotIsolated` is escapable with an explicit `allow_unisolated`, because a
development machine has no `isolcpus` and a rule that cannot be switched off
cannot be tested. **The default is to refuse**, and the flag appears in whatever
the engine reports about itself — a bypassed guard that leaves no trace is a
guard that gets bypassed permanently.

**6. `ShardPlan::validate()` runs before a single thread is spawned.** Half a
runtime that then refuses is worse than no runtime: it leaves threads to join and
sockets to close on an error path nobody exercises.

**7. Assigning sessions to shards belongs to the caller**, through a trait with a
round-robin default. Real deployments shard by counterparty, not evenly, and the
engine does not know which counterparty matters.

**8. Every thread this crate creates takes an affinity** — the engine threads, the
journal writer, and the `RingDispatch` consumer. Pinning the engine to an isolated
core and letting the journal writer float is self-defeating, because it can land
on that very core. Leaving one unset stays possible and the documentation says it
is a **choice**, not a harmless default.

**9. Behind a feature named `affinity`, gating the `mod` declaration itself**
(D5, non-negotiable 6), reusing the `libc` that `standard` already made optional —
one dependency tree, not two, which `crates/engine/Cargo.toml` already anticipated
in writing.

**10. Exactly one `unsafe` block**, around the `sched_setaffinity` call, with a
comment naming the test that reads the mask back (non-negotiable 8). If a second
one appears, the design is wrong and this ADR is what it has to argue with.

**11. Orthogonal to the mode.** The plan scoped its *motivation* to `hft`, because
D8's sentence is an `hft` sentence. The mechanism is not mode-specific: a
`standard` engine may be pinned, and the four rejections mean the same thing in
both modes. What stays `hft`-only is the claim in `DESIGN.md` §8 that the pinned
thread has the core to itself.

### The shape

```rust
pub struct CoreId(pub usize);

pub enum AffinityError {
    NotSupported,                    // not Linux, or the feature is off
    NoSuchCore(CoreId),
    NotOnline(CoreId),
    NotIsolated(CoreId),             // escapable with allow_unisolated
    SmtSiblingOf(CoreId, CoreId),
    Denied(CoreId),                  // EPERM
    ReadbackMismatch(CoreId),        // set returned Ok; the mask disagrees
    DuplicateCore(CoreId),           // two shards, one core
}

pub struct ShardPlan {
    shards: Vec<CoreId>,             // one shard per core; count = len
    journal_core: Option<CoreId>,
    consumer_cores: Vec<CoreId>,
    allow_unisolated: bool,
}

impl ShardPlan {
    pub fn validate(&self) -> Result<(), AffinityError>;
}
```

`DuplicateCore` is not in the plan's sketch and is added here: two shards naming
the same id is the same lie as two shards on SMT siblings, and it is the easier
mistake to make.

## Consequences

**Good**

- `DESIGN.md` D8 stops being a claim the code does not honour. Open item 21
  closes on a mechanism, not on a rewording.
- The failure that this design most needs to prevent — *running unpinned while
  reporting low latency* — becomes impossible rather than unlikely.
- Reading `online` as well as `isolated` catches a real configuration on the real
  reference machine, found by reading the files rather than by reasoning about
  them.
- `serve()` and `serve_hft()` keep their signatures. Sharding is a second road,
  not a replacement, so nothing existing breaks.

**Bad, and named**

- **The first `unsafe` in `engine`.** `unsafe_code = "warn"` will fire, and that is
  correct — it is meant to be visible. The mitigation is that it is one block
  around one call with a read-back test behind it, and that a second block is a
  signal to stop.
- **`allow_unisolated` will be set and left set.** Every escape hatch is. The only
  defence chosen here is that it is reported rather than silent, and that is
  weaker than an enforcement.
- **A rule that cannot be exercised on the reference machine.** SMT is off on any
  §9 box, so `SmtSiblingOf` can only be tested against a synthesised topology.
  That means the *reading* of `thread_siblings_list` is tested and the *reality*
  is not, which is a genuine hole rather than a formality.
- **More API surface, for something that does not make the engine faster.**
  `[measured 2026-08-30]` pinning moved neither median nor mode. This buys
  truthfulness and a jitter defence that is still unmeasured, not throughput.
- **Sharding freezes an assignment.** A session stays on the shard it was given;
  rebalancing at runtime is deliberately out of scope and would need its own ADR.
- **`/sys` parsing is a new class of code** in a crate that had none — string
  handling, ranges like `6-7,14-15`, and files that differ between kernels. It is
  all startup, and it is all places `unwrap` would be easy to write.

## Alternatives considered

**Let the engine choose cores.** Rejected in decision 1: the OS's notion of a free
core is blind to `isolcpus`, IRQ placement and SMT topology, and the result looks
pinned without being pinned.

**Pin from the spawning thread with `sched_setaffinity(tid, …)`.** Works, and it
needs the child's tid, which means a handshake, which means a window in which the
new thread runs unpinned. Pinning from inside, first thing, has no window.

**Trust `sched_setaffinity`'s return value.** Rejected on the same grounds that
made `check-ktls-available.sh` call a syscall instead of grepping a config file,
and that made `check-no-kernel-sleep.sh` read the mode back out of the binary. A
success code says the call was accepted, not that the state is what was asked for.

**Verify pinning with `scaling_cur_freq`.** Rejected on measurement:
`[measured 2026-08-30]` it freezes on a `nohz_full` core, so it would report the
same number whether the thread was there or not — a check that cannot fail.

**Warn instead of refuse.** Rejected by decision 3. A warning in a process that
starts anyway is read once, on the day it is added.

## Open questions

1. **What is the residency check in a test?** `sched_getcpu()` is one syscall and
   is what the thread itself sees; the `processor` field of
   `/proc/self/task/<tid>/stat` is an observation from outside the thread. The
   plan asks for the second. Whether the first is enough for a unit test, with the
   second reserved for the gate, is decided in step 2.
2. **Does `Engine` gain a shard-aware constructor, or does `shard.rs` own a
   `Vec<Engine>`?** The second keeps `Engine` untouched and is the current
   intention, but it has to survive the `Acceptor` handing sockets across a
   channel — which is itself a queue this design has opinions about
   ([ADR-0011](ADR-0011-a-full-ring-disconnects.md)).
3. **What happens when a pinned thread panics?** The rest of the shards keep
   running with one core silently idle. Nothing here decides whether that is a
   process-level failure.
4. **Does `check-no-kernel-sleep.sh` still mean anything with several engine
   threads?** It attributes by tid and takes the first `engine-tid` it finds. With
   M shards it would check one of M. That is step 4's problem and it is named here
   so it does not become a surprise.
