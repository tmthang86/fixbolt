//! The generated FIX 4.4 dictionary driving the real parser.
//!
//! `crates/codec` tests its DATA path against a hand-written three-entry
//! dictionary, to stay free of a dependency cycle. This is the other half: the
//! table `build.rs` produced from the XML, used for real.
#![allow(clippy::unwrap_used, clippy::panic)]

use nanofix_codec::{FieldIndex, Parsed, Validation, parse_into};
use nanofix_dict::Fix44;

fn frame(body: &[u8]) -> Vec<u8> {
    let mut out = b"8=FIX.4.4\x01".to_vec();
    out.extend_from_slice(format!("9={}\u{1}", body.len()).as_bytes());
    out.extend_from_slice(body);
    let sum = out.iter().fold(0u8, |a, &b| a.wrapping_add(b));
    out.extend_from_slice(format!("10={sum:03}\u{1}").as_bytes());
    out
}

#[test]
fn the_generated_table_carries_a_data_value_containing_a_separator() {
    let msg = frame(b"35=A\x0134=2\x0195=5\x0196=ab\x01cd\x0198=0\x01");
    let mut idx: FieldIndex<64> = FieldIndex::new();
    let r = parse_into::<Fix44, 64>(&msg, &mut idx, Validation::ALL).unwrap();
    assert_eq!(
        r,
        Parsed::Complete {
            consumed: msg.len()
        }
    );
    assert_eq!(idx.view(&msg).get(96), Some(&b"ab\x01cd"[..]));
}

#[test]
fn the_generated_table_knows_signature_takes_tag_93() {
    let msg = frame(b"35=A\x0134=2\x0193=4\x0189=a\x01bc\x01");
    let mut idx: FieldIndex<64> = FieldIndex::new();
    parse_into::<Fix44, 64>(&msg, &mut idx, Validation::ALL).unwrap();
    assert_eq!(idx.view(&msg).get(89), Some(&b"a\x01bc"[..]));
}

#[test]
fn an_ordinary_message_is_unaffected_by_the_data_path() {
    let msg = frame(b"35=D\x0134=2\x0111=ID\x0155=INTC\x0154=1\x01");
    let mut idx: FieldIndex<64> = FieldIndex::new();
    parse_into::<Fix44, 64>(&msg, &mut idx, Validation::ALL).unwrap();
    let v = idx.view(&msg);
    assert_eq!(v.get(55), Some(&b"INTC"[..]));
    assert_eq!(v.len(), 8);
}
