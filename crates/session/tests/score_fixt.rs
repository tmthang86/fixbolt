//! The FIXT 1.1 score: **180 / 180**, three corpora of sixty.
//!
//! `CLAUDE.md` §2 non-negotiable 3 names the 59 FIX 4.4 definitions as the
//! session layer's gate; `tests/score.rs` is that gate and nothing here touches
//! it. This is the second one ADR-0080 decision 2 asks for, and it is a
//! different kind of evidence: the 59 prove the state machine still does what
//! it did, these 180 prove the **same** state machine does it through a second
//! `E::Dict`, built from two XML files, with three FIXT rules on top.
//!
//! # Three corpora, one table
//!
//! `[measured 2026-09-19]`, ADR-0080 *Context*: `fix50`, `fix50sp1` and
//! `fix50sp2` differ only in the counterparty's `SenderCompID` and in the
//! `1137=DefaultApplVerID` their Logons carry (`7`, `8`, `9`). So there is one
//! generated table — SP2's, a superset — and the other two are covered **by
//! parameter, not by table**, which is decision 2's own sentence. Each corpus
//! prints its own number, because three sixties summed into one 180 would hide
//! a corpus that scored 59 and one that scored 61.
//!
//! # The file the 59 do not have
//!
//! `1d_InvalidLogonNoDefaultApplVerID.def` — a `Logon` carrying `98=0 108=30`
//! and no `1137`, answered by `eDISCONNECT` with no `E` line in front of it.
//! It is the entire oracle for [`fixbolt_session::DropReason`]'s
//! `LogonWithoutDefaultApplVerId`, and it is the one file that fails if the
//! check is removed.
//!
//! # This test is RED at 173 / 180, and the seven are not session defects
//!
//! `[measured 2026-09-19]` `fix50 57/60 fix50sp1 58/60 fix50sp2 58/60`. Every
//! FIXT rule of ADR-0080 decision 3 is built and every file that turns on one
//! of them passes. The seven that fail are **three questions ADR-0080 decision
//! 2 did not answer**, each needing a decision this row may not take, and each
//! measured rather than reasoned:
//!
//! 1. **An admin message's body is validated against the merged table, and
//!    QuickFIX validates it against the transport dictionary alone.**
//!    `14a_BadField.def` sends `999=HI` on a `35=0` and expects `373=0`
//!    *Invalid tag number*. `FIXT11.xml` does not define `999`; `FIX50SP2.xml`
//!    does (`LegUnitOfMeasure`), so the merged table answers
//!    `is_defined_tag(999) == true`, `allows("0", 999) == false`, and this
//!    session says `373=2`. ADR-0083 *Context* item 4 records the QuickFIX
//!    behaviour in passing — *"`Message.cpp` line 328–329 parses an admin
//!    message's body against the session dictionary"*. One merged
//!    `is_defined_tag` cannot express it; `Tables` has no question that
//!    separates a transport field from an application one. Three files.
//! 2. **A repeating group's members are value-checked here and are not by
//!    QuickFIX.** `14i_RepeatingGroupCountNotEqual.def` declares `386=3` and
//!    sends two `336=PRE-OPEN` entries, expecting `373=16`. `336` is
//!    `group_members("D", 386)[0]` and `enum_allows(336, b"PRE-OPEN")` is
//!    `Some(false)` under SP2 — FIX 4.4 gives `336` no enumerated values and
//!    FIX 5.0 gave it none either, SP1 added six — so `scan_fields` answers
//!    `373=5` before `bad_group_count` is reached. QuickFIX's
//!    `DataDictionary::iterate` walks the top-level `FieldMap` only; group
//!    members live in nested maps and their values are never checked. This
//!    engine's index is flat by design (D2), so the rule would have to be the
//!    session's — and it would change FIX 4.4 behaviour, which is a row of its
//!    own. Two files, `fix50sp1` and `fix50sp2`.
//! 3. **The three corpora are not one corpus with a different `1137`.**
//!    ADR-0080 *Context* says they *"differ only in the CompID and the value of
//!    `1137`"*. `21_RepeatingGroupSpecifierWithValueOfZero.def` disproves it:
//!    `fix50`'s copy carries `336=ONE_MAIN` on a `35=d` and `fix50sp1`'s and
//!    `fix50sp2`'s do not. `TradingSessionID(336)` carries **no** enumerated
//!    values in `FIX50.xml`, six in `FIX50SP1.xml` and seven in `FIX50SP2.xml`
//!    (counted with `xml.etree` on 2026-09-19), so QuickFIX echoed the message
//!    and the SP2 table refuses it `373=5`. Covering `fix50` "by parameter,
//!    not by table" is what does not hold. One file.
//!
//! All three are for the architect (`CLAUDE.md` §12: a design problem goes to
//! the architect through the manager). **Nothing here was relaxed to make a
//! number**: the assertion below still reads 180, because that is what the plan
//! row promises and a gate quietly lowered to what was achieved is not a gate.
#![cfg(feature = "fix50sp2")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
// Not a library crate's source: non-negotiable 7 is about `crates/*/src`, and
// `scripts/check-indexing-debt.sh` counts nothing outside it. An index that
// panics in a test is a failing test, which is what a test is for.
#![allow(clippy::indexing_slicing)]

use fixbolt_codec::TagValue;
use fixbolt_conformance::runner::{Conn, Input, Link, SessionUnderTest, run_scenario};
use fixbolt_conformance::script::{Corpus, fixt_corpora, load_corpus};
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

/// How many of `corpus`'s files pass, and which do not.
fn score(corpus: &Corpus) -> (usize, usize, Vec<String>) {
    let all = load_corpus(corpus).unwrap_or_else(|e| panic!("{e}"));
    let mut passed = 0;
    let mut failed = Vec::new();
    for s in &all {
        let mut adapter = Adapter::new(corpus);
        let failures = run_scenario(s, &mut adapter);
        if failures.is_empty() {
            passed += 1;
        } else {
            let first = &failures[0];
            failed.push(format!("{}:{} {}", first.file, first.line_no, first.reason));
        }
    }
    (passed, all.len(), failed)
}

/// **180 / 180**, and each corpus's own number is printed.
///
/// The reversal this test exists for is written down in the plan's row B4:
/// remove the `1137` check in `Session::judge` and
/// `1d_InvalidLogonNoDefaultApplVerID` must go red with *expected DISCONNECT,
/// engine sent Logon* — in one corpus per directory, so three files, not one.
#[test]
fn the_three_fixt_corpora_score_sixty_each() {
    let mut line = String::new();
    let mut total = 0;
    let mut expected = 0;
    let mut detail = String::new();
    for corpus in fixt_corpora() {
        let (passed, of, failed) = score(&corpus);
        line.push_str(&format!("{} {passed}/{of} ", corpus.dir));
        total += passed;
        expected += of;
        for f in failed {
            detail.push_str(&format!("\n  {}: {f}", corpus.dir));
        }
    }
    println!("{}", line.trim_end());
    assert_eq!((total, expected), (180, 180), "{}{detail}", line.trim_end());
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
