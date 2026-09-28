//! Non-negotiable 1, over a dictionary the application generated itself
//! (ADR-0207; plan `2026-09-26-docs-for-embedders` step 27).
//!
//! `crates/library/benches/alloc.rs` proves the handler layer allocates
//! nothing over FIX 4.4. This asks the same of [`Desk`] over `Venue`: a
//! generated type's tables are `static`s and its lookups are `match`es, so a
//! reply carrying the venue's own repeating group — read through
//! `GroupIter<Venue>`, ordered by `Venue::group_order` — must stay at zero.
//!
//! Three cases, each proven to take its path before it is counted:
//!
//! * `venue-reply` — an order with two `NoVenueFees` entries, answered with an
//!   `ExecutionReport` echoing them in the declared order;
//! * `venue-silent` — the same order, declined;
//! * `venue-reject` — a message type the desk does not take, answered with a
//!   business reject.
//!
//! And the control every alloc bench in this repository carries: the same
//! reply with one `to_vec()` in it, which must read non-zero in the same run
//! (`docs/reference/a-benchmark-measured-its-own-fixture.md`).
//!
//! # The `unsafe` here
//!
//! The counting allocator of the other alloc benches: every method forwards to
//! `System` unchanged but for a relaxed counter, and this is a benchmark binary
//! that nothing ships.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
#![allow(unsafe_code)]
// A benchmark binary: non-negotiable 7 is about what ships.
#![allow(clippy::indexing_slicing)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

use custom_dictionary::Desk;
use custom_dictionary::venue::Venue;
use fixbolt::{Answer, App, Application, Handler, Incoming, Reply};

static ALLOCS: AtomicUsize = AtomicUsize::new(0);

struct Counting;

// SAFETY: every method forwards to `System`, a correct allocator, with the
// pointer, layout and size it was given; the only addition is a relaxed counter.
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

const STAMP: &[u8] = b"20260928-10:00:00.123";
const ROUNDS: u32 = 1_000;

/// A whole message with a correct `9=` and `10=`, built before counting starts.
fn framed(body: &str) -> Vec<u8> {
    let whole = format!("8=FIX.4.4\u{1}9={}\u{1}{body}", body.len());
    let sum: u32 = whole.bytes().map(u32::from).sum();
    format!("{whole}10={:03}\u{1}", sum % 256).into_bytes()
}

/// An order carrying the venue's own group, amount first as declared.
fn order() -> Vec<u8> {
    framed(
        "35=D\u{1}34=2\u{1}49=TW44\u{1}52=20260928-10:00:00\u{1}56=ISLD\u{1}\
         11=ORD-1\u{1}21=1\u{1}38=100\u{1}40=Z\u{1}54=1\u{1}55=IBM\u{1}\
         60=20260928-10:00:00\u{1}5001=ACCT-7\u{1}\
         5003=2\u{1}5005=0.25\u{1}5004=1\u{1}5005=0.10\u{1}5004=2\u{1}",
    )
}

/// A `QuoteRequest`, which [`Desk`] does not take.
fn quote_request() -> Vec<u8> {
    framed(
        "35=R\u{1}34=3\u{1}49=TW44\u{1}52=20260928-10:00:00\u{1}56=ISLD\u{1}\
         131=Q-1\u{1}146=0\u{1}",
    )
}

/// [`Desk`] with one heap allocation in front of it. The control.
#[derive(Default)]
struct LeakyDesk(Desk);

impl Handler<256, 64, 1024, Venue> for LeakyDesk {
    fn on_message(
        &mut self,
        msg: &Incoming<'_, 256, Venue>,
        reply: Reply<'_, 64, 1024, Venue>,
    ) -> Answer {
        let copied = msg.get(11).unwrap_or(b"").to_vec();
        std::hint::black_box(&copied);
        self.0.on_message(msg, reply)
    }
}

/// Declines everything, over the same dictionary.
struct Mute;

impl Handler<256, 64, 1024, Venue> for Mute {
    fn on_message(
        &mut self,
        _msg: &Incoming<'_, 256, Venue>,
        reply: Reply<'_, 64, 1024, Venue>,
    ) -> Answer {
        reply.silent()
    }
}

type Over<H> = App<H, 256, 64, 1024, Venue>;

/// Drive `n` messages through an application; the last reply's range, if any.
fn drive<A: Application>(
    app: &mut A,
    wire: &[u8],
    out: &mut [u8],
    n: u32,
) -> (u32, Option<std::ops::Range<usize>>) {
    let mut sent = 0;
    let mut last = None;
    for seq in 1..=n {
        last = app.on_message(
            wire,
            fixbolt_session::Header {
                seq,
                stamp: STAMP,
                last_processed: None,
            },
            out,
        );
        if last.is_some() {
            sent += 1;
        }
    }
    (sent, last)
}

fn main() {
    let order = order();
    let quote = quote_request();
    let mut out = [0u8; 4096];

    let mut desk = Over::<Desk>::with_sizes(Desk::default());
    let mut leaky = Over::<LeakyDesk>::with_sizes(LeakyDesk::default());
    let mut mute = Over::<Mute>::with_sizes(Mute);
    let mut refuser = Over::<Desk>::with_sizes(Desk::default());

    // Each path proven to be the path it claims before anything is counted: the
    // reply carries the venue's group, echoed in its declared order.
    let (sent, range) = drive(&mut desk, &order, &mut out, 1);
    assert_eq!(sent, 1, "the replying path must actually reply");
    let wire = String::from_utf8_lossy(&out[range.expect("a reply")]).replace('\u{1}', "|");
    assert!(
        wire.contains("|5003=2|5005=0.25|5004=1|5005=0.10|5004=2|"),
        "the reply must carry NoVenueFees in Venue's declared order, or this \
         case measures another path: {wire}"
    );
    assert_eq!(
        drive(&mut leaky, &order, &mut out, 1).0,
        1,
        "the control replies too"
    );
    assert_eq!(
        drive(&mut mute, &order, &mut out, 1).0,
        0,
        "the silent path declines"
    );
    let (sent, range) = drive(&mut refuser, &quote, &mut out, 1);
    assert_eq!(sent, 1, "the reject path must write a message");
    let wire = String::from_utf8_lossy(&out[range.expect("a reject")]).replace('\u{1}', "|");
    assert!(
        wire.contains("|35=j|"),
        "and it must be a business reject: {wire}"
    );

    let mut replied = 0;
    let reply_allocs = count(|| {
        replied = drive(&mut desk, &order, &mut out, ROUNDS).0;
    });
    assert_eq!(
        replied, ROUNDS,
        "the counted window must hold {ROUNDS} replies"
    );

    let silent_allocs = count(|| {
        drive(&mut mute, &order, &mut out, ROUNDS);
    });

    let mut refused = 0;
    let reject_allocs = count(|| {
        refused = drive(&mut refuser, &quote, &mut out, ROUNDS).0;
    });
    assert_eq!(
        refused, ROUNDS,
        "every quote request must have been refused"
    );

    let control_allocs = count(|| {
        drive(&mut leaky, &order, &mut out, ROUNDS);
    });

    println!(
        "allocations: venue-reply {reply_allocs} venue-silent {silent_allocs} \
         venue-reject {reject_allocs} control-injected {control_allocs}"
    );
    assert_eq!(
        [reply_allocs, silent_allocs, reject_allocs],
        [0, 0, 0],
        "a handler over a generated dictionary allocated on a per-message path"
    );
    assert!(
        control_allocs >= ROUNDS as usize,
        "the injected control read {control_allocs} for {ROUNDS} messages — the \
         counter is not seeing this path, so the zeros above are not evidence"
    );
}
