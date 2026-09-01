//! What repeating groups cost, and the one number the plan asked for.
//!
//! `group_members` returns the dictionary's declaration order, so membership is
//! a **linear scan**, and the longest member list in FIX 4.4 is `(AE, 552)` with
//! 61 tags. The plan flagged that as an open question and refused to optimise
//! it without a number. These are the numbers.
//!
//! Read `walk` against `parse`: a group is only walked if something asks for it,
//! so a message with no groups pays none of this.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
#[path = "harness.rs"]
mod harness;

use std::hint::black_box;

use fixbolt_codec::{
    Dictionary, FieldIndex, GroupData, GroupEntryData, TemplateBuilder, Validation, parse_into,
};
use fixbolt_dict::Fix44;

fn main() {
    harness::suite(|b| {
        // The one populated group in the acceptance corpus: NoTradingSessions in a
        // NewOrderSingle, two entries, member list of 2.
        let small: &[u8] = b"8=FIX.4.4\x019=110\x0135=D\x0134=2\x0149=TW44\x01\
52=00000000-00:00:00.000\x0156=ISLD\x0111=ID\x0140=1\x0154=1\x0155=INTC\x01\
386=2\x01336=PRE-OPEN\x01336=AFTER-HOURS\x0160=00000000-00:00:00.000\x0110=000\x01";

        // TradeCaptureReport, nested to the deepest chain FIX 4.4 has:
        // 552 -> 78 -> 756 -> 806. The 552 member list is the 61-tag one.
        let deep: &[u8] = b"8=FIX.4.4\x019=163\x0135=AE\x0134=2\x0149=TW\x0156=ISLD\x01\
52=20260828-00:00:00.000\x01571=TRID\x01487=0\x01856=0\x01828=0\x01552=1\x0154=1\x01\
37=ORD1\x0178=1\x0179=ACC1\x01756=1\x01757=NP1\x01806=1\x01760=SUB1\x0180=50\x01\
60=20260828-00:00:00.000\x0110=000\x01";

        let mut si: FieldIndex<64> = FieldIndex::new();
        parse_into::<Fix44, 64>(small, &mut si, Validation::NONE).expect("small parses");
        let mut di: FieldIndex<64> = FieldIndex::new();
        parse_into::<Fix44, 64>(deep, &mut di, Validation::NONE).expect("deep parses");

        b.bench("walk 1 group, 2 entries, 2 members", || {
            let v = si.view(black_box(small));
            let mut n = 0u32;
            for e in v.group::<Fix44>(b"D", 386).expect("386") {
                n += u32::from(e.get(336).is_some());
            }
            black_box(n);
        });

        b.bench("walk 4 levels, 61-tag member list", || {
            let v = di.view(black_box(deep));
            let mut n = 0u32;
            for side in v.group::<Fix44>(b"AE", 552).expect("552") {
                for a in side.group::<Fix44>(b"AE", 78).expect("78") {
                    for p in a.group::<Fix44>(b"AE", 756).expect("756") {
                        for sub in p.group::<Fix44>(b"AE", 806).expect("806") {
                            n += u32::from(sub.get(760).is_some());
                        }
                    }
                }
            }
            black_box(n);
        });

        // Membership alone, with no scanning around it: the cost the plan asked
        // about, isolated. 80 is the last member of the 6-tag list; 591 sits deep
        // in the 61-tag one. `[measured 2026-08-28]` 5.6 ns on an Apple M5 — this
        // is the answer to the plan's open question: a linear scan of the longest
        // member list FIX 4.4 has is cheap enough that no second sorted table is
        // bought.
        b.bench("group_members contains, 61 tags", || {
            let m = Fix44::group_members(black_box(b"AE"), black_box(552));
            black_box(m.contains(&black_box(591)));
        });

        let t = TemplateBuilder::<32, 512>::new(b"FIX.4.4")
            .field(35, b"D")
            .field(49, b"ISLD")
            .field(56, b"TW44")
            .field(34, b"2")
            .field(52, b"00000000-00:00:00.000")
            .group(386)
            .build::<Fix44>()
            .expect("template");
        let mut out = [0u8; 512];
        let entries = [
            GroupEntryData {
                fields: &[(336, b"PRE-OPEN".as_ref())],
                groups: &[],
            },
            GroupEntryData {
                fields: &[(336, b"AFTER-HOURS".as_ref())],
                groups: &[],
            },
        ];
        let g = [GroupData {
            counter: 386,
            entries: &entries,
        }];
        b.bench("encode 1 group, 2 entries", || {
            let r = t.encode_with::<Fix44>(&mut out, &[], black_box(&g));
            black_box(r).ok();
            // Audited 2026-09-01 alongside `dispatch.rs`'s deleted copy: this
            // case did NOT move (107.4 -> 104.9, inside its own spread), so
            // nothing was being elided. Kept so that stays true by construction
            // rather than by the optimiser's current mood.
            black_box(&out);
        });
    });
}
