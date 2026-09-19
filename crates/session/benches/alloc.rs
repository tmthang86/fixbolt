//! Non-negotiable 1, for the session layer: **zero** allocations on the path a
//! message actually takes.
//!
//! `crates/codec/benches/alloc.rs` proves it for parse, encode, lookup, group
//! and text. This proves it for the layer above them — the one that owns a
//! `FieldIndex` and decides what to do with a message. The session is where a
//! `String` for an error, or a `Vec` for a resend, would be easiest to reach
//! for and hardest to notice.
//!
//! # The `unsafe` here
//!
//! Identical to the codec bench and sound for the same three reasons: every
//! method forwards to `System` unchanged but for a relaxed counter; this is a
//! benchmark binary, so nothing ships it; and it is proven by reversal, not by
//! reading — see the note under `judge_allocs` below.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
#![allow(unsafe_code)]
// Not a library crate's source: non-negotiable 7 is about `crates/*/src`, and
// `scripts/check-indexing-debt.sh` counts nothing outside it. An index that
// panics in a test is a failing test, which is what a test is for.
#![allow(clippy::indexing_slicing)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

use fixbolt_codec::TagValue;
use fixbolt_conformance::echo::echo;
use fixbolt_conformance::script::{Kind, scenarios, with_real_checksum};
use fixbolt_dict::Fix44;
use fixbolt_engine::journal::Store;
use fixbolt_session::schedule::{Schedule, Weekdays};
use fixbolt_session::text::SessionText;
use fixbolt_session::{Acceptor, Application, Config, Initiator, Link, Session, clock};

#[cfg(feature = "fix50sp2")]
use fixbolt_codec::{FieldIndex, Parsed, Validation, parse_into};
#[cfg(feature = "fix50sp2")]
use fixbolt_dict::Fixt11Fix50Sp2Tables;
#[cfg(feature = "fix50sp2")]
use fixbolt_session::validate;

static ALLOCS: AtomicUsize = AtomicUsize::new(0);

struct Counting;

// SAFETY: every method forwards to `System`, which is a correct allocator, with
// the same pointer, layout and size it was given. The only addition is a relaxed
// counter increment. See the module comment.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        ALLOCS.fetch_add(1, Ordering::Relaxed);
        unsafe { System.alloc(l) }
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        unsafe { System.dealloc(p, l) }
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 {
        ALLOCS.fetch_add(1, Ordering::Relaxed);
        unsafe { System.realloc(p, l, n) }
    }
    unsafe fn alloc_zeroed(&self, l: Layout) -> *mut u8 {
        ALLOCS.fetch_add(1, Ordering::Relaxed);
        unsafe { System.alloc_zeroed(l) }
    }
}

#[global_allocator]
static A: Counting = Counting;

fn count<F: FnOnce()>(f: F) -> usize {
    let before = ALLOCS.load(Ordering::Relaxed);
    f();
    ALLOCS.load(Ordering::Relaxed) - before
}

/// The first `I` line of a definition file, as the loader produces it.
///
/// Real corpus lines rather than invented packets — `CLAUDE.md` §7 — and it
/// closes a specific trap: a hand-written `9=` or `10=` that is one byte out
/// makes `received` bail at the frame, and the bench then reports zero
/// allocations for a path it never walked. The links are asserted below for
/// the same reason.
fn first_input(file: &str) -> Vec<u8> {
    inputs(file).into_iter().next().expect("an I line")
}

/// Every `I` line of a definition file, in order.
fn inputs(file: &str) -> Vec<Vec<u8>> {
    scenarios()
        .expect("corpus")
        .into_iter()
        .find(|s| s.file == file)
        .unwrap_or_else(|| panic!("{file} is not in the corpus"))
        .steps
        .into_iter()
        .filter_map(|s| match s.kind {
            Kind::Send(m) => Some(m.wire),
            _ => None,
        })
        .collect()
}

/// The corpus's own application, echoing every order back.
///
/// It is the heaviest thing that runs behind [`Session::received_with`]: a
/// second `FieldIndex<256>`, a `TemplateBuilder<128, 4096>` and an encode, all
/// on the stack of one call. If any of it reached the heap, `deliver` below
/// would say so.
struct EchoApp;

impl Application for EchoApp {
    fn on_message(
        &mut self,
        msg: &[u8],
        hdr: fixbolt_session::Header<'_>,
        out: &mut [u8],
    ) -> Option<core::ops::Range<usize>> {
        let (seq, stamp) = (hdr.seq, hdr.stamp);
        echo(msg, out, seq, stamp).ok()
    }
}

fn acceptor() -> Session<TagValue<Fix44, 256>, Acceptor> {
    Session::new(Config::acceptor(b"FIX.4.4", b"ISLD", b"TW44"))
}

/// The FIXT 1.1 / FIX 5.0 SP2 encoding, spelled as `tests/score_fixt.rs` and
/// `tests/fixt.rs` spell it — `dict` publishes no alias for this pairing, and
/// `CLAUDE.md` §6 says the caller picks `N`. 256 because that is the capacity a
/// real FIXT session is given in those two files, and this bench must count the
/// path a session takes rather than a cheaper one.
#[cfg(feature = "fix50sp2")]
type Fixt = TagValue<Fixt11Fix50Sp2Tables, 256>;

/// The corpus's FIXT acceptor: ISLD, speaking FIX 5.0 SP2 (`1137=9`) to
/// TW50SP2. The same configuration `tests/fixt.rs` builds.
#[cfg(feature = "fix50sp2")]
fn fixt_acceptor() -> Session<Fixt, Acceptor> {
    Session::new(Config::acceptor_fixt(
        b"FIXT.1.1",
        b"ISLD",
        b"TW50SP2",
        b"9",
    ))
}

/// One FIXT message with `9=` and `10=` computed, `|` for SOH — `tests/fixt.rs`'s
/// `msg`, which is where these bytes come from.
///
/// Every call is outside a counted window: what is measured is the path a
/// message takes, never the building of one.
#[cfg(feature = "fix50sp2")]
fn fixt_msg(body: &str) -> Vec<u8> {
    let body = body.replace('|', "\u{1}");
    let mut m = format!("8=FIXT.1.1\u{1}9={}\u{1}", body.len()).into_bytes();
    m.extend_from_slice(body.as_bytes());
    let sum: u32 = m.iter().map(|c| u32::from(*c)).sum();
    m.extend_from_slice(format!("10={:03}\u{1}", sum % 256).as_bytes());
    m
}

/// 32 of the 48 top-level groups `TradeCaptureReport(AE)` declares whose
/// delimiter enumerates no value, so a one-entry group can carry any
/// well-formed one — copied field for field from `tests/fixt.rs`'s
/// `AE_GROUPS`, which explains the 32-blocks/33-counters shape in full.
///
/// Duplicated rather than shared: `tests/fixt.rs` is an integration test
/// binary, not a library this bench crate can depend on, and B4d's row asks
/// for the *same bytes* as B4c's Test 3, not merely a similar fixture.
#[cfg(feature = "fix50sp2")]
const AE_GROUPS: [(&str, &str, &str); 32] = [
    ("1907", "1903", "X"),
    ("1116", "1117", "R"),
    ("454", "455", "A"),
    ("1976", "1977", "1"),
    ("2304", "2305", "A"),
    ("1018", "1019", "P"),
    ("40278", "40471", "B"),
    ("41230", "41231", "B"),
    ("41092", "41093", "E"),
    ("41094", "41095", "F"),
    ("42775", "42776", "B"),
    ("41116", "41117", "B"),
    ("41137", "41138", "20260919"),
    ("41140", "41141", "B"),
    ("41152", "41153", "20260919"),
    ("40019", "40020", "Y"),
    ("40181", "40182", "1.0"),
    ("40022", "40023", "USD"),
    ("40204", "40209", "0"),
    ("42296", "42297", "E"),
    ("2734", "2733", "M"),
    ("2746", "2747", "20260919-12:00:00.000"),
    ("40040", "40041", "D"),
    ("40046", "40047", "S"),
    ("40042", "40043", "M"),
    ("711", "311", "U"),
    ("1703", "1704", "1.0"),
    ("555", "600", "L"),
    ("768", "769", "20260919-12:00:00.000"),
    ("1387", "1388", "1"),
    ("41312", "41313", "J"),
    ("2104", "2105", "A"),
];

/// `tests/fixt.rs`'s `trade_capture_report`, with the tail's nested `447=`
/// pulled out as a parameter instead of hard-coded to `D`.
///
/// B4d's third liveness assertion needs a corrupted **copy** of the fault-free
/// fixture — one field changed, nothing else — and the nested `447` is the one
/// that sits *after* the 33rd counter, past every slot `SeenCounters` has: the
/// top-level `447={stray}` a line below is `tests/fixt.rs`'s Test 1 fixture
/// (a member met *before* its own counter), a different fault this case is not
/// about. `regulatory_count` and `stray` stay fixed at `"1"` / `"D"` here — the
/// fault-free values — so the only thing that differs between this case's two
/// calls is `nested_party_id_source`.
#[cfg(feature = "fix50sp2")]
fn trade_capture_report(
    regulatory_count: &str,
    stray: &str,
    nested_party_id_source: &str,
) -> Vec<u8> {
    let mut body = String::from("35=AE|34=2|49=TW50SP2|52=20260828-12:00:00|56=ISLD|");
    for (counter, delimiter, value) in AE_GROUPS {
        let count = if counter == "1907" {
            regulatory_count
        } else {
            "1"
        };
        body.push_str(&format!("{counter}={count}|{delimiter}={value}|"));
    }
    body.push_str(&format!("447={stray}|"));
    body.push_str(&format!(
        "552=1|54=1|453=1|448=A|447={nested_party_id_source}|452=1|"
    ));
    fixt_msg(&body)
}

fn main() {
    let now = clock::parse_utc(b"20260828-12:00:00").expect("a real instant");

    // A Logon this acceptor accepts, and three it refuses — one per rule that
    // does not need a wrong CompID to fire. Loaded before counting starts.
    let good = first_input("15_HeaderAndBodyFieldsOrderedDifferently.def");
    let refused = [
        first_input("1d_InvalidLogonWrongBeginString.def"),
        first_input("1d_InvalidLogonBadSendingTime.def"),
        first_input("1e_NotLogonMessage.def"),
    ];

    // Constructed before counting starts. What is measured is the path a
    // message takes, not the setting up of a session.
    let mut session = acceptor();
    session.connect(|_| unreachable!("step 1 emits nothing"));
    session.tick(now, |_| unreachable!("step 1 emits nothing"));

    // Warm anything lazy in the runtime — and prove both paths are the paths
    // they claim to be, so a zero below means "did not allocate" and not
    // "did not run".
    assert_eq!(
        session.received(&good, |_| ()),
        Link::Up,
        "the accepted path must actually accept"
    );
    for wire in &refused {
        let mut s = acceptor();
        s.connect(|_| ());
        s.tick(now, |_| ());
        assert_eq!(
            s.received(wire, |_| ()),
            Link::Dropped,
            "the refused path must actually refuse"
        );
    }

    // A fresh session each time. Replaying one Logon into the *same* session
    // would be refused as a sequence number already used from the second
    // iteration on, and 9 999 of the 10 000 would measure the early return
    // rather than the path this case is named for.
    let accept_allocs = count(|| {
        for _ in 0..10_000 {
            let mut s = acceptor();
            s.connect(|_| ());
            s.tick(now, |_| ());
            s.received(&good, |_| ());
        }
    });

    // The refusal path, and a fresh session each time so a dropped link does
    // not short-circuit the judging. `Session::new` is inside the count on
    // purpose: an engine builds one per connection, and a per-connection
    // allocation is exactly the kind this bench exists to catch.
    //
    // `[measured]` the reversal: making `Refusal` carry a `String` reports
    // `refuse 30000` here instead of 0.
    let refuse_allocs = count(|| {
        for _ in 0..10_000 {
            for wire in &refused {
                let mut s = acceptor();
                s.connect(|_| ());
                s.tick(now, |_| ());
                s.received(wire, |_| ());
            }
        }
    });

    let tick_allocs = count(|| {
        for i in 0..10_000u64 {
            session.tick(now + i, |_| ());
        }
    });

    // The two paths step 4 added, and both of them *send*. `108=30` in `4b`'s
    // Logon, so one tick a whole interval later is a heartbeat this session
    // decided on by itself — the only output in this crate that no input asked
    // for. A fresh session per iteration, for the reason `accept` gives.
    let heartbeat_wire = inputs("4b_ReceivedTestRequest.def");
    let (beat_logon, test_request) = (&heartbeat_wire[0], &heartbeat_wire[1]);
    {
        let mut s = acceptor();
        s.connect(|_| ());
        s.tick(now, |_| ());
        s.received(beat_logon, |_| ());
        let mut sent = 0usize;
        s.tick(now + 30_000, |_| sent += 1);
        assert_eq!(sent, 1, "the beat path must actually beat");
        let mut answered = 0usize;
        s.received(test_request, |_| answered += 1);
        assert_eq!(answered, 1, "the answer path must actually answer");
    }

    let beat_allocs = count(|| {
        for _ in 0..10_000 {
            let mut s = acceptor();
            s.connect(|_| ());
            s.tick(now, |_| ());
            s.received(beat_logon, |_| ());
            s.tick(now + 30_000, |_| ());
        }
    });

    let answer_allocs = count(|| {
        for _ in 0..10_000 {
            let mut s = acceptor();
            s.connect(|_| ());
            s.tick(now, |_| ());
            s.received(beat_logon, |_| ());
            s.received(test_request, |_| ());
        }
    });

    // Step 5's paths, and two of them send. `RejectResentMessage.def` opens a
    // gap with a `TestRequest` running ahead, closes it with the message that
    // was missing, and the held one is replayed — a `ResendRequest` out, a
    // 512-byte copy off the queue, and a `Heartbeat` out.
    // `8_OnlyAdminMessages.def` asks this end for messages back and gets a
    // `SequenceReset` gap fill.
    let resent = inputs("RejectResentMessage.def");
    let (gap_logon, runs_ahead, closes_gap) = (&resent[0], &resent[1], &resent[2]);
    let admin = inputs("8_OnlyAdminMessages.def");
    {
        let mut s = acceptor();
        s.connect(|_| ());
        s.tick(now, |_| ());
        let mut n = 0usize;
        s.received(gap_logon, |_| n += 1);
        s.received(runs_ahead, |_| n += 1);
        assert_eq!(n, 2, "a Logon reply and a resend request");
        s.received(closes_gap, |_| n += 1);
        assert_eq!(n, 4, "a reject, and the held message replayed");

        let mut s = acceptor();
        s.connect(|_| ());
        s.tick(now, |_| ());
        let mut n = 0usize;
        for wire in &admin[..5] {
            s.received(wire, |_| n += 1);
        }
        assert_eq!(n, 5, "a Logon reply, three heartbeats and one gap fill");
    }

    let gap_allocs = count(|| {
        for _ in 0..10_000 {
            let mut s = acceptor();
            s.connect(|_| ());
            s.tick(now, |_| ());
            s.received(gap_logon, |_| ());
            s.received(runs_ahead, |_| ());
            s.received(closes_gap, |_| ());
        }
    });

    let fill_allocs = count(|| {
        for _ in 0..10_000 {
            let mut s = acceptor();
            s.connect(|_| ());
            s.tick(now, |_| ());
            for wire in &admin[..5] {
                s.received(wire, |_| ());
            }
        }
    });

    // Step 6a's path: an application message handed over, echoed, and the
    // reply written back through the session's own buffer.
    let ordered = inputs("15_HeaderAndBodyFieldsOrderedDifferently.def");
    let (order_logon, order) = (&ordered[0], &ordered[1]);
    {
        let mut s = acceptor();
        let mut journal = Store::new();
        s.connect(|_| ());
        s.tick(now, |_| ());
        s.received(order_logon, |_| ());
        let mut echoed = 0usize;
        s.received_with(order, &mut EchoApp, &mut journal, |_| echoed += 1);
        assert_eq!(echoed, 1, "the delivery path must actually deliver");
    }

    // **One journal, built before the window and reused.** `[measured
    // 2026-09-04]` `SLOTS` went from 8 to 4096 and the ring stopped fitting
    // inline, so `Store::new()` allocates — and it was being called 10 000
    // times inside this window, which read `10000` and had nothing to do with
    // the session. ADR-0046 decision 3 calls that construction startup work.
    // Reuse is sound here because each iteration builds a fresh session that
    // starts at `34=1` and overwrites the same slots before reading them back.
    let mut shared = Store::new();
    let deliver_allocs = count(|| {
        for _ in 0..10_000 {
            let mut s = acceptor();
            let journal = &mut shared;
            s.connect(|_| ());
            s.tick(now, |_| ());
            s.received(order_logon, |_| ());
            s.received_with(order, &mut EchoApp, journal, |_| ());
        }
    });

    // Step 6b's path: three kept application messages replayed, each rebuilt
    // with `43=Y` and its original `52=` carried as `122=`, then framed again.
    let only_app = inputs("8_OnlyApplicationMessages.def");
    {
        let mut s = acceptor();
        let mut journal = Store::new();
        s.connect(|_| ());
        s.tick(now, |_| ());
        for wire in &only_app[..4] {
            s.received_with(wire, &mut EchoApp, &mut journal, |_| ());
        }
        let mut replayed = 0usize;
        s.received_with(&only_app[4], &mut EchoApp, &mut journal, |_| replayed += 1);
        assert_eq!(replayed, 3, "the resend path must actually resend");
    }

    let resend_allocs = count(|| {
        for _ in 0..10_000 {
            let mut s = acceptor();
            let journal = &mut shared;
            s.connect(|_| ());
            s.tick(now, |_| ());
            for wire in &only_app[..5] {
                s.received_with(wire, &mut EchoApp, journal, |_| ());
            }
        }
    });

    // The initiator's two send paths: the Logon it owes on connecting, and an
    // application message it originates. Neither exists on the acceptor side —
    // everything an acceptor sends is an answer.
    let order = inputs("15_HeaderAndBodyFieldsOrderedDifferently.def")[1].clone();
    let logon_reply = inputs("15_HeaderAndBodyFieldsOrderedDifferently.def")[0].clone();
    {
        let mut s: Session<TagValue<Fix44, 256>, Initiator> =
            Session::new(Config::initiator(b"FIX.4.4", b"TW44", b"ISLD"));
        s.connect(|_| ());
        let mut sent = 0usize;
        s.tick(now, |_| sent += 1);
        assert_eq!(sent, 1, "the initiator must actually send a Logon");
    }

    let logon_out_allocs = count(|| {
        for _ in 0..10_000 {
            let mut s: Session<TagValue<Fix44, 256>, Initiator> =
                Session::new(Config::initiator(b"FIX.4.4", b"TW44", b"ISLD"));
            s.connect(|_| ());
            s.tick(now, |_| ());
        }
    });

    // Originating an application message needs a logged-on session, and the
    // acceptor is the side this corpus can log on in one step.
    {
        let mut s = acceptor();
        let mut journal = Store::new();
        s.connect(|_| ());
        s.tick(now, |_| ());
        s.received(&logon_reply, |_| ());
        let mut sent = 0usize;
        s.send_application(&order, &mut journal, |_| sent += 1);
        assert_eq!(sent, 1, "the origination path must actually originate");
    }

    let originate_allocs = count(|| {
        for _ in 0..10_000 {
            let mut s = acceptor();
            let journal = &mut shared;
            s.connect(|_| ());
            s.tick(now, |_| ());
            s.received(&logon_reply, |_| ());
            s.send_application(&order, journal, |_| ());
        }
    });

    // The three the operator orders: a heartbeat nobody asked for, a
    // `TestRequest` with a caller's `112=`, and a `ResendRequest` with a
    // caller's range. All three send, all three are new send paths, and all
    // three take a `&[u8]` or a `u32` from outside the session — which is the
    // shape that tempts a `to_vec()`.
    //
    // The acceptor is the side this corpus can log on in one step; the
    // functions themselves are role-blind, and `tests/initiator.rs` drives them
    // from the initiator.
    {
        let mut s = acceptor();
        s.connect(|_| ());
        s.tick(now, |_| ());
        s.received(&logon_reply, |_| ());
        let mut sent = 0usize;
        assert!(s.send_heartbeat(|_| sent += 1), "the beat must go out");
        assert!(
            s.send_test_request(b"OPERATOR-7", |_| sent += 1),
            "the test request must go out"
        );
        assert!(
            s.send_resend_request(4, 9, |_| sent += 1),
            "the resend request must go out"
        );
        assert_eq!(sent, 3, "three ordered paths, three messages");
    }

    let ordered_allocs = count(|| {
        for _ in 0..10_000 {
            let mut s = acceptor();
            s.connect(|_| ());
            s.tick(now, |_| ());
            s.received(&logon_reply, |_| ());
            s.send_heartbeat(|_| ());
            s.send_test_request(b"OPERATOR-7", |_| ());
            s.send_resend_request(4, 9, |_| ());
        }
    });

    let clock_allocs = count(|| {
        for _ in 0..10_000 {
            let _ = clock::parse_utc(b"20260828-12:00:00.123");
            let _ = clock::parse_utc(b"not a timestamp!!");
        }
    });

    // A Logout's or a Reject's `58=` text. The two numbered variants are the
    // case that tempts `format!`, which is exactly what non-negotiable 2
    // forbids here. This case moved from `codec`'s bench with the table.
    let mut text = [0u8; SessionText::MAX_LEN];
    let _ = SessionText::ValueIsIncorrect.render(&mut text);
    let text_allocs = count(|| {
        for i in 0..10_000u32 {
            for v in SessionText::ALL {
                let _ = v.render(&mut text);
            }
            let _ = SessionText::MsgSeqNumTooLow {
                expecting: i,
                received: i / 2,
            }
            .render(&mut text);
        }
    });

    // --- the schedule -------------------------------------------------------
    //
    // `tick` now consults a `Schedule` on **every** turn, so the arithmetic is
    // on the path non-negotiable 1 is about. Two cases, because the branches
    // differ: inside its hours a session ticks on, outside them it sends a
    // `Logout` and gives up the link, and only the second walks the send path.
    //
    // A weekly window with a weekday filter, not the cheapest shape: a `daily`
    // window with `Weekdays::ALL` skips the day lookup entirely, and a zero
    // from the case that cannot allocate proves nothing about the one that
    // might.
    let filtered = Schedule::daily(8 * 3_600, 17 * 3_600)
        .expect("legal")
        .with_weekdays(Weekdays::WEEKDAYS)
        .expect("Monday to Friday");
    let day = 86_400_000u64;
    let open_at = {
        // The corpus's own instant may fall on a weekend; find an open one.
        let mut t = now;
        while !filtered.contains(t) {
            t += day;
        }
        t
    };
    let shut_at = open_at + 20 * 3_600_000;
    assert!(
        filtered.contains(open_at),
        "the open case must really be open"
    );
    assert!(!filtered.contains(shut_at), "and the shut case really shut");

    let mut open_session: Session<TagValue<Fix44, 256>, Acceptor> =
        Session::new(Config::acceptor(b"FIX.4.4", b"ISLD", b"TW44").with_schedule(filtered));
    open_session.connect(|_| ());
    open_session.tick(open_at, |_| ());
    assert_eq!(
        open_session.tick(open_at, |_| ()),
        Link::Up,
        "an open session ticks on"
    );
    let schedule_open_allocs = count(|| {
        for i in 0..10_000u64 {
            open_session.tick(open_at + i % 1_000, |_| ());
        }
    });

    // The closing turn, built fresh each time because it ends the link.
    {
        let mut s: Session<TagValue<Fix44, 256>, Acceptor> =
            Session::new(Config::acceptor(b"FIX.4.4", b"ISLD", b"TW44").with_schedule(filtered));
        s.connect(|_| ());
        s.tick(open_at, |_| ());
        s.received(&logon_reply, |_| ());
        let mut sent = 0usize;
        assert_eq!(
            s.tick(shut_at, |_| sent += 1),
            Link::Dropped,
            "the window shut on a live session"
        );
        assert_eq!(sent, 1, "and the closing path must really send the Logout");
    }
    let schedule_shut_allocs = count(|| {
        for _ in 0..10_000 {
            let mut s: Session<TagValue<Fix44, 256>, Acceptor> = Session::new(
                Config::acceptor(b"FIX.4.4", b"ISLD", b"TW44").with_schedule(filtered),
            );
            s.connect(|_| ());
            s.tick(open_at, |_| ());
            s.received(&logon_reply, |_| ());
            s.tick(shut_at, |_| ());
        }
    });

    // `789=` lower than the outbound count: the unprompted retransmit. It runs
    // through `continue_replay`, so it is the same machinery `resend` above
    // counts — but reached from the **Logon**, which is a path no other case
    // here takes. `[verified 2026-09-06]` the corpus carries no `789=` at all,
    // so this wire is built here.
    let next_expected_logon = {
        let s = String::from_utf8(good.clone()).expect("ascii");
        // `108=2` here, not `108=30`: this file's Logon carries a two-second
        // interval. `[measured 2026-09-06]` anchoring on the wrong text made
        // `replace` a no-op, the wire carried no `789=`, and the liveness
        // assertion below caught it — which is the only reason this comment
        // exists rather than a silent zero.
        let body = s.replace("108=2\u{1}", "108=2\u{1}789=498\u{1}");
        assert_ne!(body, s, "the 789 field must actually be inserted");
        let after_9 = body.find("\u{1}35=").expect("35= follows the frame") + 1;
        let at_10 = body.find("\u{1}10=").map_or(body.len(), |i| i + 1);
        let head_end = body.find('\u{1}').expect("8= is a field") + 1;
        let n = at_10 - after_9;
        with_real_checksum(
            format!(
                "{}9={n}\u{1}{}10=0\u{1}",
                &body[..head_end],
                &body[after_9..at_10]
            )
            .as_bytes(),
        )
    };
    {
        // The path is proven live before it is counted: a zero below must mean
        // *did not allocate*, never *did not run*.
        let mut s: Session<TagValue<Fix44, 256>, Acceptor> = Session::resume(
            Config::acceptor(b"FIX.4.4", b"ISLD", b"TW44")
                .with_next_expected(true)
                .with_last_processed(true),
            500,
            1,
        );
        let mut sent = 0usize;
        s.connect(|_| ());
        s.tick(now, |_| ());
        s.received(&next_expected_logon, |_| sent += 1);
        assert_eq!(
            sent, 2,
            "the 789 path must answer the Logon and then fill the gap"
        );
        // Both halves of `789` are on this path, so both are counted: the knob
        // is on, so the reply carries the field, and the inbound `789=498`
        // drives the replay. A case that only proved one of them would leave
        // the other's arithmetic uncounted.
        let mut reply = Vec::new();
        let mut s2: Session<TagValue<Fix44, 256>, Acceptor> = Session::new(
            Config::acceptor(b"FIX.4.4", b"ISLD", b"TW44")
                .with_next_expected(true)
                .with_last_processed(true),
        );
        s2.connect(|_| ());
        s2.tick(now, |_| ());
        s2.received(&good, |b| reply.extend_from_slice(b));
        assert!(
            reply.windows(4).any(|w| w == b"789="),
            "the outbound half of 789 must be on this path too"
        );
        assert!(
            reply.windows(4).any(|w| w == b"369="),
            "and 369, so this case counts both fields' arithmetic"
        );
    }
    let next_expected_allocs = count(|| {
        for _ in 0..10_000 {
            let mut s: Session<TagValue<Fix44, 256>, Acceptor> = Session::resume(
                Config::acceptor(b"FIX.4.4", b"ISLD", b"TW44")
                    .with_next_expected(true)
                    .with_last_processed(true),
                500,
                1,
            );
            s.connect(|_| ());
            s.tick(now, |_| ());
            s.received(&next_expected_logon, |_| ());
        }
    });

    // --- FIXT 1.1 / FIX 5.0 SP2 ---------------------------------------------
    //
    // Everything above drives `Fix44`. B1-B4 added a second dictionary, a
    // second `BeginString` rule, `1137=` on the Logon and a fourth scan pass
    // for group members, and **not one of those paths had ever been counted**.
    // ADR-0084 was accepted citing a bench case for these tables; these are it.
    //
    // The three are here rather than in `crates/codec/benches/alloc.rs`
    // because all three are the *session's* pass: `validate` is this crate's
    // function, and the Logon is this crate's encoder. The codec's own bench
    // carries the FIXT parse.
    #[cfg(feature = "fix50sp2")]
    let (fixt_validate_allocs, fixt_group_allocs, fixt_logon_allocs, fixt_trade_capture_allocs) = {
        // A `35=D` off the FIXT wire, no repeating group: the message shape the
        // 17 cases above already cover for FIX 4.4, now through the other
        // dictionary. Parsed once, outside every counted window — the parse has
        // its own case in the codec's bench and pricing it twice would say
        // nothing here.
        let nos = fixt_msg(
            "35=D|34=2|49=TW50SP2|52=20260828-12:00:00|56=ISLD|11=ID|21=1|\
38=002000.00|40=1|54=1|55=INTC|60=20260828-12:00:00|167=CS|",
        );
        let mut nos_idx: FieldIndex<256> = FieldIndex::new();
        let r = parse_into::<Fixt11Fix50Sp2Tables, 256>(&nos, &mut nos_idx, Validation::ALL);
        assert!(
            matches!(r, Ok(Parsed::Complete { .. })),
            "the FIXT fixture must parse: {r:?}"
        );
        let nos_view = nos_idx.view(&nos);
        // Liveness, and the strongest form available: `None` is returned only
        // after **all four** passes have run to the end. A faulty message would
        // return early and this case would then count a prefix — which is the
        // trap `benches/validate.rs`' module comment is about.
        assert_eq!(
            validate::<Fixt11Fix50Sp2Tables, 256>(&nos_view, b"D"),
            None,
            "the FIXT validate path must actually run the whole pass"
        );
        let validate_allocs = count(|| {
            for _ in 0..10_000 {
                let _ = validate::<Fixt11Fix50Sp2Tables, 256>(&nos_view, b"D");
            }
        });

        // **The same pass over a message that really carries a group.**
        //
        // `[verified 2026-09-19]` the eight `.def` files behind the 17 cases
        // above — `15_HeaderAndBodyFieldsOrderedDifferently`,
        // `1d_InvalidLogonBadSendingTime`, `1d_InvalidLogonWrongBeginString`,
        // `1e_NotLogonMessage`, `4b_ReceivedTestRequest`, `8_OnlyAdminMessages`,
        // `8_OnlyApplicationMessages`, `RejectResentMessage` — contain **zero**
        // group counters between them. So every existing case proves that a
        // *group-free* message allocates nothing, and B4b's fourth scan pass
        // and its `SeenCounters` stack array live on exactly the path none of
        // them walks.
        //
        // `NoPartyIDs(453)` on the `35=D`, two entries, rather than the
        // header's `NoHops(627)`: 453 is a body group with five declared
        // members on this message type (`448, 447, 452, 2376, 802`), so a
        // populated instance exercises `SeenCounters::record`, `defers` and
        // `scan_group_members` over several members and two repetitions.
        // `NoHops` would do the same job with three members, and is the harder
        // fixture to keep fault-free because the header's order rules apply to
        // it as well as the group's.
        let grouped = fixt_msg(
            "35=D|34=2|49=TW50SP2|52=20260828-12:00:00|56=ISLD|11=ID|21=1|\
38=002000.00|40=1|54=1|55=INTC|453=2|448=PARTYA|447=D|452=1|448=PARTYB|447=D|\
452=2|60=20260828-12:00:00|167=CS|",
        );
        let mut grp_idx: FieldIndex<256> = FieldIndex::new();
        let r = parse_into::<Fixt11Fix50Sp2Tables, 256>(&grouped, &mut grp_idx, Validation::ALL);
        assert!(
            matches!(r, Ok(Parsed::Complete { .. })),
            "the grouped FIXT fixture must parse: {r:?}"
        );
        let grp_view = grp_idx.view(&grouped);
        // Three liveness assertions, because a zero here would otherwise be the
        // easiest one in this file to get for the wrong reason.
        //
        // One: the group is really populated — two entries walked off the index
        // the parser built, not a `453=2` the scan never looked past.
        assert_eq!(
            grp_view
                .group::<Fixt11Fix50Sp2Tables>(b"D", 453)
                .expect("453 is a group of D")
                .count(),
            2,
            "the group case must actually carry two party entries"
        );
        // Two: the whole pass runs to the end on it.
        assert_eq!(
            validate::<Fixt11Fix50Sp2Tables, 256>(&grp_view, b"D"),
            None,
            "the grouped FIXT message must be fault-free"
        );
        // Three, and the one that pins the *deferred member* path specifically:
        // break one member's value and the fault must come back. `452` is an
        // `int`, so `452=NOPE` is a `373=6` — and it can only be found by the
        // fourth pass, because `scan_fields` defers a member of a group this
        // message carries (ADR-0084 decision 2).
        let broken = fixt_msg(
            "35=D|34=2|49=TW50SP2|52=20260828-12:00:00|56=ISLD|11=ID|21=1|\
38=002000.00|40=1|54=1|55=INTC|453=2|448=PARTYA|447=D|452=1|448=PARTYB|447=D|\
452=NOPE|60=20260828-12:00:00|167=CS|",
        );
        let mut broken_idx: FieldIndex<256> = FieldIndex::new();
        let _ = parse_into::<Fixt11Fix50Sp2Tables, 256>(&broken, &mut broken_idx, Validation::ALL);
        assert!(
            validate::<Fixt11Fix50Sp2Tables, 256>(&broken_idx.view(&broken), b"D").is_some(),
            "the group-member pass must actually inspect a member's value"
        );
        let group_allocs = count(|| {
            for _ in 0..10_000 {
                let _ = validate::<Fixt11Fix50Sp2Tables, 256>(&grp_view, b"D");
            }
        });

        // The outbound Logon B4 added: a FIXT acceptor answers with its **own**
        // `1137=`, not the counterparty's echoed back (ADR-0080 decision 3).
        // That is a field written into the reply from the `Config`, which is
        // the shape that tempts a `to_vec()` — and `out.rs` writes it on a path
        // no FIX 4.4 case above reaches.
        let fixt_logon =
            fixt_msg("35=A|34=1|49=TW50SP2|52=20260828-12:00:00|56=ISLD|98=0|108=30|1137=9|");
        {
            let mut s = fixt_acceptor();
            s.connect(|_| ());
            s.tick(now, |_| ());
            let mut reply = Vec::new();
            assert_eq!(
                s.received(&fixt_logon, |b| reply.extend_from_slice(b)),
                Link::Up,
                "the FIXT logon path must actually log on"
            );
            assert!(
                reply.windows(7).any(|w| w == b"\x011137=9"),
                "and the reply must actually carry 1137"
            );
        }
        // A fresh session each time, for the reason `accept` gives above: a
        // replayed Logon is refused from the second iteration on.
        let logon_allocs = count(|| {
            for _ in 0..10_000 {
                let mut s = fixt_acceptor();
                s.connect(|_| ());
                s.tick(now, |_| ());
                s.received(&fixt_logon, |_| ());
            }
        });

        // **The same pass over a `TradeCaptureReport` whose 33 group counters
        // fill `SeenCounters`** (ADR-0085). `d7be83d` made the overflow
        // fallback positional — `in_a_group_before` — instead of a switch to a
        // different question, so the 32-slot array is now a cache of the same
        // answer rather than a second code path. Until this case existed
        // nothing had ever counted it: the three cases above are all far short
        // of 32 counters, so `SeenCounters::defers`'s `full` branch has never
        // run inside this bench binary.
        //
        // Same bytes as `tests/fixt.rs`'s
        // `the_same_thirty_three_counters_without_the_two_faults_are_accepted`
        // (B4c's Test 3, the fault-free twin) — a bench on different bytes
        // proves something else.
        let clean = trade_capture_report("1", "D", "D");
        let mut clean_idx: FieldIndex<256> = FieldIndex::new();
        let r = parse_into::<Fixt11Fix50Sp2Tables, 256>(&clean, &mut clean_idx, Validation::ALL);
        assert!(
            matches!(r, Ok(Parsed::Complete { .. })),
            "the 33-counter TradeCaptureReport fixture must parse: {r:?}"
        );
        let clean_view = clean_idx.view(&clean);
        // Three liveness assertions, for the same reason the populated-group
        // case above gives.
        //
        // One: both groups this fixture carries are really populated off the
        // index the parser built — the top-level `1907` block and the `552`
        // block the array overflowed into — not a count the scan never looked
        // past.
        assert_eq!(
            clean_view
                .group::<Fixt11Fix50Sp2Tables>(b"AE", 1907)
                .expect("1907 is a group of AE")
                .count(),
            1,
            "the 1907 block must carry its one declared entry"
        );
        assert_eq!(
            clean_view
                .group::<Fixt11Fix50Sp2Tables>(b"AE", 552)
                .expect("552 is a group of AE")
                .count(),
            1,
            "the 552 block must carry its one declared entry"
        );
        // Two: the whole pass runs to the end on it.
        assert_eq!(
            validate::<Fixt11Fix50Sp2Tables, 256>(&clean_view, b"AE"),
            None,
            "the 33-counter TradeCaptureReport must be fault-free"
        );
        let trade_capture_allocs = count(|| {
            for _ in 0..10_000 {
                let _ = validate::<Fixt11Fix50Sp2Tables, 256>(&clean_view, b"AE");
            }
        });
        // Three, and the one that proves the `full` branch actually ran in
        // *this bench binary* rather than only in `tests/fixt.rs`: a corrupted
        // **copy**, one field changed — the `447=` sitting after the 33rd
        // counter, inside `552`'s nested `453` group, past every slot
        // `SeenCounters` has. `447=ZZ` is not a `PartyIDSource`, and
        // `tests/fixt.rs`'s Test 1 already proves that same enum answer for
        // the same tag, reached from a different pass; this proves the
        // fourth pass reaches it too, through `in_a_group_before` rather than
        // the array.
        let broken = trade_capture_report("1", "D", "ZZ");
        let mut broken_idx: FieldIndex<256> = FieldIndex::new();
        let _ = parse_into::<Fixt11Fix50Sp2Tables, 256>(&broken, &mut broken_idx, Validation::ALL);
        assert_eq!(
            validate::<Fixt11Fix50Sp2Tables, 256>(&broken_idx.view(&broken), b"AE"),
            Some(SessionText::ValueIsIncorrect),
            "the full-array fallback must still inspect a deferred member's value"
        );

        (
            validate_allocs,
            group_allocs,
            logon_allocs,
            trade_capture_allocs,
        )
    };

    println!(
        "allocations: accept {accept_allocs} refuse {refuse_allocs} \
         tick {tick_allocs} beat {beat_allocs} answer {answer_allocs} \
         gap {gap_allocs} fill {fill_allocs} deliver {deliver_allocs} \
         resend {resend_allocs} logon_out {logon_out_allocs} \
         originate {originate_allocs} ordered {ordered_allocs} \
         clock {clock_allocs} text {text_allocs} \
         schedule-open {schedule_open_allocs} schedule-shut {schedule_shut_allocs} \
         next-expected {next_expected_allocs}"
    );
    // An array, not a tuple: `Debug` and `PartialEq` stop at twelve.
    assert_eq!(
        [
            accept_allocs,
            refuse_allocs,
            tick_allocs,
            beat_allocs,
            answer_allocs,
            gap_allocs,
            fill_allocs,
            deliver_allocs,
            resend_allocs,
            logon_out_allocs,
            originate_allocs,
            ordered_allocs,
            clock_allocs,
            text_allocs,
            schedule_open_allocs,
            schedule_shut_allocs,
            next_expected_allocs
        ],
        [0; 17],
        "non-negotiable 1: the session layer allocates nothing, on any path"
    );

    // Counted and asserted on their own rather than folded into the array
    // above, so the seventeen FIX 4.4 cases read identically whether the
    // feature is on or off.
    #[cfg(feature = "fix50sp2")]
    {
        println!(
            "allocations: validate NewOrderSingle (FIXT tables) {fixt_validate_allocs} \
             validate NewOrderSingle (populated group) {fixt_group_allocs} \
             encode Logon (FIXT, 1137) {fixt_logon_allocs} \
             validate TradeCaptureReport (33 groups) {fixt_trade_capture_allocs}"
        );
        assert_eq!(
            [
                fixt_validate_allocs,
                fixt_group_allocs,
                fixt_logon_allocs,
                fixt_trade_capture_allocs
            ],
            [0; 4],
            "non-negotiable 1: the FIXT 1.1 paths allocate nothing either"
        );
    }
}
