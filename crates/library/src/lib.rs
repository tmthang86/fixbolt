#![doc = include_str!("../README.md")]
#![warn(missing_docs)]
#![cfg_attr(docsrs, feature(doc_cfg))]

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

/// Why `as_i64`, `as_u32`, `as_char` or `as_decimal` could not read a value.
/// Re-exported so a caller can match on it without depending on
/// `fixbolt-codec` itself.
pub use fixbolt_codec::ConvertError;

/// Reading a price, a quantity or any FIX float, and writing one back
/// (ADR-0120, `docs/decisions/`).
///
/// Read with `as_decimal(view.get(44)?)`, the way `as_i64` reads an integer.
/// Equality is structural — `1.5 != 1.50` — and a value that did not arrive in
/// canonical form writes back canonically: echo `view.get(tag)` when the
/// counterparty's exact bytes matter.
///
/// ```
/// use fixbolt::{ConvertError, Decimal, as_decimal};
///
/// let price = as_decimal(b"12345.6789").unwrap();
/// assert_eq!((price.mantissa(), price.exponent()), (123_456_789, -4));
///
/// let mut out = [0u8; Decimal::MAX_LEN];
/// assert_eq!(price.format(&mut out), b"12345.6789");
/// assert_eq!(as_decimal(b"002000.00").unwrap().format(&mut out), b"2000.00");
/// // FIX floats carry a minus or nothing; a `+` is not a number.
/// assert_eq!(as_decimal(b"+200.00"), Err(ConvertError::NotANumber));
/// ```
pub use fixbolt_codec::Decimal;
/// Read a FIX float as a [`Decimal`]; the grammar is the session's own.
pub use fixbolt_codec::as_decimal;

/// Who this acceptor serves, when, and under what numbers.
pub use fixbolt_session::{
    Application, Config, DictionaryChecks, Link, ResetPolicy, Role,
    schedule::{Schedule, Weekday, Weekdays},
};
pub use fixbolt_session::{MAX_BEGIN_STRING_LEN, MAX_COMP_ID_LEN, Peer};

/// Starting, stopping, and why a connection ended.
pub use fixbolt_engine::{ServeError, Shutdown, serve_hft};

/// The third-party notice for the QuickFIX-derived dictionaries this crate
/// builds on, transitively, through `fixbolt-dict` (ADR-0104). **Anyone
/// distributing a binary built with `fixbolt` carries QuickFIX-derived
/// tables and owes the QuickFIX Software License's conditions 2 and 3** —
/// printing this constant wherever the application already lists its
/// third-party notices satisfies condition 3's "in the software itself"
/// clause. See `docs/GUIDE.md` for the rest of what a distributor owes.
pub use fixbolt_dict::NOTICE;

/// The dictionary a message is read and written by (ADR-0207 decision 5).
///
/// [`dict::Fix44`] is the default everywhere a dictionary is a type parameter
/// — [`App`], [`Handler`], [`Reply`], [`Incoming`] — and a dictionary of the
/// application's own is any type implementing [`dict::Dictionary`] and
/// [`dict::Tables`]. **The parameter is not yet used**: plan
/// `2026-09-26-docs-for-embedders` step 26 makes it the dictionary the parse
/// and the reply go through.
pub mod dict {
    pub use fixbolt_codec::{Dictionary, TagValue};
    pub use fixbolt_dict::{FieldType, Fix44, Tables};
}

/// The `*_with` forms, for a deployment that must name `N`, `RX` and `TX`.
///
/// `[2026-09-05]` **`docs/CONFIGURATION.md` used to tell a reader to
/// "instantiate `Engine<..., N, RX, TX>` directly", and from this crate that was
/// not possible** — `Engine` is deliberately not re-exported, so the only way to
/// a 16 KiB receive buffer was to depend on `fixbolt-engine` and rewrite the
/// serving loop. These are the way. See `fixbolt_engine::serve_with`.
pub use fixbolt_engine::serve_hft_with;

/// `serve` blocks when idle and is the default mode, so it is the one an
/// example reaches for — and it exists only under `standard`.
///
/// **The `#[cfg]` is on the item, not only in `Cargo.toml`.** Non-negotiable 6:
/// a feature that gates a manifest entry and nothing in the source is a claim
/// the compiler cannot check. `unix` rides along because `engine` has no poller
/// anywhere else, so on such a target this name does not exist rather than
/// failing at startup.
#[cfg(all(feature = "standard", unix))]
pub use fixbolt_engine::{serve, serve_with, serve_with_recovery, serve_with_recovery_with};

/// Dialling out, and the rule for coming back.
///
/// `[added 2026-09-05]` `Settings::into_initiator` hands back a
/// [`reconnect::Policy`], so the front door that takes one has to be nameable
/// from here too: a caller who can build the argument and cannot name the
/// function has a configuration file it cannot honour, which is the same shape
/// `Settings::log` already argues against.
#[cfg(all(feature = "standard", unix))]
pub use fixbolt_engine::{connect_and_serve, connect_and_serve_with};

/// The backoff ladder an initiator follows after a connection ends.
pub use fixbolt_engine::reconnect;

/// `hft` has no `standard` to depend on, so its recovery entry point is always
/// present.
pub use fixbolt_engine::{serve_hft_with_recovery, serve_hft_with_recovery_with};

/// Which counterparty a connection is, decided before a session exists.
pub use fixbolt_engine::presession::{Entry, Identity, LimitError, Limits, Registry, Table};

/// Who this acceptor serves, out of a file rather than out of a rebuild.
pub use fixbolt_engine::settings::{ConnectionType, Problem, Settings, SettingsError};

pub use fixbolt_engine::MAX_ON_LOGON;
pub use fixbolt_engine::observe::{
    Admin, Command, Event, EventKind, Handles, Observer, SessionSnapshot, Snapshot,
};
/// What an operator can see and change while the engine runs.
pub use fixbolt_engine::origin::{ORIGIN_CAPACITY, ORIGIN_LEN, Sender};

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

/// SBE 1.0, decoded and encoded over generated tables (ADR-0081) — a codec you
/// bring your own transport to, not a session.
///
/// **This is not another mode of `serve*`.** ADR-0078 keeps SBE out of the FIX
/// session layer entirely: it carries no `BeginString`, no `MsgSeqNum`, no
/// `Logon` — nothing the session state machine or `serve*` needs — so there is
/// no `serve_sbe` and never will be one behind this feature. What lands here
/// is [`sbe::SbeView`] to read a message and [`sbe::MessageWriter`] to write
/// one, both over `&'static` tables `sbe-gen` compiles from a schema; the
/// caller supplies the socket, the framing (SOFH or otherwise) and the loop.
///
/// `ADR-0082` decision 4 draws the boundary at the type: `Sbe<S>` implements
/// `codec::Encoding` so it can be measured on the same footing as tag=value,
/// but `Session<Sbe<S>, _>` is rejected before the dictionary is ever asked —
/// on the five associated-type equalities `crates/session/src/lib.rs` states
/// as *"What the session needs of an `Encoding`, beyond the trait"`. The
/// doctest below is that rejection, kept honest by the compiler on every
/// build of this feature.
///
/// ```compile_fail
/// use fixbolt::sbe::{ByteOrder, MessageLayout, Sbe, Schema};
/// use fixbolt_session::{Acceptor, Config, Session};
///
/// // A schema is enough to name `Sbe<S>`; this example never decodes a
/// // message, because the point is what will not compile.
/// struct DocSchema;
/// impl Schema for DocSchema {
///     const ID: u16 = 1;
///     const VERSION: u16 = 0;
///     const BYTE_ORDER: ByteOrder = ByteOrder::Little;
///     fn message(_template_id: u16) -> Option<&'static MessageLayout> {
///         None
///     }
/// }
///
/// let cfg = Config::acceptor(b"FIX.4.4", b"US", b"THEM");
/// // Fails: `Sbe<DocSchema>` does not satisfy the bounds `Session::new`
/// // requires of its `Encoding` — `Sbe<S>::View<'_>` is `SbeView<'_>`, never
/// // `MessageView<'_, N>` for any `N`, and `SbeTables<S>` implements
/// // `codec::Dictionary` only, never `dict::Tables` (ADR-0082 decision 4).
/// let _session: Session<Sbe<DocSchema>, Acceptor> = Session::new(cfg);
/// ```
#[cfg(feature = "sbe")]
pub use fixbolt_sbe as sbe;
