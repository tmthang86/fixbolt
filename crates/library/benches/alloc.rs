//! Non-negotiable 1, for the layer an application actually writes against.
//!
//! `crates/codec/benches/alloc.rs` proves it for parse and encode,
//! `crates/session/benches/alloc.rs` for the state machine, and
//! `crates/engine/benches/alloc.rs` for the layer that touches the socket. This
//! proves it for the convenience layer — the one where a `Vec` for a reply, a
//! `String` for an exec id, or a `format!` in an error path would be easiest to
//! reach for, and where the whole design would be undone by any of the three.
//!
//! # Every case carries its own control
//!
//! `[2026-09-02]` a zero from a counting benchmark and a zero from a benchmark
//! that ran nothing are the same output —
//! `docs/reference/a-benchmark-measured-its-own-fixture.md`, and the process
//! rule that came out of it. So each case here is followed by an **injection**:
//! the same window with a `to_vec()` in it. A case is evidence only when its
//! own control reads non-zero in the same run.
//!
//! # The `unsafe` here
//!
//! Identical to the other three benches and sound for the same three reasons:
//! every method forwards to `System` unchanged but for a relaxed counter; this
//! is a benchmark binary, so nothing ships it; and it is proven by reversal,
//! not by reading.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
#![allow(unsafe_code)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

use fixbolt::{Answer, App, Application, Handler, Incoming, Reply};

static ALLOCS: AtomicUsize = AtomicUsize::new(0);

struct Counting;

// SAFETY: every method forwards to `System`, which is a correct allocator, with
// the same pointer, layout and size it was given. The only addition is a
// relaxed counter increment. See the module comment.
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

/// `52` as the session hands it over: 21 bytes, milliseconds included.
const STAMP: &[u8] = b"20260902-10:00:00.123";

/// A whole `NewOrderSingle` with a correct `9=` and `10=`.
///
/// Built once, before counting starts. A hand-written frame one byte out makes
/// `parse_into` bail and the bench then reports zero for a path it never
/// walked — the trap `crates/session/benches/alloc.rs` names in the same words.
fn order() -> Vec<u8> {
    let body = "35=D\u{1}34=2\u{1}49=TW44\u{1}52=20260902-10:00:00\u{1}56=ISLD\u{1}\
                11=ORD-1\u{1}21=1\u{1}38=100\u{1}40=2\u{1}44=42\u{1}54=1\u{1}55=IBM\u{1}\
                59=0\u{1}60=20260902-10:00:00\u{1}";
    let whole = format!("8=FIX.4.4\u{1}9={}\u{1}{body}", body.len());
    let sum: u32 = whole.bytes().map(u32::from).sum();
    format!("{whole}10={:03}\u{1}", sum % 256).into_bytes()
}

/// The handler under test: reads six fields, echoes three, renders one number.
///
/// The same shape as `examples/shared/order_handler.rs`. Written out rather
/// than `#[path]`-included because a bench must not depend on an example
/// staying the shape a bench needs.
#[derive(Default)]
struct Desk {
    fills: u32,
}

impl Handler for Desk {
    fn on_message(&mut self, msg: &Incoming<'_>, reply: Reply<'_>) -> Answer {
        if msg.msg_type() != b"D" {
            return reply.silent();
        }
        self.fills += 1;
        let mut buf = [0u8; 16];
        let id = render(self.fills, &mut buf);
        reply
            .message(b"8")
            .field(37, id)
            .field(17, id)
            .field(150, b"F")
            .field(39, b"2")
            .field(11, msg.get(11).unwrap_or(b""))
            .field(55, msg.get(55).unwrap_or(b""))
            .field(54, msg.get(54).unwrap_or(b""))
            .field(38, msg.get(38).unwrap_or(b""))
            .field(31, msg.get(44).unwrap_or(b""))
            .field(151, b"0")
            .send()
    }
}

/// A desk that refuses every order it is given, through `Reply::business_reject`.
///
/// `[added 2026-09-05]` The four-tag `35=j` helper renders two numbers into
/// stack buffers and writes four fields; nothing about that has to allocate,
/// and this is what says it does not rather than a comment saying it should
/// not — `CLAUDE.md` §2 rule 1.
#[derive(Default)]
struct Refuser {
    refused: u32,
}

impl Handler for Refuser {
    fn on_message(&mut self, msg: &Incoming<'_>, reply: Reply<'_>) -> Answer {
        if msg.msg_type() != b"D" {
            return reply.silent();
        }
        self.refused += 1;
        reply
            .business_reject(
                msg.seq().unwrap_or_default(),
                msg.msg_type(),
                2,
                b"not a security we trade",
            )
            .send()
    }
}

/// The same handler with **one `to_vec()` in it**. The control.
#[derive(Default)]
struct LeakyDesk {
    inner: Desk,
}

impl Handler for LeakyDesk {
    fn on_message(&mut self, msg: &Incoming<'_>, reply: Reply<'_>) -> Answer {
        // One heap allocation, on the path the case above claims is clean. If
        // `handler-reply` can read 0 while this reads 0 too, neither number is
        // about allocation.
        let copied = msg.get(11).unwrap_or(b"").to_vec();
        std::hint::black_box(&copied);
        self.inner.on_message(msg, reply)
    }
}

/// A handler that answers nothing, for the path where the application declines.
struct Mute;

impl Handler for Mute {
    fn on_message(&mut self, _msg: &Incoming<'_>, reply: Reply<'_>) -> Answer {
        reply.silent()
    }
}

fn render(mut v: u32, buf: &mut [u8; 16]) -> &[u8] {
    buf[..5].copy_from_slice(b"EXEC-");
    let mut digits = [0u8; 10];
    let mut i = 10;
    if v == 0 {
        i = 9;
        digits[9] = b'0';
    }
    while v > 0 && i > 0 {
        i -= 1;
        digits[i] = b'0' + u8::try_from(v % 10).unwrap_or(0);
        v /= 10;
    }
    let len = 10 - i;
    buf[5..5 + len].copy_from_slice(&digits[i..]);
    &buf[..5 + len]
}

/// Drive `n` messages through an application, counting nothing but that.
fn drive<A: Application>(app: &mut A, wire: &[u8], out: &mut [u8], n: u32) -> u32 {
    let mut sent = 0;
    for seq in 1..=n {
        if app.on_message(wire, seq, STAMP, out).is_some() {
            sent += 1;
        }
    }
    sent
}

const ROUNDS: u32 = 1_000;

fn main() {
    let wire = order();
    let mut out = [0u8; 4096];

    // Everything constructed before counting starts. What is measured is the
    // path a message takes, not the building of the application.
    let mut desk = App::<Desk>::with_sizes(Desk::default());
    let mut leaky = App::<LeakyDesk>::with_sizes(LeakyDesk::default());
    let mut mute = App::<Mute>::with_sizes(Mute);
    let mut garbage_app = App::<Desk>::with_sizes(Desk::default());
    let mut refuser = App::<Refuser>::with_sizes(Refuser::default());

    // Warm anything lazy, and prove each path is the path it claims to be — so
    // a zero below means "did not allocate" rather than "did not run".
    assert_eq!(
        drive(&mut desk, &wire, &mut out, 1),
        1,
        "the replying path must actually reply"
    );
    assert_eq!(
        drive(&mut leaky, &wire, &mut out, 1),
        1,
        "the control must take the same path, not a shorter one"
    );
    assert_eq!(
        drive(&mut mute, &wire, &mut out, 1),
        0,
        "the silent path must actually decline"
    );
    assert_eq!(
        drive(&mut garbage_app, b"not a fix message", &mut out, 1),
        0,
        "the unparsable path must actually fail to parse"
    );
    assert_eq!(
        garbage_app.unparsable(),
        1,
        "and it must say so — an uncounted refusal is one nobody can explain"
    );

    assert_eq!(
        drive(&mut refuser, &wire, &mut out, 1),
        1,
        "the business-reject path must actually write a message"
    );

    let mut replied = 0;
    let reply_allocs = count(|| {
        replied = drive(&mut desk, &wire, &mut out, ROUNDS);
    });
    assert_eq!(
        replied, ROUNDS,
        "the counted window must contain {ROUNDS} replies, not {replied}"
    );

    let mut leaked = 0;
    let control_allocs = count(|| {
        leaked = drive(&mut leaky, &wire, &mut out, ROUNDS);
    });
    assert_eq!(leaked, ROUNDS, "the control must reply as often");

    let silent_allocs = count(|| {
        drive(&mut mute, &wire, &mut out, ROUNDS);
    });

    let mut refused = 0;
    let reject_allocs = count(|| {
        refused = drive(&mut refuser, &wire, &mut out, ROUNDS);
    });
    assert_eq!(refused, ROUNDS, "every order must have been refused");

    let before_unparsable = garbage_app.unparsable();
    let unparsable_allocs = count(|| {
        drive(&mut garbage_app, b"not a fix message", &mut out, ROUNDS);
    });
    assert_eq!(
        garbage_app.unparsable() - before_unparsable,
        u64::from(ROUNDS),
        "the unparsable window must have refused {ROUNDS} messages"
    );

    println!(
        "allocations: handler-reply {reply_allocs} handler-silent {silent_allocs} \
         reject {reject_allocs} unparsable {unparsable_allocs} \
         control-injected {control_allocs}"
    );

    assert_eq!(
        [
            reply_allocs,
            silent_allocs,
            reject_allocs,
            unparsable_allocs
        ],
        [0, 0, 0, 0],
        "the library layer allocated on a path an application takes per message"
    );

    // The control is the whole point of the three zeros above. A run where it
    // is also zero is a run that measured nothing, and reporting the zeros
    // would be reporting a false green.
    assert!(
        control_allocs >= ROUNDS as usize,
        "the injected control read {control_allocs} allocations for {ROUNDS} \
         messages — the counter is not seeing this path, so the zeros above are \
         not evidence"
    );
}
