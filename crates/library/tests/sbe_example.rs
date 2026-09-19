//! Proves `examples/sbe_decode.rs` — the same schema and the same bytes,
//! through `examples/shared/sbe_car.rs`, so this cannot pass by decoding a
//! second copy that merely agrees with itself.

#![cfg(feature = "sbe")]
// A test binary, not a library crate: non-negotiable 7 is about what ships.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use fixbolt::sbe::Value;

#[path = "../examples/shared/sbe_car.rs"]
mod sbe_car;

#[test]
fn the_example_decodes_the_fields_it_wrote() {
    let bytes = sbe_car::wire_bytes();
    let car = sbe_car::decode(&bytes).expect("SimplifiedCar decodes");

    assert_eq!(car.name, "SimplifiedCar");
    assert_eq!(car.template_id, 1);
    assert_eq!(car.serial_number, Some(Value::UInt(123_456_789)));
    assert_eq!(car.model_year, Some(Value::UInt(2026)));
    assert_eq!(car.available, Some(Value::Char(b'Y')));
    assert_eq!(car.code, Some(Value::Array(b"CAR1")));
}
