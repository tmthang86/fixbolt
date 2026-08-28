//! Reading repeating groups off the flat index.
//!
//! The load-bearing question is not "how do I read entry 3" — it is **where
//! does the group end**. On a flat `FieldIndex` a group ends at the first tag
//! that is not one of its members, and a nested group's members are not members
//! of the outer group. So a scanner that does not skip nested regions cuts the
//! outer group short. `[measured 2026-08-28]` **235 of the 731 group positions
//! in FIX 4.4 contain a nested group** — 32%, not an edge case.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use nanofix_codec::{FieldIndex, Validation, parse_into};
use nanofix_dict::Fix44;

/// `|` for SOH; `9=` and `10=` computed. The group tests hand-write field
/// sequences because what is under test is *where the scan stops*, not whether
/// a packet looks like real traffic — and for depth-4 nesting there is no real
/// capture in this repository to use instead.
fn wire(body_after_9: &str) -> Vec<u8> {
    let body: String = body_after_9.replace('|', "\x01");
    let mut out = format!("8=FIX.4.4\x019={}\x01{body}", body.len()).into_bytes();
    let sum: u32 = out.iter().map(|&b| u32::from(b)).sum::<u32>() % 256;
    out.extend_from_slice(format!("10={sum:03}\x01").as_bytes());
    out
}

fn index_of(w: &[u8]) -> FieldIndex<256> {
    let mut idx = FieldIndex::<256>::new();
    let r = parse_into::<Fix44, 256>(w, &mut idx, Validation::ALL);
    assert!(r.is_ok(), "test message does not parse: {r:?}");
    idx
}

#[test]
fn declared_and_counted_are_separate_numbers() {
    // The one populated group in the whole acceptance corpus, and it is there to
    // test a WRONG count: 386=3 with two 336 entries. The parser must report
    // both numbers and judge neither — the session decides whether that is a
    // Reject 373=16, and needs 386 to put in 371=.
    let line = common::load_all()
        .into_iter()
        .find(|l| l.file.starts_with("14i") && l.wire.windows(6).any(|w| w == b"\x01386=3"))
        .expect("14i I-line with 386=3");
    let idx = index_of(&line.wire);
    let g = idx
        .view(&line.wire)
        .group::<Fix44>(b"D", 386)
        .expect("386 is present");
    assert_eq!(g.declared(), Some(3), "the count field says 3");
    assert_eq!(g.counted(), 2, "two 336 entries are actually there");
}

#[test]
fn an_entry_holds_its_own_fields_and_not_the_next_ones() {
    let line = common::load_all()
        .into_iter()
        .find(|l| l.file.starts_with("14i") && l.wire.windows(6).any(|w| w == b"\x01386=3"))
        .expect("14i I-line");
    let idx = index_of(&line.wire);
    let view = idx.view(&line.wire);
    let mut g = view.group::<Fix44>(b"D", 386).unwrap();

    let e1 = g.next().expect("entry 1");
    assert_eq!(e1.get(336), Some(&b"PRE-OPEN"[..]));
    let e2 = g.next().expect("entry 2");
    assert_eq!(e2.get(336), Some(&b"AFTER-HOURS"[..]));
    assert!(g.next().is_none(), "60= ends the group, it is not a member");

    // 60 sits immediately after the last entry. An entry that ran to the end of
    // the message would answer this, and it must not.
    assert_eq!(e2.get(60), None, "SendingTime is outside the group");
}

#[test]
fn a_group_ends_at_the_first_non_member() {
    let w = wire(
        "35=D|34=2|49=TW|56=ISLD|52=20260828-00:00:00.000|386=2|336=A|336=B|60=20260828-00:00:00.000|",
    );
    let idx = index_of(&w);
    let mut g = idx.view(&w).group::<Fix44>(b"D", 386).unwrap();
    assert_eq!(g.counted(), 2);
    assert!(g.next().is_some() && g.next().is_some() && g.next().is_none());
}

#[test]
fn an_absent_counter_is_none_and_a_zero_count_is_an_empty_group() {
    let w = wire("35=D|34=2|49=TW|56=ISLD|52=20260828-00:00:00.000|60=20260828-00:00:00.000|");
    let idx = index_of(&w);
    assert!(
        idx.view(&w).group::<Fix44>(b"D", 386).is_none(),
        "no 386 at all"
    );

    let z =
        wire("35=D|34=2|49=TW|56=ISLD|52=20260828-00:00:00.000|386=0|60=20260828-00:00:00.000|");
    let idx = index_of(&z);
    let g = idx
        .view(&z)
        .group::<Fix44>(b"D", 386)
        .expect("386=0 is a group with no entries");
    assert_eq!(g.declared(), Some(0));
    assert_eq!(g.counted(), 0);
}

#[test]
fn a_counter_that_is_not_a_number_declares_nothing_and_still_counts() {
    // A malformed count is a different reject from a mismatched one, so the two
    // must be distinguishable. `declared()` is Option for exactly this line.
    let w = wire(
        "35=D|34=2|49=TW|56=ISLD|52=20260828-00:00:00.000|386=abc|336=A|60=20260828-00:00:00.000|",
    );
    let idx = index_of(&w);
    let g = idx.view(&w).group::<Fix44>(b"D", 386).unwrap();
    assert_eq!(g.declared(), None, "abc is not a count");
    assert_eq!(g.counted(), 1, "one entry is still on the wire");
}

// ---- nesting -------------------------------------------------------------
//
// TradeCaptureReport(AE), the deepest chain FIX 4.4 has:
//   552 NoSides -> 78 NoAllocs -> 756 NoNested2PartyIDs -> 806 NoNested2PartySubIDs
// Delimiters 54, 79, 757, 760. `80` is a member of 78 and of nothing deeper, so
// it is the tag that proves each inner scan stopped and the outer one resumed.

const AE: &str = concat!(
    "35=AE|34=2|49=TW|56=ISLD|52=20260828-00:00:00.000|",
    "571=TRID|487=0|856=0|828=0|",
    "552=2|",
    "54=1|37=ORD1|453=0|78=1|79=ACC1|756=1|757=NP1|758=D|806=1|760=SUB1|807=2|80=50|",
    "54=2|37=ORD2|78=0|",
    "60=20260828-00:00:00.000|",
);

#[test]
fn a_nested_group_does_not_truncate_the_one_around_it() {
    let w = wire(AE);
    let idx = index_of(&w);
    let mut sides = idx.view(&w).group::<Fix44>(b"AE", 552).unwrap();
    assert_eq!(sides.declared(), Some(2));
    // Without the nested skip, the scan stops at 79 — the first tag inside
    // NoAllocs — and reports one entry.
    assert_eq!(sides.counted(), 2, "54=2 starts a second side");

    let s1 = sides.next().unwrap();
    assert_eq!(s1.get(37), Some(&b"ORD1"[..]));
    let s2 = sides.next().unwrap();
    assert_eq!(s2.get(37), Some(&b"ORD2"[..]));
    assert!(sides.next().is_none());
}

#[test]
fn nesting_reaches_all_four_levels() {
    let w = wire(AE);
    let idx = index_of(&w);
    let view = idx.view(&w);

    let mut sides = view.group::<Fix44>(b"AE", 552).unwrap();
    let side1 = sides.next().unwrap();

    let mut allocs = side1.group::<Fix44>(b"AE", 78).expect("level 2");
    assert_eq!(allocs.counted(), 1);
    let a1 = allocs.next().unwrap();
    assert_eq!(a1.get(79), Some(&b"ACC1"[..]));
    assert_eq!(
        a1.get(80),
        Some(&b"50"[..]),
        "80 belongs to the alloc, past the nested part"
    );

    let mut parties = a1.group::<Fix44>(b"AE", 756).expect("level 3");
    assert_eq!(parties.counted(), 1);
    let p1 = parties.next().unwrap();
    assert_eq!(p1.get(757), Some(&b"NP1"[..]));
    assert_eq!(p1.get(80), None, "80 is the alloc's, not the party's");

    let mut subs = p1.group::<Fix44>(b"AE", 806).expect("level 4");
    assert_eq!(subs.counted(), 1);
    let sub1 = subs.next().unwrap();
    assert_eq!(sub1.get(760), Some(&b"SUB1"[..]));
    assert_eq!(sub1.get(807), Some(&b"2"[..]));
}

#[test]
fn a_nested_group_is_scoped_to_its_entry() {
    let w = wire(AE);
    let idx = index_of(&w);
    let view = idx.view(&w);
    let mut sides = view.group::<Fix44>(b"AE", 552).unwrap();
    let _ = sides.next().unwrap();
    let side2 = sides.next().unwrap();

    // Side 2 declares 78=0. Reading NoAllocs from side 2 must find side 2's
    // counter, not side 1's — the flat index holds both.
    let allocs = side2
        .group::<Fix44>(b"AE", 78)
        .expect("78=0 is present in side 2");
    assert_eq!(allocs.declared(), Some(0));
    assert_eq!(allocs.counted(), 0);
    assert!(side2.get(79).is_none(), "side 2 has no allocation account");
}

#[test]
fn an_empty_nested_group_is_skipped_without_ending_the_outer_one() {
    // 453=0 sits between 37 and 78 inside side 1. A scanner that treats a zero
    // count as "no group here" and keeps walking is right; one that stops is not.
    let w = wire(AE);
    let idx = index_of(&w);
    let view = idx.view(&w);
    let side1 = view.group::<Fix44>(b"AE", 552).unwrap().next().unwrap();
    assert_eq!(
        side1.get(78),
        Some(&b"1"[..]),
        "78 is past the empty 453 group"
    );
}

#[test]
fn a_nested_counter_is_not_a_top_level_group() {
    // NoAllocs(78) exists in a TradeCaptureReport only inside NoSides(552).
    // A search that walks the flat index without stepping over group regions
    // finds side 1's copy and presents it as the message's own.
    let w = wire(AE);
    let idx = index_of(&w);
    assert!(
        idx.view(&w).group::<Fix44>(b"AE", 78).is_none(),
        "78 is inside 552, not a group of the message"
    );
    // The same counter IS top-level in an AllocationInstruction, and the rule
    // must not have cost that.
    let j = wire(
        "35=J|34=2|49=TW|56=ISLD|52=20260828-00:00:00.000|70=AL1|71=0|626=1|857=0|54=1|53=100|6=1.0|75=20260828|78=2|79=A|79=B|60=20260828-00:00:00.000|",
    );
    let jidx = index_of(&j);
    let g = jidx
        .view(&j)
        .group::<Fix44>(b"J", 78)
        .expect("78 is top-level in J");
    assert_eq!(g.counted(), 2);
}
