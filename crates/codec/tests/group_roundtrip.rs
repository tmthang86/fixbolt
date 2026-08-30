//! Writing repeating groups: parse -> encode -> byte-identical.
//!
//! The message for every one of the 731 `(msg_type, counter)` pairs the FIX 4.4
//! dictionary declares is generated from the dictionary itself, so "covers every
//! group" is checked rather than claimed. Nested groups are generated to the
//! full depth of 4.
//!
//! **The fields are handed to the encoder in reverse declaration order.** If the
//! encoder wrote them in the order it was given, every message here would come
//! back reversed. Ordering has to come from `group_order`, which is what
//! non-negotiable 5 says — and it is the one rule a group can break that the
//! ascending-tag rule for the body cannot catch, because inside a group the
//! order is *not* ascending.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeSet;
use std::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};

use nanofix_codec::{
    Dictionary, FieldIndex, GroupData, GroupEntryData, TemplateBuilder, Validation, parse_into,
};
use nanofix_dict::{Fix44, GROUP_KEYS};

const SOH: u8 = 0x01;
const MAX_DEPTH: usize = 4;

/// Is `counter` a group of `mt` that no other group of `mt` contains?
fn is_top_level(mt: &[u8], counter: u32) -> bool {
    !GROUP_KEYS
        .iter()
        .any(|(m, c)| *m == mt && *c != counter && Fix44::group_members(m, *c).contains(&counter))
}

/// One entry, as bytes in declaration order, and as data in reverse.
struct Built {
    wire: Vec<u8>,
    data: GroupData<'static>,
}

/// Generates a legal instance of one group, walking `group_order`.
///
/// **The wire bytes come from the same table the encoder reads.** So this proves
/// parse-then-encode is byte-stable and that the encoder ignores the order it
/// was handed — it does **not** prove the table's order matches what a real
/// counterparty writes. That is `tools/interop` against `libquickfix`, step 5,
/// and nothing here substitutes for it.
fn build(
    mt: &'static [u8],
    counter: u32,
    entries: usize,
    depth: usize,
    seen: &mut BTreeSet<u32>,
) -> Built {
    seen.insert(counter);
    let order = Fix44::group_order(mt, counter);
    let delimiter = order[0];

    let mut wire = Vec::new();
    let mut datas: Vec<GroupEntryData<'static>> = Vec::new();
    for e in 0..entries {
        let v: &'static [u8] = Box::leak(format!("V{e}").into_bytes().into_boxed_slice());
        let mut fields: Vec<(u32, &'static [u8])> = Vec::new();
        let mut subs: Vec<GroupData<'static>> = Vec::new();
        let mut plain = 0;

        for &tag in order {
            if tag == delimiter {
                push(&mut wire, tag, v);
                fields.push((tag, v));
            } else if Fix44::group_delimiter(mt, tag).is_some() {
                // A nested group. Every one of them, not just the first: three
                // counters in FIX 4.4 exist only as the second nested group of
                // some other group, and taking only the first misses them.
                if depth < MAX_DEPTH {
                    let sub = build(mt, tag, 1, depth + 1, seen);
                    push(&mut wire, tag, b"1");
                    wire.extend_from_slice(&sub.wire);
                    subs.push(sub.data);
                }
            } else if Fix44::data_length_tag(tag).is_some() {
                // **A DATA member, and its value carries the separator.**
                // `STATUS.md` item 8. This branch used to be a `continue` with a
                // comment saying DATA was a different test; it was not tested
                // anywhere, and `[measured 2026-08-30]` FIX 4.4 has 66 DATA
                // members across these tables.
                //
                // The length is handed over deliberately WRONG. The encoder
                // computes it from the value, so a test that supplied the right
                // one could not tell the two apart — the same shape as a fixture
                // that agrees with the bug.
                let len_tag = Fix44::data_length_tag(tag).unwrap_or(0);
                let dv: &'static [u8] =
                    Box::leak(format!("D{e}\u{1}x").into_bytes().into_boxed_slice());
                DATA_WRITTEN.fetch_add(1, AtomicOrdering::Relaxed);
                push(&mut wire, len_tag, dv.len().to_string().as_bytes());
                push(&mut wire, tag, dv);
                fields.push((len_tag, b"999999" as &[u8]));
                fields.push((tag, dv));
            } else if order
                .iter()
                .any(|d| Fix44::data_length_tag(*d) == Some(tag))
            {
                // The length member of a DATA member in this group. It is
                // written when its data is, immediately in front — pushing it
                // here as an ordinary field would emit it twice.
            } else if plain < 2 {
                push(&mut wire, tag, v);
                fields.push((tag, v));
                plain += 1;
            }
        }

        // Handed over backwards. If the encoder wrote what it was given rather
        // than what the dictionary says, every message here would come back
        // reversed.
        fields.reverse();
        subs.reverse();
        datas.push(GroupEntryData {
            fields: Box::leak(fields.into_boxed_slice()),
            groups: Box::leak(subs.into_boxed_slice()),
        });
    }
    Built {
        wire,
        data: GroupData {
            counter,
            entries: Box::leak(datas.into_boxed_slice()),
        },
    }
}

/// How many DATA members this run actually wrote.
///
/// **A round-trip that covers no DATA member is a round-trip that proves
/// nothing about DATA**, and it would look exactly like one that did — the same
/// shape as a benchmark reporting zero allocations for a path that never ran.
/// The assertion at the end reads this.
static DATA_WRITTEN: AtomicUsize = AtomicUsize::new(0);

fn push(out: &mut Vec<u8>, tag: u32, value: &[u8]) {
    out.extend_from_slice(tag.to_string().as_bytes());
    out.push(b'=');
    out.extend_from_slice(value);
    out.push(SOH);
}

fn frame(body: &[u8]) -> Vec<u8> {
    let mut out = format!("8=FIX.4.4\x019={}\x01", body.len()).into_bytes();
    out.extend_from_slice(body);
    let sum: u32 = out.iter().map(|&b| u32::from(b)).sum::<u32>() % 256;
    out.extend_from_slice(format!("10={sum:03}\x01").as_bytes());
    out
}

#[test]
fn every_declared_group_round_trips_byte_for_byte() {
    let mut covered: BTreeSet<u32> = BTreeSet::new();
    let mut checked = 0usize;
    let mut out = [0u8; 8192];

    for (mt, counter) in GROUP_KEYS.iter() {
        // NoHops(627) is the header's, keyed under the empty message type
        // because it belongs to all 93. Exercise it under a real one.
        let mt: &'static [u8] = if mt.is_empty() { b"0" } else { mt };
        if !is_top_level(mt, *counter) {
            continue; // reached through its parent, and covered that way
        }
        let built = build(mt, *counter, 2, 1, &mut covered);

        let mut body = Vec::new();
        push(&mut body, 35, mt);
        push(&mut body, 34, b"2");
        push(&mut body, 49, b"TW");
        push(&mut body, 52, b"20260828-00:00:00.000");
        push(&mut body, 56, b"ISLD");
        push(&mut body, *counter, b"2");
        body.extend_from_slice(&built.wire);
        let expected = frame(&body);

        let mut idx = FieldIndex::<512>::new();
        parse_into::<Fix44, 512>(&expected, &mut idx, Validation::ALL)
            .unwrap_or_else(|e| panic!("{}/{counter} does not parse: {e:?}", show(mt)));

        let t = TemplateBuilder::<64, 1024>::new(b"FIX.4.4")
            .field(35, mt)
            .field(34, b"2")
            .field(49, b"TW")
            .field(52, b"20260828-00:00:00.000")
            .field(56, b"ISLD")
            .group(*counter)
            .build::<Fix44>()
            .unwrap_or_else(|e| panic!("{}/{counter} template: {e:?}", show(mt)));

        let r = t
            .encode_with::<Fix44>(&mut out, &[], &[built.data])
            .unwrap_or_else(|e| panic!("{}/{counter} encode: {e:?}", show(mt)));

        assert_eq!(
            String::from_utf8_lossy(&out[r.clone()]).replace('\x01', "|"),
            String::from_utf8_lossy(&expected).replace('\x01', "|"),
            "{}/{counter} did not round-trip",
            show(mt)
        );
        checked += 1;
    }

    println!(
        "round-tripped {checked} top-level positions, {} counters",
        covered.len()
    );
    let data = DATA_WRITTEN.load(AtomicOrdering::Relaxed);
    println!("wrote {data} DATA members, each with a separator inside its value");
    assert!(
        data >= 20,
        "the DATA members must actually be exercised; wrote {data}"
    );
    assert!(
        checked >= 357,
        "expected every top-level position, got {checked}"
    );
    assert_eq!(
        covered.len(),
        59,
        "every counter must be exercised, missing {:?}",
        (0..1000)
            .filter(|c| GROUP_KEYS.iter().any(|(_, k)| k == c) && !covered.contains(c))
            .collect::<Vec<_>>()
    );
}

fn show(mt: &[u8]) -> String {
    String::from_utf8_lossy(mt).into_owned()
}

#[test]
fn the_encoder_orders_a_group_and_the_caller_cannot() {
    // NoMDEntries in a snapshot: 269 first by declaration, then 270. Handed over
    // backwards, it must still come out forwards.
    let order = Fix44::group_order(b"W", 268);
    assert_eq!(order[0], 269, "the dictionary says 269 leads");
    let second = order[1];

    let t = TemplateBuilder::<16, 256>::new(b"FIX.4.4")
        .field(35, b"W")
        .field(49, b"TW")
        .field(56, b"ISLD")
        .group(268)
        .build::<Fix44>()
        .unwrap();
    let mut out = [0u8; 512];
    let entry = GroupEntryData {
        fields: &[(second, b"9".as_ref()), (269, b"0".as_ref())],
        groups: &[],
    };
    let r = t
        .encode_with::<Fix44>(
            &mut out,
            &[],
            &[GroupData {
                counter: 268,
                entries: &[entry],
            }],
        )
        .unwrap();
    let s = String::from_utf8_lossy(&out[r]).replace('\x01', "|");
    assert!(
        s.contains(&format!("268=1|269=0|{second}=9|")),
        "group written out of declaration order: {s}"
    );
}
