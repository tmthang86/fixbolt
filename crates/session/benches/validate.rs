//! What the dictionary pass costs on one inbound message.
//!
//! `STATUS.md` open item 39. `crates/codec/benches/parse.rs` reads 120.4 ns for
//! a `NewOrderSingle`, and that figure is **framing, field indexing, `9=` and
//! `10=`** — it parses with `NoDict`, a dictionary whose every answer is a
//! no-op. The session then runs a second, separate pass over the same message,
//! and until this file existed nothing timed it.
//!
//! # What is inside the timed region
//!
//! [`fixbolt_session::validate`] and nothing else: the wire-order field scan
//! (`is_header`, `is_defined_tag`, `field_type`, `allows`, the duplicate check,
//! `enum_allows`, `accepts`), then the required-tag lookups for the header and
//! for this message type — each a linear scan of the field index — then the
//! group counters. The parse happens **once, outside the loop**, because the
//! parse already has its own three cases and adding it here would price it
//! twice.
//!
//! # What is not
//!
//! Everything else `Session::received_with` does: sequence numbers, CompIDs,
//! `SendingTime`, the schedule, the application callback, and any send. So this
//! is the pass, not the message.
//!
//! # Two messages, because item 39 says the cost is per field and per required
//! tag
//!
//! `NewOrderSingle` carries 14 fields against `Heartbeat`'s 6, and ~13 required
//! tags against ~8. They are the two ends of the range, and the same two
//! messages `parse.rs` uses — so the two files' figures can be read side by
//! side.
//!
//! # And the two messages `tools/w2w` actually sends
//!
//! The two cases above are the shapes `parse.rs` uses, so the two files read
//! side by side. They are **not** the bytes the wire-to-wire figures are about:
//! `tools/w2w` sends a `TestRequest` on `--path admin` and a `NewOrderSingle`
//! with `44=Price` and `40=2` on `--path app`, and `DESIGN.md` §8's 3 898 ns is
//! the difference between those two round trips. Subtracting one pass from the
//! other only means something if both are the bytes that were sent, so both are
//! cases here too, copied field for field from `tools/w2w/src/main.rs` with a
//! fixed timestamp in place of the live one.
//!
//! # Fault-free messages only, on purpose
//!
//! `validate` returns on the first fault, so a faulty message measures a
//! *prefix* of the pass and the prefix depends on where the fault is. The
//! figure item 39 asks for is the whole pass, which is what a good message
//! runs. It is also the only path where `validate` is codegen-identical to what
//! `Session::judge` runs, since the `371=` tag reference `validate` drops is
//! built only on the faulting arms.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
#[path = "../../codec/benches/harness.rs"]
mod harness;

use fixbolt_codec::{FieldIndex, Parsed, Validation, parse_into};
use fixbolt_dict::Fix44;
use fixbolt_session::validate;
use std::hint::black_box;

#[cfg(feature = "fix50sp2")]
use fixbolt_dict::Fixt11Fix50Sp2Tables;

fn main() {
    harness::suite(|b| {
        // The same two messages as `crates/codec/benches/parse.rs`.
        let nos: &[u8] = b"8=FIX.4.4\x019=125\x0135=D\x0134=2\x0149=TW44\x01\
52=20260905-12:00:00.000\x0156=ISLD\x0111=ID\x0121=1\x0138=002000.00\x0140=1\x01\
54=1\x0155=INTC\x0160=20260905-12:00:00.000\x01167=CS\x0110=076\x01";
        let hb: &[u8] = b"8=FIX.4.4\x019=51\x0135=0\x0134=2\x0149=TW44\x01\
52=20260905-12:00:00.000\x0156=ISLD\x0110=253\x01";

        // Parsed with `Fix44`, not `NoDict`: the session's own index is built
        // by `parse_into::<Fix44, N>`, and a group-aware index is what the pass
        // walks. Outside the loop — the parse is priced in `parse.rs`.
        let mut nos_idx: FieldIndex<64> = FieldIndex::new();
        let r = parse_into::<Fix44, 64>(nos, &mut nos_idx, Validation::ALL);
        assert!(
            matches!(r, Ok(Parsed::Complete { .. })),
            "NewOrderSingle {r:?}"
        );
        let mut hb_idx: FieldIndex<64> = FieldIndex::new();
        let r = parse_into::<Fix44, 64>(hb, &mut hb_idx, Validation::ALL);
        assert!(matches!(r, Ok(Parsed::Complete { .. })), "Heartbeat {r:?}");

        let nos_view = nos_idx.view(nos);
        let hb_view = hb_idx.view(hb);

        // Both messages must be fault-free, or the case times a prefix of the
        // pass and says nothing. Asserted here rather than assumed: this is the
        // one thing that would make every figure below quietly wrong, and a
        // bench that measures a first-field reject would still look stable.
        assert_eq!(
            validate::<Fix44, 64>(&nos_view, b"D"),
            None,
            "NewOrderSingle is clean"
        );
        assert_eq!(
            validate::<Fix44, 64>(&hb_view, b"0"),
            None,
            "Heartbeat is clean"
        );

        b.bench("validate NewOrderSingle", || {
            let v = validate::<Fix44, 64>(black_box(&nos_view), black_box(b"D"));
            black_box(v);
        });
        b.bench("validate Heartbeat", || {
            let v = validate::<Fix44, 64>(black_box(&hb_view), black_box(b"0"));
            black_box(v);
        });

        // The bytes `tools/w2w` sends, field for field — `test_request` and
        // `new_order_single` in `tools/w2w/src/main.rs`, with a fixed timestamp
        // in place of `stamp()`. These two are what `DESIGN.md` §8's 3 898 ns
        // is a difference of, so these two are what may be subtracted.
        let w_tr: &[u8] = b"8=FIX.4.4\x019=57\x0135=1\x0134=2\x0149=W2W\x01\
52=20260905-12:00:00.000\x0156=ISLD\x01112=W1\x0110=043\x01";
        let w_nos: &[u8] = b"8=FIX.4.4\x019=126\x0135=D\x0134=2\x0149=W2W\x01\
52=20260905-12:00:00.000\x0156=ISLD\x0111=W1\x0121=1\x0138=002000.00\x0140=2\x01\
44=20.15\x0154=1\x0155=INTC\x0160=20260905-12:00:00.000\x0110=064\x01";

        let mut w_tr_idx: FieldIndex<64> = FieldIndex::new();
        let r = parse_into::<Fix44, 64>(w_tr, &mut w_tr_idx, Validation::ALL);
        assert!(
            matches!(r, Ok(Parsed::Complete { .. })),
            "w2w TestRequest {r:?}"
        );
        let mut w_nos_idx: FieldIndex<64> = FieldIndex::new();
        let r = parse_into::<Fix44, 64>(w_nos, &mut w_nos_idx, Validation::ALL);
        assert!(
            matches!(r, Ok(Parsed::Complete { .. })),
            "w2w NewOrderSingle {r:?}"
        );

        let w_tr_view = w_tr_idx.view(w_tr);
        let w_nos_view = w_nos_idx.view(w_nos);
        assert_eq!(
            validate::<Fix44, 64>(&w_tr_view, b"1"),
            None,
            "w2w TestRequest is clean"
        );
        assert_eq!(
            validate::<Fix44, 64>(&w_nos_view, b"D"),
            None,
            "w2w NewOrderSingle is clean"
        );

        b.bench("validate TestRequest, w2w bytes", || {
            let v = validate::<Fix44, 64>(black_box(&w_tr_view), black_box(b"1"));
            black_box(v);
        });
        b.bench("validate NewOrderSingle, w2w bytes", || {
            let v = validate::<Fix44, 64>(black_box(&w_nos_view), black_box(b"D"));
            black_box(v);
        });

        // The same pass over the FIXT 1.1 / FIX 5.0 SP2 tables. The same
        // fourteen fields as `validate NewOrderSingle` above, with
        // `8=FIXT.1.1` and this side's CompID, so the two figures may be read
        // side by side and the difference is the **table**, not the message.
        //
        // The FIXT pair is a much larger table — 25 929 generated `(msg_type,
        // counter)` group pairs against FIX 4.4's 731, and 164 message types
        // against 93 — so the per-field `match` arms this pass walks are not
        // the same code at all, even though the fields are.
        //
        // `[machine 2026-09-19]` no baseline exists for this case on any CPU:
        // `benches/baselines.tsv` records a Ryzen and this is not it, so the
        // harness prints NO BASELINE and compares it against nothing. That is
        // the expected reading here and it is **not** a published figure —
        // `CLAUDE.md` §2 non-negotiable 10.
        #[cfg(feature = "fix50sp2")]
        {
            let fixt_nos: &[u8] = b"8=FIXT.1.1\x019=128\x0135=D\x0134=2\x0149=TW50SP2\x01\
52=20260905-12:00:00.000\x0156=ISLD\x0111=ID\x0121=1\x0138=002000.00\x0140=1\x01\
54=1\x0155=INTC\x0160=20260905-12:00:00.000\x01167=CS\x0110=111\x01";
            let mut fixt_idx: FieldIndex<64> = FieldIndex::new();
            let r =
                parse_into::<Fixt11Fix50Sp2Tables, 64>(fixt_nos, &mut fixt_idx, Validation::ALL);
            assert!(
                matches!(r, Ok(Parsed::Complete { .. })),
                "FIXT NewOrderSingle {r:?}"
            );
            let fixt_view = fixt_idx.view(fixt_nos);
            // Fault-free, asserted for the reason the header gives: `validate`
            // returns on the first fault, so a faulty message times a prefix of
            // the pass and the figure would be stable and meaningless.
            assert_eq!(
                validate::<Fixt11Fix50Sp2Tables, 64>(&fixt_view, b"D"),
                None,
                "FIXT NewOrderSingle is clean"
            );
            b.bench("validate NewOrderSingle (FIXT tables)", || {
                let v =
                    validate::<Fixt11Fix50Sp2Tables, 64>(black_box(&fixt_view), black_box(b"D"));
                black_box(v);
            });

            // The timing twin of `crates/session/benches/alloc.rs`'s
            // `validate TradeCaptureReport (33 groups)` — same bytes, same
            // fixture-building comment there in full. 33 group counters fill
            // `SeenCounters` (ADR-0085), so this is the only case in this file
            // that ever reaches `defers`'s `full` branch and
            // `in_a_group_before` rather than the array; `parse.rs` carries no
            // twin because the shape this measures is the session's own pass,
            // not the parse.
            //
            // `[machine 2026-09-19]` no baseline exists for this case either,
            // for the same reason given above: NO BASELINE printed is the
            // expected reading, not a failure.
            let clean = fixt_trade_capture_report("1", "D", "D");
            let mut clean_idx: FieldIndex<256> = FieldIndex::new();
            let r =
                parse_into::<Fixt11Fix50Sp2Tables, 256>(&clean, &mut clean_idx, Validation::ALL);
            assert!(
                matches!(r, Ok(Parsed::Complete { .. })),
                "the 33-counter TradeCaptureReport fixture must parse: {r:?}"
            );
            let clean_view = clean_idx.view(&clean);
            assert_eq!(
                validate::<Fixt11Fix50Sp2Tables, 256>(&clean_view, b"AE"),
                None,
                "the 33-counter TradeCaptureReport must be fault-free"
            );
            b.bench("validate TradeCaptureReport (33 groups)", || {
                let v =
                    validate::<Fixt11Fix50Sp2Tables, 256>(black_box(&clean_view), black_box(b"AE"));
                black_box(v);
            });
        }
    });
}

/// `crates/session/benches/alloc.rs`'s `AE_GROUPS` and `trade_capture_report`,
/// unchanged — see that file for why these bytes and why they are duplicated
/// rather than shared.
#[cfg(feature = "fix50sp2")]
const AE_GROUPS: [(&str, &str, &str); 32] = [
    ("1907", "1903", "X"),
    ("1116", "1117", "R"),
    ("454", "455", "A"),
    ("1976", "1977", "1"),
    ("2304", "2305", "A"),
    ("1018", "1019", "P"),
    ("40278", "40471", "B"),
    ("41230", "41231", "B"),
    ("41092", "41093", "E"),
    ("41094", "41095", "F"),
    ("42775", "42776", "B"),
    ("41116", "41117", "B"),
    ("41137", "41138", "20260919"),
    ("41140", "41141", "B"),
    ("41152", "41153", "20260919"),
    ("40019", "40020", "Y"),
    ("40181", "40182", "1.0"),
    ("40022", "40023", "USD"),
    ("40204", "40209", "0"),
    ("42296", "42297", "E"),
    ("2734", "2733", "M"),
    ("2746", "2747", "20260919-12:00:00.000"),
    ("40040", "40041", "D"),
    ("40046", "40047", "S"),
    ("40042", "40043", "M"),
    ("711", "311", "U"),
    ("1703", "1704", "1.0"),
    ("555", "600", "L"),
    ("768", "769", "20260919-12:00:00.000"),
    ("1387", "1388", "1"),
    ("41312", "41313", "J"),
    ("2104", "2105", "A"),
];

#[cfg(feature = "fix50sp2")]
fn fixt_trade_capture_report(
    regulatory_count: &str,
    stray: &str,
    nested_party_id_source: &str,
) -> Vec<u8> {
    let mut body = String::from(
        "35=AE\x0134=2\x0149=TW50SP2\x0152=20260828-12:00:00\x01\
56=ISLD\x01",
    );
    for (counter, delimiter, value) in AE_GROUPS {
        let count = if counter == "1907" {
            regulatory_count
        } else {
            "1"
        };
        body.push_str(&format!("{counter}={count}\x01{delimiter}={value}\x01"));
    }
    body.push_str(&format!("447={stray}\x01"));
    body.push_str(&format!(
        "552=1\x0154=1\x01453=1\x01448=A\x01447={nested_party_id_source}\x01452=1\x01"
    ));
    let mut m = format!("8=FIXT.1.1\x019={}\x01", body.len()).into_bytes();
    m.extend_from_slice(body.as_bytes());
    let sum: u32 = m.iter().map(|c| u32::from(*c)).sum();
    m.extend_from_slice(format!("10={:03}\x01", sum % 256).as_bytes());
    m
}
