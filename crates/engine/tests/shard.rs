//! Step 4 of [threads-and-affinity]: the shard runtime, without a session in
//! sight.
//!
//! Every test here hands [`Shards`] something that is **not** an engine — a
//! counter behind the same [`Shardable`] trait. That is what the trait is for:
//! the runtime's own behaviour (does it pin, does it refuse, does it spread
//! connections, does it shut down) is separable from whether FIX works, and
//! mixing the two would mean debugging a protocol failure to learn something
//! about a channel.
//!
//! The 59 acceptance definitions **through** this runtime are in
//! `tests/shard_wire.rs`.
//!
//! [threads-and-affinity]: ../../../docs/plans/2026-08-30-threads-and-affinity.md

#![cfg(all(feature = "affinity", target_os = "linux"))]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use fixbolt_engine::affinity::{AffinityError, CoreId, ShardPlan, Topology};
use fixbolt_engine::shard::{Assign, ShardError, Shardable, Shards};
use fixbolt_engine::transport::TcpTransport;

/// What this machine can host.
///
/// `[measured 2026-08-31]` a GitHub runner has **two vCPUs that are two threads
/// of one physical core**, so it cannot host two shards at all — and the runtime
/// is right to refuse, which is what the second arm asserts. Neither arm is a
/// skip: on a machine with room the test runs, and on one without it the test
/// checks the refusal. A `#[test]` that returns early reports `ok`, and a green
/// nobody earned is exactly what `CLAUDE.md` §10 is about.
enum Hosting {
    /// Cores this machine accepts, one per physical core, isolation waived —
    /// CI has no `isolcpus` and neither does a laptop.
    Room(ShardPlan),
    /// Fewer physical cores than shards asked for, and the ids that collide.
    NoRoom { plan: ShardPlan, wanted: usize },
}

fn hosting_for(shards: usize) -> Hosting {
    let topology = Topology::read().expect("reading /sys on Linux");
    let mut cores: Vec<CoreId> = Vec::new();
    for candidate in topology.online() {
        if cores
            .iter()
            .any(|taken| topology.siblings_of(*taken).contains(candidate))
        {
            continue;
        }
        cores.push(*candidate);
        if cores.len() == shards {
            break;
        }
    }
    if cores.len() == shards {
        return Hosting::Room(ShardPlan::new(cores).allow_unisolated());
    }
    // Not enough physical cores. Build the plan somebody would write anyway —
    // the first `shards` online ids — so the refusal can be asserted.
    let naive: Vec<CoreId> = topology.online().iter().copied().take(shards).collect();
    Hosting::NoRoom {
        plan: ShardPlan::new(naive).allow_unisolated(),
        wanted: shards,
    }
}

/// The plan for `shards` shards, or `None` with the refusal already asserted.
fn plan_for(shards: usize) -> Option<ShardPlan> {
    match hosting_for(shards) {
        Hosting::Room(plan) => Some(plan),
        Hosting::NoRoom { plan, wanted } => {
            let dropped = Arc::new(AtomicUsize::new(0));
            let d = Arc::clone(&dropped);
            let refused = Shards::start(&plan, move |_| Counter {
                added: Arc::new(AtomicUsize::new(0)),
                dropped: Arc::clone(&d),
                held: Vec::new(),
            });
            match refused.err() {
                Some(ShardError::Affinity(AffinityError::SmtSiblingOf(_, _))) => {}
                Some(other) => panic!(
                    "this machine has fewer than {wanted} physical cores, so the plan must be \
                     refused as SMT siblings; got {other:?}"
                ),
                None => panic!(
                    "this machine has fewer than {wanted} physical cores and the runtime \
                     started {wanted} shards on them anyway"
                ),
            }
            None
        }
    }
}

/// A shard that counts what it is given and records that it was dropped.
///
/// `turn` always reports that nothing moved, so the runtime's loop goes straight
/// to `idle` — which yields, because a spinning stub in a test suite pins a core
/// for no reason.
struct Counter {
    added: Arc<AtomicUsize>,
    dropped: Arc<AtomicUsize>,
    /// Held so the accepted sockets stay open for as long as the shard does.
    held: Vec<TcpTransport>,
}

impl Shardable for Counter {
    fn add(&mut self, transport: TcpTransport) {
        self.held.push(transport);
        self.added.fetch_add(1, Ordering::Release);
    }
    fn turn(&mut self) -> bool {
        false
    }
    fn idle(&mut self) {
        std::thread::yield_now();
    }
}

impl Drop for Counter {
    fn drop(&mut self) {
        self.dropped.fetch_add(1, Ordering::Release);
    }
}

/// One real connected socket, wrapped the way the acceptor would wrap it.
fn a_connection(listener: &TcpListener) -> TcpTransport {
    let addr = listener.local_addr().expect("bound");
    let _client = TcpStream::connect(addr).expect("loopback");
    let (sock, _) = listener.accept().expect("accepted");
    // Leak the client end: these tests care about where a connection lands, not
    // about what is sent over it.
    core::mem::forget(_client);
    TcpTransport::new(sock).expect("non-blocking")
}

fn spin_until(mut done: impl FnMut() -> bool, within: Duration) -> bool {
    let start = Instant::now();
    while start.elapsed() < within {
        if done() {
            return true;
        }
        std::thread::yield_now();
    }
    done()
}

#[test]
fn every_shard_thread_confirms_the_core_it_was_given() {
    let Some(plan) = plan_for(2) else { return };
    let added = Arc::new(AtomicUsize::new(0));
    let dropped = Arc::new(AtomicUsize::new(0));
    let (a, d) = (Arc::clone(&added), Arc::clone(&dropped));

    let shards = Shards::start(&plan, move |_| Counter {
        added: Arc::clone(&a),
        dropped: Arc::clone(&d),
        held: Vec::new(),
    })
    .expect("a plan this machine accepts");

    // Not the plan's own cores echoed back: each thread read this from the
    // scheduler after pinning itself.
    assert_eq!(
        shards.confirmed_cores(),
        plan.shards(),
        "the threads are not on the cores the plan named"
    );
    assert_eq!(shards.len(), 2);
    assert!(shards.all_alive());
}

/// `ShardPlan::validate()` is what refuses, and not the pin that follows it.
///
/// **The test above does not prove this and its name used to suggest it did.**
/// `[measured 2026-08-31]` with `plan.validate()` deleted and the pin left in
/// place, that test still passed: `pin_current_thread(CoreId(9999))` fails
/// inside the thread, the same `NoSuchCore` comes back, and no engine is built
/// either way. Both of its assertions are true whether or not ADR-0015
/// decision 6 is honoured.
///
/// This one separates them. `cpu0` is online everywhere, so **the pin would
/// succeed**; it is outside `isolcpus` on the reference machine and on CI, so
/// only `validate()` refuses it. Delete `validate()` and this goes red while
/// the other stays green.
#[test]
fn validation_is_what_refuses_and_not_the_pin_behind_it() {
    let topology = Topology::read().expect("reading /sys on Linux");
    let core = topology
        .online()
        .iter()
        .find(|c| !topology.isolated().contains(c))
        .copied()
        .expect("some online core is outside isolcpus on any real machine");

    let dropped = Arc::new(AtomicUsize::new(0));
    let d = Arc::clone(&dropped);
    // No allow_unisolated: this is the rule under test.
    let plan = ShardPlan::new(vec![core]);
    let result = Shards::start(&plan, move |_| Counter {
        added: Arc::new(AtomicUsize::new(0)),
        dropped: Arc::clone(&d),
        held: Vec::new(),
    });

    match result.err() {
        Some(ShardError::Affinity(AffinityError::NotIsolated(refused))) => {
            assert_eq!(refused, core);
        }
        Some(other) => panic!("expected NotIsolated({core}), got {other:?}"),
        None => panic!("an unisolated core must be refused before any thread starts"),
    }
}

#[test]
fn a_plan_the_machine_refuses_starts_nothing_at_all() {
    let built = Arc::new(AtomicUsize::new(0));
    let b = Arc::clone(&built);
    let dropped = Arc::new(AtomicUsize::new(0));
    let d = Arc::clone(&dropped);

    let plan = ShardPlan::new(vec![CoreId(9999)]).allow_unisolated();
    let result = Shards::start(&plan, move |_| {
        b.fetch_add(1, Ordering::Release);
        Counter {
            added: Arc::new(AtomicUsize::new(0)),
            dropped: Arc::clone(&d),
            held: Vec::new(),
        }
    });

    match result.err() {
        Some(ShardError::Affinity(AffinityError::NoSuchCore(CoreId(9999)))) => {}
        Some(other) => panic!("expected NoSuchCore(9999), got {other:?}"),
        None => panic!("a plan naming cpu9999 must not start"),
    }
    assert_eq!(
        built.load(Ordering::Acquire),
        0,
        "the plan was refused, so no engine may have been built"
    );
}

#[test]
fn round_robin_spreads_connections_across_shards() {
    let Some(plan) = plan_for(2) else { return };
    let counts: Vec<Arc<AtomicUsize>> = (0..2).map(|_| Arc::new(AtomicUsize::new(0))).collect();
    let per_shard = counts.clone();
    let dropped = Arc::new(AtomicUsize::new(0));
    let d = Arc::clone(&dropped);

    let mut shards = Shards::start(&plan, move |i| Counter {
        added: Arc::clone(&per_shard[i]),
        dropped: Arc::clone(&d),
        held: Vec::new(),
    })
    .expect("a plan this machine accepts");

    let listener = TcpListener::bind("127.0.0.1:0").expect("a free port");
    let mut landed = Vec::new();
    for _ in 0..4 {
        landed.push(shards.hand(a_connection(&listener)).expect("handed"));
    }
    assert_eq!(landed, vec![0, 1, 0, 1], "round robin, in accept order");

    assert!(
        spin_until(
            || counts.iter().all(|c| c.load(Ordering::Acquire) == 2),
            Duration::from_secs(5)
        ),
        "each shard should have taken two: {:?}",
        counts
            .iter()
            .map(|c| c.load(Ordering::Acquire))
            .collect::<Vec<_>>()
    );
}

#[test]
fn an_assignment_outside_the_range_is_refused_and_not_clamped() {
    struct AlwaysNinetyNine;
    impl Assign for AlwaysNinetyNine {
        fn shard_for(&mut self, _shards: usize) -> usize {
            99
        }
    }

    let Some(plan) = plan_for(1) else { return };
    let dropped = Arc::new(AtomicUsize::new(0));
    let d = Arc::clone(&dropped);
    let mut shards = Shards::start(&plan, move |_| Counter {
        added: Arc::new(AtomicUsize::new(0)),
        dropped: Arc::clone(&d),
        held: Vec::new(),
    })
    .expect("a plan this machine accepts")
    .with_assign(Box::new(AlwaysNinetyNine));

    let listener = TcpListener::bind("127.0.0.1:0").expect("a free port");
    match shards.hand(a_connection(&listener)) {
        Err(ShardError::BadAssignment { shard: 99, of: 1 }) => {}
        Err(other) => panic!("expected BadAssignment {{ 99, 1 }}, got {other:?}"),
        Ok(landed) => panic!("an out-of-range assignment landed on shard {landed}"),
    }
}

#[test]
fn dropping_the_runtime_ends_every_thread() {
    let Some(plan) = plan_for(2) else { return };
    let dropped = Arc::new(AtomicUsize::new(0));
    let d = Arc::clone(&dropped);

    let shards = Shards::start(&plan, move |_| Counter {
        added: Arc::new(AtomicUsize::new(0)),
        dropped: Arc::clone(&d),
        held: Vec::new(),
    })
    .expect("a plan this machine accepts");

    assert_eq!(dropped.load(Ordering::Acquire), 0, "nothing dropped yet");
    drop(shards);

    assert!(
        spin_until(
            || dropped.load(Ordering::Acquire) == 2,
            Duration::from_secs(5)
        ),
        "both engines should have been dropped, saw {}",
        dropped.load(Ordering::Acquire)
    );
}
