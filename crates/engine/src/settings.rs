//! Who this acceptor serves, read from a file.
//!
//! # The gap this closes
//!
//! `[verified 2026-09-02]` the only way to put a counterparty into a
//! [`Table`](crate::presession::Table) was [`Table::serving`], which is Rust and
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
use std::path::Path;

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
        }
    }
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
}

impl fmt::Display for Problem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::Unreadable => "the file could not be read",
            Self::NotAKeyValue => "expected `Key=Value` or a `[SECTION]` header",
            Self::UnknownSection => "unknown section — expected [DEFAULT] or [SESSION]",
            Self::KeyOutsideSection => "a setting before the first [DEFAULT] or [SESSION]",
            Self::UnknownKey => "unknown key",
            Self::RepeatedKey => "the same key twice in one block",
            Self::MissingKey => "a required key is missing",
            Self::ValueTooLong => "the value is longer than a session configuration can hold",
            Self::NotANumber => "expected a number",
            Self::NoSessions => "no [SESSION] block — this acceptor would serve nobody",
            Self::DuplicateSession => "two [SESSION] blocks name the same FIX identity",
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
        }
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
}

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

        let mut configs: Vec<Config> = Vec::with_capacity(sessions.len());
        for block in sessions {
            let cfg = build(block.over(default))?;
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
        Ok(Self { configs })
    }

    /// The configurations, in file order.
    #[must_use]
    pub fn configs(&self) -> &[Config] {
        &self.configs
    }

    /// A registry serving exactly the counterparties this file names.
    #[must_use]
    pub fn into_table(self) -> Table {
        let mut table = Table::with_capacity(self.configs.len());
        for cfg in self.configs {
            table = table.serving(cfg);
        }
        table
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
    Ok(cfg)
}
