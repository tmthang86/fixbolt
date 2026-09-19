//! C3(1): the three RC4 §7 hex dumps, decoded field by field through the
//! **generated** `examples_rc4.rs` tables (`fixbolt-sbe-gen`'s output for
//! `vendor/sbe-spec/v1-0-RC4/resources/Examples.xml`) — the read-side half
//! of ADR-0081 decision 3(a). Re-encoding is C4's job, not this one's.
//!
//! `crates/sbe/src/tests.rs` already decodes these same bytes through
//! **hand-written** tables (step C1); this file decodes them through the
//! generator's own output, so a bug in flattening/offset resolution shows up
//! here even when the hand table happens to agree.
//!
//! **The oracle is the dump's bytes, not RC4's own interpretation tables**
//! (`docs/reference/the-sbe-rc4-example-dumps-disagree-with-their-own-
//! tables.md`): every assertion below is on what the transcribed bytes say,
//! and a table disagreement is called out where it bites.
//!
//! Same lint relaxation as `tests/generated.rs`: an integration test is its
//! own crate root, so a panic here is the test failing, which is what a
//! test is for (CLAUDE.md §2 non-negotiable 7's own reasoning).
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use fixbolt_sbe::{SbeError, SbeView, Schema, Value};

mod support;
use support::{EXEC_DUMP, ORDER_DUMP, REJECT_DUMP, sbe};

mod rc4_generated {
    include!(concat!(env!("OUT_DIR"), "/examples_rc4.rs"));
}
use rc4_generated::Examples;

// The generated schema's own id, read faithfully from the fetched resource
// (`vendor/sbe-spec/v1-0-RC4/resources/Examples.xml`'s `id="91"`), is not
// the `schemaId` the three dumps carry on the wire (`100`, consistently —
// checked below). A third number, `id="100"`, is what `07Examples.md`'s own
// *prose* sample schema declares (never fetched, illustrative only). None of
// `root()`/`tail()` care which of the three is "right" — they key off byte
// order, not schema id — so decoding proceeds via `Examples::message`
// directly rather than `SbeView::layout`, which does check `schemaId` and
// would refuse every one of these dumps as `WrongSchema`.
#[test]
fn the_generated_schemas_own_id_disagrees_with_every_dumps_wire_schema_id() -> Result<(), SbeError>
{
    assert_eq!(Examples::ID, 91, "the fetched resource's own id=\"91\"");
    for dump in [&ORDER_DUMP[..], &EXEC_DUMP[..], &REJECT_DUMP[..]] {
        let v = SbeView::decode::<Examples>(sbe(dump))?;
        assert_eq!(v.schema_id(), 100, "every §7 dump's wire schemaId");
        assert_eq!(v.layout::<Examples>(), Err(SbeError::WrongSchema));
    }
    Ok(())
}

fn f(layout: &'static fixbolt_sbe::MessageLayout, id: u16) -> &'static fixbolt_sbe::FieldLayout {
    layout
        .field(id)
        .unwrap_or_else(|| panic!("no field id {id} in {}", layout.name))
}

// --- NewOrderSingle: flat, fixed length -------------------------------------

#[test]
fn new_order_single_decodes_field_by_field_through_the_generated_table() -> Result<(), SbeError> {
    let msg = sbe(&ORDER_DUMP);
    let v = SbeView::decode::<Examples>(msg)?;
    assert_eq!((v.template_id(), v.schema_id(), v.version()), (99, 100, 0));
    assert_eq!(v.block_length(fixbolt_sbe::ByteOrder::Little), Ok(54));

    let nos = Examples::message(99).expect("template 99 (NewOrderSingle)");
    assert_eq!(nos.name, "NewOrderSingle");
    let root = v.root::<Examples>()?;
    assert_eq!(root.bytes().len(), 54);

    assert_eq!(root.value(f(nos, 11))?, Some(Value::Array(b"ORD00001")));
    assert_eq!(root.value(f(nos, 1))?, Some(Value::Array(b"ACCT01\0\0")));
    assert_eq!(root.value(f(nos, 55))?, Some(Value::Array(b"GEM4\0\0\0\0")));
    assert_eq!(
        root.value(f(nos, 54))?,
        Some(Value::Char(b'1')),
        "Side = Buy"
    );
    // The dump, not RC4's table (docs/reference/the-sbe-rc4-example-dumps-
    // disagree-with-their-own-tables.md): 2014-10-06 20:27:24.432 UTC, not
    // the table's 2013-10-10 13:35:33.135.
    assert_eq!(
        root.value(f(nos, 60))?,
        Some(Value::UInt(1_412_627_244_432_000_000))
    );
    let qty = root.field(f(nos, 38))?.expect("OrderQty present");
    assert_eq!(qty.member("mantissa")?, Some(Value::Int(7)));
    assert_eq!(qty.member("exponent")?, Some(Value::Int(0)));
    assert_eq!(
        root.value(f(nos, 40))?,
        Some(Value::Char(b'2')),
        "OrdType = Limit"
    );
    let px = root.field(f(nos, 44))?.expect("Price present");
    assert_eq!(px.element(0)?, Some(Value::Int(99_610)), "99.610");
    assert_eq!(px.element(1)?, Some(Value::Int(-3)));
    // StopPx: the dump's mantissa bytes are `i64::MIN` little-endian, the
    // optional int64's null — not the table's `...08`.
    assert_eq!(root.value(f(nos, 99))?, None, "StopPx = null");
    let stop = root.field(f(nos, 99))?.expect("StopPx field present");
    assert_eq!(stop.bytes(), &[0, 0, 0, 0, 0, 0, 0, 0x80][..]);

    assert_eq!(
        v.tail::<Examples>()?.position(),
        msg.len(),
        "flat: nothing follows the root block"
    );
    Ok(())
}

// --- ExecutionReport: a repeating group -------------------------------------

fn f_group(g: &'static fixbolt_sbe::GroupLayout, id: u16) -> &'static fixbolt_sbe::FieldLayout {
    g.field(id)
        .unwrap_or_else(|| panic!("no field id {id} in group {}", g.name))
}

#[test]
fn execution_report_decodes_root_and_group_through_the_generated_table() -> Result<(), SbeError> {
    let msg = sbe(&EXEC_DUMP);
    let v = SbeView::decode::<Examples>(msg)?;
    assert_eq!((v.template_id(), v.schema_id(), v.version()), (98, 100, 0));
    let er = Examples::message(98).expect("template 98 (ExecutionReport)");
    let root = v.root::<Examples>()?;

    assert_eq!(root.value(f(er, 37))?, Some(Value::Array(b"O0000001")));
    assert_eq!(root.value(f(er, 17))?, Some(Value::Array(b"EXEC0000")));
    assert_eq!(
        root.value(f(er, 150))?,
        Some(Value::Char(b'F')),
        "ExecType = Trade"
    );
    assert_eq!(
        root.value(f(er, 39))?,
        Some(Value::Char(b'1')),
        "OrdStatus = PartialFilled"
    );
    assert_eq!(root.value(f(er, 55))?, Some(Value::Array(b"GEM4\0\0\0\0")));
    let mmy = root.field(f(er, 200))?.expect("MaturityMonthYear present");
    assert_eq!(mmy.member("year")?, Some(Value::UInt(2014)));
    assert_eq!(mmy.member("month")?, Some(Value::UInt(6)));
    // Declared required in the example schema, so 0xff is a value, not null.
    assert_eq!(mmy.member("day")?, Some(Value::UInt(255)));
    assert_eq!(mmy.member("week")?, Some(Value::UInt(255)));
    assert_eq!(root.value(f(er, 54))?, Some(Value::Char(b'1')));
    assert_eq!(root.value(f(er, 151))?, Some(Value::Int(1)));
    assert_eq!(root.value(f(er, 14))?, Some(Value::Int(6)));
    // The dump (`dd 3f` = 16349, 2014-10-06), not the table's `753e` /
    // 2013-10-11 (reference doc, same disagreement class).
    assert_eq!(root.value(f(er, 75))?, Some(Value::UInt(16_349)));

    assert_eq!(er.groups.len(), 1, "FillsGrp");
    let fills_grp = &er.groups[0];
    assert_eq!(fills_grp.name, "FillsGrp");
    let mut g = v.tail::<Examples>()?.group(fills_grp)?;
    assert_eq!(g.count(), 2);
    let mut fills = [(0i64, 0i64); 2];
    let mut i = 0;
    while let Some(e) = g.next_entry()? {
        let px = e.block().value(f_group(fills_grp, 1364))?;
        let qty = e.block().value(f_group(fills_grp, 1365))?;
        if let (Some(Value::Int(px)), Some(Value::Int(qty)), Some(slot)) =
            (px, qty, fills.get_mut(i))
        {
            *slot = (px, qty);
        }
        i += 1;
    }
    let end = g.finish()?.position();
    assert_eq!(fills, [(99_610, 2), (99_620, 4)]);
    assert_eq!(end, msg.len(), "the group ends where the message ends");
    Ok(())
}

// --- BusinessMessageReject: varData -----------------------------------------

#[test]
fn business_reject_decodes_root_and_var_data_through_the_generated_table() -> Result<(), SbeError> {
    let msg = sbe(&REJECT_DUMP);
    let v = SbeView::decode::<Examples>(msg)?;
    // The dump's own header bytes: template 97, schema 100 — not the doc's
    // *printed* "Interpreted value" column for this row, which misreads its
    // own hex as template 100 / schema 0 (reference doc, "header
    // template / schema" row).
    assert_eq!((v.template_id(), v.schema_id(), v.version()), (97, 100, 0));
    let bmr = Examples::message(97).expect("template 97 (BusinessMessageReject)");
    let root = v.root::<Examples>()?;
    assert_eq!(root.value(f(bmr, 379))?, Some(Value::Array(b"ORD00001")));
    assert_eq!(
        root.value(f(bmr, 380))?,
        Some(Value::UInt(6)),
        "NotAuthorized"
    );

    assert_eq!(bmr.var_data.len(), 1, "Text");
    let (text, after) = v.tail::<Examples>()?.var_data(&bmr.var_data[0])?;
    assert_eq!(text, Some(&b"Not authorized to trade that instrument"[..]));
    assert_eq!(after.position(), msg.len());
    Ok(())
}
