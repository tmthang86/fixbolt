//! The FIXT 1.1 / FIX 5.0 SP2 table, built from **two** XML files.
//!
//! Plan row B1 of `docs/plans/2026-09-19-phase-2-fixt-and-sbe.md`, as amended by
//! [ADR-0083]. Six facts measured on 2026-09-19 stop the naive pair build, and
//! four of them are only visible from here: this file is the guard the ADR
//! names for decisions 2, 4 and 5.
//!
//! Every assertion below is a *rule* rather than a spot check, so each one says
//! which decision would silently stop holding if it went red.
//!
//! [ADR-0083]: ../../../docs/decisions/ADR-0083-ten-field-types-one-field-spelled-two-ways-one-empty-component-and-where-the-sp2-oracle-comes-from.md
#![cfg(feature = "fix50sp2")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use fixbolt_codec::Dictionary as _;
use fixbolt_dict::{FieldType, Fixt11Fix50Sp2Tables as Fixt, Tables as _};

/// The FIXT 1.1 `<header>`: 29 direct `<field>` children, plus `NoHops(627)`
/// and the three fields inside it.
///
/// `[measured 2026-09-19]` at pin `386ce46e`. Taking only the direct children
/// would put the four hop tags in the message BODY when writing, which is
/// `CLAUDE.md` §2 item 5's exact failure mode — the trap `build.rs` already
/// carries for FIX 4.4 and which FIXT 1.1 repeats field for field.
#[test]
fn the_header_is_the_twenty_nine_fields_and_the_hop_group() {
    const DIRECT: [u32; 29] = [
        8, 9, 34, 35, 43, 49, 50, 52, 56, 57, 90, 91, 97, 115, 116, 122, 128, 129, 142, 143, 144,
        145, 212, 213, 347, 369, 1128, 1129, 1156,
    ];
    // The counter and its three members. `is_header` must answer for all four.
    const HOPS: [u32; 4] = [627, 628, 629, 630];

    for tag in DIRECT.into_iter().chain(HOPS) {
        assert!(Fixt::is_header(tag), "tag {tag} is a header field");
    }
    // The set, not only the count: a table that answered `true` for everything
    // would pass the loop above.
    let counted = (0..=50_002u32).filter(|t| Fixt::is_header(*t)).count();
    assert_eq!(
        counted,
        DIRECT.len() + HOPS.len(),
        "33 header tags, and nothing else"
    );
}

/// Admin messages come from `FIXT11.xml`, application messages from
/// `FIX50SP2.xml`, and the two sets do not overlap.
#[test]
fn both_halves_of_the_pair_reached_the_message_table() {
    // Logon, from the transport file.
    assert!(Fixt::is_msg_type(b"A"));
    // NewOrderSingle, from the application file.
    assert!(Fixt::is_msg_type(b"D"));
    assert!(!Fixt::is_msg_type(b"ZZ"));

    // 8 + 156. A message type declared in both files would `die` in the build
    // (`two messages share msgtype`), so this count is also the proof that the
    // two halves are disjoint.
    let printable: Vec<Vec<u8>> = (0x20u8..0x7f)
        .flat_map(|a| std::iter::once(vec![a]).chain((0x20u8..0x7f).map(move |b| vec![a, b])))
        .collect();
    let found = printable.iter().filter(|mt| Fixt::is_msg_type(mt)).count();
    assert_eq!(found, 164, "8 admin + 156 application message types");
}

/// `DefaultApplVerID(1137)` is required on a FIXT 1.1 Logon, and `1128`,
/// `1156` are fields the FIX 4.4 table has never heard of.
#[test]
fn the_fixt_session_fields_are_in_the_table() {
    assert!(
        Fixt::required(b"A").contains(&1137),
        "Logon requires DefaultApplVerID"
    );
    assert!(Fixt::is_defined_tag(1128), "ApplVerID");
    assert_eq!(
        Fixt::field_type(1156),
        Some(FieldType::Int),
        "ApplExtID is an INT"
    );
}

/// **ADR-0083 decision 2.** `XmlData(213)` is `DATA` in `FIXT11.xml` and
/// `XMLDATA` in `FIX50SP2.xml`.
///
/// Because the pair build compares the `FieldType` **variant** and not the XML
/// spelling, the two are one type, the table says `Data`, and the length
/// pairing works from either file — with no exception row and no winner. If
/// this went red the merge would be comparing spellings again, and the build
/// would have died long before the test ran.
#[test]
fn the_field_spelled_two_ways_needs_no_exception() {
    assert_eq!(Fixt::field_type(213), Some(FieldType::Data));
    assert_eq!(Fixt::data_length_tag(213), Some(212));
    // The other spelling, on a field only FIX50SP2 has: SecurityXML(1185) is
    // `XMLDATA`, and mapping it to STRING would split it on an embedded 0x01 —
    // which is what both QuickFIX engines do at the pin.
    assert_eq!(Fixt::field_type(1185), Some(FieldType::Data));
    assert_eq!(Fixt::data_length_tag(1185), Some(1184));
}

/// **ADR-0083 decision 5.** The one DATA field the `{name}Len` /
/// `{name}Length` rule cannot pair.
///
/// `EncodedUnderlyingMarketDisruptionFallbackUnderlierSecurityDesc(41874)`
/// takes `…SecDescLen(41873)` — `Security` abbreviated to `Sec`. A `tag - 1`
/// fallback would have been right here and wrong for twelve other fields, so
/// the pairing is a named exception checked in both directions.
#[test]
fn the_one_named_length_exception_is_in_the_table() {
    assert_eq!(Fixt::data_length_tag(41874), Some(41873));
    // And the rule it is an exception to still does the other 82 by name.
    assert_eq!(Fixt::data_length_tag(96), Some(95), "RawData/RawDataLength");
    assert_eq!(
        Fixt::data_length_tag(41812),
        Some(41811),
        "the nearest DATA field that the name rule does pair"
    );
}

/// **ADR-0083 decision 4.** `MsgTypeGrp` is `<component name='MsgTypeGrp' />`
/// — self-closing, no children — in `FIXT11.xml` and full in `FIX50SP2.xml`.
///
/// Resolving the admin messages against the transport file's component map
/// alone, which is ADR-0080 decision 2 read literally, drops `NoMsgTypes(384)`
/// from the Logon table **in silence**: no `.def` in the three FIXT corpora
/// carries `384=`, so 180 acceptance files would stay green while a
/// counterparty negotiating per-message-type versions was answered `373=13`
/// for a repeated `372`. One component map, and the full definition wins.
#[test]
fn the_logon_carries_the_msg_type_group() {
    assert!(Fixt::allows(b"A", 384), "NoMsgTypes may appear on a Logon");
    assert_eq!(Fixt::group_delimiter(b"A", 384), Some(372));
    assert_eq!(
        Fixt::group_order(b"A", 384),
        [372, 385, 1130, 1406, 1131, 1410],
        "RefMsgType, MsgDirection, RefApplVerID, RefApplExtID, RefCstmApplVerID, \
         DefaultVerIndicator — declaration order, delimiter first"
    );
    // The other shared component, `HopGrp`, is byte-identical in the two files
    // and merges with no message at all.
    assert_eq!(Fixt::group_order(b"", 627), [628, 629, 630]);
}

/// **ADR-0083 decision 2, the enum half.** Four of the 71 shared fields carry
/// different value lists, and in every case FIX50SP2's set is a superset.
///
/// The table carries the superset, so a `10` on `ApplVerID` — FIX 5.0 SP2
/// itself — is not answered `373=5` merely because `FIXT11.xml` was written
/// before it existed.
#[test]
fn the_superset_enum_list_is_the_one_in_the_table() {
    assert_eq!(Fixt::enum_allows(1128, b"10"), Some(true), "ApplVerID");
    assert_eq!(Fixt::enum_allows(1409, b"9"), Some(true), "SessionStatus");
    assert_eq!(Fixt::enum_allows(1409, b"10"), Some(true));
    // Still a real check, not a table that says yes to everything.
    assert_eq!(Fixt::enum_allows(1128, b"99"), Some(false));
    // A field enumerated on the SP2 side only carries that side's list.
    assert_eq!(Fixt::enum_allows(8, b"FIXT.1.1"), Some(true), "BeginString");
    // Not enumerated is `None`, which is a different answer from `Some(false)`.
    assert_eq!(Fixt::enum_allows(34, b"1"), None, "MsgSeqNum");
}

/// **ADR-0083 decision 1, second rule.** On a multi-value type the enum check
/// is per token.
///
/// `18=2 A` is one legal `MULTIPLECHARVALUE` holding two values, and both
/// QuickFIX C++ and QuickFIX/J split on spaces before they look. This table is
/// built to that rule; the FIX 4.4 table keeps the whole-value check until the
/// plan row that changes its session behaviour lands, which is why the last two
/// assertions are here and not in `enums.rs`.
#[test]
fn a_multi_value_field_is_checked_one_token_at_a_time() {
    assert_eq!(
        Fixt::enum_allows(18, b"2"),
        Some(true),
        "ExecInst, one value"
    );
    assert_eq!(Fixt::enum_allows(18, b"2 A"), Some(true), "two values");
    assert_eq!(Fixt::enum_allows(18, b"2 A F"), Some(true), "three");
    // One bad token is enough.
    assert_eq!(Fixt::enum_allows(18, b"2 \x7f"), Some(false));
    // A MULTIPLESTRINGVALUE field behaves the same way.
    assert_eq!(Fixt::enum_allows(277, b"A B"), Some(true), "TradeCondition");
    // And a single-valued enumerated field is untouched by the split.
    assert_eq!(Fixt::enum_allows(54, b"1"), Some(true), "Side");
    assert_eq!(
        Fixt::enum_allows(54, b"1 2"),
        Some(false),
        "not multi-value"
    );
}

/// The trait impls delegate to the generated functions, and to the *right*
/// ones — the failure mode `tables.rs`'s own test exists for.
#[test]
fn the_dictionary_and_tables_impls_answer_from_the_pair_table() {
    // `Dictionary`, the parse path's half.
    assert!(<Fixt as fixbolt_codec::Dictionary>::is_header(1128));
    assert_eq!(
        <Fixt as fixbolt_codec::Dictionary>::group_members(b"A", 384).len(),
        6
    );
    // `Tables`, the validation pass's half.
    assert_eq!(
        <Fixt as fixbolt_dict::Tables>::field_type(213),
        Some(FieldType::Data)
    );
    assert!(<Fixt as fixbolt_dict::Tables>::is_msg_type(b"D"));
    // A tag neither dictionary defines. 50002 is the highest FIX50SP2 carries.
    assert!(Fixt::is_defined_tag(50_002));
    assert!(!Fixt::is_defined_tag(50_003));
    assert_eq!(Fixt::field_type(50_003), None);
}

/// **ADR-0084 decision 1.** A message is validated against the tag set of the
/// layer that defines it.
///
/// `14a_BadField.def` sends `999=HI` on a `35=0` Heartbeat and expects `373=0`,
/// *Invalid tag number*. `999` is `LegUnitOfMeasure` in `FIX50SP2.xml` and
/// absent from `FIXT11.xml`, so the merged table's `is_defined_tag` says `true`
/// and the engine would answer `373=2` instead. Both QuickFIX engines validate
/// an admin body against the transport dictionary alone, and this is the table
/// half of saying the same thing: the transport bitset and the transport
/// message list are **generated from one file**, so "defined by the transport
/// file" and "a message of the transport file" cannot drift apart (`DESIGN.md`
/// D3 — the rule lives in a table, never at a call site).
#[test]
fn a_session_message_is_checked_against_the_transport_layers_tags() {
    use fixbolt_dict::fixt11_fix50sp2::{is_transport_message, is_transport_tag};

    // The transport file's 71 fields, and nothing else. `[measured 2026-09-19]`
    // at pin `386ce46e`: highest tag 1409, so the bitset is 23 `u64` words.
    assert!(
        !is_transport_tag(999),
        "LegUnitOfMeasure is FIX50SP2's, not FIXT11's"
    );
    assert!(is_transport_tag(1137), "DefaultApplVerID");
    assert!(is_transport_tag(35), "MsgType");
    assert!(!is_transport_tag(55), "Symbol is an application field");

    // The eight transport messages, `0 1 2 3 4 5 A n` — every one `msgcat`
    // `admin`. Read from the transport file's `<messages>`, never from the
    // session's hand-written `ADMIN` const, which has seven entries and lacks
    // `n`.
    assert!(is_transport_message(b"n"), "XMLnonFIX");
    assert!(!is_transport_message(b"D"), "NewOrderSingle");

    // And the question the session actually asks.
    assert!(
        !<Fixt as fixbolt_dict::Tables>::is_defined_tag_for(b"0", 999),
        "`999` on a Heartbeat is 373=0"
    );
    assert!(
        <Fixt as fixbolt_dict::Tables>::is_defined_tag_for(b"D", 999),
        "`999` on a NewOrderSingle is a defined tag"
    );
}

/// **ADR-0084 decision 3, "What bidirectional drift does to this decision".**
/// Enumerations between FIX50, SP1 and SP2 drift **both ways**, not only the
/// strict direction `score_fixt`'s divergence assertion can see — a passing
/// file is never examined there, so nothing in the 180 `.def`s would ever
/// catch the permissive direction going missing. This test pins it directly
/// on the table, independent of any corpus.
///
/// `[measured 2026-09-19]` with `xml.etree` over `vendor/quickfix/spec/`,
/// re-verified against the built table below rather than assumed from the
/// ADR: `TradingSessionID(336)` is **stricter** under SP2 than FIX50 (0
/// values there, 7 here); `DeskOrderHandlingInst(1035)` is **more
/// permissive** — **24 values in `FIX50.xml`**, 0 in `FIX50SP2.xml` — so a
/// value FIX50 would refuse is not enumerated here at all;
/// `MarketUpdateAction(1395)` is the same shape one service pack later —
/// **3 values in `FIX50SP1.xml`**, 0 in `FIX50SP2.xml`. No `.def` in the 180
/// carries `1035=` or `1395=`, so this is the only guard on the permissive
/// direction until the FIX50 successor table (decision 3) exists — the day
/// it lands, this test gains its other half: `Some(false)` on that table for
/// the same two values.
#[test]
fn enum_drift_between_service_packs_is_pinned_in_both_directions() {
    // Stricter than FIX50: FIX50 has no enumeration for 336 at all
    // (`enum_allows` there is `None`), SP2 refuses `ONE_MAIN`.
    assert_eq!(
        Fixt::enum_allows(336, b"ONE_MAIN"),
        Some(false),
        "TradingSessionID: SP2 enumerates it (7 values) and refuses ONE_MAIN"
    );
    // More permissive than FIX50: DeskOrderHandlingInst, 24 values in
    // FIX50.xml, 0 in FIX50SP2.xml — SP2 does not enumerate it at all, so a
    // value FIX50 would answer `Some(false)` to is `None` here.
    assert_eq!(
        Fixt::enum_allows(1035, b"ZZZ"),
        None,
        "DeskOrderHandlingInst: 24 values in FIX50.xml, 0 in FIX50SP2.xml"
    );
    // More permissive than SP1: MarketUpdateAction, 3 values in
    // FIX50SP1.xml, 0 in FIX50SP2.xml.
    assert_eq!(
        Fixt::enum_allows(1395, b"Z"),
        None,
        "MarketUpdateAction: 3 values in FIX50SP1.xml, 0 in FIX50SP2.xml"
    );
}

/// `is_admin` and `is_transport_message` name the **same eight** message types.
///
/// `[measured 2026-09-20]` every `<message>` in `FIXT11.xml` carries
/// `msgcat='admin'` (8 of 8) and every one of the 156 in `FIX50SP2.xml` carries
/// `msgcat='app'` (0 admin). So on the pair table the two questions have the
/// same answer today — and that coincidence is exactly what must be pinned: the
/// day the application file ships an admin message, or the transport file an
/// app one, this test says so instead of letting two tables disagree silently.
///
/// The set includes `n` (XMLnonFIX), the element the hand-written seven-item
/// list beside the call site was missing.
#[test]
fn the_transport_files_admin_set_is_the_tables_admin_set() {
    use fixbolt_dict::fixt11_fix50sp2::is_transport_message;

    const TRANSPORT: [&[u8]; 8] = [b"0", b"1", b"2", b"3", b"4", b"5", b"A", b"n"];
    for mt in TRANSPORT {
        assert!(
            is_transport_message(mt),
            "{} is a message of FIXT11.xml",
            String::from_utf8_lossy(mt)
        );
        assert!(
            Fixt::is_admin(mt),
            "{} carries msgcat='admin'",
            String::from_utf8_lossy(mt)
        );
    }
    // The set, not only the eight: a table answering `true` for everything
    // would pass the loop above. **Every** one-byte and two-byte type that
    // ASCII can spell is asked — 128 + 128 × 128 = 16 512 of them — and the two
    // answers must never part. A `MsgType` outside ASCII is not something
    // either XML file can declare, so the sweep is exhaustive over what the
    // generated table can possibly answer `true` for.
    let mut admin = 0usize;
    let mut asked = 0usize;
    {
        let mut ask = |mt: &[u8]| {
            assert_eq!(
                Fixt::is_admin(mt),
                is_transport_message(mt),
                "msgcat='admin' and \"a message of FIXT11.xml\" part company on {}",
                String::from_utf8_lossy(mt)
            );
            asked += 1;
            if Fixt::is_admin(mt) {
                admin += 1;
            }
        };
        for a in 0u8..=127 {
            ask(&[a]);
            for b in 0u8..=127 {
                ask(&[a, b]);
            }
        }
    }
    assert_eq!(
        asked, 16_512,
        "every one- and two-byte ASCII type was asked"
    );
    assert_eq!(
        admin, 8,
        "FIXT11.xml has eight messages, all msgcat='admin'"
    );
    // An application message of the pair is not admin.
    assert!(!Fixt::is_admin(b"D"), "NewOrderSingle is msgcat='app'");
}
