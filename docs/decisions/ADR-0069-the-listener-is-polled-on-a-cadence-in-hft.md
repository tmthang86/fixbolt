# ADR-0069 — The listener is polled on a cadence in `hft`, and on every wake in `standard`

**Status:** Proposed (architect, 2026-09-18; step 1 of
[polling-the-listener-less-often-than-the-sessions](../plans/2026-09-18-polling-the-listener-less-often-than-the-sessions.md),
whose questions Q1–Q3 the owner answered as recommended). Becomes *Accepted* in the commit that
publishes the plan's step 5 measurement, and not before. · **Date:** 2026-09-18 ·
**Plan:** docs/plans/2026-09-18-polling-the-listener-less-often-than-the-sessions.md
**Answers:** `STATUS.md` open item 89 — *whether the listener should be polled less often than
the sessions, and what that measures as*.
**Changes nothing in:** [ADR-0012](ADR-0012-latency-first-and-one-session-per-polling-thread.md)
and [ADR-0013](ADR-0013-two-modes-standard-and-hft.md) — both halves of `CLAUDE.md` §2 rule 4
stand, and this ADR adds a rule that keeps the `standard` half true;
[ADR-0014](ADR-0014-standard-mode-blocks-on-poll.md) — the listener stays in the poll set;
`serve_sharded_hft` (`crates/engine/src/shard.rs`), whose acceptor is a blocking thread of its
own and never touches a shard's loop.

## Context

`[measured 2026-09-15]` Boot B step B9
([measured-costs](../reference/measured-costs.md), *B9 — perf on the engine thread*): over
400 000 loopback round trips in `hft`, `fixbolt_engine::Acceptor::accept` holds **49.9%** of the
engine thread's samples on the administrative path and **36.7%** on the application path;
`__x64_sys_accept4` alone 27.7% / 18.3%. The callchain under `do_accept` is `sock_alloc_file`,
`inode_init_always_gfp`, then `__fput`, `evict`: the kernel builds a socket file and an inode and
tears them down again before answering `EAGAIN`.

Those are shares of a spin loop, not per-message costs. The loop is `pump`
(`crates/engine/src/lib.rs`): every iteration asks the listener first, then turns the pre-session
set, then turns every connection, then idles if nothing moved. `tools/w2w` carries a `pump` of the
same shape. So a request landing on a session socket while the thread is inside `accept4` waits
for that syscall — allocation, teardown and all — before it is read. How long that is has never
been measured; B9 says only that it is a large part of every iteration.

Three facts bound the design:

1. **The allocation is inherent to `accept4`, on every kernel.** `do_accept` in `net/socket.c`
   (torvalds master) calls `sock_alloc()`, `sock_alloc_file()` and `security_socket_accept()`
   *before* `ops->accept()`; an empty queue is discovered last and unwound through `out_fd`
   ([net/socket.c](https://raw.githubusercontent.com/torvalds/linux/master/net/socket.c)).
2. **There is no syscall-free way to ask whether the queue is empty.** `tcp_ioctl` answers
   `SIOCINQ` and `SIOCOUTQ` with `-EINVAL` on a `TCP_LISTEN` socket
   ([net/ipv4/tcp.c](https://raw.githubusercontent.com/torvalds/linux/master/net/ipv4/tcp.c)).
   `poll(fd, 1, 0)` does not allocate but is a syscall, and its name is in
   `scripts/check-no-kernel-sleep.sh`'s `SLEEPERS` list.
3. **In `standard` the listener is a poll source.** `pump` hands it to `Engine::idle_with`, so a
   connect wakes the poller (D8, ADR-0014). A wake caused by the listener that is then *not*
   followed by an `accept` leaves the listener readable, and the next `poll` returns at once —
   for ever. That is a working engine burning a core, the one thing `standard` exists to avoid.

What sibling systems do: Seastar polls its reactor sources on a ~0.5 ms period rather than every
iteration and stops polling I/O when the task backlog is high
([seastar #652](https://github.com/scylladb/seastar/issues/652),
[seastar-dev](https://groups.google.com/g/seastar-dev/c/YCi-jbD4TC4)); Aeron runs its
Conductor on a duty cycle and idle strategy separate from the Sender and Receiver
([media driver](https://aeron.io/docs/aeron/media-driver/),
[agents and idle strategies](https://aeron.io/docs/agrona/agents-idle-strategies/)). Onload's
spin applies to blocking `accept()` only; non-blocking sockets "always return immediately"
([EF_SPIN_USEC](https://docs.amd.com/r/en-US/ug1586-onload-user/EF_SPIN_USEC)). io_uring's
multishot accept arms once and completes per connection
([io_uring_prep_multishot_accept(3)](https://man7.org/linux/man-pages/man3/io_uring_prep_multishot_accept.3.html),
[LWN](https://lwn.net/Articles/868303/)). No engine was found that publishes what a
non-blocking `accept4` on an empty listener adds to a message's latency.

## Decision

1. **The listener is asked on a cadence, counted in loop iterations, in the spin half.** `pump`
   keeps one `u32` countdown; the accept loop runs when it reads zero and is then reloaded with
   `listener_every − 1`; otherwise it is decremented and the accept loop is skipped. No clock is
   read for this and nothing is allocated.
2. **The cadence is a `Limits` field: `listener_every: NonZeroU32`, default 1.** Default 1 is
   today's loop, iteration for iteration. The settings key is `listener_every_turns`.
   `Limits::new(pending, logon_ms)` keeps its signature; the value is set by a builder method.
3. **After every return from `idle_with`, the countdown is reset to zero.** The next iteration
   asks the listener unconditionally. This is what keeps rule 4's `standard` half true (fact 3):
   a wake is answered by an accept, so the poll set is drained and the poller blocks again. The
   rule is not gated on the mode; in the spin half `idle` returns at once, so the reset is
   harmless there and has a meaning of its own (consequence 3 below).
4. **The default moves only with a figure.** A default other than 1 is set in the commit that
   publishes the step-5 A/B (N ∈ {1, 16, 256}, interleaved within one procedure, two procedures,
   verdict per [ADR-0068](ADR-0068-a-published-figure-is-two-procedures-shown-side-by-side.md)),
   naming the benchmark, the machine and the §9 settings (`CLAUDE.md` §2 rule 10). A difference
   under ADR-0068's 5% is published as *not distinguishable* and the default stays 1.
5. **Rule 4's gates do not change.** `accept4` was never in `SLEEPERS`; fewer calls of it is not
   something `check-no-kernel-sleep.sh` sees. `check-standard-gives-the-core-back.sh` is run
   with `--listener-every 64` as well as without, and the test
   `standard_accepts_on_the_wake_not_after_n_wakeups` holds decision 3.

## Alternatives considered

- **(a) A fixed cadence in iterations — chosen.** One subtraction and one branch per iteration;
  the worst-case accept delay is a closed formula (consequence 2). An adaptive N is a later ADR if
  a fixed one measures short.
- **(b) A cadence in time — every X µs from the clock `pump` already reads.** No new clock read,
  but the delay it buys is then a function of load: at 0 sessions an iteration is tens of
  nanoseconds and X µs is thousands of them; at 8 busy sessions it is a handful. The iteration
  count says directly how many turns a connect can wait, which is the number `GUIDE.md` has to
  state. Rejected for having the wrong unit, not for cost.
- **(c) Leave the loop alone; record that spin share is not latency.** True, and it is what B9
  already says. It closes item 89 with no latency number at all, and the number is cheap to take
  once the knob exists. Rejected as a stopping point; it is the outcome if the A/B shows nothing.
- **(d) Move the listener off the engine thread** — a blocking acceptor thread handing sockets
  over a queue, as `shard.rs` already does — or io_uring multishot accept. (d) is the only option
  that removes `accept4` from the engine thread entirely. The thread form changes `serve_hft`'s
  contract (it spawns no thread; the caller pins — D8, GUIDE.md §9); the io_uring form needs a
  dependency, `unsafe`, Linux only, and `io_uring_enter` is in `SLEEPERS`. Deferred to a plan of
  its own, opened only if (a) leaves more than ~1 µs of `accept4` in the round trip at N = 256
  (plan Q3, answered: decide after the numbers).

## Consequences

Good:

- **A latency, not a share.** Item 89's question gets an answer in nanoseconds of round trip,
  taken by the committed procedure, or the honest reading that the effect is inside the drift.
- **Today's behaviour is the default.** N = 1 reproduces the current loop exactly; nothing on the
  hot path changes until a figure says it should, and `listener_every_one_is_todays_loop` runs
  the 59 definitions over a socket to hold that.
- **`standard` is untouched in effect.** Decision 3 makes every wake an accept, so the poll set
  is drained as before; `serve_sharded_hft` never enters this code.
- **The cost of a large N is a formula, not a surprise.** See the next list.

Bad:

- **When the engine is idle, N saves nothing.** `Spin::idle` returns at once, the countdown is
  reset, and the listener is asked on the next iteration — every iteration, as today. The cadence
  only thins the listener while something is moving, which is exactly when an `accept4` sits
  between two messages, but it means B9's spin-share figure will *not* fall on an idle engine and
  must not be read as a result.
- **Maximum accept latency is N × one iteration.** An iteration is roughly `Engine::turn` at
  ~449 ns per session (`[measured 2026-08-31]`, `benches/turn.rs`, DESIGN.md §8) plus the
  pre-session turn; a connect arriving just after the listener was asked waits up to that many
  iterations under load. At N = 256 and one busy session that is on the order of 100 µs (`[estimated]`, arithmetic
  from the per-session figure; the pre-session turn and an empty iteration are unmeasured); with
  many sessions it grows linearly. `docs/CONFIGURATION.md` states the formula and `GUIDE.md`
  names reconnect storms as the case to size N for.
- **A second copy of the rule.** `tools/w2w` has its own `pump`, so the countdown and the
  reset-on-wake exist twice; the w2w copy is what the A/B measures, the engine copy is what ships,
  and a drift between them would measure one loop and publish it for the other. The plan's
  step 4 mirrors the engine's rule line for line; a shared helper is not attempted here because
  the w2w loop is deliberately kept separate so a reversal that changes it proves nothing about
  the engine (its own rustdoc).
- **The figure may not clear the drift.** Boot B moved nine of ten interval-0 loopback arms by
  4.7–6.7% between procedures; the expected effect is at most about one `accept4` per message (`[estimated]` from the
  loop's shape, which is what step 5 measures).
  If it is under 5% the ADR is accepted with "not distinguishable", the knob stays at 1, and item
  89 closes on that — a real result, but not the one the profile suggested.
- **The reset-on-wake rule is a line somebody can delete.** Its absence is invisible in `hft`
  and catastrophic in `standard`. It is held by a test and a script reversal (plan, *Cách kiểm
  chứng*), not by prose.

## Sources

- `docs/reference/measured-costs.md`, *B9 — perf on the engine thread, diagnostic*
  (`[measured 2026-09-15]`).
- `crates/engine/src/lib.rs`, `pump` and `Engine::idle_with`; `crates/engine/src/shard.rs`,
  module note *Why the acceptor thread is allowed to block*; `scripts/check-no-kernel-sleep.sh`,
  `SLEEPERS`.
- Linux `net/socket.c`, `do_accept`; `net/ipv4/tcp.c`, `tcp_ioctl` — URLs above, read 2026-09-18.
- Seastar, Aeron, Onload, io_uring — URLs above, read 2026-09-18.
