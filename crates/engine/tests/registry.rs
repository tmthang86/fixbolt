//! Two counterparties, two `Config`s, **one acceptor**.
//!
//! Step 1 of [counterparty-registry], and it is written to be **red**. What it
//! asks for is the thing [ADR-0026] decided and nothing here can do yet: a FIX
//! gateway serves many counterparties, and this engine serves exactly one.
//!
//! # Why it is red, and it is not "does not compile"
//!
//! `Config` pins a single `target_comp_id` (`crates/session/src/lib.rs:259`),
//! the `Logon` check requires the inbound `49=` to match it (`:1154`), and
//! every entry point takes **one** `Config` — `serve_sharded_hft` hands the
//! same one to every shard (`crates/engine/src/shard.rs:410`, `:431`). So an
//! acceptor can be *told about* two counterparties and can only *hold* one.
//! [`gateway`] is where that shows: it takes a slice and can use one element of
//! it. **The second counterparty is refused in silence**, which is what these
//! two tests report.
//!
//! The irony ADR-0026 names is visible from here: `presession::identity_of`
//! already reads `(49, 56)` off the `Logon` and `HashRoute` already spreads
//! distinct identities across shards — each of which rejects every identity but
//! one. Routing by identity today chooses between engines that all say no.
//!
//! # What step 2 changes, and what it must not
//!
//! Only [`gateway`] — the construction. The two `#[test]` functions are the
//! specification and do not move; if they had to be edited to go green they
//! were never measuring what they claim (`CLAUDE.md` §10).
//!
//! # Why a `Loopback` and a `ManualClock`
//!
//! The same reasons `tests/pending.rs` gives: time is a `u64` the test moves by
//! hand, so nothing here is timing-sensitive, and no port is bound, so nothing
//! here fails on a busy machine. The session, the framer, the pre-session stage
//! and the application are all the real ones.
//!
//! [counterparty-registry]: ../../../docs/plans/2026-09-01-counterparty-registry.md
//! [ADR-0026]: ../../../docs/decisions/ADR-0026-a-counterparty-registry-in-the-pre-session-stage.md
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::ops::Range;

use fixbolt_conformance::script::{FIXED_TIME_MILLIS, Kind, load_all};
use fixbolt_engine::clock::ManualClock;
use fixbolt_engine::dispatch::InlineDispatch;
use fixbolt_engine::journal::Store;
use fixbolt_engine::presession::{Identity, Limits, PendingSet, Registry, Table, is_logon};
use fixbolt_engine::transport::{Io, Loopback, Transport};
use fixbolt_engine::wait::Yield;
use fixbolt_engine::{Application, Config, Engine};

const PRE: usize = 1024;
/// The field-index size, the receive and the transmit buffers, all as
/// `tests/wire.rs` sizes them — this test runs the same messages through the
/// same engine and has no reason to size it differently.
///
/// `[measured 2026-09-01]` the first draft used `N = 8` and every `Logon` was
/// refused in silence, TW44's included: a `Logon` carries nine fields, the
/// index had room for eight, and a parse failure before a `Logon` looks exactly
/// like an identity the acceptor does not serve. That is the shape `CLAUDE.md`
/// §10 warns about — a red that names the wrong cause — and it is why
/// [`the_corpus_counterparty_still_logs_on`] exists below.
const N: usize = 256;
const RX: usize = 4096;
const TX: usize = 8192;
const T0: u64 = FIXED_TIME_MILLIS;

/// The counterparty the acceptance corpus logs on as, and the one it logs on to.
const CORPUS_SENDER: &[u8] = b"TW44";
const US: &[u8] = b"ISLD";

/// A second counterparty. Not in the corpus — no `.def` file names two — so it
/// is the corpus's own `Logon` with `49=` rewritten and `9=`/`10=` recomputed.
/// See [`relabel`], and the round-trip test that proves the rewriting.
const OTHER_SENDER: &[u8] = b"BETA";

type Acceptor = Engine<
    Loopback,
    fixbolt_session::Acceptor,
    InlineDispatch<EchoApp>,
    ManualClock,
    Yield,
    Store,
    N,
    RX,
    TX,
>;

/// The acceptance corpus's own application, as `tests/wire.rs` uses it.
#[derive(Default)]
struct EchoApp(fixbolt_conformance::echo::Echo);

impl Application for EchoApp {
    fn on_message(
        &mut self,
        msg: &[u8],
        seq: u32,
        stamp: &[u8],
        out: &mut [u8],
    ) -> Option<Range<usize>> {
        self.0.reply(msg, seq, stamp, out)
    }
}

// --- the messages ------------------------------------------------------------

/// A real, in-sequence `Logon` from the acceptance corpus: `35=A`, `34=1`,
/// `49=TW44`, `56=ISLD`.
///
/// `[verified 2026-09-01]` twelve distinct `Logon` lines exist across the
/// `fix44` definitions and most are deliberately malformed — a wrong
/// `BeginString`, a `SendingTime` in 2001, `34=5`. This takes the one that is
/// simply correct, so a refusal in these tests is about identity and about
/// nothing else.
fn corpus_logon() -> Vec<u8> {
    load_all()
        .expect("the corpus is fetched — scripts/fetch-quickfix-assets.sh")
        .into_iter()
        .find_map(|s| match s.kind {
            Kind::Send(m)
                if is_logon(&m.wire)
                    && contains_field(&m.wire, b"34=1")
                    && contains_field(&m.wire, b"8=FIX.4.4")
                    && contains_field(&m.wire, b"49=TW44")
                    && contains_field(&m.wire, b"56=ISLD")
                    && contains_field(&m.wire, b"108=30") =>
            {
                Some(m.wire)
            }
            _ => None,
        })
        .expect("the corpus sends a well-formed FIX.4.4 Logon from TW44")
}

/// Is `field` one whole SOH-delimited field of `msg`?
///
/// Fields, not `windows`: a search for the bytes anywhere would match inside a
/// value a counterparty controls, which is the trap `presession::field_value`
/// already has a test for.
fn contains_field(msg: &[u8], field: &[u8]) -> bool {
    msg.split(|b| *b == 1).any(|f| f == field)
}

/// The same `Logon` from a different counterparty: `49=` rewritten, `9=` and
/// `10=` recomputed.
///
/// **Derived from a real message rather than invented** (`CLAUDE.md` §7). The
/// corpus has no file in which two counterparties talk to one acceptor, so the
/// second identity has to come from somewhere; taking the real bytes and
/// changing exactly one field is the smallest step away from them.
///
/// Proven by [`relabelling_to_the_same_sender_reproduces_the_corpus_bytes`] —
/// rewriting `TW44` to `TW44` must give back the corpus bytes exactly,
/// including the `BodyLength` and `CheckSum` the loader computed.
fn relabel(wire: &[u8], sender: &[u8]) -> Vec<u8> {
    let mut head = Vec::new();
    let mut body = Vec::new();
    for field in wire.split(|b| *b == 1).filter(|f| !f.is_empty()) {
        // Both are recomputed below; carrying the old ones through would
        // produce a message whose length says one thing and whose bytes say
        // another.
        if field.starts_with(b"9=") || field.starts_with(b"10=") {
            continue;
        }
        let out = if field.starts_with(b"8=") {
            &mut head
        } else {
            &mut body
        };
        if field.starts_with(b"49=") {
            out.extend_from_slice(b"49=");
            out.extend_from_slice(sender);
        } else {
            out.extend_from_slice(field);
        }
        out.push(1);
    }
    let mut msg = head;
    msg.extend_from_slice(b"9=");
    msg.extend_from_slice(body.len().to_string().as_bytes());
    msg.push(1);
    msg.extend_from_slice(&body);
    let sum = fixbolt_codec::checksum(&msg);
    msg.extend_from_slice(b"10=");
    msg.extend_from_slice(&fixbolt_codec::format_checksum(sum));
    msg.push(1);
    msg
}

// --- the acceptor under test -------------------------------------------------

/// One acceptor, a pre-session stage in front of it, and one client socket per
/// counterparty.
struct Gateway {
    set: PendingSet<Loopback, Table, PRE>,
    /// One engine per configured counterparty, and the `Config` each was built
    /// with — ADR-0026 decision 5: the registry decides *which engine* a
    /// connection belongs to, it does not make one engine multi-identity. That
    /// is what keeps the single-logon rule answerable by counting the
    /// connections one engine holds.
    engines: Vec<(Config, Acceptor)>,
    /// The counterparty ends of the wires, in the order they connected.
    peers: Vec<Loopback>,
    /// Sockets that settled and had nowhere to go. Distinct from a session
    /// refusing a `Logon`: this is the stage in front having no configuration
    /// for the identity it just read.
    unrouted: usize,
}

/// Build an acceptor that serves `configs`.
///
/// **This is the only thing step 2 changed**, and it is where the redness was.
/// Before [ADR-0026] it read `vec![Engine::new(configs[0], …)]` and dropped
/// everything after the first: an acceptor carried one `Config`, so it could be
/// *told about* two counterparties and only *hold* one. The two specification
/// tests below did not move.
///
/// Now the configurations go into a `presession::Table`, the pre-session stage
/// looks the identity up before a session exists, and there is one engine per
/// entry — ADR-0026 decision 5.
///
/// [ADR-0026]: ../../../docs/decisions/ADR-0026-a-counterparty-registry-in-the-pre-session-stage.md
fn gateway(configs: &[Config]) -> Gateway {
    let mut table = Table::with_capacity(configs.len());
    let mut engines = Vec::with_capacity(configs.len());
    for cfg in configs {
        table = table.serving(*cfg);
        engines.push((
            *cfg,
            Engine::new(
                *cfg,
                InlineDispatch::new(EchoApp::default()),
                ManualClock::at(T0),
                Yield,
                4,
            ),
        ));
    }
    Gateway {
        set: PendingSet::new(Limits::new(8, 30_000).expect("both above zero"), table),
        engines,
        peers: Vec::new(),
        unrouted: 0,
    }
}

impl Gateway {
    /// Open a connection and send `first` on it. Returns the peer's index.
    fn connect(&mut self, first: &[u8]) -> usize {
        let (near, mut far) = Loopback::pair();
        assert!(
            self.set.admit(near, T0).is_ok(),
            "the pending table has room"
        );
        assert!(
            matches!(far.send(first), Io::Ready(n) if n == first.len()),
            "the whole message went onto the wire"
        );
        self.peers.push(far);
        self.peers.len() - 1
    }

    /// Let the pre-session stage settle every socket, hand each on, and turn
    /// every engine until nothing moves.
    ///
    /// Bounded rather than `loop`: a run that cannot settle is a failing test,
    /// not a hanging one. Deterministic throughout — nothing sleeps, and the
    /// clock is the corpus's fixed instant.
    fn settle(&mut self) {
        for _ in 0..64 {
            self.set.turn(T0);
            while let Some(i) = self.set.settled() {
                let Some(p) = self.set.take(i) else { break };
                self.hand(p);
            }
            let mut moved = false;
            for (_, e) in &mut self.engines {
                moved |= e.turn();
            }
            if !moved && self.set.is_empty() {
                // One more pass: an engine may have queued a reply the peer has
                // not been given a chance to read.
                for (_, e) in &mut self.engines {
                    e.turn();
                }
                return;
            }
        }
    }

    /// Give one settled socket to the engine that serves its identity.
    ///
    /// The pre-session stage has already asked the registry, so the `Config` is
    /// on the `Pending` — this only has to find the engine holding it. A socket
    /// whose identity the registry refused never reaches here: `PendingSet::turn`
    /// let go of it and counted it in `Progress::unknown`.
    fn hand(&mut self, p: fixbolt_engine::presession::Pending<Loopback, PRE>) {
        let Some(cfg) = p.config() else {
            self.unrouted += 1;
            return;
        };
        let Some((_, engine)) = self.engines.iter_mut().find(|(c, _)| *c == cfg) else {
            self.unrouted += 1;
            return;
        };
        let (t, buf, len) = p.into_parts();
        if engine.add_with_prefix(t, &buf[..len]).is_err() {
            self.unrouted += 1;
        }
    }

    /// Everything the acceptor has said to peer `i` so far.
    fn reply(&mut self, i: usize) -> Vec<u8> {
        let mut out = Vec::new();
        let mut buf = [0u8; 4096];
        while let Some(peer) = self.peers.get_mut(i) {
            match peer.recv(&mut buf) {
                Io::Ready(n) => out.extend_from_slice(&buf[..n]),
                Io::Idle | Io::Closed | Io::Failed(_) => break,
            }
        }
        out
    }
}

/// The value of the first whole field tagged `tag` (which includes the `=`).
fn field<'a>(msg: &'a [u8], tag: &[u8]) -> Option<&'a [u8]> {
    msg.split(|b| *b == 1).find_map(|f| f.strip_prefix(tag))
}

/// Assert that `reply` is a `Logon` addressed from `us` to `them`.
///
/// Reports what actually came back rather than `assert!(…)`, because the two
/// failures this test can have look nothing alike: **no bytes at all** is a
/// counterparty the acceptor has no configuration for, and a `Logout` with a
/// `58=` is a session that answered and objected.
#[track_caller]
fn assert_logon_back(reply: &[u8], us: &[u8], them: &[u8], who: &str) {
    assert!(
        !reply.is_empty(),
        "{who} was refused in silence: the acceptor sent it nothing.\n\
         An acceptor holds one `Config` and therefore one `target_comp_id`, so \
         it can serve one counterparty. This is what ADR-0026's registry is for.",
    );
    let got_type = field(reply, b"35=").map(<[u8]>::to_vec);
    assert_eq!(
        got_type.as_deref(),
        Some(&b"A"[..]),
        "{who} got {} back, not a Logon:\n  {}",
        got_type
            .as_deref()
            .map_or("nothing".to_owned(), |t| String::from_utf8_lossy(t)
                .into_owned()),
        String::from_utf8_lossy(reply).replace('\u{1}', "|"),
    );
    assert_eq!(
        field(reply, b"49="),
        Some(us),
        "{who}'s Logon came back from the wrong SenderCompID:\n  {}",
        String::from_utf8_lossy(reply).replace('\u{1}', "|"),
    );
    assert_eq!(
        field(reply, b"56="),
        Some(them),
        "{who}'s Logon came back addressed to somebody else:\n  {}",
        String::from_utf8_lossy(reply).replace('\u{1}', "|"),
    );
}

// --- the specification -------------------------------------------------------

/// The helper that makes a second counterparty is proven, not assumed.
///
/// Rewriting `TW44`'s `Logon` to `TW44` must give back the corpus bytes, byte
/// for byte — which exercises the `BodyLength` and `CheckSum` arithmetic
/// against numbers the corpus loader computed independently. Without this, a
/// refusal in the tests below could be a malformed message rather than an
/// unserved identity, and they would be red for the wrong reason.
#[test]
fn relabelling_to_the_same_sender_reproduces_the_corpus_bytes() {
    let logon = corpus_logon();
    assert_eq!(
        relabel(&logon, CORPUS_SENDER),
        logon,
        "the relabeller is not byte-exact, so nothing built with it can be trusted",
    );
}

/// The control, and it is green today.
///
/// The counterparty this acceptor **is** configured for logs on and gets its
/// `Logon` back. Without this, every red below could be a harness that cannot
/// serve anybody — and `[measured 2026-09-01]` that is exactly what the first
/// draft was: `N = 8` refused TW44 too, and the failure message still blamed
/// the missing registry. A red that names the wrong cause is worse than no
/// test, so the acceptor's one working case is asserted alongside the two it
/// cannot do yet.
#[test]
fn the_corpus_counterparty_still_logs_on() {
    let logon = corpus_logon();
    let mut gw = gateway(&[Config::acceptor(b"FIX.4.4", US, CORPUS_SENDER)]);

    let only = gw.connect(&relabel(&logon, CORPUS_SENDER));
    gw.settle();

    assert_eq!(gw.unrouted, 0, "the settled socket reached an engine");
    assert_logon_back(&gw.reply(only), US, CORPUS_SENDER, "TW44");
}

/// **The specification.** One acceptor, two counterparties, and each sees its
/// own comp IDs come back.
///
/// This is what separates an acceptor from a point-to-point link, and it is the
/// largest gap in `PRD.md` §3.
#[test]
fn two_counterparties_log_on_to_one_acceptor() {
    let logon = corpus_logon();
    let mut gw = gateway(&[
        Config::acceptor(b"FIX.4.4", US, CORPUS_SENDER),
        Config::acceptor(b"FIX.4.4", US, OTHER_SENDER),
    ]);

    let first = gw.connect(&relabel(&logon, CORPUS_SENDER));
    let second = gw.connect(&relabel(&logon, OTHER_SENDER));
    gw.settle();

    assert_eq!(gw.unrouted, 0, "every settled socket reached an engine");
    let (a, b) = (gw.reply(first), gw.reply(second));
    assert_logon_back(&a, US, CORPUS_SENDER, "TW44");
    assert_logon_back(&b, US, OTHER_SENDER, "BETA");
}

/// The same refusal with the single-logon rule taken out of the picture.
///
/// `Engine::turn` enforces *one identity, one connection* by counting the
/// connections it holds, so in the test above `BETA` arrives at an engine that
/// already has `TW44` on it and two different rules could be doing the
/// refusing. Here `BETA` connects **alone**, to an acceptor configured for both
/// — and is still refused, because `configs[1]` has nowhere to live.
///
/// Same fix, and it is the one assertion that names the cause exactly.
#[test]
fn the_second_configured_counterparty_is_served_when_it_connects_alone() {
    let logon = corpus_logon();
    let mut gw = gateway(&[
        Config::acceptor(b"FIX.4.4", US, CORPUS_SENDER),
        Config::acceptor(b"FIX.4.4", US, OTHER_SENDER),
    ]);

    let only = gw.connect(&relabel(&logon, OTHER_SENDER));
    gw.settle();

    assert_eq!(gw.unrouted, 0, "the settled socket reached an engine");
    assert_logon_back(&gw.reply(only), US, OTHER_SENDER, "BETA");
}

// --- the registry itself -----------------------------------------------------

/// A set holding one socket that has already sent `first`, in front of
/// `registry`.
fn one_socket<R: Registry>(registry: R, first: &[u8]) -> (PendingSet<Loopback, R, PRE>, Loopback) {
    let mut set = PendingSet::new(Limits::new(4, 30_000).expect("both above zero"), registry);
    let (near, mut far) = Loopback::pair();
    assert!(set.admit(near, T0).is_ok(), "the ceiling is four");
    assert!(
        matches!(far.send(first), Io::Ready(_)),
        "the message is on the wire"
    );
    (set, far)
}

/// Did the acceptor say anything back?
fn said_anything(peer: &mut Loopback) -> bool {
    let mut buf = [0u8; 64];
    matches!(peer.recv(&mut buf), Io::Ready(_))
}

/// **An acceptor that admits an identity nobody configured is an open port.**
///
/// [ADR-0026] decision 6 gives the registry no default, and this is the test
/// that says what "no default" costs: a `Table::new()` refuses every
/// connection there will ever be. If somebody ever makes an empty registry mean
/// *accept anything* — QuickFIX/J's wildcard `ANY_SESSION` template, which this
/// design deliberately does not offer — this goes red.
///
/// [ADR-0026]: ../../../docs/decisions/ADR-0026-a-counterparty-registry-in-the-pre-session-stage.md
#[test]
fn an_empty_registry_refuses_every_connection() {
    let logon = corpus_logon();
    let (mut set, mut peer) = one_socket(Table::new(), &relabel(&logon, CORPUS_SENDER));

    let p = set.turn(T0);

    assert_eq!(p.settled, 0, "an empty registry settles nobody");
    assert_eq!(p.unknown, 1, "and counts the refusal: {p:?}");
    assert!(set.is_empty(), "the socket was let go of, not held");
    assert!(
        !said_anything(&mut peer),
        "a refused counterparty is told nothing — ADR-0026 decision 3 refuses it \
         exactly as an invalid identity is refused, and the corpus expects silence"
    );
}

/// A counterparty the registry does not serve never reaches a session.
///
/// The refusal moves one stage earlier than it used to: before ADR-0026 the
/// pre-session stage let every identity through and `Session` refused the wrong
/// ones with `Refusal::WrongSenderCompId`. Now the stage answers first, and the
/// socket is gone before an engine has seen it. **The observable is unchanged**
/// — silence, then a disconnect — which is why `tests/wire.rs` still scores 59.
#[test]
fn an_identity_nobody_configured_is_refused_before_a_session_exists() {
    let logon = corpus_logon();
    let table = Table::new().serving(Config::acceptor(b"FIX.4.4", US, CORPUS_SENDER));
    let (mut set, mut peer) = one_socket(table, &relabel(&logon, OTHER_SENDER));

    let p = set.turn(T0);

    assert_eq!(p.settled, 0, "BETA is not served by this table");
    assert_eq!(p.unknown, 1, "and the refusal is counted: {p:?}");
    assert_eq!(
        p.not_logon, 0,
        "it was a perfectly good Logon — a different fault"
    );
    assert!(!said_anything(&mut peer), "refused in silence");
}

/// The control for the two above, and the reason both can be trusted.
///
/// Same table, same harness, the identity it **does** serve: the socket settles
/// and carries the configuration the registry chose. Without this, a
/// `PendingSet` that had stopped settling anything at all would make both
/// refusal tests pass — the lesson written up in
/// `docs/reference/silence-before-a-logon-has-many-causes.md`.
#[test]
fn a_configured_identity_settles_and_carries_its_configuration() {
    let logon = corpus_logon();
    let cfg = Config::acceptor(b"FIX.4.4", US, CORPUS_SENDER);
    let (mut set, _peer) = one_socket(Table::new().serving(cfg), &relabel(&logon, CORPUS_SENDER));

    let p = set.turn(T0);

    assert_eq!(p.settled, 1, "TW44 is served: {p:?}");
    assert_eq!(p.unknown, 0, "and nothing was refused");
    let i = set.settled().expect("a settled socket");
    assert_eq!(
        set.identity_at(i)
            .map(|id: Identity<'_>| (id.sender.to_vec(), id.target.to_vec())),
        Some((CORPUS_SENDER.to_vec(), US.to_vec())),
        "the identity the stage read"
    );
    assert_eq!(
        set.take(i).and_then(|p| p.config()),
        Some(cfg),
        "and the configuration the registry chose for it"
    );
}

/// `Table` keys on the entry's own `Config`, so two counterparties are two
/// entries and neither can answer for the other.
#[test]
fn a_table_serves_each_entry_and_nobody_else() {
    let alpha = Config::acceptor(b"FIX.4.4", US, CORPUS_SENDER);
    let beta = Config::acceptor(b"FIX.4.4", US, OTHER_SENDER);
    let table = Table::new().serving(alpha).serving(beta);
    assert_eq!(table.len(), 2);
    assert!(!table.is_empty());

    let served = |sender: &[u8], target: &[u8]| {
        table
            .lookup(Identity { sender, target })
            .map(fixbolt_engine::presession::Entry::config)
    };

    assert_eq!(served(CORPUS_SENDER, US), Some(alpha));
    assert_eq!(served(OTHER_SENDER, US), Some(beta));
    // Reversed comp IDs are a real corpus case — `2k_CompIDDoesNotMatchProfile`.
    assert_eq!(served(US, CORPUS_SENDER), None, "reversed is not a match");
    assert_eq!(served(b"NOBODY", US), None, "and a stranger is nobody");
}
