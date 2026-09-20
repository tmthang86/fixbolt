//! The one `NewOrderSingle` every benchmark in this repository is measured on.
//!
//! **Not a bench target.** `codec` has `autobenches = false` and this file is
//! included by path, the way `harness.rs` is. Both spellings, because a
//! `#[path]` include is a coupling cargo does not see and a rename breaks the
//! includer at its next build rather than at the rename:
//!
//! ```ignore
//! #[path = "fixture.rs"] mod fixture;                        // from codec
//! #[path = "../../codec/benches/fixture.rs"] mod fixture;    // from any other crate
//! ```
//!
//! # Why the message lives here and nowhere else
//!
//! It was pasted into four bench files, carrying `10=098` against an actual
//! checksum of 097. It was found and corrected on 2026-09-05 in one of the
//! four, found again on 2026-09-19 in a second, and filed as `STATUS.md` item
//! 94 on 2026-09-20 — three sightings, one file fixed each time, because the
//! fixture lived in three more. ADR-0089 ends that by giving it one source and
//! two tests: `crates/codec/tests/bench_fixture.rs` parses this constant on
//! every `cargo test --all`, and refuses a fifth copy appearing under
//! `crates/*/benches/`.
//!
//! See [`a-benchmark-parsed-a-message-the-parser-rejects`] (the mechanism) and
//! [`a-bench-message-that-fails-its-own-checksum`] (the correction that reached
//! one file of four).
//!
//! [`a-benchmark-parsed-a-message-the-parser-rejects`]: ../../../docs/reference/a-benchmark-parsed-a-message-the-parser-rejects.md
//! [`a-bench-message-that-fails-its-own-checksum`]: ../../../docs/reference/a-bench-message-that-fails-its-own-checksum.md

/// The `NewOrderSingle` from `docs/reference/measured-costs.md`.
///
/// Codec-valid, **not** dictionary-valid: `52=00000000-00:00:00.000` is not a
/// timestamp any dictionary accepts, which is why [`assert_valid`] parses with
/// `NoDict`. A bench that one day validates this against `Fix44` needs a second
/// fixture, and `assert_valid` will say so on its first run.
pub const NEW_ORDER_SINGLE: &[u8] = b"8=FIX.4.4\x019=126\x0135=D\x0134=2\x0149=TW44\x01\
52=00000000-00:00:00.000\x0156=ISLD\x0111=ID\x0121=1\x0138=002000.00\x0140=1\x01\
54=1\x0155=INTC\x0160=00000000-00:00:00.000\x01167=BOO\x0110=097\x01";

/// Parse [`NEW_ORDER_SINGLE`] under `Validation::ALL` and panic unless the
/// whole message was consumed and the index was really filled.
///
/// **Every bench that takes the constant calls this before it counts or times
/// anything** — including the two engine benches that hand the bytes to
/// `deliver` and never read the checksum digit. "This bench does not parse the
/// message" is the sentence that kept the wrong bytes alive for fifteen days.
pub fn assert_valid() {
    let mut idx: fixbolt_codec::FieldIndex<64> = fixbolt_codec::FieldIndex::new();
    let r = fixbolt_codec::parse_into::<fixbolt_codec::NoDict, 64>(
        NEW_ORDER_SINGLE,
        &mut idx,
        fixbolt_codec::Validation::ALL,
    );
    assert_eq!(
        r,
        Ok(fixbolt_codec::Parsed::Complete {
            consumed: NEW_ORDER_SINGLE.len()
        }),
        "the shared bench message must parse clean under full validation"
    );
    assert_eq!(
        idx.view(NEW_ORDER_SINGLE).get(55),
        Some(b"INTC".as_ref()),
        "the shared bench message must really fill the index"
    );
}
