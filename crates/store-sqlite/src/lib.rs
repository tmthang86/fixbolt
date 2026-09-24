//! **A SQLite message store for fixbolt**: the `Async` journal with a
//! database for a file, one database per session.
//!
//! [`SqliteJournal`] implements the session's `Journal` exactly as the
//! engine's `FileJournal` does under `Durability::Async`, at the same cost to
//! the engine thread — a `MemJournal` answers every `get`, and each record is
//! pushed onto a ring — while a writer thread of its own commits the records
//! to SQLite in batches (WAL, `synchronous = NORMAL` by default). **There is
//! no mode in which `put` waits for the disk**; a deployment that needs that
//! keeps `FileJournal` with `Durability::Fsync`.
//!
//! What it shares with the engine, it shares through the engine's own handles
//! (ADR-0181): its writer retires through a `WriterTicket`, so `serve*`'s
//! `wait_for_retired_writers` waits for it after serving; it lets go through
//! a `Releaser`, so a `Recovery` parks a quick reconnect on
//! [`SqliteJournal::released`]; it idles by `ring::Idle`.
//!
//! # Feature
//!
//! `sqlite`, on by default, gates every module and the `rusqlite` dependency
//! (SQLite compiled in, `bundled`). Without it this crate is empty and
//! compiles no C (ADR-0182 decision 1).
//!
//! Decisions: ADR-0180 (the store), ADR-0181 (the engine's handles), ADR-0182
//! (the feature and the release). The file map is
//! `docs/internals/store-sqlite.md`.
#![forbid(unsafe_code)]
#![cfg_attr(docsrs, feature(doc_cfg))]

// Non-negotiable 6: the feature gates the `mod` declarations themselves.
#[cfg(feature = "sqlite")]
mod journal;
#[cfg(feature = "sqlite")]
mod schema;
#[cfg(feature = "sqlite")]
mod writer;

#[cfg(feature = "sqlite")]
pub use journal::{Progress, SqliteJournal, SqliteOptions, SqliteStore, Synchronous};
