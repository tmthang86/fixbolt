//! What keeping a message for resend costs, and how much of that is the ring
//! being two megabytes.
//!
//! Step A2 of `plans/2026-09-05-what-is-left-and-what-a-message-touches.md`,
//! for `STATUS.md` open item 49.
//!
//! # The question
//!
//! `tools/w2w --path app` costs **3 898 ns** more per round trip than `--path
//! admin`, and ~2 804 ns of that is unattributed. One named candidate is
//! [`Journal::put`] of the outbound `ExecutionReport`: **the administrative path
//! never does it**, because a `Heartbeat` is not kept for resend. So the whole
//! of this figure is a term in that difference, not a fraction of one.
//!
//! # The ring is not where it was recorded
//!
//! `[measured 2026-09-05]` [`Store`] — what `tools/w2w` runs and what
//! `DESIGN.md` D7 means by the default — is `MemJournal<4096, 512>`, and since
//! [ADR-0046] its slots are `Box<[Slot<LEN>]>`. So `size_of::<Store>()` is
//! **32 bytes**, and the **2 MiB** of ring lives on the heap, per connection.
//! `docs/reference/measured-costs.md` recorded 33 288 bytes *inside* the
//! `Connection` on 2026-08-30, which was true when it was written and stopped
//! being true on 2026-09-04.
//!
//! `put` addresses by `seq % 4096`, so consecutive messages land in consecutive
//! slots and a busy session **walks the whole 2 MiB and wraps**. L2 on the §9
//! desktop is 512 KiB and L3 is 32 MiB. Each `put` writes 512 bytes — eight
//! cache lines — into a slot last touched 4 095 messages ago.
//!
//! # Why three cases
//!
//! Two of them differ only in **where** the bytes go, and that difference is the
//! whole point:
//!
//! | Case | What it is |
//! |---|---|
//! | `journal put, 191 bytes, walking` | What the engine actually does: `seq + 1` every time, so every `put` is a different slot and the ring is walked end to end |
//! | `journal put, 87 bytes, walking` | The size control. If the cost were the copy, this case is 104 bytes cheaper and nothing else changes |
//! | `journal put, 191 bytes, one slot` | **The same work, one address.** `seq` is fixed, so slot 0 every time and its eight lines stay in L1 |
//!
//! The third minus the first is what a 2 MiB ring costs, isolated from the copy
//! by holding the copy constant — a reversal placed in the cache domain rather
//! than in a guard. The second minus the first says how much is bytes.
//!
//! # The sizes are measured
//!
//! `[measured 2026-09-05]` `strace -f -e trace=sendto` on `./target/release/w2w`
//! at its default flags: the `ExecutionReport` this journals is **191 bytes** and
//! the `Heartbeat` the administrative path does not journal is **87**. Open item
//! 49 recorded these as "~200" and "~70".
//!
//! # What a tight loop cannot tell you, and where the rest of the answer is
//!
//! `[measured 2026-09-05, first run]` `walking` reads **8.9 ns** and `one slot`
//! **8.2 ns**. Taken at face value that says a 2 MiB ring costs 0.7 ns, and
//! taking it at face value would be wrong.
//!
//! A stride of exactly 512 bytes through 2 MiB, with nothing between one `put`
//! and the next, is the friendliest pattern a hardware prefetcher will ever
//! see. **The engine does not present it that way**: between two `put` calls it
//! reads a socket, frames, parses, runs the session and builds a reply, and
//! those evict lines and interrupt the stream. So `walking` is a **floor** for
//! what `put` costs in an engine, not the figure.
//!
//! The ceiling is arithmetic: a 512-byte slot is eight cache lines, and this
//! machine's measured RAM latency is 78.5 ns, so a `put` whose every line misses
//! to RAM costs at most ~630 ns. **Between 9 and 630 ns is not an answer**, and
//! this bench is deliberately not asked to narrow it: `benches/density.rs` runs
//! a real engine turn and sweeps the ring from 8 slots to 4 096 at one session,
//! which prices the ring where it actually sits. That is step B-ii of the same
//! plan.
//!
//! # What this does NOT measure
//!
//! **A [`FileJournal`].** `Durability::Async` hands the same bytes to a writer
//! thread through a ring, and `Fsync` reaches the disk on the engine thread.
//! Neither is what `tools/w2w` runs, and both belong to wave C.
//!
//! [ADR-0046]: ../../../docs/decisions/ADR-0046-the-ring-is-the-resend-store-and-a-replay-goes-in-batches.md
//! [`FileJournal`]: fixbolt_engine::journal::FileJournal
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

#[path = "../../codec/benches/harness.rs"]
mod harness;

use std::hint::black_box;

use fixbolt_engine::journal::Store;
use fixbolt_session::journal::Journal;

fn main() {
    harness::suite(|b| {
        // `seq + 1` every time: the engine's own pattern, and the one that
        // walks the ring.
        for (name, len) in [
            ("journal put, 191 bytes, walking", 191usize),
            ("journal put, 87 bytes, walking", 87),
        ] {
            let mut journal = Store::new();
            let msg = vec![b'x'; len];

            // Assert the path before timing it, and assert it by READING BACK.
            // `put` returning `true` says the slot was not refused; it does not
            // say the bytes arrived, and a `put` that wrote the right length of
            // the wrong thing would time identically.
            assert!(journal.put(1, &msg), "{name}: the first put was refused");
            assert_eq!(
                journal.get(1),
                Some(&msg[..]),
                "{name}: the ring gave back other bytes"
            );

            let mut seq = 2u32;
            b.bench(name, || {
                // 1.41 million iterations against a u32 that starts at 2: no
                // wrap, and every one of them a different slot.
                assert!(journal.put(black_box(seq), black_box(&msg)));
                seq += 1;
            });
        }

        // The same work at one address. `seq` never changes, so `seq % 4096` is
        // slot 0 on every iteration and its eight lines never leave L1.
        {
            let mut journal = Store::new();
            let msg = vec![b'x'; 191];
            let name = "journal put, 191 bytes, one slot";

            assert!(journal.put(4096, &msg), "{name}: the first put was refused");
            assert_eq!(
                journal.get(4096),
                Some(&msg[..]),
                "{name}: the ring gave back other bytes"
            );

            b.bench(name, || {
                assert!(journal.put(black_box(4096), black_box(&msg)));
            });
        }
    });
}
