//! The four FIXT 1.1 rules of ADR-0080 decision 3, one test each, plus the
//! FIX 4.4 twin that proves each one is scoped to the `BeginString`.
//!
//! `tests/score_fixt.rs` runs the corpora; this file holds the cases the
//! corpora cannot see. Two of the four have **no oracle at all** and ADR-0080
//! says so in its own text — a `1137` that differs from ours, and a `1128`
//! outside the FIX 5.0 family. These are the tests that keep those two
//! decisions from drifting, and each one names the ADR rather than restating
//! its reasoning.
#![cfg(feature = "fix50sp2")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
// Not a library crate's source — see `tests/score.rs`'s note.
#![allow(clippy::indexing_slicing)]

use fixbolt_codec::TagValue;
use fixbolt_conformance::script::{FIXED_TIME_IN, FIXED_TIME_MILLIS};
use fixbolt_dict::{Fix44, Fixt11Fix50Sp2Tables};
use fixbolt_session::journal::NoJournal;
use fixbolt_session::{
    Acceptor, Application, Config, DictionaryChecks, DropReason, Header, Session,
};

/// The FIXT 1.1 / FIX 5.0 SP2 encoding, as `tests/score_fixt.rs` spells it.
type Fixt = TagValue<Fixt11Fix50Sp2Tables, 256>;
/// FIX 4.4, for the twin that proves each rule is scoped to the `BeginString`.
type Fix44Tv = TagValue<Fix44, 256>;

/// What the corpus's acceptor is: ISLD, speaking FIX 5.0 SP2 to TW50SP2.
fn fixt_acceptor() -> Session<Fixt, Acceptor> {
    Session::new(Config::acceptor_fixt(
        b"FIXT.1.1",
        b"ISLD",
        b"TW50SP2",
        b"9",
    ))
}

/// One message, with `9=` and `10=` computed, `<T>` replaced by the instant the
/// harness ticks to, and `|` for SOH.
fn msg(begin: &str, body: &str) -> Vec<u8> {
    let body = body.replace("<T>", FIXED_TIME_IN).replace('|', "\u{1}");
    let mut m = format!("8={begin}\u{1}9={}\u{1}", body.len()).into_bytes();
    m.extend_from_slice(body.as_bytes());
    let sum: u32 = m.iter().map(|c| u32::from(*c)).sum();
    m.extend_from_slice(format!("10={:03}\u{1}", sum % 256).as_bytes());
    m
}

/// Everything the session put on the wire, as one readable string.
fn readable(out: &[Vec<u8>]) -> String {
    out.iter()
        .map(|m| String::from_utf8_lossy(m).replace('\u{1}', "|"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// **A FIXT `Logon` without `1137` is dropped, and not one byte goes out.**
///
/// `1d_InvalidLogonNoDefaultApplVerID.def` is four lines and its last is
/// `eDISCONNECT` with **no `E` line in front of it**. The assertion is on the
/// output being *empty*, not on a `Logout` being absent: a `Reject`, a
/// `Logout`, or a `Logon` followed by a drop would each satisfy "no Logout"
/// and each is a different engine.
#[test]
fn a_logon_without_1137_is_dropped_and_nothing_is_sent() {
    let mut s = fixt_acceptor();
    let mut out: Vec<Vec<u8>> = Vec::new();
    s.connect(|b: &[u8]| out.push(b.to_vec()));
    s.tick(FIXED_TIME_MILLIS, |b: &[u8]| out.push(b.to_vec()));
    let logon = msg(
        "FIXT.1.1",
        "35=A|34=1|49=TW50SP2|52=<T>|56=ISLD|98=0|108=30|",
    );
    let link = s.received(&logon, |b: &[u8]| out.push(b.to_vec()));

    assert_eq!(link, fixbolt_session::Link::Dropped);
    assert_eq!(
        s.last_drop_reason(),
        Some(DropReason::LogonWithoutDefaultApplVerId),
        "the reason must name the missing field, not the generic incomplete Logon"
    );
    assert!(
        out.is_empty(),
        "nothing at all may go out; the engine sent:\n{}",
        readable(&out)
    );
}

/// **A `1137` other than ours is accepted**, and readable afterwards.
///
/// ADR-0080 decision 3: SP2's tables are a superset of SP0's and SP1's and no
/// `.def` in the three corpora sends a mismatch, so a refusal here would be a
/// rule with no test. This is the test that stops one being added by accident,
/// and `Session::peer_default_appl_ver_id` is how an operator sees the
/// disagreement the session chose not to act on.
#[test]
fn a_default_appl_ver_id_that_differs_from_ours_still_logs_on() {
    let mut s = fixt_acceptor();
    let mut out: Vec<Vec<u8>> = Vec::new();
    s.connect(|b: &[u8]| out.push(b.to_vec()));
    s.tick(FIXED_TIME_MILLIS, |b: &[u8]| out.push(b.to_vec()));
    // Ours is `9` (FIX 5.0 SP2); theirs says `7` (FIX 5.0).
    let logon = msg(
        "FIXT.1.1",
        "35=A|34=1|49=TW50SP2|52=<T>|56=ISLD|98=0|108=30|1137=7|",
    );
    let link = s.received(&logon, |b: &[u8]| out.push(b.to_vec()));

    assert_eq!(link, fixbolt_session::Link::Up);
    assert!(s.is_logged_on(), "a mismatch is accepted, not refused");
    assert_eq!(s.peer_default_appl_ver_id(), Some(&b"7"[..]));
    let wire = readable(&out);
    assert!(
        wire.contains("|1137=9|"),
        "the reply states what THIS end speaks, it does not echo theirs:\n{wire}"
    );
}

/// **`1128` naming a FIX 5.0 family version is parsed and ignored.**
///
/// The neutral twin of the test below. A branch that refused nothing would pass
/// that one and fail this one, which is the whole reason a rule that *accepts*
/// gets a test of its own.
#[test]
fn an_appl_ver_id_in_the_fix_50_family_is_validated_normally() {
    let (out, next_in) = after_an_order_carrying("1128=9");
    let out = readable(&out);
    assert!(
        !out.contains("|35=3|"),
        "`1128=9` is this session's own version and must not be refused:\n{out}"
    );
    // Silence alone would pass the line above even if the order had been
    // dropped on the floor, so the count is what says it was *accepted*: the
    // Logon was `34=1` and the order `34=2`, so the session now wants 3.
    assert_eq!(next_in, 3, "the order was taken, not merely not-rejected");
}

/// **`1128` naming FIX 4.2 is `Reject 373=5` with `371=1128`.**
///
/// ADR-0080 decision 3, and one of its two behaviours with no oracle: no `.def`
/// in the three corpora carries `1128` at all. `0`–`6` name FIX 2.7 through
/// FIX 4.4, which a FIXT session cannot be speaking; `text.rs` gains no new
/// text because *Value is incorrect* is already the right sentence.
#[test]
fn an_appl_ver_id_outside_the_fix_50_family_is_rejected() {
    let out = readable(&after_an_order_carrying("1128=4").0);
    assert!(out.contains("|35=3|"), "expected a Reject:\n{out}");
    assert!(out.contains("|371=1128|"), "naming the field:\n{out}");
    assert!(out.contains("|373=5|"), "with `Value is incorrect`:\n{out}");
}

/// **`35=n` XMLnonFIX is *not* asked the `1128` rule.** ADR-0086 decision 3.
///
/// The rule of ADR-0080 decision 3 is an **application** message rule, and
/// which messages are not application ones is the dictionary's answer, read
/// from `msgcat` — `Tables::is_admin`, eight types in both dictionaries — and
/// no longer a seven-entry list beside the call site that lacked `n`. So the
/// very `1128=4` that earns `373=5 371=1128` on the `35=D` two tests above
/// earns nothing here.
///
/// **There is no oracle**: no `.def` in the three corpora sends `35=n` at all,
/// which is exactly why this test exists and why ADR-0086 wrote its expected
/// failure down before it was run. `n` is a transport message for `373=0`
/// already (ADR-0084 decision 1); this makes it one for `1128` too.
#[test]
fn an_xmlnonfix_message_is_not_asked_the_appl_ver_id_rule() {
    let (out, next_in, link) = after_a_report(&xmlnonfix("1128=4|"));

    let wire = readable(&out);
    assert_eq!(link, fixbolt_session::Link::Up, "still up:\n{wire}");
    assert!(
        !wire.contains("|35=3|"),
        "expected no reject, engine sent {}",
        reason_and_tag(&wire)
    );
    // Silence alone would pass the line above even if the message had been
    // dropped on the floor: the Logon was `34=1` and this `34=2`, so a session
    // that took it now wants 3.
    assert_eq!(next_in, 3, "it was taken, not merely not-rejected");
}

/// **Routing does not change: `35=n` still reaches the application.**
/// ADR-0086 decision 2, and the test that decision names.
///
/// `SESSION_OWNED` — `ADMIN` renamed — stays at **seven** entries and `n` is
/// not one of them, so an XMLnonFIX message is handed to `Application::on_message`
/// and any reply is journalled, exactly as before. QuickFIX C++ and QuickFIX/J
/// both answer `isApp() == true` for `35=n`; QuickFIX/n calls it admin. No
/// `.def` arbitrates, so **no gate but this one can see the day somebody
/// "tidies" 3407 into `D::is_admin`** and silently starts gap-filling over a
/// message a venue sent.
///
/// It is green before the change as well as after — that is the point of it.
#[test]
fn xmlnonfix_still_reaches_the_application() {
    /// Answers nothing, and remembers every message it was handed.
    #[derive(Default)]
    struct Recorder {
        seen: Vec<String>,
    }

    impl Application for Recorder {
        fn on_message(
            &mut self,
            msg: &[u8],
            _hdr: Header<'_>,
            _out: &mut [u8],
        ) -> Option<core::ops::Range<usize>> {
            self.seen
                .push(String::from_utf8_lossy(msg).replace('\u{1}', "|"));
            None
        }
    }

    let mut s = fixt_acceptor();
    let mut sink: Vec<Vec<u8>> = Vec::new();
    let mut app = Recorder::default();
    s.connect(|b: &[u8]| sink.push(b.to_vec()));
    s.tick(FIXED_TIME_MILLIS, |b: &[u8]| sink.push(b.to_vec()));
    let logon = msg(
        "FIXT.1.1",
        "35=A|34=1|49=TW50SP2|52=<T>|56=ISLD|98=0|108=30|1137=9|",
    );
    s.received_with(&logon, &mut app, &mut NoJournal, |b: &[u8]| {
        sink.push(b.to_vec());
    });
    sink.clear();
    app.seen.clear();

    let link = s.received_with(&xmlnonfix(""), &mut app, &mut NoJournal, |b: &[u8]| {
        sink.push(b.to_vec())
    });

    let wire = readable(&sink);
    assert_eq!(link, fixbolt_session::Link::Up, "still up:\n{wire}");
    assert!(
        sink.is_empty(),
        "the session answers nothing itself:\n{wire}"
    );
    assert_eq!(
        app.seen.len(),
        1,
        "`35=n` belongs to the application, and exactly one arrived: {:?}",
        app.seen
    );
    assert!(
        app.seen[0].contains("|35=n|"),
        "and it is the XMLnonFIX message, whole and untouched: {:?}",
        app.seen
    );
    assert_eq!(s.next_in(), 3, "and it counted");
}

/// An XMLnonFIX message at `34=2`, with whatever extra header fields the
/// caller names. `FIXT11.xml` declares `<message name="XMLnonFIX"
/// msgtype="n" msgcat="admin"/>` — self-closing, so the standard header and
/// trailer are the whole message.
fn xmlnonfix(extra: &str) -> Vec<u8> {
    msg(
        "FIXT.1.1",
        &format!("35=n|34=2|49=TW50SP2|52=<T>|56=ISLD|{extra}"),
    )
}

/// Log on, then send the corpus's own `NewOrderSingle` with one extra header
/// field, and return everything that went out afterwards, with the `34=` the
/// session wants next.
fn after_an_order_carrying(extra: &str) -> (Vec<Vec<u8>>, u32) {
    let mut s = fixt_acceptor();
    let mut sink: Vec<Vec<u8>> = Vec::new();
    s.connect(|b: &[u8]| sink.push(b.to_vec()));
    s.tick(FIXED_TIME_MILLIS, |b: &[u8]| sink.push(b.to_vec()));
    let logon = msg(
        "FIXT.1.1",
        "35=A|34=1|49=TW50SP2|52=<T>|56=ISLD|98=0|108=30|1137=9|",
    );
    s.received(&logon, |b: &[u8]| sink.push(b.to_vec()));
    sink.clear();

    // `8_OnlyApplicationMessages.def`'s order, which the corpus accepts, plus
    // `1128=`. It sits among the header fields because `1128` is a FIXT header
    // field and `373=14` refuses a header field after a body one.
    let order = msg(
        "FIXT.1.1",
        &format!(
            "35=D|34=2|49=TW50SP2|52=<T>|56=ISLD|{extra}|11=ID|21=3|40=1|54=1|55=INTC|60=<T>|"
        ),
    );
    s.received(&order, |b: &[u8]| sink.push(b.to_vec()));
    let next_in = s.next_in();
    (sink, next_in)
}

/// **A FIX 4.4 session emits no `1137`** — the rule is the `BeginString`'s.
///
/// The slot is declared on the `Logon` skeleton for every encoding
/// (`out::Outbound::new`, non-negotiable 5: the dictionary places it, not a
/// call site). An unset slot is not written, and this is what says so on the
/// wire rather than in the comment.
#[test]
fn a_fix_44_session_emits_no_1137() {
    let mut s: Session<Fix44Tv, Acceptor> =
        Session::new(Config::acceptor(b"FIX.4.4", b"ISLD", b"TW44"));
    let mut out: Vec<Vec<u8>> = Vec::new();
    s.connect(|b: &[u8]| out.push(b.to_vec()));
    s.tick(FIXED_TIME_MILLIS, |b: &[u8]| out.push(b.to_vec()));
    let logon = msg("FIX.4.4", "35=A|34=1|49=TW44|52=<T>|56=ISLD|98=0|108=30|");
    s.received(&logon, |b: &[u8]| out.push(b.to_vec()));

    let wire = readable(&out);
    assert!(s.is_logged_on(), "the FIX 4.4 Logon still works:\n{wire}");
    assert!(
        !wire.contains("1137"),
        "a FIX 4.4 Logon neither requires nor emits `1137`:\n{wire}"
    );
    // And the rule that refuses a Logon without it is FIXT's alone: this one
    // carried no `1137` and was answered rather than dropped.
    assert_eq!(s.last_drop_reason(), None);
}

// ---------------------------------------------------------------------------
// ADR-0085: what a counterparty is told once the deferral array is full.
// ---------------------------------------------------------------------------

/// 32 of the 48 top-level groups `TradeCaptureReport(AE)` declares — the ones
/// whose delimiter enumerates no value, so a one-entry group can carry any
/// well-formed one. `(counter, delimiter, a value the delimiter's type
/// accepts)`, ADR-0085 *Sources*.
///
/// Thirty-two blocks, **thirty-three** counters: `40204`'s delimiter `40209`
/// is itself a `NumInGroup` with a group of its own, so that block records
/// two. `SEEN` is 32, so everything after the last block is past the bound —
/// which `lib.rs`'s `an_ae_with_thirty_three_group_counters_fills_the_array`
/// asserts directly, this file being unable to see a private field.
const AE_GROUPS: [(&str, &str, &str); 32] = [
    ("1907", "1903", "X"),
    ("1116", "1117", "R"),
    ("454", "455", "A"),
    ("1976", "1977", "1"),
    ("2304", "2305", "A"),
    ("1018", "1019", "P"),
    ("40278", "40471", "B"),
    ("41230", "41231", "B"),
    ("41092", "41093", "E"),
    ("41094", "41095", "F"),
    ("42775", "42776", "B"),
    ("41116", "41117", "B"),
    ("41137", "41138", "20260919"),
    ("41140", "41141", "B"),
    ("41152", "41153", "20260919"),
    ("40019", "40020", "Y"),
    ("40181", "40182", "1.0"),
    ("40022", "40023", "USD"),
    ("40204", "40209", "0"),
    ("42296", "42297", "E"),
    ("2734", "2733", "M"),
    ("2746", "2747", "20260919-12:00:00.000"),
    ("40040", "40041", "D"),
    ("40046", "40047", "S"),
    ("40042", "40043", "M"),
    ("711", "311", "U"),
    ("1703", "1704", "1.0"),
    ("555", "600", "L"),
    ("768", "769", "20260919-12:00:00.000"),
    ("1387", "1388", "1"),
    ("41312", "41313", "J"),
    ("2104", "2105", "A"),
];

/// The 32 blocks, then a stray `447` at top level, then the `552` group whose
/// nested `453` is the group `447` belongs to.
///
/// `regulatory_count` is `1907`'s declared count — `2` against one entry is a
/// `373=16` waiting in the third pass. `stray` is the top-level `447`: `ZZ` is
/// not a `PartyIDSource`, `D` is.
fn trade_capture_report(regulatory_count: &str, stray: &str) -> Vec<u8> {
    let mut body = String::from("35=AE|34=2|49=TW50SP2|52=<T>|56=ISLD|");
    for (counter, delimiter, value) in AE_GROUPS {
        let count = if counter == "1907" {
            regulatory_count
        } else {
            "1"
        };
        body.push_str(&format!("{counter}={count}|{delimiter}={value}|"));
    }
    body.push_str(&format!("447={stray}|"));
    body.push_str("552=1|54=1|453=1|448=A|447=D|452=1|");
    msg("FIXT.1.1", &body)
}

/// Log on, send `wire`, and return what went out after the Logon, with the
/// `34=` the session wants next and the link state.
fn after_a_report(wire: &[u8]) -> (Vec<Vec<u8>>, u32, fixbolt_session::Link) {
    after_a_report_on(fixt_acceptor(), wire)
}

/// The same, on a session the caller configured.
fn after_a_report_on(
    mut s: Session<Fixt, Acceptor>,
    wire: &[u8],
) -> (Vec<Vec<u8>>, u32, fixbolt_session::Link) {
    let mut sink: Vec<Vec<u8>> = Vec::new();
    s.connect(|b: &[u8]| sink.push(b.to_vec()));
    s.tick(FIXED_TIME_MILLIS, |b: &[u8]| sink.push(b.to_vec()));
    let logon = msg(
        "FIXT.1.1",
        "35=A|34=1|49=TW50SP2|52=<T>|56=ISLD|98=0|108=30|1137=9|",
    );
    s.received(&logon, |b: &[u8]| sink.push(b.to_vec()));
    sink.clear();
    let link = s.received(wire, |b: &[u8]| sink.push(b.to_vec()));
    (sink, s.next_in(), link)
}

/// The `373=` and `371=` a Reject carries, in that order — so a failure here
/// reads as the ADR wrote it down before it was run.
fn reason_and_tag(reject: &str) -> String {
    let field = |name: &str| {
        reject
            .split('|')
            .find(|f| f.starts_with(name))
            .unwrap_or("<absent>")
            .to_string()
    };
    format!("{} {}", field("373="), field("371="))
}

/// **A stray member met after the array is full is still answered in wire
/// order.** ADR-0085 decisions 1, 2 and 5.
///
/// 33 group counters fill `SeenCounters`, so every field after them asks
/// `in_a_group_before` instead of the array. `447=ZZ` sits *before* its own
/// counter `453`, so it is not a member of anything yet: `373=5` naming it,
/// from the first pass, ahead of the `373=16` that `1907=2` has waiting.
///
/// The reversal this was written against — put `in_a_group` back in the
/// `full` branch — makes the answer `373=16 371=1907`, because the whole-
/// message walk finds `453` further down and defers `447` past the count
/// check. Same bytes, two reason codes, chosen by a capacity nobody published.
#[test]
fn a_stray_member_is_answered_in_wire_order_when_the_array_is_full() {
    let (out, _next_in, _link) = after_a_report(&trade_capture_report("2", "ZZ"));

    let wire = readable(&out);
    assert_eq!(out.len(), 1, "exactly one Reject:\n{wire}");
    assert!(
        wire.contains("|373=5|") && wire.contains("|371=447|"),
        "expected 373=5 371=447, engine sent {}",
        reason_and_tag(&wire)
    );
}

/// The twin: the same 33 counters, both faults removed, and nothing is said.
///
/// Without it the test above could be green because the message is malformed
/// in some way that has nothing to do with the array — a shape this engine
/// refuses for a third reason would satisfy "one Reject naming 447" too.
#[test]
fn the_same_thirty_three_counters_without_the_two_faults_are_accepted() {
    let (out, next_in, link) = after_a_report(&trade_capture_report("1", "D"));

    let wire = readable(&out);
    assert_eq!(link, fixbolt_session::Link::Up, "still up:\n{wire}");
    assert!(out.is_empty(), "nothing is said about it:\n{wire}");
    assert_eq!(next_in, 3, "and it counted, so it was accepted");
}

// ---------------------------------------------------------------------------
// ADR-0085 decision 2, under `ValidateUserDefinedFields=N`: the fallback must
// ignore exactly the counters the array ignores.
// ---------------------------------------------------------------------------

/// 33 groups of `AE` whose counter **and** delimiter are below tag 5000, so the
/// array still fills when `ValidateUserDefinedFields=N` is taking user-defined
/// tags out of the scan. `1907` is last, so the array fills on it.
///
/// The sub-5000 counters left out are nested groups of `AE`, where
/// `MessageView::group` answers `None` and `bad_group_count` ends that pass
/// without a verdict — pre-existing, and it would have silenced the `373=16`
/// this fixture needs as its competing fault.
const AE_SUB_5000_GROUPS: [(&str, &str, &str); 33] = [
    ("73", "2887", "A"),
    ("78", "79", "A"),
    ("136", "137", "1.0"),
    ("453", "448", "A"),
    ("454", "455", "A"),
    ("457", "458", "A"),
    ("539", "524", "A"),
    ("555", "600", "L"),
    ("711", "311", "U"),
    ("756", "757", "A"),
    ("768", "769", "20260919-12:00:00.000"),
    ("781", "782", "A"),
    ("802", "523", "A"),
    ("804", "545", "A"),
    ("806", "760", "A"),
    ("887", "888", "A"),
    ("1016", "1012", "20260919-12:00:00.000"),
    ("1018", "1019", "P"),
    ("1058", "1059", "A"),
    ("1116", "1117", "R"),
    ("1334", "1335", "A"),
    ("1342", "1330", "A"),
    ("1387", "1388", "1"),
    ("1491", "1492", "20260919"),
    ("1516", "1517", "A"),
    ("1562", "1563", "A"),
    ("1586", "1587", "1.0"),
    ("1671", "1691", "A"),
    ("1703", "1704", "1.0"),
    ("1844", "1845", "A"),
    ("1855", "1856", "A"),
    ("1861", "1862", "A"),
    ("1907", "1903", "X"),
];

/// The 33 sub-5000 blocks, then `40212 NoPayments` — a user-defined counter —
/// and `492 PaymentMethod`, a member of it that is not user-defined.
///
/// `regulatory_count` is `1907`'s declared count: `2` against one entry is the
/// `373=16` that wins if `492` is wrongly deferred.
fn payments_after_a_full_array(regulatory_count: &str, payment_method: &str) -> Vec<u8> {
    let mut body = String::from("35=AE|34=2|49=TW50SP2|52=<T>|56=ISLD|");
    for (counter, delimiter, value) in AE_SUB_5000_GROUPS {
        let count = if counter == "1907" {
            regulatory_count
        } else {
            "1"
        };
        body.push_str(&format!("{counter}={count}|{delimiter}={value}|"));
    }
    body.push_str(&format!("40212=1|40213=1|492={payment_method}|"));
    msg("FIXT.1.1", &body)
}

/// The corpus's FIXT acceptor with `ValidateUserDefinedFields=N`.
fn fixt_acceptor_skipping_user_defined() -> Session<Fixt, Acceptor> {
    Session::new(
        Config::acceptor_fixt(b"FIXT.1.1", b"ISLD", b"TW50SP2", b"9")
            .with_validation(DictionaryChecks::new().skipping_user_defined_fields()),
    )
}

/// **A counter the scan is told to ignore does not defer its member.**
/// ADR-0085 decision 2, under `ValidateUserDefinedFields=N`.
///
/// `scan_fields` drops a tag at or above 5000 before it can record it, so
/// `40212` never enters `SeenCounters` — and `in_a_group_before`, which
/// answers once the array is full, must not find it either. `492` is then a
/// stray top-level field: `373=5` naming it, ahead of the `373=16` waiting on
/// `1907=2`.
///
/// The knob is the only variable, and the second half of this test moves it
/// back: with the check on, `40212` *is* a counter the scan passed, `492`
/// waits for it, and the answer is the `373=16`.
#[test]
fn a_member_of_a_user_defined_group_is_not_deferred_when_the_scan_ignores_its_counter() {
    let wire = payments_after_a_full_array("2", "ZZ");

    let (out, _next_in, _link) = after_a_report_on(fixt_acceptor_skipping_user_defined(), &wire);
    let readable_out = readable(&out);
    assert_eq!(out.len(), 1, "exactly one Reject:\n{readable_out}");
    assert!(
        readable_out.contains("|373=5|") && readable_out.contains("|371=492|"),
        "expected 373=5 371=492, engine sent {}",
        reason_and_tag(&readable_out)
    );

    let (out, _next_in, _link) = after_a_report(&wire);
    let readable_out = readable(&out);
    assert_eq!(out.len(), 1, "exactly one Reject:\n{readable_out}");
    assert!(
        readable_out.contains("|373=16|") && readable_out.contains("|371=1907|"),
        "with the check on, the same bytes defer 492 and the count speaks: {}",
        reason_and_tag(&readable_out)
    );
}

/// The twin: both faults removed and the knob still on, so the shape itself is
/// not what earns the Reject above.
#[test]
fn the_same_user_defined_group_without_the_two_faults_is_accepted() {
    let (out, next_in, link) = after_a_report_on(
        fixt_acceptor_skipping_user_defined(),
        &payments_after_a_full_array("1", "1"),
    );

    let wire = readable(&out);
    assert_eq!(link, fixbolt_session::Link::Up, "still up:\n{wire}");
    assert!(out.is_empty(), "nothing is said about it:\n{wire}");
    assert_eq!(next_in, 3, "and it counted, so it was accepted");
}
