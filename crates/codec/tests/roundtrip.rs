//! Parse a real message, rebuild it from a template, and compare bytes.
//!
//! The only test that exercises D9 end to end: sort at build time, body first,
//! prefix right-aligned, `BodyLength` and `CheckSum` computed here. If field
//! order ever came from a call site — non-negotiable 5 — this is where it shows.
//!
//! The interesting part is the **iff**. A line round-trips byte-identical (up to
//! `10=`) exactly when it was already in canonical order and its `9=` was right.
//! That is a falsifiable claim about 533 real messages, and it explains every
//! line that differs without listing any of them.
#![allow(clippy::unwrap_used, clippy::panic, clippy::expect_used)]

mod common;

use fixbolt_codec::{
    Dictionary, FieldIndex, MessageView, Parsed, Template, TemplateBuilder, Validation, parse_into,
};
use fixbolt_dict::Fix44;

/// `MsgType` first, then header tags ascending, then body tags ascending —
/// the order the acceptance comparator checks positionally.
fn key(tag: u32) -> (u8, u32) {
    if tag == 35 {
        (0, 0)
    } else if Fix44::is_header(tag) {
        (1, tag)
    } else {
        (2, tag)
    }
}

/// Are this message's fields already in the order a template would produce?
fn is_canonical(view: &MessageView<'_, 64>) -> bool {
    let mut prev = (0u8, 0u32);
    for i in 0..view.len() {
        let Some((tag, _)) = view.field_at(i) else {
            return false;
        };
        if matches!(tag, 8..=10) {
            continue;
        }
        let k = key(tag);
        if k < prev {
            return false;
        }
        prev = k;
    }
    true
}

fn template_of(view: &MessageView<'_, 64>) -> Option<Template<64, 2048>> {
    let mut b = TemplateBuilder::<64, 2048>::new(view.get(8)?);
    for i in 0..view.len() {
        let (tag, value) = view.field_at(i)?;
        if matches!(tag, 8..=10) {
            continue;
        }
        b = b.field(tag, value);
    }
    b.build::<Fix44>().ok()
}

/// Everything up to and including the SOH before `10=`.
fn up_to_trailer(msg: &[u8]) -> &[u8] {
    msg.windows(4)
        .position(|w| w == [0x01, b'1', b'0', b'='])
        .map_or(msg, |i| &msg[..=i])
}

#[test]
fn a_canonical_message_round_trips_byte_for_byte() {
    let lines = common::load_all();
    let mut idx: FieldIndex<64> = FieldIndex::new();
    let mut out = [0u8; 8192];

    let (mut canonical, mut reordered) = (0usize, 0usize);
    let mut wrong: Vec<String> = Vec::new();

    for l in &lines {
        if parse_into::<Fix44, 64>(&l.wire, &mut idx, Validation::NONE)
            != Ok(Parsed::Complete {
                consumed: l.wire.len(),
            })
        {
            continue;
        }
        // A correct 9= is the other half of the claim: three E lines carry a
        // stale one and three I lines a deliberately wrong one, and the template
        // computes the truth rather than reproducing the file. This re-parse
        // refills `idx`, so the view is taken after it, not before.
        let body_length_ok = parse_into::<Fix44, 64>(
            &l.wire,
            &mut idx,
            Validation {
                body_length: true,
                check_sum: false,
            },
        ) == Ok(Parsed::Complete {
            consumed: l.wire.len(),
        });
        let view = idx.view(&l.wire);
        let expect_identical = is_canonical(&view) && body_length_ok;

        let Some(t) = template_of(&view) else {
            wrong.push(format!("{}:{} template would not build", l.file, l.line_no));
            continue;
        };
        let Ok(r) = t.encode(&mut out, &[]) else {
            wrong.push(format!("{}:{} encode failed", l.file, l.line_no));
            continue;
        };
        let identical = up_to_trailer(&out[r]) == up_to_trailer(&l.wire);

        if identical != expect_identical {
            wrong.push(format!(
                "{}:{} canonical={} body_length_ok={} but identical={}",
                l.file,
                l.line_no,
                is_canonical(&view),
                body_length_ok,
                identical
            ));
        }
        if identical {
            canonical += 1;
        } else {
            reordered += 1;
        }
    }

    assert!(
        wrong.is_empty(),
        "\nround-trip did not follow the rule for {} line(s):\n  {}",
        wrong.len(),
        wrong.join("\n  ")
    );
    assert_eq!(canonical, 505, "messages already in canonical order");
    assert_eq!(
        reordered, 28,
        "messages the template had to reorder or relength"
    );
    println!("round-trip: {canonical} identical, {reordered} reordered or re-lengthed");
}

#[test]
fn the_checksum_we_write_is_the_checksum_of_what_we_wrote() {
    let lines = common::load_all();
    let mut idx: FieldIndex<64> = FieldIndex::new();
    let mut out = [0u8; 8192];
    let mut n = 0usize;

    for l in &lines {
        if parse_into::<Fix44, 64>(&l.wire, &mut idx, Validation::NONE)
            != Ok(Parsed::Complete {
                consumed: l.wire.len(),
            })
        {
            continue;
        }
        let view = idx.view(&l.wire);
        let Some(t) = template_of(&view) else {
            continue;
        };
        let Ok(r) = t.encode(&mut out, &[]) else {
            continue;
        };

        // Re-parse our own output with everything on. If the template got
        // BodyLength or CheckSum wrong, the parser is the thing that says so.
        let msg = out[r].to_vec();
        let mut back: FieldIndex<64> = FieldIndex::new();
        assert_eq!(
            parse_into::<Fix44, 64>(&msg, &mut back, Validation::ALL),
            Ok(Parsed::Complete {
                consumed: msg.len()
            }),
            "{}:{} the template produced a frame its own parser rejects:\n  {}",
            l.file,
            l.line_no,
            String::from_utf8_lossy(&msg).replace('\u{1}', "|")
        );
        n += 1;
    }
    assert_eq!(n, 533);
    println!("{n} encoded messages re-parse cleanly with full validation");
}

#[test]
fn one_template_serves_every_shape_of_reject() {
    // 35=3 takes eight different field sets across the corpus. A template with
    // optional slots must cover all of them, or D9's mechanism does not work and
    // the engine needs one template per shape.
    let t = TemplateBuilder::<32, 512>::new(b"FIX.4.4")
        .field(35, b"3")
        .field(49, b"ISLD")
        .field(56, b"TW44")
        .slot(34)
        .slot(52)
        .slot(45)
        .slot(58)
        .slot(371)
        .slot(372)
        .slot(373)
        .build::<Fix44>()
        .unwrap();

    let mut out = [0u8; 512];
    let mut idx: FieldIndex<64> = FieldIndex::new();

    // Everything supplied.
    let full = t
        .encode(
            &mut out,
            &[
                (34, b"2".as_ref()),
                (52, b"00000000-00:00:00.000".as_ref()),
                (45, b"2".as_ref()),
                (58, b"Invalid tag number".as_ref()),
                (371, b"999".as_ref()),
                (372, b"0".as_ref()),
                (373, b"0".as_ref()),
            ],
        )
        .unwrap();
    let msg = out[full].to_vec();
    assert_eq!(
        parse_into::<Fix44, 64>(&msg, &mut idx, Validation::ALL),
        Ok(Parsed::Complete {
            consumed: msg.len()
        })
    );
    // Structure asserted here; the checksum is not hard-coded — Validation::ALL
    // above already proved it, and a magic constant would only be a second copy
    // of the same arithmetic.
    assert_eq!(
        String::from_utf8_lossy(up_to_trailer(&msg)).replace('\u{1}', "|"),
        "8=FIX.4.4|9=98|35=3|34=2|49=ISLD|52=00000000-00:00:00.000|56=TW44|\
         45=2|58=Invalid tag number|371=999|372=0|373=0|"
    );

    // The same template with four slots left out. Nothing is written for them,
    // and BodyLength and CheckSum follow.
    let short = t
        .encode(
            &mut out,
            &[
                (34, b"3".as_ref()),
                (52, b"00000000-00:00:00.000".as_ref()),
                (45, b"3".as_ref()),
            ],
        )
        .unwrap();
    let msg = out[short].to_vec();
    assert_eq!(
        parse_into::<Fix44, 64>(&msg, &mut idx, Validation::ALL),
        Ok(Parsed::Complete {
            consumed: msg.len()
        })
    );
    assert_eq!(
        String::from_utf8_lossy(up_to_trailer(&msg)).replace('\u{1}', "|"),
        // 5 + 5 + 8 + 25 + 8 + 5 = 56, counted by hand and by the encoder.
        "8=FIX.4.4|9=56|35=3|34=3|49=ISLD|52=00000000-00:00:00.000|56=TW44|45=3|"
    );
}
