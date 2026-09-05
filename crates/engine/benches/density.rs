//! What a message costs when there are N sessions on the thread, and where the
//! cache wall actually is.
//!
//! Steps B1-B3 of `plans/2026-09-05-what-is-left-and-what-a-message-touches.md`,
//! for the open half of `STATUS.md` item 14.
//!
//! # The question, and why the old arithmetic is void
//!
//! `docs/reference/measured-costs.md` asks how much of a `Connection` a message
//! touches, and answers with two bounds 14x apart: the L2 edge at **N ~= 9** if a
//! message touches all of it, at **N ~= 128** if it touches 4 KiB. Both were
//! computed from `size_of::<Connection<..>>()` = **54 600 bytes**, measured
//! 2026-08-30.
//!
//! `[measured 2026-09-05]` that number is **21 456 bytes**. [ADR-0046] boxed the
//! journal ring on 2026-09-04, so `size_of::<MemJournal<64,512>>()` went from
//! 33 288 to **32**, and the 2 MiB of `Store` moved to the heap. The bold
//! sentence "one connection does not fit in L1" is now false — 20.95 KiB fits a
//! 32 KiB L1d with room to spare.
//!
//! So the bounds are not narrowed here, they are **replaced**. And rather than
//! measure a footprint and infer a wall through a latency model, this measures
//! the wall.
//!
//! # Two sweeps, because the ring left the struct
//!
//! * **B-i** — `engine turn, N busy sessions`, N from 1 to 64, with the ring
//!   pinned small (`MemJournal<8, 512>`, 4 KiB of heap). Whatever wall appears is
//!   the **struct's**.
//! * **B-ii** — one session, ring at 8, 64, 512 and 4096 slots. Identical work,
//!   4 KiB to 2 MiB of memory behind it. That prices the ring where it sits,
//!   which `benches/journal.rs` deliberately could not: a 512-byte stride in a
//!   tight loop is the friendliest thing a prefetcher ever sees, and an engine
//!   does not present it that way.
//!
//! Figures are **per turn**, as in `benches/turn.rs`. Per message is the figure
//! divided by N, and the division is done in the documents rather than here so
//! that a baseline is compared against the thing that was measured.
//!
//! # `Feed`, and why not `Loopback`
//!
//! [`Loopback`](fixbolt_engine::transport::Loopback) keeps bytes in a
//! `VecDeque<u8>`. It allocates as it grows, it charges per byte to push and
//! pop, and — the one that matters — **it gives every connection a heap buffer
//! of its own**. This is a cache measurement; running it over a fake with its
//! own working set measures the fake.
//!
//! `Feed` copies one message into the caller's buffer and returns, which is the
//! same class of work a real `read()` does minus the syscall, and it counts and
//! discards what is sent back.
//!
//! **The sequence number is patched in place**, at a fixed width of eight
//! digits, with the checksum carried incrementally. That is not an optimisation:
//! `[measured 2026-08-30]`, recorded in `benches/alloc.rs`, replaying one
//! message makes the session refuse a sequence number already used and drop the
//! link, and from the third iteration such a bench measures an engine with no
//! connections.
//!
//! # What this does NOT measure
//!
//! * **A syscall.** `Feed` has no kernel in it, deliberately and unlike
//!   `benches/turn.rs`: `recv on a quiet socket` is 418.5 ns on this box and
//!   would bury a cache term of tens of nanoseconds. The syscall is measured in
//!   `turn.rs`, and this is the other half.
//! * **Sharding.** N here is sessions on **one** engine.
//! * **An application doing work.** The desk copies a prepared
//!   `ExecutionReport` body. Its cost is constant in N and is not part of a
//!   `Connection`.
//!
//! [ADR-0046]: ../../../docs/decisions/ADR-0046-the-ring-is-the-resend-store-and-a-replay-goes-in-batches.md
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

#[path = "../../codec/benches/harness.rs"]
mod harness;

use std::hint::black_box;
use std::ops::Range;

use fixbolt_engine::clock::ManualClock;
use fixbolt_engine::dispatch::InlineDispatch;
use fixbolt_engine::journal::MemJournal;
use fixbolt_engine::transport::{Io, Transport};
use fixbolt_engine::wait::Yield;
use fixbolt_engine::{Application, Config, Engine};

/// The instant the acceptance corpus and every engine bench here run at. A
/// `SendingTime` away from the engine's clock is refused for skew, and the bench
/// would then measure the refusal path and call it the message path.
const STAMP: &str = "20260828-12:00:00.000";

/// Width of the `34=` field in the fed order, in digits.
///
/// Fixed so the message length — and therefore `9=` — never changes, which is
/// what lets the sequence number be patched rather than the message rebuilt.
/// Eight digits reaches 99 999 999; the harness asks for 1.41 million.
const SEQ_DIGITS: usize = 8;

/// `8=`, `9=` and `10=` around a body, with both computed.
fn frame(body: &str) -> Vec<u8> {
    let head = format!("8=FIX.4.4\x019={}\x01", body.len());
    let mut out = head.into_bytes();
    out.extend_from_slice(body.as_bytes());
    let sum: u32 = out.iter().map(|b| u32::from(*b)).sum();
    out.extend_from_slice(format!("10={:03}\x01", sum % 256).as_bytes());
    out
}

/// Every session on this engine needs its **own** counterparty, at a fixed
/// width so that no message changes length with `N`.
///
/// `[measured 2026-09-05]` an `Engine` enforces "is this identity already
/// logged on", so two connections claiming `49=W2W` are one connection: the
/// first sweep written here added two and `connections()` read **1**. That is
/// the engine being right, and `benches/turn.rs` never met it because its
/// sessions are idle and never log on at all.
fn peer(i: usize) -> String {
    format!("W2W{i:05}")
}

/// The `NewOrderSingle` `tools/w2w --path app` sends, field for field, with the
/// sequence number widened to a fixed eight digits and the counterparty to
/// eight characters.
fn order(seq: u32, i: usize) -> Vec<u8> {
    frame(&format!(
        "35=D\x0134={seq:0SEQ$}\x0149={who}\x0152={STAMP}\x0156=ISLD\x0111=W1\x0121=1\x01\
         38=002000.00\x0140=2\x0144=20.15\x0154=1\x0155=INTC\x0160={STAMP}\x01",
        SEQ = SEQ_DIGITS,
        who = peer(i)
    ))
}

fn logon(i: usize) -> Vec<u8> {
    frame(&format!(
        "35=A\x0134={:0width$}\x0149={who}\x0152={STAMP}\x0156=ISLD\x0198=0\x01108=30\x01",
        1,
        width = SEQ_DIGITS,
        who = peer(i)
    ))
}

/// One session's wire, without a kernel and without a queue.
struct Feed {
    logon: Vec<u8>,
    order: Vec<u8>,
    /// Where the eight `34=` digits start in `order`.
    seq_at: usize,
    /// Where the three `10=` digits start in `order`.
    sum_at: usize,
    /// Byte sum of `order` up to `sum_at`, kept in step with the patches.
    sum: u32,
    /// `false` until the Logon has been handed over.
    logged_on: bool,
    /// Bytes the engine wrote back. Read outside the timed region.
    sent: usize,
}

impl Feed {
    fn new(i: usize) -> Self {
        // Built at 1, not 2: `recv` advances BEFORE it hands the order over,
        // so the first order out carries 2 — the number the session expects
        // after the Logon spent 1. Built at 2 the first order is a 3, the
        // session answers `MsgSeqNum too low` and drops the link, and the whole
        // sweep then times an engine with no connections.
        let order = order(1, i);
        let seq_at = find(&order, b"\x0134=") + 4;
        let sum_at = order.len() - 4;
        // **Up to `10=`, not up to its digits.** FIX's checksum covers every
        // byte before the `10=` field itself, so the `1`, `0` and `=` are
        // outside it. Summing to `sum_at` puts those three bytes in and every
        // message is then rejected for a bad checksum — which is exactly what
        // the first run of this bench did.
        let sum = order[..sum_at - 3].iter().map(|b| u32::from(*b)).sum();
        Self {
            logon: logon(i),
            order,
            seq_at,
            sum_at,
            sum,
            logged_on: false,
            sent: 0,
        }
    }

    /// Add one to the eight decimal digits at `seq_at`, and carry the change
    /// into `sum` rather than re-summing 149 bytes.
    ///
    /// A digit that goes `d` to `d+1` adds one to the sum; a `9` that becomes a
    /// `0` takes nine away and carries left.
    fn advance(&mut self) {
        let mut i = self.seq_at + SEQ_DIGITS;
        while i > self.seq_at {
            i -= 1;
            if self.order[i] == b'9' {
                self.order[i] = b'0';
                self.sum -= 9;
            } else {
                self.order[i] += 1;
                self.sum += 1;
                break;
            }
        }
        let c = self.sum % 256;
        self.order[self.sum_at] = b'0' + u8::try_from(c / 100).unwrap_or(0);
        self.order[self.sum_at + 1] = b'0' + u8::try_from((c / 10) % 10).unwrap_or(0);
        self.order[self.sum_at + 2] = b'0' + u8::try_from(c % 10).unwrap_or(0);
    }
}

fn find(haystack: &[u8], needle: &[u8]) -> usize {
    haystack
        .windows(needle.len())
        .position(|w| w == needle)
        .expect("the field is in the message this file just built")
}

impl Transport for Feed {
    const POLLABLE: bool = false;

    fn recv(&mut self, buf: &mut [u8]) -> Io {
        let src = if self.logged_on {
            self.advance();
            &self.order
        } else {
            self.logged_on = true;
            &self.logon
        };
        let n = src.len();
        buf[..n].copy_from_slice(src);
        Io::Ready(n)
    }

    fn send(&mut self, buf: &[u8]) -> Io {
        self.sent += buf.len();
        Io::Ready(buf.len())
    }
}

/// Answers every order with the same prepared `ExecutionReport` body.
///
/// **Deliberately not `tools/w2w`'s `Desk`.** That one parses the order and
/// patches a template, which is `DESIGN.md` D9's cost and is already priced by
/// `crates/codec/benches/serialize.rs`. Here it would be a constant added to
/// every point of the sweep, obscuring the term the sweep exists to find.
struct Desk {
    reply: Vec<u8>,
    seen: u64,
}

impl Desk {
    fn new() -> Self {
        Self {
            // `34=`, `52=`, `9=` and `10=` are rewritten by the session — see
            // `Application::on_message` — so the placeholders here are honest
            // rather than wrong.
            reply: frame(
                "35=8\x0134=0\x0149=ISLD\x0152=20260828-12:00:00.000\x0156=W2W\x01\
                 37=E0000001\x0117=X0000001\x01150=F\x0139=2\x0111=W1\x0155=INTC\x0154=1\x01\
                 38=002000.00\x0132=002000.00\x0131=20.15\x01151=0\x0114=002000.00\x016=20.15\x01",
            ),
            seen: 0,
        }
    }
}

impl Application for Desk {
    fn on_message(&mut self, _: &[u8], _: u32, _: &[u8], out: &mut [u8]) -> Option<Range<usize>> {
        self.seen += 1;
        let n = self.reply.len();
        out.get_mut(..n)?.copy_from_slice(&self.reply);
        Some(0..n)
    }
}

type Dense<const SLOTS: usize> = Engine<
    Feed,
    fixbolt_session::Acceptor,
    InlineDispatch<Desk>,
    ManualClock,
    Yield,
    MemJournal<SLOTS, 512>,
    64,
    4096,
    8192,
>;

/// `n` sessions, each logged on and each with an order waiting.
///
/// Returns the engine only after proving the path runs: `n` orders delivered to
/// the application on one turn, and `n` connections still alive afterwards. **A
/// sweep whose sessions never logged on is flat, fast and meaningless**, and it
/// is the failure this assertion exists for.
fn engine_with<const SLOTS: usize>(n: usize) -> Dense<SLOTS> {
    let mut engine: Dense<SLOTS> = Engine::new(
        Config::acceptor(b"FIX.4.4", b"ISLD", b"W2W"),
        InlineDispatch::new(Desk::new()),
        ManualClock::at(fixbolt_conformance::script::FIXED_TIME_MILLIS),
        Yield,
        n.max(1),
    );
    for i in 0..n {
        engine
            .add_with_prefix_and_config(
                Feed::new(i),
                Config::acceptor(b"FIX.4.4", b"ISLD", peer(i).as_bytes()),
                &[],
            )
            .expect("an empty prefix fits any RX");
    }
    assert_eq!(engine.connections(), n, "the sweep must have {n} sessions");

    // Turn one is the Logon and its answer; from turn two every session hands
    // over an order.
    engine.turn();
    let before = engine.dispatch_mut().handler_mut().seen;
    engine.turn();
    let after = engine.dispatch_mut().handler_mut().seen;
    assert_eq!(
        after - before,
        n as u64,
        "each of the {n} sessions must deliver exactly one order per turn, \
         or this sweep is measuring sessions that were dropped at logon"
    );
    assert_eq!(
        engine.connections(),
        n,
        "and must still hold {n} sessions after two turns"
    );
    engine
}

fn main() {
    harness::suite(|b| {
        // B-i. The ring is pinned at 8 slots — 4 KiB of heap — so that what the
        // sweep sees is the `Connection` and not the journal.
        // 128 was measured once and then removed: the sweep costs O(N), the
        // last point alone is most of a run, and a baseline needs twenty clean
        // runs. Its figure is NOT quoted anywhere — non-negotiable 10 asks for
        // the committed benchmark that produced a number, and a case that is no
        // longer in the repository is not one. What it cost is written into
        // "Sửa 4" of the plan: the touched-set estimate is now a range rather
        // than a figure. What it did not cost is the answer — a message
        // touching the whole struct would put a STEP at N ~= 20, and there is
        // none.
        for n in [1usize, 2, 4, 8, 16, 32, 64] {
            let mut engine = engine_with::<8>(n);
            b.bench(&format!("engine turn, {n} busy sessions"), || {
                black_box(engine.turn());
            });
        }

        // B-ii. One session, identical work, the ring from 4 KiB to 2 MiB.
        // `Store` — what `tools/w2w` runs — is the last of these.
        {
            let mut e64 = engine_with::<64>(1);
            b.bench("engine turn, 1 busy, ring 64", || {
                black_box(e64.turn());
            });
            let mut e512 = engine_with::<512>(1);
            b.bench("engine turn, 1 busy, ring 512", || {
                black_box(e512.turn());
            });
            let mut e4096 = engine_with::<4096>(1);
            b.bench("engine turn, 1 busy, ring 4096", || {
                black_box(e4096.turn());
            });
        }
    });
}
