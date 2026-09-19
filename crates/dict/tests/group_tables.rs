//! The generated group tables.
//!
//! The load-bearing property is that they are keyed by **(msg_type, counter)**
//! and not by counter alone. Four counters take different delimiters in
//! different messages, and `268` is the painful one: market-data snapshot and
//! incremental refresh share the counter and disagree on the delimiter, so a
//! flat table silently mis-cuts every incremental refresh — the highest-volume
//! message type there is.
#![allow(clippy::unwrap_used, clippy::panic)]

use fixbolt_codec::Dictionary;
use fixbolt_dict::Fix44;

#[test]
fn the_four_ambiguous_counters_resolve_by_message() {
    // 268 NoMDEntries
    assert_eq!(
        Fix44::group_delimiter(b"W", 268),
        Some(269),
        "MarketDataSnapshotFullRefresh"
    );
    assert_eq!(
        Fix44::group_delimiter(b"X", 268),
        Some(279),
        "MarketDataIncrementalRefresh"
    );
    // 124 NoExecs
    assert_eq!(
        Fix44::group_delimiter(b"J", 124),
        Some(32),
        "AllocationInstruction"
    );
    assert_eq!(
        Fix44::group_delimiter(b"BA", 124),
        Some(17),
        "CollateralReport"
    );
    // 295 NoQuoteEntries
    assert_eq!(Fix44::group_delimiter(b"Z", 295), Some(55), "QuoteCancel");
    assert_eq!(Fix44::group_delimiter(b"i", 295), Some(299), "MassQuote");
    // 420 NoBidComponents
    assert_eq!(Fix44::group_delimiter(b"k", 420), Some(66), "BidRequest");
    assert_eq!(Fix44::group_delimiter(b"l", 420), Some(12), "BidResponse");
}

#[test]
fn a_group_reached_through_a_component_is_still_found() {
    // NoTradingSessions(386) reaches NewOrderSingle only through TrdgSesGrp.
    // A generator that reads <message> children alone finds nothing.
    assert_eq!(Fix44::group_delimiter(b"D", 386), Some(336));
    assert_eq!(Fix44::group_members(b"D", 386), &[336, 625]);
}

#[test]
fn the_header_group_belongs_to_every_message() {
    // NoHops(627) is declared in <header>, so it can appear in any message.
    // Keying it to one msg_type would lose it for the other 92.
    for mt in [b"0".as_ref(), b"A".as_ref(), b"D".as_ref(), b"BA".as_ref()] {
        assert_eq!(Fix44::group_delimiter(mt, 627), Some(628), "for {mt:?}");
    }
    assert_eq!(Fix44::group_members(b"D", 627), &[628, 629, 630]);
}

#[test]
fn an_unknown_pair_is_none_not_a_guess() {
    assert_eq!(
        Fix44::group_delimiter(b"D", 268),
        None,
        "no market data in an order"
    );
    assert_eq!(Fix44::group_delimiter(b"D", 99999), None);
    assert_eq!(Fix44::group_members(b"D", 268), &[] as &[u32]);
}

#[test]
fn order_is_declaration_order_with_the_delimiter_first() {
    // NOT ascending by tag: 269 comes before 270 here by declaration, and the
    // delimiter is first by definition. Verified against libquickfix output at
    // step 5 of the plan; until then this pins what the XML says.
    let order = Fix44::group_order(b"W", 268);
    assert_eq!(order.first(), Some(&269), "delimiter leads");
    assert_eq!(
        Fix44::group_members(b"W", 268),
        order,
        "members and order are one list"
    );
}

#[test]
fn the_tables_cover_what_the_dictionary_declares() {
    // 93 <group> declarations, 59 distinct counter tags including NoHops(627)
    // from the header, 731 positions once components are expanded.
    //
    // The plan said 1028 positions and could not be reproduced by any counting
    // method tried on 2026-08-28; 731 is what the file contains. See the
    // delivery log.
    assert_eq!(fixbolt_dict::GROUP_COUNTERS, 59);
    assert_eq!(fixbolt_dict::GROUP_POSITIONS, 731);
}

/// Every group counter is a `NUMINGROUP`, in both generated tables.
///
/// Not a curiosity: `crates/session/src/lib.rs`'s `scan_fields` tests
/// `field_type(tag) == Some(FieldType::NumInGroup)` before it asks
/// `group_delimiter` at all, because a `NumInGroup` comparison is on a value
/// already in hand and `group_delimiter` is a two-level match. If a dictionary
/// ever declared a `<group>` on a field of some other type, that counter would
/// never be recorded, the members behind it would keep their value checked in
/// wire order, and ADR-0084 decision 2's ordering would silently stop applying
/// to them. This is the test that would go red first.
///
/// `[measured 2026-09-19]` 93 `<group>` declarations in `FIX44.xml`, 2 in
/// `FIXT11.xml`, 561 in `FIX50SP2.xml`; none on a field of any other type.
#[test]
fn every_group_counter_is_a_num_in_group() {
    use fixbolt_dict::{FieldType, Tables};

    for (mt, counter) in &fixbolt_dict::GROUP_KEYS {
        assert_eq!(
            <Fix44 as Tables>::field_type(*counter),
            Some(FieldType::NumInGroup),
            "FIX 4.4 counter {counter} on {}",
            String::from_utf8_lossy(mt)
        );
    }

    #[cfg(feature = "fix50sp2")]
    for (mt, counter) in &fixbolt_dict::fixt11_fix50sp2::GROUP_KEYS {
        assert_eq!(
            <fixbolt_dict::Fixt11Fix50Sp2Tables as Tables>::field_type(*counter),
            Some(FieldType::NumInGroup),
            "FIXT 1.1 / FIX 5.0 SP2 counter {counter} on {}",
            String::from_utf8_lossy(mt)
        );
    }
}
