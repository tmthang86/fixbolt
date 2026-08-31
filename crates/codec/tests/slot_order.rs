//! The order the caller hands its slots in must not reach the wire.
//!
//! Non-negotiable 5: field ordering comes from the template's declared parts,
//! never from a call site. `tests/group_roundtrip.rs` already proves this for
//! fields *inside a repeating group*, by handing every group's members in
//! reverse declaration order. **The body path had no equivalent at all**, and
//! that is why this file exists.
//!
//! It was written as the guard for a forward-cursor fast path in
//! `Template::encode_with` — try the position after the previous hit before
//! scanning. `[measured 2026-08-31]` **that fast path was written, measured and
//! then reverted**: 30 runs per arm on the same machine put it **5.2 ns slower**
//! (+3.4%), reproducing at +4.5 and +4.4 ns within each of the machine's two
//! modes. See `docs/reference/measured-costs.md`. The guard is kept because the
//! property it holds is a non-negotiable, not because of the change that
//! prompted it — and the next attempt at that fast path will need it.
//!
//! Every case asserts **byte equality against the forward ordering**, not
//! length and not a field count: an encoder that skipped a field would keep the
//! ordering and change the length, and one that took a neighbouring value would
//! keep the length and change the bytes.
//!
//! **Proven by reversal, twice.** Making `Part::Slot` take the caller's slots in
//! the order supplied — the defect non-negotiable 5 forbids — turns four of
//! these six red on **wrong bytes**, not on a panic; `no_slots_still_encodes`
//! and `one_slot_is_identical` stay green, correctly, because with zero or one
//! slot there is no ordering to get wrong. Deleting the cursor's fallback scan,
//! while it existed, turned the same four red on **dropped fields**.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use fixbolt_codec::{Dictionary, NoDict, TemplateBuilder};

/// The 14 slots of the `ExecutionReport` the serialise benchmark encodes, in
/// the order the template declares them.
const TAGS: [u32; 14] = [34, 52, 37, 17, 150, 39, 55, 54, 38, 32, 31, 151, 14, 6];
const VALS: [&[u8]; 14] = [
    b"2",
    b"20260831-01:00:00.000",
    b"ORD00001",
    b"EXE00001",
    b"F",
    b"2",
    b"INTC",
    b"1",
    b"2000",
    b"2000",
    b"20.15",
    b"0",
    b"2000",
    b"20.15",
];

fn template() -> fixbolt_codec::Template<32, 512> {
    let mut tb = TemplateBuilder::<32, 512>::new(b"FIX.4.4")
        .field(35, b"8")
        .field(49, b"ISLD")
        .field(56, b"TW44");
    for tag in TAGS {
        tb = tb.slot(tag);
    }
    tb.build::<NoDict>().expect("template builds")
}

/// Encode `which` of the 14 slots, handed to `encode` in `order`.
fn encode(which: &[usize], order: &[usize]) -> Vec<u8> {
    let t = template();
    let slots: Vec<(u32, &[u8])> = order
        .iter()
        .filter(|i| which.contains(i))
        .map(|&i| (TAGS[i], VALS[i]))
        .collect();
    let mut out = [0u8; 512];
    let r = t.encode(&mut out, &slots).expect("encodes");
    out[r].to_vec()
}

fn forward() -> Vec<usize> {
    (0..14).collect()
}

fn reversed() -> Vec<usize> {
    (0..14).rev().collect()
}

/// A fixed permutation, not a random one: a test that shuffles differently on
/// every run reports a different thing on every run.
fn shuffled() -> Vec<usize> {
    vec![7, 0, 13, 4, 9, 2, 11, 6, 1, 12, 5, 8, 3, 10]
}

#[test]
fn reversed_is_identical() {
    let all = forward();
    assert_eq!(encode(&all, &reversed()), encode(&all, &forward()));
}

#[test]
fn shuffled_is_identical() {
    let all = forward();
    assert_eq!(encode(&all, &shuffled()), encode(&all, &forward()));
}

/// The cursor's hardest case: the template declares 14 slots and the caller
/// supplies 10, so the aligned guess is wrong from the first gap onwards. An
/// unsupplied slot is simply not written — that is `encode_with`'s documented
/// behaviour and it must survive the fast path.
#[test]
fn a_gap_does_not_shift_the_cursor() {
    let some: Vec<usize> = vec![0, 1, 2, 5, 6, 7, 9, 11, 12, 13];
    let base = encode(&some, &forward());
    assert_eq!(encode(&some, &reversed()), base);
    assert_eq!(encode(&some, &shuffled()), base);

    // and the four omitted tags are genuinely absent, so the case is not
    // vacuously equal to itself through some other bug
    for missing in [3usize, 4, 8, 10] {
        let needle = format!("\x01{}=", TAGS[missing]).into_bytes();
        assert!(
            !base.windows(needle.len()).any(|w| w == needle),
            "tag {} was not supplied and must not be written",
            TAGS[missing]
        );
    }
    // while a supplied one is present, so the search above can find things
    let present = format!("\x01{}=", TAGS[5]).into_bytes();
    assert!(base.windows(present.len()).any(|w| w == present));
}

/// Only one slot, supplied. The cursor starts at 0 and the match is at 0, which
/// is the case a cursor gets right by accident; it is here so the boundary is
/// stated rather than assumed.
#[test]
fn one_slot_is_identical() {
    assert_eq!(encode(&[0], &reversed()), encode(&[0], &forward()));
}

/// No slots at all: the template's static parts and nothing else.
#[test]
fn no_slots_still_encodes() {
    let empty: Vec<usize> = Vec::new();
    let base = encode(&empty, &forward());
    assert!(base.starts_with(b"8=FIX.4.4\x019="));
    assert_eq!(encode(&empty, &reversed()), base);
}

/// The `DataLen` part looks up a **different** tag from the one it writes —
/// `data_tag`, not `tag` — so a cursor aligned on the parts is aligned on the
/// wrong thing there. This pins that the length and its DATA still come out
/// right whatever order the caller used.
struct D;

impl Dictionary for D {
    fn is_header(tag: u32) -> bool {
        matches!(tag, 8 | 9 | 35 | 34 | 49 | 52 | 56)
    }
    fn data_length_tag(tag: u32) -> Option<u32> {
        match tag {
            96 => Some(95), // RawData <- RawDataLength
            _ => None,
        }
    }
}

#[test]
fn data_length_survives_any_order() {
    let t = TemplateBuilder::<16, 256>::new(b"FIX.4.4")
        .field(35, b"D")
        .field(49, b"ME")
        .field(56, b"YOU")
        .slot(34)
        // the length slot must be declared immediately in front of its DATA;
        // `tests/data_encode.rs` proves the builder refuses otherwise
        .slot(95)
        .slot(96)
        .slot(52)
        .build::<D>()
        .expect("template builds");

    let raw: &[u8] = b"a\x01b\x01c"; // contains SOH, which is why 95 exists
    let fwd: Vec<(u32, &[u8])> = vec![(34, b"2"), (96, raw), (52, b"20260831-01:00:00.000")];
    let rev: Vec<(u32, &[u8])> = vec![(52, b"20260831-01:00:00.000"), (96, raw), (34, b"2")];

    let mut a = [0u8; 256];
    let mut b = [0u8; 256];
    let ra = t.encode_with::<D>(&mut a, &fwd, &[]).expect("encodes");
    let rb = t.encode_with::<D>(&mut b, &rev, &[]).expect("encodes");
    assert_eq!(a[ra.clone()], b[rb]);

    // the length must be the DATA's real length, not something the ordering
    // happened to make plausible
    let wire = a[ra].to_vec();
    let needle = b"\x0195=5\x0196=".to_vec();
    assert!(
        wire.windows(needle.len()).any(|w| w == needle),
        "expected 95=5 immediately in front of 96, got {:?}",
        String::from_utf8_lossy(&wire)
    );
}
