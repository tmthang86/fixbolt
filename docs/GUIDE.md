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

## 1. The one thing that decides your latency

**Sessions per polling thread.** Nothing else on this page comes close.

An idle turn of the engine is one non-blocking `read` per connection. `[measured 2026-08-30]`
that syscall costs **703 ns**, flat from 1 to 256 sockets, and **353.8 ns of it is kernel entry
and exit doing nothing at all**. So the sweep is `N × 703 ns`, and a message that arrives just
after its socket was polled waits a whole sweep before anyone looks at it.

| Sessions on one thread | Added latency, worst case | Against the engine's own parse at 125.5 ns |
|---|---|---|
| 1 | 0.70 µs | 5.6× the parse, just to find the message |
| 2 | 1.41 µs | exceeds `DESIGN.md` §8's entire user-space budget |
| 16 | 11.2 µs | comparable to the whole kernel-TCP floor |
| 128 | 90 µs | |

**If you care about latency, run one session per thread and pin that thread to an isolated
core.** If you are building a gateway for many clients, you are in the `density` shape — that
is supported and reasonable, and you should plan against `N × 703 ns` rather than against this
project's headline figures ([ADR-0012](decisions/ADR-0012-latency-first-and-one-session-per-polling-thread.md)).

**Nothing enforces this.** The engine will happily carry 500 sessions on one thread and will
not warn you.

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

Under `RingDispatch`, if your thread stops draining, the ring fills. `[measured 2026-08-30]`
at the current 64 KiB default that takes **352 messages**, which at full rate is **tens of
microseconds** — not the milliseconds the design originally assumed.

A message the ring refuses is one the session has already **accepted, numbered, journalled and
acknowledged by sequence number**, and that your application never saw. For order flow that is
not backpressure, it is silent loss —
[ADR-0011](decisions/ADR-0011-a-full-ring-disconnects.md) is the decision about what to do
instead, and it is still `Proposed`.

**What you must do:**

- **Drain continuously.** Whatever your consumer thread does per message, it must be faster
  than the wire, with margin. Tens of microseconds of slack is not a lot.
- **Watch `refused()`.** If it is ever non-zero, you have already lost messages. It must be
  wired to something that a human sees — a metric, an alert, a log on the *cold* path.
- **Size the ring for your stall, not for your throughput.** The question is not "how many
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
one; it is not a default to reach for without measuring what it costs you.

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
   §8's budget came to exclude the 703 ns that dominates it.

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
  decision to change that and is still `Proposed`.
- **It has no session schedule.** Start time, end time and weekday resets are a known gap
  (`PRD.md`), so nothing ends a session on a clock.
- **It does not pin its own threads.** `DESIGN.md` D8 says the engine thread is pinned to an
  isolated core; `[2026-08-30]` nothing in the code does that — `STATUS.md` open item 21.
  **Pin it yourself**, with `taskset` or `sched_setaffinity`, or D8's premise does not hold.
- **It is not TLS-complete.** ADR-0005 is accepted on reasoning; the kTLS question is only now
  answerable.
