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

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

use nanofix_conformance::echo::echo;
use nanofix_conformance::script::{Kind, scenarios};
use nanofix_session::text::SessionText;
use nanofix_session::{Acceptor, Application, Config, Link, Session, clock};

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
        seq: u32,
        stamp: &[u8],
        out: &mut [u8],
    ) -> Option<core::ops::Range<usize>> {
        echo(msg, out, seq, stamp).ok()
    }
}

fn acceptor() -> Session<Acceptor, 256> {
    Session::new(Config::acceptor(b"FIX.4.4", b"ISLD", b"TW44"))
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
        s.connect(|_| ());
        s.tick(now, |_| ());
        s.received(order_logon, |_| ());
        let mut echoed = 0usize;
        s.received_with(order, &mut EchoApp, |_| echoed += 1);
        assert_eq!(echoed, 1, "the delivery path must actually deliver");
    }

    let deliver_allocs = count(|| {
        for _ in 0..10_000 {
            let mut s = acceptor();
            s.connect(|_| ());
            s.tick(now, |_| ());
            s.received(order_logon, |_| ());
            s.received_with(order, &mut EchoApp, |_| ());
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

    println!(
        "allocations: accept {accept_allocs} refuse {refuse_allocs} \
         tick {tick_allocs} beat {beat_allocs} answer {answer_allocs} \
         gap {gap_allocs} fill {fill_allocs} deliver {deliver_allocs} \
         clock {clock_allocs} text {text_allocs}"
    );
    assert_eq!(
        (
            accept_allocs,
            refuse_allocs,
            tick_allocs,
            beat_allocs,
            answer_allocs,
            gap_allocs,
            fill_allocs,
            deliver_allocs,
            clock_allocs,
            text_allocs
        ),
        (0, 0, 0, 0, 0, 0, 0, 0, 0, 0),
        "non-negotiable 1: the session layer allocates nothing, on any path"
    );
}
