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

use nanofix_conformance::script::{Kind, scenarios};
use nanofix_session::text::SessionText;
use nanofix_session::{Acceptor, Config, Link, Session, clock};

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
    scenarios()
        .expect("corpus")
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

    let accept_allocs = count(|| {
        for _ in 0..10_000 {
            session.received(&good, |_| ());
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
         tick {tick_allocs} clock {clock_allocs} text {text_allocs}"
    );
    assert_eq!(
        (
            accept_allocs,
            refuse_allocs,
            tick_allocs,
            clock_allocs,
            text_allocs
        ),
        (0, 0, 0, 0, 0),
        "non-negotiable 1: the session layer allocates nothing, on any path"
    );
}
