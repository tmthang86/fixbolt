//! When an initiator that lost its connection tries again.
//!
//! Step 1 of [an-initiator-that-comes-back]. Pure: no socket, no real clock,
//! no sleep. Every instant here is an argument.
//!
//! # These tests are invented, and that is worth saying
//!
//! Nothing in this repository's corpora covers reconnect. The 59 acceptance
//! definitions are written for an acceptor and never reconnect an initiator;
//! the mirrored corpus is at 2 / 50; `scripts/interop.sh` connects once. So
//! unlike `tests/score.rs`, which measures this engine against somebody else's
//! suite, this file measures it against **this project's own reading of what a
//! reconnect should do**. `crates/dict/tests/field_types.rs` carries the same
//! weakness and says so in the same words.
//!
//! What that costs is concrete: a rule everybody would agree with but nobody
//! wrote down here would pass. What it still buys is the arithmetic — a
//! ceiling that does not hold, or a backoff that resets when it should not,
//! is a real outage and is caught here.
//!
//! [an-initiator-that-comes-back]: ../../../docs/plans/2026-09-02-an-initiator-that-comes-back.md
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use fixbolt_engine::reconnect::{Next, Policy, PolicyError};
use fixbolt_session::schedule::Schedule;

const SECOND: u64 = 1_000;

fn policy() -> Policy {
    Policy::new(SECOND, 30 * SECOND).expect("a legal pair")
}

/// Before anything has failed, there is nothing to wait for.
#[test]
fn a_fresh_policy_says_connect_now() {
    assert_eq!(policy().next(0), Next::Now);
    assert_eq!(policy().next(12_345), Next::Now);
}

/// **The doubling, and the ceiling that stops it.**
///
/// Asserted as the whole sequence rather than one step at a time: a policy
/// that doubled correctly and then *ignored* its ceiling would pass every
/// single-step assertion up to the sixth.
#[test]
fn the_delay_doubles_and_then_stops_at_the_ceiling() {
    let mut p = policy();
    let mut now = 0;
    let mut waits = Vec::new();
    for _ in 0..8 {
        p.dropped(now);
        let Next::At(at) = p.next(now) else {
            panic!("a policy that has just seen a drop must make the caller wait")
        };
        waits.push(at - now);
        now = at;
    }
    assert_eq!(
        waits,
        [1_000, 2_000, 4_000, 8_000, 16_000, 30_000, 30_000, 30_000],
        "1s doubling to a 30s ceiling, and then staying there — a backoff with \
         no ceiling is an hour of silence after a long outage"
    );
}

/// The wait is over when it is over, and not before.
#[test]
fn the_wait_ends_at_its_instant_and_not_a_millisecond_early() {
    let mut p = policy();
    p.dropped(10_000);

    assert_eq!(p.next(10_999), Next::At(11_000), "999 ms in: not yet");
    assert_eq!(p.next(11_000), Next::Now, "at the instant: now");
    assert_eq!(p.next(11_001), Next::Now, "and after it: still now");
}

/// **A session that logged on resets the ladder.**
///
/// Without this a connection that flaps once an hour walks its way to the
/// ceiling over a day and then takes 30 s to recover from a blip that lasted
/// 200 ms.
#[test]
fn logging_on_puts_the_ladder_back_to_the_bottom() {
    let mut p = policy();
    for step in [0, 1_000, 3_000, 7_000] {
        p.dropped(step);
    }
    // Four drops in: the next wait is 8 s, not 1 s.
    p.dropped(20_000);
    assert_eq!(p.next(20_000), Next::At(36_000), "the premise: 16 s in");

    p.logged_on();
    p.dropped(40_000);

    assert_eq!(
        p.next(40_000),
        Next::At(41_000),
        "a session that got up resets the ladder, so the next outage starts at \
         one second again"
    );
}

/// `stop` is final, and it beats a wait that is already due.
#[test]
fn stop_is_final_and_outranks_a_wait_that_has_come_due() {
    let mut p = policy();
    assert_eq!(p.next(0), Next::Now, "the premise");

    p.stop();

    assert_eq!(p.next(0), Next::Stop);
    assert_eq!(
        p.next(u64::MAX),
        Next::Stop,
        "and no later instant undoes it"
    );
    p.dropped(1_000);
    assert_eq!(p.next(9_999), Next::Stop, "nor does another drop");
}

/// **Outside its hours an initiator does not call out.**
///
/// Dialling a venue that is shut is a refused connection, and a refused
/// connection starts a backoff ladder for the wrong reason — the outage gets
/// blamed on the network when the answer is *we are closed*.
///
/// It answers `At(now + ceiling)` rather than the instant the window opens,
/// and that is a limit rather than a choice: `Schedule` can say whether an
/// instant is inside a window and cannot compute the next opening. Asking
/// again twice a minute while a venue is shut costs nothing; inventing an
/// instant would cost correctness.
#[test]
fn a_closed_schedule_refuses_to_connect_and_asks_again_later() {
    // 08:00–17:00 UTC, and the instants below are chosen against that.
    let hours = Schedule::daily(8 * 3_600, 17 * 3_600).expect("legal hours");
    let p = policy().with_schedule(hours);

    let day = 86_400_000u64;
    let at = |h: u64, m: u64| day * 20_000 + h * 3_600_000 + m * 60_000;

    assert!(hours.contains(at(12, 0)), "the premise: midday is open");
    assert!(!hours.contains(at(3, 0)), "and 3 a.m. is not");

    assert_eq!(p.next(at(12, 0)), Next::Now, "inside its hours it dials");
    assert_eq!(
        p.next(at(3, 0)),
        Next::At(at(3, 0) + 30 * SECOND),
        "outside them it waits a ceiling and asks again"
    );
}

/// **The schedule is asked before the ladder**, so a closed venue never gets
/// the shorter of the two answers.
///
/// # The instant this asserts at is the whole test
///
/// `[measured 2026-09-02]` the first version of this asked at an instant where
/// the ladder had **already come due**, and swapping the two checks was a
/// **no-op** — with the ladder due, both orderings reach the schedule and both
/// answer `At(now + ceiling)`. The reversal passed and proved nothing.
///
/// The orderings only disagree while the ladder is **still pending**:
///
/// | at `three_am + 500 ms`, ladder due at `+1 s`, venue shut | answer |
/// |---|---|
/// | schedule first — what this engine does | `At(now + 30 s)` |
/// | ladder first | `At(+1 s)` — a dial into a shut venue, 29 s early |
///
/// Both instants are asserted, because the due one is the case a reader
/// expects and the pending one is the case that discriminates.
/// `docs/reference/a-reversal-needs-an-input-where-the-answers-differ.md`.
#[test]
fn a_closed_schedule_outranks_the_ladder_including_before_it_is_due() {
    let hours = Schedule::daily(8 * 3_600, 17 * 3_600).expect("legal hours");
    let mut p = policy().with_schedule(hours);

    let day = 86_400_000u64;
    let three_am = day * 20_000 + 3 * 3_600_000;

    // One drop: the ladder says "wait 1 s".
    p.dropped(three_am);

    assert_eq!(
        p.next(three_am + 500),
        Next::At(three_am + 500 + 30 * SECOND),
        "**the discriminating one.** The ladder is not due for another 500 ms; \
         a policy that consulted it first would answer At(+1 s) and dial into a \
         venue that is shut"
    );
    assert_eq!(
        p.next(three_am + 2 * SECOND),
        Next::At(three_am + 2 * SECOND + 30 * SECOND),
        "and once the ladder has come due the schedule still decides — the case \
         both orderings agree on, kept because it is the one a reader expects"
    );
}

/// Bounds that describe no useful behaviour are refused rather than repaired.
#[test]
fn bounds_that_mean_nothing_are_refused() {
    assert_eq!(Policy::new(0, 30_000).err(), Some(PolicyError::FirstIsZero));
    assert_eq!(
        Policy::new(10_000, 5_000).err(),
        Some(PolicyError::CeilingBelowFirst),
        "almost always two arguments the wrong way round"
    );
    assert!(
        Policy::new(5_000, 5_000).is_ok(),
        "equal is a fixed interval, which is what QuickFIX's ReconnectInterval is"
    );
}
