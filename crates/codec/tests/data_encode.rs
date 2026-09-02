//! The write path for DATA fields. `STATUS.md` open item 9.
//!
//! A DATA field is the one place in FIX tag=value where the value may legally
//! contain the separator, so its length cannot be read off the data — it comes
//! from a length field that must sit **immediately in front**. The read path has
//! had that rule since the codec plan (`tests/data_fields.rs`). The write path
//! has never had it, and these tests are what that costs.
//!
//! **No `.def` file in the corpus carries a DATA message**, so nothing here is
//! backed by real data. Every frame is built to the FIX 4.4 specification and is
//! labelled as such — the same caveat `tests/data_fields.rs` carries, and the
//! weakest evidence in this crate.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use fixbolt_codec::{Dictionary, EncodeError, TemplateBuilder};

/// The three DATA pairings that matter to these tests, including the one that
/// breaks the `tag - 1` rule everything else happens to satisfy.
struct D;

impl Dictionary for D {
    fn is_header(tag: u32) -> bool {
        matches!(tag, 8 | 9 | 35 | 34 | 49 | 52 | 56)
    }
    fn data_length_tag(tag: u32) -> Option<u32> {
        match tag {
            89 => Some(93), // Signature     <- SignatureLength, NOT 88
            91 => Some(90), // SecureData    <- SecureDataLen
            96 => Some(95), // RawData       <- RawDataLength
            _ => None,
        }
    }
}

fn base() -> TemplateBuilder<16, 256> {
    let mut b = TemplateBuilder::new(b"FIX.4.4");
    b.field(35, b"D")
        .field(49, b"ME")
        .field(56, b"YOU")
        .slot(34)
        .slot(52);
    b
}

fn positions(wire: &[u8], tag: u32) -> Vec<usize> {
    let needle = format!("\x01{tag}=").into_bytes();
    let mut out = Vec::new();
    let mut at = 0;
    while let Some(i) = wire[at..].windows(needle.len()).position(|w| w == needle) {
        out.push(at + i);
        at += i + 1;
    }
    out
}

/// **The case that is wrong today.** Fifteen of the sixteen FIX 4.4 DATA pairs
/// have `length == data - 1`, so sorting body tags ascending puts the length
/// first by luck. `Signature(89)` takes `SignatureLength(93)`, so ascending
/// order emits the data *before* its length — which no reader can frame.
#[test]
fn a_data_field_is_written_immediately_after_its_length() {
    let t = base()
        .slot(93)
        .slot(89)
        .build::<D>()
        .expect("a template with a signature");
    let mut out = [0u8; 512];
    let r = t
        .encode(
            &mut out,
            &[
                (34, b"2"),
                (52, b"20260830-00:00:00"),
                (93, b"5"),
                (89, b"a\x01b\x01c"),
            ],
        )
        .expect("encodes");
    let wire = &out[r];
    let len_at = *positions(wire, 93).first().expect("93 is present");
    let data_at = *positions(wire, 89).first().expect("89 is present");
    assert!(
        len_at < data_at,
        "SignatureLength must precede Signature:\n{}",
        String::from_utf8_lossy(wire).replace('\u{1}', "|")
    );
    // Immediately before, not merely before: one field between the two SOHs and
    // that field is the length itself. `len_at` and `data_at` both point AT the
    // separator in front of their tag, so the slice between them is exactly
    // `93=<digits>` and holds no separator of its own.
    let between = &wire[len_at + 1..data_at];
    assert!(
        between.starts_with(b"93=") && !between.contains(&1),
        "nothing may sit between the length and its data:\n{}",
        String::from_utf8_lossy(wire).replace('\u{1}', "|")
    );

    // And the body length counts the embedded separators rather than stopping
    // at the first one, which is the whole reason DATA needs a length field.
    assert!(
        wire.starts_with(b"8=FIX.4.4\x019=58\x01"),
        "BodyLength must count the SOH inside the signature:\n{}",
        String::from_utf8_lossy(wire).replace('\u{1}', "|")
    );
}

/// A DATA slot with no length slot cannot be written at all, and the template
/// must say so when it is built — once, at startup — rather than emitting a
/// field no reader can frame, every message, for ever.
#[test]
fn a_data_slot_without_its_length_is_refused_at_build() {
    let Err(err) = base().slot(96).build::<D>() else {
        panic!("a DATA slot with no length slot must not build");
    };
    assert!(
        matches!(err, EncodeError::DataWithoutLength(96)),
        "expected DataWithoutLength(96), got {err:?}"
    );
}

/// The same rule for a static DATA field, because a caller can write one of
/// those too and the wire cannot tell the difference.
#[test]
fn a_static_data_field_without_its_length_is_refused_at_build() {
    let Err(err) = base().field(91, b"xy").build::<D>() else {
        panic!("a static DATA field with no length must not build");
    };
    assert!(
        matches!(err, EncodeError::DataWithoutLength(91)),
        "expected DataWithoutLength(91), got {err:?}"
    );
}

/// **The caller does not get to state the length.** If it did, the invariant
/// would be advice: one wrong number and every reader mis-frames. The encoder
/// writes the true byte count of the value it just placed, and a value
/// containing `0x01` is the case that makes this observable at all.
#[test]
fn the_encoder_writes_the_length_itself_and_counts_embedded_soh() {
    let t = base()
        .slot(95)
        .slot(96)
        .build::<D>()
        .expect("a template with raw data");
    let mut out = [0u8; 512];
    let value = b"a\x01bb\x01ccc"; // 8 bytes, two of them SOH
    let r = t
        .encode(
            &mut out,
            &[
                (34, b"2"),
                (52, b"20260830-00:00:00"),
                (95, b"999"), // deliberately a lie
                (96, value),
            ],
        )
        .expect("encodes");
    let wire = &out[r];
    let at = *positions(wire, 95).first().expect("95 is present");
    let start = at + 1 + b"95=".len();
    let end = start + wire[start..].iter().position(|b| *b == 1).expect("a SOH");
    assert_eq!(
        &wire[start..end],
        b"8",
        "the encoder must write the real length, not the caller's 999:\n{}",
        String::from_utf8_lossy(wire).replace('\u{1}', "|")
    );
}

// ---- DATA inside a repeating group. `STATUS.md` open item 8. -----------------

/// A group whose members include a real FIX 4.4 DATA pair.
///
/// `[measured 2026-08-30]` FIX 4.4 has **66 DATA members across its group
/// tables, and every one of them has its length member declared immediately in
/// front** — so declaration order was already right, and `group_roundtrip.rs`
/// skipped the case rather than proving it. What was missing is enforcement:
/// nothing required the pair to be supplied together, or the length to be true.
struct G;

const ENTRY: &[u32] = &[
    448, // PartyID — the delimiter
    354, // EncodedTextLen
    355, // EncodedText  <- DATA, and 354 sits immediately in front
];

impl Dictionary for G {
    fn is_header(tag: u32) -> bool {
        matches!(tag, 8 | 9 | 35 | 34 | 49 | 52 | 56)
    }
    fn data_length_tag(tag: u32) -> Option<u32> {
        match tag {
            355 => Some(354),
            96 => Some(95),
            _ => None,
        }
    }
    fn group_order(_msg_type: &[u8], counter: u32) -> &'static [u32] {
        if counter == 453 { ENTRY } else { &[] }
    }
}

fn group_template() -> fixbolt_codec::Template<16, 256> {
    TemplateBuilder::<16, 256>::new(b"FIX.4.4")
        .field(35, b"D")
        .field(49, b"ME")
        .field(56, b"YOU")
        .slot(34)
        .slot(52)
        .group(453)
        .build::<G>()
        .expect("a template with a party group")
}

/// The whole point of a DATA field, inside a group: a value carrying `0x01`
/// survives, because the length in front says how far to read.
#[test]
fn a_data_member_of_a_group_round_trips_with_embedded_soh() {
    use fixbolt_codec::{GroupData, GroupEntryData};
    let t = group_template();
    let value: &[u8] = b"x\x01y\x01z\x01!"; // 7 bytes, three of them SOH
    let entries = [GroupEntryData {
        fields: &[(448, b"P1" as &[u8]), (354, b"999"), (355, value)],
        groups: &[],
    }];
    let mut out = [0u8; 512];
    let r = t
        .encode_with::<G>(
            &mut out,
            &[(34, b"2"), (52, b"20260830-00:00:00")],
            &[GroupData {
                counter: 453,
                entries: &entries,
            }],
        )
        .expect("encodes");
    let wire = &out[r];
    let text = String::from_utf8_lossy(wire).replace('\u{1}', "|");

    // The encoder wrote the true length, not the caller's 999.
    // `needle.len()`, not a hand-counted window. A window one byte off matches
    // nothing and reads exactly like a check that passed.
    let needle = b"\x01354=7\x01355=";
    assert!(
        wire.windows(needle.len()).any(|w| w == needle),
        "354 must carry the real length and sit immediately before 355:\n{text}"
    );
    // And every byte of the value is present, separators included.
    let tag355 = b"\x01355=";
    let at = wire
        .windows(tag355.len())
        .position(|w| w == tag355)
        .expect("355 is present")
        + tag355.len();
    assert_eq!(
        &wire[at..at + value.len()],
        value,
        "value survives:\n{text}"
    );
}

/// A DATA member supplied to a group that does not declare its length member is
/// refused, and refused before anything is written.
#[test]
fn a_group_data_member_without_its_length_is_refused() {
    use fixbolt_codec::{GroupData, GroupEntryData};
    // 96 (RawData) takes 95, and this group declares neither 95 nor 96 — so
    // supplying 96 is refused as not a member first. The case that matters is a
    // member whose LENGTH is missing from the order, which `NO_LEN` below is.
    struct H;
    const NO_LEN: &[u32] = &[448, 355];
    impl Dictionary for H {
        fn is_header(tag: u32) -> bool {
            matches!(tag, 8 | 9 | 35 | 34 | 49 | 52 | 56)
        }
        fn data_length_tag(tag: u32) -> Option<u32> {
            if tag == 355 { Some(354) } else { None }
        }
        fn group_order(_m: &[u8], counter: u32) -> &'static [u32] {
            if counter == 453 { NO_LEN } else { &[] }
        }
    }
    let t = TemplateBuilder::<16, 256>::new(b"FIX.4.4")
        .field(35, b"D")
        .slot(34)
        .group(453)
        .build::<H>()
        .expect("builds");
    let entries = [GroupEntryData {
        fields: &[(448, b"P1" as &[u8]), (355, b"data")],
        groups: &[],
    }];
    let mut out = [0u8; 512];
    let got = t.encode_with::<H>(
        &mut out,
        &[(34, b"2")],
        &[GroupData {
            counter: 453,
            entries: &entries,
        }],
    );
    assert!(
        matches!(got, Err(EncodeError::DataWithoutLength(355))),
        "expected DataWithoutLength(355), got {got:?}"
    );
}
