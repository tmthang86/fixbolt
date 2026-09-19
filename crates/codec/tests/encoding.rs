//! The [`Encoding`] trait reads exactly what the concrete API reads.
//!
//! The trait's whole claim is that it adds a name and not a behaviour: ADR-0079
//! decision 2 keeps one view type per encoding precisely so that going through
//! the trait costs nothing and changes nothing. That claim is falsifiable, and
//! this is where it is falsified — on **real messages from the 59 acceptance
//! definitions**, not on invented ones (`CLAUDE.md` §7), comparing
//! `Encoding::field` against `MessageView::get` for every tag each message
//! actually carries, plus a handful the messages do not carry so that `None`
//! is compared too.
//!
//! `api_unchanged` is the other half: row A1 requires `parse_into`,
//! `MessageView` and `FieldIndex` to be untouched, and the way to hold a *type*
//! still is to write the old call down and make the compiler check it.
#![allow(clippy::unwrap_used, clippy::panic, clippy::expect_used)]
// Not a library crate's source: non-negotiable 7 is about `crates/*/src`, and
// `scripts/check-indexing-debt.sh` counts nothing outside it. An index that
// panics in a test is a failing test, which is what a test is for.
#![allow(clippy::indexing_slicing)]

mod common;

use common::Direction;
use fixbolt_codec::{
    Encoding, FieldIndex, MessageView, NoDict, Parsed, TagValue, Template, TemplateBuilder,
    Validation, parse_into,
};

/// The encoding under test: FIX 4.4 tag=value, the parser's own `NoDict`.
///
/// `NoDict` rather than `Fix44` on purpose — this test is about the trait
/// forwarding, and a dictionary is a second thing that could explain a
/// difference. `benches/alloc.rs` keeps its templates dictionary-free for the
/// same reason.
type Fix = TagValue<NoDict, 64>;

/// Five real messages, one per `MsgType`, taken in corpus order from the `I`
/// lines of the 59 definitions.
///
/// One per type rather than the first five lines: the first five are three
/// Logons and two Heartbeats, which would compare the same field set five
/// times.
fn five_real_messages() -> Vec<(String, usize, Vec<u8>)> {
    let mut seen: Vec<Vec<u8>> = Vec::new();
    let mut out = Vec::new();
    for line in common::load_all() {
        if line.direction != Direction::In {
            continue;
        }
        let mut idx: FieldIndex<64> = FieldIndex::new();
        // Only messages the parser reads whole: a deliberately garbled line
        // would compare two half-filled indexes and prove nothing.
        if !matches!(
            parse_into::<NoDict, 64>(&line.wire, &mut idx, Validation::NONE),
            Ok(Parsed::Complete { .. })
        ) {
            continue;
        }
        let Some(mt) = idx.view(&line.wire).get(35).map(<[u8]>::to_vec) else {
            continue;
        };
        if seen.contains(&mt) {
            continue;
        }
        seen.push(mt);
        out.push((line.file.clone(), line.line_no, line.wire.clone()));
        if out.len() == 5 {
            break;
        }
    }
    assert_eq!(
        out.len(),
        5,
        "the corpus must supply five parseable messages of five different types"
    );
    out
}

/// `Encoding::field` is `MessageView::get`, on every tag five real messages
/// carry — and on four they do not.
#[test]
fn field_through_the_trait_is_message_view_get() {
    let mut compared = 0usize;
    for (file, line_no, wire) in five_real_messages() {
        // Concrete API.
        let mut direct: FieldIndex<64> = FieldIndex::new();
        let d = parse_into::<NoDict, 64>(&wire, &mut direct, Validation::ALL);

        // Same bytes, same dictionary, through the trait.
        let mut through: FieldIndex<64> = FieldIndex::new();
        let t = <Fix as Encoding>::parse(&wire, &mut through, Validation::ALL);

        assert_eq!(d, t, "{file}:{line_no} — parse outcomes differ");

        let dv: MessageView<'_, 64> = direct.view(&wire);
        let tv = <Fix as Encoding>::view(&through, &wire);
        assert_eq!(dv.len(), tv.len(), "{file}:{line_no} — field counts differ");

        for i in 0..dv.len() {
            let (tag, _) = dv.field_at(i).expect("field in range");
            assert_eq!(
                <Fix as Encoding>::field(tv, tag),
                dv.get(tag),
                "{file}:{line_no} — tag {tag}"
            );
            compared += 1;
        }
        // Absent tags: the comparison must cover `None`, or a `field` that
        // returned `Some(&[])` for everything would pass the loop above.
        for tag in [1u32, 4242, 99_999, 0] {
            assert_eq!(
                <Fix as Encoding>::field(tv, tag),
                dv.get(tag),
                "{file}:{line_no} — absent tag {tag}"
            );
            compared += 1;
        }
    }
    assert!(
        compared >= 60,
        "five real messages should mean at least 60 comparisons, got {compared}"
    );
}

/// The session fields the trait hands up are the ones `get` returns.
#[test]
fn session_fields_are_the_same_bytes() {
    for (file, line_no, wire) in five_real_messages() {
        let mut idx: FieldIndex<64> = FieldIndex::new();
        let _ = <Fix as Encoding>::parse(&wire, &mut idx, Validation::ALL);
        let v = <Fix as Encoding>::view(&idx, &wire);
        let s = <Fix as Encoding>::session_fields(v).expect("tag=value has session fields");
        let direct = idx.view(&wire);
        assert_eq!(s.begin_string, direct.get(8), "{file}:{line_no} — 8");
        assert_eq!(s.msg_type, direct.get(35), "{file}:{line_no} — 35");
        assert_eq!(s.msg_seq_num, direct.get(34), "{file}:{line_no} — 34");
        assert_eq!(s.sender_comp_id, direct.get(49), "{file}:{line_no} — 49");
        assert_eq!(s.target_comp_id, direct.get(56), "{file}:{line_no} — 56");
        assert_eq!(s.sending_time, direct.get(52), "{file}:{line_no} — 52");
        assert_eq!(s.poss_dup_flag, direct.get(43), "{file}:{line_no} — 43");
        assert_eq!(
            s.orig_sending_time,
            direct.get(122),
            "{file}:{line_no} — 122"
        );
        // Every message here has a `MsgType`; a `SessionFields` that read the
        // wrong tag would still pass the comparisons above if `get` were also
        // wrong, so pin one value to the wire.
        assert!(
            s.msg_type.is_some(),
            "{file}:{line_no} — 35 must be present"
        );
    }
}

/// `Encoding::encode` writes the same bytes `Template::encode` writes.
#[test]
fn encode_through_the_trait_is_template_encode() {
    let t = TemplateBuilder::<16, 256>::new(b"FIX.4.4")
        .field(35, b"0")
        .field(49, b"ISLD")
        .field(56, b"TW44")
        .slot(34)
        .slot(52)
        .build::<NoDict>()
        .expect("template");
    let slots: &[(u32, &[u8])] = &[(34, b"7"), (52, b"20260919-00:00:00.000")];

    let mut a = [0u8; 256];
    let ra = t.encode(&mut a, slots).expect("direct");
    let mut b = [0u8; 256];
    let rb = <Fix as Encoding>::encode(&t, &mut b, slots).expect("through the trait");

    assert_eq!(ra, rb, "ranges differ");
    assert_eq!(a[ra.clone()], b[rb], "bytes differ");
    // The path must be live before the equality means anything.
    assert!(
        a[ra].starts_with(b"8=FIX.4.4\x019="),
        "the encode path must actually run"
    );
}

/// Row A1: the existing public API did not change one character.
///
/// This test exists to **fail to compile** the day someone changes a signature,
/// a size or a name that phase 2 promised not to touch. Nothing here is clever;
/// that is the point.
#[test]
fn api_unchanged() {
    // The sizes `lib.rs` asserts, asserted again from outside the crate.
    assert_eq!(core::mem::size_of::<MessageView<'static, 64>>(), 24);
    assert_eq!(core::mem::size_of::<fixbolt_codec::FieldEntry>(), 12);

    // `parse_into::<D, N>(buf, &mut FieldIndex<N>, Validation) -> Result<Parsed, ParseError>`
    let msg: &[u8] = b"8=FIX.4.4\x019=45\x0135=0\x0134=2\x0149=TW\x0152=20260919-00:00:00\x01\
56=ISLD\x0110=220\x01";
    let mut idx: FieldIndex<64> = FieldIndex::new();
    let parsed: Result<Parsed, fixbolt_codec::ParseError> =
        parse_into::<NoDict, 64>(msg, &mut idx, Validation::ALL);
    assert!(matches!(parsed, Ok(Parsed::Complete { .. })), "{parsed:?}");

    // `FieldIndex::view(&self, &[u8]) -> MessageView<'_, N>`, and `get` by `u32`.
    let view: MessageView<'_, 64> = idx.view(msg);
    let mt: Option<&[u8]> = view.get(35);
    assert_eq!(mt, Some(b"0".as_ref()));
    let at: Option<(u32, &[u8])> = view.field_at(0);
    assert_eq!(at, Some((8, b"FIX.4.4".as_ref())));

    // `TemplateBuilder::<P, S>::new(&[u8]).build::<D>() -> Result<Template<P, S>, _>`
    // and `Template::encode(&self, &mut [u8], &[(u32, &[u8])]) -> Result<Range, _>`.
    let t: Template<8, 128> = TemplateBuilder::<8, 128>::new(b"FIX.4.4")
        .field(35, b"0")
        .slot(34)
        .build::<NoDict>()
        .expect("template");
    let mut out = [0u8; 128];
    let r: core::ops::Range<usize> = t.encode(&mut out, &[(34, b"1".as_ref())]).expect("encode");
    assert!(out[r].starts_with(b"8=FIX.4.4\x019="));
}
