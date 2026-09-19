//! Shared plumbing for `spec_examples.rs`, `car_roundtrip.rs` and
//! `versioning.rs` (plan step C3): the RC4 §7 dump bytes, a hand-assembled
//! `Car` message, and a schema-agnostic "walk everything" helper used by
//! `versioning.rs`'s truncation sweep.
//!
//! Not itself a test binary: a file named `mod.rs` under `tests/` is never
//! auto-discovered by cargo as its own target (the same
//! `tests/common/mod.rs` idiom `crates/dict` and the wider ecosystem use),
//! and each of the three test files that wants it writes a plain
//! `mod support;` — resolved relative to `tests/`, where all three live.
//!
//! Every function here reads through `fixbolt_sbe`'s public API only, so it
//! carries no dependency on which generated schema (`examples_rc4.rs` or
//! `car.rs`) a caller decodes with.
//!
//! Each of the three test files compiles its own copy of this module (an
//! integration test is its own crate root, so `mod support;` is not shared
//! compilation) and each uses only part of it — `dead_code` would otherwise
//! fire per binary, since a `pub` item in a binary crate has no outside
//! consumer to be reachable for.
#![allow(dead_code)]

use fixbolt_sbe::{Cursor, GroupLayout, SbeError, SbeView, Schema};

// ============================================================================
// RC4 §7 dumps, transcribed from `vendor/sbe-spec/v1-0-RC4/doc/07Examples.md`
// (fetched by `scripts/fetch-sbe-assets.sh`, never committed) — the same
// bytes `crates/sbe/src/tests.rs` reads for step C1's hand-written tables.
// Citations: order dump `07Examples.md:134-142`, execution dump
// `07Examples.md:262-272`, reject dump `07Examples.md:344-350`.
// ============================================================================

/// Simple Open Framing Header in front of every §7 dump: 4-byte big-endian
/// message size, 2-byte encoding type `eb50`. Not part of the SBE message.
pub const SOFH_LEN: usize = 6;

/// "Wire format of an order message" (`07Examples.md:134-142`), 68 bytes.
pub const ORDER_DUMP: [u8; 68] = [
    0x00, 0x00, 0x00, 0x44, 0xeb, 0x50, 0x36, 0x00, 0x63, 0x00, 0x64, 0x00, 0x00, 0x00, 0x4f,
    0x52, //
    0x44, 0x30, 0x30, 0x30, 0x30, 0x31, 0x41, 0x43, 0x43, 0x54, 0x30, 0x31, 0x00, 0x00, 0x47,
    0x45, //
    0x4d, 0x34, 0x00, 0x00, 0x00, 0x00, 0x31, 0x00, 0x84, 0x68, 0x90, 0xfe, 0xa8, 0x9a, 0x13,
    0x07, //
    0x00, 0x00, 0x00, 0x32, 0x1a, 0x85, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, //
    0x00, 0x00, 0x00, 0x80,
];

/// "Wire format of an execution message" (`07Examples.md:262-272`), 84 bytes.
pub const EXEC_DUMP: [u8; 84] = [
    0x00, 0x00, 0x00, 0x54, 0xeb, 0x50, 0x2a, 0x00, 0x62, 0x00, 0x64, 0x00, 0x00, 0x00, 0x4f,
    0x30, //
    0x30, 0x30, 0x30, 0x30, 0x30, 0x31, 0x45, 0x58, 0x45, 0x43, 0x30, 0x30, 0x30, 0x30, 0x46,
    0x31, //
    0x47, 0x45, 0x4d, 0x34, 0x00, 0x00, 0x00, 0x00, 0xde, 0x07, 0x06, 0xff, 0xff, 0x31, 0x01,
    0x00, //
    0x00, 0x00, 0x06, 0x00, 0x00, 0x00, 0xdd, 0x3f, 0x0c, 0x00, 0x02, 0x00, 0x1a, 0x85, 0x01,
    0x00, //
    0x00, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x24, 0x85, 0x01, 0x00, 0x00, 0x00, 0x00,
    0x00, //
    0x04, 0x00, 0x00, 0x00,
];

/// "Wire format of a business reject message" (`07Examples.md:344-350`), 64 bytes.
pub const REJECT_DUMP: [u8; 64] = [
    0x00, 0x00, 0x00, 0x40, 0xeb, 0x50, 0x09, 0x00, 0x61, 0x00, 0x64, 0x00, 0x00, 0x00, 0x4f,
    0x52, //
    0x44, 0x30, 0x30, 0x30, 0x30, 0x31, 0x06, 0x27, 0x00, 0x4e, 0x6f, 0x74, 0x20, 0x61, 0x75,
    0x74, //
    0x68, 0x6f, 0x72, 0x69, 0x7a, 0x65, 0x64, 0x20, 0x74, 0x6f, 0x20, 0x74, 0x72, 0x61, 0x64,
    0x65, //
    0x20, 0x74, 0x68, 0x61, 0x74, 0x20, 0x69, 0x6e, 0x73, 0x74, 0x72, 0x75, 0x6d, 0x65, 0x6e, 0x74,
];

/// The SBE message inside a dump, after checking the SOFH size field is the
/// dump's length — the transcription is checked, not trusted (same guard as
/// `crates/sbe/src/tests.rs::sbe`).
pub fn sbe(dump: &[u8]) -> &[u8] {
    let size = dump.get(..4).map(|b| [b[0], b[1], b[2], b[3]]);
    assert_eq!(size.map(u32::from_be_bytes), u32::try_from(dump.len()).ok());
    assert_eq!(
        dump.get(4..6),
        Some(&[0xeb, 0x50][..]),
        "SOFH: SBE 1.0 little-endian"
    );
    dump.get(SOFH_LEN..).unwrap_or_default()
}

// ============================================================================
// A little-endian byte builder, test-local only — `fixbolt-sbe-gen` stays a
// generator and `fixbolt-sbe` stays zero-dependency; nothing here is part of
// either crate's own build (ADR-0081 decision 1).
// ============================================================================

#[derive(Default)]
pub struct W(Vec<u8>);

impl W {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn bytes(mut self, b: &[u8]) -> Self {
        self.0.extend_from_slice(b);
        self
    }

    #[must_use]
    pub fn u8(self, v: u8) -> Self {
        self.bytes(&v.to_le_bytes())
    }

    #[must_use]
    pub fn i8(self, v: i8) -> Self {
        self.bytes(&v.to_le_bytes())
    }

    #[must_use]
    pub fn u16(self, v: u16) -> Self {
        self.bytes(&v.to_le_bytes())
    }

    #[must_use]
    pub fn u32(self, v: u32) -> Self {
        self.bytes(&v.to_le_bytes())
    }

    #[must_use]
    pub fn u64(self, v: u64) -> Self {
        self.bytes(&v.to_le_bytes())
    }

    #[must_use]
    pub fn f32(self, v: f32) -> Self {
        self.bytes(&v.to_le_bytes())
    }

    #[must_use]
    pub fn into_vec(self) -> Vec<u8> {
        self.0
    }
}

// ============================================================================
// A hand-assembled `Car` message (`example-schema.xml` + `common-types.xml`,
// package="baseline" id="1"). Every value used to build it is named here so
// `car_roundtrip.rs` asserts against the same constants it was built from —
// this exercises every offset and field *kind* the generator emits, it does
// not re-derive the values independently (that independent check is
// `crates/sbe-gen/tests/generated.rs`'s hand-computed-offsets test, which
// this step does not duplicate).
//
// Field kinds covered: char arrays (`vehicleCode`, `engine.manufacturerCode`),
// enums (`available`, `code`, `engine.boosterEnabled`,
// `engine.booster.BoostType`), a set (`extras`), composites with one-level
// `<ref>` (`engine` → `Percentage`, `BooleanType`, `Booster`), a fixed
// numeric array (`someNumbers`), constants (`discountedModel`, `engine.
// maxRpm`, `engine.fuel`), a nested group (`performanceFigures` →
// `acceleration`, second entry's nested group empty), `fuelFigures` with a
// per-entry `varData`, and all three root `varData` (`manufacturer`,
// `model`, `activationCode`). Car has **no** `presence="optional"` element
// anywhere (checked by hand against both XML files) — optional-with-null is
// out of scope for this message; `spec_examples.rs`'s `StopPx`/`Price` and
// `crates/sbe/src/tests.rs` already cover that presence kind.
pub const CAR_SERIAL_NUMBER: u64 = 1_234_567_890_123;
pub const CAR_MODEL_YEAR: u16 = 2016;
pub const CAR_AVAILABLE: u8 = 1; // BooleanType::T
pub const CAR_CODE: u8 = b'C'; // Model::C
pub const CAR_SOME_NUMBERS: [u32; 4] = [10, 20, 30, 40];
pub const CAR_VEHICLE_CODE: &[u8; 6] = b"XY9988";
pub const CAR_EXTRAS: u8 = 0b101; // sunRoof + cruiseControl
pub const CAR_ENGINE_CAPACITY: u16 = 1200;
pub const CAR_ENGINE_NUM_CYLINDERS: u8 = 4;
pub const CAR_ENGINE_MANUFACTURER_CODE: &[u8; 3] = b"VW1";
pub const CAR_ENGINE_EFFICIENCY: i8 = 35;
pub const CAR_ENGINE_BOOSTER_ENABLED: u8 = 1; // BooleanType::T
pub const CAR_ENGINE_BOOSTER_TYPE: u8 = b'T'; // Booster.BoostType::TURBO
pub const CAR_ENGINE_BOOSTER_HORSEPOWER: u8 = 200;

pub const CAR_FUEL_SPEED_0: u16 = 100;
pub const CAR_FUEL_MPG_0: f32 = 25.5;
pub const CAR_FUEL_NOTE_0: &[u8] = b"hwy";
pub const CAR_FUEL_SPEED_1: u16 = 120;
pub const CAR_FUEL_MPG_1: f32 = 22.1;
pub const CAR_FUEL_NOTE_1: &[u8] = b""; // present, empty — not null (SBE makes no distinction)

pub const CAR_OCTANE_0: u8 = 95;
pub const CAR_ACCEL_0_MPH_0: u16 = 30;
pub const CAR_ACCEL_0_SECONDS_0: f32 = 3.5;
pub const CAR_ACCEL_0_MPH_1: u16 = 60;
pub const CAR_ACCEL_0_SECONDS_1: f32 = 6.2;
pub const CAR_OCTANE_1: u8 = 91; // second performanceFigures entry: empty acceleration group

pub const CAR_MANUFACTURER: &[u8] = b"Ford";
pub const CAR_MODEL: &[u8] = b"Mustang";
pub const CAR_ACTIVATION_CODE: &[u8] = b"ABC123XYZ";

/// Builds the message above: header (`blockLength=45, templateId=1,
/// schemaId=1, version=0`), the 45-byte root block, `fuelFigures` (2 entries,
/// each with its own `usageDescription`), `performanceFigures` (2 entries,
/// the first with a 2-entry nested `acceleration`, the second with an empty
/// one), then the three root `varData`.
#[must_use]
pub fn car_message() -> Vec<u8> {
    let root = W::new()
        .u64(CAR_SERIAL_NUMBER)
        .u16(CAR_MODEL_YEAR)
        .u8(CAR_AVAILABLE)
        .u8(CAR_CODE)
        .u32(CAR_SOME_NUMBERS[0])
        .u32(CAR_SOME_NUMBERS[1])
        .u32(CAR_SOME_NUMBERS[2])
        .u32(CAR_SOME_NUMBERS[3])
        .bytes(CAR_VEHICLE_CODE)
        .u8(CAR_EXTRAS)
        // discountedModel: presence="constant" — zero wire bytes.
        .u16(CAR_ENGINE_CAPACITY)
        .u8(CAR_ENGINE_NUM_CYLINDERS)
        // engine.maxRpm: presence="constant" (9000) — zero wire bytes.
        .bytes(CAR_ENGINE_MANUFACTURER_CODE)
        // engine.fuel: presence="constant" ("Petrol") — zero wire bytes.
        .i8(CAR_ENGINE_EFFICIENCY)
        .u8(CAR_ENGINE_BOOSTER_ENABLED)
        .u8(CAR_ENGINE_BOOSTER_TYPE)
        .u8(CAR_ENGINE_BOOSTER_HORSEPOWER);

    let fuel_entry = |speed: u16, mpg: f32, note: &[u8]| {
        W::new()
            .u16(speed)
            .f32(mpg)
            .u32(u32::try_from(note.len()).unwrap_or(0))
            .bytes(note)
            .into_vec()
    };
    let fuel_figures = W::new()
        .u16(6) // dimension blockLength: speed(2) + mpg(4)
        .u16(2) // dimension numInGroup
        .bytes(&fuel_entry(
            CAR_FUEL_SPEED_0,
            CAR_FUEL_MPG_0,
            CAR_FUEL_NOTE_0,
        ))
        .bytes(&fuel_entry(
            CAR_FUEL_SPEED_1,
            CAR_FUEL_MPG_1,
            CAR_FUEL_NOTE_1,
        ));

    let acceleration_0 = W::new()
        .u16(6) // dimension blockLength: mph(2) + seconds(4)
        .u16(2) // dimension numInGroup
        .u16(CAR_ACCEL_0_MPH_0)
        .f32(CAR_ACCEL_0_SECONDS_0)
        .u16(CAR_ACCEL_0_MPH_1)
        .f32(CAR_ACCEL_0_SECONDS_1);
    let acceleration_1_empty = W::new().u16(6).u16(0);

    let performance_figures = W::new()
        .u16(1) // dimension blockLength: octaneRating(1)
        .u16(2) // dimension numInGroup
        .u8(CAR_OCTANE_0)
        .bytes(&acceleration_0.into_vec())
        .u8(CAR_OCTANE_1)
        .bytes(&acceleration_1_empty.into_vec());

    let var_data = |bytes: &[u8]| {
        W::new()
            .u32(u32::try_from(bytes.len()).unwrap_or(0))
            .bytes(bytes)
            .into_vec()
    };

    W::new()
        .u16(45) // header blockLength
        .u16(1) // header templateId (Car)
        .u16(1) // header schemaId (Baseline)
        .u16(0) // header version
        .bytes(&root.into_vec())
        .bytes(&fuel_figures.into_vec())
        .bytes(&performance_figures.into_vec())
        .bytes(&var_data(CAR_MANUFACTURER))
        .bytes(&var_data(CAR_MODEL))
        .bytes(&var_data(CAR_ACTIVATION_CODE))
        .into_vec()
}

// ============================================================================
// A generic "walk everything, field by field" helper, used by `versioning.
// rs`'s truncation sweep. Schema-agnostic: it reads only `layout.fields`/
// `.groups`/`.var_data`, so it walks any message of any `Schema` the same
// way `crates/sbe/src/tests.rs`'s per-message walkers do by hand.
// ============================================================================

/// Decodes `msg` under `S` and reads every root field, every group entry
/// (depth first) and every `varData`, in schema order. `Ok(())` only when
/// nothing on the wire is short; any truncation anywhere is `Err`, never a
/// panic — the property `versioning.rs`'s truncation sweep asserts.
pub fn walk_message<S: Schema>(msg: &[u8]) -> Result<(), SbeError> {
    let v = SbeView::decode::<S>(msg)?;
    let layout = S::message(v.template_id()).ok_or(SbeError::UnknownTemplate)?;
    let root = v.root::<S>()?;
    for f in layout.fields {
        root.value(f)?;
    }
    let mut cursor = v.tail::<S>()?;
    for g in layout.groups {
        cursor = walk_group(cursor, g)?;
    }
    for vd in layout.var_data {
        let (_, next) = cursor.var_data(vd)?;
        cursor = next;
    }
    Ok(())
}

/// Walks one group's entries depth first: each entry's fixed fields, then
/// its nested groups, then its own `varData` — SBE 1.0 RC4 "Nested
/// repeating group wire format". Returns the cursor after the group
/// ([`Group::finish`]), which re-derives the same position independently of
/// this function's own walk (`entries_stay_aligned_when_the_caller_ignores_
/// their_tails` in `crates/sbe/src/tests.rs` proves the two never diverge).
fn walk_group<'a>(cursor: Cursor<'a>, g: &'static GroupLayout) -> Result<Cursor<'a>, SbeError> {
    let mut group = cursor.group(g)?;
    while let Some(entry) = group.next_entry()? {
        let block = entry.block();
        for f in g.fields {
            block.value(f)?;
        }
        let mut tail = entry.tail();
        for ng in g.groups {
            tail = walk_group(tail, ng)?;
        }
        for vd in g.var_data {
            let (_, next) = tail.var_data(vd)?;
            tail = next;
        }
    }
    group.finish()
}
