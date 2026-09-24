# A synchronous scrape waits for the exporter's next tick

`[2026-09-24]` found while building `tools/w2w --metrics` and `scripts/scrape-loop.sh`, phase 4
row 1 step 5 of
[the metrics exporter plan](../plans/2026-09-24-p4-metrics-exporter.md)
([ADR-0170](../decisions/ADR-0170-the-metrics-exporter-holds-an-observer-and-nothing-else-allocates-nothing-per-scrape-and-publishes-only-after-its-kill-line.md)).

**A client that connects, waits for the reply, then repeats is capped near the exporter's own
wake interval — not the rate the client asks for.** The first version of `scrape-loop.sh` did
exactly that (connect, wait for the answer, sleep, repeat) and, asked for 10 Hz against the
exporter's default 100 ms `tick`, reached **4.2 Hz**.

## The mechanism

[`crates/metrics/src/thread.rs`](../../crates/metrics/src/thread.rs)'s `Loop::run` is one
sleeping thread: it wakes every `tick`, drains events, calls `accept_pending()` once to take
every connection **already waiting** on the non-blocking listener, answers each in turn on
that same thread, and sleeps again until the next `tick`. Two things follow:

1. **A connection made just after a wake is not accepted until the next one.** In the worst
   case that is a full `tick` (default 100 ms) spent doing nothing but waiting to be noticed,
   before the request is even read.
2. **The exporter answers one connection at a time, on one thread.** If a scrape is the one
   that triggers `refresh()`'s ask-and-wait (at most once per `min_request_interval`), it can
   additionally block that thread for up to `fresh_wait` (default 50 ms) before writing a
   reply — and every other connection accepted in the same wake queues up behind it
   (ADR-0170 *Consequences*: "one scraper at a time").

Real Prometheus is unaffected: it scrapes on its own fixed schedule and does not wait for one
answer before starting the next tick's request. The trap is specific to a **closed-loop**
client — connect, wait, repeat — which is exactly the shape a naive test harness or a `curl`
loop takes by default.

## What was seen

Quoted from `scripts/scrape-loop.sh`'s own header, written the day this was found:

> `[measured 2026-09-24]` the first version waited for each answer and then slept; against the
> exporter's default 100 ms `tick` — a scrape waits up to one tick to be accepted — it reached
> 4.2 Hz when asked for 10.

One machine, one script, the default `tick`; not a general figure for every rate or every
`tick` setting.

## The fix, and what guards it

`scripts/scrape-loop.sh` now **starts** one attempt every `1/hz` seconds and does not wait for
it before starting the next — pacing is open-loop, matching what a real Prometheus does, so
the script's own throughput does not depend on how fast any one scrape is answered. That is
the operational answer this repository ships.

**No automated test asserts the ceiling itself.** There is no regression test that would go
red if a future change made the effective cap worse than today's — the only place the number
4.2 Hz appears is the comment above and this page. `scripts/scrape-loop.sh` prints the rate it
actually achieved (`sent at … Hz asked …`) so a human reading its output notices a large gap
between the two, but nothing fails the build on one. A change to `Loop::run`'s wake order, or
to a client that reverts to a closed loop, would not be caught by CI.

## Sources

- [`crates/metrics/src/thread.rs`](../../crates/metrics/src/thread.rs) — `Loop::run`,
  `accept_pending`, `refresh`.
- [`scripts/scrape-loop.sh`](../../scripts/scrape-loop.sh) — the pacing fix and its own
  measurement comment.
- [ADR-0170](../decisions/ADR-0170-the-metrics-exporter-holds-an-observer-and-nothing-else-allocates-nothing-per-scrape-and-publishes-only-after-its-kill-line.md)
  decisions 2, 3, 5, and *Consequences*, "One scraper at a time."
