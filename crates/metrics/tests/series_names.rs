//! **Every series name is public API** (ADR-0171 decision 1), so it is written
//! down twice: once in `src/series.rs`, which the encoder reads, and once here,
//! by hand. A rename, a removal, a changed type or a changed label set breaks a
//! user's dashboard and alert rules while every compiler and every other test
//! stays green — this file is the one place that notices.
//!
//! Changing a line below is allowed. It is the moment to add a line to
//! `CHANGELOG.md`, and a removal or rename is a breaking change under this
//! crate's semver even though no tool can see it.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

/// `(name, type, labels)` — `docs/plans/2026-09-24-p4-metrics-exporter.md` §C,
/// in its order.
const EXPECTED: &[(&str, &str, &[&str])] = &[
    ("fixbolt_snapshot_available", "gauge", &["engine"]),
    ("fixbolt_snapshot_age_seconds", "gauge", &["engine"]),
    ("fixbolt_snapshots_published_total", "counter", &["engine"]),
    ("fixbolt_healthy", "gauge", &["engine"]),
    ("fixbolt_connections", "gauge", &["engine"]),
    ("fixbolt_sessions_logged_on", "gauge", &["engine"]),
    ("fixbolt_snapshot_truncated", "gauge", &["engine"]),
    ("fixbolt_refused_connections_total", "counter", &["engine"]),
    ("fixbolt_unframeable_prelogon_total", "counter", &["engine"]),
    ("fixbolt_sources_missing_total", "counter", &["engine"]),
    ("fixbolt_message_log_lost_total", "counter", &["engine"]),
    ("fixbolt_events_lost_total", "counter", &["engine"]),
    ("fixbolt_ring_to_app_used_bytes", "gauge", &["engine"]),
    ("fixbolt_ring_to_app_capacity_bytes", "gauge", &["engine"]),
    ("fixbolt_presession_slots_used", "gauge", &["engine"]),
    ("fixbolt_presession_slots_capacity", "gauge", &["engine"]),
    ("fixbolt_session_logged_on", "gauge", &["engine", "conn"]),
    (
        "fixbolt_session_next_out_seq_num",
        "gauge",
        &["engine", "conn"],
    ),
    (
        "fixbolt_session_next_in_seq_num",
        "gauge",
        &["engine", "conn"],
    ),
    (
        "fixbolt_session_clock_skew_seconds",
        "gauge",
        &["engine", "conn"],
    ),
    (
        "fixbolt_session_pending_output",
        "gauge",
        &["engine", "conn"],
    ),
    (
        "fixbolt_session_journal_refused_total",
        "counter",
        &["engine", "conn"],
    ),
    (
        "fixbolt_session_resend_beyond_journal_total",
        "counter",
        &["engine", "conn"],
    ),
    ("fixbolt_events_total", "counter", &["engine", "kind"]),
    (
        "fixbolt_session_ends_total",
        "counter",
        &["engine", "reason"],
    ),
    ("fixbolt_exporter_scrapes_total", "counter", &[]),
    ("fixbolt_exporter_bad_requests_total", "counter", &[]),
];

#[test]
fn series_names() {
    let got: Vec<(&str, &str, &[&str])> = fixbolt_metrics::series::ALL
        .iter()
        .map(|s| (s.name(), s.kind().as_str(), s.labels()))
        .collect();
    let want: Vec<(&str, &str, &[&str])> = EXPECTED.to_vec();
    assert_eq!(
        got, want,
        "the series table in `src/series.rs` no longer matches the list written by hand \
         here. A series name, type or label set is public API (ADR-0171 decision 1): if the \
         change is meant, edit this list too AND add a line to CHANGELOG.md — a removal or \
         rename breaks users' dashboards and alerts"
    );
}

#[test]
fn every_name_follows_the_prometheus_naming_guide() {
    for s in fixbolt_metrics::series::ALL {
        let name = s.name();
        assert!(name.starts_with("fixbolt_"), "{name}: the prefix");
        assert!(
            name.bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_'),
            "{name}: snake_case ASCII"
        );
        assert_eq!(
            name.ends_with("_total"),
            s.kind().as_str() == "counter",
            "{name}: `_total` on counters and only on counters"
        );
        assert!(!s.help().is_empty(), "{name}: a HELP line");
        assert!(
            !s.help().contains(['\\', '\n']),
            "{name}: HELP text needs no escaping"
        );
    }
}
