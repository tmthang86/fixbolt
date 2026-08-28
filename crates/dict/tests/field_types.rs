//! What each of the 23 FIX 4.4 field types will and will not accept.
//!
//! **These cases are written by hand, not taken from a capture.** `CLAUDE.md`
//! §7 prefers real messages, and the acceptance corpus supplies exactly two:
//! `38=+200.00` and `126=20040415`, both in the `14f` family. Twenty-three types
//! and two real cases is not coverage, so the rest of this file is invented and
//! says so.
//!
//! Each type gets at least one accepted value and one refused one. A type that
//! accepts everything is the failure mode — `Reject 373=6` then never fires and
//! nothing in the 59 definitions notices, because only two of them look.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use nanofix_dict::{FieldType, Fix44};

/// The two the corpus actually supplies. Everything else here is invented.
#[test]
fn the_two_cases_the_corpus_supplies() {
    // 14f_IncorrectDataFormat.def: OrderQty is QTY and `+200.00` is refused.
    assert_eq!(Fix44::field_type(38), Some(FieldType::Qty));
    assert!(!FieldType::Qty.accepts(b"+200.00"), "the sign is the fault");
    assert!(FieldType::Qty.accepts(b"200.00"));
    assert!(FieldType::Qty.accepts(b"002000.00"), "2a sends this one");

    // RejectResentMessage.def: ExpireTime is UTCTIMESTAMP and a bare date is not
    // one.
    assert_eq!(Fix44::field_type(126), Some(FieldType::UtcTimestamp));
    assert!(!FieldType::UtcTimestamp.accepts(b"20040415"));
    assert!(FieldType::UtcTimestamp.accepts(b"20040415-12:00:00"));
}

#[test]
fn every_type_refuses_something() {
    // The whole point. A type that accepts everything makes `373=6` dead code.
    let cases: &[(FieldType, &[u8], &[u8])] = &[
        (FieldType::Int, b"42", b"4.2"),
        (FieldType::Length, b"9", b"-9"),
        (FieldType::SeqNum, b"1", b"0"),
        (FieldType::NumInGroup, b"3", b""),
        (FieldType::Float, b"-1.5", b"1.5.5"),
        (FieldType::Qty, b"100", b"1e5"),
        (FieldType::Price, b"12.25", b"twelve"),
        (FieldType::PriceOffset, b"-0.25", b"- 0.25"),
        (FieldType::Amt, b"1000.00", b"$1000"),
        (FieldType::Percentage, b"0.0525", b"5%"),
        (FieldType::Char, b"1", b"12"),
        (FieldType::Boolean, b"Y", b"1"),
        (FieldType::String, b"MSFT", b""),
        (FieldType::MultipleValueString, b"A B", b""),
        (FieldType::Currency, b"USD", b"US"),
        (FieldType::Country, b"US", b"USA"),
        (FieldType::Exchange, b"N", b""),
        (FieldType::MonthYear, b"202608", b"2026-08"),
        (FieldType::LocalMktDate, b"20260828", b"2026-08-28"),
        (FieldType::UtcDateOnly, b"20260828", b"260828"),
        (FieldType::UtcTimeOnly, b"12:00:00", b"12:00"),
        (
            FieldType::UtcTimestamp,
            b"20260828-12:00:00",
            b"20260828 12:00:00",
        ),
    ];
    assert_eq!(
        cases.len(),
        22,
        "22 of the 23 FIX 4.4 types. DATA is the exception and has its own test"
    );

    for (ty, good, bad) in cases {
        assert!(ty.accepts(good), "{ty:?} should accept {good:?}");
        assert!(!ty.accepts(bad), "{ty:?} should refuse {bad:?}");
    }
    assert!(
        !cases.iter().any(|(t, _, _)| *t == FieldType::Data),
        "DATA belongs in the test below, not this one"
    );
}

#[test]
fn data_is_the_one_type_that_accepts_anything_and_that_is_correct() {
    // A DATA field is delimited by the length field in front of it, so its
    // bytes are whatever the length says — including `0x01`, including none.
    // There is no format to be wrong about. This is written as its own test
    // because "every type refuses something" is a rule with exactly one
    // exception, and an exception buried in a table is an exception nobody
    // reads.
    for v in [&b""[..], b"\x01\x02raw", b"=", b"8=FIX.4.4\x01"] {
        assert!(FieldType::Data.accepts(v), "DATA refused {v:?}");
    }
    assert_eq!(Fix44::field_type(96), Some(FieldType::Data), "RawData");
}

#[test]
fn an_empty_value_is_never_a_value_except_for_data() {
    // `14d_TagSpecifiedWithoutValue.def` sends `56=` and expects `373=4`, which
    // is a different code from `373=6`. So the session checks emptiness before
    // it checks the type, and `accepts` is not what holds that rule — but it
    // must not disagree with it either.
    for tag in [38u32, 126, 55, 34, 43] {
        let ty = Fix44::field_type(tag).expect("a known tag has a type");
        assert!(
            !ty.accepts(b""),
            "tag {tag} is {ty:?} and accepted an empty value"
        );
    }
    // DATA is the exception the parser already handles by length, not by
    // scanning: a zero-length RawData is a legal field.
    assert!(FieldType::Data.accepts(b""));
}

#[test]
fn an_undefined_tag_has_no_type() {
    for tag in [0u32, 999, 5000, 957, u32::MAX] {
        assert_eq!(Fix44::field_type(tag), None, "tag {tag}");
    }
}
