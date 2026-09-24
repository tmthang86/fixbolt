//! Every series this crate exports: its name, its type, its labels and its
//! `HELP` text — **the one place they are defined** (ADR-0171 decision 1).
//!
//! A series name is public API. Dashboards and alert rules are written against
//! these strings and no compiler sees them, so a second copy of the list lives
//! in `tests/series_names.rs` and the two must agree; changing one is the
//! moment to add a line to `CHANGELOG.md`.
//!
//! Names follow Prometheus's naming guide (ADR-0171 decision 2): the `fixbolt_`
//! prefix, base units (`_seconds`, `_bytes`), `_total` only on counters, and a
//! boolean is a gauge of 0 or 1. Labels are bounded (decision 3): `engine` is
//! chosen by the caller, `conn` is a `ConnId` of at most `MAX_SESSIONS` live
//! sessions, and `kind` and `reason` come from the fixed sets below — no label
//! value is ever read off the wire.

/// A Prometheus metric type, as the `# TYPE` line spells it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A level that may go up or down.
    Gauge,
    /// A running total that only goes up, until the engine restarts.
    Counter,
}

impl Kind {
    /// The word on the `# TYPE` line.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Gauge => "gauge",
            Self::Counter => "counter",
        }
    }
}

/// Which labels a series carries, and so how many samples it has per engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Scope {
    /// About the exporter itself: no labels, one sample.
    Exporter,
    /// `engine`: one sample per engine.
    Engine,
    /// `engine`, `conn`: one sample per session the snapshot carries.
    Session,
    /// `engine`, `kind`: one sample per [`EVENT_KINDS`] entry.
    EventKind,
    /// `engine`, `reason`: one sample per [`DROP_REASONS`] entry, and `other`.
    Reason,
}

impl Scope {
    const fn labels(self) -> &'static [&'static str] {
        match self {
            Self::Exporter => &[],
            Self::Engine => &["engine"],
            Self::Session => &["engine", "conn"],
            Self::EventKind => &["engine", "kind"],
            Self::Reason => &["engine", "reason"],
        }
    }
}

/// Where a series' value comes from. The encoder matches on this, never on a
/// name, so a rename cannot silently change which number is printed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Source {
    SnapshotAvailable,
    SnapshotAge,
    Published,
    Healthy,
    Connections,
    SessionsLoggedOn,
    Truncated,
    Refused,
    Unframeable,
    SourcesMissing,
    LogLost,
    EventsLost,
    RingUsed,
    RingCapacity,
    PresessionUsed,
    PresessionCapacity,
    SessionLoggedOn,
    SessionNextOut,
    SessionNextIn,
    SessionSkew,
    SessionPendingOutput,
    SessionJournalRefused,
    SessionResendBeyond,
    Events,
    SessionEnds,
    Scrapes,
    BadRequests,
}

/// One series: what a `# HELP` and a `# TYPE` line say about it, and where its
/// samples come from.
#[derive(Debug, Clone, Copy)]
pub struct Series {
    name: &'static str,
    kind: Kind,
    help: &'static str,
    pub(crate) scope: Scope,
    pub(crate) source: Source,
}

impl Series {
    /// The metric name, as scraped.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        self.name
    }

    /// Gauge or counter.
    #[must_use]
    pub const fn kind(&self) -> Kind {
        self.kind
    }

    /// The label names every sample of this series carries, in order.
    #[must_use]
    pub const fn labels(&self) -> &'static [&'static str] {
        self.scope.labels()
    }

    /// The `# HELP` text.
    #[must_use]
    pub const fn help(&self) -> &'static str {
        self.help
    }
}

const fn s(
    name: &'static str,
    kind: Kind,
    scope: Scope,
    source: Source,
    help: &'static str,
) -> Series {
    Series {
        name,
        kind,
        help,
        scope,
        source,
    }
}

use Kind::{Counter, Gauge};

/// Every series, in the order a scrape prints them.
pub const ALL: &[Series] = &[
    s(
        "fixbolt_snapshot_available",
        Gauge,
        Scope::Engine,
        Source::SnapshotAvailable,
        "1 once the engine has published a snapshot; every series read off a snapshot is absent until then",
    ),
    s(
        "fixbolt_snapshot_age_seconds",
        Gauge,
        Scope::Engine,
        Source::SnapshotAge,
        "Seconds since the exporter last saw the engine publish; grows while a standard engine sleeps",
    ),
    s(
        "fixbolt_snapshots_published_total",
        Counter,
        Scope::Engine,
        Source::Published,
        "Snapshots the engine has built; flat while asked means the engine is not turning",
    ),
    s(
        "fixbolt_healthy",
        Gauge,
        Scope::Engine,
        Source::Healthy,
        "1 when Snapshot::healthy(): at least one session, all logged on, no refusal and no missing source",
    ),
    s(
        "fixbolt_connections",
        Gauge,
        Scope::Engine,
        Source::Connections,
        "Connections the engine holds",
    ),
    s(
        "fixbolt_sessions_logged_on",
        Gauge,
        Scope::Engine,
        Source::SessionsLoggedOn,
        "Sessions the snapshot describes that are logged on",
    ),
    s(
        "fixbolt_snapshot_truncated",
        Gauge,
        Scope::Engine,
        Source::Truncated,
        "1 when the engine held more sessions than a snapshot carries (MAX_SESSIONS)",
    ),
    s(
        "fixbolt_refused_connections_total",
        Counter,
        Scope::Engine,
        Source::Refused,
        "Connections ended because the dispatch would not take a message (ADR-0011); zero when healthy",
    ),
    s(
        "fixbolt_unframeable_prelogon_total",
        Counter,
        Scope::Engine,
        Source::Unframeable,
        "Sockets dropped before Logon because their first message could not be framed",
    ),
    s(
        "fixbolt_sources_missing_total",
        Counter,
        Scope::Engine,
        Source::SourcesMissing,
        "Connections that claimed to be pollable and gave no descriptor; zero when healthy",
    ),
    s(
        "fixbolt_message_log_lost_total",
        Counter,
        Scope::Engine,
        Source::LogLost,
        "Messages the message log never wrote; zero when healthy",
    ),
    s(
        "fixbolt_events_lost_total",
        Counter,
        Scope::Engine,
        Source::EventsLost,
        "Events never delivered to a reader: the event ring was full or busy",
    ),
    s(
        "fixbolt_ring_to_app_used_bytes",
        Gauge,
        Scope::Engine,
        Source::RingUsed,
        "Bytes waiting in the ring from the engine to the application; only under RingDispatch",
    ),
    s(
        "fixbolt_ring_to_app_capacity_bytes",
        Gauge,
        Scope::Engine,
        Source::RingCapacity,
        "Size of the ring from the engine to the application; a full ring ends the session (D10b)",
    ),
    s(
        "fixbolt_presession_slots_used",
        Gauge,
        Scope::Engine,
        Source::PresessionUsed,
        "Pre-session slots taken: sockets waiting to send Logon plus connections parked for recovery",
    ),
    s(
        "fixbolt_presession_slots_capacity",
        Gauge,
        Scope::Engine,
        Source::PresessionCapacity,
        "The pre-session ceiling, Limits::pending(); only behind a serve front door",
    ),
    s(
        "fixbolt_session_logged_on",
        Gauge,
        Scope::Session,
        Source::SessionLoggedOn,
        "1 when this session is logged on",
    ),
    s(
        "fixbolt_session_next_out_seq_num",
        Gauge,
        Scope::Session,
        Source::SessionNextOut,
        "The next outgoing MsgSeqNum(34) of this session",
    ),
    s(
        "fixbolt_session_next_in_seq_num",
        Gauge,
        Scope::Session,
        Source::SessionNextIn,
        "The next expected incoming MsgSeqNum(34) of this session",
    ),
    s(
        "fixbolt_session_clock_skew_seconds",
        Gauge,
        Scope::Session,
        Source::SessionSkew,
        "SendingTime(52) of the last message received minus the engine's clock; absent until measured",
    ),
    s(
        "fixbolt_session_pending_output",
        Gauge,
        Scope::Session,
        Source::SessionPendingOutput,
        "1 when this session has bytes queued that the socket has not taken",
    ),
    s(
        "fixbolt_session_journal_refused_total",
        Counter,
        Scope::Session,
        Source::SessionJournalRefused,
        "Outgoing messages the journal refused to record for this session",
    ),
    s(
        "fixbolt_session_resend_beyond_journal_total",
        Counter,
        Scope::Session,
        Source::SessionResendBeyond,
        "Resend requests reaching further back than this session's journal holds",
    ),
    s(
        "fixbolt_events_total",
        Counter,
        Scope::EventKind,
        Source::Events,
        "Events read from the engine, by kind; only when the exporter owns the event stream",
    ),
    s(
        "fixbolt_session_ends_total",
        Counter,
        Scope::Reason,
        Source::SessionEnds,
        "Sessions ended, by DropReason; only when the exporter owns the event stream",
    ),
    s(
        "fixbolt_exporter_scrapes_total",
        Counter,
        Scope::Exporter,
        Source::Scrapes,
        "Requests for /metrics this exporter answered",
    ),
    s(
        "fixbolt_exporter_bad_requests_total",
        Counter,
        Scope::Exporter,
        Source::BadRequests,
        "Requests this exporter refused: malformed, too large, too slow, wrong path or method",
    ),
];

/// The `kind` label of `fixbolt_events_total`, one per `EventKind` variant
/// today, and `other` last for a variant added after this list was written
/// (`EventKind` is `#[non_exhaustive]`).
pub(crate) const EVENT_KINDS: [&str; 14] = [
    "logged_on",
    "ended",
    "ended_without_reason",
    "tls_fell_back_to_userspace",
    "administered",
    "resend_beyond_journal",
    "journal_refused",
    "journal_unwritten",
    "message_log_lost",
    "spoke_first_to_the_bound",
    "origination_undeliverable",
    "message_log_unsent",
    "tls_handshake_refused",
    "other",
];

/// Which [`EVENT_KINDS`] entry an event counts under.
pub(crate) const fn event_kind_index(kind: &fixbolt_engine::observe::EventKind) -> usize {
    use fixbolt_engine::observe::EventKind as K;
    match kind {
        K::LoggedOn => 0,
        K::Ended(_) => 1,
        K::EndedWithoutReason => 2,
        K::TlsFellBackToUserspace => 3,
        K::Administered { .. } => 4,
        K::ResendBeyondJournal { .. } => 5,
        K::JournalRefused { .. } => 6,
        K::JournalUnwritten { .. } => 7,
        K::MessageLogLost { .. } => 8,
        K::SpokeFirstToTheBound { .. } => 9,
        K::OriginationUndeliverable { .. } => 10,
        K::MessageLogUnsent { .. } => 11,
        K::TlsHandshakeRefused { .. } => 12,
        _ => 13,
    }
}

/// `DropReason`'s variants today, as its derived `Debug` prints each, and the
/// `reason` label each is exported under.
///
/// **By name, not by type**, and that is forced: `fixbolt-engine` does not
/// re-export `fixbolt_session::DropReason`, and ADR-0170 decision 9 allows this
/// crate no second runtime dependency to name it through. The `Debug` name of a
/// fieldless enum is its variant name, written into a stack buffer by
/// [`reason_index`] — no allocation. A variant added later, or renamed, lands
/// on `other`, which is what ADR-0170 decision 7 asks for; the unit test
/// `with_events_every_drop_reason_today_has_its_own_label` names every variant
/// through a dev-dependency and fails if one of today's does.
pub(crate) const DROP_REASONS: [(&str, &str); 23] = [
    ("WrongBeginString", "wrong_begin_string"),
    ("NotALogon", "not_a_logon"),
    ("LogonIncomplete", "logon_incomplete"),
    (
        "LogonWithoutDefaultApplVerId",
        "logon_without_default_appl_ver_id",
    ),
    ("WrongSenderCompId", "wrong_sender_comp_id"),
    ("WrongTargetCompId", "wrong_target_comp_id"),
    ("SendingTimeOutOfRange", "sending_time_out_of_range"),
    ("NeverTicked", "never_ticked"),
    ("SequenceNumberTooLow", "sequence_number_too_low"),
    ("NextExpectedTooHigh", "next_expected_too_high"),
    ("OutsideSchedule", "outside_schedule"),
    ("CannotSend", "cannot_send"),
    ("HeartbeatTimeout", "heartbeat_timeout"),
    ("LogonTimedOut", "logon_timed_out"),
    ("LogoutTimedOut", "logout_timed_out"),
    ("PeerLogout", "peer_logout"),
    ("ScheduleClosed", "schedule_closed"),
    ("TransportClosed", "transport_closed"),
    ("DuplicateIdentity", "duplicate_identity"),
    ("RefusedByDeployment", "refused_by_deployment"),
    ("SlowApplication", "slow_application"),
    ("SlowConsumer", "slow_consumer"),
    ("EngineShutdown", "engine_shutdown"),
];

/// How many `reason` samples a scrape prints: every [`DROP_REASONS`] entry,
/// and `other`.
pub(crate) const REASONS: usize = DROP_REASONS.len() + 1;

/// The `reason` label at `i`, `other` past the named ones.
pub(crate) fn reason_label(i: usize) -> &'static str {
    DROP_REASONS.get(i).map_or("other", |(_, label)| label)
}

/// Which `reason` a drop reason counts under: its index in [`DROP_REASONS`],
/// or `DROP_REASONS.len()` — `other` — for a name not in the table.
pub(crate) fn reason_index<R: core::fmt::Debug>(reason: &R) -> usize {
    let mut name = Name::default();
    // A `Debug` longer than the buffer is not one of today's names; `other`.
    if core::fmt::write(&mut name, format_args!("{reason:?}")).is_err() {
        return DROP_REASONS.len();
    }
    let written = name.buf.get(..name.len).unwrap_or(&[]);
    DROP_REASONS
        .iter()
        .position(|(debug, _)| debug.as_bytes() == written)
        .unwrap_or(DROP_REASONS.len())
}

/// A fixed buffer `core::fmt` writes into, refusing rather than growing.
struct Name {
    buf: [u8; 48],
    len: usize,
}

impl Default for Name {
    fn default() -> Self {
        Self {
            buf: [0; 48],
            len: 0,
        }
    }
}

impl core::fmt::Write for Name {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        let end = self.len.checked_add(s.len()).ok_or(core::fmt::Error)?;
        let dst = self.buf.get_mut(self.len..end).ok_or(core::fmt::Error)?;
        dst.copy_from_slice(s.as_bytes());
        self.len = end;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::indexing_slicing
    )]

    use super::*;
    use fixbolt_session::DropReason as D;

    /// Every `DropReason` variant as of 2026-09-24, written out by hand. A new
    /// variant is not in this list and lands on `other` until someone adds it
    /// to [`DROP_REASONS`] — which is decided, not missed: ADR-0170 decision 7.
    const TODAY: [D; 23] = [
        D::WrongBeginString,
        D::NotALogon,
        D::LogonIncomplete,
        D::LogonWithoutDefaultApplVerId,
        D::WrongSenderCompId,
        D::WrongTargetCompId,
        D::SendingTimeOutOfRange,
        D::NeverTicked,
        D::SequenceNumberTooLow,
        D::NextExpectedTooHigh,
        D::OutsideSchedule,
        D::CannotSend,
        D::HeartbeatTimeout,
        D::LogonTimedOut,
        D::LogoutTimedOut,
        D::PeerLogout,
        D::ScheduleClosed,
        D::TransportClosed,
        D::DuplicateIdentity,
        D::RefusedByDeployment,
        D::SlowApplication,
        D::SlowConsumer,
        D::EngineShutdown,
    ];

    #[test]
    fn with_events_every_drop_reason_today_has_its_own_label() {
        let mut seen = [false; REASONS];
        for r in TODAY {
            let i = reason_index(&r);
            assert!(
                i < DROP_REASONS.len(),
                "{r:?} has no label of its own and would be exported as reason=\"other\""
            );
            assert!(!seen[i], "{r:?} shares its label with another reason");
            seen[i] = true;
        }
        assert_eq!(reason_label(DROP_REASONS.len()), "other");
        for (_, label) in DROP_REASONS {
            assert!(
                label.bytes().all(|b| b.is_ascii_lowercase() || b == b'_'),
                "{label}: a label value needs no escaping"
            );
        }
    }

    #[test]
    fn a_name_not_in_the_table_is_other() {
        #[derive(Debug)]
        struct SomethingNew;
        assert_eq!(reason_index(&SomethingNew), DROP_REASONS.len());
        #[derive(Debug)]
        struct AVeryLongNameThatDoesNotFitTheBufferAtAllNotEvenClose;
        assert_eq!(
            reason_index(&AVeryLongNameThatDoesNotFitTheBufferAtAllNotEvenClose),
            DROP_REASONS.len()
        );
    }
}
