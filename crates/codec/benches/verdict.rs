// How one measured figure is judged against one recorded baseline.
//
// Its own file, `include!`d by `harness.rs` and by
// `crates/codec/tests/bench_verdict.rs`, because a bench target built with
// `harness = false` is a `main()` that `cargo test` never runs — so the rule
// that decides every timing gate in `DESIGN.md` §6 would otherwise be the one
// piece of logic in this repository with no test at all.
//
// Pure: no clock, no I/O, no formatting. `f64` in, an enum out.

/// What a measured figure says about its recorded baseline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)] // Each consumer of this file uses a different subset.
enum Verdict {
    /// Inside the band. The only outcome that is a pass.
    InBand,
    /// Above `baseline * margin`. A regression.
    Over,
    /// Below `baseline / margin`, which is **not** a pass and **not** a
    /// failure — see [`Verdict::Under`]'s note below.
    Under,
}

/// Judge `best` against `baseline`, using `margin` in **both** directions.
///
/// # Why there is a floor at all
///
/// `[measured 2026-09-01]` `inline deliver + reply` published **1.3 ns** for a
/// day while doing **8.5 ns** of work: `out` was written every iteration and
/// read by nobody, so the optimiser deleted a 163-byte copy. A ceiling cannot
/// see that. A case that stops measuring reads far *under* its limit and passes
/// — forever, and more comfortably every day. It was found by arithmetic during
/// an unrelated experiment, not by a gate. `STATUS.md` open item 25.
///
/// # Why `Under` is reported rather than failed
///
/// A figure below the floor has exactly two causes and **both need the same
/// action from a person**:
///
/// * a real optimisation — and then the baseline must be re-recorded, or the
///   ceiling above it is wider than the truth and guards nothing;
/// * a benchmark that stopped measuring — and then it must be fixed.
///
/// Making it red would mean every genuine speed-up breaks the gate before it
/// can be merged, and the thing people would learn is to widen the margin. So
/// `Under` is counted and printed on its own line, and `scripts/bench.sh
/// --strict` — what a `DESIGN.md` §9 machine runs during a deliberate
/// measurement session — is where it becomes fatal. Exactly how `NO BASELINE`
/// is already handled.
///
/// **This is also what stops baselines going stale**, which
/// [ADR-0016](../../../docs/decisions/ADR-0016-per-machine-baselines-replace-absolute-targets.md)
/// listed in its own Consequences as an accepted cost: *"a real speed-up leaves
/// the baseline generous until somebody re-records"*. Now something asks.
///
/// # Why the floor is the same margin
///
/// `margin` is `max/median` over the `n` runs that produced the baseline — that
/// case's own measured spread. A figure below `median / margin` is outside that
/// spread in the other direction.
///
/// **It is not symmetric and this is deliberate.** `best` is a minimum over
/// rounds and the baseline is a *median* over runs, so the distribution below
/// the median is the wider one and this floor will occasionally report a figure
/// that is only noise. That is affordable **only because `Under` is a report**,
/// and the way to resolve a false one is to re-record the baseline — which is
/// the right thing to do anyway. A separately measured `low_margin` column
/// needs `n >= 20` runs on a §9 machine and is deliberately not invented here.
#[allow(dead_code)]
fn verdict(best: f64, baseline: f64, margin: f64) -> Verdict {
    if best > baseline * margin {
        Verdict::Over
    } else if best < baseline / margin {
        Verdict::Under
    } else {
        Verdict::InBand
    }
}
