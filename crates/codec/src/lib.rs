//! Parse and serialise FIX 4.4 in place at the I/O buffer.
//!
//! `no_std` on purpose. That alone proves nothing about allocation — the crate
//! could still pull in `alloc`, and a caller can allocate freely. What proves it
//! is `benches/alloc.rs` with a counting allocator. See the plan, "Bất biến bị
//! đụng tới", row 1.
#![no_std]

pub mod dict;

pub use dict::{Dictionary, NoDict};
