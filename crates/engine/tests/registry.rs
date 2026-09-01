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
use fixbolt_engine::presession::{Limits, PendingSet, is_logon};
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
    set: PendingSet<Loopback, PRE>,
    engines: Vec<Acceptor>,
    /// The counterparty ends of the wires, in the order they connected.
    peers: Vec<Loopback>,
    /// Sockets that settled and had nowhere to go. Distinct from a session
    /// refusing a `Logon`: this is the stage in front having no configuration
    /// for the identity it just read.
    unrouted: usize,
}

/// Build an acceptor that serves `configs`.
///
/// **This function is the whole of step 1's redness, and the whole of what step
/// 2 replaces.** Today an acceptor carries one `Config`: `serve` and
/// `serve_hft` each take one (`crates/engine/src/lib.rs:572`, `:601`), and
/// `serve_sharded_hft` takes one and hands the *same* one to every shard
/// (`crates/engine/src/shard.rs:410`, `:431`). There is nowhere to put
/// `configs[1]`, so it is dropped here — visibly, rather than by a signature
/// that never let the caller mention it.
///
/// After [ADR-0026] this builds a `presession::Table` from every entry and the
/// pre-session stage looks the identity up. The two `#[test]` functions below
/// do not change.
///
/// [ADR-0026]: ../../../docs/decisions/ADR-0026-a-counterparty-registry-in-the-pre-session-stage.md
fn gateway(configs: &[Config]) -> Gateway {
    assert!(!configs.is_empty(), "an acceptor with no configuration");
    let engines = vec![Engine::new(
        // ── the line that cannot hold a second counterparty ──────────────────
        configs[0],
        InlineDispatch::new(EchoApp::default()),
        ManualClock::at(T0),
        Yield,
        4,
    )];
    Gateway {
        set: PendingSet::new(Limits::new(8, 30_000).expect("both above zero")),
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
            for e in &mut self.engines {
                moved |= e.turn();
            }
            if !moved && self.set.is_empty() {
                // One more pass: an engine may have queued a reply the peer has
                // not been given a chance to read.
                for e in &mut self.engines {
                    e.turn();
                }
                return;
            }
        }
    }

    /// Give one settled socket to the engine that serves its identity.
    ///
    /// Today there is one engine and it serves one identity, so every
    /// connection goes to it and the ones it does not serve are refused by the
    /// session — silently, before a reply, which is exactly what
    /// `1c_InvalidLogonBadSenderCompID.def` expects and exactly what makes a
    /// second counterparty indistinguishable from an impostor.
    fn hand(&mut self, p: fixbolt_engine::presession::Pending<Loopback, PRE>) {
        let Some(engine) = self.engines.first_mut() else {
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
