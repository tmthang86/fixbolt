# Best Practices — `standard` Mode

`standard` is the **default** mode: `serve` blocks the engine thread when there is
nothing to do, so the engine is usable on a shared host. This page is operational
advice for that mode. For `hft`, see [best-practices-hft.md](best-practices-hft.md);
the two are separate because a claim that does not name its mode is incomplete
([ADR-0013](decisions/ADR-0013-two-modes-standard-and-hft.md), non-negotiable 4).

> **Most of what a `standard` deployment wants to know is not yet measured on this
> engine.** The wire-to-wire and per-message figures published so far are `hft` numbers
> on a tuned box; `standard` has no latency figure worth quoting. Where this page gives
> a number it names its source; where it does not, treat the advice as reasoned, not
> measured, and measure on your own box before you rely on it.

---

## 1. The idle timeout, and why blocking is correct here

`standard` blocks in `poll(2)` with a timeout ([ADR-0014](decisions/ADR-0014-standard-mode-blocks-on-poll.md)).
The default is **100 ms** (`block::DEFAULT_TIMEOUT_MS`), and a timeout below **5 ms**
(`block::MIN_TIMEOUT_MS`) is **raised to it, not rejected** — the engine will not spin
by accident.

- A shorter timeout wakes the engine sooner to send heartbeats and check schedules, at
  the cost of more wakeups when idle.
- `HeartBtInt` is a whole number of seconds, so 100 ms is well inside it: the default
  never delays a heartbeat.
- **Do not drive the timeout toward zero to chase latency.** A `standard` engine that
  spins is as much a defect as an `hft` engine that sleeps (non-negotiable 4). If you
  need to not sleep, use `hft` — do not approximate it here.

---

## 2. Many sessions per thread is normal here

Unlike `hft`, where one polling thread carries one session
([ADR-0012](decisions/ADR-0012-latency-first-and-one-session-per-polling-thread.md)),
`standard` is where you put many sessions on one thread. This is the `density` shape,
and it is supported, not tolerated. Size the connection capacity for the number of
sessions you expect, not for one.

---

## 3. Durability — pick the policy your recovery needs

The journal has three policies (`journal::Durability`): `None`, `Async`, and `Fsync`.

| Policy | What it buys | What it costs |
|---|---|---|
| `None` | nothing is persisted | a restart cannot resume; fastest |
| `Async` | writes reach the OS, flushed in the background | survives a process crash, not a power loss |
| `Fsync` | every write is forced to disk before the ack | survives power loss; a syscall per message |

`Fsync` puts a syscall on the message path. In `standard` that is usually acceptable;
choose it when a counterparty will replay against your sequence numbers after a crash.
See [GUIDE.md](GUIDE.md) for how recovery reads the journal back.

---

## 4. What the handler should and should not do

Your `App`/`Handler` runs on the engine thread. In `standard` mode it may block briefly
without starving other work the way it would in `hft`, but the discipline still holds:

- **Do** keep per-message work bounded and return promptly.
- **Do not** perform unbounded I/O (a database write, a remote call) inline. If the
  handler stalls, every session on that thread stalls with it.
- **When inline work cannot be bounded**, move to a ring: `RingDispatch` decouples the
  application from the engine thread so a slow handler cannot back-pressure the wire
  ([ADR-0002](decisions/ADR-0002-engine-library-split.md), D4). The ring disconnects a
  session it cannot keep up with rather than blocking the engine
  ([ADR-0011](decisions/ADR-0011-a-full-ring-disconnects.md)) — see [GUIDE.md](GUIDE.md).

---

## 5. Shared hosts and containers

`standard` exists so the engine can run beside other tenants. Nothing here pins a core
or asks for kernel tuning — that is `hft`'s concern and the HFT playbook's. A container
with a CPU limit is a normal `standard` deployment; just size the idle timeout so the
engine is not the reason the host stays busy.

---

## 6. What is not measured in this mode

Named plainly, because a page of advice that hides its uncertainty is worse than none:

- **No `standard` latency figure is published.** The `~449 ns × N` per-turn cost and the
  16 µs round trip in [DESIGN.md](DESIGN.md) §8 are `hft` numbers on a tuned box. They
  do not transfer to `standard`, which blocks.
- **The wakeup cost of the blocking path has never been measured on a §9 machine.** It
  is carried in `DESIGN.md` §8 with a *from the literature* label for exactly this
  reason.
- **Durability policy costs are not benchmarked here.** The table above is directional.
  Measure `Fsync` against your storage before you commit to it on a latency-sensitive
  `standard` session.

---

## 7. The resend ring, in `standard` mode

`[2026-09-04]` [ADR-0046](decisions/ADR-0046-the-ring-is-the-resend-store-and-a-replay-goes-in-batches.md).

**`standard` mode is where session density lives**, and the resend ring is now the largest
per-session allocation: `SLOTS × (SLOT_LEN + 8)` ≈ **2 MiB** at the defaults.
`[measured 2026-09-04, Apple M5, macOS 15]` `tools/w2w --mode standard` reads
**+2 195 456 bytes** of maximum resident set against the old `SLOTS = 8`.

- **Two hundred sessions is 400 MiB of ring.** If that is your shape, set a smaller `N`
  through the const generic — `docs/CONFIGURATION.md` has the formula, and it is about how long
  a disconnection you are willing to replay across, not about how much memory you have.
- **`Engine::add` builds the journal on the engine thread**, which in this mode is a thread
  that is about to block anyway. Accepting a connection is not on the message path, so the
  allocation is affordable here in a way it is not in `hft` — see the other page.
- **Watch `resend_beyond_journal`.** Non-zero means a counterparty asked for messages the ring
  no longer held and got gap fills instead. `SessionSnapshot` carries the running total and
  `EventKind::ResendBeyondJournal` carries each change, with how far back the ring reached.

## The message log

`[2026-09-04]` **Turn it on.** `standard` already blocks when idle, so a writer thread that
shares a core with the engine costs nothing anybody will notice, and the file is the first thing
anyone asks for when a counterparty disputes a fill.

```
FileLogPath=/var/log/fixbolt/messages.log
```

`[DEFAULT]` only — one engine, one file, `conn=` and `shard=` inside it. Rotate with `logrotate`
and `copytruncate`; the engine never rotates anything. Watch `Snapshot::log_lost` and
`EventKind::MessageLogLost`: non-zero means the disk is behind the engine and the file has
holes. `GUIDE.md` §6c has the seven things the type system cannot tell you.
