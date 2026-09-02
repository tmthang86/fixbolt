//! What the convenience layer costs, so that [ADR-0041] has a number rather
//! than an adjective.
//!
//! `CLAUDE.md` §2 non-negotiable 10: no performance claim without the committed
//! benchmark that produced it. ADR-0041 says the library layer buys its API
//! with a second parse and a template build; these are the three cases that
//! say how much, and they are split so that the answer is attributable rather
//! than a single lump:
//!
//! | Case | What it is |
//! |---|---|
//! | `library, parse only` | the second parse, alone — the half the session already did |
//! | `library, reply only` | one `TemplateBuilder`, built and encoded, with no parse |
//! | `library, on_message` | the whole of `App::on_message`, which is both plus the handler |
//!
//! **They do not quite add up, and that is recorded rather than smoothed
//! over.** `[measured 2026-09-02, Intel Xeon @ 2.80GHz, three runs]` parse
//! 188-195, reply 2135-2150, `on_message` 2062-2131 — so the whole is about
//! **the reply alone**, roughly 200 ns *less* than the sum, against a
//! run-to-run spread of ~3%. The gap is the size of the parse, which is the
//! honest reading: measured in isolation the parse costs ~190 ns, and measured
//! inside `on_message` it costs about nothing that this harness can see. What
//! is **not** claimed is why. What the three numbers do settle is the question
//! ADR-0041 asks, and they settle it the other way round from the guess in its
//! plan: the second parse is ~9% of the path and the template build is the
//! rest.
//!
//! # What is not here
//!
//! A comparison against a hand-written [`fixbolt_session::Application`]. There
//! is nothing to compare: a handler that reads `11`, `55` and `54` has to parse
//! the message however it is written, so the honest question is *"what does the
//! parse cost"* — which `library, parse only` answers — and not *"what does the
//! library cost against an application that does less".*
//!
//! [ADR-0041]: ../../../docs/decisions/ADR-0041-the-library-layer-buys-an-api-with-a-template-per-message.md
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

#[path = "../../codec/benches/harness.rs"]
mod harness;

use std::hint::black_box;

use fixbolt::{Answer, App, Application, Handler, Incoming, Reply};
use fixbolt_codec::{FieldIndex, Validation, parse_into};
use fixbolt_dict::Fix44;

const STAMP: &[u8] = b"20260902-10:00:00.123";

/// A `NewOrderSingle` with a correct frame. A frame one byte out makes
/// `parse_into` bail, and the bench then times a path it never walked.
fn order() -> Vec<u8> {
    let body = "35=D\u{1}34=2\u{1}49=TW44\u{1}52=20260902-10:00:00\u{1}56=ISLD\u{1}\
                11=ORD-1\u{1}21=1\u{1}38=100\u{1}40=2\u{1}44=42\u{1}54=1\u{1}55=IBM\u{1}\
                59=0\u{1}60=20260902-10:00:00\u{1}";
    let whole = format!("8=FIX.4.4\u{1}9={}\u{1}{body}", body.len());
    let sum: u32 = whole.bytes().map(u32::from).sum();
    format!("{whole}10={:03}\u{1}", sum % 256).into_bytes()
}

/// The same desk `examples/shared/order_handler.rs` runs, without its counter:
/// a counter that overflows mid-run would change what is being timed.
struct Desk;

impl Handler for Desk {
    fn on_message(&mut self, msg: &Incoming<'_>, reply: Reply<'_>) -> Answer {
        if msg.msg_type() != b"D" {
            return reply.silent();
        }
        reply
            .message(b"8")
            .field(37, b"EXEC-1")
            .field(17, b"EXEC-1")
            .field(150, b"F")
            .field(39, b"2")
            .field(11, msg.get(11).unwrap_or(b""))
            .field(55, msg.get(55).unwrap_or(b""))
            .field(54, msg.get(54).unwrap_or(b""))
            .field(38, msg.get(38).unwrap_or(b""))
            .field(31, msg.get(44).unwrap_or(b""))
            .field(151, b"0")
            .send()
    }
}

fn main() {
    let wire = order();
    let mut out = [0u8; 4096];

    harness::suite(|b| {
        // The second parse, on its own. This is the cost ADR-0041 is about.
        let mut idx: FieldIndex<256> = FieldIndex::new();
        assert!(
            parse_into::<Fix44, 256>(&wire, &mut idx, Validation::NONE).is_ok(),
            "the fixture must parse, or this times a bail-out"
        );
        b.bench("library, parse only", || {
            black_box(parse_into::<Fix44, 256>(
                black_box(&wire),
                &mut idx,
                Validation::NONE,
            ))
            .ok();
        });

        // The reply alone: a template built and encoded, no parse in the window.
        b.bench("library, reply only", || {
            let r =
                Reply::<64, 1024>::new(b"FIX.4.4", black_box(7), STAMP, b"ISLD", b"TW44", &mut out);
            black_box(
                r.message(b"8")
                    .field(37, b"EXEC-1")
                    .field(17, b"EXEC-1")
                    .field(150, b"F")
                    .field(39, b"2")
                    .field(11, b"ORD-1")
                    .field(55, b"IBM")
                    .field(54, b"1")
                    .field(38, b"100")
                    .field(31, b"42")
                    .field(151, b"0")
                    .send(),
            );
        });

        // The whole path an application message takes through this crate.
        let mut app = App::<Desk>::with_sizes(Desk);
        assert!(
            app.on_message(&wire, 7, STAMP, &mut out).is_some(),
            "the path must reply before it is timed"
        );
        b.bench("library, on_message", || {
            black_box(app.on_message(black_box(&wire), 7, STAMP, &mut out));
        });
    });
}
