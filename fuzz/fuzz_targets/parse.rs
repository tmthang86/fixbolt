//! `parse_into` must never panic and never read out of bounds, whatever arrives
//! on the socket.
//!
//! The three properties asserted here are the ones a counterparty can attack:
//!
//! 1. No panic. `crates/codec` has `clippy::panic`, `unwrap_used` and
//!    `expect_used` denied, but an index or a slice can still panic; only running
//!    the code proves otherwise.
//! 2. `Complete { consumed }` never claims more bytes than exist. A caller that
//!    trusts it would then slice past the end of its own read buffer.
//! 3. Every field the index reports is inside the consumed prefix. This is what
//!    makes `LengthOutOfBounds` load-bearing: a DATA length is attacker-supplied.
//!
//! Run: `cargo +nightly fuzz run parse -- -max_total_time=600`

#![no_main]

use libfuzzer_sys::fuzz_target;
use fixbolt_codec::{parse_into, FieldIndex, Parsed, Validation};
use fixbolt_dict::Fix44;

fuzz_target!(|data: &[u8]| {
    for v in [Validation::ALL, Validation::NONE] {
        let mut idx: FieldIndex<64> = FieldIndex::new();
        let Ok(Parsed::Complete { consumed }) = parse_into::<Fix44, 64>(data, &mut idx, v) else {
            continue;
        };

        assert!(
            consumed <= data.len(),
            "consumed {consumed} of {} bytes",
            data.len()
        );

        let view = idx.view(data);
        for i in 0..view.len() {
            let Some((tag, value)) = view.field_at(i) else {
                panic!("field_at({i}) is None while len() is {}", view.len());
            };
            // The value must lie inside the message that was reported complete.
            let start = value.as_ptr() as usize - data.as_ptr() as usize;
            assert!(
                start + value.len() <= consumed,
                "tag {tag} runs to {} but only {consumed} bytes were consumed",
                start + value.len()
            );
        }
    }
});
