//! C3(2): a hand-assembled `Car` message, decoded through the **generated**
//! `car.rs` table (`fixbolt-sbe-gen`'s output for Real Logic's
//! `example-schema.xml` + `common-types.xml`). Decode-only here —
//! re-encoding (a byte-for-byte round trip) is C4's job, not this one's, so
//! the file name is kept from the plan but nothing in it writes bytes back
//! out through the generated tables.
//!
//! `support::car_message()` builds the bytes from the same named constants
//! this file asserts against (see that module's doc for why: it is not an
//! independent re-derivation of the schema's offsets — `tests/generated.rs`
//! already is that, by hand, from the XML). What this file checks is that
//! every field *kind* Car exercises — char arrays, enums, a set, a
//! composite with a one-level `<ref>`, a fixed numeric array, constants, a
//! nested group, and all three `varData` — reads back through the generated
//! tables as what was written.
//!
//! Same lint relaxation as `tests/generated.rs` (an integration test is its
//! own crate root; a panic here is the test failing).
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use fixbolt_sbe::{SbeError, SbeView, Schema, Value};

mod support;
use support::{
    CAR_ACCEL_0_MPH_0, CAR_ACCEL_0_MPH_1, CAR_ACCEL_0_SECONDS_0, CAR_ACCEL_0_SECONDS_1,
    CAR_ACTIVATION_CODE, CAR_AVAILABLE, CAR_CODE, CAR_ENGINE_BOOSTER_ENABLED,
    CAR_ENGINE_BOOSTER_HORSEPOWER, CAR_ENGINE_BOOSTER_TYPE, CAR_ENGINE_CAPACITY,
    CAR_ENGINE_EFFICIENCY, CAR_ENGINE_MANUFACTURER_CODE, CAR_ENGINE_NUM_CYLINDERS, CAR_EXTRAS,
    CAR_FUEL_MPG_0, CAR_FUEL_MPG_1, CAR_FUEL_NOTE_0, CAR_FUEL_NOTE_1, CAR_FUEL_SPEED_0,
    CAR_FUEL_SPEED_1, CAR_MANUFACTURER, CAR_MODEL, CAR_MODEL_YEAR, CAR_OCTANE_0, CAR_OCTANE_1,
    CAR_SERIAL_NUMBER, CAR_SOME_NUMBERS, CAR_VEHICLE_CODE, car_message,
};

mod car_generated {
    include!(concat!(env!("OUT_DIR"), "/car.rs"));
}
use car_generated::Baseline;

fn f(layout: &'static fixbolt_sbe::MessageLayout, id: u16) -> &'static fixbolt_sbe::FieldLayout {
    layout
        .field(id)
        .unwrap_or_else(|| panic!("no field id {id} in {}", layout.name))
}

/// Reads a `length=4` `uint32` array (`someNumbers`) out of its raw
/// little-endian bytes — `Value::Array` hands back wire bytes for any
/// numeric array; the caller decodes them (`schema.rs`'s own documented
/// rule for a numeric, non-`char` array).
fn u32_array_le(bytes: &[u8]) -> Vec<u32> {
    bytes
        .as_chunks::<4>()
        .0
        .iter()
        .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

#[test]
fn car_decodes_every_field_kind_through_the_generated_table() -> Result<(), SbeError> {
    let msg = car_message();
    let v = SbeView::decode::<Baseline>(&msg)?;
    assert_eq!((v.template_id(), v.schema_id(), v.version()), (1, 1, 0));
    assert_eq!(v.layout::<Baseline>()?.name, "Car");

    let car = Baseline::message(1).expect("template 1 (Car)");
    assert_eq!(car.block_length, 45);
    let root = v.root::<Baseline>()?;
    assert_eq!(root.bytes().len(), 45);

    // Scalars, an enum, a char enum, a set, a fixed numeric array, a char array.
    assert_eq!(
        root.value(f(car, 1))?,
        Some(Value::UInt(CAR_SERIAL_NUMBER)),
        "serialNumber"
    );
    assert_eq!(
        root.value(f(car, 2))?,
        Some(Value::UInt(u64::from(CAR_MODEL_YEAR))),
        "modelYear"
    );
    assert_eq!(
        root.value(f(car, 3))?,
        Some(Value::UInt(u64::from(CAR_AVAILABLE))),
        "available (BooleanType::T)"
    );
    assert_eq!(
        root.value(f(car, 4))?,
        Some(Value::Char(CAR_CODE)),
        "code (Model::C)"
    );
    let some_numbers = root.value(f(car, 5))?.expect("someNumbers present");
    let Value::Array(bytes) = some_numbers else {
        panic!("someNumbers is a fixed array, got {some_numbers:?}");
    };
    assert_eq!(
        u32_array_le(bytes),
        CAR_SOME_NUMBERS.to_vec(),
        "someNumbers"
    );
    assert_eq!(
        root.value(f(car, 6))?,
        Some(Value::Array(CAR_VEHICLE_CODE)),
        "vehicleCode"
    );
    assert_eq!(
        root.value(f(car, 7))?,
        Some(Value::UInt(u64::from(CAR_EXTRAS))),
        "extras (OptionalExtras set)"
    );

    // discountedModel: presence="constant" valueRef="Model.C" — zero wire
    // bytes, read straight from the table.
    assert_eq!(
        root.value(f(car, 8))?,
        Some(Value::Char(b'C')),
        "discountedModel = Model.C"
    );

    // engine: a composite with two literal constants (maxRpm, fuel) and a
    // one-level <ref> chain (Percentage, BooleanType, Booster).
    let engine = root.field(f(car, 9))?.expect("engine present");
    assert_eq!(
        engine.member("capacity")?,
        Some(Value::UInt(u64::from(CAR_ENGINE_CAPACITY)))
    );
    assert_eq!(
        engine.member("numCylinders")?,
        Some(Value::UInt(u64::from(CAR_ENGINE_NUM_CYLINDERS)))
    );
    assert_eq!(
        engine.member("maxRpm")?,
        Some(Value::UInt(9000)),
        "engine.maxRpm: constant, no wire bytes"
    );
    assert_eq!(
        engine.member("manufacturerCode")?,
        Some(Value::Array(CAR_ENGINE_MANUFACTURER_CODE))
    );
    assert_eq!(
        engine.member("fuel")?,
        Some(Value::Array(b"Petrol")),
        "engine.fuel: constant, no wire bytes"
    );
    assert_eq!(
        engine.member("efficiency")?,
        Some(Value::Int(i64::from(CAR_ENGINE_EFFICIENCY))),
        "engine.efficiency (ref Percentage)"
    );
    assert_eq!(
        engine.member("boosterEnabled")?,
        Some(Value::UInt(u64::from(CAR_ENGINE_BOOSTER_ENABLED))),
        "engine.boosterEnabled (ref BooleanType)"
    );
    assert_eq!(
        engine.member("booster.BoostType")?,
        Some(Value::Char(CAR_ENGINE_BOOSTER_TYPE)),
        "engine.booster.BoostType (ref Booster, one level)"
    );
    assert_eq!(
        engine.member("booster.horsePower")?,
        Some(Value::UInt(u64::from(CAR_ENGINE_BOOSTER_HORSEPOWER)))
    );

    // fuelFigures: a flat group with a per-entry varData.
    assert_eq!(car.groups.len(), 2, "fuelFigures, performanceFigures");
    let fuel_figures = &car.groups[0];
    assert_eq!(fuel_figures.name, "fuelFigures");
    let mut fuel = v.tail::<Baseline>()?.group(fuel_figures)?;
    assert_eq!(fuel.count(), 2);

    let e0 = fuel.next_entry()?.expect("fuelFigures entry 0");
    assert_eq!(
        e0.block().value(f_group(fuel_figures, 11))?,
        Some(Value::UInt(u64::from(CAR_FUEL_SPEED_0))),
        "speed[0]"
    );
    assert_eq!(
        e0.block().value(f_group(fuel_figures, 12))?,
        Some(Value::Float(f64::from(CAR_FUEL_MPG_0))),
        "mpg[0]"
    );
    let (note0, after0) = e0.tail().var_data(&fuel_figures.var_data[0])?;
    assert_eq!(note0, Some(CAR_FUEL_NOTE_0), "usageDescription[0]");

    let e1 = fuel.next_entry()?.expect("fuelFigures entry 1");
    assert_eq!(
        e1.block().value(f_group(fuel_figures, 11))?,
        Some(Value::UInt(u64::from(CAR_FUEL_SPEED_1))),
        "speed[1]"
    );
    assert_eq!(
        e1.block().value(f_group(fuel_figures, 12))?,
        Some(Value::Float(f64::from(CAR_FUEL_MPG_1))),
        "mpg[1]"
    );
    let (note1, after1) = e1.tail().var_data(&fuel_figures.var_data[0])?;
    assert_eq!(
        note1,
        Some(CAR_FUEL_NOTE_1),
        "usageDescription[1]: present and empty, not null"
    );
    assert_eq!(fuel.next_entry()?, None, "only two fuelFigures entries");
    let after_fuel = fuel.finish()?;
    assert!(after0.position() < after1.position());
    assert_eq!(
        after_fuel.position(),
        after1.position(),
        "finish() lands where the last entry's tail did"
    );

    // performanceFigures: a nested group, second entry's nested group empty.
    let performance_figures = &car.groups[1];
    assert_eq!(performance_figures.name, "performanceFigures");
    let mut perf = after_fuel.group(performance_figures)?;
    assert_eq!(perf.count(), 2);

    let p0 = perf.next_entry()?.expect("performanceFigures entry 0");
    assert_eq!(
        p0.block().value(f_group(performance_figures, 14))?,
        Some(Value::UInt(u64::from(CAR_OCTANE_0))),
        "octaneRating[0]"
    );
    let accel_layout = &performance_figures.groups[0];
    assert_eq!(accel_layout.name, "acceleration");
    let mut accel0 = p0.tail().group(accel_layout)?;
    assert_eq!(accel0.count(), 2);
    let a00 = accel0.next_entry()?.expect("acceleration[0][0]");
    assert_eq!(
        a00.block().value(f_group(accel_layout, 16))?,
        Some(Value::UInt(u64::from(CAR_ACCEL_0_MPH_0)))
    );
    assert_eq!(
        a00.block().value(f_group(accel_layout, 17))?,
        Some(Value::Float(f64::from(CAR_ACCEL_0_SECONDS_0)))
    );
    let a01 = accel0.next_entry()?.expect("acceleration[0][1]");
    assert_eq!(
        a01.block().value(f_group(accel_layout, 16))?,
        Some(Value::UInt(u64::from(CAR_ACCEL_0_MPH_1)))
    );
    assert_eq!(
        a01.block().value(f_group(accel_layout, 17))?,
        Some(Value::Float(f64::from(CAR_ACCEL_0_SECONDS_1)))
    );
    // Explicit, though `perf`'s own cursor re-derives this independently
    // when it walks p0's tail on the next `next_entry()`/`finish()` (proof:
    // `entries_stay_aligned_when_the_caller_ignores_their_tails` in
    // `crates/sbe/src/tests.rs`) — this just proves nothing here is
    // truncated.
    accel0.finish()?;

    let p1 = perf.next_entry()?.expect("performanceFigures entry 1");
    assert_eq!(
        p1.block().value(f_group(performance_figures, 14))?,
        Some(Value::UInt(u64::from(CAR_OCTANE_1))),
        "octaneRating[1]"
    );
    let mut accel1 = p1.tail().group(accel_layout)?;
    assert_eq!(accel1.count(), 0, "second entry's acceleration is empty");
    assert_eq!(accel1.next_entry()?, None);
    let after_perf = perf.finish()?;

    // Root varData, in schema order.
    assert_eq!(car.var_data.len(), 3, "manufacturer, model, activationCode");
    let (manufacturer, c) = after_perf.var_data(&car.var_data[0])?;
    assert_eq!(manufacturer, Some(CAR_MANUFACTURER));
    let (model, c) = c.var_data(&car.var_data[1])?;
    assert_eq!(model, Some(CAR_MODEL));
    let (activation_code, c) = c.var_data(&car.var_data[2])?;
    assert_eq!(activation_code, Some(CAR_ACTIVATION_CODE));
    assert_eq!(
        c.position(),
        msg.len(),
        "varData ends where the message does"
    );

    Ok(())
}

fn f_group(g: &'static fixbolt_sbe::GroupLayout, id: u16) -> &'static fixbolt_sbe::FieldLayout {
    g.field(id)
        .unwrap_or_else(|| panic!("no field id {id} in group {}", g.name))
}
