# Push-then-retire can race a writer's `finish` unless `finish` on a still-running ticket is recorded

> `[measured 2026-09-24]` — found building step 1 and its fix round of
> [plans/2026-09-24-p4-sqlite-store.md](../plans/2026-09-24-p4-sqlite-store.md),
> [ADR-0181](../decisions/ADR-0181-a-journal-outside-the-engine-joins-the-engines-writer-bookkeeping-through-three-public-handles.md)
> *Revision 1* and *Revision 2*. Full account and the state table are in the ADR; this page is
> the short form and the test names.

## What happened

`WriterTicket` is the public handle a journal's writer joins the engine's shutdown bookkeeping
through (`wait_for_retired_writers`). The first draft required the engine thread to call
`retire(stop_pushed)` **before** its one-byte `STOP` record became visible to the writer, so
`retire` had to know in advance whether the push would succeed — something only `FileJournal`
could answer, through a `pub(crate)` `Producer::fits(len)` no journal outside the crate can call.

Pushed the safe way round instead — push `STOP`, *then* `retire(pushed)` — a new race opened: the
writer can pop `STOP` and call `finish()` on a ticket that is still *running* (because `retire`
has not run yet) before the engine thread's `retire` executes. As first written, `finish()` on a
*running* ticket did nothing, so the ticket was still *running* when `retire` finally ran; `retire`
then raised the wait-for count that **nothing would ever lower** — `wait_for_retired_writers` ran
to its timeout on every shutdown that hit the race (*Revision 1*).

The fix for that (*Revision 1*: `finish` from *running* moves straight to *finished*, so a later
`retire` returns `false` and touches no count) opened a **second** race: if the writer's `finish`
won before the engine's `retire`, `retire`'s `false` return, as *Revision 1* had it, also meant
*"counted nothing at all"* — so `writers_retired()`, whose rustdoc promises it counts *"every
writer `Journal::retire` has let go, finished or not"*, did not rise. Three readers of that
counter lost to this race: the `retire` case's liveness check in
`crates/engine/benches/alloc.rs`, the rustdoc itself, and this plan's step-4 case
`sqlite-retire` (*Revision 2*).

## The rule now

Two counters, not one (ADR-0181's table, *Revision 2*): `RETIRED_WRITERS` (what
`wait_for_retired_writers` waits for) and `WRITERS_RETIRED` (`writers_retired()`) answer
different questions. A fifth, internal ticket state — *finished, never retired* — is where
`finish()` from *running* goes; the **first** `retire` that later meets that state moves it to
*finished*, raises `WRITERS_RETIRED` by exactly one, leaves `RETIRED_WRITERS` untouched, and
returns `false`. Whatever order the engine's push, its `retire` and the writer's `finish` land
in, `RETIRED_WRITERS` is raised only by a retire that found the writer still running and lowered
only by that writer's `finish` — never lowered unraised, never twice — and the first `retire` of
a ticket always raises `writers_retired()` by one. **`retire`'s return value says only "a writer
is left for `wait_for_retired_writers` to wait for"; nothing may read it as evidence a retire
happened at all.**

## Why the race is hard to see without forcing it

Unaided, the window between the engine thread's push and its `retire` call is tens of
nanoseconds against at least two syscalls the writer must make first (waking, reading the
stopped ring, closing its file); the ADR calls it *"a rare flake on a loaded two-vCPU runner"*.
The reproduction that found it injected a 5 ms delay between push and retire to make the writer
win reliably.

## The tests that guard it

| Guard | What it proves | Reversal |
|---|---|---|
| `crates/engine/tests/writer_hooks.rs::a_ticket_retired_twice_is_counted_once` | a second `retire` never raises the wait-for count again | R1: `finish` lowers the count without a CAS → red here |
| `crates/engine/tests/writer_hooks.rs::finish_without_retire_changes_no_count` | `finish` before any `retire` touches no count | as above |
| `crates/engine/tests/writer_hooks.rs::a_retire_after_the_writer_finished_counts_nothing` | *Revision 1*'s fix: `retire` after `finish` returns `false` and raises `RETIRED_WRITERS` by nothing | R2: `retire` does not raise the count → red at `wait_for_retired_writers_waits_for_a_ticket_finished_on_another_thread` |
| `crates/engine/tests/writer_hooks.rs::the_first_retire_is_counted_even_after_the_writer_finished` | *Revision 2*'s fix: that same `retire` still raises `writers_retired()` by exactly one | drop the *finished, never retired* state (finish from *running* goes straight to *finished*) → red on this test's `writers_retired()` assertion |
| `crates/engine/benches/alloc.rs`, case `retire`, run with a 5 ms delay injected between push and retire | the liveness check (`writers_retired()` == before + 1) holds under the race, not only without it | shown red on `8d6b21c` before the fix; green after, with and without the delay |
| The plan's step-4 case `sqlite-retire` | proves the store's own path by `writers_retired()` rising by one — **never** by `WriterTicket::retire`'s return value | using the return value instead would read `false` under this exact race and report the path as not taken |

**The rule for a journal** — push `STOP` once, then `retire(pushed)`, then detach; the writer
commits, closes, releases, then finishes, in that order — is stated once, in ADR-0181 decision 1,
and repeated nowhere else that could drift from it; `GUIDE.md` §6d points here instead of
restating it.
