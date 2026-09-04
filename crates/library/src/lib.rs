#![doc = include_str!("../README.md")]
#![warn(missing_docs)]

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

/// The message log: every message this engine saw or sent, one line each.
///
/// [`NoLog`] is what an engine that wants none passes, and it compiles away
/// entirely. [`FileLog`] writes a text file — `docs/GUIDE.md` §6a says how to
/// read one, what `lost` means, and why rotation is the operator's job.
pub use fixbolt_engine::msglog::{Direction, FileLog, MaybeLog, MessageLog, NoLog, shard_path};

/// Why a connection ended.
pub use fixbolt_session::DropReason;
