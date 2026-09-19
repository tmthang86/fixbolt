//! Decoding one SBE message with no socket, no session, no `serve*`.
//!
//! `fixbolt::sbe` is `fixbolt-sbe` re-exported behind the `sbe` feature
//! (ADR-0078, ADR-0082 decision 4): SBE is a codec, the caller brings the
//! transport. This example brings a `Vec<u8>` standing in for one.
//!
//! The schema is `examples/shared/sbe_car.rs`, the same file
//! `tests/sbe_example.rs` decodes — hand-written, not `sbe-gen`'s output.
//!
//! ```text
//! cargo run -p fixbolt --example sbe_decode --features sbe
//! ```

#[path = "shared/sbe_car.rs"]
mod sbe_car;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let bytes = sbe_car::wire_bytes();
    let car = sbe_car::decode(&bytes).map_err(|e| format!("sbe decode failed: {e:?}"))?;

    println!("message: {} (template {})", car.name, car.template_id);
    println!("SerialNumber = {:?}", car.serial_number);
    println!("ModelYear    = {:?}", car.model_year);
    println!("Available    = {:?}", car.available);
    println!("Code         = {:?}", car.code);
    Ok(())
}
