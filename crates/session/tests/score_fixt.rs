//! The FIXT 1.1 score: **179 / 180**, three corpora of sixty, with the 180th
//! **asserted** as a known, accepted divergence rather than reached.
//!
//! `CLAUDE.md` §2 non-negotiable 3 names the 59 FIX 4.4 definitions as the
//! session layer's gate; `tests/score.rs` is that gate and nothing here touches
//! it. This is the second one ADR-0080 decision 2 asks for, and it is a
//! different kind of evidence: the 59 prove the state machine still does what
//! it did, these 180 prove the **same** state machine does it through a
//! second `E::Dict`, built from two XML files, with the FIXT rules of
//! [ADR-0084] on top — decision 1 (a session message is checked against the
//! tag set of the layer that defines it) and decision 2 (a group member's
//! value waits for the count) are built, and every file that turns on one of
//! them passes.
//!
//! # Three corpora, one table
//!
//! `[measured 2026-09-19]`, masked properly (`SenderCompID`, `1137`,
//! `BodyLength`, SOH matched as a byte, not `.`): `fix50sp1` is byte-equal to
//! `fix50sp2` on **60 / 60** files, so the one generated `fix50sp2` table is
//! their oracle by measurement. `fix50` differs on exactly one file — see
//! below.
//!
//! # The file the 59 do not have
//!
//! `1d_InvalidLogonNoDefaultApplVerID.def` — a `Logon` carrying `98=0 108=30`
//! and no `1137`, answered by `eDISCONNECT` with no `E` line in front of it.
//! It is the entire oracle for [`fixbolt_session::DropReason`]'s
//! `LogonWithoutDefaultApplVerId`, and it is the one file that fails if the
//! check is removed.
//!
//! # `336`, and the one accepted divergence — [ADR-0084] decision 3
//!
//! `TradingSessionID(336)` carries **0** enumerated values in `FIX50.xml` and
//! **7** in `FIX50SP2.xml`. QuickFIX ran each of the three corpora against
//! its own application dictionary — `FIX50.xml` for `fix50`, `FIX50SP2.xml`
//! for the other two — so `21_RepeatingGroupSpecifierWithValueOfZero.def`'s
//! `336=ONE_MAIN` on a `35=d` is echoed there and is not here: scoring
//! `fix50` on the one generated SP2 table gives it no way to be lenient about
//! a value only `FIX50.xml` allows, so `scan_fields` answers
//! `Reject 373=5 371=336` — *Value is incorrect (out of range) for this tag*
//! — where `FIX50.xml`'s own oracle expects the message processed.
//!
//! The successor that reaches 180 / 180 **by table**, not by exception, is a
//! generated `fixt11_fix50.rs`, named in decision 3 and not yet built.
//! **The day it lands, this divergence assertion is deleted** and `fix50`
//! runs on its own table.
//!
//! **This assertion pins the strict direction only.** Decision 3's *"What
//! bidirectional drift does to this decision"* paragraph and its
//! *Bad — and accepted* bullet "The divergence assertion sees one direction"
//! say why: enumerations drift both ways between FIX50, SP1 and SP2, and a
//! file that *passes* is never examined here, so a value the SP2 table
//! wrongly *accepts* — the permissive direction, e.g.
//! `DeskOrderHandlingInst(1035)`, 24 values in `FIX50.xml` and 0 in
//! `FIX50SP2.xml` — is invisible to this test by construction. No `.def` in
//! these 180 files carries such a value, so `crates/dict/tests/fixt.rs` pins
//! that direction directly on the table, which is the only guard on that
//! side until the successor exists.
//!
//! **Nothing here was relaxed to make a number**: 179 is the honest score,
//! recorded by counting and pinned by content — not lowered by editing a
//! fixture or excluding a file (`CLAUDE.md` §10).
//!
//! [ADR-0084]: ../../../docs/decisions/ADR-0084-a-session-message-is-checked-against-the-layer-that-defines-it-a-members-value-waits-for-the-count-and-fix50-is-its-own-oracle.md
#![cfg(feature = "fix50sp2")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
// Not a library crate's source: non-negotiable 7 is about `crates/*/src`, and
// `scripts/check-indexing-debt.sh` counts nothing outside it. An index that
// panics in a test is a failing test, which is what a test is for.
#![allow(clippy::indexing_slicing)]

use fixbolt_codec::TagValue;
use fixbolt_conformance::runner::{Conn, Failure, Input, Link, SessionUnderTest, run_scenario};
use fixbolt_conformance::script::{Corpus, FIXED_TIME_MILLIS, Kind, fixt_corpora, load_corpus};
use fixbolt_dict::Fixt11Fix50Sp2Tables;
use fixbolt_engine::frame::{Cut, Framer};
use fixbolt_engine::journal::Store;
use fixbolt_session::{Acceptor, Config, Session};

/// The encoding the 180 run against: the FIXT 1.1 / FIX 5.0 SP2 tables under
/// tag=value, with the same index capacity `tests/score.rs` gives FIX 4.4.
///
/// `dict` names `Fix44TagValue` for the FIX 4.4 pairing; there is no published
/// alias for this one yet, so the test spells it — `CLAUDE.md` §6, the caller
/// picks `N`.
type Fixt = TagValue<Fixt11Fix50Sp2Tables, 256>;

/// `session` cannot depend on `conformance` — that is the dev-dependency
/// direction, and reversing it is a cycle. So the two crates each own a `Link`
/// and this maps between them.
fn link(l: fixbolt_session::Link) -> Link {
    match l {
        fixbolt_session::Link::Up => Link::Up,
        fixbolt_session::Link::Dropped => Link::Dropped,
    }
}

/// `tests/score.rs`'s adapter, with the configuration taken as an argument.
///
/// Deliberately a near-copy rather than a shared helper: `score.rs` is the gate
/// that must keep passing **unmodified** (§2 item 3), and a shared fixture is a
/// file two gates can be broken from at once. The three things it stands in for
/// — identity ownership, framing, the application — are the same three its own
/// doc names.
struct Adapter {
    cfg: Config,
    conns: Vec<Wire>,
    app: EchoApp,
}

impl EchoApp {
    /// The fixture is told the corpus's `1137=` because the corpus's own
    /// `35=j` carries it — `conformance::echo::business_reject_with` says what
    /// is measured and what is not known about why.
    fn new(corpus: &Corpus) -> Self {
        Self(
            fixbolt_conformance::echo::Echo::default()
                .speaking(corpus.default_appl_ver_id.as_bytes()),
        )
    }
}

/// One connection: its state machine and the bytes that have arrived for it.
struct Wire {
    conn: Conn,
    session: Session<Fixt, Acceptor>,
    journal: Store,
    rx: Framer<RX>,
}

/// `[measured]` the longest message in the corpus is 200 bytes; 4 KiB leaves
/// room for an application message an order of magnitude bigger.
const RX: usize = 4096;

impl Adapter {
    fn new(corpus: &Corpus) -> Self {
        Self {
            cfg: acceptor(corpus),
            conns: Vec::new(),
            app: EchoApp::new(corpus),
        }
    }

    fn at(&mut self, conn: Conn) -> usize {
        if let Some(i) = self.conns.iter().position(|w| w.conn == conn) {
            return i;
        }
        self.conns.push(Wire {
            conn,
            session: Session::new(self.cfg),
            journal: Store::new(),
            rx: Framer::new(),
        });
        self.conns.len() - 1
    }
}

impl SessionUnderTest for Adapter {
    fn step<F: FnMut(&[u8])>(&mut self, conn: Conn, input: Input<'_>, mut emit: F) -> Link {
        let i = self.at(conn);
        let Input::Bytes(bytes) = input else {
            let s = &mut self.conns[i].session;
            return link(match input {
                Input::Connect => s.connect(emit),
                Input::Disconnect => s.disconnect(emit),
                Input::Tick(ms) => s.tick(ms, emit),
                Input::Bytes(_) => unreachable!("handled below"),
                Input::Originate(i) => panic!("the acceptor corpus must not be driven: {i:?}"),
            });
        };

        {
            let spare = self.conns[i].rx.spare();
            let n = spare.len().min(bytes.len());
            spare[..n].copy_from_slice(&bytes[..n]);
            self.conns[i].rx.filled(n);
        }

        let mut result = Link::Up;
        loop {
            let taken = match self.conns[i].rx.cut() {
                Cut::Need => break,
                Cut::Message(n) | Cut::Garbage(n) => n,
            };

            // One identity, one connection — `1b_DuplicateIdentity.def` and
            // `AlreadyLoggedOn.def`, both of which the FIXT corpora also carry.
            let taken_is_logon = field(self.conns[i].rx.bytes(taken), 35) == Some(b"A");
            if taken_is_logon
                && self
                    .conns
                    .iter()
                    .enumerate()
                    .any(|(j, w)| j != i && w.session.is_logged_on())
            {
                self.conns[i].rx.take(taken);
                self.conns[i].session.disconnect(&mut emit);
                return Link::Dropped;
            }

            let app = &mut self.app;
            let w = &mut self.conns[i];
            result =
                link(
                    w.session
                        .received_with(w.rx.bytes(taken), app, &mut w.journal, &mut emit),
                );
            w.rx.take(taken);
            if result == Link::Dropped {
                break;
            }
        }
        result
    }
}

/// The acceptance server's own application, echoing under the FIXT tables.
///
/// The one thing row B3's `echo.rs` half is for: the same fixture, told which
/// encoding to rebuild a reply with. `8_AdminAndApplicationMessages`,
/// `8_OnlyApplicationMessages`, `15_HeaderAndBodyFieldsOrderedDifferently` and
/// `2r_UnregisteredMsgType` are in all three corpora and none of them passes
/// without it.
struct EchoApp(fixbolt_conformance::echo::Echo<Fixt>);

impl fixbolt_session::Application for EchoApp {
    fn on_message(
        &mut self,
        msg: &[u8],
        hdr: fixbolt_session::Header<'_>,
        out: &mut [u8],
    ) -> Option<std::ops::Range<usize>> {
        self.0.reply(msg, hdr.seq, hdr.stamp, out)
    }
}

/// The acceptor a corpus talks to: ISLD, against that corpus's counterparty and
/// its `1137=`.
fn acceptor(corpus: &Corpus) -> Config {
    Config::acceptor_fixt(
        b"FIXT.1.1",
        b"ISLD",
        corpus.comp_id.as_bytes(),
        corpus.default_appl_ver_id.as_bytes(),
    )
}

/// How many of `corpus`'s files pass, and every failure — [`Failure`] itself,
/// not a formatted string, so a caller can check *which* file and *which*
/// line failed rather than trusting a printed line. `Failure::reason` carries
/// the comparator's summary (`Mismatch::FieldCount { expected: 17, actual: 13
/// }` for the one ADR-0084 decision 3 accepts); it does not carry the
/// engine's own bytes — [`engine_output_at`] gets those, separately, for the
/// one file that needs them.
fn score(corpus: &Corpus) -> (usize, usize, Vec<Failure>) {
    let all = load_corpus(corpus).unwrap_or_else(|e| panic!("{e}"));
    let mut passed = 0;
    let mut failed = Vec::new();
    for s in &all {
        let mut adapter = Adapter::new(corpus);
        let failures = run_scenario(s, &mut adapter);
        if failures.is_empty() {
            passed += 1;
        } else {
            failed.push(
                failures
                    .into_iter()
                    .next()
                    .expect("checked non-empty above"),
            );
        }
    }
    (passed, all.len(), failed)
}

/// Drive one scenario up to one `E` line and return the engine's own bytes
/// for it, rather than trusting the comparator's summary of the mismatch.
///
/// `run_scenario`'s [`Failure::reason`] on
/// `21_RepeatingGroupSpecifierWithValueOfZero.def:17` is
/// `Mismatch::FieldCount { expected: 17, actual: 13 }` — it says the two
/// messages have a different shape and nothing about what the engine actually
/// put on the wire. ADR-0084 decision 3's content check needs the bytes
/// themselves, so this replays the file's own `Connect` / `Send` / `Tick`
/// steps through a fresh [`Adapter`] — the same session-under-test the score
/// uses — and returns the first message produced for the input immediately
/// before the named `E` line.
fn engine_output_at(corpus: &Corpus, file: &str, line_no: usize) -> Vec<u8> {
    let all = load_corpus(corpus).unwrap_or_else(|e| panic!("{e}"));
    let s = all
        .iter()
        .find(|s| s.file == file)
        .unwrap_or_else(|| panic!("{file} is not in {}", corpus.dir));
    let mut adapter = Adapter::new(corpus);
    let mut pending: Vec<Vec<u8>> = Vec::new();
    let now = FIXED_TIME_MILLIS;
    for step in &s.steps {
        let conn = Conn(step.session.unwrap_or(1));
        match &step.kind {
            Kind::Connect => {
                adapter.step(conn, Input::Connect, |b: &[u8]| pending.push(b.to_vec()));
                adapter.step(conn, Input::Tick(now), |b: &[u8]| pending.push(b.to_vec()));
            }
            Kind::Disconnect => {
                adapter.step(conn, Input::Disconnect, |b: &[u8]| pending.push(b.to_vec()));
            }
            Kind::Send(m) => {
                adapter.step(conn, Input::Tick(now), |b: &[u8]| pending.push(b.to_vec()));
                adapter.step(conn, Input::Bytes(&m.wire), |b: &[u8]| {
                    pending.push(b.to_vec())
                });
            }
            Kind::Expect(_) => {
                if step.line_no == line_no {
                    return pending
                        .into_iter()
                        .next()
                        .unwrap_or_else(|| panic!("{file}:{line_no} produced no output at all"));
                }
                if !pending.is_empty() {
                    pending.remove(0);
                }
            }
            Kind::ExpectDisconnect => {}
        }
    }
    panic!("{file}:{line_no} was never reached while driving the scenario");
}

/// **179 / 180.** ADR-0084 decision 3: the 180th is not skipped, not
/// excluded and no fixture is edited — it is asserted, by content, to be
/// exactly one known divergence.
///
/// The reversal `1d_InvalidLogonNoDefaultApplVerID` proves is written down in
/// the plan's row B4: remove the `1137` check in `Session::judge` and it must
/// go red with *expected DISCONNECT, engine sent Logon* — in one corpus per
/// directory, so three files, not one.
#[test]
fn the_three_fixt_corpora_score_sixty_each() {
    let mut line = String::new();
    let mut total = 0;
    let mut expected = 0;
    let mut detail = String::new();
    let mut scores: Vec<(&'static str, usize, usize)> = Vec::new();
    let mut all_failures: Vec<(&'static str, Failure)> = Vec::new();
    for corpus in fixt_corpora() {
        let (passed, of, failures) = score(&corpus);
        line.push_str(&format!("{} {passed}/{of} ", corpus.dir));
        total += passed;
        expected += of;
        for f in &failures {
            detail.push_str(&format!(
                "\n  {}: {}:{} {}",
                corpus.dir, f.file, f.line_no, f.reason
            ));
        }
        scores.push((corpus.dir, passed, of));
        all_failures.extend(failures.into_iter().map(|f| (corpus.dir, f)));
    }
    println!("{}", line.trim_end());

    assert_eq!((total, expected), (179, 180), "{}{detail}", line.trim_end());

    // `fix50sp1` and `fix50sp2` each 60 / 60; `fix50` 59 / 60 — named per
    // corpus, so a mismatch says *which* corpus moved, not only that the sum
    // did.
    for (dir, passed, of) in &scores {
        let want = if *dir == "fix50" { 59 } else { 60 };
        assert_eq!(
            (*passed, *of),
            (want, 60),
            "{dir} should be {want}/60{detail}"
        );
    }

    // The single failure is exactly this file, on this corpus, on this line —
    // any other file, any other corpus, or a second failure, is red.
    assert_eq!(
        all_failures.len(),
        1,
        "expected exactly one failing file across the 180{detail}"
    );
    let (corpus_dir, failure) = &all_failures[0];
    assert_eq!(
        *corpus_dir, "fix50",
        "the one accepted divergence is fix50's alone (ADR-0084 decision 3){detail}"
    );
    assert_eq!(
        failure.file, "21_RepeatingGroupSpecifierWithValueOfZero.def",
        "a different file failed than the one ADR-0084 decision 3 names{detail}"
    );
    assert_eq!(
        failure.line_no, 17,
        "the failure is on a different line than ADR-0084 decision 3 names{detail}"
    );

    // The comparator's own reason (`Mismatch::FieldCount { expected: 17,
    // actual: 13 }`) says only that the shapes differ. Drive the file again
    // and read what the engine actually put on the wire.
    let corpus = fixt_corpora()
        .into_iter()
        .find(|c| c.dir == *corpus_dir)
        .expect("named just above");
    let wire = engine_output_at(&corpus, &failure.file, failure.line_no);
    assert_eq!(
        field(&wire, 35),
        Some(&b"3"[..]),
        "expected a Reject (35=3), not the echoed 35=d"
    );
    assert_eq!(
        field(&wire, 373),
        Some(&b"5"[..]),
        "expected SessionRejectReason 5, Value is incorrect (out of range) for this tag"
    );
    assert_eq!(
        field(&wire, 371),
        Some(&b"336"[..]),
        "expected RefTagID naming TradingSessionID(336)"
    );
}

/// The file the FIX 4.4 corpus does not have is one of the sixty, by name.
///
/// Not implied by the count: a corpus that scored 60 with this file failing and
/// some other file passing twice is impossible, but a corpus whose loader
/// silently skipped it would still read 59/59 and then 60 once anything else
/// was added. Naming it is what makes the reversal in row B4 land here.
#[test]
fn the_no_default_appl_ver_id_file_is_one_of_the_sixty() {
    for corpus in fixt_corpora() {
        let all = load_corpus(&corpus).unwrap_or_else(|e| panic!("{e}"));
        assert!(
            all.iter()
                .any(|s| s.file == "1d_InvalidLogonNoDefaultApplVerID.def"),
            "{} does not carry the file the FIXT rule is proved by",
            corpus.dir
        );
        let s = all
            .iter()
            .find(|s| s.file == "1d_InvalidLogonNoDefaultApplVerID.def")
            .expect("named just above");
        let mut adapter = Adapter::new(&corpus);
        let failures = run_scenario(s, &mut adapter);
        assert!(
            failures.is_empty(),
            "{}: {:?}",
            corpus.dir,
            failures.first().map(|f| f.reason.to_string())
        );
    }
}

/// The value of one field, by tag.
fn field(wire: &[u8], tag: u32) -> Option<&[u8]> {
    let needle = format!("\u{1}{tag}=");
    let at = wire
        .windows(needle.len())
        .position(|w| w == needle.as_bytes())?
        + needle.len();
    let end = wire[at..].iter().position(|&b| b == 1)? + at;
    Some(&wire[at..end])
}
