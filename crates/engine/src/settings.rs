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
            Self::NoSessions => "no [SESSION] block — this acceptor would serve nobody",
            Self::DuplicateSession => "two [SESSION] blocks name the same FIX identity",
            Self::BadTime => "expected a time of day as HH:MM:SS",
            Self::BadWeekday => "expected a weekday, e.g. Monday or Mon",
            Self::ImpossibleSchedule => "these times and days describe no session",
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
            // Handled before a block ever sees them. A `[SESSION]` carrying one
            // is refused in `parse`, not here, so the error can say why.
            Key::FileLogPath | Key::ConnectionType => {
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
        let mut table = Table::with_capacity(self.configs.len());
        for cfg in self.configs {
            table = table.serving(cfg);
        }
        Ok(table)
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
