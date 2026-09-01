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

use std::io::Write;
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use fixbolt_engine::affinity::{AffinityError, CoreId, ShardPlan, Topology};
use fixbolt_engine::presession::{Identity, Limits, One, Pending, PendingSet};
use fixbolt_engine::shard::{Route, ShardError, Shardable, Shards};
use fixbolt_engine::transport::TcpTransport;
use fixbolt_session::Config;

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
            let refused = Shards::<PRE>::start(&plan, move |_| Counter {
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
    fn add(&mut self, transport: TcpTransport, cfg: Config, prefix: &[u8]) -> bool {
        assert!(!prefix.is_empty(), "a routed connection carries its Logon");
        // The registry chose it before the connection was handed over — a shard
        // never guesses whose socket it has (ADR-0030).
        assert!(
            cfg.serves(b"TW44", b"ISLD"),
            "the configuration travels with the connection"
        );
        self.held.push(transport);
        self.added.fetch_add(1, Ordering::Release);
        true
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
const PRE: usize = 4096;

/// `[measured 2026-09-01]` read off the implementation once and then frozen.
/// Changing the hash changes these, and that is the point: a deployment's
/// counterparties would move between shards, which is a migration and not a
/// refactor. See
/// [`the_route_is_written_down_and_not_merely_deterministic_today`].
const PINNED: [usize; 4] = [1, 0, 3, 1];

/// A `Logon` from `sender` to ISLD, with a real body length and checksum.
fn logon_from(sender: &str) -> Vec<u8> {
    let body = format!(
        "35=A\u{1}34=1\u{1}49={sender}\u{1}52=20260828-12:00:00\u{1}56=ISLD\u{1}98=0\u{1}108=30\u{1}"
    );
    let head = format!("8=FIX.4.4\u{1}9={}\u{1}", body.len());
    let mut wire = head.into_bytes();
    wire.extend_from_slice(body.as_bytes());
    let sum: u32 = wire.iter().map(|b| u32::from(*b)).sum();
    wire.extend_from_slice(format!("10={:03}\u{1}", sum % 256).as_bytes());
    wire
}

/// A connection that has already said who it is, the way the acceptor loop
/// hands one over.
///
/// The counterparty the acceptance corpus logs on as: `49=TW44` in, `56=ISLD`
/// in, so this end is `ISLD` and the counterparty is `TW44`.
///
/// Before [ADR-0026] the pre-session stage let every identity through and the
/// session refused the wrong ones. Now the stage asks a [`Registry`] first, so
/// these tests have to say who this acceptor serves — and it is the same
/// counterparty the corpus was always logging on as.
///
/// [ADR-0026]: ../../../docs/decisions/ADR-0026-a-counterparty-registry-in-the-pre-session-stage.md
/// [`Registry`]: fixbolt_engine::presession::Registry
fn cfg() -> Config {
    Config::acceptor(b"FIX.4.4", b"ISLD", b"TW44")
}

/// It goes through a real `PendingSet` rather than being built by hand: the
/// only way to make a `Pending` is to have read a whole `Logon` off a socket,
/// and a test that could shortcut that would be testing a different thing.
fn a_connection_from(listener: &TcpListener, sender: &str) -> Pending<TcpTransport, PRE> {
    let addr = listener.local_addr().expect("bound");
    let mut client = TcpStream::connect(addr).expect("loopback");
    let (sock, _) = listener.accept().expect("accepted");
    let wire = logon_from(sender);
    client
        .write_all(&wire)
        .expect("the Logon fits a socket buffer");
    // Leak the client end: these tests care about where a connection lands.
    core::mem::forget(client);

    let mut set: PendingSet<TcpTransport, One, PRE> = PendingSet::new(
        Limits::new(1, 30_000).expect("both above zero"),
        One::new(cfg()),
    );
    let t = TcpTransport::new(sock).expect("non-blocking");
    assert!(set.admit(t, 0).is_ok(), "room for one");
    let deadline = Instant::now() + Duration::from_secs(5);
    while set.settled().is_none() && Instant::now() < deadline {
        set.turn(0);
    }
    let i = set.settled().expect("the Logon arrived over loopback");
    set.take(i).expect("out")
}

fn a_connection(listener: &TcpListener) -> Pending<TcpTransport, PRE> {
    a_connection_from(listener, "TW44")
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

    let shards = Shards::<PRE>::start(&plan, move |_| Counter {
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
    let result = Shards::<PRE>::start(&plan, move |_| Counter {
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
    let result = Shards::<PRE>::start(&plan, move |_| {
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
fn the_same_identity_always_lands_on_the_same_shard() {
    let Some(plan) = plan_for(2) else { return };
    let counts: Vec<Arc<AtomicUsize>> = (0..2).map(|_| Arc::new(AtomicUsize::new(0))).collect();
    let per_shard = counts.clone();
    let dropped = Arc::new(AtomicUsize::new(0));
    let d = Arc::clone(&dropped);

    let mut shards = Shards::<PRE>::start(&plan, move |i| Counter {
        added: Arc::clone(&per_shard[i]),
        dropped: Arc::clone(&d),
        held: Vec::new(),
    })
    .expect("a plan this machine accepts");

    let listener = TcpListener::bind("127.0.0.1:0").expect("a free port");
    // Two identities that the stable hash sends to different shards — asserted
    // below, so this is a fact about the hash and not a hope about it.
    let mut landed = Vec::new();
    for who in ["TW44", "TW45", "TW44", "TW45"] {
        landed.push(
            shards
                .hand(a_connection_from(&listener, who))
                .expect("handed"),
        );
    }
    assert_eq!(
        landed[0], landed[2],
        "the same identity must always land on the same shard — the single-logon \
         rule can only count connections one engine holds"
    );
    assert_eq!(landed[1], landed[3]);
    assert_ne!(
        landed[0], landed[1],
        "and these two identities are on different shards, which is what makes \
         the counts below mean anything"
    );

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
fn a_route_outside_the_range_is_refused_and_not_clamped() {
    struct AlwaysNinetyNine;
    impl Route for AlwaysNinetyNine {
        fn shard_for(&mut self, _id: Identity<'_>, _shards: usize) -> usize {
            99
        }
    }

    let Some(plan) = plan_for(1) else { return };
    let dropped = Arc::new(AtomicUsize::new(0));
    let d = Arc::clone(&dropped);
    let mut shards = Shards::<PRE>::start(&plan, move |_| Counter {
        added: Arc::new(AtomicUsize::new(0)),
        dropped: Arc::clone(&d),
        held: Vec::new(),
    })
    .expect("a plan this machine accepts")
    .with_route(Box::new(AlwaysNinetyNine));

    let listener = TcpListener::bind("127.0.0.1:0").expect("a free port");
    match shards.hand(a_connection(&listener)) {
        Err(ShardError::BadRoute { shard: 99, of: 1 }) => {}
        Err(other) => panic!("expected BadRoute {{ 99, 1 }}, got {other:?}"),
        Ok(landed) => panic!("an out-of-range assignment landed on shard {landed}"),
    }
}

#[test]
fn dropping_the_runtime_ends_every_thread() {
    let Some(plan) = plan_for(2) else { return };
    let dropped = Arc::new(AtomicUsize::new(0));
    let d = Arc::clone(&dropped);

    let shards = Shards::<PRE>::start(&plan, move |_| Counter {
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

/// The route is written down, not merely deterministic-for-now.
///
/// [ADR-0020] decision 7: the same counterparty must reach the same shard on
/// this run, on the **next** run, and after a reconnect, because the
/// single-logon rule can only count connections one engine holds.
/// `DefaultHasher` is seeded per process and would pass a
/// "twice in a row is the same" test while failing across a restart — the worst
/// shape a bug can have. These constants are what makes that impossible: a
/// seeded hash cannot reproduce them.
///
/// [ADR-0020]: ../../../docs/decisions/ADR-0020-a-pre-session-stage-owns-the-socket-until-logon.md
#[test]
fn the_route_is_written_down_and_not_merely_deterministic_today() {
    use fixbolt_engine::shard::HashRoute;
    fn at(sender: &[u8], target: &[u8], shards: usize) -> usize {
        HashRoute.shard_for(Identity::comp_ids(sender, target), shards)
    }

    assert_eq!(at(b"TW44", b"ISLD", 2), PINNED[0]);
    assert_eq!(at(b"TW45", b"ISLD", 2), PINNED[1]);
    assert_eq!(at(b"TW44", b"ISLD", 8), PINNED[2]);
    assert_eq!(at(b"WT", b"DLSI", 8), PINNED[3]);

    // The separator between the halves is load-bearing: without it these two
    // are the same byte string and two different counterparties would share a
    // shard for a reason nobody could see.
    let a = (0..64).map(|n| at(b"AB", b"C", n + 1)).collect::<Vec<_>>();
    let b = (0..64).map(|n| at(b"A", b"BC", n + 1)).collect::<Vec<_>>();
    assert_ne!(a, b, "(AB, C) and (A, BC) must not hash alike");

    // Out of range never happens by construction, at any width.
    for n in 1..64usize {
        assert!(at(b"TW44", b"ISLD", n) < n);
    }
    assert_eq!(
        at(b"TW44", b"ISLD", 0),
        0,
        "zero shards is answered, not divided by"
    );
}
