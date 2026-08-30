//! The clock rules the 59 definitions cannot see.
//!
//! The acceptance harness ticks a whole `HeartBtInt` at a time, because that is
//! the only thing a `.def` file's silence can mean. So every threshold in
//! `Session::tick` is invisible to it: `[measured 2026-08-29]` any test-request
//! threshold in (1×, 2×] and any timeout in (2×, 3×] reproduces
//! `6_SendTestRequest.def` exactly. This file ticks by the millisecond and
//! names the numbers.
//!
//! It also holds the two rules the corpus states only once, so the file that
//! states them would pass for a weaker rule: a garbled frame is fatal **only**
//! when it claims to be a Logon, and `108=0` means no heartbeats at all.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use fixbolt_conformance::script::{FIXED_TIME_MILLIS, Kind, scenarios, with_real_checksum};
use fixbolt_session::{Acceptor, Config, Link, Session};

/// `HeartBtInt` for the tests that name a threshold, in milliseconds. Ten
/// seconds so 1.2× and 2.4× are whole milliseconds and the assertions can sit
/// on the exact boundary.
const BEAT_MS: u64 = 10_000;

fn acceptor() -> Session<Acceptor, 256> {
    Session::new(Config::acceptor(b"FIX.4.4", b"ISLD", b"TW44"))
}

/// The first `I` line of a definition file, as the loader produces it. A real
/// corpus line rather than an invented packet — `CLAUDE.md` §7.
fn first_input(file: &str) -> Vec<u8> {
    scenarios()
        .unwrap_or_else(|e| panic!("{e}"))
        .into_iter()
        .find(|s| s.file == file)
        .unwrap_or_else(|| panic!("{file} is not in the corpus"))
        .steps
        .into_iter()
        .find_map(|s| match s.kind {
            Kind::Send(m) => Some(m.wire),
            _ => None,
        })
        .unwrap_or_else(|| panic!("{file} has no I line"))
}

/// Recompute `9=` and `10=` after the body has changed length.
///
/// The `10=0` placeholder is not decoration: a message with no trailer parses
/// as `Incomplete`, which this layer reads as "wait for more", and every
/// assertion built on one passes without the message ever being judged.
fn reframe(wire: &[u8]) -> Vec<u8> {
    let s = String::from_utf8(wire.to_vec()).expect("ascii");
    let after_9 = s.find("\u{1}35=").expect("35= follows the frame") + 1;
    let at_10 = s.find("\u{1}10=").map_or(s.len(), |i| i + 1);
    let body = at_10 - after_9;
    let head_end = s.find('\u{1}').expect("8= is a field") + 1;
    let rebuilt = format!(
        "{}9={body}\u{1}{}10=0\u{1}",
        &s[..head_end],
        &s[after_9..at_10]
    );
    with_real_checksum(rebuilt.as_bytes())
}

fn replace(wire: &[u8], from: &str, to: &str) -> Vec<u8> {
    let s = String::from_utf8(wire.to_vec()).expect("ascii");
    let patched = s.replace(from, to);
    assert_ne!(patched, s, "{from} is not in the message");
    patched.into_bytes()
}

/// A real Logon — `1c_InvalidTargetCompID.def`'s line with its one wrong field
/// corrected — asking for `beat` seconds between heartbeats.
fn logon_asking_for(beat_secs: u64) -> Vec<u8> {
    let wire = replace(
        &first_input("1c_InvalidTargetCompID.def"),
        "56=DLSI",
        "56=ISLD",
    );
    reframe(&replace(&wire, "108=30", &format!("108={beat_secs}")))
}

/// A session logged on at [`FIXED_TIME_MILLIS`], with the Logon reply dropped.
fn logged_on(beat_secs: u64) -> Session<Acceptor, 256> {
    let mut s = acceptor();
    s.connect(|_| {});
    s.tick(FIXED_TIME_MILLIS, |_| {});
    let link = s.received(&logon_asking_for(beat_secs), |_| {});
    assert_eq!(link, Link::Up, "the Logon should have been accepted");
    assert!(s.is_logged_on());
    s
}

/// Tick to `FIXED_TIME_MILLIS + after_ms` and report what came out.
fn at(s: &mut Session<Acceptor, 256>, after_ms: u64) -> (Link, Vec<String>) {
    let mut out = Vec::new();
    let link = s.tick(FIXED_TIME_MILLIS + after_ms, |b| {
        out.push(String::from_utf8_lossy(b).replace('\u{1}', "|"));
    });
    (link, out)
}

/// The three numbers, at the millisecond.
///
/// QuickFIX's `SessionState` puts them at 1.0, 1.2 and 2.4 times
/// `HeartBtInt`. The corpus agrees with any of a wide band; this does not.
#[test]
fn the_three_thresholds_are_where_quickfix_puts_them() {
    let mut s = logged_on(BEAT_MS / 1_000);

    // 1× — a heartbeat, because *we* have been quiet that long.
    let (_, out) = at(&mut s, BEAT_MS - 1);
    assert!(out.is_empty(), "one millisecond early: {out:?}");
    let (_, out) = at(&mut s, BEAT_MS);
    assert_eq!(out.len(), 1, "a heartbeat is due: {out:?}");
    assert!(out[0].contains("|35=0|"), "{}", out[0]);
    assert!(
        !out[0].contains("|112="),
        "a heartbeat nobody asked for carries no TestReqID: {}",
        out[0]
    );

    // 1.2× — a test request, because *they* have been quiet that long. Measured
    // from the Logon, which is the last thing that arrived.
    let (_, out) = at(&mut s, BEAT_MS * 12 / 10 - 1);
    assert!(out.is_empty(), "one millisecond early: {out:?}");
    let (_, out) = at(&mut s, BEAT_MS * 12 / 10);
    assert_eq!(out.len(), 1, "a test request is due: {out:?}");
    assert!(out[0].contains("|35=1|"), "{}", out[0]);
    assert!(out[0].contains("|112=TEST|"), "{}", out[0]);

    // 2.4× — the link goes, and nothing is said on the way out.
    let (link, out) = at(&mut s, BEAT_MS * 24 / 10 - 1);
    assert_eq!(link, Link::Up, "one millisecond early");
    assert!(out.is_empty(), "{out:?}");
    let (link, out) = at(&mut s, BEAT_MS * 24 / 10);
    assert_eq!(link, Link::Dropped, "the counterparty never answered");
    assert!(out.is_empty(), "a timeout is not announced: {out:?}");
}

/// No heartbeat goes out while a test request is unanswered.
///
/// Between 1.2× and 2.4× there is a whole interval in which this session has
/// been quiet long enough for a heartbeat to be due. QuickFIX suppresses it —
/// asking twice is what the test request already did. The corpus never lands a
/// tick in that window.
#[test]
fn an_unanswered_test_request_silences_the_heartbeat() {
    let mut s = logged_on(BEAT_MS / 1_000);
    let (_, out) = at(&mut s, BEAT_MS * 12 / 10);
    assert!(out[0].contains("|35=1|"), "the test request: {}", out[0]);

    // A full interval later. `silent` is now ≥ `HeartBtInt`, so without the
    // suppression a heartbeat would go out here.
    let (link, out) = at(&mut s, BEAT_MS * 12 / 10 + BEAT_MS);
    assert_eq!(link, Link::Up, "not yet timed out");
    assert!(
        out.is_empty(),
        "nothing is said until the test request is answered: {out:?}"
    );
}

/// An answered test request puts the session back to normal.
#[test]
fn answering_a_test_request_starts_the_clock_again() {
    let mut s = logged_on(BEAT_MS / 1_000);
    let (_, out) = at(&mut s, BEAT_MS * 12 / 10);
    assert!(out[0].contains("|35=1|"), "{}", out[0]);

    // The counterparty's own heartbeat, `34=2`, arriving at 1.3×.
    let (_, _) = at(&mut s, BEAT_MS * 13 / 10);
    let hb = reframe(&replace(
        &replace(&logon_asking_for(BEAT_MS / 1_000), "35=A", "35=0"),
        "98=0\u{1}108=10\u{1}",
        "",
    ));
    let hb = reframe(&replace(&hb, "34=1", "34=2"));
    let mut out = Vec::new();
    let link = s.received(&hb, |b| {
        out.push(String::from_utf8_lossy(b).replace('\u{1}', "|"));
    });
    assert_eq!(link, Link::Up, "a heartbeat is not an error");
    assert!(out.is_empty(), "a heartbeat is not answered: {out:?}");

    // One interval after *that*, the ordinary heartbeat resumes: the outstanding
    // test request is gone.
    let (link, out) = at(&mut s, BEAT_MS * 13 / 10 + BEAT_MS);
    assert_eq!(link, Link::Up, "the timeout clock restarted too");
    assert_eq!(out.len(), 1, "the heartbeat is due again: {out:?}");
    assert!(out[0].contains("|35=0|"), "{}", out[0]);
}

/// A garbled frame is fatal **only** when it claims to be a Logon.
///
/// `1d_InvalidLogonLengthInvalid.def` is the corpus's only garbled frame before
/// a Logon, and it is itself a Logon — so "any unreadable frame before a Logon
/// is fatal" passes that file just as well, and is wrong. QuickFIX identifies
/// the type out of the raw bytes and hangs up on that alone.
#[test]
fn a_garbled_frame_is_fatal_only_when_it_claims_to_be_a_logon() {
    // The corpus's own bad Logon: `9=40` over a body that is not 40 bytes.
    let bad_logon = first_input("1d_InvalidLogonLengthInvalid.def");
    let mut s = acceptor();
    s.connect(|_| {});
    s.tick(FIXED_TIME_MILLIS, |_| {});
    let mut out = Vec::new();
    assert_eq!(
        s.received(&bad_logon, |b| out.push(b.to_vec())),
        Link::Dropped,
        "a Logon that cannot be read is the end of the connection"
    );
    assert!(out.is_empty(), "and nothing is said about it");

    // The same frame, still unreadable, but announcing itself as a Heartbeat.
    let bad_other = replace(&bad_logon, "35=A", "35=0");
    let mut s = acceptor();
    s.connect(|_| {});
    s.tick(FIXED_TIME_MILLIS, |_| {});
    let mut out = Vec::new();
    assert_eq!(
        s.received(&bad_other, |b| out.push(b.to_vec())),
        Link::Up,
        "anything else unreadable is dropped on the floor, not hung up on"
    );
    assert!(out.is_empty(), "and nothing is said about that either");
}

/// `108=0` is FIX 4.4's way of asking for no heartbeats at all.
///
/// Nothing in the corpus sends it — `[measured]` `108=` is 30, 6 or 2 across
/// all 59 files — so a session that treated zero as "every zero milliseconds"
/// would score the same and flood a real counterparty.
#[test]
fn a_zero_heart_bt_int_means_the_session_never_speaks_first() {
    let mut s = logged_on(0);
    for after in [1, BEAT_MS, BEAT_MS * 100, BEAT_MS * 10_000] {
        let (link, out) = at(&mut s, after);
        assert_eq!(link, Link::Up, "and it never times out either");
        assert!(out.is_empty(), "at {after} ms: {out:?}");
    }
}

/// A session that has not exchanged a Logon never speaks first.
///
/// The corpus reconnects in two files and both send a fresh Logon immediately,
/// so it never sits in this state long enough to notice. **This is the only
/// thing holding the rule**: `connect` deliberately leaves the previous
/// connection's interval in place, because clearing it as well was a line
/// whose reversal changed nothing.
#[test]
fn a_session_awaiting_a_logon_never_speaks_first() {
    let mut s = logged_on(BEAT_MS / 1_000);
    let (_, out) = at(&mut s, BEAT_MS);
    assert_eq!(out.len(), 1, "the first connection does beat: {out:?}");

    s.disconnect(|_| {});
    s.connect(|_| {});
    // The clock does not run backwards for a reconnect, so this connection
    // starts life already past both thresholds of the previous one. Nothing
    // but the state may stop it.
    for after in [BEAT_MS * 2, BEAT_MS * 4, BEAT_MS * 8] {
        let (link, out) = at(&mut s, after);
        assert_eq!(link, Link::Up, "no Logon, no timeout either, at {after} ms");
        assert!(
            out.is_empty(),
            "no Logon has been exchanged on this connection, \
             and the previous one's interval is still in the struct: {out:?}"
        );
    }
}
