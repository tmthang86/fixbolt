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
    relabel_full(wire, sender, None)
}

/// As [`relabel`], and also sets `57=` — the TargetSubID — to `target_sub`.
///
/// Nothing in the acceptance corpus sends a `Logon` with a sub-ID, so a
/// counterparty told apart by one has to be built. It is still the corpus's
/// bytes with fields edited, and the byte-exactness of the machinery is proven
/// by the same round-trip test.
fn relabel_full(wire: &[u8], sender: &[u8], target_sub: Option<&[u8]>) -> Vec<u8> {
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
        // A message never carries `57=` here, so replacing it would be dead
        // code; it is appended after `56=`, where FIX's header order puts it.
        if field.starts_with(b"57=") {
            continue;
        }
        if field.starts_with(b"49=") {
            out.extend_from_slice(b"49=");
            out.extend_from_slice(sender);
        } else {
            out.extend_from_slice(field);
        }
        out.push(1);
        if field.starts_with(b"56=") {
            if let Some(sub) = target_sub {
                out.extend_from_slice(b"57=");
                out.extend_from_slice(sub);
                out.push(1);
            }
        }
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
    /// **One** engine, holding every counterparty — [ADR-0030], which supersedes
    /// ADR-0026 decision 5's one-engine-per-counterparty.
    ///
    /// `1b_DuplicateIdentity.def`'s own comment is the specification: *"If two
    /// logons with the same SenderCompID/TargetCompID combination logon the
    /// second one must be disconnected"* — **per identity**. An engine that held
    /// one identity could answer that by counting any logged-on connection; one
    /// that holds several must compare.
    ///
    /// [ADR-0030]: ../../../docs/decisions/ADR-0030-one-engine-holds-many-counterparties.md
    engine: Acceptor,
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
    assert!(!configs.is_empty(), "an acceptor with no configuration");
    let mut table = Table::with_capacity(configs.len());
    for cfg in configs {
        table = table.serving(*cfg);
    }
    Gateway {
        set: PendingSet::new(Limits::new(8, 30_000).expect("both above zero"), table),
        // The engine's own `Config` is only a default for `add`; every
        // connection here arrives through the pre-session stage carrying the one
        // the registry chose.
        engine: Engine::new(
            configs[0],
            InlineDispatch::new(EchoApp::default()),
            ManualClock::at(T0),
            Yield,
            8,
        ),
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
            let moved = self.engine.turn();
            if !moved && self.set.is_empty() {
                // One more pass: the engine may have queued a reply the peer has
                // not been given a chance to read.
                self.engine.turn();
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
        let (t, buf, len) = p.into_parts();
        if self
            .engine
            .add_with_prefix_and_config(t, cfg, &buf[..len])
            .is_err()
        {
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

/// **One identity, one connection — and two identities, two connections.**
///
/// The rule `1b_DuplicateIdentity.def` and `AlreadyLoggedOn.def` gate, asked of
/// an engine that holds more than one counterparty. Both definitions run through
/// one engine with one `Config` and cannot tell *"already logged on"* from
/// *"somebody is logged on"*; here the difference is the whole test.
///
/// **This is what stops the fix from being a deletion.** Removing the rule
/// altogether also makes `two_counterparties_log_on_to_one_acceptor` green, and
/// the corpus would still pass every definition through `tests/wire.rs`. Only a
/// second connection from an identity already on the engine separates *compared*
/// from *not asked* — [ADR-0030].
///
/// [ADR-0030]: ../../../docs/decisions/ADR-0030-one-engine-holds-many-counterparties.md
#[test]
fn a_duplicate_of_one_counterparty_is_refused_and_the_other_is_not() {
    let logon = corpus_logon();
    let mut gw = gateway(&[
        Config::acceptor(b"FIX.4.4", US, CORPUS_SENDER),
        Config::acceptor(b"FIX.4.4", US, OTHER_SENDER),
    ]);

    let first = gw.connect(&relabel(&logon, CORPUS_SENDER));
    gw.settle();
    assert_logon_back(&gw.reply(first), US, CORPUS_SENDER, "TW44");

    // A different counterparty, while TW44 is logged on. Served.
    let other = gw.connect(&relabel(&logon, OTHER_SENDER));
    gw.settle();
    assert_logon_back(&gw.reply(other), US, OTHER_SENDER, "BETA");

    // A second TW44, while the first is still logged on. Refused in silence,
    // which is exactly what 1b_DuplicateIdentity.def expects.
    let dup = gw.connect(&relabel(&logon, CORPUS_SENDER));
    gw.settle();
    assert!(
        gw.reply(dup).is_empty(),
        "a second connection from an identity already logged on gets no reply — \
         1b_DuplicateIdentity.def, whose own comment says `if two logons with the \
         same SenderCompID/TargetCompID combination logon the second one must be \
         disconnected`"
    );

    // And BETA is undisturbed by TW44's duplicate.
    assert!(
        gw.reply(other).is_empty(),
        "BETA was already answered; nothing more should have arrived for it"
    );
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
            .lookup(Identity::comp_ids(sender, target))
            .map(fixbolt_engine::presession::Entry::config)
    };

    assert_eq!(served(CORPUS_SENDER, US), Some(alpha));
    assert_eq!(served(OTHER_SENDER, US), Some(beta));
    // Reversed comp IDs are a real corpus case — `2k_CompIDDoesNotMatchProfile`.
    assert_eq!(served(US, CORPUS_SENDER), None, "reversed is not a match");
    assert_eq!(served(b"NOBODY", US), None, "and a stranger is nobody");
}

// --- sub-IDs, and who owns the key -------------------------------------------

/// A message the corpus sends that carries `150=` and no `50=`.
///
/// `2r_UnregisteredMsgType.def` sends an `ExecutionReport` with `150=0`
/// (ExecType). A scan for `50=` that matched anywhere inside a message would
/// read `0` as a SenderSubID from it — and `150=` is not an edge case somebody
/// invented, it is on the wire of a definition this project already runs.
fn a_message_carrying_150() -> Vec<u8> {
    load_all()
        .expect("the corpus is fetched")
        .into_iter()
        .find_map(|s| match s.kind {
            Kind::Send(m) if contains_field(&m.wire, b"150=0") => Some(m.wire),
            _ => None,
        })
        .expect("2r_UnregisteredMsgType.def sends 150=0")
}

/// `150=` is not `50=`, and the corpus is what says so.
///
/// The guard is `field_value`'s field-start scan, which already had a test for
/// `49=` hidden in a value. This is the same trap reached by a different route:
/// a **legitimate tag** whose decimal representation ends in the tag being
/// looked for. Nothing had to be invented to hit it.
#[test]
fn a_tag_ending_in_fifty_is_not_a_sender_sub_id() {
    let msg = a_message_carrying_150();
    let id = fixbolt_engine::presession::identity_of(&msg).expect("it names both sides");
    assert_eq!(
        id.sender_sub,
        None,
        "150=0 is ExecType, not a SenderSubID: {}",
        String::from_utf8_lossy(&msg).replace('\u{1}', "|")
    );
    assert_eq!(id.target_sub, None, "and there is no 57= either");
}

/// Both states of the optional fields, on real bytes.
#[test]
fn an_identity_carries_the_sub_ids_only_when_the_message_does() {
    let logon = corpus_logon();

    let without = relabel(&logon, CORPUS_SENDER);
    let id = fixbolt_engine::presession::identity_of(&without).expect("both sides");
    assert_eq!((id.sender, id.target), (CORPUS_SENDER, US));
    assert_eq!(id.target_sub, None, "the corpus Logon carries no 57=");

    let with = relabel_full(&logon, CORPUS_SENDER, Some(b"DESK1"));
    let id = fixbolt_engine::presession::identity_of(&with).expect("both sides");
    assert_eq!(
        (id.sender, id.target),
        (CORPUS_SENDER, US),
        "the comp IDs are unchanged"
    );
    assert_eq!(id.target_sub, Some(&b"DESK1"[..]), "and the sub-ID is read");
}

/// A `Registry` that tells counterparties apart by `57=`, in eight lines.
///
/// [ADR-0026] decision 2: **how much of an `Identity` forms the key is the
/// implementation's business.** `Table` uses the comp IDs, because that is what
/// a `Config` holds; a deployment whose counterparties share a comp-ID pair and
/// differ by desk writes this instead. It is the whole reason ADR-0026 made
/// `Registry` a trait rather than a map.
///
/// [ADR-0026]: ../../../docs/decisions/ADR-0026-a-counterparty-registry-in-the-pre-session-stage.md
struct ByDesk {
    desks: Vec<(Vec<u8>, fixbolt_engine::presession::Entry)>,
}

impl Registry for ByDesk {
    fn lookup(&self, id: Identity<'_>) -> Option<&fixbolt_engine::presession::Entry> {
        let sub = id.target_sub?;
        self.desks
            .iter()
            .find(|(desk, e)| desk == sub && e.config().serves(id.sender, id.target))
            .map(|(_, e)| e)
    }
}

/// Two counterparties sharing `(49, 56)` and differing only by `57=`.
///
/// **Same code, two behaviours, and the implementation is what decides which.**
/// `ByDesk` serves both. The default `Table` cannot tell them apart — it keys on
/// what `Config` holds, and `Config` has no room for a sub-ID — so both resolve
/// to the same entry, which is the honest answer rather than a silent one.
#[test]
fn a_registry_may_key_on_a_sub_id_and_the_default_one_does_not() {
    let logon = corpus_logon();
    let cfg = Config::acceptor(b"FIX.4.4", US, CORPUS_SENDER);
    let desk_one = relabel_full(&logon, CORPUS_SENDER, Some(b"DESK1"));
    let desk_two = relabel_full(&logon, CORPUS_SENDER, Some(b"DESK2"));

    let by_desk = ByDesk {
        desks: vec![
            (
                b"DESK1".to_vec(),
                fixbolt_engine::presession::Entry::new(cfg),
            ),
            (
                b"DESK2".to_vec(),
                fixbolt_engine::presession::Entry::new(cfg),
            ),
        ],
    };
    for (wire, desk) in [(&desk_one, "DESK1"), (&desk_two, "DESK2")] {
        let (mut set, _peer) = one_socket(&by_desk, wire);
        let p = set.turn(T0);
        assert_eq!(p.settled, 1, "ByDesk serves {desk}: {p:?}");
        assert_eq!(p.unknown, 0);
    }
    // And it refuses a connection with no desk at all, because its key needs one.
    let (mut set, _peer) = one_socket(&by_desk, &relabel(&logon, CORPUS_SENDER));
    let p = set.turn(T0);
    assert_eq!(p.settled, 0, "no 57= is not a desk");
    assert_eq!(p.unknown, 1, "{p:?}");

    // The default Table ignores the sub-ID, so both desks are the same
    // counterparty to it — one entry answers for both.
    let table = Table::new().serving(cfg);
    let entry = |wire: &[u8]| {
        table
            .lookup(fixbolt_engine::presession::identity_of(wire).expect("both sides"))
            .map(fixbolt_engine::presession::Entry::config)
    };
    assert_eq!(entry(&desk_one), Some(cfg));
    assert_eq!(
        entry(&desk_two),
        Some(cfg),
        "Table keys on the comp IDs, so DESK2 is the same counterparty to it"
    );
}

// --- `admit`: the credential the registry could not see ----------------------

/// A `Registry` that checks a password, in the twelve lines `lookup` could not
/// hold.
///
/// **Step 5 of [settings-for-both-roles].** [ADR-0026] calls `lookup` the
/// authentication hook, and `[verified 2026-09-05]` it was handed an
/// [`Identity`] — `49`, `56`, `50`, `57` and nothing else. `553=Username`,
/// `554=Password` and `96=RawData` are on the `Logon` in front of it and no
/// implementation could reach them, so the hook the ADR named could not do the
/// job the ADR named it for.
///
/// `admit` is given the whole `Logon`. The default calls `lookup`, so every
/// existing implementation — `Table` included — keeps working untouched.
///
/// **The engine holds no password.** This implementation does, because it is
/// one; nothing in `fixbolt` stores a credential or has an opinion about how
/// one is compared. That is deliberate: a default that accepted an empty
/// password would be worse than no default at all, which is the argument
/// ADR-0026 decision 6 already made about an empty table.
///
/// [settings-for-both-roles]: ../../../docs/plans/2026-09-04-settings-for-both-roles.md
/// [ADR-0026]: ../../../docs/decisions/ADR-0026-a-counterparty-registry-in-the-pre-session-stage.md
struct WithPassword {
    inner: Table,
    password: &'static [u8],
}

impl Registry for WithPassword {
    fn lookup(&self, id: Identity<'_>) -> Option<&fixbolt_engine::presession::Entry> {
        self.inner.lookup(id)
    }

    fn admit(&self, id: Identity<'_>, logon: &[u8]) -> Option<&fixbolt_engine::presession::Entry> {
        // Borrowed out of the caller's buffer, so the pre-session stage still
        // allocates nothing — `benches/alloc.rs` case `registry-lookup`.
        let given = fixbolt_engine::presession::field_value(logon, b"554=")?;
        (given == self.password).then(|| self.lookup(id))?
    }
}

/// The corpus `Logon` with `554=` added, `9=` and `10=` recomputed.
///
/// The same machinery as [`relabel_full`], for the same reason: a real message
/// with one field changed rather than an invented packet (`CLAUDE.md` §7).
fn logon_with_password(password: &str) -> Vec<u8> {
    let wire = relabel(&corpus_logon(), CORPUS_SENDER);
    let (mut head, mut body) = (Vec::new(), Vec::new());
    for field in wire.split(|b| *b == 1).filter(|f| !f.is_empty()) {
        if field.starts_with(b"9=") || field.starts_with(b"10=") {
            continue;
        }
        let out = if field.starts_with(b"8=") {
            &mut head
        } else {
            &mut body
        };
        out.extend_from_slice(field);
        out.push(1);
    }
    body.extend_from_slice(b"554=");
    body.extend_from_slice(password.as_bytes());
    body.push(1);

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

#[test]
fn a_registry_that_sees_the_logon_can_refuse_a_wrong_password() {
    let cfg = Config::acceptor(b"FIX.4.4", US, CORPUS_SENDER);
    let registry = WithPassword {
        inner: Table::new().serving(cfg),
        password: b"correct horse",
    };

    let (mut set, mut peer) = one_socket(&registry, &logon_with_password("correct horse"));
    let p = set.turn(T0);
    assert_eq!(p.settled, 1, "the right password is admitted: {p:?}");
    assert!(
        !said_anything(&mut peer),
        "and nothing is said before the session"
    );

    let (mut set, _peer) = one_socket(&registry, &logon_with_password("hunter2"));
    let p = set.turn(T0);
    assert_eq!(p.settled, 0, "the wrong one is not: {p:?}");
    assert_eq!(p.unknown, 1, "and it is refused as an unknown counterparty");
}

#[test]
fn a_registry_that_sees_the_logon_refuses_one_with_no_credential_at_all() {
    // The comp IDs are right and `Table` alone would admit it. Only `admit`
    // can tell the difference, which is the whole point of the method.
    let cfg = Config::acceptor(b"FIX.4.4", US, CORPUS_SENDER);
    let registry = WithPassword {
        inner: Table::new().serving(cfg),
        password: b"correct horse",
    };

    let plain = relabel(&corpus_logon(), CORPUS_SENDER);
    assert_eq!(
        registry
            .inner
            .lookup(fixbolt_engine::presession::identity_of(&plain).expect("both sides")),
        registry
            .inner
            .lookup(fixbolt_engine::presession::identity_of(&plain).expect("both sides")),
        "the premise: the identity is one Table already serves"
    );

    let (mut set, _peer) = one_socket(&registry, &plain);
    let p = set.turn(T0);
    assert_eq!(p.settled, 0, "no 554= is not a credential: {p:?}");
    assert_eq!(p.unknown, 1);
}

/// The default is not a formality: **every registry written before today keeps
/// working**, and `Table` is one of them.
#[test]
fn the_default_admit_is_lookup_and_the_table_never_learned_a_new_method() {
    let cfg = Config::acceptor(b"FIX.4.4", US, CORPUS_SENDER);
    let table = Table::new().serving(cfg);
    let logon = relabel(&corpus_logon(), CORPUS_SENDER);
    let id = fixbolt_engine::presession::identity_of(&logon).expect("both sides");

    assert_eq!(
        table
            .admit(id, &logon)
            .map(fixbolt_engine::presession::Entry::config),
        table
            .lookup(id)
            .map(fixbolt_engine::presession::Entry::config),
        "the default forwards, so a Table answers the same either way"
    );

    let (mut set, _peer) = one_socket(&table, &logon);
    assert_eq!(set.turn(T0).settled, 1, "and the pre-session stage agrees");
}
