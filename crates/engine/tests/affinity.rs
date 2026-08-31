//! Step 2 of [threads-and-affinity]: does pinning actually pin?
//!
//! [ADR-0015](../../../docs/decisions/ADR-0015-explicit-cores-pinned-from-inside-and-read-back.md)
//! decision 2 says a call returning `Ok` is not evidence, so every test here
//! looks at something other than the return value: the mask the kernel gives
//! back, and the `processor` field of `/proc/thread-self/stat`, which is
//! written by the scheduler rather than by us.
//!
//! **Every pinning test runs on a spawned thread.** The test binary shares one
//! process; pinning the harness thread would narrow every other test in this
//! file to one core and the damage would look like flakiness.
//!
//! [threads-and-affinity]: ../../../docs/plans/2026-08-30-threads-and-affinity.md

#![cfg(all(feature = "affinity", target_os = "linux"))]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use fixbolt_engine::affinity::{self, AffinityError, CoreId};

/// A core this thread is allowed to run on, whatever the process's mask is.
///
/// Not `CoreId(0)`: a cgroup cpuset can exclude it, and a test that fails
/// because of the container it runs in is testing the container.
fn a_core_we_may_use() -> CoreId {
    let mask = affinity::current_mask().expect("reading this thread's own mask");
    *mask
        .first()
        .expect("a thread is allowed on at least one core")
}

#[test]
fn pinning_reads_back_as_the_core_that_was_asked_for() {
    let result = std::thread::spawn(|| {
        let core = a_core_we_may_use();
        affinity::pin_current_thread(core)?;
        // The kernel's answer, not ours.
        let mask = affinity::current_mask()?;
        Ok::<_, AffinityError>((core, mask))
    })
    .join()
    .expect("the thread did not panic");

    let (core, mask) = result.expect("pinning a core we are already on");
    assert_eq!(
        mask,
        vec![core],
        "after pinning to {core:?} the kernel should report exactly that one core"
    );
}

#[test]
fn a_pinned_thread_stays_on_its_core_while_it_works() {
    let seen = std::thread::spawn(|| {
        let core = a_core_we_may_use();
        affinity::pin_current_thread(core)?;

        // `processor` in /proc/thread-self/stat is where the scheduler last
        // ran this thread. Sampled while it is doing work, so the answer is not
        // about a thread that never got scheduled again.
        let mut seen = std::collections::BTreeSet::new();
        let mut sink = 0u64;
        for i in 0..200u64 {
            for j in 0..20_000u64 {
                sink = sink.wrapping_add(i ^ j);
            }
            seen.insert(affinity::running_on()?);
        }
        assert!(sink != u64::MAX, "keep the loop");
        Ok::<_, AffinityError>((core, seen))
    })
    .join()
    .expect("the thread did not panic");

    let (core, seen) = seen.expect("pinning a core we are already on");
    assert_eq!(
        seen.into_iter().collect::<Vec<_>>(),
        vec![core],
        "a pinned thread must be observed on exactly one core, {core:?}"
    );
}

#[test]
fn a_core_that_does_not_exist_is_an_error_and_not_a_panic() {
    let err = std::thread::spawn(|| affinity::pin_current_thread(CoreId(9999)))
        .join()
        .expect("the thread did not panic")
        .expect_err("core 9999 does not exist on any machine this runs on");

    assert!(
        matches!(err, AffinityError::NoSuchCore(CoreId(9999))),
        "expected NoSuchCore(9999), got {err:?}"
    );
}

#[test]
fn failing_to_pin_leaves_the_thread_where_it_was() {
    // Decision 3 stops startup on a failure; it must not also leave the thread
    // in a half-changed state on the way out.
    let ok = std::thread::spawn(|| {
        let before = affinity::current_mask()?;
        let err = affinity::pin_current_thread(CoreId(9999)).expect_err("must fail");
        let after = affinity::current_mask()?;
        Ok::<_, AffinityError>((before == after, err))
    })
    .join()
    .expect("the thread did not panic");

    let (unchanged, err) = ok.expect("reading the mask");
    assert!(
        unchanged,
        "a refused pin changed the thread's affinity anyway, after {err:?}"
    );
}

#[test]
fn the_error_type_names_the_core_that_was_refused() {
    // CLAUDE.md §6 asks for fieldless errors on a hot path. This one is startup
    // only, and ADR-0015 decision 4 keeps the core in it because
    // `NoSuchCore(CoreId(9999))` tells an operator what to change.
    let err = AffinityError::NoSuchCore(CoreId(9999));
    let text = format!("{err}");
    assert!(
        text.contains("9999"),
        "the message must name the core; got {text:?}"
    );
}
