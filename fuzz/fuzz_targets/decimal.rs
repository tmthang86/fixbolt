//! `as_decimal` and `Decimal::format` on whatever bytes arrive in a field.
//!
//! ADR-0120, plan `docs/plans/2026-09-23-p3-decimal.md` step 4. Three
//! properties, each one a sentence of that ADR:
//!
//! 1. No panic, on any input. Clippy denies `unwrap`/`expect`/`panic!` and
//!    `indexing_slicing` in `crates/codec/src`; only running the code proves
//!    no arithmetic or slice was missed.
//! 2. Decision 5(b): a value that parses writes back to bytes that parse to
//!    the same `Decimal`. Every parsed value has `exponent <= 0` and a
//!    mantissa other than `i64::MIN`, which is exactly (b)'s domain.
//! 3. Decision 3: `FieldType::Price.accepts(v)` is true exactly when
//!    `as_decimal(v)` is `Ok` or `Err(Overflow)` — the session's grammar and
//!    the codec's are two pieces of code, and this is the rule binding them.
//!
//! Run: `cargo +nightly fuzz run decimal -- -max_total_time=60`

#![no_main]

use fixbolt_codec::{ConvertError, Decimal, as_decimal};
use fixbolt_dict::FieldType;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let parsed = as_decimal(data);

    let codec_accepts = matches!(parsed, Ok(_) | Err(ConvertError::Overflow));
    assert_eq!(
        FieldType::Price.accepts(data),
        codec_accepts,
        "the session and the codec disagree on {data:?}: as_decimal says {parsed:?}"
    );

    if let Ok(d) = parsed {
        assert!(d.exponent() <= 0, "a FIX-parsed exponent is never positive: {d:?}");
        let mut out = [0u8; Decimal::MAX_LEN];
        let written = d.format(&mut out);
        assert_eq!(
            as_decimal(written),
            Ok(d),
            "{d:?} wrote {written:?}, which does not read back as itself"
        );
    }
});
