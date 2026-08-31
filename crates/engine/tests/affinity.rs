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

// ---------------------------------------------------------------------------
// Step 3 — the refusals
// ---------------------------------------------------------------------------

use fixbolt_engine::affinity::{ShardPlan, Topology};

/// The §9 desktop as it actually reads, `[measured 2026-08-31]`, tuned and
/// scoring `pass 10 fail 0`.
///
/// Committed as a fixture because of what it contains: `isolated` names 14 and
/// 15, and `online` does not. A validator that read `isolated` alone would
/// accept a core that cannot run anything, and this is the machine where that
/// would have happened.
fn tuned_desktop() -> Topology {
    Topology::from_sysfs("0-15", "0-7", "6-7,14-15", &[(6, "6"), (7, "7")])
        .expect("the fixture parses")
}

/// The same machine before §9 turned SMT off, `[measured 2026-08-30]`.
/// `cpu6`↔`cpu14` and `cpu7`↔`cpu15` were sibling pairs then.
fn desktop_with_smt_on() -> Topology {
    Topology::from_sysfs(
        "0-15",
        "0-15",
        "6-7,14-15",
        &[(6, "6,14"), (7, "7,15"), (14, "6,14"), (15, "7,15")],
    )
    .expect("the fixture parses")
}

#[test]
fn a_cpu_list_parses_the_shapes_sysfs_actually_writes() {
    let t = Topology::from_sysfs("0-15", "0-3,7", "", &[]).expect("parses");
    assert_eq!(t.online().len(), 5, "0,1,2,3,7");
    assert!(t.isolated().is_empty(), "an empty isolated file means none");
    assert_eq!(t.present().len(), 16);
}

#[test]
fn a_core_that_is_not_present_at_all_is_refused() {
    let plan = ShardPlan::new(vec![CoreId(99)]);
    assert_eq!(
        tuned_desktop().validate(&plan),
        Err(AffinityError::NoSuchCore(CoreId(99)))
    );
}

#[test]
fn an_isolated_core_that_is_offline_is_still_refused() {
    // The trap this fixture exists for. cpu14 IS in isolcpus; it is also
    // offline, because §9 turns SMT off. Reading `isolated` alone would accept
    // it.
    let plan = ShardPlan::new(vec![CoreId(14)]);
    assert_eq!(
        tuned_desktop().validate(&plan),
        Err(AffinityError::NotOnline(CoreId(14))),
        "an isolated but offline core must be refused, not accepted"
    );
}

#[test]
fn a_core_outside_isolcpus_is_refused_by_default() {
    let plan = ShardPlan::new(vec![CoreId(0)]);
    assert_eq!(
        tuned_desktop().validate(&plan),
        Err(AffinityError::NotIsolated(CoreId(0)))
    );
}

#[test]
fn allow_unisolated_lifts_exactly_one_rule_and_no_other() {
    let ok = ShardPlan::new(vec![CoreId(0)]).allow_unisolated();
    assert_eq!(tuned_desktop().validate(&ok), Ok(()));

    let still_bad = ShardPlan::new(vec![CoreId(99)]).allow_unisolated();
    assert_eq!(
        tuned_desktop().validate(&still_bad),
        Err(AffinityError::NoSuchCore(CoreId(99))),
        "allow_unisolated must not become allow_anything"
    );

    let also_bad = ShardPlan::new(vec![CoreId(14)]).allow_unisolated();
    assert_eq!(
        tuned_desktop().validate(&also_bad),
        Err(AffinityError::NotOnline(CoreId(14)))
    );
}

#[test]
fn an_isolated_online_core_is_accepted() {
    // Without this the whole file could pass by refusing everything.
    let plan = ShardPlan::new(vec![CoreId(6), CoreId(7)]);
    assert_eq!(tuned_desktop().validate(&plan), Ok(()));
}

#[test]
fn two_shards_on_smt_siblings_are_refused() {
    let plan = ShardPlan::new(vec![CoreId(6), CoreId(14)]);
    assert_eq!(
        desktop_with_smt_on().validate(&plan),
        Err(AffinityError::SmtSiblingOf(CoreId(6), CoreId(14)))
    );
}

#[test]
fn the_same_core_named_twice_is_refused() {
    let plan = ShardPlan::new(vec![CoreId(6), CoreId(6)]);
    assert_eq!(
        tuned_desktop().validate(&plan),
        Err(AffinityError::DuplicateCore(CoreId(6)))
    );
}

#[test]
fn a_support_thread_may_not_sit_on_a_shard_core() {
    // ADR-0015 decision 8: pinning the engine and letting the journal writer
    // float defeats the isolation. Naming the engine's own core defeats it
    // harder.
    let plan = ShardPlan::new(vec![CoreId(6)]).with_journal_core(CoreId(6));
    assert_eq!(
        tuned_desktop().validate(&plan),
        Err(AffinityError::DuplicateCore(CoreId(6)))
    );
}

#[test]
fn a_support_thread_on_an_smt_sibling_of_a_shard_is_refused() {
    let plan = ShardPlan::new(vec![CoreId(6)]).with_journal_core(CoreId(14));
    assert_eq!(
        desktop_with_smt_on().validate(&plan),
        Err(AffinityError::SmtSiblingOf(CoreId(6), CoreId(14))),
        "a support thread sharing a physical core with a shard contends with it"
    );
}

#[test]
fn a_support_thread_need_not_be_isolated() {
    // It is not the engine thread. Requiring isolation for it would push it onto
    // the cores this design is trying to keep clear.
    let plan = ShardPlan::new(vec![CoreId(6)]).with_journal_core(CoreId(0));
    assert_eq!(tuned_desktop().validate(&plan), Ok(()));
}

#[test]
fn a_plan_with_no_shards_is_refused() {
    assert_eq!(
        tuned_desktop().validate(&ShardPlan::new(vec![])),
        Err(AffinityError::EmptyPlan)
    );
}

#[test]
fn this_machine_reads_back_consistently() {
    let t = Topology::read().expect("reading /sys on Linux");
    assert!(!t.present().is_empty(), "some CPU is present");
    for core in t.online() {
        assert!(
            t.present().contains(core),
            "{core} is online but not present, which cannot be true"
        );
    }
    // The plan a caller would actually write on this machine must be accepted
    // by the machine's own reading, not only by the fixture.
    if let Some(core) = t.isolated().iter().find(|c| t.online().contains(c)) {
        assert_eq!(t.validate(&ShardPlan::new(vec![*core])), Ok(()));
    }
}

// ---------------------------------------------------------------------------
// Step 5 — the threads that are not engine threads
// ---------------------------------------------------------------------------

use fixbolt_engine::journal::{Durability, FileJournal};

/// A core this thread is **not** on.
///
/// Using the current one would make the reversal below useless: a spawned thread
/// often starts on its parent's core, so an unpinned thread would read the right
/// answer by accident and the test would pass with the pin removed.
fn a_core_we_are_not_on() -> CoreId {
    let here = affinity::running_on().expect("this thread is somewhere");
    let mask = affinity::current_mask().expect("reading this thread's own mask");
    mask.into_iter()
        .find(|c| *c != here)
        .expect("this machine has more than one usable core")
}

#[test]
fn spawn_pinned_reports_the_core_the_thread_was_observed_on() {
    let core = a_core_we_are_not_on();
    let (handle, on) =
        affinity::spawn_pinned("test-pinned", core, std::thread::yield_now).expect("pinned");
    assert_eq!(on, core, "the thread reported a core it was not asked for");
    handle.join().expect("the thread did not panic");
}

#[test]
fn spawn_pinned_reports_a_bad_core_to_the_caller_that_asked() {
    // Decision 3: the failure reaches the thread that can stop startup, rather
    // than dying quietly on the new thread.
    let err = affinity::spawn_pinned("test-doomed", CoreId(9999), || {})
        .expect_err("cpu9999 does not exist");
    assert!(
        err.to_string().contains("9999"),
        "the message must name the core; got {err}"
    );
}

#[test]
fn the_journal_writer_runs_on_the_core_it_was_given() {
    let dir = std::env::temp_dir().join(format!(
        "fixbolt-journal-affinity-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    std::fs::create_dir_all(&dir).expect("a temp directory");
    let path = dir.join("journal.bin");

    let core = a_core_we_are_not_on();
    let Ok(journal) = FileJournal::<8, 256>::open_pinned(&path, Durability::Async, core) else {
        panic!("opening a pinned journal on {core}");
    };

    assert_eq!(
        journal.writer_core(),
        Some(core),
        "the writer thread is not on the core the caller named"
    );

    drop(journal);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn pinning_a_journal_that_has_no_writer_thread_is_refused() {
    // `Fsync` writes on the engine thread. Accepting a core here and ignoring
    // it is how a deployment ends up believing it pinned something.
    let dir = std::env::temp_dir().join(format!("fixbolt-journal-fsync-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("a temp directory");
    let path = dir.join("journal.bin");

    let Err(err) = FileJournal::<8, 256>::open_pinned(&path, Durability::Fsync, CoreId(0)) else {
        panic!("Fsync has no writer thread, so pinning one must be refused");
    };
    assert!(
        err.to_string().contains("no writer thread"),
        "the refusal must say why; got {err}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
