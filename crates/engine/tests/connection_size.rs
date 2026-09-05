//! What one connection costs in memory, as a fact rather than as a sentence.
//!
//! **Step 0 of [settings-for-both-roles].** That step decides *not* to raise
//! `RX` from 4 KiB, and the decision rests on two numbers that were prose in
//! three documents and asserted nowhere: raising `RX` costs exactly the buffer,
//! and the 2 MiB resend ring is on the heap rather than inside the struct.
//! [ADR-0055] publishes both. This file is what makes them still true tomorrow.
//!
//! # Why a `size_of` needs a test at all
//!
//! `size_of` is a compile-time fact, so it cannot drift on its own — it drifts
//! when somebody adds a field. That already happened twice here without anybody
//! noticing:
//!
//! * `docs/reference/measured-costs.md` computed a cache wall from
//!   `size_of::<Connection<..>>() = 54 600`, measured 2026-08-30. [ADR-0046]
//!   boxed the journal ring on 2026-09-04 and the real figure became 21 456 —
//!   **a 2.5x error carried in a document for five days**, found by
//!   `benches/density.rs` and not by anything that ran.
//! * The plan's own figure, 23 752 bytes, was measured 2026-09-04. It reads
//!   **23 760** here: eight bytes arrived with wave A. Nothing was wrong with
//!   the number when it was written, and nothing told the person who read it
//!   next.
//!
//! Both numbers were *arithmetic inputs* in a document. `CLAUDE.md` §4 —
//! **prose does not hold a constraint**.
//!
//! # What is asserted, and what is deliberately not
//!
//! The absolute size is **printed, not asserted**. Pinning it would make every
//! honest field addition fail a test that has no opinion about field additions,
//! which is a gate that gets deleted the third time it fires.
//!
//! What is asserted is the one thing the decision was taken on: **raising `RX`
//! costs the buffer and nothing else.** No padding appears, so the arithmetic in
//! [ADR-0055] — *four times the buffer for +0.57% of a session* — is a
//! subtraction and not an estimate.
//!
//! # The second test that was written here, and deleted
//!
//! `[measured 2026-09-05]` This file first carried a second test asserting that
//! the 2 MiB resend ring is on the heap, since every figure above assumes it.
//! Its reversal — putting `slots: [Slot<LEN>; N]` back inline — **did not turn
//! it red. It stopped the crate compiling**, at
//! `journal.rs:110`'s `const _: () = assert!(size_of::<Store>() <= 64);`, whose
//! own comment reads *"going back to an inline array is a compile error, not a
//! test"*.
//!
//! So the test could not fail while the crate it tested existed. It was not a
//! weak guard, it was **no guard**, and it would have read as coverage forever.
//! A reversal is what told the difference, which is the whole reason
//! `CLAUDE.md` §10 asks for one.
//!
//! [settings-for-both-roles]: ../../../docs/plans/2026-09-04-settings-for-both-roles.md
//! [ADR-0046]: ../../../docs/decisions/ADR-0046-the-ring-is-the-resend-store-and-a-replay-goes-in-batches.md
//! [ADR-0055]: ../../../docs/decisions/ADR-0055-max-message-size-is-not-a-key-and-rx-is-the-answer.md

use fixbolt_engine::conn::Connection;
use fixbolt_engine::journal::Store;
use fixbolt_engine::transport::TcpTransport;
use fixbolt_session::Acceptor;

/// One acceptor connection at the shipped sizes, `RX` left to the caller.
///
/// `256`, `8192` and `1024` are the defaults every front door passes
/// (`serve` → `serve_with::<256, 4096, 8192, 1024, ..>`), so this is the
/// connection a user gets rather than a shape invented for the test.
type Conn<const RX: usize> = Connection<TcpTransport, Acceptor, Store, 256, RX, 8192, 1024>;

#[test]
fn raising_the_receive_buffer_costs_the_buffer_and_no_padding() {
    let small = core::mem::size_of::<Conn<4096>>();
    let large = core::mem::size_of::<Conn<16384>>();
    println!("Connection RX=4096  : {small} bytes");
    println!("Connection RX=16384 : {large} bytes");

    assert_eq!(
        large - small,
        16384 - 4096,
        "a bigger receive buffer grew the connection by more than the buffer, \
         so ADR-0055's cost arithmetic is no longer a subtraction"
    );
}
