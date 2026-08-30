//! A timing harness small enough to have no dependencies.
//!
//! Criterion is what `DESIGN.md` §6 names, and it gives outlier detection and
//! confidence intervals this does not. It is deferred for one reason: **these
//! benchmarks must assert**, and Criterion measures without asserting. A target
//! that lives in a comment is a wish — the reference project missed its own
//! commented target by 7x and nothing noticed.
//!
//! What is asserted is a **regression ceiling**, not the published target. The
//! measured baseline is 139 ns for a `NewOrderSingle` and the published gate is
//! 150 ns, an 8% margin; an unpinned laptop varies by more than 8%, so a hard
//! assert would go red at random and a gate that goes red at random gets
//! switched off. `DESIGN.md` §6 states both numbers and why they differ.
//!
//! **Every case is measured and printed before any of them is allowed to fail.**
//! `[measured 2026-08-30]` The earlier version asserted inside each case, so the
//! first one over its ceiling ended the process and the rest were never run: a
//! single 17.8 ns outlier on `inline deliver + reply` — baseline 3.7 ns — threw
//! away both `ring` figures on that run, and `groups` lost three of its four
//! cases the same way. A benchmark exists to produce numbers; one that hides the
//! numbers behind the first flapping case cannot do the job CI needs it for.
//!
//! The assertion lives in [`Suite::finish`], reached only through [`suite`], so
//! a bench cannot report figures without also being checked against its
//! ceilings — forgetting is a compile error rather than a silent pass. Proven by
//! reversal in `scripts/bench.sh`'s own check and by lowering a ceiling by hand:
//! see `docs/plans/2026-08-30-benches-that-run.md`.

use std::hint::black_box;
use std::time::Instant;

/// Run a set of timed cases, then check every ceiling once they have all been
/// measured and printed.
pub fn suite<F: FnOnce(&mut Suite)>(f: F) {
    let mut suite = Suite {
        over: Vec::new(),
        cases: 0,
    };
    f(&mut suite);
    suite.finish();
}

/// Collects the cases of one bench target. Obtained only from [`suite`].
pub struct Suite {
    over: Vec<String>,
    cases: usize,
}

impl Suite {
    /// Time `f`, print its figure, and record whether it exceeded `ceiling_ns`.
    ///
    /// Recording rather than asserting is deliberate — see the module docs.
    pub fn bench<F: FnMut()>(&mut self, name: &str, ceiling_ns: f64, mut f: F) {
        for _ in 0..10_000 {
            f();
        }
        let mut best = f64::INFINITY;
        for _ in 0..7 {
            let iters = 200_000u32;
            let t = Instant::now();
            for _ in 0..iters {
                f();
            }
            let ns = t.elapsed().as_nanos() as f64 / f64::from(iters);
            best = best.min(ns);
        }
        let over = best > ceiling_ns;
        let mark = if over { "  OVER CEILING" } else { "" };
        println!("{name:<34} {best:>8.1} ns/op   ceiling {ceiling_ns:.0}{mark}");
        self.cases += 1;
        if over {
            self.over.push(format!(
                "{name}: {best:.1} ns/op exceeds the {ceiling_ns:.0} ns regression ceiling"
            ));
        }
        black_box(best);
    }

    fn finish(self) {
        // A suite that measured nothing must not report success. This is the
        // same shape as a zero-allocation figure for a path that never ran.
        assert!(self.cases > 0, "this bench target ran no cases at all");
        assert!(
            self.over.is_empty(),
            "{} of {} case(s) over the regression ceiling:\n{}",
            self.over.len(),
            self.cases,
            self.over.join("\n")
        );
    }
}
