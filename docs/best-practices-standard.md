# Best Practices for `standard` Mode

`standard` is the **default** mode. `serve` blocks the engine thread when there is nothing to
do, so the engine can run on a shared host, in a container, or on a laptop. This page is
operational advice for that mode. For `hft`, see [best-practices-hft.md](best-practices-hft.md).
The two pages are separate because advice that does not name its mode is incomplete
([ADR-0013](decisions/ADR-0013-two-modes-standard-and-hft.md), non-negotiable 4).

> **Most `standard`-specific numbers are not measured yet.** The only `standard` latency
> figure is the loopback round trip in [DESIGN.md §8](DESIGN.md); the per-message and per-turn
> figures published elsewhere are `hft` numbers on a tuned box. Where this page gives a
> number it names its source. Where it does not, the advice is reasoned, not measured, and you
> should measure on your own machine before relying on it.

---

## 1. The idle timeout, and why blocking is right here

`standard` blocks in `poll(2)` with a timeout ([ADR-0014](decisions/ADR-0014-standard-mode-blocks-on-poll.md)).
The default is **100 ms** (`block::DEFAULT_TIMEOUT_MS`). A timeout below **5 ms**
(`block::MIN_TIMEOUT_MS`) is **raised to 5 ms, not rejected**, so the engine cannot be made
to spin by accident.

- A shorter timeout wakes the engine sooner to send heartbeats and check schedules, at the
  cost of more idle wakeups.
- `HeartBtInt` is whole seconds, so 100 ms never delays a heartbeat.
- **Do not push the timeout toward zero to chase latency.** A `standard` engine that spins is
  as much a defect as an `hft` engine that sleeps. If you need the engine not to sleep, use
  `hft`; do not approximate it here.

The engine is woken by data, not by the timeout: `[measured 2026-08-30]` on a shared
container the `standard` round-trip p50 was 29 µs, three orders of magnitude below the 100 ms
timeout.

---

## 2. Many sessions per thread is normal here

In `hft` one polling thread carries one session
([ADR-0012](decisions/ADR-0012-latency-first-and-one-session-per-polling-thread.md)). In
`standard` the thread blocks rather than sweeping, so many sessions on one thread is the
expected shape (called `density`), and it is supported rather than tolerated. Size the
connection capacity for the number of sessions you expect.

---

## 3. Journal durability: pick the policy your recovery needs

| Policy | What it survives | What it costs |
|---|---|---|
| no journal (`NoJournal`) | nothing; a restart cannot resume | fastest |
| `Durability::Async` | a process crash, not a power loss | one `write` per message, flushed by a background thread |
| `Durability::Fsync` | a power loss | a disk sync per message, on the engine thread, in **both** directions since [ADR-0017](decisions/ADR-0017-the-inbound-count-is-persisted-after-delivery.md) |

`Fsync` puts a disk on the message path. In `standard` that is usually acceptable; choose it
when a counterparty will replay against your sequence numbers after a crash.
[GUIDE.md §6](GUIDE.md) explains how recovery reads the journal back. None of the three costs
has been benchmarked here; measure `Fsync` against your storage before committing to it.

---

## 4. What the handler should and should not do

Your `Handler` runs on the engine thread. In `standard` a brief stall does not starve other
work the way it would in `hft`, but the rule still holds:

- **Do** keep per-message work bounded and return promptly.
- **Do not** do unbounded I/O inline: a database write, a remote call. If the handler stalls,
  every session on that thread stalls with it.
- **If the work cannot be bounded**, use `RingDispatch`. The application moves to its own
  thread and the engine hands messages across a ring ([ADR-0002](decisions/ADR-0002-engine-library-split.md)).
  A ring that fills **disconnects the session** rather than blocking the engine
  ([ADR-0011](decisions/ADR-0011-a-full-ring-disconnects.md)); [GUIDE.md §4](GUIDE.md)
  says what that means for you.

---

## 5. Shared hosts and containers

`standard` exists so the engine can run beside other tenants. Nothing in this mode pins a
core or asks for kernel tuning; that is `hft`'s concern and the [HFT playbook](hft-playbook.md)'s.
A container with a CPU limit is a normal `standard` deployment. Size the idle timeout so the
engine is not the reason the host stays busy.

---

## 6. What is not measured in this mode

- **No `standard` stage-by-stage latency figure is published.** The round trip is measured
  (`[measured 2026-09-02]` p50 19 447 ns administrative, 20 920 ns through an application,
  loopback, tuned Ryzen 7 3700X), but the wakeup is one opaque term and nothing decomposes it.
- **The `~449 ns × N` per-turn cost is an `hft` figure** and does not transfer to a mode that
  blocks.
- **The wakeup cost has never been measured on a tuned machine.** [DESIGN.md §8](DESIGN.md)
  carries it as 2–5 µs *from the literature*, and the measured `hft`-versus-`standard`
  difference of 3 437 ns is consistent with that.
- **Durability costs are not benchmarked.** The table in §3 is directional.

---

## 7. The resend ring in `standard` mode

`[2026-09-04]` ([ADR-0046](decisions/ADR-0046-the-ring-is-the-resend-store-and-a-replay-goes-in-batches.md))
The in-memory ring is the whole resend store, and it is now the largest per-session
allocation: `SLOTS × (SLOT_LEN + 8)` ≈ **2 MiB** at the defaults.
`[measured 2026-09-04, Apple M5, macOS 15]` `tools/w2w --mode standard` reads
**+2 195 456 bytes** of maximum resident set against the old `SLOTS = 8`.

- **Two hundred sessions is 400 MiB of ring.** If that is your shape, set a smaller `N`
  through the const generic. [CONFIGURATION.md](CONFIGURATION.md) has the formula; it is
  about how long a disconnection you are willing to replay across, not about memory.
- **`Engine::add` builds the journal on the engine thread.** In this mode that thread is
  about to block anyway, and accepting a connection is not on the message path, so the
  allocation is affordable here in a way it is not in `hft`.
- **Watch `resend_beyond_journal`.** Non-zero means a counterparty asked for messages the
  ring no longer held and got gap fills. `SessionSnapshot` carries the running total and
  `EventKind::ResendBeyondJournal` carries each change with how far back it reached.

---

## 8. The message log

`[2026-09-04]` **Turn it on.** `standard` already blocks when idle, so a writer thread sharing
a core with the engine costs nothing anyone will notice, and the file is the first thing
asked for when a counterparty disputes a fill.

```ini
FileLogPath=/var/log/fixbolt/messages.log
```

`[DEFAULT]` only: one engine, one file, with `conn=` and `shard=` inside it. Rotate with
`logrotate` and `copytruncate`; the engine never rotates. Watch `Snapshot::log_lost` and
`EventKind::MessageLogLost`: non-zero means the disk fell behind the engine and the file has
holes. [GUIDE.md §6c](GUIDE.md) lists the seven things about the log the type system cannot
tell you.

---

## 9. Stopping it, in `standard` mode

`[2026-09-05]` **Hold a `Handles` and stop through it.** `standard` is the mode that runs on
shared hosts, in containers and under supervisors, so this is the mode where "stopped" most
often means *"the orchestrator sent a signal"* — and a process that dies to a signal without
saying `35=5` looks, to the counterparty, exactly like a dead line. They reconnect, and any
bytes still queued are lost having already spent their sequence numbers.

```rust
let handles = fixbolt::Handles::new();
let admin = handles.admin();
std::thread::spawn(move || {
    wait_for_your_shutdown_signal();
    admin.shutdown(30_000);      // grace, on the engine's clock
});
let done = fixbolt::serve(addr, table, app, 64, limits, log, handles)?;
if !done.clean() {
    eprintln!("{} of {} never answered", done.timed_out(), done.sessions());
}
```

**Two things that are specific to this mode.** The grace period is spent **blocking on
readiness**, so an engine waiting for goodbyes gives the core back exactly as it does when
idle — the wait is not a spin. And a `standard` engine is usually holding many sessions
(§2), so `Shutdown::timed_out()` is the number to read: one counterparty that never answered
is one whose sequence numbers may need reconciling by hand before the next start.

**Set your supervisor's kill deadline above the grace you pass.** `SIGKILL` arriving during
the grace is the shape this whole mechanism exists to avoid, and systemd's default
`TimeoutStopSec` is 90 s.
[ADR-0038](decisions/ADR-0038-an-ordered-shutdown-is-a-state-not-a-flag.md),
[ADR-0054](decisions/ADR-0054-the-handles-are-made-before-the-engine-and-the-engine-adopts-them.md).
