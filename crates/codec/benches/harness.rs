//! A timing harness small enough to have no dependencies.
//!
//! Criterion is what `DESIGN.md` §6 names, and it gives outlier detection and
//! confidence intervals this does not. It is deferred for one reason: **these
//! benchmarks must assert**, and Criterion measures without asserting. A target
//! that lives in a comment is a wish — the reference project missed its own
//! commented target by 7x and nothing noticed.
//!
//! # What is asserted, since ADR-0016
//!
//! **A per-machine baseline, not an absolute figure.** Each case is compared
//! against the number this project measured for that case *on the CPU model it
//! is running on*, times that case's own margin. Both live in
//! `benches/baselines.tsv`, one line per (CPU model, case), each carrying the
//! sample size, the date, and the `scripts/check-machine.sh` verdict of the run
//! that recorded it.
//!
//! **The margin is per case rather than one constant**, because
//! `[measured 2026-08-31]` a single one cannot work: nine of the twelve cases
//! hold inside 7.6% of their own median over 21 runs, while `inline deliver +
//! reply` lands in one of three discrete clusters spanning 32%. One margin wide
//! enough for that case is blind for the other nine.
//!
//! The absolute ceilings this file used to assert were tuned to an Apple M5, and
//! `STATUS.md` open item 20 measured what that costs: `ring, one way` reads
//! 260.9 ns on a Ryzen 7 3700X, 270.7-272.9 on an EPYC 9V74 and 327.2-331.1 on
//! an EPYC 7763 — a 21% spread between two machines of the same vendor, against
//! ~1% within either. A single number across that pool sat *below the fastest of
//! the three*, which is a ceiling no machine passes and therefore a ceiling that
//! has stopped saying anything.
//!
//! # The three outcomes, and why the third is the dangerous one
//!
//! | Outcome | Printed | `finish` |
//! |---|---|---|
//! | baseline found, inside the band | the figure and both limits | green |
//! | baseline found, over `baseline * margin` | `OVER BASELINE` | **red** |
//! | baseline found, under `baseline / margin` | `UNDER BASELINE` | **not red**, counted |
//! | **no baseline for this CPU** | `NO BASELINE`, plus the line to paste | **not red** |
//!
//! **The floor is not decoration.** `[measured 2026-09-01]` `inline deliver +
//! reply` published 1.3 ns for a day while doing 8.5 ns of work, because the
//! optimiser deleted a copy nobody read — and a ceiling passes that forever,
//! more comfortably every day. `UNDER BASELINE` is reported rather than failed
//! because a real optimisation lands there too, and both causes need the same
//! thing from a person: re-record, or fix the benchmark. `scripts/bench.sh
//! --strict` is where it becomes fatal. `STATUS.md` open item 25, and see
//! `verdict.rs` for the whole argument.
//!
//! The third outcome is how every case on an unknown machine could go quietly
//! green, so it is guarded three ways: it prints as its own state rather than as
//! a pass, [`Suite::finish`] counts it separately on a line `scripts/bench.sh`
//! greps, and `bench.sh --strict` — what a `DESIGN.md` §9 machine runs — treats
//! a single one as fatal. Proven by reversal: making [`cpu_model`] return a
//! string no baseline names turns **all twelve** cases into `NO BASELINE` rather
//! than into passes.
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
//! baselines — forgetting is a compile error rather than a silent pass.

use std::hint::black_box;
use std::time::Instant;

// The comparison rule, on its own so that `crates/codec/tests/bench_verdict.rs`
// can test it: a `harness = false` bench target is a `main()` that `cargo test`
// never runs, so this was the one piece of logic in the repository deciding
// every timing gate with nothing testing it. One source, two consumers.
include!("verdict.rs");

/// The recorded baselines, compiled in rather than read at runtime: a missing
/// file is then a build failure and not a silently unchecked run.
const BASELINES: &str = include_str!("../../../benches/baselines.tsv");

/// One machine's recorded figure for one case.
#[derive(Clone, Copy)]
struct Baseline {
    /// ns/op, the median over whole suite runs.
    ns: f64,
    /// What this case may drift on this machine before it is a regression.
    ///
    /// **Per case, not one constant, and the reason is measured.**
    /// `[measured 2026-08-31]` 21 runs on the §9 desktop, quiet: nine of the
    /// twelve cases hold inside 7.6% of their own median, but `ring, one way`
    /// draws a second mode at +24% on 2 runs in 21 and `inline deliver + reply`
    /// lands in one of *three* discrete clusters — 6.3, 7.4, 8.2 — with nothing
    /// in between. A single margin wide enough for those is 1.35, which would
    /// let `encode ExecutionReport` drift 241.6 -> 326 ns unnoticed against a
    /// real spread of 7.6%. So the margin travels with the case.
    ///
    /// The modes are drawn **per process**, so best-of-7 within one process
    /// cannot suppress them: all seven samples sit inside whichever mode that
    /// process got. See `docs/reference/measured-costs.md`.
    margin: f64,
}

/// The CPU model, in the exact spelling `scripts/check-machine.sh` prints, so
/// that the machine block travelling with every figure carries the lookup key.
///
/// `None` on a platform neither branch knows, which lands every case in the
/// `NO BASELINE` outcome rather than in a pass.
fn cpu_model() -> Option<String> {
    if cfg!(target_os = "linux") {
        let info = std::fs::read_to_string("/proc/cpuinfo").ok()?;
        info.lines()
            .find(|l| l.starts_with("model name"))
            .and_then(|l| l.split_once(':'))
            .map(|(_, v)| v.trim().to_string())
    } else if cfg!(target_os = "macos") {
        let out = std::process::Command::new("sysctl")
            .args(["-n", "machdep.cpu.brand_string"])
            .output()
            .ok()?;
        let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if s.is_empty() { None } else { Some(s) }
    } else {
        None
    }
}

/// The baseline recorded for `case` on `cpu`, if `benches/baselines.tsv` has a
/// line for that pair.
///
/// A line whose numbers do not parse yields `None` and therefore the
/// `NO BASELINE` outcome, which is loud. Skipping it silently would turn a typo
/// in the data file into an unchecked case.
fn baseline_for(cpu: &str, case: &str) -> Option<Baseline> {
    BASELINES
        .lines()
        .map(str::trim_end)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter_map(|l| {
            let mut f = l.split('\t');
            Some((f.next()?, f.next()?, f.next()?, f.next()?))
        })
        .find(|(c, n, _, _)| *c == cpu && *n == case)
        .and_then(|(_, _, ns, margin)| {
            Some(Baseline {
                ns: ns.trim().parse().ok()?,
                margin: margin.trim().parse().ok()?,
            })
        })
}

/// Run a set of timed cases, then check every baseline once they have all been
/// measured and printed.
pub fn suite<F: FnOnce(&mut Suite)>(f: F) {
    let cpu = cpu_model();
    match &cpu {
        Some(c) => println!("machine   {c}"),
        None => println!("machine   UNKNOWN — no baseline can be looked up"),
    }
    let mut suite = Suite {
        cpu,
        over: Vec::new(),
        under: Vec::new(),
        missing: Vec::new(),
        cases: 0,
    };
    f(&mut suite);
    suite.finish();
}

/// Collects the cases of one bench target. Obtained only from [`suite`].
pub struct Suite {
    cpu: Option<String>,
    over: Vec<String>,
    /// Cases below `baseline / margin`. **Not merged into `over`**: `finish`
    /// asserts on that one, and a real optimisation lands here too — see
    /// `verdict.rs`. Counted, printed, and made fatal by `bench.sh --strict`.
    under: Vec<String>,
    missing: Vec<String>,
    cases: usize,
}

impl Suite {
    /// Time `f`, print its figure, and compare it against this machine's
    /// recorded baseline for `name`.
    ///
    /// Recording rather than asserting is deliberate — see the module docs.
    pub fn bench<F: FnMut()>(&mut self, name: &str, mut f: F) {
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
        self.cases += 1;

        let baseline = self.cpu.as_deref().and_then(|cpu| baseline_for(cpu, name));

        match baseline {
            Some(b) => {
                let ceiling = b.ns * b.margin;
                let floor = b.ns / b.margin;
                let v = verdict(best, b.ns, b.margin);
                let mark = match v {
                    Verdict::InBand => "",
                    Verdict::Over => "  OVER BASELINE",
                    Verdict::Under => "  UNDER BASELINE",
                };
                println!(
                    "{name:<34} {best:>8.1} ns/op   baseline {:.1} x{:.2} = \
                     [{floor:.1}, {ceiling:.1}]{mark}",
                    b.ns, b.margin
                );
                match v {
                    Verdict::InBand => {}
                    Verdict::Over => self.over.push(format!(
                        "{name}: {best:.1} ns/op exceeds {ceiling:.1} ns \
                         (baseline {:.1} x {:.2})",
                        b.ns, b.margin
                    )),
                    // Not pushed into `over`: `finish` asserts on that, and a
                    // genuine speed-up must not break the build before it can
                    // be merged. Counted, printed, and fatal only under
                    // `bench.sh --strict`.
                    Verdict::Under => self.under.push(format!(
                        "{name}: {best:.1} ns/op is below {floor:.1} ns \
                         (baseline {:.1} / {:.2}) — re-record the baseline, or \
                         the benchmark stopped measuring",
                        b.ns, b.margin
                    )),
                }
            }
            None => {
                // Printed as its own state, never as a pass. The paste-ready
                // line is here so that recording a baseline is one copy rather
                // than a hunt through the file format.
                let cpu = self.cpu.as_deref().unwrap_or("UNKNOWN");
                println!("{name:<34} {best:>8.1} ns/op   NO BASELINE for '{cpu}'");
                println!(
                    "    to record, after check-machine.sh reads fail 0, \
                     append to benches/baselines.tsv (median of N>=20 runs, \
                     margin from the ladder in that file's header):"
                );
                println!("    {cpu}\t{name}\t{best:.1}\t<margin>\t<n>\t<date>\t<verdict>");
                self.missing.push(name.to_string());
            }
        }
        black_box(best);
    }

    fn finish(self) {
        // A suite that measured nothing must not report success. This is the
        // same shape as a zero-allocation figure for a path that never ran.
        assert!(self.cases > 0, "this bench target ran no cases at all");

        // Counted on its own line because `scripts/bench.sh` greps for it and
        // because "not red" must never be allowed to read as "green".
        if !self.missing.is_empty() {
            println!(
                "cases without a baseline: {}  {}",
                self.missing.len(),
                self.missing.join(", ")
            );
        }

        // Its own grep-able line, for the same reason as the one above: "not
        // red" must never be allowed to read as "green". `scripts/bench.sh`
        // reads this and `--strict` makes it fatal.
        if !self.under.is_empty() {
            println!(
                "cases under their baseline: {}  {}",
                self.under.len(),
                self.under.join(" | ")
            );
        }

        assert!(
            self.over.is_empty(),
            "{} of {} case(s) over the machine baseline:\n{}",
            self.over.len(),
            self.cases,
            self.over.join("\n")
        );
    }
}
