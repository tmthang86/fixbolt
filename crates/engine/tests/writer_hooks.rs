//! The three handles a journal outside this crate uses to join the engine's
//! writer bookkeeping: [`WriterTicket`], [`Released::pair`] with its
//! [`Releaser`], and [`Idle`].
//!
//! [ADR-0181](../../../docs/decisions/ADR-0181-a-journal-outside-the-engine-joins-the-engines-writer-bookkeeping-through-three-public-handles.md);
//! plan `docs/plans/2026-09-24-p4-sqlite-store.md`, *Chia việc* row 1.
//!
//! **The retired-writer count is one number for the whole process**, so every
//! test here takes [`serial`] first and leaves the count at zero. This file
//! opens no journal, so nothing else in its process moves the count.
//! [`wait_for_retired_writers`] with a zero timeout is the probe: it answers
//! `true` exactly when the count is zero, without sleeping.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use fixbolt_engine::journal::{
    Released, TicketState, WriterTicket, wait_for_retired_writers, writers_retired,
};
use fixbolt_engine::ring::{IDLE_SLEEP, IDLE_SPINS, Idle};

static SERIAL: Mutex<()> = Mutex::new(());

/// One test at a time: the count they assert on is process-wide.
fn serial() -> MutexGuard<'static, ()> {
    SERIAL.lock().unwrap_or_else(PoisonError::into_inner)
}

/// `true` when no retired writer is outstanding. Never sleeps: the count is
/// read once before the zero timeout is compared.
fn count_is_zero() -> bool {
    wait_for_retired_writers(Duration::ZERO)
}

#[test]
fn a_ticket_retired_twice_is_counted_once() {
    let _serial = serial();
    assert!(count_is_zero(), "premise: no retired writer outstanding");
    let ever = writers_retired();

    let ticket = WriterTicket::new();
    assert_eq!(ticket.state(), TicketState::Running);
    assert!(
        ticket.retire(true),
        "the first retire is the one that counts"
    );
    assert!(!ticket.retire(true), "a second retire changes nothing");
    assert!(!ticket.retire(false), "nor does a third, whatever it says");
    assert_eq!(
        ticket.state(),
        TicketState::Retired,
        "the first retire's word stands: STOP was pushed"
    );
    assert_eq!(
        writers_retired(),
        ever + 1,
        "retired once, counted once, ever"
    );
    assert!(!count_is_zero(), "a retired ticket is waited for");

    ticket.finish();
    assert_eq!(ticket.state(), TicketState::Finished);
    assert!(count_is_zero(), "finishing lowered the count back to zero");
    ticket.finish();
    assert!(
        count_is_zero(),
        "a second finish lowered nothing: the count did not wrap below zero"
    );
}

#[test]
fn finish_without_retire_changes_no_count() {
    let _serial = serial();
    assert!(count_is_zero(), "premise: no retired writer outstanding");

    let never_retired = WriterTicket::new();
    never_retired.finish();
    assert!(
        count_is_zero(),
        "finishing a ticket nobody retired lowered nothing"
    );
    assert_eq!(never_retired.state(), TicketState::Running);

    // With one real writer outstanding, the unretired ticket's finish must not
    // pay that writer's count off for it.
    let retired = WriterTicket::new();
    assert!(retired.retire(false));
    assert_eq!(retired.state(), TicketState::RetiredStopWhenDry);
    never_retired.finish();
    assert!(
        !count_is_zero(),
        "the retired writer is still waited for after another ticket's finish"
    );
    retired.finish();
    assert!(count_is_zero(), "and released once it finishes itself");
}

#[test]
fn wait_for_retired_writers_waits_for_a_ticket_finished_on_another_thread() {
    let _serial = serial();
    assert!(count_is_zero(), "premise: no retired writer outstanding");

    let ticket = WriterTicket::new();
    assert!(ticket.retire(true));
    let writer = ticket.clone();
    let start = Instant::now();
    let finisher = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(50));
        writer.finish();
    });
    assert!(
        wait_for_retired_writers(Duration::from_secs(5)),
        "the writer finished well inside the timeout"
    );
    let waited = start.elapsed();
    assert!(
        waited >= Duration::from_millis(50),
        "wait_for_retired_writers returned after {waited:?}, before the writer finished at 50 ms"
    );
    finisher.join().unwrap();
    assert_eq!(ticket.state(), TicketState::Finished);
}

#[test]
fn released_turns_true_only_when_its_releaser_releases() {
    let _serial = serial();
    let (releaser, released) = Released::pair();
    let kept = released.clone();
    assert!(!released.is_released(), "nothing has let go yet");

    // Another pair's releaser says nothing about this one.
    let (other_releaser, other) = Released::pair();
    other_releaser.release();
    assert!(other.is_released());
    assert!(
        !released.is_released(),
        "another writer's release is not ours"
    );
    assert!(!kept.is_released());

    releaser.release();
    assert!(released.is_released(), "our writer let go");
    assert!(kept.is_released(), "every clone hears it");
}

#[test]
fn idle_is_reachable_from_outside_the_crate() {
    let _serial = serial();
    let mut idle = Idle::new();

    // The spins: IDLE_SPINS empty polls, none of which sleeps. Bounded far
    // above what spinning costs and far below one sleep each, so a preempted
    // runner does not turn it red.
    let spun = Instant::now();
    for _ in 0..IDLE_SPINS {
        idle.wait();
    }
    let spinning = spun.elapsed();
    let all_slept = IDLE_SLEEP * IDLE_SPINS;
    assert!(
        spinning < all_slept / 4,
        "{IDLE_SPINS} spins took {spinning:?}: they slept"
    );

    // Spins spent: the next empty poll sleeps at least IDLE_SLEEP.
    let slept = Instant::now();
    idle.wait();
    let sleeping = slept.elapsed();
    assert!(
        sleeping >= IDLE_SLEEP,
        "once the spins are spent an empty poll sleeps {IDLE_SLEEP:?}; it took {sleeping:?}"
    );

    // A record arrived: spinning starts again.
    idle.reset();
    let respun = Instant::now();
    for _ in 0..IDLE_SPINS {
        idle.wait();
    }
    let respinning = respun.elapsed();
    assert!(
        respinning < all_slept / 4,
        "after reset {IDLE_SPINS} polls took {respinning:?}: reset did not restart the spins"
    );
}
