//! The encode side of `fixbolt-sbe` (plan step C4) over the tables
//! `sbe-gen` generates — the byte-for-byte re-encode oracle moved here from
//! C3.
//!
//! 1. The three SBE 1.0 RC4 §7 dumps, decoded and written back through the
//!    native `MessageWriter` from the **decoded values** (element by element,
//!    constants and nulls included), equal the dump byte for byte.
//! 2. A Real Logic `Car` with two groups (one nested), a `varData` inside a
//!    group entry and three root `varData`, built by the writer, decodes back
//!    to the values written, and re-encodes to the same bytes.
//! 3. `NewOrderSingle` — the only flat message of the §7 schema (no groups,
//!    no `varData`) — written through `codec::Encoding::encode` with an
//!    `SbeTemplate` equals the order dump.
//!
//! The dumps are transcribed from `vendor/sbe-spec/v1-0-RC4/doc/07Examples.md`
//! exactly as `crates/sbe/src/tests.rs` transcribes them; the dumps win over
//! the spec's interpretation tables
//! (`docs/reference/the-sbe-rc4-example-dumps-disagree-with-their-own-tables.md`).
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use fixbolt_sbe::{
    Cursor, Encoding, EntryWriter, FieldId, FieldLayout, GroupLayout, MessageLayout, MessageWriter,
    Parsed, Sbe, SbeError, SbeTemplate, SbeView, Schema, Validation, Value, VarDataLayout,
};

mod rc4_generated {
    include!(concat!(env!("OUT_DIR"), "/examples_rc4.rs"));
}

mod car_generated {
    include!(concat!(env!("OUT_DIR"), "/car.rs"));
}

use car_generated::Baseline as Car;

/// The generated §7 tables under the `schemaId` the dumps carry. The XML says
/// `id="91"` and the generator reads it faithfully; every RC4 dump header says
/// 100 (the reference note above). A writer writes `S::ID`, so re-encoding the
/// dumps needs a schema whose `ID` is the dumps'. The layouts are the
/// generated ones, untouched.
struct Rc4AsDumped;
impl Schema for Rc4AsDumped {
    const ID: u16 = 100;
    const VERSION: u16 = rc4_generated::Examples::VERSION;
    const BYTE_ORDER: fixbolt_sbe::ByteOrder = rc4_generated::Examples::BYTE_ORDER;
    fn message(template_id: u16) -> Option<&'static MessageLayout> {
        rc4_generated::Examples::message(template_id)
    }
}

// --- the §7 dumps, transcribed ----------------------------------------------

/// Simple Open Framing Header in front of every §7 dump; not part of the SBE
/// message.
const SOFH_LEN: usize = 6;

/// "Wire format of an order message", 68 bytes.
const ORDER_DUMP: [u8; 68] = [
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

/// "Wire format of an execution message", 84 bytes.
const EXEC_DUMP: [u8; 84] = [
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

/// "Wire format of a business reject message", 64 bytes.
const REJECT_DUMP: [u8; 64] = [
    0x00, 0x00, 0x00, 0x40, 0xeb, 0x50, 0x09, 0x00, 0x61, 0x00, 0x64, 0x00, 0x00, 0x00, 0x4f,
    0x52, //
    0x44, 0x30, 0x30, 0x30, 0x30, 0x31, 0x06, 0x27, 0x00, 0x4e, 0x6f, 0x74, 0x20, 0x61, 0x75,
    0x74, //
    0x68, 0x6f, 0x72, 0x69, 0x7a, 0x65, 0x64, 0x20, 0x74, 0x6f, 0x20, 0x74, 0x72, 0x61, 0x64,
    0x65, //
    0x20, 0x74, 0x68, 0x61, 0x74, 0x20, 0x69, 0x6e, 0x73, 0x74, 0x72, 0x75, 0x6d, 0x65, 0x6e, 0x74,
];

/// The SBE message inside a dump, after checking the SOFH size field.
fn sbe(dump: &[u8]) -> &[u8] {
    let size = u32::from_be_bytes(dump[..4].try_into().unwrap());
    assert_eq!(size as usize, dump.len(), "SOFH size is the dump's length");
    assert_eq!(&dump[4..6], &[0xeb, 0x50], "SOFH: SBE 1.0 little-endian");
    &dump[SOFH_LEN..]
}

/// Byte-for-byte equality that names the first differing offset — in the
/// SBE message, header at 0 — so a red test points at the field.
fn assert_same_bytes(what: &str, got: &[u8], want: &[u8]) {
    if let Some(i) = got.iter().zip(want).position(|(g, w)| g != w) {
        panic!(
            "{what}: first difference at offset {i}: got {:02x}, want {:02x}\n got: {got:02x?}\nwant: {want:02x?}",
            got[i], want[i]
        );
    }
    assert_eq!(
        got.len(),
        want.len(),
        "{what}: same prefix, different length"
    );
}

// --- re-encode: decode with the reader, write every decoded value back ------

/// Copies every element of every field of `fields`, as the reader decodes it
/// (`None` for a null, the schema's value for a constant), through `put`.
fn copy_block(
    block: fixbolt_sbe::Block<'_>,
    fields: &'static [FieldLayout],
    mut put: impl FnMut(&FieldLayout, usize, Option<Value<'_>>) -> Result<(), SbeError>,
) -> Result<(), SbeError> {
    for f in fields {
        let r = block.field(f)?.expect("version 0 carries every field");
        for i in 0..f.elements.len() {
            put(f, i, r.element(i)?)?;
        }
    }
    Ok(())
}

/// Where the groups and `varData` of a block are written: the root or an
/// entry. The writer's two tail owners, behind one face for the recursion.
trait TailWriter {
    fn group_with(
        &mut self,
        g: &GroupLayout,
        f: &mut dyn FnMut(&mut fixbolt_sbe::GroupWriter<'_>) -> Result<(), SbeError>,
    ) -> Result<(), SbeError>;
    fn var(&mut self, v: &VarDataLayout, bytes: &[u8]) -> Result<(), SbeError>;
}

impl TailWriter for MessageWriter<'_> {
    fn group_with(
        &mut self,
        g: &GroupLayout,
        f: &mut dyn FnMut(&mut fixbolt_sbe::GroupWriter<'_>) -> Result<(), SbeError>,
    ) -> Result<(), SbeError> {
        self.group(g, f)
    }
    fn var(&mut self, v: &VarDataLayout, bytes: &[u8]) -> Result<(), SbeError> {
        self.var_data(v, bytes)
    }
}

impl TailWriter for EntryWriter<'_> {
    fn group_with(
        &mut self,
        g: &GroupLayout,
        f: &mut dyn FnMut(&mut fixbolt_sbe::GroupWriter<'_>) -> Result<(), SbeError>,
    ) -> Result<(), SbeError> {
        self.group(g, f)
    }
    fn var(&mut self, v: &VarDataLayout, bytes: &[u8]) -> Result<(), SbeError> {
        self.var_data(v, bytes)
    }
}

/// Reads the groups and `varData` at `cur` per the layout and writes each
/// through `w`, recursing into nested groups. Returns the cursor after them.
fn copy_tail<'a>(
    mut cur: Cursor<'a>,
    groups: &'static [GroupLayout],
    var_data: &'static [VarDataLayout],
    w: &mut dyn TailWriter,
) -> Result<Cursor<'a>, SbeError> {
    for g in groups {
        let mut grp = cur.group(g)?;
        let mut after = None;
        w.group_with(g, &mut |gw| {
            while let Some(e) = grp.next_entry()? {
                gw.entry(|ew| {
                    copy_block(e.block(), g.fields, |f, i, v| ew.put_element(f, i, v))?;
                    copy_tail(e.tail(), g.groups, g.var_data, ew)?;
                    Ok(())
                })?;
            }
            after = Some(grp.finish()?);
            Ok(())
        })?;
        cur = after.expect("the group closure ran");
    }
    for v in var_data {
        let (bytes, next) = cur.var_data(v)?;
        w.var(v, bytes.expect("version 0 carries every varData"))?;
        cur = next;
    }
    Ok(cur)
}

/// Decodes `msg` with schema `S` and writes it back into `out` from the
/// decoded values. Returns the length written.
fn reencode<S: Schema>(msg: &[u8], out: &mut [u8]) -> Result<usize, SbeError> {
    let view = SbeView::decode::<S>(msg)?;
    let layout = view.layout::<S>()?;
    let mut w = MessageWriter::new::<S>(out, view.template_id())?;
    copy_block(view.root::<S>()?, layout.fields, |f, i, v| {
        w.put_element(f, i, v)
    })?;
    let end = copy_tail(view.tail::<S>()?, layout.groups, layout.var_data, &mut w)?;
    assert_eq!(
        end.position(),
        msg.len(),
        "the reader walked the whole message"
    );
    w.finish()
}

#[test]
fn the_three_rc4_dumps_reencode_byte_for_byte_from_their_decoded_values() -> Result<(), SbeError> {
    for (name, dump) in [
        ("NewOrderSingle", &ORDER_DUMP[..]),
        ("ExecutionReport", &EXEC_DUMP[..]),
        ("BusinessMessageReject", &REJECT_DUMP[..]),
    ] {
        let msg = sbe(dump);
        let mut out = [0xAAu8; 128];
        let len = reencode::<Rc4AsDumped>(msg, &mut out)?;
        assert_same_bytes(&format!("{name} re-encode"), &out[..len], msg);
    }
    Ok(())
}

#[test]
fn an_optional_field_left_unwritten_goes_out_null() -> Result<(), SbeError> {
    // StopPx in the order dump is the int64 null. Write every field but it:
    // the initial block's null must reproduce the dump.
    let msg = sbe(&ORDER_DUMP);
    let view = SbeView::decode::<Rc4AsDumped>(msg)?;
    let layout = view.layout::<Rc4AsDumped>()?;
    let mut out = [0u8; 64];
    let mut w = MessageWriter::new::<Rc4AsDumped>(&mut out, 99)?;
    let root = view.root::<Rc4AsDumped>()?;
    for f in layout.fields.iter().filter(|f| f.name != "StopPx") {
        w.put_bytes(f, root.field(f)?.unwrap().bytes())?;
    }
    let len = w.finish()?;
    assert_same_bytes("NewOrderSingle without StopPx", &out[..len], msg);
    Ok(())
}

// --- Encoding::encode through a template: the flat message ------------------

#[test]
fn new_order_single_through_encoding_encode_is_the_order_dump() -> Result<(), SbeError> {
    type E = Sbe<Rc4AsDumped>;
    let layout = Rc4AsDumped::message(99).unwrap();
    assert!(
        layout.groups.is_empty() && layout.var_data.is_empty(),
        "NewOrderSingle is flat"
    );
    let t: <E as Encoding>::Template<0, 64> = SbeTemplate::new(99)?;
    let transact_time = 1_412_627_244_432_000_000u64.to_le_bytes();
    let qty = 7i32.to_le_bytes();
    let price = 99_610i64.to_le_bytes();
    // StopPx is not a slot: the template's skeleton already holds its null.
    let slots: [(FieldId, &[u8]); 8] = [
        (FieldId(11), b"ORD00001"),
        (FieldId(1), b"ACCT01\0\0"),
        (FieldId(55), b"GEM4\0\0\0\0"),
        (FieldId(54), b"1"),
        (FieldId(60), &transact_time),
        (FieldId(38), &qty),
        (FieldId(40), b"2"),
        (FieldId(44), &price),
    ];
    let mut out = [0u8; 80];
    // `P` is unused by SBE, so it cannot be inferred: the caller names it.
    let range = E::encode::<0, 64>(&t, &mut out, &slots)?;
    assert_eq!(range, 0..62);
    assert_same_bytes(
        "NewOrderSingle via Encoding::encode",
        &out[range],
        sbe(&ORDER_DUMP),
    );
    Ok(())
}

#[test]
fn the_order_dump_parses_through_encoding_and_reads_by_field_id() -> Result<(), SbeError> {
    type E = Sbe<Rc4AsDumped>;
    let msg = sbe(&ORDER_DUMP);
    let mut scratch = <E as Encoding>::Scratch::default();
    assert_eq!(
        E::parse(msg, &mut scratch, Validation::ALL)?,
        Parsed::Complete { consumed: 62 }
    );
    let view = E::view(&scratch, msg);
    assert_eq!(E::field(view, FieldId(11)), Some(&b"ORD00001"[..]));
    assert_eq!(
        E::field(view, FieldId(99)),
        Some(&[0, 0, 0, 0, 0, 0, 0, 0x80][..])
    );
    assert_eq!(E::session_fields(view), None);
    Ok(())
}

// --- Car: nested groups and varData, built by the writer --------------------

fn car() -> &'static MessageLayout {
    Car::message(1).unwrap()
}
fn fld(fields: &'static [FieldLayout], name: &str) -> &'static FieldLayout {
    fields.iter().find(|f| f.name == name).unwrap()
}
fn member(f: &FieldLayout, name: &str) -> usize {
    f.elements.iter().position(|e| e.name == name).unwrap()
}
fn grp(groups: &'static [GroupLayout], name: &str) -> &'static GroupLayout {
    groups.iter().find(|g| g.name == name).unwrap()
}
fn var(v: &'static [VarDataLayout], name: &str) -> &'static VarDataLayout {
    v.iter().find(|d| d.name == name).unwrap()
}

const FUEL: [(u64, f32, &str); 3] = [
    (30, 35.9, "Urban Cycle"),
    (55, 49.0, "Combined Cycle"),
    (75, 40.0, "Highway Cycle"),
];
const PERF: [(u64, [(u64, f32); 3]); 2] = [
    (95, [(30, 4.0), (60, 7.5), (100, 12.2)]),
    (99, [(30, 3.8), (60, 7.1), (100, 11.8)]),
];
const SOME_NUMBERS: [u32; 4] = [1, 2, 3, 4];

fn build_car(out: &mut [u8]) -> Result<usize, SbeError> {
    let m = car();
    let mut w = MessageWriter::new::<Car>(out, 1)?;
    w.put(fld(m.fields, "serialNumber"), Value::UInt(1234))?;
    w.put(fld(m.fields, "modelYear"), Value::UInt(2013))?;
    w.put(fld(m.fields, "available"), Value::UInt(1))?;
    w.put(fld(m.fields, "code"), Value::Char(b'A'))?;
    let mut numbers = [0u8; 16];
    for (i, n) in SOME_NUMBERS.iter().enumerate() {
        numbers[i * 4..i * 4 + 4].copy_from_slice(&n.to_le_bytes());
    }
    w.put(fld(m.fields, "someNumbers"), Value::Array(&numbers))?;
    w.put(fld(m.fields, "vehicleCode"), Value::Array(b"abcdef"))?;
    w.put(fld(m.fields, "extras"), Value::UInt(0b101))?;
    let engine = fld(m.fields, "engine");
    for (name, v) in [
        ("capacity", Value::UInt(2000)),
        ("numCylinders", Value::UInt(4)),
        ("manufacturerCode", Value::Array(b"123")),
        ("efficiency", Value::Int(35)),
        ("boosterEnabled", Value::UInt(1)),
        ("booster.BoostType", Value::Char(b'N')),
        ("booster.horsePower", Value::UInt(200)),
    ] {
        w.put_element(engine, member(engine, name), Some(v))?;
    }
    let fuel = grp(m.groups, "fuelFigures");
    w.group(fuel, |g| {
        for (speed, mpg, usage) in FUEL {
            g.entry(|e| {
                e.put(fld(fuel.fields, "speed"), Value::UInt(speed))?;
                e.put(fld(fuel.fields, "mpg"), Value::Float(f64::from(mpg)))?;
                e.var_data(var(fuel.var_data, "usageDescription"), usage.as_bytes())
            })?;
        }
        Ok(())
    })?;
    let perf = grp(m.groups, "performanceFigures");
    let accel = grp(perf.groups, "acceleration");
    w.group(perf, |g| {
        for (octane, rows) in PERF {
            g.entry(|e| {
                e.put(fld(perf.fields, "octaneRating"), Value::UInt(octane))?;
                e.group(accel, |a| {
                    for (mph, secs) in rows {
                        a.entry(|x| {
                            x.put(fld(accel.fields, "mph"), Value::UInt(mph))?;
                            x.put(fld(accel.fields, "seconds"), Value::Float(f64::from(secs)))
                        })?;
                    }
                    Ok(())
                })
            })?;
        }
        Ok(())
    })?;
    w.var_data(var(m.var_data, "manufacturer"), b"Honda")?;
    w.var_data(var(m.var_data, "model"), b"Civic VTi")?;
    w.var_data(var(m.var_data, "activationCode"), b"abcdef")?;
    w.finish()
}

#[test]
fn a_car_built_by_the_writer_decodes_back_to_what_was_written() -> Result<(), SbeError> {
    let mut out = [0u8; 512];
    let len = build_car(&mut out)?;
    // 8 header + 45 root; fuelFigures 4 + 3 × (6 + 4) + 11 + 14 + 13;
    // performanceFigures 4 + 2 × (1 + 4 + 3 × 6); three varData 4 + 5, 4 + 9, 4 + 6.
    assert_eq!(len, 8 + 45 + (4 + 30 + 38) + (4 + 2 * 23) + (9 + 13 + 10));
    let msg = &out[..len];

    // Parse through the trait measures exactly the message.
    let mut scratch = Default::default();
    assert_eq!(
        Sbe::<Car>::parse(msg, &mut scratch, Validation::ALL)?,
        Parsed::Complete { consumed: len }
    );

    let m = car();
    let view = SbeView::decode::<Car>(msg)?;
    let root = view.root::<Car>()?;
    assert_eq!(
        root.value(fld(m.fields, "serialNumber"))?,
        Some(Value::UInt(1234))
    );
    assert_eq!(
        root.value(fld(m.fields, "modelYear"))?,
        Some(Value::UInt(2013))
    );
    assert_eq!(root.value(fld(m.fields, "code"))?, Some(Value::Char(b'A')));
    assert_eq!(
        root.value(fld(m.fields, "vehicleCode"))?,
        Some(Value::Array(b"abcdef"))
    );
    let engine = root.field(fld(m.fields, "engine"))?.unwrap();
    assert_eq!(engine.member("capacity")?, Some(Value::UInt(2000)));
    assert_eq!(
        engine.member("maxRpm")?,
        Some(Value::UInt(9000)),
        "constant"
    );
    assert_eq!(
        engine.member("manufacturerCode")?,
        Some(Value::Array(b"123"))
    );
    assert_eq!(engine.member("booster.horsePower")?, Some(Value::UInt(200)));

    let fuel = grp(m.groups, "fuelFigures");
    let mut g = view.tail::<Car>()?.group(fuel)?;
    assert_eq!(g.count(), 3);
    for (speed, mpg, usage) in FUEL {
        let e = g.next_entry()?.unwrap();
        assert_eq!(
            e.block().value(fld(fuel.fields, "speed"))?,
            Some(Value::UInt(speed))
        );
        assert_eq!(
            e.block().value(fld(fuel.fields, "mpg"))?,
            Some(Value::Float(f64::from(mpg)))
        );
        let (text, _) = e.tail().var_data(var(fuel.var_data, "usageDescription"))?;
        assert_eq!(text, Some(usage.as_bytes()));
    }
    let perf = grp(m.groups, "performanceFigures");
    let accel = grp(perf.groups, "acceleration");
    let mut p = g.finish()?.group(perf)?;
    assert_eq!(p.count(), 2);
    for (octane, rows) in PERF {
        let e = p.next_entry()?.unwrap();
        assert_eq!(
            e.block().value(fld(perf.fields, "octaneRating"))?,
            Some(Value::UInt(octane))
        );
        let mut a = e.tail().group(accel)?;
        assert_eq!(a.count(), 3);
        for (mph, secs) in rows {
            let x = a.next_entry()?.unwrap();
            assert_eq!(
                x.block().value(fld(accel.fields, "mph"))?,
                Some(Value::UInt(mph))
            );
            assert_eq!(
                x.block().value(fld(accel.fields, "seconds"))?,
                Some(Value::Float(f64::from(secs)))
            );
        }
    }
    let cur = p.finish()?;
    let (manufacturer, cur) = cur.var_data(var(m.var_data, "manufacturer"))?;
    let (model, cur) = cur.var_data(var(m.var_data, "model"))?;
    let (code, cur) = cur.var_data(var(m.var_data, "activationCode"))?;
    assert_eq!(manufacturer, Some(&b"Honda"[..]));
    assert_eq!(model, Some(&b"Civic VTi"[..]));
    assert_eq!(code, Some(&b"abcdef"[..]));
    assert_eq!(cur.position(), len);

    // And the decoded values write back to the same bytes.
    let mut again = [0u8; 512];
    let len2 = reencode::<Car>(msg, &mut again)?;
    assert_same_bytes("Car re-encode", &again[..len2], msg);
    Ok(())
}

#[test]
fn groups_and_var_data_not_written_go_out_empty_and_the_message_still_walks() -> Result<(), SbeError>
{
    // Only the last varData is written: both groups (count 0) and the two
    // varData before it (length 0) must be closed on the way.
    let m = car();
    let mut out = [0u8; 128];
    let mut w = MessageWriter::new::<Car>(&mut out, 1)?;
    w.var_data(var(m.var_data, "activationCode"), b"xyz")?;
    let len = w.finish()?;
    assert_eq!(len, 8 + 45 + 4 + 4 + 4 + 4 + (4 + 3));
    let mut scratch = Default::default();
    assert_eq!(
        Sbe::<Car>::parse(&out[..len], &mut scratch, Validation::ALL)?,
        Parsed::Complete { consumed: len }
    );
    // Writing a group after a varData is refused, not silently misplaced.
    let mut w = MessageWriter::new::<Car>(&mut out, 1)?;
    w.var_data(var(m.var_data, "model"), b"m")?;
    assert_eq!(
        w.group(grp(m.groups, "fuelFigures"), |_| Ok(())),
        Err(SbeError::OutOfOrder)
    );
    assert_eq!(
        w.var_data(var(m.var_data, "manufacturer"), b"x"),
        Err(SbeError::OutOfOrder)
    );
    Ok(())
}
