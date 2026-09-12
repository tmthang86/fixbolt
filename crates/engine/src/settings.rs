//! Who this acceptor serves, read from a file.
//!
//! # The gap this closes
//!
//! `[verified 2026-09-02]` the only way to put a counterparty into a
//! [`Table`] was [`Table::serving`], which is Rust and
//! therefore a recompilation. Adding a counterparty to a running acceptor is an
//! operator's job, usually the evening before that counterparty reaches UAT.
//! Behind a rebuild it needs a toolchain and the source, it makes changing a
//! `HeartBtInt` the same class of release as changing the hot path, and it
//! leaves no way to diff two environments except by reading two programs.
//!
//! [`Table::serving`]: crate::presession::Table::serving
//!
//! # The format is QuickFIX's, and the reason is the reader
//!
//! ```ini
//! [DEFAULT]
//! BeginString=FIX.4.4
//! SenderCompID=ISLD
//!
//! [SESSION]
//! TargetCompID=TW44
//!
//! [SESSION]
//! TargetCompID=BANZAI
//! HeartBtInt=60
//! ```
//!
//! `[DEFAULT]` supplies values to every `[SESSION]` after it; a `[SESSION]`
//! overrides its own. Every FIX operator alive has already read a file shaped
//! like this, which is the whole argument for it. **Nothing is copied from
//! QuickFIX** — the shape is data, the parser here is this crate's
//! ([ADR-0001](../../../docs/decisions/ADR-0001-relationship-to-quickfix.md)).
//!
//! # Three rules that differ from QuickFIX on purpose
//!
//! 1. **An unrecognised key is an error.** QuickFIX ignores settings it does not
//!    know. Here a mistyped `Starttime` would fall back to a default, and the
//!    default schedule is [`Schedule::always`] — a session that should close at
//!    five would quietly stay open all night. That is the shape
//!    [ADR-0026](../../../docs/decisions/ADR-0026-a-counterparty-registry-in-the-pre-session-stage.md)
//!    decision 6 already refused once.
//! 2. **A file with no `[SESSION]` is an error.** An empty [`Table`] refuses
//!    every connection, so a mistyped path would behave exactly like a firewall
//!    dropping the port — two causes, one observable, which is the trap
//!    `docs/reference/two-time-rules-share-one-observable.md` has now cost three
//!    times.
//! 3. **Every error names its line.** The person editing this file does not read
//!    Rust, and *"line 14: unknown key `Starttime`"* is usable where a type name
//!    is not.
//!
//! [`Schedule::always`]: fixbolt_session::schedule::Schedule::always
//! [`Table`]: crate::presession::Table
//!
//! # Where the allocation goes
//!
//! Here, at startup, and nowhere near a turn. Parsing allocates a
//! [`String`] for the file and a [`Vec`] for the configurations;
//! [`Registry::lookup`](crate::presession::Registry::lookup) still allocates
//! nothing, and `benches/alloc.rs` case `registry-lookup` still reads 0.

use std::fmt;
use std::path::{Path, PathBuf};

use fixbolt_session::schedule::{Schedule, Weekday, Weekdays};
use fixbolt_session::{Config, MAX_BEGIN_STRING_LEN, MAX_COMP_ID_LEN};

use crate::presession::Table;

/// One recognised setting.
///
/// An enum rather than a bare string so that every place a key is handled is
/// **exhaustive**: adding a variant without teaching the parser what to do with
/// it does not compile. There is no `_` arm anywhere in this module, for the
/// same reason `Refusal` has none.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Key {
    BeginString,
    SenderCompId,
    TargetCompId,
    HeartBtInt,
    MaxSkewMillis,
    StartTime,
    EndTime,
    StartDay,
    EndDay,
    Weekdays,
    /// **`[DEFAULT]` only, and engine-wide.** One engine writes one log, and
    /// `conn=` tells the counterparties apart inside it — see
    /// [`Settings::log`].
    FileLogPath,
    /// **`[DEFAULT]` only.** Which role this whole file configures. A file
    /// names one role: the two are served by different entry points taking
    /// different things, and a file that meant both would have to be read twice
    /// to find out which half applied.
    ConnectionType,
    /// Initiator only: where to dial.
    SocketConnectHost,
    /// Initiator only: which port.
    SocketConnectPort,
    /// Initiator only: the first backoff delay, **in seconds**.
    ReconnectInterval,
    /// Initiator only: the largest backoff delay, **in seconds**. QuickFIX has
    /// no such key; without one the ladder would double forever.
    ReconnectCeiling,
    /// Restart both counts as the connection is made.
    ResetOnLogon,
    /// Restart both counts once the `Logout` exchange is over.
    ResetOnLogout,
    /// Restart both counts when the link drops for any other reason.
    ResetOnDisconnect,
    /// How long a connection may sit without completing its `Logon`, **in
    /// seconds**.
    LogonTimeout,
    /// How long to wait for the `Logout` this end asked for, **in seconds**.
    LogoutTimeout,
    /// Do not refuse a defined tag that this `MsgType` does not carry.
    AllowUnknownMsgFields,
    /// Ask the dictionary about tags at or above 5000. `Y` is the default and
    /// means **keep checking**.
    ValidateUserDefinedFields,
    /// QuickFIX **C++**'s spelling. QuickFIX/J and quickfixgo call the same
    /// thing `EnableNextExpectedMsgSeqNum`, and this repository had recorded
    /// the Java one — see
    /// `docs/reference/who-owns-the-outbound-header.md`. The C++ name is
    /// taken because that is the engine `scripts/interop.sh` runs against, so
    /// a file written for one works unchanged against the other.
    SendNextExpectedMsgSeqNum,
    /// The QuickFIX/J and QuickFIX/n spelling, because QuickFIX C++ has no key
    /// for this at all — it never sends the field.
    EnableLastMsgSeqNumProcessed,
    /// Fractional digits on `52=SendingTime` going out: `3`, `6` or `9`.
    ///
    /// **QuickFIX C++'s spelling and QuickFIX C++'s shape** — an integer, not a
    /// named width. `[researched 2026-09-09]` that engine reads it with
    /// `getInt` and accepts 0–9 (`Session.h:167–174`), defaulting to 3
    /// (`Session.cpp:68`), so a `.cfg` shared between the two ends carries a
    /// number. This engine writes only 3, 6 and 9, and **refuses the other
    /// seven rather than rounding to the nearest** — ADR-0057 open question 3.
    TimestampPrecision,
    /// **`[DEFAULT]` only, and the acceptor's.** Whether this listener speaks
    /// TLS at all.
    ///
    /// QuickFIX **C++ has no key for this** — it selects TLS by instantiating
    /// `SSLSocketAcceptor` instead, which is a decision made in C++ and not in
    /// a file. This is QuickFIX/J's spelling, taken under the naming law in
    /// `docs/CONFIGURATION.md` §1: C++'s name where C++ has one, J's where it
    /// does not.
    SocketUseSsl,
    /// **`[DEFAULT]` only.** The PEM chain this acceptor presents. QuickFIX
    /// C++'s spelling.
    ServerCertificateFile,
    /// **`[DEFAULT]` only.** The private key for that chain. QuickFIX C++'s
    /// spelling.
    ServerCertificateKeyFile,
    /// **`[DEFAULT]` only.** Refuse the deployment rather than fall back to
    /// userspace TLS.
    ///
    /// No engine surveyed has this key, so the name is this repository's, on
    /// the precedent of `ReconnectCeiling`. It carries a bare `Y`/`N` because
    /// that is all [`serve_tls_requiring`] takes, and it is checked twice —
    /// once before `bind`, once per connection — by ADR-0060.
    ///
    /// [`serve_tls_requiring`]: crate::serve_tls_requiring
    TlsRequireKernel,
}

impl Key {
    /// The spelling in the file, or `None` — which is an error, never a shrug.
    fn parse(name: &str) -> Option<Self> {
        match name {
            "BeginString" => Some(Self::BeginString),
            "SenderCompID" => Some(Self::SenderCompId),
            "TargetCompID" => Some(Self::TargetCompId),
            "HeartBtInt" => Some(Self::HeartBtInt),
            "MaxSkewMillis" => Some(Self::MaxSkewMillis),
            "StartTime" => Some(Self::StartTime),
            "EndTime" => Some(Self::EndTime),
            "StartDay" => Some(Self::StartDay),
            "EndDay" => Some(Self::EndDay),
            "Weekdays" => Some(Self::Weekdays),
            // QuickFIX's own spelling, so somebody arriving with an existing
            // configuration file recognises it without reading anything.
            "FileLogPath" => Some(Self::FileLogPath),
            "ConnectionType" => Some(Self::ConnectionType),
            "SocketConnectHost" => Some(Self::SocketConnectHost),
            "SocketConnectPort" => Some(Self::SocketConnectPort),
            "ReconnectInterval" => Some(Self::ReconnectInterval),
            "ReconnectCeiling" => Some(Self::ReconnectCeiling),
            "ResetOnLogon" => Some(Self::ResetOnLogon),
            "ResetOnLogout" => Some(Self::ResetOnLogout),
            "ResetOnDisconnect" => Some(Self::ResetOnDisconnect),
            "LogonTimeout" => Some(Self::LogonTimeout),
            "LogoutTimeout" => Some(Self::LogoutTimeout),
            "AllowUnknownMsgFields" => Some(Self::AllowUnknownMsgFields),
            "ValidateUserDefinedFields" => Some(Self::ValidateUserDefinedFields),
            "SendNextExpectedMsgSeqNum" => Some(Self::SendNextExpectedMsgSeqNum),
            "EnableLastMsgSeqNumProcessed" => Some(Self::EnableLastMsgSeqNumProcessed),
            "TimestampPrecision" => Some(Self::TimestampPrecision),
            "SocketUseSSL" => Some(Self::SocketUseSsl),
            "ServerCertificateFile" => Some(Self::ServerCertificateFile),
            "ServerCertificateKeyFile" => Some(Self::ServerCertificateKeyFile),
            "TlsRequireKernel" => Some(Self::TlsRequireKernel),
            _ => None,
        }
    }

    const fn name(self) -> &'static str {
        match self {
            Self::BeginString => "BeginString",
            Self::SenderCompId => "SenderCompID",
            Self::TargetCompId => "TargetCompID",
            Self::HeartBtInt => "HeartBtInt",
            Self::MaxSkewMillis => "MaxSkewMillis",
            Self::StartTime => "StartTime",
            Self::EndTime => "EndTime",
            Self::StartDay => "StartDay",
            Self::EndDay => "EndDay",
            Self::Weekdays => "Weekdays",
            Self::FileLogPath => "FileLogPath",
            Self::ConnectionType => "ConnectionType",
            Self::SocketConnectHost => "SocketConnectHost",
            Self::SocketConnectPort => "SocketConnectPort",
            Self::ReconnectInterval => "ReconnectInterval",
            Self::ReconnectCeiling => "ReconnectCeiling",
            Self::ResetOnLogon => "ResetOnLogon",
            Self::ResetOnLogout => "ResetOnLogout",
            Self::ResetOnDisconnect => "ResetOnDisconnect",
            Self::LogonTimeout => "LogonTimeout",
            Self::LogoutTimeout => "LogoutTimeout",
            Self::AllowUnknownMsgFields => "AllowUnknownMsgFields",
            Self::ValidateUserDefinedFields => "ValidateUserDefinedFields",
            Self::SendNextExpectedMsgSeqNum => "SendNextExpectedMsgSeqNum",
            Self::EnableLastMsgSeqNumProcessed => "EnableLastMsgSeqNumProcessed",
            Self::TimestampPrecision => "TimestampPrecision",
            Self::SocketUseSsl => "SocketUseSSL",
            Self::ServerCertificateFile => "ServerCertificateFile",
            Self::ServerCertificateKeyFile => "ServerCertificateKeyFile",
            Self::TlsRequireKernel => "TlsRequireKernel",
        }
    }
}

/// Which role a configuration file describes.
///
/// **A file names one.** The two roles are served by different entry points
/// taking different arguments — `serve(addr, table, ..)` against
/// `connect_and_serve(addr, cfg, .., policy, ..)` — so a file meaning both
/// would have to be read twice to find out which half applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ConnectionType {
    /// Wait to be dialled. The default, and what every file written before
    /// 2026-09-05 means.
    #[default]
    Acceptor,
    /// Dial out, and keep dialling. Requires `SocketConnectHost` and
    /// `SocketConnectPort`.
    Initiator,
}

/// What went wrong with the configuration, in the words an operator needs.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Problem {
    /// The file could not be read at all.
    Unreadable,
    /// A line that is neither a section header nor `Key=Value`.
    NotAKeyValue,
    /// `[Something]` that is neither `[DEFAULT]` nor `[SESSION]`.
    UnknownSection,
    /// A setting before the first section header.
    KeyOutsideSection,
    /// A key this engine does not recognise. **Not ignored** — see the module
    /// documentation.
    UnknownKey,
    /// A key that belongs in `[DEFAULT]` and was found in a `[SESSION]`.
    ///
    /// Two of them. `FileLogPath`, because an engine writes **one** log and two
    /// counterparties asking for two files is a configuration that cannot be
    /// honoured; and `ConnectionType`, because a file names one role. Both are
    /// refused rather than resolved by picking one — the operator would never
    /// learn which.
    ///
    /// `[renamed 2026-09-05]` This was `SessionOnly`, which said the opposite
    /// of what it means.
    DefaultOnly,
    /// A key that belongs to the other role.
    ///
    /// Either a dialling key in a file that is not an initiator, or one of
    /// [`Settings::into_table`] / [`Settings::into_initiator`] called on a file
    /// declaring the other role. **The second is the expensive one**: an
    /// initiator file poured into `into_table` would build a working acceptor
    /// that serves the venue instead of dialling it, and nothing on the wire
    /// would say so, because nothing would happen on the wire.
    WrongRole,
    /// `ConnectionType=initiator` with more than one `[SESSION]`.
    ///
    /// An initiator holds one session and `connect_and_serve` takes one
    /// `Config`. Picking the first would be a guess about which counterparty
    /// the author meant.
    OneSessionPerInitiator,
    /// `ReconnectInterval` and `ReconnectCeiling` describe no ladder — a zero
    /// first delay, or a ceiling below it.
    ImpossiblePolicy,
    /// A key that wants `Y` or `N` got something else. **Not guessed at**:
    /// `true` read as `Y` today is `1` read as `N` tomorrow.
    NotAFlag,
    /// `ConnectionType` that is neither `acceptor` nor `initiator`.
    BadConnectionType,
    /// The same key twice in one block, which has no meaning to give it.
    RepeatedKey,
    /// A required key is missing from both `[DEFAULT]` and the `[SESSION]`.
    MissingKey,
    /// A value longer than a [`Config`] can hold. It is refused rather than
    /// truncated, because a truncated name matches nothing and would configure
    /// an acceptor that serves nobody.
    ValueTooLong,
    /// A key that wants a number did not get one.
    NotANumber,
    /// `TimestampPrecision` named a width this engine does not write.
    ///
    /// QuickFIX C++ accepts any integer 0–9; this engine writes 3, 6 or 9.
    /// **Refused and not rounded**: answering `TimestampPrecision=4` with six
    /// digits would put a resolution on the wire that the operator did not
    /// choose, and nothing downstream would say so. ADR-0057 open question 3.
    UnsupportedPrecision,
    /// No `[SESSION]` block at all. An empty table refuses every connection,
    /// which is indistinguishable from a network fault.
    NoSessions,
    /// Two `[SESSION]` blocks naming the same FIX identity.
    DuplicateSession,
    /// A time that is not `HH:MM:SS`, or not a time of day.
    BadTime,
    /// A day name this parser does not know.
    BadWeekday,
    /// Times and days that name no schedule the session layer will build — a
    /// zero-length window, or weekdays on a weekly one.
    ImpossibleSchedule,
    /// `SocketUseSSL=Y` in a build that was compiled without the `tls`
    /// feature.
    ///
    /// **Refused cleanly, not parsed and ignored** — non-negotiable 6. A
    /// build with nothing optional installed has no `rustls` in it at all, so
    /// the only two honest answers to this file are this error and a plaintext
    /// acceptor that nobody asked for. `docs/CONFIGURATION.md` §4 lists which
    /// features a key needs.
    NeedsFeature,
    /// A file asking for TLS handed to [`Settings::into_table`].
    ///
    /// **The expensive mistake this catches**: the table would be perfectly
    /// well formed, the port would answer, and the acceptor built from it would
    /// serve **plaintext** with the certificate unread on disk. Nothing on the
    /// wire would say so. Same shape as [`Problem::WrongRole`], same answer —
    /// a line number and the door that works, [`Settings::into_tls_table`].
    NeedsTlsDoor,
}

impl fmt::Display for Problem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::Unreadable => "the file could not be read",
            Self::NotAKeyValue => "expected `Key=Value` or a `[SECTION]` header",
            Self::UnknownSection => "unknown section — expected [DEFAULT] or [SESSION]",
            Self::KeyOutsideSection => "a setting before the first [DEFAULT] or [SESSION]",
            Self::UnknownKey => "unknown key",
            Self::DefaultOnly => "this key belongs in [DEFAULT], not in a [SESSION]",
            Self::WrongRole => "this belongs to the other ConnectionType",
            Self::OneSessionPerInitiator => "an initiator holds one session, so one [SESSION]",
            Self::ImpossiblePolicy => "these reconnect bounds describe no ladder",
            Self::NotAFlag => "expected Y or N",
            Self::BadConnectionType => {
                "expected ConnectionType=acceptor or ConnectionType=initiator"
            }
            Self::RepeatedKey => "the same key twice in one block",
            Self::MissingKey => "a required key is missing",
            Self::ValueTooLong => "the value is longer than a session configuration can hold",
            Self::NotANumber => "expected a number",
            Self::UnsupportedPrecision => {
                "expected TimestampPrecision=3, 6 or 9 — this engine writes no other width"
            }
            Self::NoSessions => "no [SESSION] block — this acceptor would serve nobody",
            Self::DuplicateSession => "two [SESSION] blocks name the same FIX identity",
            Self::BadTime => "expected a time of day as HH:MM:SS",
            Self::BadWeekday => "expected a weekday, e.g. Monday or Mon",
            Self::ImpossibleSchedule => "these times and days describe no session",
            Self::NeedsFeature => "this engine was built without the feature this key needs",
            Self::NeedsTlsDoor => {
                "this file asks for TLS, and into_table() would serve it as plaintext"
            }
        };
        f.write_str(s)
    }
}

/// A [`Problem`], the line it is on, and what was written there.
///
/// The line number is the point: configuration is edited by people who do not
/// read Rust, and an error without a line sends them to read the whole file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingsError {
    line: usize,
    problem: Problem,
    detail: String,
}

impl SettingsError {
    /// The 1-based line, or 0 for a problem about the file as a whole.
    #[must_use]
    pub const fn line(&self) -> usize {
        self.line
    }

    /// What kind of problem it is.
    #[must_use]
    pub const fn problem(&self) -> &Problem {
        &self.problem
    }

    fn at(line: usize, problem: Problem, detail: impl Into<String>) -> Self {
        Self {
            line,
            problem,
            detail: detail.into(),
        }
    }
}

impl fmt::Display for SettingsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.line == 0 {
            write!(f, "{}: {}", self.problem, self.detail)
        } else {
            write!(f, "line {}: {}: {}", self.line, self.problem, self.detail)
        }
    }
}

impl std::error::Error for SettingsError {}

/// One `[DEFAULT]` or `[SESSION]` block, before it becomes a [`Config`].
///
/// Values borrow the text. The line number travels with each so that an error
/// discovered at the end — a missing key, a value too long — still points at
/// the line that caused it rather than at the end of the block.
#[derive(Debug, Default, Clone, Copy)]
struct Block<'a> {
    begin_string: Option<(usize, &'a str)>,
    sender: Option<(usize, &'a str)>,
    target: Option<(usize, &'a str)>,
    heart_bt_int: Option<(usize, &'a str)>,
    max_skew: Option<(usize, &'a str)>,
    start_time: Option<(usize, &'a str)>,
    end_time: Option<(usize, &'a str)>,
    start_day: Option<(usize, &'a str)>,
    end_day: Option<(usize, &'a str)>,
    weekdays: Option<(usize, &'a str)>,
    host: Option<(usize, &'a str)>,
    port: Option<(usize, &'a str)>,
    reconnect_interval: Option<(usize, &'a str)>,
    reconnect_ceiling: Option<(usize, &'a str)>,
    reset_on_logon: Option<(usize, &'a str)>,
    reset_on_logout: Option<(usize, &'a str)>,
    reset_on_disconnect: Option<(usize, &'a str)>,
    logon_timeout: Option<(usize, &'a str)>,
    logout_timeout: Option<(usize, &'a str)>,
    allow_unknown_msg_fields: Option<(usize, &'a str)>,
    validate_user_defined_fields: Option<(usize, &'a str)>,
    send_next_expected: Option<(usize, &'a str)>,
    enable_last_processed: Option<(usize, &'a str)>,
    timestamp_precision: Option<(usize, &'a str)>,
}

impl<'a> Block<'a> {
    /// Record one setting, refusing a repeat.
    ///
    /// The `match` is exhaustive over [`Key`]: a new setting that is parsed but
    /// never stored does not compile.
    fn set(&mut self, key: Key, line: usize, value: &'a str) -> Result<(), SettingsError> {
        let slot = match key {
            Key::BeginString => &mut self.begin_string,
            Key::SenderCompId => &mut self.sender,
            Key::TargetCompId => &mut self.target,
            Key::HeartBtInt => &mut self.heart_bt_int,
            Key::MaxSkewMillis => &mut self.max_skew,
            Key::StartTime => &mut self.start_time,
            Key::EndTime => &mut self.end_time,
            Key::StartDay => &mut self.start_day,
            Key::EndDay => &mut self.end_day,
            Key::Weekdays => &mut self.weekdays,
            Key::SocketConnectHost => &mut self.host,
            Key::SocketConnectPort => &mut self.port,
            Key::ReconnectInterval => &mut self.reconnect_interval,
            Key::ReconnectCeiling => &mut self.reconnect_ceiling,
            Key::ResetOnLogon => &mut self.reset_on_logon,
            Key::ResetOnLogout => &mut self.reset_on_logout,
            Key::ResetOnDisconnect => &mut self.reset_on_disconnect,
            Key::LogonTimeout => &mut self.logon_timeout,
            Key::LogoutTimeout => &mut self.logout_timeout,
            Key::AllowUnknownMsgFields => &mut self.allow_unknown_msg_fields,
            Key::ValidateUserDefinedFields => &mut self.validate_user_defined_fields,
            Key::SendNextExpectedMsgSeqNum => &mut self.send_next_expected,
            Key::EnableLastMsgSeqNumProcessed => &mut self.enable_last_processed,
            Key::TimestampPrecision => &mut self.timestamp_precision,
            // Handled before a block ever sees them. A `[SESSION]` carrying one
            // is refused in `parse`, not here, so the error can say why.
            Key::FileLogPath
            | Key::ConnectionType
            | Key::SocketUseSsl
            | Key::ServerCertificateFile
            | Key::ServerCertificateKeyFile
            | Key::TlsRequireKernel => {
                return Err(SettingsError::at(line, Problem::DefaultOnly, key.name()));
            }
        };
        if slot.is_some() {
            return Err(SettingsError::at(line, Problem::RepeatedKey, key.name()));
        }
        *slot = Some((line, value));
        Ok(())
    }

    /// A `[SESSION]`'s own settings over the `[DEFAULT]` block's.
    fn over(self, base: Self) -> Self {
        Self {
            begin_string: self.begin_string.or(base.begin_string),
            sender: self.sender.or(base.sender),
            target: self.target.or(base.target),
            heart_bt_int: self.heart_bt_int.or(base.heart_bt_int),
            max_skew: self.max_skew.or(base.max_skew),
            start_time: self.start_time.or(base.start_time),
            end_time: self.end_time.or(base.end_time),
            start_day: self.start_day.or(base.start_day),
            end_day: self.end_day.or(base.end_day),
            weekdays: self.weekdays.or(base.weekdays),
            host: self.host.or(base.host),
            port: self.port.or(base.port),
            reconnect_interval: self.reconnect_interval.or(base.reconnect_interval),
            reconnect_ceiling: self.reconnect_ceiling.or(base.reconnect_ceiling),
            reset_on_logon: self.reset_on_logon.or(base.reset_on_logon),
            reset_on_logout: self.reset_on_logout.or(base.reset_on_logout),
            reset_on_disconnect: self.reset_on_disconnect.or(base.reset_on_disconnect),
            logon_timeout: self.logon_timeout.or(base.logon_timeout),
            logout_timeout: self.logout_timeout.or(base.logout_timeout),
            allow_unknown_msg_fields: self
                .allow_unknown_msg_fields
                .or(base.allow_unknown_msg_fields),
            validate_user_defined_fields: self
                .validate_user_defined_fields
                .or(base.validate_user_defined_fields),
            send_next_expected: self.send_next_expected.or(base.send_next_expected),
            enable_last_processed: self.enable_last_processed.or(base.enable_last_processed),
            timestamp_precision: self.timestamp_precision.or(base.timestamp_precision),
        }
    }

    /// The dialling key this block carries, if any, so an acceptor file can
    /// refuse it by line.
    fn dialling_key(self) -> Option<(usize, Key)> {
        [
            (self.host, Key::SocketConnectHost),
            (self.port, Key::SocketConnectPort),
            (self.reconnect_interval, Key::ReconnectInterval),
            (self.reconnect_ceiling, Key::ReconnectCeiling),
        ]
        .into_iter()
        .find_map(|(slot, key)| slot.map(|(line, _)| (line, key)))
    }
}

/// The four `[DEFAULT]`-only TLS keys, before they become a [`TlsSettings`].
///
/// A block of its own rather than four more fields on [`Block`], because
/// [`Block`] is merged per `[SESSION]` and these are never per-session: one
/// listener presents one certificate, and SNI is out of scope (ADR-0005
/// question 5). Keeping them out of [`Block`] is what makes
/// "a TLS key in a `[SESSION]` is an error" a fact about the type rather than
/// a rule somebody has to remember.
#[derive(Debug, Default, Clone, Copy)]
struct TlsBlock<'a> {
    use_ssl: Option<(usize, &'a str)>,
    certificate: Option<(usize, &'a str)>,
    private_key: Option<(usize, &'a str)>,
    require_kernel: Option<(usize, &'a str)>,
}

impl<'a> TlsBlock<'a> {
    /// The slot this key fills, or `None` when the key is not one of these
    /// four.
    ///
    /// The `match` is exhaustive over [`Key`] and has **no `_` arm**, for the
    /// same reason nothing else in this module does: a new setting has to be
    /// classified here before it compiles.
    fn slot(&mut self, key: Key) -> Option<&mut Option<(usize, &'a str)>> {
        match key {
            Key::SocketUseSsl => Some(&mut self.use_ssl),
            Key::ServerCertificateFile => Some(&mut self.certificate),
            Key::ServerCertificateKeyFile => Some(&mut self.private_key),
            Key::TlsRequireKernel => Some(&mut self.require_kernel),
            Key::BeginString
            | Key::SenderCompId
            | Key::TargetCompId
            | Key::HeartBtInt
            | Key::MaxSkewMillis
            | Key::StartTime
            | Key::EndTime
            | Key::StartDay
            | Key::EndDay
            | Key::Weekdays
            | Key::FileLogPath
            | Key::ConnectionType
            | Key::SocketConnectHost
            | Key::SocketConnectPort
            | Key::ReconnectInterval
            | Key::ReconnectCeiling
            | Key::ResetOnLogon
            | Key::ResetOnLogout
            | Key::ResetOnDisconnect
            | Key::LogonTimeout
            | Key::LogoutTimeout
            | Key::AllowUnknownMsgFields
            | Key::ValidateUserDefinedFields
            | Key::SendNextExpectedMsgSeqNum
            | Key::EnableLastMsgSeqNumProcessed
            | Key::TimestampPrecision => None,
        }
    }

    /// The first of the four this file carries, in a fixed order so that the
    /// key blamed for a whole-block problem does not depend on where the
    /// operator happened to type it.
    fn anchor(self) -> Option<(usize, Key)> {
        [
            (self.use_ssl, Key::SocketUseSsl),
            (self.certificate, Key::ServerCertificateFile),
            (self.private_key, Key::ServerCertificateKeyFile),
            (self.require_kernel, Key::TlsRequireKernel),
        ]
        .into_iter()
        .find_map(|(slot, key)| slot.map(|(line, _)| (line, key)))
    }

    /// The four keys checked against each other, against the role, and against
    /// what this build can actually do.
    ///
    /// Returns the `SocketUseSSL` line beside the settings, so
    /// [`Settings::into_table`] can refuse by line rather than by type name.
    fn settle(self, role: ConnectionType) -> Result<Option<(usize, TlsSettings)>, SettingsError> {
        let Some((anchor_line, anchor_key)) = self.anchor() else {
            return Ok(None);
        };
        // **Acceptor-only, and the same refusal a dialling key gets on an
        // acceptor file.** These four describe the certificate a *server*
        // presents; an initiator that needs to present one needs
        // `ClientCertificateFile`, which this engine does not have yet.
        if role == ConnectionType::Initiator {
            return Err(SettingsError::at(
                anchor_line,
                Problem::WrongRole,
                format!("{} needs ConnectionType=acceptor", anchor_key.name()),
            ));
        }
        let enabled = match self.use_ssl {
            Some(v) => flag(v, Key::SocketUseSsl)?,
            None => false,
        };
        let ssl_line = self.use_ssl.map_or(anchor_line, |(line, _)| line);
        if !enabled {
            // A certificate, a key or `TlsRequireKernel` with the switch off or
            // absent describes something that will not happen. Refused rather
            // than read and ignored: the operator wrote down a certificate and
            // would otherwise get a plaintext acceptor with no sentence about
            // it anywhere.
            let dependent = [
                (self.certificate, Key::ServerCertificateFile),
                (self.private_key, Key::ServerCertificateKeyFile),
                (self.require_kernel, Key::TlsRequireKernel),
            ]
            .into_iter()
            .find_map(|(slot, key)| slot.map(|(line, _)| (line, key)));
            if let Some((line, key)) = dependent {
                return Err(SettingsError::at(
                    line,
                    Problem::MissingKey,
                    format!("{} does nothing without SocketUseSSL=Y", key.name()),
                ));
            }
            return Ok(None);
        }
        let certificate = required(self.certificate, Key::ServerCertificateFile, ssl_line)?;
        let private_key = required(self.private_key, Key::ServerCertificateKeyFile, ssl_line)?;
        let require_kernel = match self.require_kernel {
            Some(v) => flag(v, Key::TlsRequireKernel)?,
            None => false,
        };

        // **Non-negotiable 6, and the only `#[cfg]` in this module.** A build
        // with nothing optional installed has no `rustls` in it, so the only
        // two honest answers to `SocketUseSSL=Y` are this error and a plaintext
        // acceptor nobody asked for. *Refused cleanly*, not *parsed and
        // ignored*.
        //
        // It is **last** on purpose: the shape of the file is the operator's
        // problem in every build, so "you forgot the certificate" is said by
        // the featureless build too, and every refusal above this line is
        // covered by a test that runs in both feature sets.
        #[cfg(not(feature = "tls"))]
        if enabled {
            return Err(SettingsError::at(
                ssl_line,
                Problem::NeedsFeature,
                "SocketUseSSL=Y needs this engine built with the `tls` feature",
            ));
        }

        Ok(Some((
            ssl_line,
            TlsSettings {
                certificate: PathBuf::from(certificate.1),
                private_key: PathBuf::from(private_key.1),
                require_kernel,
            },
        )))
    }
}

/// The certificate this acceptor presents, and whether the kernel is required
/// to carry it.
///
/// **Paths, not bytes.** Nothing here opens a file: reading the PEM belongs at
/// start-up, beside `serve_tls*`, where an unreadable certificate is an
/// `io::Error` about a path rather than a parse error about a line. A
/// configuration parser that reads the filesystem is one that fails for two
/// unrelated reasons with one message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TlsSettings {
    certificate: PathBuf,
    private_key: PathBuf,
    require_kernel: bool,
}

impl TlsSettings {
    /// The PEM chain this acceptor presents. `ServerCertificateFile`.
    #[must_use]
    pub fn certificate(&self) -> &Path {
        &self.certificate
    }

    /// The private key for that chain. `ServerCertificateKeyFile`.
    #[must_use]
    pub fn private_key(&self) -> &Path {
        &self.private_key
    }

    /// Whether a deployment that cannot offload TLS to the kernel is refused
    /// rather than served from userspace. `TlsRequireKernel`, **off unless the
    /// file says otherwise**: ADR-0060 refuses a deployment only when that
    /// deployment asked to be refused.
    #[must_use]
    pub const fn require_kernel(&self) -> bool {
        self.require_kernel
    }
}

/// A required value, or a [`Problem::MissingKey`] naming it.
fn required(
    slot: Option<(usize, &str)>,
    key: Key,
    at: usize,
) -> Result<(usize, &str), SettingsError> {
    slot.ok_or_else(|| SettingsError::at(at, Problem::MissingKey, key.name()))
}

/// A value that must fit in a [`Config`]'s fixed-size name.
///
/// The limits come from `fixbolt_session`, not from a number written here: a
/// second copy would be a second rule, and the one that disagreed would be the
/// one deciding whether a counterparty is served.
fn fitting((line, value): (usize, &str), limit: usize, key: Key) -> Result<&[u8], SettingsError> {
    if value.len() > limit {
        return Err(SettingsError::at(
            line,
            Problem::ValueTooLong,
            format!(
                "{} is {} bytes, the limit is {limit}",
                key.name(),
                value.len()
            ),
        ));
    }
    Ok(value.as_bytes())
}

/// A key whose value is `Y` or `N`.
///
/// **Nothing else, and not case-folded.** QuickFIX writes `Y` and `N`; reading
/// `true` as `Y` today is reading `1` as `N` tomorrow, and a flag guessed
/// wrongly is a session that silently keeps or drops its numbering.
fn flag((line, value): (usize, &str), key: Key) -> Result<bool, SettingsError> {
    match value {
        "Y" => Ok(true),
        "N" => Ok(false),
        _ => Err(SettingsError::at(
            line,
            Problem::NotAFlag,
            format!("{}={value}", key.name()),
        )),
    }
}

/// A key whose value is a number.
fn number<T: std::str::FromStr>(
    (line, value): (usize, &str),
    key: Key,
) -> Result<T, SettingsError> {
    value.parse().map_err(|_| {
        SettingsError::at(line, Problem::NotANumber, format!("{}={value}", key.name()))
    })
}

/// `value` is spelled the one way this engine accepts a width in
/// `TimestampPrecision`: ASCII digits only, no leading `+`, and no leading
/// `0` unless the whole value is the single digit `0`.
///
/// Rust's own integer parser reads `+3` and `03` as `3` (measured against
/// `rustc` 1.98.0), which is looser than the spelling this key is documented
/// to take. Checked in addition to, not instead of, [`number`]: a value that
/// is not a number at all (`MICROS`) stays [`Problem::NotANumber`], and only
/// a value that *is* a number but spelled wrong becomes
/// [`Problem::UnsupportedPrecision`] — probe 3 in `mod doc_table` is what
/// caught the gap (`docs/CONFIGURATION.md` §1, `TimestampPrecision`).
fn spelled_exactly_as_digits(value: &str) -> bool {
    !value.is_empty()
        && value.bytes().all(|b| b.is_ascii_digit())
        && !(value.len() > 1 && value.as_bytes().first() == Some(&b'0'))
}

/// The counterparties a configuration file names.
#[derive(Debug, Clone, Default)]
pub struct Settings {
    configs: Vec<Config>,
    log: Option<PathBuf>,
    role: ConnectionType,
    /// The `ConnectionType=` line, so the wrong door can name it. Zero when the
    /// file did not say, which is an acceptor.
    role_line: usize,
    /// `host:port` and the backoff ladder, present exactly when the role is
    /// [`ConnectionType::Initiator`].
    dial: Option<(String, crate::reconnect::Policy)>,
    /// The certificate to present, present exactly when the file said
    /// `SocketUseSSL=Y`.
    tls: Option<TlsSettings>,
    /// The `SocketUseSSL=` line, so [`Self::into_table`] can refuse a TLS file
    /// by line. Zero when the file did not ask for TLS.
    tls_line: usize,
}

/// The first backoff delay when a file declares an initiator and says nothing
/// about reconnecting, in seconds. QuickFIX's own default for
/// `ReconnectInterval`.
pub const DEFAULT_RECONNECT_INTERVAL_SECS: u64 = 30;

impl Settings {
    /// Read and parse a configuration file.
    ///
    /// # Errors
    ///
    /// [`Problem::Unreadable`] if the file cannot be read, and every parse
    /// problem otherwise — each carrying the line it is on.
    pub fn load(path: impl AsRef<Path>) -> Result<Self, SettingsError> {
        let path = path.as_ref();
        let text = std::fs::read_to_string(path).map_err(|e| {
            SettingsError::at(0, Problem::Unreadable, format!("{}: {e}", path.display()))
        })?;
        Self::parse(&text)
    }

    /// Parse configuration text.
    ///
    /// # Errors
    ///
    /// See [`Problem`]. Every error carries a 1-based line number.
    pub fn parse(text: &str) -> Result<Self, SettingsError> {
        let mut default = Block::default();
        let mut sessions: Vec<Block<'_>> = Vec::new();
        let mut log: Option<(usize, &str)> = None;
        let mut role: Option<(usize, ConnectionType)> = None;
        let mut tls = TlsBlock::default();
        // Which block the next `Key=Value` belongs to. `None` until the first
        // header, so a setting above it is refused rather than silently landing
        // in `[DEFAULT]`.
        let mut current: Option<usize> = None;
        let mut in_default = false;

        for (i, raw) in text.lines().enumerate() {
            let line = i + 1;
            // `lines()` keeps the `\r` of a CRLF file, and a value carrying one
            // is a CompID that matches nothing.
            let trimmed = raw.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with(';') {
                continue;
            }
            if let Some(name) = trimmed.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
                match name.trim() {
                    "DEFAULT" => {
                        in_default = true;
                        current = None;
                    }
                    "SESSION" => {
                        sessions.push(Block::default());
                        in_default = false;
                        current = Some(sessions.len() - 1);
                    }
                    other => {
                        return Err(SettingsError::at(line, Problem::UnknownSection, other));
                    }
                }
                continue;
            }
            let Some((name, value)) = trimmed.split_once('=') else {
                return Err(SettingsError::at(line, Problem::NotAKeyValue, trimmed));
            };
            let (name, value) = (name.trim(), value.trim());
            let Some(key) = Key::parse(name) else {
                return Err(SettingsError::at(line, Problem::UnknownKey, name));
            };
            if key == Key::FileLogPath {
                if !in_default {
                    return Err(SettingsError::at(line, Problem::DefaultOnly, name));
                }
                if log.is_some() {
                    return Err(SettingsError::at(line, Problem::RepeatedKey, name));
                }
                log = Some((line, value));
                continue;
            }
            if key == Key::ConnectionType {
                if !in_default {
                    return Err(SettingsError::at(line, Problem::DefaultOnly, name));
                }
                if role.is_some() {
                    return Err(SettingsError::at(line, Problem::RepeatedKey, name));
                }
                let what = match value {
                    "acceptor" => ConnectionType::Acceptor,
                    "initiator" => ConnectionType::Initiator,
                    other => {
                        return Err(SettingsError::at(
                            line,
                            Problem::BadConnectionType,
                            format!("ConnectionType={other}"),
                        ));
                    }
                };
                role = Some((line, what));
                continue;
            }
            // `[DEFAULT]`-only, like `FileLogPath` and for the same kind of
            // reason: one listener presents one certificate, and two
            // `[SESSION]` blocks naming two certificates is a configuration
            // that cannot be honoured on one port without SNI (ADR-0005
            // question 5, out of scope).
            if let Some(slot) = tls.slot(key) {
                if !in_default {
                    return Err(SettingsError::at(line, Problem::DefaultOnly, name));
                }
                if slot.is_some() {
                    return Err(SettingsError::at(line, Problem::RepeatedKey, name));
                }
                *slot = Some((line, value));
                continue;
            }
            match current {
                Some(i) => match sessions.get_mut(i) {
                    Some(b) => b.set(key, line, value)?,
                    // Unreachable by construction — `current` is only ever set
                    // to an index just pushed — and answered rather than
                    // `unwrap`ped, because non-negotiable 7 has no exceptions.
                    None => return Err(SettingsError::at(line, Problem::KeyOutsideSection, name)),
                },
                None if in_default => default.set(key, line, value)?,
                None => return Err(SettingsError::at(line, Problem::KeyOutsideSection, name)),
            }
        }

        if sessions.is_empty() {
            return Err(SettingsError::at(
                0,
                Problem::NoSessions,
                "add a [SESSION] block naming a counterparty",
            ));
        }

        let (role_line, role) = role.unwrap_or((0, ConnectionType::Acceptor));
        // After the whole file has been read, because `ConnectionType=` may sit
        // below the TLS keys and the role is what decides whether they belong
        // here at all.
        let tls = tls.settle(role)?;
        // An initiator holds one session and `connect_and_serve` takes one
        // `Config`. Blamed on the second block's own line rather than on the
        // `ConnectionType=` line, because the second block is what the author
        // has to delete.
        if role == ConnectionType::Initiator
            && let Some(extra) = sessions.get(1)
        {
            return Err(SettingsError::at(
                line_of(extra.over(default)),
                Problem::OneSessionPerInitiator,
                "delete the extra [SESSION], or run one engine per venue",
            ));
        }

        let mut dial = None;
        let mut configs: Vec<Config> = Vec::with_capacity(sessions.len());
        for block in sessions {
            let merged = block.over(default);
            // A dialling key in a file that does not dial is not a harmless
            // extra: the author wrote down where to connect, and this engine
            // would sit waiting to be connected to.
            match (role, merged.dialling_key()) {
                (ConnectionType::Acceptor, Some((line, key))) => {
                    return Err(SettingsError::at(
                        line,
                        Problem::WrongRole,
                        format!("{} needs ConnectionType=initiator", key.name()),
                    ));
                }
                (ConnectionType::Initiator, _) => dial = Some(dialling(merged)?),
                (ConnectionType::Acceptor, None) => {}
            }
            let cfg = build(merged)?;
            if let Some(dup) = configs.iter().find(|c| c.same_identity_as(&cfg)) {
                let _ = dup;
                return Err(SettingsError::at(
                    line_of(block),
                    Problem::DuplicateSession,
                    "an identity is served by one [SESSION] or by none",
                ));
            }
            configs.push(cfg);
        }
        Ok(Self {
            configs,
            log: log.map(|(_, v)| PathBuf::from(v)),
            role,
            role_line,
            dial,
            tls_line: tls.as_ref().map_or(0, |(line, _)| *line),
            tls: tls.map(|(_, settings)| settings),
        })
    }

    /// The configurations, in file order.
    /// Where `FileLogPath` pointed, if the file named one.
    ///
    /// **`None` is "the operator did not ask for a log", and it is the only
    /// honest reading.** A caller that has a path here and an entry point that
    /// cannot carry a log has a configuration it cannot honour, and every entry
    /// point takes one for exactly that reason — a key that silently does
    /// nothing is the failure mode `CLAUDE.md` §10 lists.
    #[must_use]
    pub fn log(&self) -> Option<&Path> {
        self.log.as_deref()
    }

    #[must_use]
    pub fn configs(&self) -> &[Config] {
        &self.configs
    }

    /// Which role this file describes. [`ConnectionType::Acceptor`] unless the
    /// file said otherwise.
    #[must_use]
    pub const fn connection_type(&self) -> ConnectionType {
        self.role
    }

    /// The certificate this file asks to present, if it asked at all.
    ///
    /// **`None` is "the operator did not ask for TLS", and it is the only
    /// honest reading** — the same rule as [`Self::log`]. There is no default
    /// certificate path, because a default one would be a file somebody else
    /// generated.
    ///
    /// Borrowed rather than owned: the caller that needs it to keep — the one
    /// building a listener — takes [`Self::into_tls_table`] and gets it by
    /// value, while everybody else is only asking a question.
    #[must_use]
    pub const fn tls(&self) -> Option<&TlsSettings> {
        self.tls.as_ref()
    }

    /// A registry serving exactly the counterparties this file names.
    ///
    /// # Errors
    ///
    /// [`Problem::WrongRole`], naming the `ConnectionType=` line, if the file
    /// declares an initiator. **This is the expensive mistake to catch**: the
    /// table would be perfectly well formed, and an acceptor built from it
    /// would sit waiting for the venue it was told to dial. Nothing on the wire
    /// would say so, because nothing would happen on the wire.
    ///
    /// It is also the whole answer for the sharded entry point, which takes a
    /// [`Table`] and nothing else — one mechanism rather than a second check to
    /// disagree with this one.
    pub fn into_table(self) -> Result<Table, SettingsError> {
        if self.role == ConnectionType::Initiator {
            return Err(SettingsError::at(
                self.role_line,
                Problem::WrongRole,
                "this file configures an initiator: call into_initiator()",
            ));
        }
        if self.tls.is_some() {
            return Err(SettingsError::at(
                self.tls_line,
                Problem::NeedsTlsDoor,
                "this file asks for TLS: call into_tls_table()",
            ));
        }
        Ok(table_of(self.configs))
    }

    /// The registry **and** the certificate, for the entry points that serve
    /// TLS.
    ///
    /// The counterpart of [`Self::into_table`]: that door refuses a file
    /// carrying `SocketUseSSL=Y`, this one is where such a file goes. Reading
    /// the PEM is the caller's next step, not this one's — see [`TlsSettings`].
    ///
    /// # Errors
    ///
    /// [`Problem::WrongRole`] if the file declares an initiator, and
    /// [`Problem::MissingKey`] if it never asked for TLS — a file with no
    /// certificate in it cannot be served over one, and answering with a
    /// plausible default would be answering with somebody else's key.
    pub fn into_tls_table(self) -> Result<(Table, TlsSettings), SettingsError> {
        if self.role == ConnectionType::Initiator {
            return Err(SettingsError::at(
                self.role_line,
                Problem::WrongRole,
                "this file configures an initiator: call into_initiator()",
            ));
        }
        let Some(tls) = self.tls else {
            return Err(SettingsError::at(
                0,
                Problem::MissingKey,
                "this file asks for no TLS: add SocketUseSSL=Y, or call into_table()",
            ));
        };
        Ok((table_of(self.configs), tls))
    }

    /// The three things `connect_and_serve` needs: the session's configuration,
    /// where to dial, and how to come back.
    ///
    /// **The address is text, not a resolved `SocketAddr`.**
    /// `TcpStream::connect` takes the text and looks it up on every dial, so a
    /// venue whose DNS fails over keeps working; resolving here would pin the
    /// engine to one address chosen at startup, and would put a nameserver in
    /// the path of reading a file.
    ///
    /// # Errors
    ///
    /// [`Problem::WrongRole`] if the file describes an acceptor. Everything
    /// else — a missing host, a port that is not a number, bounds that describe
    /// no ladder — was already refused by [`Self::parse`], with its line.
    pub fn into_initiator(
        self,
    ) -> Result<(Config, String, crate::reconnect::Policy), SettingsError> {
        let Some((addr, policy)) = self.dial else {
            return Err(SettingsError::at(
                self.role_line,
                Problem::WrongRole,
                "this file configures an acceptor: call into_table()",
            ));
        };
        let Some(cfg) = self.configs.first().copied() else {
            // Unreachable by construction: `parse` refuses a file with no
            // `[SESSION]`. Answered rather than `unwrap`ped — non-negotiable 7
            // has no exceptions.
            return Err(SettingsError::at(
                0,
                Problem::NoSessions,
                "add a [SESSION] naming the venue",
            ));
        };
        Ok((cfg, addr, policy))
    }
}

/// The registry both acceptor doors build, so the two cannot drift.
fn table_of(configs: Vec<Config>) -> Table {
    let mut table = Table::with_capacity(configs.len());
    for cfg in configs {
        table = table.serving(cfg);
    }
    table
}

/// A line to blame a whole block for: its `TargetCompID`, which is the setting
/// that distinguishes one `[SESSION]` from another.
fn line_of(block: Block<'_>) -> usize {
    block.target.map_or(0, |(line, _)| line)
}

/// One merged block becomes one [`Config`].
fn build(block: Block<'_>) -> Result<Config, SettingsError> {
    let at = line_of(block);
    let begin = required(block.begin_string, Key::BeginString, at)?;
    let sender = required(block.sender, Key::SenderCompId, at)?;
    let target = required(block.target, Key::TargetCompId, at)?;

    let mut cfg = Config::acceptor(
        fitting(begin, MAX_BEGIN_STRING_LEN, Key::BeginString)?,
        fitting(sender, MAX_COMP_ID_LEN, Key::SenderCompId)?,
        fitting(target, MAX_COMP_ID_LEN, Key::TargetCompId)?,
    );
    if let Some(v) = block.heart_bt_int {
        cfg = cfg.with_heart_bt_int(number(v, Key::HeartBtInt)?);
    }
    if let Some(v) = block.max_skew {
        cfg = cfg.with_max_skew_ms(number(v, Key::MaxSkewMillis)?);
    }
    if let Some(schedule) = schedule(block)? {
        cfg = cfg.with_schedule(schedule);
    }

    let mut reset = fixbolt_session::ResetPolicy::new();
    if let Some(v) = block.reset_on_logon
        && flag(v, Key::ResetOnLogon)?
    {
        reset = reset.on_logon();
    }
    if let Some(v) = block.reset_on_logout
        && flag(v, Key::ResetOnLogout)?
    {
        reset = reset.on_logout();
    }
    if let Some(v) = block.reset_on_disconnect
        && flag(v, Key::ResetOnDisconnect)?
    {
        reset = reset.on_disconnect();
    }
    cfg = cfg.with_reset(reset);

    // Seconds in the file, milliseconds in the session. QuickFIX writes seconds
    // and every operator who will read this file has written those.
    if let Some(v) = block.logon_timeout {
        let secs: u64 = number(v, Key::LogonTimeout)?;
        cfg = cfg.with_logon_timeout_ms(secs.saturating_mul(1_000));
    }
    if let Some(v) = block.logout_timeout {
        let secs: u64 = number(v, Key::LogoutTimeout)?;
        cfg = cfg.with_logout_timeout_ms(secs.saturating_mul(1_000));
    }

    let mut checks = fixbolt_session::DictionaryChecks::new();
    if let Some(v) = block.allow_unknown_msg_fields
        && flag(v, Key::AllowUnknownMsgFields)?
    {
        checks = checks.allowing_unknown_msg_fields();
    }
    // **The one key whose `Y` means *do the work*.** Every other flag here is
    // off by default and `Y` turns something on; this one is on by default and
    // `N` turns it off, because that is the name QuickFIX gave it. Getting it
    // backwards would silently accept every tag above 5000 on a desk that wrote
    // the safer value.
    if let Some(v) = block.validate_user_defined_fields
        && !flag(v, Key::ValidateUserDefinedFields)?
    {
        checks = checks.skipping_user_defined_fields();
    }
    cfg = cfg.with_validation(checks);

    // The two sequence-resync fields. Both **off** unless the file says
    // otherwise, which is what every engine surveyed does: a counterparty that
    // does not expect an optional field can answer a `Reject`.
    if let Some(v) = block.send_next_expected {
        cfg = cfg.with_next_expected(flag(v, Key::SendNextExpectedMsgSeqNum)?);
    }
    if let Some(v) = block.enable_last_processed {
        cfg = cfg.with_last_processed(flag(v, Key::EnableLastMsgSeqNumProcessed)?);
    }
    if let Some(v) = block.timestamp_precision {
        let digits: u32 = number(v, Key::TimestampPrecision)?;
        // **Refused, not rounded.** QuickFIX C++ takes 0-9 here; this engine
        // writes 3, 6 or 9, and the six widths in between would have to become
        // some other width to go out at all. A configuration error names the
        // line; a silent clamp names nothing. ADR-0057 open question 3.
        //
        // **Spelled exactly, not merely parsed.** `value.parse::<u32>()`
        // above reads `+3` and `03` as `3`, which is looser than the
        // spelling this key documents — probe 3 in `mod doc_table` found the
        // gap on 2026-09-12. `Problem::UnsupportedPrecision` is what a wrong
        // width already answers with, and a wrong spelling of a right width
        // is refused the same way, not rounded to it.
        if !spelled_exactly_as_digits(v.1) {
            return Err(SettingsError::at(
                v.0,
                Problem::UnsupportedPrecision,
                format!("{}={}", Key::TimestampPrecision.name(), v.1),
            ));
        }
        let precision =
            fixbolt_codec::Precision::from_fractional_digits(digits).ok_or_else(|| {
                SettingsError::at(
                    v.0,
                    Problem::UnsupportedPrecision,
                    format!("{}={}", Key::TimestampPrecision.name(), v.1),
                )
            })?;
        cfg = cfg.with_timestamp_precision(precision);
    }

    Ok(cfg)
}

/// How many times the first backoff delay the ceiling is, when a file gives an
/// interval and no ceiling.
///
/// `ReconnectCeiling` has no QuickFIX equivalent: without a ceiling the ladder
/// doubles forever. Sixteen, because it turns the common `ReconnectInterval=30`
/// into eight minutes rather than into days.
const DEFAULT_CEILING_MULTIPLE: u64 = 16;

/// Where an initiator dials, and how it comes back.
fn dialling(block: Block<'_>) -> Result<(String, crate::reconnect::Policy), SettingsError> {
    let at = line_of(block);
    let host = required(block.host, Key::SocketConnectHost, at)?;
    let port = required(block.port, Key::SocketConnectPort, at)?;
    // Parsed only far enough to know it is a port. **The host is not resolved
    // here**: `TcpStream::connect` takes the text and looks it up on every
    // dial, so a venue whose DNS fails over keeps working — and a parser that
    // does name lookups is a parser that blocks on a nameserver.
    let port: u16 = number(port, Key::SocketConnectPort)?;
    let addr = format!("{}:{port}", host.1);

    let first_s: u64 = match block.reconnect_interval {
        Some(v) => number(v, Key::ReconnectInterval)?,
        None => DEFAULT_RECONNECT_INTERVAL_SECS,
    };
    let ceiling_s: u64 = match block.reconnect_ceiling {
        Some(v) => number(v, Key::ReconnectCeiling)?,
        None => first_s.saturating_mul(DEFAULT_CEILING_MULTIPLE),
    };
    let blame = block
        .reconnect_ceiling
        .or(block.reconnect_interval)
        .map_or(at, |(line, _)| line);
    let policy = crate::reconnect::Policy::new(
        first_s.saturating_mul(1_000),
        ceiling_s.saturating_mul(1_000),
    )
    .map_err(|e| {
        SettingsError::at(
            blame,
            Problem::ImpossiblePolicy,
            format!("ReconnectInterval={first_s}, ReconnectCeiling={ceiling_s}: {e:?}"),
        )
    })?;
    Ok((addr, policy))
}

/// Seconds since midnight, from `HH:MM:SS`.
///
/// Strict on shape as well as on range: `8:00:00` and `08:00` are refused
/// rather than guessed at, because a file that is nearly right in two places is
/// how a session ends up open at the wrong hour.
fn time_of_day((line, value): (usize, &str), key: Key) -> Result<u32, SettingsError> {
    let bad = || SettingsError::at(line, Problem::BadTime, format!("{}={value}", key.name()));
    let mut parts = value.split(':');
    let (Some(h), Some(m), Some(s), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return Err(bad());
    };
    if h.len() != 2 || m.len() != 2 || s.len() != 2 {
        return Err(bad());
    }
    let (Ok(h), Ok(m), Ok(s)) = (h.parse::<u32>(), m.parse::<u32>(), s.parse::<u32>()) else {
        return Err(bad());
    };
    if h > 23 || m > 59 || s > 59 {
        return Err(bad());
    }
    Ok(h * 3_600 + m * 60 + s)
}

/// One weekday, by full name or by the usual three letters.
///
/// Case-sensitive, and the error says what was expected. Accepting `monday`
/// too would mean accepting `MONDAY` and `Monday ` next, and each of those is a
/// file that reads as correct to a person and differently to the parser.
fn weekday(name: &str) -> Option<Weekday> {
    match name {
        "Monday" | "Mon" => Some(Weekday::Monday),
        "Tuesday" | "Tue" => Some(Weekday::Tuesday),
        "Wednesday" | "Wed" => Some(Weekday::Wednesday),
        "Thursday" | "Thu" => Some(Weekday::Thursday),
        "Friday" | "Fri" => Some(Weekday::Friday),
        "Saturday" | "Sat" => Some(Weekday::Saturday),
        "Sunday" | "Sun" => Some(Weekday::Sunday),
        _ => None,
    }
}

/// One day name, or a [`Problem::BadWeekday`] quoting what was written.
fn one_day((line, value): (usize, &str), key: Key) -> Result<Weekday, SettingsError> {
    weekday(value).ok_or_else(|| {
        SettingsError::at(line, Problem::BadWeekday, format!("{}={value}", key.name()))
    })
}

/// The schedule a block describes, or [`None`] for one that says nothing about
/// hours — which means [`Schedule::always`] and is the default the 59
/// acceptance definitions run under.
///
/// # The four shapes, and why a half-written one is refused
///
/// * nothing → `None`
/// * `StartTime` + `EndTime` → [`Schedule::daily`]
/// * those plus `StartDay` + `EndDay` → [`Schedule::weekly`]
/// * those plus `Weekdays` → [`Schedule::with_weekdays`]
///
/// A block with `StartTime` and no `EndTime` is refused rather than completed
/// with midnight: the writer meant something, and the parser does not know
/// what.
fn schedule(block: Block<'_>) -> Result<Option<Schedule>, SettingsError> {
    let at = line_of(block);
    let (start, end) = match (block.start_time, block.end_time) {
        (None, None) => {
            // A day without an hour describes nothing, and silently ignoring it
            // is exactly the failure `Problem::UnknownKey` exists to prevent.
            for (slot, key) in [
                (block.start_day, Key::StartDay),
                (block.end_day, Key::EndDay),
                (block.weekdays, Key::Weekdays),
            ] {
                if let Some((line, _)) = slot {
                    return Err(SettingsError::at(
                        line,
                        Problem::MissingKey,
                        format!("{} needs StartTime and EndTime", key.name()),
                    ));
                }
            }
            return Ok(None);
        }
        (Some(s), Some(e)) => (
            time_of_day(s, Key::StartTime)?,
            time_of_day(e, Key::EndTime)?,
        ),
        (Some(_), None) => {
            return Err(SettingsError::at(
                at,
                Problem::MissingKey,
                Key::EndTime.name(),
            ));
        }
        (None, Some(_)) => {
            return Err(SettingsError::at(
                at,
                Problem::MissingKey,
                Key::StartTime.name(),
            ));
        }
    };

    let impossible =
        |line: usize, why: &str| SettingsError::at(line, Problem::ImpossibleSchedule, why);

    let built = match (block.start_day, block.end_day) {
        (None, None) => Schedule::daily(start, end)
            .ok_or_else(|| impossible(at, "StartTime and EndTime are the same instant"))?,
        (Some(sd), Some(ed)) => {
            if let Some((line, _)) = block.weekdays {
                return Err(impossible(
                    line,
                    "Weekdays cannot narrow a weekly window — StartDay and EndDay already choose the days",
                ));
            }
            Schedule::weekly(
                one_day(sd, Key::StartDay)?,
                start,
                one_day(ed, Key::EndDay)?,
                end,
            )
            .ok_or_else(|| {
                impossible(
                    at,
                    "StartDay/StartTime and EndDay/EndTime are the same instant",
                )
            })?
        }
        (Some(_), None) => {
            return Err(SettingsError::at(
                at,
                Problem::MissingKey,
                Key::EndDay.name(),
            ));
        }
        (None, Some(_)) => {
            return Err(SettingsError::at(
                at,
                Problem::MissingKey,
                Key::StartDay.name(),
            ));
        }
    };

    let Some((line, list)) = block.weekdays else {
        return Ok(Some(built));
    };
    let mut days = Weekdays::NONE;
    for name in list.split(',') {
        days = days.and(one_day((line, name.trim()), Key::Weekdays)?);
    }
    built
        .with_weekdays(days)
        .map(Some)
        .ok_or_else(|| impossible(line, "Weekdays is empty"))
}

/// `docs/CONFIGURATION.md` §1 and the [`Key`] enum, checked against each other.
///
/// `[measured 2026-09-12]` nothing read that document. The 26 keys were
/// exhaustive in one direction only — the compiler forces every `match self`
/// over [`Key`] to cover every variant — and the documentation direction had
/// never been checked at all, which is how `docs/CONFIGURATION.md:21` came to
/// say *"Twenty-three keys"* above a table of 26 rows for a week.
///
/// **Until 2026-09-12 only the first cell of each row was checked**, which is
/// a much narrower promise than the module read as: *does a row exist whose
/// key cell names this key, and does every key cell name a key?* Everything
/// else in the row was prose to this gate, so a row could say the exact
/// opposite of the code and stay green. That is not hypothetical —
/// `[measured 2026-09-12]` the `TlsRequireKernel` notes cell was written
/// *"does nothing without `SocketUseSSL=Y`"* while `settle` **refuses** it,
/// and the senior review of PR #63 then falsified the meaning, the values and
/// the default of two rows at once, changed the count sentence to *"Four
/// hundred keys"*, and the suite still read `3 passed; 0 failed`.
///
/// # The five questions these tests answer
///
/// 1. **Does every key have a row, and every row a key?** The first cell.
/// 2. **Is the count sentence true?** `**<number in words> keys** are
///    recognised`, against the number of arms in [`Key::name`].
/// 3. **Is a written default really the default?** A *Default* cell that
///    begins with a backticked literal is a declaration: the minimal sample
///    for that key's group is parsed without the key, and again with
///    `KEY=<the documented default>` at the end of its `[DEFAULT]`, and the
///    two must agree on the configurations, the role, where it dials, the
///    certificate and the log path.
/// 4. **Are the listed values the values the parser takes?** A *Values* cell
///    that is two or more backticked literals joined by nothing but `or` and
///    commas is an enumeration: every literal in it must survive the parser —
///    a *context* error such as [`Problem::MissingKey`] is allowed, a *value*
///    error such as [`Problem::NotAFlag`] is not — and a literal the cell does
///    **not** list must be refused as a bad value.
/// 5. **Is a `[DEFAULT]`-only claim true?** A *Where* cell saying `[DEFAULT]`
///    **only** must give [`Problem::DefaultOnly`] when the key is written into
///    a `[SESSION]`, and one saying `[DEFAULT]` or `[SESSION]` must not.
///
/// # What they do not answer, and it is most of the table
///
/// **The *Meaning* cell, every note cell and every paragraph between the
/// tables are prose, and prose is a hand-check.** There is no machine reading
/// for *"does nothing without `SocketUseSSL=Y`"* that does not first make the
/// author write the sentence in a language a machine reads, at which point the
/// document has stopped being one. A *Default* cell written as prose
/// (`required`, `none`, `16 × ...`) and a *Values* cell that is not an
/// enumeration (`ASCII, max 32 bytes`) are outside these probes on purpose —
/// **they are counted as skipped, so the price of writing a cell in prose is
/// visible rather than silent**, and each probe carries a floor on how many
/// rows it reached that may only be raised. `docs/CONFIGURATION.md` §1 states
/// that boundary for the reader of the document; this is the same boundary
/// stated for the reader of the code.
#[cfg(test)]
mod doc_table {
    use super::{Key, Problem, Settings};

    /// This crate's own source, so the key list comes from the compiler rather
    /// than from a second list that can drift. Resolved relative to this file.
    const SRC: &str = include_str!("settings.rs");

    /// `CARGO_MANIFEST_DIR` is `crates/engine`, so the repository root is two
    /// levels up. Absolute, so it does not depend on which directory the test
    /// binary is run from. Read at **run** time, not `include_str!`, so an
    /// edited document is compared without rebuilding anything.
    const DOC_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/CONFIGURATION.md");

    /// The line that opens `Key::name`, matched whole so that this module's own
    /// mention of it — inside a string literal — cannot be mistaken for it.
    const NAME_FN: &str = "const fn name(self) -> &'static str {";
    /// The same for `Key::parse`, the second witness in [`the_key_scrape_is_not_silently_short`].
    const PARSE_FN: &str = "fn parse(name: &str) -> Option<Self> {";

    /// Leading spaces on `line`.
    fn indent(line: &str) -> usize {
        line.len() - line.trim_start().len()
    }

    /// Every string literal in the arms of the function `opens` opens.
    ///
    /// `Key::name` is a `match self` with no wildcard arm, so a new [`Key`]
    /// variant does not compile until it has an arm there carrying its
    /// spelling. Reading that function back is therefore a key list the
    /// **compiler** keeps complete, which a list written out here would not be.
    fn arm_literals(opens: &str) -> Vec<&'static str> {
        let mut out: Vec<&'static str> = Vec::new();
        let mut open_indent = None;
        for line in SRC.lines() {
            let trimmed = line.trim();
            let Some(fn_indent) = open_indent else {
                if trimmed == opens {
                    open_indent = Some(indent(line));
                }
                continue;
            };
            // The function's own closing brace. The `match`'s brace is nested
            // one level deeper, so it does not end the scan.
            if trimmed == "}" && indent(line) <= fn_indent {
                break;
            }
            // The spelling sits on the right of `=>` in `Key::name` and on the
            // left of it in `Key::parse`, so take the first quoted word on any
            // arm line. A comment is not an arm.
            if trimmed.starts_with("//") || !line.contains("=>") {
                continue;
            }
            let mut quoted = line.split('"');
            let _before = quoted.next();
            if let Some(name) = quoted.next() {
                if !name.is_empty() {
                    out.push(name);
                }
            }
        }
        assert!(
            open_indent.is_some(),
            "settings.rs no longer contains the line `{opens}` — the doc/key gate reads its match arms and is now reading nothing"
        );
        out
    }

    /// One row of a `docs/CONFIGURATION.md` §1 table, with its cells reachable
    /// by the name of the column above them.
    ///
    /// **By name, because §1 has four tables and two column shapes.** Three
    /// carry `| Key | Meaning | Values | Default | Where | Source |`; the ten
    /// `Config` keys sit in one with no *Where* and no *Source*, under a
    /// paragraph that makes the claim in prose instead. Reading the fifth cell
    /// by position would read a *Where* claim off a table that has none.
    struct DocRow<'a> {
        key: &'a str,
        columns: Vec<(&'a str, &'a str)>,
    }

    impl<'a> DocRow<'a> {
        /// The cell under the column called `header`, or [`None`] when this
        /// row's table has no such column — which is a *skip*, never a pass.
        fn cell(&self, header: &str) -> Option<&'a str> {
            self.columns
                .iter()
                .find(|(name, _)| *name == header)
                .map(|(_, value)| *value)
        }
    }

    /// The cells of one Markdown table row, trimmed, without the two empty
    /// strings the leading and trailing `|` produce.
    fn row_cells(line: &str) -> Vec<&str> {
        let line = line.trim();
        let Some(inner) = line.strip_prefix('|') else {
            return Vec::new();
        };
        let inner = inner.strip_suffix('|').unwrap_or(inner);
        inner.split('|').map(str::trim).collect()
    }

    /// A `|---|---|` rule under a header.
    fn is_separator(cells: &[&str]) -> bool {
        !cells.is_empty()
            && cells
                .iter()
                .all(|c| !c.is_empty() && c.chars().all(|ch| ch == '-' || ch == ':'))
    }

    /// The word inside a cell that is **a backticked word and nothing else**,
    /// which is what a key cell is and what no header, rule or prose cell is.
    ///
    /// There is deliberately no character class listing what a key name may
    /// contain — `docs/reference/a-matcher-excluded-the-separator-every-real-name-uses.md`
    /// is this repository's case of exactly that going wrong.
    fn backticked_word(cell: &str) -> Option<&str> {
        let inner = cell.strip_prefix('`')?.strip_suffix('`')?;
        if inner.is_empty() || inner.contains('`') || inner.contains(char::is_whitespace) {
            return None;
        }
        Some(inner)
    }

    /// Every row of `docs/CONFIGURATION.md` §1 that names a key, in order,
    /// each carrying its own table's column names.
    ///
    /// §1 runs from its own `## ` heading to the next one, so the `###`
    /// subsection inside it is included and the `## 2.` tables are not. A
    /// table is read as a settings table only when its header's first column
    /// is `Key`: the two-column table in the `TimestampPrecision` subsection
    /// says how the session is driven, not what a setting takes, and its rows
    /// must not be read as settings.
    fn doc_rows(doc: &str) -> Vec<DocRow<'_>> {
        let mut inside = false;
        let mut seen_section_one = false;
        let mut headers: Vec<&str> = Vec::new();
        let mut rows = Vec::new();
        for line in doc.lines() {
            if line.starts_with("## ") {
                inside = line.starts_with("## 1.");
                seen_section_one |= inside;
                headers.clear();
                continue;
            }
            if !inside {
                continue;
            }
            if !line.starts_with('|') {
                // A table ends at the first line that is not a row, so the
                // next table's rows cannot inherit this one's column names.
                headers.clear();
                continue;
            }
            let cells = row_cells(line);
            if is_separator(&cells) {
                continue;
            }
            let Some(first) = cells.first() else {
                continue;
            };
            let Some(name) = backticked_word(first) else {
                // Not a key row, so it is the header of whatever table starts
                // here — remembered only when it is a settings table.
                headers = if *first == "Key" {
                    cells.clone()
                } else {
                    Vec::new()
                };
                continue;
            };
            assert!(
                !headers.is_empty(),
                "docs/CONFIGURATION.md §1: the row for `{name}` sits under no `| Key | ... |` header, so its cells cannot be read by column"
            );
            assert_eq!(
                cells.len(),
                headers.len(),
                "docs/CONFIGURATION.md §1: the row for `{name}` has {} cells under {} columns",
                cells.len(),
                headers.len()
            );
            rows.push(DocRow {
                key: name,
                columns: headers.iter().copied().zip(cells).collect(),
            });
        }
        assert!(
            seen_section_one,
            "docs/CONFIGURATION.md has no `## 1.` heading — the doc/key gate scopes itself to that section and is now reading nothing"
        );
        rows
    }

    fn configuration_md() -> String {
        let doc = std::fs::read_to_string(DOC_PATH).unwrap_or_default();
        assert!(!doc.is_empty(), "cannot read {DOC_PATH}");
        doc
    }

    /// Both directions, with a distinct sentence each. A check that can only
    /// fail one way is half a gate.
    #[test]
    fn configuration_md_section_1_lists_exactly_the_keys() {
        let doc = configuration_md();
        let rows = doc_rows(&doc);
        for name in arm_literals(NAME_FN) {
            assert!(
                rows.iter().any(|row| row.key == name),
                "docs/CONFIGURATION.md §1: Key has no doc row: `{name}`"
            );
        }
        for row in rows {
            assert!(
                Key::parse(row.key).is_some(),
                "docs/CONFIGURATION.md §1: doc row has no Key: `{}`",
                row.key
            );
        }
    }

    #[test]
    fn every_key_name_parses_back_to_its_key() {
        for name in arm_literals(NAME_FN) {
            let parsed = Key::parse(name);
            assert!(
                parsed.is_some(),
                "Key::parse rejects its own name: `{name}`"
            );
            assert_eq!(
                parsed.map(Key::name),
                Some(name),
                "Key::parse and Key::name disagree about `{name}`"
            );
        }
    }

    /// The scrape's own observable. A text scan that stops matching reports an
    /// empty list, which reads exactly like a healthy tree — the failure mode
    /// of `docs/reference/a-matcher-excluded-the-separator-every-real-name-uses.md`.
    /// Two witnesses answer that: `Key::parse`'s arms, scraped independently,
    /// and a floor that may only ever be raised.
    #[test]
    fn the_key_scrape_is_not_silently_short() {
        /// Keys on 2026-09-12, the four TLS keys included. Raise it when keys
        /// are added; never lower it.
        const FLOOR: usize = 30;

        let mut names = arm_literals(NAME_FN);
        let mut parses = arm_literals(PARSE_FN);
        assert!(
            names.len() >= FLOOR,
            "the Key::name scrape found {} names, below the floor of {FLOOR} — it has stopped matching",
            names.len()
        );
        names.sort_unstable();
        let before = names.len();
        names.dedup();
        assert_eq!(before, names.len(), "Key::name repeats a spelling");
        parses.sort_unstable();
        parses.dedup();
        assert_eq!(
            names, parses,
            "Key::name and Key::parse do not spell the same set of keys"
        );
    }

    // ------------------------------------------------------------------
    // The four probes, `[added 2026-09-12]`. Everything above reads the key
    // cell; everything below reads the cells that are already **values** —
    // a default, a list of literals, a `[DEFAULT]`-only claim — and asks the
    // parser whether they are true. The rest of the table stays prose, and
    // the skip counts below are what make that visible.
    // ------------------------------------------------------------------

    /// A minimal configuration file that parses, split where a probe writes.
    ///
    /// Split rather than one string because *where* the line goes is the
    /// question in two of the probes: a documented default is written at the
    /// end of the `[DEFAULT]` block, and a `[DEFAULT]`-only claim is tested by
    /// writing the key into the `[SESSION]`.
    #[derive(Clone, Copy)]
    struct Sample {
        /// The `[DEFAULT]` block, ending in a newline.
        default_block: &'static str,
        /// Everything after it — one `[SESSION]`, ending in a newline.
        sessions: &'static str,
    }

    impl Sample {
        fn text(self) -> String {
            format!("{}{}", self.default_block, self.sessions)
        }

        /// The sample with one more line at the end of `[DEFAULT]`.
        fn with_in_default(self, line: &str) -> String {
            format!("{}{line}\n{}", self.default_block, self.sessions)
        }

        /// The sample with one more line at the end of the `[SESSION]`.
        fn with_in_session(self, line: &str) -> String {
            format!("{}{}{line}\n", self.default_block, self.sessions)
        }
    }

    /// The file every acceptor key is probed in. **It carries no
    /// `ConnectionType` and no `SocketUseSSL`**, because a sample that already
    /// names the key under probe answers [`Problem::RepeatedKey`] — which is
    /// not a value error, and would make a probe green for the wrong reason.
    const ACCEPTOR: Sample = Sample {
        default_block: "[DEFAULT]\nBeginString=FIX.4.4\nSenderCompID=ISLD\n",
        sessions: "\n[SESSION]\nTargetCompID=TW44\n",
    };

    /// The file the dialling keys are probed in. `ReconnectInterval` and
    /// `ReconnectCeiling` are absent for the same reason.
    const INITIATOR: Sample = Sample {
        default_block: "[DEFAULT]\nBeginString=FIX.4.4\nSenderCompID=ISLD\nConnectionType=initiator\nSocketConnectHost=127.0.0.1\nSocketConnectPort=9876\n",
        sessions: "\n[SESSION]\nTargetCompID=TW44\n",
    };

    /// The file `TlsRequireKernel` is probed in, and **the reason it needs one
    /// of its own**: `settle` refuses that key when `SocketUseSSL` is not `Y`,
    /// *before* it ever reads its value, so in the `ACCEPTOR` sample a foreign
    /// literal would come back [`Problem::MissingKey`] instead of
    /// [`Problem::NotAFlag`] and probe 3 would pass on a parser that never
    /// looked. The paths are never opened — `settle` builds a `PathBuf` and
    /// reading the PEM happens at start-up, beside `serve_tls*`.
    #[cfg(feature = "tls")]
    const TLS: Sample = Sample {
        default_block: "[DEFAULT]\nBeginString=FIX.4.4\nSenderCompID=ISLD\nSocketUseSSL=Y\nServerCertificateFile=/nonexistent/cert.pem\nServerCertificateKeyFile=/nonexistent/key.pem\n",
        sessions: "\n[SESSION]\nTargetCompID=TW44\n",
    };

    /// Which minimal file a key can be written into.
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Group {
        Acceptor,
        Initiator,
        Tls,
    }

    /// The sample a key is probed in.
    ///
    /// **Exhaustive over [`Key`] with no `_` arm**, like every other `match`
    /// in this module: a new setting has to say which file it can be written
    /// into before it compiles, so "skip it" is never something a new key
    /// falls into by default.
    const fn group(key: Key) -> Group {
        match key {
            Key::BeginString
            | Key::SenderCompId
            | Key::TargetCompId
            | Key::HeartBtInt
            | Key::MaxSkewMillis
            | Key::StartTime
            | Key::EndTime
            | Key::StartDay
            | Key::EndDay
            | Key::Weekdays
            | Key::FileLogPath
            | Key::ConnectionType
            | Key::ResetOnLogon
            | Key::ResetOnLogout
            | Key::ResetOnDisconnect
            | Key::LogonTimeout
            | Key::LogoutTimeout
            | Key::AllowUnknownMsgFields
            | Key::ValidateUserDefinedFields
            | Key::SendNextExpectedMsgSeqNum
            | Key::EnableLastMsgSeqNumProcessed
            | Key::TimestampPrecision
            // `SocketUseSSL=N` and a foreign literal are both answered by the
            // plain acceptor file, in either feature set — `settle` reads the
            // flag before it asks whether this build has `rustls` in it.
            | Key::SocketUseSsl => Group::Acceptor,
            Key::SocketConnectHost
            | Key::SocketConnectPort
            | Key::ReconnectInterval
            | Key::ReconnectCeiling => Group::Initiator,
            Key::ServerCertificateFile | Key::ServerCertificateKeyFile | Key::TlsRequireKernel => {
                Group::Tls
            }
        }
    }

    /// The sample for a group, or [`None`] when this build cannot write one —
    /// the TLS file needs `SocketUseSSL=Y`, which a build without the `tls`
    /// feature refuses by design (non-negotiable 6). A key with no sample is a
    /// **skip, counted**, never a pass.
    fn sample(group: Group) -> Option<Sample> {
        match group {
            Group::Acceptor => Some(ACCEPTOR),
            Group::Initiator => Some(INITIATOR),
            #[cfg(feature = "tls")]
            Group::Tls => Some(TLS),
            #[cfg(not(feature = "tls"))]
            Group::Tls => None,
        }
    }

    /// `n` written the way §1's count sentence writes it, for 1–99.
    fn english(n: usize) -> Option<String> {
        const UNITS: [&str; 20] = [
            "zero",
            "one",
            "two",
            "three",
            "four",
            "five",
            "six",
            "seven",
            "eight",
            "nine",
            "ten",
            "eleven",
            "twelve",
            "thirteen",
            "fourteen",
            "fifteen",
            "sixteen",
            "seventeen",
            "eighteen",
            "nineteen",
        ];
        const TENS: [&str; 10] = [
            "", "", "twenty", "thirty", "forty", "fifty", "sixty", "seventy", "eighty", "ninety",
        ];
        if n < 20 {
            return UNITS.get(n).map(|word| (*word).to_string());
        }
        if n > 99 {
            return None;
        }
        let ten = TENS.get(n / 10)?;
        match n % 10 {
            0 => Some((*ten).to_string()),
            unit => UNITS.get(unit).map(|word| format!("{ten}-{word}")),
        }
    }

    /// Only the lines inside `## 1.`, the same scoping [`doc_rows`] uses — so
    /// a probe that claims to read that section cannot be satisfied by a
    /// sentence sitting outside it.
    fn section_one(doc: &str) -> impl Iterator<Item = &str> {
        let mut inside = false;
        doc.lines().filter(move |line| {
            if line.starts_with("## ") {
                inside = line.starts_with("## 1.");
                false
            } else {
                inside
            }
        })
    }

    /// **Probe 1.** The sentence above the table, inside `## 1.`, counts the
    /// keys.
    ///
    /// `[measured 2026-09-12]` the senior review of PR #63 rewrote it to *Four
    /// hundred keys* and nothing went red; before that it said *Twenty-three*
    /// over 26 rows for a week. `[measured 2026-09-12]` a decoy count sentence
    /// placed before `## 1.` begins was enough to satisfy an earlier version
    /// of this probe, which scanned the whole document rather than the
    /// section it claims to read — [`section_one`] is the fix.
    #[test]
    fn the_count_sentence_counts_the_keys() {
        let doc = configuration_md();
        let n = arm_literals(NAME_FN).len();
        let written = section_one(&doc).find_map(|line| {
            line.strip_prefix("**")
                .and_then(|rest| rest.split_once(" keys** are recognised"))
                .map(|(word, _)| word)
        });
        assert!(
            written.is_some(),
            "docs/CONFIGURATION.md §1 no longer has a `**<number in words> keys** are recognised` sentence — the count gate is reading nothing"
        );
        let expected = english(n);
        assert!(
            expected.is_some(),
            "Key has {n} arms, which this test cannot write in words — english() covers 1-99"
        );
        if let (Some(written), Some(expected)) = (written, expected) {
            assert_eq!(
                written.to_ascii_lowercase(),
                expected,
                "docs/CONFIGURATION.md §1: the count sentence says `{written} keys` but Key has {n} arms, which is `{expected}`"
            );
        }
    }

    /// The literal a *Default* cell **begins** with, which is how a cell
    /// declares a default rather than describing one. `` `120000` (2 minutes)
    /// `` declares `120000`; `required`, `none` and ``16 × `ReconnectInterval` ``
    /// declare nothing this can check.
    fn leading_literal(cell: &str) -> Option<&str> {
        let (literal, _) = cell.strip_prefix('`')?.split_once('`')?;
        if literal.is_empty() {
            None
        } else {
            Some(literal)
        }
    }

    /// **Probe 2.** Writing a documented default changes nothing.
    ///
    /// The two parses are compared on what a configuration *is* — the
    /// configurations, the role, where it dials, the certificate, the log
    /// path — and **not** on `role_line` or `tls_line`, which are positions in
    /// a file and differ by construction as soon as a line is added.
    ///
    /// This is the probe that reads the **code** rather than the document:
    /// moving `DEFAULT_RECONNECT_INTERVAL_SECS` away from 30 turns the
    /// `ReconnectInterval` row red without anybody touching Markdown.
    #[test]
    fn a_documented_default_changes_nothing() {
        /// Rows reached on 2026-09-12: 15, and 16 with the `tls` feature.
        /// Raise it when a prose *Default* cell becomes a literal one; never
        /// lower it.
        const FLOOR: usize = if cfg!(feature = "tls") { 16 } else { 15 };

        let doc = configuration_md();
        let (mut probed, mut skipped) = (0_usize, 0_usize);
        for row in doc_rows(&doc) {
            let name = row.key;
            let Some(key) = Key::parse(name) else {
                continue; // named by configuration_md_section_1_lists_exactly_the_keys
            };
            // A *Default* cell written as prose is outside this probe, and the
            // count is the price of writing one.
            let Some(default) = row.cell("Default").and_then(leading_literal) else {
                skipped += 1;
                continue;
            };
            let Some(sample) = sample(group(key)) else {
                skipped += 1;
                continue;
            };
            probed += 1;

            let silent = Settings::parse(&sample.text());
            let written = Settings::parse(&sample.with_in_default(&format!("{name}={default}")));
            assert!(
                silent.is_ok(),
                "the sample this probe writes {name} into does not parse on its own: {:?}",
                silent.as_ref().err()
            );
            assert!(
                written.is_ok(),
                "docs/CONFIGURATION.md §1: {name} documents default `{default}` but writing {name}={default} changes the parsed settings: it is refused — {:?}",
                written.as_ref().err()
            );
            if let (Ok(silent), Ok(written)) = (silent, written) {
                assert_eq!(
                    silent.configs, written.configs,
                    "docs/CONFIGURATION.md §1: {name} documents default `{default}` but writing {name}={default} changes the parsed settings: the session configurations"
                );
                assert_eq!(
                    silent.role, written.role,
                    "docs/CONFIGURATION.md §1: {name} documents default `{default}` but writing {name}={default} changes the parsed settings: the role"
                );
                assert_eq!(
                    format!("{:?}", silent.dial),
                    format!("{:?}", written.dial),
                    "docs/CONFIGURATION.md §1: {name} documents default `{default}` but writing {name}={default} changes the parsed settings: where it dials"
                );
                assert_eq!(
                    silent.tls, written.tls,
                    "docs/CONFIGURATION.md §1: {name} documents default `{default}` but writing {name}={default} changes the parsed settings: the certificate"
                );
                assert_eq!(
                    silent.log, written.log,
                    "docs/CONFIGURATION.md §1: {name} documents default `{default}` but writing {name}={default} changes the parsed settings: the log path"
                );
            }
        }
        println!("probe 2 — Default cells: {probed} probed, {skipped} skipped");
        assert!(
            probed >= FLOOR,
            "probe 2 reached {probed} rows, below its floor of {FLOOR} — either a documented default was rewritten as prose, or this probe has stopped matching"
        );
    }

    /// The literals of a *Values* cell that is **nothing but an enumeration**:
    /// two or more backticked literals with nothing between them but `or`,
    /// commas and spaces.
    ///
    /// **Two or more, and the single-literal case is deliberately out.** A
    /// lone literal in a *Values* cell is a shape rather than a value —
    /// `` `HH:MM:SS` `` is what `StartTime` looks like, not something anybody
    /// may write — and writing it out would answer [`Problem::BadTime`] from a
    /// parser that is behaving exactly as documented. An enumeration has
    /// something to enumerate.
    fn enumerated(cell: &str) -> Option<Vec<&str>> {
        let literals: Vec<&str> = cell.split('`').skip(1).step_by(2).collect();
        if literals.len() < 2 {
            return None;
        }
        let joined_by_or_alone = cell
            .split('`')
            .step_by(2)
            .flat_map(|outside| outside.split(|c: char| c.is_whitespace() || c == ','))
            .filter(|word| !word.is_empty())
            .all(|word| word == "or");
        joined_by_or_alone.then_some(literals)
    }

    /// A refusal about the **value** that was written, as against one about
    /// the context it was written in.
    ///
    /// The split is the whole point of probe 3: `ConnectionType=initiator` in
    /// a file with no `SocketConnectHost` is [`Problem::MissingKey`], which
    /// says nothing about whether `initiator` is a value this parser knows.
    /// Only this finite set says *that value is not one of mine*.
    const fn is_about_the_value(problem: &Problem) -> bool {
        matches!(
            problem,
            Problem::NotAFlag
                | Problem::NotANumber
                | Problem::BadConnectionType
                | Problem::UnsupportedPrecision
                | Problem::BadTime
                | Problem::BadWeekday
                | Problem::ValueTooLong
        )
    }

    /// The 66-character alphabet the short leg of probe 3's universe is drawn
    /// from: every digit, every letter in both cases, and the four
    /// punctuation marks a bare value may legally carry (`+ - . _`). No
    /// whitespace, `=`, `#`, `;`, `[` or `]`: the INI reader trims a value and
    /// gives those other characters meaning only at the start of a line, so
    /// leaving them out keeps the universe to *values*, never *syntax* — the
    /// way this probe avoids a spurious red, by design rather than luck.
    const ALPHABET: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz+-._";

    /// Nine neighbours of one listed literal, which is where a real false
    /// accept already found lives (`03`, `+3`, `Acceptor`): lower- and
    /// upper-cased whole, the first character alone case-swapped, a leading
    /// or trailing `0`, a leading `+`, an extra `x` glued on either side, and
    /// the literal doubled.
    fn neighbours(literal: &str) -> Vec<String> {
        let mut chars = literal.chars();
        let first = chars.next();
        let rest: String = chars.collect();
        let mut out = vec![literal.to_lowercase(), literal.to_uppercase()];
        if let Some(first) = first {
            let swapped: String = if first.is_lowercase() {
                first.to_uppercase().collect()
            } else {
                first.to_lowercase().collect()
            };
            out.push(format!("{swapped}{rest}"));
        }
        out.push(format!("0{literal}"));
        out.push(format!("+{literal}"));
        out.push(format!("{literal}0"));
        out.push(format!("{literal}x"));
        out.push(format!("x{literal}"));
        out.push(format!("{literal}{literal}"));
        out
    }

    /// The bounded universe probe 3's reverse direction searches for a
    /// *Values* cell listing `listed`: every string of length 1 and 2 over
    /// [`ALPHABET`] (66 + 66² = 4 422 strings) plus [`neighbours`] of each
    /// listed literal, minus `listed` itself, deduplicated.
    fn candidates(listed: &[&str]) -> Vec<String> {
        let mut set: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        for &a in ALPHABET {
            set.insert((a as char).to_string());
        }
        for &a in ALPHABET {
            for &b in ALPHABET {
                set.insert(format!("{}{}", a as char, b as char));
            }
        }
        for &literal in listed {
            for neighbour in neighbours(literal) {
                set.insert(neighbour);
            }
        }
        for &literal in listed {
            set.remove(literal);
        }
        set.into_iter().collect()
    }

    /// **Probe 3.** A *Values* cell that lists literals is checked in two
    /// directions, and the two are not the same strength.
    ///
    /// **Forward, exhaustive over what is listed:** every literal the cell
    /// writes must be a value the parser accepts for that key.
    ///
    /// **Reverse, a bounded search, not a sample.** [`candidates`] builds the
    /// universe described above it. Every candidate not already in `listed`
    /// gets one of three outcomes: refused as a bad value
    /// ([`is_about_the_value`]) is correct and silent; accepted is gathered
    /// across the whole row and asserted once, naming every such value, not
    /// just the first; refused for some other reason means the universe
    /// itself wrote something with syntax meaning and is a bug in the probe,
    /// not a finding about the document — reported immediately, naming the
    /// candidate. A zero-guard in the style of `scripts/check-indexing-debt.sh`
    /// fails a row whose universe came out under 4 400 candidates, which
    /// means the universe was not built rather than the row having nothing
    /// left to try.
    ///
    /// `[measured 2026-09-12]` the senior review of PR #63 rewrote
    /// `SocketUseSSL`'s cell to `` `1` or `0` `` and the suite stayed green;
    /// the forward direction now refuses both literals and would catch it.
    /// `[measured 2026-09-12]` `TimestampPrecision`'s cell narrowed from
    /// `` `3`, `6` or `9` `` to `` `3` or `6` ``: the parser still accepted
    /// `9`, the cell no longer listed it, and the suite stayed green — the
    /// old reverse direction sampled three fixed candidates (`1`, `true`,
    /// `nope`) and none of them was `9`. The bounded search above is the
    /// fix: `9` sits in the base universe by construction (every length-1
    /// string over [`ALPHABET`]), so narrowing the cell now goes red.
    #[test]
    fn an_enumerated_values_cell_is_what_the_parser_accepts() {
        /// Rows reached on 2026-09-12: 10, and 11 with the `tls` feature.
        /// Never lower it.
        const FLOOR: usize = if cfg!(feature = "tls") { 11 } else { 10 };
        /// Below this many candidates for one row, the universe was not
        /// built — the zero-guard `candidates` owes every row.
        const MIN_UNIVERSE: usize = 4400;

        let doc = configuration_md();
        let (mut probed, mut skipped) = (0_usize, 0_usize);
        for row in doc_rows(&doc) {
            let name = row.key;
            let Some(key) = Key::parse(name) else {
                continue;
            };
            let Some(listed) = row.cell("Values").and_then(enumerated) else {
                skipped += 1;
                continue;
            };
            let Some(sample) = sample(group(key)) else {
                skipped += 1;
                continue;
            };
            probed += 1;

            for literal in &listed {
                let refusal =
                    Settings::parse(&sample.with_in_default(&format!("{name}={literal}")))
                        .err()
                        .map(|e| e.problem().clone());
                let about_the_value = refusal.as_ref().is_some_and(is_about_the_value);
                assert!(
                    !about_the_value,
                    "docs/CONFIGURATION.md §1: doc lists `{literal}` as a value of {name} but the parser refuses it: {refusal:?}"
                );
            }

            // The other direction: a bounded search, not a sample of three.
            let universe = candidates(&listed);
            assert!(
                universe.len() >= MIN_UNIVERSE,
                "probe 3 tried {} candidates for {name}, below {MIN_UNIVERSE} — the universe was not built",
                universe.len()
            );
            let mut accepted: Vec<String> = Vec::new();
            for candidate in &universe {
                let refusal =
                    Settings::parse(&sample.with_in_default(&format!("{name}={candidate}")))
                        .err()
                        .map(|e| e.problem().clone());
                match refusal {
                    None => accepted.push(candidate.clone()),
                    Some(problem) => {
                        let about_the_value = is_about_the_value(&problem);
                        assert!(
                            about_the_value,
                            "probe 3 wrote {candidate:?} for {name} and the parser refused it for a reason that is not about the value ({problem:?}) — the candidate universe leaked a syntax character; fix the probe, not the document"
                        );
                    }
                }
            }
            assert!(
                accepted.is_empty(),
                "docs/CONFIGURATION.md §1: {name} lists {listed:?} but the parser also accepts {accepted:?} — either the document is short or the parser is lax"
            );
        }
        println!("probe 3 — enumerated Values cells: {probed} probed, {skipped} skipped");
        assert!(
            probed >= FLOOR,
            "probe 3 reached {probed} rows, below its floor of {FLOOR} — either a list of values was rewritten as prose, or this probe has stopped matching"
        );
    }

    /// A *Where* cell that claims the key is `[DEFAULT]`-only.
    fn says_default_only(cell: &str) -> bool {
        cell.contains("`[DEFAULT]` **only**") || cell.contains("`[DEFAULT]` only")
    }

    /// A *Where* cell that claims a `[SESSION]` may carry the key.
    fn says_a_session_may_carry_it(cell: &str) -> bool {
        cell.contains("`[DEFAULT]` or `[SESSION]`") || cell.contains("`[SESSION]` (or `[DEFAULT]`)")
    }

    /// **Probe 4.** A `[DEFAULT]`-only claim is refused in a `[SESSION]`, and
    /// a `[SESSION]` claim is not.
    ///
    /// This is the **sibling of the mistake that was actually made**: the
    /// `TlsRequireKernel` notes cell said *"does nothing without
    /// `SocketUseSSL=Y`"* where the parser refuses it, and no machine can read
    /// that sentence — but the `[DEFAULT]`-only claim in the cell beside it is
    /// a value, and this reads it.
    ///
    /// One sample for every row, because the question is about *sections* and
    /// not about roles: every `[DEFAULT]`-only key is refused at the line it
    /// is read on, before the file's role or this build's features come into
    /// it.
    #[test]
    fn a_where_cell_that_claims_default_only_is_refused_in_a_session() {
        /// Rows reached on 2026-09-12: 18. The ten `Config` keys have no
        /// *Where* column and the two reconnect keys say `initiator only`,
        /// which is a claim about a role and not about a section.
        const FLOOR: usize = 18;

        let doc = configuration_md();
        let (mut probed, mut skipped) = (0_usize, 0_usize);
        for row in doc_rows(&doc) {
            let name = row.key;
            if Key::parse(name).is_none() {
                continue;
            }
            let Some(claim) = row.cell("Where") else {
                skipped += 1;
                continue;
            };
            // Any value: the question is whether the section is allowed, and
            // the section is refused before the value is read.
            let refusal = Settings::parse(&ACCEPTOR.with_in_session(&format!("{name}=Y")))
                .err()
                .map(|e| e.problem().clone());
            let default_only = refusal.as_ref() == Some(&Problem::DefaultOnly);
            if says_default_only(claim) {
                probed += 1;
                assert!(
                    default_only,
                    "docs/CONFIGURATION.md §1: {name} is documented as `[DEFAULT]` only but the parser does not say DefaultOnly in a [SESSION]: {refusal:?}"
                );
            } else if says_a_session_may_carry_it(claim) {
                probed += 1;
                assert!(
                    !default_only,
                    "docs/CONFIGURATION.md §1: {name} is documented as allowed in [SESSION] but the parser says DefaultOnly"
                );
            } else {
                skipped += 1;
            }
        }
        println!("probe 4 — Where cells: {probed} probed, {skipped} skipped");
        assert!(
            probed >= FLOOR,
            "probe 4 reached {probed} rows, below its floor of {FLOOR} — either a section claim was rewritten as prose, or this probe has stopped matching"
        );
    }
}
