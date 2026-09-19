//! Decode SBE 1.0 messages in place, over layout tables generated from a
//! schema (ADR-0081).
//!
//! The runtime half of the SBE pair: `sbe-gen` turns a schema into the
//! `&'static` tables of [`schema`]; this crate reads any message through them.
//! One code path for every schema, no code per message.
//!
//! ```text
//! SbeView::decode::<S>(buf)      header, and the root block is all there
//!   .layout::<S>()               which MessageLayout (schema id, template id)
//!   .root::<S>()?.value(&FIELD)  a root field, by its &'static FieldLayout
//!   .tail::<S>()?                a Cursor after the root block
//!     .group(&G)?                a Group: next_entry() → Entry { block, tail }
//!     .finish()?                 the Cursor after the group
//!     .var_data(&V)?             (bytes, the Cursor after them)
//! ```
//!
//! **What holds the constraints.** `no_std` and no `alloc`: nothing here can
//! allocate, which is CLAUDE.md §2 rule 1 by construction; the counting
//! allocator bench that proves the hot path end to end is a later step of the
//! plan. No `unsafe`: `forbid` below. No panic: the workspace denies
//! `unwrap`/`expect`/`panic!` and `indexing_slicing`, so every read goes
//! through a bounds-checked `get` and a truncated message is an `Err`.
//!
//! Scope is ADR-0081 decision 5; the header is the recommended 8-byte one
//! ([`HEADER_LEN`]); versioning follows decision 4 — a field, group or
//! `varData` whose `sinceVersion` is newer than the header's version reads as
//! absent, and blocks are walked by the `blockLength` on the wire, not the one
//! in the schema.
#![no_std]
#![forbid(unsafe_code)]

pub mod error;
pub mod group;
pub mod header;
pub mod schema;
pub mod vardata;
pub mod view;
mod wire;

#[cfg(test)]
#[allow(
    clippy::indexing_slicing,
    reason = "test fixtures patch known offsets of fixed-size arrays; a panic there is a failing test"
)]
mod tests;

pub use error::SbeError;
pub use group::{Cursor, Entry, Group};
pub use header::{HEADER_LEN, MessageHeader};
pub use schema::{
    DimensionLayout, Element, FieldLayout, GroupLayout, LengthType, MessageLayout, Presence,
    Primitive, Schema, Value, VarDataLayout,
};
pub use view::{Block, FieldRef, SbeView};
pub use wire::ByteOrder;
