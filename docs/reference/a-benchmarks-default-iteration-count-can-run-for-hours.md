# A benchmark harness's default iteration count can run for hours on the wrong case

`[measured 2026-09-24]` phase 4 row 5, `crates/engine/benches/density.rs`'s `busy_loop` module,
named in the `io_uring` transport plan's row 5 step 5 commit.

`codec/benches/harness.rs`'s `Suite::bench` runs every closure it is handed **1.41 million
times**, a count sized for the cases it was built around — a parse, a serialise, a dispatch
hop — each costing nanoseconds. The `io_uring` plan's busy-loop cases are a different shape:
one "round" drives N sessions through a real order and its execution report over real sockets,
tens of microseconds *per session*, and at N = 64 a round is well past a millisecond. Handed to
`Suite::bench` unchanged, that case does not fail — it runs for **over an hour**,
indistinguishable from a hang unless someone is watching the clock. `[measured 2026-09-24]` the
first run of this section was stopped in N = 16's case for exactly that.

This is the sibling of any fixed per-iteration default that assumes every case is small: the
count is right for the cases the harness was designed around and silently, catastrophically
wrong — by three or four orders of magnitude — for a case built afterwards that shares the same
call signature but not the same cost.

## The fix

`busy_loop::time` (`crates/engine/benches/density.rs`) does not call `Suite::bench` for these
cases at all. It runs its own best-of-7 loop, sized by `rounds = (20_000 /
clients.len()).max(50)` — fewer rounds at higher N, more at low N — and hands the single best
per-round time straight to `Suite::figure`, the same seam `benches/wakeup.rs` already uses for
its own differently-shaped, cross-thread sampling (ADR-0096).

## Guard

There is no automated reversal: the failure mode is a benchmark run taking hours rather than
producing a wrong number, and nothing here asserts against that short of a wall-clock ceiling
no CI job currently has. The guard is structural — `busy_loop::time`'s own loop, not
`Suite::bench`, is what every busy-loop case calls — and its module comment records the count
and the measured stall, so the next person sizing a new bench case whose per-iteration cost is
not nanoseconds does not reach for `Suite::bench` by habit.
