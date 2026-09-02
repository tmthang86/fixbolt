//! fixbolt — a FIX 4.4 engine you embed, from the application's side.
//!
//! `DESIGN.md` §3 L4 and §7 step 8. This crate adds **no capability**: every
//! byte still goes through [`fixbolt_engine`] and [`fixbolt_session`]. What it
//! adds is a place to stand.
//!
//! # The two things it does
//!
//! **One crate to depend on.** `serve` lives in `fixbolt_engine`, `Config` in
//! `fixbolt_session`, `Table` and `Limits` in `fixbolt_engine::presession`,
//! `Settings` in `fixbolt_engine::settings`, `Observer` and `Admin` in
//! `fixbolt_engine::observe`. Five paths and two manifest entries for one job.
//! They are all re-exported here, and **only** the ones an application needs:
//! `Engine`, `Dispatch`, `Transport`, `wait`, `shard`, `affinity`, `frame` and
//! `ring` are deliberately absent. Reaching for one of those means naming
//! `fixbolt-engine` in your own manifest, and that extra line is the pause it
//! is there to cause.
//!
//! **A handler that does not have to know the session's job.** [`Handler`]
//! receives a message already parsed and answers through [`Reply`], which
//! writes `8`, `9`, `10`, `34`, `49`, `52` and `56` itself and sorts every
//! field the handler names from the generated tables.
//!
//! ```no_run
//! use fixbolt::{Answer, Handler, Incoming, Limits, Reply, Settings};
//!
//! struct Desk;
//!
//! impl Handler for Desk {
//!     fn on_message(&mut self, msg: &Incoming<'_>, reply: Reply<'_>) -> Answer {
//!         if msg.msg_type() != b"D" {
//!             return reply.silent();
//!         }
//!         reply
//!             .message(b"8")
//!             .field(37, b"EXEC-1")
//!             .field(150, b"0")
//!             .field(39, b"0")
//!             .send()
//!     }
//! }
//!
//! # // `serve` is `standard` only, so the example that calls it is too — the
//! # // same `#[cfg]` the re-export carries, applied to the doctest.
//! # #[cfg(all(feature = "standard", unix))]
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let table = Settings::load("acceptor.cfg")?.into_table();
//! fixbolt::serve(
//!     "0.0.0.0:9876",
//!     table,
//!     fixbolt::app(Desk),
//!     64,
//!     Limits::new(64, 30_000)?,
//! )?;
//! # Ok(())
//! # }
//! # #[cfg(not(all(feature = "standard", unix)))]
//! # fn main() {}
//! ```
//!
//! # What it costs, and the door that stays open
//!
//! [`App`] parses the message a second time — the session parsed it already and
//! does not hand the index out — and builds one template per reply.
//! [ADR-0041](../../../docs/decisions/ADR-0041-the-library-layer-buys-an-api-with-a-parse.md)
//! is that decision, with the measurement. If you want neither, implement
//! [`fixbolt_session::Application`] yourself and hand *that* to [`serve`]: the
//! raw seam is not taken away, and `crates/conformance/src/echo.rs` is a worked
//! example of using it.
//!
//! # The rule this crate cannot enforce
//!
//! [`Handler::on_message`] runs **on the engine thread**, inline
//! ([ADR-0002](../../../docs/decisions/ADR-0002-engine-library-split.md)). A
//! handler that blocks stops heartbeats, sequence numbers and every other
//! session on that thread. `docs/GUIDE.md` §2 is the long version.

mod app;
mod reply;

pub use app::{App, Handler, Incoming, app};
pub use reply::{Answer, Message, Reply, ReplyError};

// ---------------------------------------------------------------------------
// The facade. Re-exports only — nothing below this line is defined here.
// ---------------------------------------------------------------------------

/// Reading a message the handler was given.
pub use fixbolt_codec::{
    GroupData, GroupEntryData, GroupIter, MessageView, as_char, as_i64, as_u32,
};

/// Who this acceptor serves, when, and under what numbers.
pub use fixbolt_session::{
    Application, Config, Link, Role,
    schedule::{Schedule, Weekday, Weekdays},
};
pub use fixbolt_session::{MAX_BEGIN_STRING_LEN, MAX_COMP_ID_LEN};

/// Starting, stopping, and why a connection ended.
pub use fixbolt_engine::{ServeError, Shutdown, serve_hft};

/// `serve` blocks when idle and is the default mode, so it is the one an
/// example reaches for — and it exists only under `standard`.
///
/// **The `#[cfg]` is on the item, not only in `Cargo.toml`.** Non-negotiable 6:
/// a feature that gates a manifest entry and nothing in the source is a claim
/// the compiler cannot check. `unix` rides along because `engine` has no poller
/// anywhere else, so on such a target this name does not exist rather than
/// failing at startup.
#[cfg(all(feature = "standard", unix))]
pub use fixbolt_engine::{serve, serve_with_recovery};

/// `hft` has no `standard` to depend on, so its recovery entry point is always
/// present.
pub use fixbolt_engine::serve_hft_with_recovery;

/// Which counterparty a connection is, decided before a session exists.
pub use fixbolt_engine::presession::{Entry, Identity, LimitError, Limits, Registry, Table};

/// Who this acceptor serves, out of a file rather than out of a rebuild.
pub use fixbolt_engine::settings::{Problem, Settings, SettingsError};

/// What an operator can see and change while the engine runs.
pub use fixbolt_engine::observe::{
    Admin, Command, Event, EventKind, Observer, SessionSnapshot, Snapshot,
};

/// What a session left behind, and how it is asked for.
pub use fixbolt_engine::recovery::{FromFn, NoRecovery, Recovery, Resumed};

/// The journal: in memory, on disk, and read back from outside.
pub use fixbolt_engine::journal::{FileJournal, Reader, Record, Records, Store};

/// Why a connection ended.
pub use fixbolt_session::DropReason;
