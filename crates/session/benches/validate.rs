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
        assert_eq!(validate(&nos_view, b"D"), None, "NewOrderSingle is clean");
        assert_eq!(validate(&hb_view, b"0"), None, "Heartbeat is clean");

        b.bench("validate NewOrderSingle", || {
            let v = validate(black_box(&nos_view), black_box(b"D"));
            black_box(v);
        });
        b.bench("validate Heartbeat", || {
            let v = validate(black_box(&hb_view), black_box(b"0"));
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
        assert_eq!(validate(&w_tr_view, b"1"), None, "w2w TestRequest is clean");
        assert_eq!(
            validate(&w_nos_view, b"D"),
            None,
            "w2w NewOrderSingle is clean"
        );

        b.bench("validate TestRequest, w2w bytes", || {
            let v = validate(black_box(&w_tr_view), black_box(b"1"));
            black_box(v);
        });
        b.bench("validate NewOrderSingle, w2w bytes", || {
            let v = validate(black_box(&w_nos_view), black_box(b"D"));
            black_box(v);
        });
    });
}
