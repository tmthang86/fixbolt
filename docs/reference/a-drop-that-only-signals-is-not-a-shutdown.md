# A drop that only signals is not a shutdown

> `[measured 2026-09-24]` — main's CI run
> [`35918095562`](https://github.com/tmthang86/fixbolt/actions/runs/35918095562) (commit
> `094bfc3`, a docs-only merge) went red in `fmt · clippy · test` on code that was green on run
> [`35916076957`](https://github.com/tmthang86/fixbolt/actions/runs/35916076957). Fixed on branch
> `fix/after-serving-race`. `STATUS.md` open item 105.

## What happened

```
test through_the_shard_runtime::shard_serve_returns_after_its_writers_finished ... FAILED
assertion `left == right` failed: all 2000 records must be on disk once the retired writer is accounted for; found 1990
```

`crates/engine/tests/after_serving.rs` dropped a `shard::Shards` and then called
`journal::wait_for_retired_writers(5 s)`, expecting the wait to cover the shard's journal writer.
It did not. `Shards` had no `Drop` of its own: dropping it dropped the channel senders and
**detached** the shard threads. The shard thread notices the disconnect on its next pass, drops
its engine — which is when the connection's journal is *retired* and counted
([ADR-0153](../decisions/ADR-0153-a-connections-journal-is-retired-without-waiting-and-its-writer-is-awaited-only-after-serving.md)
decision 3) — and only then waits. The test's wait started first, read the retired-writer count
as **zero** because nothing had been retired yet, and returned `true` at once. The records were
counted while the writer was still writing.

`wait_for_retired_writers` answers "is any **retired** writer still running". Zero before the
retire is a true answer to that question and says nothing about the writer about to be retired.

A user hits it the same way: drop `Shards`, return from `main`, and the process exits while the
detached shard thread is still inside its teardown — the loss `Async` accepts on a crash, taken on
a clean stop.

## Why it was not seen

Green on the same code in CI run `35916076957`, and 98 runs of 100 on the owner's desktop
(`[measured 2026-09-24]`, `tmt-B450-I-AORUS-PRO-WIFI`, §9 settings **not** in force, the test run
alone, `--features affinity`, debug build). The window is the
shard thread's time from the disconnect to its engine's drop; the writer is usually quick enough
to have flushed every record by the time the test counts them anyway. `STATUS.md` item 105
already said the shard test's wait was unobservable — the same fact, read as a weak test rather
than as a wrong one.

A second, smaller trap sat beside it: the count is **process-wide**, so a
`wait_for_retired_writers(Duration::ZERO)` check in one test reads the writers of every other
test running in the same binary. `[measured 2026-09-24]` 1 whole-binary run in 150 went red on the
`serve_with_recovery` door's own check that way, after the shard test began asking `ZERO` too.

## What holds it now

1. **`impl Drop for Shards` disconnects, then joins every shard thread.** Each thread has dropped
   its engine and run `after_serving(None)` before it ends, so the drop returns only once the
   writers are done or the shard's wait timed out. The order matters: joining with a sender still
   alive would never return.
2. `shard_serve_returns_after_its_writers_finished` asserts, straight after the drop and with no
   wait of its own: `writers_retired()` rose (the retire happened inside the drop),
   `wait_for_retired_writers(Duration::ZERO)` is `true`, and all 2 000 records are on disk.
3. The three tests in `after_serving.rs` take one lock (`one_at_a_time`), so no test reads
   another's writer.

**Reversal.** `Drop` emptied, nothing widening the window: 10 runs of 10 red on the
`Duration::ZERO` check, *"drop(shards) returned while a retired writer was still writing — the
shard thread's after_serving did not wait"* — the shard usually retires the journal before the
test reads `writers_retired()`, but its writer has not finished. With a 50 ms sleep injected
before the shard thread's teardown, 20 of 20 red on the first check instead, *"drop(shards)
returned before the shard thread retired the connection's journal — Shards' Drop did not join
the shard"*; the fix in, 200 of 200 green. Without the sleep, the fix in: 200 of 200 green
alone, 200 of 200 green as the whole binary. Emptying `after_serving` with the fix in turns the shard test red 10 runs of 20 — not
every run, because the one writer often finishes inside the join anyway; item 105 stays open for
that.
