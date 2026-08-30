# ADR-0014 — `standard` blocks on `poll(2)`, and the waiter is given the sockets

- **Status**: **Proposed — 2026-08-30**. Awaiting the owner's signature. Step 2 of
  [plans/2026-08-30-standard-mode.md](../plans/2026-08-30-standard-mode.md) does not start until
  this is `Accepted`.
- **Date**: 2026-08-30
- **Deciders**: Tran Manh Thang
- **Related**: **answers all four open questions of
  [ADR-0013](ADR-0013-two-modes-standard-and-hft.md)**,
  [ADR-0012](ADR-0012-latency-first-and-one-session-per-polling-thread.md) decision 3,
  [DESIGN.md §4 D5, D8, §6, §8](../DESIGN.md),
  [plans/2026-08-30-standard-mode.md](../plans/2026-08-30-standard-mode.md),
  [plans/2026-08-30-threads-and-affinity.md](../plans/2026-08-30-threads-and-affinity.md)
  (which takes the same dependency), `CLAUDE.md` §2 non-negotiables 1, 6, 7, 8

> **On the number.** `docs/decisions/` has no ADR-0009 and that is **a deliberate gap, not a
> lost file**: [plans/2026-08-30-gates-that-can-be-trusted.md](../plans/2026-08-30-gates-that-can-be-trusted.md)
> claimed the number for a `SessionUnderTest::step` API change, then dropped the design and
> deleted the hook instead of shipping it — that plan's own delivery log says so. `CLAUDE.md` §5
> forbids reusing a number, so 0009 stays empty and this ADR is 0014. The plan
> `threads-and-affinity` also expected 0014 for its own step 1; it was not written first, so it
> takes 0015.

## Context

[ADR-0013](ADR-0013-two-modes-standard-and-hft.md) was signed on 2026-08-30: two modes, and
`standard` — which blocks when idle and gives the core back — **is the default**. It left four
questions open on purpose, because they are implementation choices that deserve their own
decision rather than being settled inside a plan.

`standard` does not exist. The engine spins, always. [DESIGN.md](../DESIGN.md) D8 and
[GUIDE.md](../GUIDE.md) §0 both print *"not built yet"* to their readers, and anybody who runs
the engine as shipped sees a process pinning a core at 100% and concludes it is broken.

### What the code says today

`[read 2026-08-30]`

1. **`Waiting` cannot express `standard`, by signature.** `fn idle(&mut self)` takes no
   parameters, so no implementation of it can see a socket. `Spin` is
   `core::hint::spin_loop()`, `Park` is `std::thread::yield_now()`; neither needs to.
2. **`Transport` does not say what it is.** `recv` and `send`, and nothing else. There is no way
   to ask a transport for something a poller could wait on.
3. **`std` exposes no readiness API** — ADR-0013's third fact, unchanged. `set_nonblocking` and
   `WouldBlock` are the whole surface.
4. **`engine` has no external dependency**, and neither does any other crate in the workspace
   except `dict`'s build-time XML reader.
5. **Nothing in the workspace uses `dyn Transport`, `dyn Waiting` or `dyn Dispatch`** — `grep`
   for `dyn` across `crates/` and `tools/` returns nothing. So an associated const on either
   trait costs no object safety that anything is using.
6. **`const { assert!(…) }` is already an idiom here** — `crates/engine/tests/transport.rs:110`
   uses it on `Spin::SLEEPS`.
7. **The gate that guards `hft` lists the calls `standard` must make.**
   `scripts/check-no-kernel-sleep.sh` fails on
   `epoll_wait|epoll_pwait|poll|ppoll|select|pselect6|futex|nanosleep|clock_nanosleep|sched_yield|io_uring_enter`.

## Decision

### 1. The mechanism is `poll(2)`, through `libc`, behind a default-on `standard` feature

ADR-0013 open question 1, first half.

**Why `poll(2)`.** It is one call that works on Linux and macOS, it is in `libc`, and it needs no
runtime. `epoll` and `kqueue` would be **two** mechanisms and two code paths, bought for a
difference nobody here has measured — and `CLAUDE.md` §2 rule 10 is about exactly that kind of
purchase. `epoll` is a later ADR, and it arrives with numbers or not at all.

**Why not `polling` or `mio`.** They buy Windows, which decision 2 puts out of scope, and they
bring a dependency tree to do it. Neither is wrong; both are premature.

**Why `libc` is an acceptable dependency for `engine`.** It has no transitive dependencies, it
pulls in no async runtime — which `CLAUDE.md` §6 would make a separate ADR of — and it is **the
same dependency [threads-and-affinity](../plans/2026-08-30-threads-and-affinity.md) already
takes** for `sched_setaffinity`. One tree, not two.

**The feature is `standard`, and it is on by default**, because the mode is the default. The
consequence is worth naming as a property rather than discovering later:
`--no-default-features` yields an engine with **no dependency at all**, `hft` only, and the CI
job that builds exactly that already exists.

**The cost, stated here rather than found later:** `poll(2)` is **O(N) in registered
descriptors per wakeup**, where `epoll` is O(1). At the shape `standard` is for — many sessions
on one blocking thread — that is a real term and it is **unmeasured**. It is accepted, not
dismissed; see open question 2.

### 2. Windows is out of scope, and is refused with a typed error — never a silent spin

ADR-0013 open question 1, second half.

An engine that quietly falls back to spinning on a platform it does not support is precisely the
failure ADR-0013 exists to prevent: it looks like it is doing what you asked and it is burning a
core. A platform with no poller returns a typed error from the constructor and does not run.

### 3. `Waiting` is given the sources; `Transport` says what its source is

ADR-0013 decision 2 says blocking on readiness is a `Transport` concern *because the waiter must
know the sockets*. But the idle turn is still `Waiting`'s. The seam that joins them: let the
waiter **see** the sources, and let each transport **name** its own.

```rust
/// A handle a poller can wait on. On POSIX, a file descriptor.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Source(/* RawFd, #[cfg(unix)] */);

/// One source, and what is being waited for on it.
#[derive(Clone, Copy)]
pub struct Interest {
    pub source: Source,
    /// Also wait for the socket to accept bytes, not only to deliver them.
    pub writable: bool,
}

pub trait Waiting {
    /// Whether this strategy leaves user space.
    const SLEEPS: bool;
    /// Whether `idle` needs the source list to be correct and complete.
    /// `Spin` does not; `Block` does.
    const NEEDS_SOURCES: bool;
    fn idle(&mut self, interests: &[Interest]);
}

pub trait Transport {
    fn recv(&mut self, buf: &mut [u8]) -> Io;
    fn send(&mut self, buf: &[u8]) -> Io;

    /// Whether this transport can be waited on at all.
    const POLLABLE: bool = false;
    /// The handle to wait on. `Some` whenever `POLLABLE`.
    fn source(&self) -> Option<Source> { None }
}
```

`Spin::idle` ignores its argument and optimises away. `Block::idle` hands it to `poll`.

`source()` has a default body, so a transport somebody else wrote keeps compiling; it simply
cannot be used in `standard`, which decision 4 turns into a compile error rather than a surprise.

### 4. A transport that cannot be waited on is refused **at compile time**

In the body of `Engine::run`:

```rust
const { assert!(!W::NEEDS_SOURCES || T::POLLABLE, "standard mode needs a pollable transport") };
```

**Why compile time.** The alternative is a runtime check in `Engine::add`, which would have to
return `Result` where it now returns `ConnId` — changing every call site to report a condition
that is a property of the *types*, known before the program runs. `Loopback` is the case that
matters: it is the corpus's transport, it has no descriptor, and it must never silently become a
`standard` engine that wakes only on its timeout.

**And the refusal is proven by reversal without a dependency**: a `compile_fail` doctest pairing
`Block` with `Loopback`. `compile_fail` is rustdoc's, and `cargo test` runs it.

### 5. The poll timeout is the tick granularity. Default 100 ms, configurable

`Session` takes no clock — D1 — so it judges `SendingTime` and heartbeats against the last
`Input::Tick` it was given. In `hft` the engine ticks every turn. In `standard` the engine is
asleep, so **the poll timeout is the coarsest grain of time the session can see**, and it is a
correctness parameter, not a tuning knob.

100 ms. `HeartBtInt` is a whole number of seconds and the three thresholds are 1.0, 1.2 and 2.4
times it, so 100 ms is a tenth of the smallest interval that means anything. Fully idle, it costs
ten wakeups a second; the plan's step 7 gate measures what that is worth rather than assuming it
is nothing.

### 6. The source set must be **complete**, and every omission costs exactly one timeout

This is the part of `standard` that will go wrong, and the reason it will go unnoticed is that
**every one of these still works.** Each simply adds up to one whole timeout of latency:

| Source left out | What it costs |
|---|---|
| The listener's descriptor | a new connection waits up to one timeout to be accepted |
| Writable interest while `tx` is still queued | a stalled flush waits up to one timeout |
| A waker for out-of-band dispatch | an application reply from another thread waits up to one timeout |
| Nothing — the timeout is set to 0 | not blocking at all; a spin wearing `standard`'s name |

So each gets a test that asserts **elapsed time**, not `is_err()`, and each is proven by removing
that one source and watching the test go red by exactly one timeout.

**The waker is a self-pipe**, not `eventfd`: `eventfd` is Linux-only and would split this code
path across two platforms for nothing. `RingDispatch` writes one byte on push; the read end sits
in the poll set.

### 7. `wait::Park` becomes `wait::Yield`, and is documented as **neither mode**

ADR-0013 open question 3.

It cannot be deleted: every test in the repository names it, and **none of them needs to block** —
they drive `Engine::turn` by hand. A test suite that spins pins a core for nothing; a test suite
that blocks needs real sockets. `Yield` is the useful thing in between.

What must change is that it sits beside `Spin` as though the two were peers. After this:

- `wait::Spin` — `hft`.
- `wait::Block` — `standard`, behind the feature.
- `wait::Yield` — **neither**, for tests, and its rustdoc says so: it **fails the `hft` gate**
  (`sched_yield` is on that gate's list) **and fails the `standard` gate** (it burns the core).
  That is its definition, not a defect in it.

**And the `hft` gate gets stronger as a side effect.** The red half of
`check-no-kernel-sleep.sh` moves from `--park` to `--mode standard`. `sched_yield` is not
something anybody writes into an engine by accident; a blocking readiness call is exactly what an
accidental regression would look like. Which syscall name appears — `poll` or `ppoll` — is **not
asserted here**: glibc may route `poll(2)` through the `ppoll` syscall, the gate's list contains
both, and the plan's step 6 reads the trace rather than predicting it.

### 8. `hft` stays the default for `tools/w2w` and the benchmarks

ADR-0013 open question 2. They exist to produce `hft` figures; changing their default would
silently change every number this project has published. `w2w` gains `--mode standard|hft`,
default `hft`, and **prints its mode on every run** — ADR-0013 decision 4 already requires every
figure to name its mode.

### 9. `density` is a shape within `standard`, not a third mode

ADR-0013 open question 4. Many sessions on one blocking thread *is* the ordinary way to run
`standard`; giving it a mode of its own would create a third thing with nothing of its own to
say. The word stays useful as a **label on a figure**, beside its `N`, and ADR-0012 decision 3 is
unchanged.

## Consequences

**Good**

- The documented default becomes real. The engine stops looking broken to anyone who runs it
  without reading `DESIGN.md` §9.
- **`--no-default-features` becomes a shape worth having**, not just a compile check: zero
  dependencies, zero `unsafe`, `hft` only. CI already builds it on a runner with nothing
  installed.
- Non-negotiable 4's second half becomes machine-checkable, which is what makes `CLAUDE.md`'s
  own "machine-checked today" list stop saying *half*.
- `Transport::source()` has a default body, so nothing anybody else wrote stops compiling.
- The four latency cliffs above are named **before** the code exists, each with the test that
  catches it. That is the only reason they will be caught: all four produce a working engine.

**Bad, and stated now**

- **`engine` gains its first external dependency.** Jointly with the affinity plan, but this ADR
  is the one that may land first. `codec`'s zero-dependency rule is untouched; `engine`'s
  "zero" becomes "zero with no features on", which is a weaker sentence and the documentation
  must say the weaker one.
- **The crate gains its first `unsafe`**, in two blocks: the `poll` call and the self-pipe. Each
  names the test that reads its result back. `unsafe_code = "warn"` will fire, and it should.
- **`Waiting` changes signature and `Transport` gains two members** — a breaking public API
  change. Nothing is published, so the price is a `CHANGELOG.md` entry and the call sites in this
  repository; it would be a real price later.
- **`poll(2)` is O(N) per wakeup.** Accepted without measurement, which is a debt and is
  recorded as open question 2 rather than as a decision that "should be fine".
- **The associated consts cost object safety** on `Transport` and `Waiting`. Nothing uses `dyn`
  on either today; anything that wants to later must move those consts to methods and give up
  decision 4's compile-time refusal.
- **Windows users get a refusal**, which is better than a silent spin and is still a refusal.
- **The default gets slower and that number will be quoted against the project.** ADR-0013 said
  this already; this ADR is what makes it true.

## Open questions

1. **What does a `standard` wakeup actually cost on the §9 machine?** `DESIGN.md` §8's 2–5 µs is
   from the literature and says so. The plan's step 8 measures it, or the row keeps its label.
2. **Where is the crossover from `poll(2)` to `epoll`?** O(N) against O(1), at what `N`, for
   which timeout. Unmeasured. A later ADR, with numbers, or nothing.
3. **Should the timeout adapt** — short while a session is active, long while everything is idle?
   Not decided. 100 ms flat is the starting point, and an adaptive timeout is a behaviour change
   that would need its own evidence that the flat one costs something.
