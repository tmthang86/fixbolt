//! Two enum questions the FIX 4.4 path asked wrong, with no `.def` to see it.
//!
//! [ADR-0084] decision 2's row, which carries with it the per-token rule
//! [ADR-0083] decision 1 left waiting. Both are *the enum question asked wrong
//! on the FIX 4.4 path*, both live in `scan_fields`' `enum_allows` arm, and
//! neither is reachable from the 59 acceptance definitions — so every message
//! below is hand-made, and every one of them says why it exists.
//!
//! # Why the corpus is blind to both
//!
//! `14i_RepeatingGroupCountNotEqual.def` is the **only** populated repeating
//! group in the 59 (`PRD.md` §4), and the member it populates,
//! `TradingSessionID(336)`, carries **zero** enumerated values in `FIX44.xml`
//! — so no FIX 4.4 definition can put a refusable value inside a group.
//! `FIX44.xml` nevertheless has 511 distinct group-member fields, **110 of them
//! enumerated** (ADR-0084 *Context* item 9). And no definition sends a
//! multi-value field at all, so `18=2 A` — one legal two-value `ExecInst` —
//! has been earning a `373=5` since the codec's first day with nothing to say
//! so.
//!
//! # The order, and why it is not this file's invention
//!
//! QuickFIX/J asks the top-level questions first, `checkGroupCount` at the
//! counter's position among them, and only then descends into the entries;
//! QuickFIX C++ never descends at all. On a group whose count is wrong both
//! answer `373=16`. This engine keeps checking members — the dictionary
//! enumerates them and ADR-0001's posture is that a value the table can refuse
//! is refused — but it now asks **after** `373=1` and `373=16`, which is where
//! both engines put the count.
//!
//! [ADR-0083]: ../../../docs/decisions/ADR-0083-ten-field-types-one-field-spelled-two-ways-one-empty-component-and-where-the-sp2-oracle-comes-from.md
//! [ADR-0084]: ../../../docs/decisions/ADR-0084-a-session-message-is-checked-against-the-layer-that-defines-it-a-members-value-waits-for-the-count-and-fix50-is-its-own-oracle.md
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
// Not a library crate's source: non-negotiable 7 is about `crates/*/src`, and
// `scripts/check-indexing-debt.sh` counts nothing outside it.
#![allow(clippy::indexing_slicing)]

use fixbolt_codec::{Dictionary, TagValue};
use fixbolt_conformance::script::{FIXED_TIME_MILLIS, Kind, scenarios, with_real_checksum};
use fixbolt_dict::{Fix44, Tables};
use fixbolt_session::{Acceptor, Config, Link, Session};

/// Every `I` line of one definition file, in order — a real corpus line for
/// the Logon, so the session this file drives is the session the 59 drive.
fn inputs(file: &str) -> Vec<Vec<u8>> {
    scenarios()
        .unwrap_or_else(|e| panic!("{e}"))
        .into_iter()
        .find(|s| s.file == file)
        .unwrap_or_else(|| panic!("{file} is not in the corpus"))
        .steps
        .into_iter()
        .filter_map(|s| match s.kind {
            Kind::Send(m) => Some(m.wire),
            _ => None,
        })
        .collect()
}

fn logged_on() -> Session<TagValue<Fix44, 256>, Acceptor> {
    let cfg = Config::acceptor(b"FIX.4.4", b"ISLD", b"TW44");
    let mut s: Session<TagValue<Fix44, 256>, Acceptor> = Session::new(cfg);
    s.connect(|_| {});
    s.tick(FIXED_TIME_MILLIS, |_| {});
    let logon = &inputs("14a_BadField.def")[0];
    assert_eq!(s.received(logon, |_| {}), Link::Up, "the premise");
    assert!(s.is_logged_on(), "the premise");
    s
}

/// The `NewOrderSingle` body every message below is built from: the seven
/// tags `FIX44.xml` makes required for `D`, so nothing here is ever answered
/// `373=1` by accident — which is the point of a file about *ordering*.
///
/// `52=` and `60=` are the corpus's own fixed clock ([`FIXED_TIME_MILLIS`]),
/// or the session answers `373=10` *SendingTime accuracy problem* and logs
/// out before it ever looks at a value.
const REQUIRED: &str = "11=ID|21=1|38=002000.00|40=1|54=1|55=INTC|60=20260828-12:00:00.000|";

/// A `NewOrderSingle` whose body is `body`, framed with a real `9=` and `10=`.
///
/// `|` stands for SOH, as everywhere else in these tests, and `~` stands for
/// the single space inside a multi-value field — written that way so the
/// literal's own indentation can be stripped without eating it. `34=2` because
/// the Logon was `34=1`.
fn new_order_single(body: &str) -> Vec<u8> {
    let fields = format!("35=D|34=2|49=TW44|52=20260828-12:00:00.000|56=ISLD|{body}")
        .replace(' ', "")
        .replace('~', " ");
    let wire = format!("8=FIX.4.4|9={}|{fields}10=0|", fields.len()).replace('|', "\u{1}");
    with_real_checksum(wire.as_bytes())
}

fn answer(s: &mut Session<TagValue<Fix44, 256>, Acceptor>, wire: &[u8]) -> (Link, Vec<String>) {
    let mut out = Vec::new();
    let link = s.received(wire, |b| {
        out.push(String::from_utf8_lossy(b).replace('\u{1}', "|"));
    });
    (link, out)
}

// ---------------------------------------------------------------------------
// The premises, verified rather than trusted.
// ---------------------------------------------------------------------------

#[test]
fn the_party_group_is_the_shape_this_file_assumes() {
    assert_eq!(
        <Fix44 as Dictionary>::group_members(b"D", 453),
        &[448, 447, 452, 802],
        "NoPartyIDs(453) on a NewOrderSingle, delimiter first"
    );
    assert_eq!(
        <Fix44 as Dictionary>::group_delimiter(b"D", 453),
        Some(448),
        "and 453 is a counter this message type declares"
    );
    assert_eq!(
        <Fix44 as Tables>::enum_allows(447, b"ZZ"),
        Some(false),
        "PartyIDSource(447) is enumerated and `ZZ` is not one of its 18 values"
    );
}

#[test]
fn exec_inst_is_the_multi_value_field_this_file_assumes() {
    assert_eq!(
        <Fix44 as Tables>::field_type(18),
        Some(fixbolt_dict::FieldType::MultipleValueString),
        "ExecInst(18) is MULTIPLEVALUESTRING in FIX 4.4"
    );
    assert_eq!(<Fix44 as Tables>::enum_allows(18, b"2"), Some(true), "WORK");
    assert_eq!(
        <Fix44 as Tables>::enum_allows(18, b"A"),
        Some(true),
        "NO_CROSS"
    );
    // **Not `Z`.** `Z` *is* one of ExecInst's 40 FIX 4.4 values
    // (`CANCEL_IF_NOT_BEST`), so `18=2 Z` is legal under the per-token rule and would
    // make a useless negative. `T` is the one single-character code the
    // FIX 4.4 list skips between `S` and `U`.
    assert_eq!(
        <Fix44 as Tables>::enum_allows(18, b"Z"),
        Some(true),
        "Z is a real ExecInst value, which is why the negative below is T"
    );
    assert_eq!(
        <Fix44 as Tables>::enum_allows(18, b"T"),
        Some(false),
        "T is not"
    );
}

// ---------------------------------------------------------------------------
// ADR-0084 decision 2 — the count is answered before a member's value.
// ---------------------------------------------------------------------------

#[test]
fn a_bad_count_is_answered_before_a_bad_member_value() {
    // `453=3` with two entries, and `447=ZZ` inside the first. Both QuickFIX
    // engines answer `373=16` naming the counter: the count is a top-level
    // question and a member's value is not. Before ADR-0084 decision 2 this
    // engine walked the flat index in wire order and answered `373=5` naming
    // `447`, because `447` comes first on the wire.
    let mut s = logged_on();
    let wire = new_order_single(&format!(
        "{REQUIRED}453=3|448=A|447=ZZ|452=1|448=B|447=D|452=2|"
    ));
    let (link, out) = answer(&mut s, &wire);

    assert_eq!(link, Link::Up, "a Reject does not end the session");
    assert_eq!(out.len(), 1, "one Reject: {out:?}");
    assert!(out[0].contains("|35=3|"), "and it is a Reject: {out:?}");
    assert!(
        out[0].contains("|373=16|"),
        "IncorrectNumInGroupCount, not the member's value: {out:?}"
    );
    assert!(
        out[0].contains("|371=453|"),
        "naming the counter, not the member: {out:?}"
    );
}

#[test]
fn a_member_value_is_still_refused_once_the_count_agrees() {
    // The other half, and the one that goes red if someone later "fixes" the
    // ordering the QuickFIX C++ way by dropping member checks altogether.
    // `453=2` with two entries: the count is right, so the fourth pass runs
    // and `447=ZZ` is `373=5` naming `447`.
    let mut s = logged_on();
    let wire = new_order_single(&format!(
        "{REQUIRED}453=2|448=A|447=ZZ|452=1|448=B|447=D|452=2|"
    ));
    let (link, out) = answer(&mut s, &wire);

    assert_eq!(link, Link::Up, "a Reject does not end the session");
    assert_eq!(out.len(), 1, "one Reject: {out:?}");
    assert!(out[0].contains("|35=3|"), "and it is a Reject: {out:?}");
    assert!(
        out[0].contains("|373=5|"),
        "ValueIsIncorrect on the member: {out:?}"
    );
    assert!(out[0].contains("|371=447|"), "naming the member: {out:?}");
}

#[test]
fn a_group_whose_count_and_values_agree_is_accepted() {
    // The neutral case, so the two above are not both passing because groups
    // are rejected on sight. `453=2`, two well-formed entries, nothing wrong.
    let mut s = logged_on();
    let wire = new_order_single(&format!(
        "{REQUIRED}453=2|448=A|447=B|452=1|448=B|447=D|452=2|"
    ));
    let (link, out) = answer(&mut s, &wire);

    assert_eq!(link, Link::Up);
    assert!(out.is_empty(), "answered with nothing: {out:?}");
    assert_eq!(s.next_in(), 3, "and it counted, so it was accepted");
}

#[test]
fn a_required_tag_is_answered_before_a_bad_member_value() {
    // The `373=1` half of the same sentence. `60=SendingTime` is required on a
    // `NewOrderSingle`; drop it and the answer is `373=1` naming `60`, even
    // though `447=ZZ` sits in a well-counted group and would otherwise be
    // `373=5`. `missing_required` runs after the scan in this engine (`14d`
    // pins that), so this is the one arm whose order the corpus does fix.
    let mut s = logged_on();
    // `REQUIRED` without its `60=`.
    let wire = new_order_single(
        "11=ID|21=1|38=002000.00|40=1|54=1|55=INTC|\
         453=2|448=A|447=ZZ|452=1|448=B|447=D|452=2|",
    );
    let (_, out) = answer(&mut s, &wire);

    assert_eq!(out.len(), 1, "one Reject: {out:?}");
    assert!(
        out[0].contains("|373=1|"),
        "RequiredTagMissing wins over the member's value: {out:?}"
    );
    assert!(out[0].contains("|371=60|"), "naming SendingTime: {out:?}");
}

#[test]
fn a_top_level_bad_value_still_wins_inside_the_scan() {
    // Nothing moved for a field that is **not** a group member: `40=Z` is not
    // an OrdType, and it is answered `373=5` from the wire-order scan, ahead
    // of the `373=16` the bad count below it would otherwise produce. Without
    // this the change could have been "run every value check last", which is
    // a different rule from the one ADR-0084 decision 2 states.
    let mut s = logged_on();
    let wire = new_order_single(
        "11=ID|21=1|38=002000.00|40=Z|54=1|55=INTC|60=20260828-12:00:00.000|\
         453=3|448=A|447=B|452=1|448=B|447=D|452=2|",
    );
    let (_, out) = answer(&mut s, &wire);

    assert_eq!(out.len(), 1, "one Reject: {out:?}");
    assert!(out[0].contains("|373=5|"), "ValueIsIncorrect: {out:?}");
    assert!(
        out[0].contains("|371=40|"),
        "naming the top-level field, not the counter: {out:?}"
    );
}

#[test]
fn a_member_of_a_group_the_message_does_not_carry_is_checked_in_the_scan() {
    // `447` outside any group: no `453=` in the message, so no counter was
    // seen, the deferral never applies, and the scan answers `373=5` as it
    // always did. This is the "a message with no counter must never enter the
    // branch" half of ADR-0084 decision 2's shape.
    //
    // `447` alone on a `D` is allowed by the flat `allows` table — the member
    // lists are folded into it — so the fault this earns is the value one.
    let mut s = logged_on();
    let wire = new_order_single(&format!("{REQUIRED}447=ZZ|"));
    let (_, out) = answer(&mut s, &wire);

    assert_eq!(out.len(), 1, "one Reject: {out:?}");
    assert!(out[0].contains("|373=5|"), "ValueIsIncorrect: {out:?}");
    assert!(out[0].contains("|371=447|"), "naming 447: {out:?}");
}

// ---------------------------------------------------------------------------
// ADR-0083 decision 1's second rule, on the FIX 4.4 table.
// ---------------------------------------------------------------------------

#[test]
fn a_two_value_exec_inst_is_accepted() {
    // `18=2 A` — WORK and NO_CROSS — is what the FIX 4.4 specification
    // spells *"one or more space delimited multiple character values"*, and
    // both QuickFIX engines split before they look. The whole-value match this
    // engine did before answered `373=5` on a legal order.
    let mut s = logged_on();
    let wire = new_order_single(&format!("{REQUIRED}18=2~A|"));
    let (link, out) = answer(&mut s, &wire);

    assert_eq!(link, Link::Up);
    assert!(out.is_empty(), "no Reject: {out:?}");
    assert_eq!(s.next_in(), 3, "and it counted, so it was accepted");
}

#[test]
fn a_two_value_exec_inst_with_one_bad_token_is_refused() {
    // Splitting is not amnesty: **every** token must be in the list. `T` is
    // the code FIX 4.4's ExecInst list skips.
    let mut s = logged_on();
    let wire = new_order_single(&format!("{REQUIRED}18=2~T|"));
    let (_, out) = answer(&mut s, &wire);

    assert_eq!(out.len(), 1, "one Reject: {out:?}");
    assert!(out[0].contains("|373=5|"), "ValueIsIncorrect: {out:?}");
    assert!(out[0].contains("|371=18|"), "naming ExecInst: {out:?}");
}

#[test]
fn a_single_value_exec_inst_is_still_accepted() {
    // The rule must not have loosened, or tightened, the one-token case —
    // which is how every other enumerated field in the dictionary is read.
    let mut s = logged_on();
    let (_, out) = answer(&mut s, &new_order_single(&format!("{REQUIRED}18=2|")));
    assert!(out.is_empty(), "`18=2` is legal: {out:?}");
}

#[test]
fn a_single_bad_value_exec_inst_is_still_refused() {
    let mut s = logged_on();
    let (_, out) = answer(&mut s, &new_order_single(&format!("{REQUIRED}18=T|")));
    assert_eq!(out.len(), 1, "`18=T` is not: {out:?}");
    assert!(out[0].contains("|373=5|"), "ValueIsIncorrect: {out:?}");
    assert!(out[0].contains("|371=18|"), "naming ExecInst: {out:?}");
}

// ---------------------------------------------------------------------------
// The header's own repeating group, which every message carries.
// ---------------------------------------------------------------------------

/// A `Heartbeat` carrying `body` in its header, framed with a real `9=`/`10=`.
fn heartbeat(body: &str) -> Vec<u8> {
    let fields =
        format!("35=0|34=2|49=TW44|52=20260828-12:00:00.000|56=ISLD|{body}").replace(' ', "");
    let wire = format!("8=FIX.4.4|9={}|{fields}10=0|", fields.len()).replace('|', "\u{1}");
    with_real_checksum(wire.as_bytes())
}

#[test]
fn the_header_group_is_on_every_message_type_including_a_heartbeat() {
    // The premise that decides where the deferral may be gated. `NoHops(627)`
    // is declared in `FIX44.xml`'s `<header>`, so the generated table answers
    // it for **every** message type — the `627 => &G0, // <header>` arm is
    // matched on the counter alone, not on the message. A `Heartbeat` can
    // therefore carry a populated repeating group, and "does this message type
    // declare a group?" is `true` for all 93 FIX 4.4 message types.
    assert_eq!(
        <Fix44 as Dictionary>::group_delimiter(b"0", 627),
        Some(628),
        "NoHops is a header group, so a Heartbeat has one"
    );
    assert_eq!(
        <Fix44 as Dictionary>::group_members(b"0", 627),
        &[628, 629, 630],
        "HopCompID, HopSendingTime, HopRefID"
    );
    assert!(
        <Fix44 as Tables>::allows(b"0", 630),
        "and its members are allowed on a Heartbeat"
    );
}

#[test]
fn a_bad_hop_count_is_answered_before_a_bad_hop_value_on_a_heartbeat() {
    // ADR-0084 decision 2 applies to the header group exactly as it does to a
    // body one, and this is the message that proves a per-message-type gate on
    // the deferral cannot be exact: `627=3` with two hops and
    // `630=XX`, where `HopRefID(630)` is a `SEQNUM` and `XX` is not a number.
    //
    // The count must win. If the wire-order scan were allowed to skip the
    // deferral for message types that "declare no group", a `Heartbeat` would
    // answer `373=6` naming `630` instead — which is this test going red.
    let mut s = logged_on();
    let wire = heartbeat(
        "627=3|628=A|629=20260828-12:00:00.000|630=XX|\
         628=B|629=20260828-12:00:00.000|630=2|",
    );
    let (link, out) = answer(&mut s, &wire);

    assert_eq!(link, Link::Up, "a Reject does not end the session");
    assert_eq!(out.len(), 1, "one Reject: {out:?}");
    assert!(
        out[0].contains("|373=16|"),
        "IncorrectNumInGroupCount, not the hop's format: {out:?}"
    );
    assert!(out[0].contains("|371=627|"), "naming NoHops: {out:?}");
}

#[test]
fn a_hop_value_is_still_refused_once_the_hop_count_agrees() {
    // The other half, on the header group: `627=2` with two hops and the same
    // unreadable `630=XX` is `373=6` naming `630`, from the fourth pass.
    let mut s = logged_on();
    let wire = heartbeat(
        "627=2|628=A|629=20260828-12:00:00.000|630=XX|\
         628=B|629=20260828-12:00:00.000|630=2|",
    );
    let (_, out) = answer(&mut s, &wire);

    assert_eq!(out.len(), 1, "one Reject: {out:?}");
    assert!(
        out[0].contains("|373=6|"),
        "IncorrectDataFormat on the hop: {out:?}"
    );
    assert!(out[0].contains("|371=630|"), "naming HopRefID: {out:?}");
}

// ---------------------------------------------------------------------------
// A nested counter must not end the `373=16` pass.
// ---------------------------------------------------------------------------

#[test]
fn the_nested_party_sub_group_is_the_shape_this_file_assumes() {
    // The premise of the test below, verified rather than trusted:
    // `NoPartySubIDs(802)` is a member of `NoPartyIDs(453)` **and** a counter
    // in its own right, so the flat `(msg_type, counter)` table answers it on
    // a `D` while `MessageView::group` — a top-level API
    // (`crates/codec/src/group.rs:165-176`) — cannot build it.
    assert_eq!(
        <Fix44 as Dictionary>::group_members(b"D", 453),
        &[448, 447, 452, 802],
        "802 is a member of the party group"
    );
    assert_eq!(
        <Fix44 as Dictionary>::group_delimiter(b"D", 802),
        Some(523),
        "and the flat table calls 802 a counter on a D all the same"
    );
    assert_eq!(
        <Fix44 as Dictionary>::group_delimiter(b"D", 386),
        Some(336),
        "NoTradingSessions(386) is the top-level counter that follows it"
    );
}

#[test]
fn a_counter_after_a_nested_group_is_still_checked() {
    // The defect this row exists for: `bad_group_count` used `?` on
    // `view.group::<D>(..)`, and a `None` there means "no fault" for the whole
    // message. `802` is nested, so the view answers `None` for it — and every
    // counter *after* it went unchecked.
    //
    // Here the party group is well-formed and carries a nested `802=1`, and
    // `386=3` below it declares three trading sessions while sending one.
    // The answer must be `373=16` naming `386`.
    let mut s = logged_on();
    let wire = new_order_single(&format!(
        "{REQUIRED}453=1|448=A|447=B|452=1|802=1|523=SUB|803=1|386=3|336=X|"
    ));
    let (link, out) = answer(&mut s, &wire);

    assert_eq!(link, Link::Up, "a Reject does not end the session");
    assert_eq!(
        out.len(),
        1,
        "expected Reject 373=16, engine sent no reject"
    );
    assert!(out[0].contains("|35=3|"), "and it is a Reject: {out:?}");
    assert!(
        out[0].contains("|373=16|"),
        "IncorrectNumInGroupCount on the counter behind the nested group: {out:?}"
    );
    assert!(
        out[0].contains("|371=386|"),
        "naming NoTradingSessions: {out:?}"
    );
}
