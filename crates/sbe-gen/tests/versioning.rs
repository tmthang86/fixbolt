//! C3(3): schema versioning (ADR-0081 decision 4) and truncation, both
//! through the **generated** tables.
//!
//! (a) an element whose `sinceVersion` is newer than the header's version
//!     reads as absent — **stated gap**: neither fixture schema this step
//!     has (RC4's `Examples.xml`, Real Logic's `Car`) declares
//!     `sinceVersion` anywhere (checked below, and by hand against both XML
//!     files), so this property cannot be exercised *through a generated
//!     table* in this step. `crates/sbe/src/tests.rs`'s
//!     `what_a_newer_version_added_reads_as_absent_from_an_older_message`
//!     already proves it against `fixbolt_sbe` directly, with a
//!     hand-written schema built for exactly this purpose. Building a
//!     second hand-written schema here, fed through nothing the generator
//!     produced, would not close the gap — it would just be that same test
//!     again under a different file name.
//! (b) an unknown `templateId`: `S::message` returns `None`, the root block
//!     is still delimited by the wire's own `blockLength`, and nothing past
//!     it is promised (`docs/reference/the-sbe-rc4-example-dumps-disagree-
//!     with-their-own-tables.md`: blockLength-only skipping holds for flat
//!     messages; past the root, framing must say where the message ends).
//! (c) every truncation (cut at every byte length) of the three RC4 dumps
//!     and the `Car` message is an `Err` **somewhere** on a full walk of
//!     root fields, groups (nested, depth first) and `varData` — never a
//!     panic, never a silent full success.
//!
//! Same lint relaxation as `tests/generated.rs` (an integration test is its
//! own crate root; a panic here is the test failing).
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use fixbolt_sbe::{ByteOrder, GroupLayout, HEADER_LEN, MessageLayout, SbeError, SbeView, Schema};

mod support;

mod rc4_generated {
    include!(concat!(env!("OUT_DIR"), "/examples_rc4.rs"));
}
mod car_generated {
    include!(concat!(env!("OUT_DIR"), "/car.rs"));
}

// --- (a) the sinceVersion gap, with a guard that keeps it honest -----------

fn group_all_versions_zero(g: &GroupLayout) -> bool {
    g.since_version == 0
        && g.fields.iter().all(|f| f.since_version == 0)
        && g.var_data.iter().all(|v| v.since_version == 0)
        && g.groups.iter().all(group_all_versions_zero)
}

fn message_all_versions_zero(m: &MessageLayout) -> bool {
    m.since_version == 0
        && m.fields.iter().all(|f| f.since_version == 0)
        && m.var_data.iter().all(|v| v.since_version == 0)
        && m.groups.iter().all(group_all_versions_zero)
}

/// Guards the stated gap above: fails the day either fixture schema gains a
/// `sinceVersion` anywhere, which is exactly when the gap note (and the
/// missing generated-table version of `what_a_newer_version_added_reads_
/// as_absent_from_an_older_message`) must be revisited.
#[test]
fn neither_fixture_schema_declares_a_since_version_anywhere() {
    use car_generated::Baseline;
    use rc4_generated::Examples;

    for id in [97u16, 98, 99] {
        let m = Examples::message(id).unwrap_or_else(|| panic!("known RC4 template {id}"));
        assert!(message_all_versions_zero(m), "RC4 template {id}");
    }
    let car = Baseline::message(1).expect("Car");
    assert!(message_all_versions_zero(car), "Car");
}

// --- (b) unknown templateId -------------------------------------------------

#[test]
fn an_unknown_template_is_delimited_by_block_length_alone() -> Result<(), SbeError> {
    use rc4_generated::Examples;

    // NewOrderSingle's bytes, under a template the schema does not have,
    // with the header's schemaId patched to match `Examples::ID` (91, the
    // fetched resource's own id — see `spec_examples.rs`) so this is
    // `UnknownTemplate`, not `WrongSchema`.
    let mut msg = support::sbe(&support::ORDER_DUMP).to_vec();
    msg[2] = 42; // templateId: unknown to Examples
    msg[3] = 0;
    msg[4] = 91; // schemaId: Examples::ID
    msg[5] = 0;
    let v = SbeView::decode::<Examples>(&msg)?;
    assert_eq!(Examples::message(42), None);
    assert_eq!(v.layout::<Examples>(), Err(SbeError::UnknownTemplate));
    // Still delimited by the wire's own blockLength — this is a flat
    // message (no groups, no varData), the case ADR-0081 decision 4's
    // "skipped by blockLength" promise actually covers in full.
    assert_eq!(v.root_end(ByteOrder::Little), Ok(HEADER_LEN + 54));
    Ok(())
}

#[test]
fn an_unknown_templates_tail_is_not_promised_once_it_has_a_group_or_var_data()
-> Result<(), SbeError> {
    use car_generated::Baseline;

    // Car's bytes, under a template the schema does not have.
    let mut msg = support::car_message();
    msg[2] = 250; // templateId: unknown to Baseline
    msg[3] = 0;
    let v = SbeView::decode::<Baseline>(&msg)?;
    assert_eq!(Baseline::message(250), None, "unknown template");
    // The root block alone is still delimited by blockLength...
    assert_eq!(v.root_end(ByteOrder::Little), Ok(HEADER_LEN + 45));
    // ...but Car has two groups and three varData after it, and without a
    // layout for template 250 nothing in `fixbolt_sbe` can say how far they
    // run — exactly the gap `docs/reference/the-sbe-rc4-example-dumps-
    // disagree-with-their-own-tables.md` names: blockLength-only skipping
    // holds for a flat message, not past it. A framed transport (SOFH) is
    // what would have to say where this message ends.
    Ok(())
}

// --- (c) truncation, everywhere, never a panic ------------------------------

fn check_truncations<S: Schema>(name: &str, msg: &[u8]) {
    for cut in 0..msg.len() {
        let short = msg.get(..cut).unwrap_or_default();
        assert!(
            support::walk_message::<S>(short).is_err(),
            "{name}: cut at {cut} should be Err, was Ok"
        );
    }
    assert!(
        support::walk_message::<S>(msg).is_ok(),
        "{name}: the full message should walk clean"
    );
}

#[test]
fn every_truncation_of_every_c3_message_is_an_error_never_a_panic() {
    use car_generated::Baseline;
    use rc4_generated::Examples;

    let order = support::sbe(&support::ORDER_DUMP);
    let exec = support::sbe(&support::EXEC_DUMP);
    let reject = support::sbe(&support::REJECT_DUMP);
    let car = support::car_message();

    check_truncations::<Examples>("NewOrderSingle", order);
    check_truncations::<Examples>("ExecutionReport", exec);
    check_truncations::<Examples>("BusinessMessageReject", reject);
    check_truncations::<Baseline>("Car", &car);
}
