//! What a bigger message costs the kernel, and nothing else.
//!
//! Step A1 of `plans/2026-09-05-what-is-left-and-what-a-message-touches.md`,
//! for `STATUS.md` open item 49.
//!
//! # The question
//!
//! `[measured 2026-09-05]` `tools/w2w --path app` costs **3 898 ns** more per
//! round trip than `--path admin` at p50 on the §9 desktop, and committed
//! benchmarks account for only **~1 094 ns** of it. Two of the four candidates
//! named for the remaining ~2 804 ns are one thing seen from both ends: the
//! application path moves **more bytes**, so every copy the kernel makes is
//! longer — on the engine's `recvfrom`/`sendto` and on the client's blocking
//! `read` alike.
//!
//! This prices that, and only that.
//!
//! # The sizes are measured, not assumed
//!
//! `[measured 2026-09-05]` `strace -f -e trace=sendto` on
//! `./target/release/w2w` at its default flags (`--messages 20000 --warmup
//! 2000`), §9 desktop, over the last 2 000 sends of each direction — all 2 000
//! identical, so these are exact and not modal:
//!
//! | Path | in (client to engine) | out (engine to client) |
//! |---|---|---|
//! | `--path admin`, `35=1` then `35=0` | **83** | **87** |
//! | `--path app`, `35=D` then `35=8` | **149** | **191** |
//!
//! Open item 49 recorded these as "149 bytes in and ~200 out against 79 and
//! ~70". One of the four was right. The `~` was the warning, and building two
//! benchmark cases on the other three is what this measurement stopped. The
//! sizes drift with the digit count of `34=` and `11=`/`112=`, so they mean
//! nothing without a message count beside them: at `--messages 20` the same two
//! paths read 77/81 and 143/179.
//!
//! **The application path moves +66 bytes in and +104 out**, and each byte is
//! copied twice — into the kernel and out of it.
//!
//! # The answer is a slope, not a subtraction
//!
//! The two real sizes differ by 170 bytes. `[measured 2026-09-05]` that is worth
//! about **11 ns**, and the round trip they sit inside costs **~10 250 ns** on
//! this box — one part in 930. Subtracting the two middle cases directly does
//! not work and must not be attempted: across three independent repetitions
//! their difference read **-4, +13 and +46 ns**, which is scatter with a sign
//! change in it, not a measurement.
//!
//! So the term is read off the two outer cases, where the lever is 8 184 bytes
//! and the difference is ~520 ns — big enough to be real, and stable to about
//! 2% run to run. **8 in 8 out** is the floor: four syscalls and almost no
//! payload. **8192 in 8192 out** is the lever.
//!
//! `[measured 2026-09-05]` a write-plus-read of one byte costs about
//! **0.065 ns**, measured over 8, 83, 149, 512, 1024, 2048, 4096 and 8192 bytes,
//! monotone at every step, three repetitions. So the payload term between the
//! two w2w paths is `170 x 0.065` = **~11 ns**, which is **0.4%** of the 2 804 ns
//! it was a named candidate for.
//!
//! The two middle cases stay anyway, at w2w's exact sizes, because
//! non-negotiable 10 wants the bytes a claim is about to have a committed case
//! of their own — and because the slope predicting them is the only check that
//! the slope is about payload at all.
//!
//! # The absolute figures are not a round-trip claim, and here is why
//!
//! `[measured 2026-09-05]` on this machine a **TCP loopback** write-plus-read of
//! 8 bytes costs **10 235 ns**, against **1 925 ns** for a UNIX socketpair and
//! **779 ns** for a pipe, on a box whose bare syscall floor is **170 ns**
//! (`getppid`) and whose non-blocking `recv` on a quiet socket is 418.5 ns. The
//! reads are not waiting: a non-blocking version of the same loop reads **0.00
//! `EAGAIN` per operation** and costs the same, and the write alone is 5 450 ns
//! of it.
//!
//! Ten microseconds for four syscalls is not explained here, and this benchmark
//! does not need it to be: **a constant cancels in a difference**, and both w2w
//! paths pay the same one — `[measured 2026-09-05]` the two `strace` runs above
//! make **44 002 `sendto` calls each**, so the paths differ in bytes and not in
//! syscall count. The anomaly is its own open item and its own write-up.
//!
//! # What is NOT in the timed region, and so is not in the answer
//!
//! * **A NIC.** This is `127.0.0.1` — no driver, no interrupt, no wire. Open
//!   item 40 is the row that needs those, and nothing here speaks to it.
//! * **A thread wakeup.** `tools/w2w` has a client thread and an engine thread
//!   and pays a scheduler hop each way; this is one thread doing four syscalls.
//!   That hop is absent from **both** cases equally, which is why the difference
//!   still means what it says while neither absolute figure is comparable to a
//!   w2w number.
//! * **Any FIX.** The buffers are filler bytes. The kernel does not parse, and a
//!   real message here would only invite the figure to be quoted as a round trip
//!   it is not.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

#[path = "../../codec/benches/harness.rs"]
mod harness;

use std::hint::black_box;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};

/// Two connected ends of a loopback TCP connection, both blocking, both with
/// Nagle off.
///
/// **`set_nodelay` is not a detail.** `[measured 2026-08-30]` a wire gate in
/// this repository walked 39 to 59 with its own timeout while the actual cause
/// was Nagle, and the wrong answer reached five documents. A 40 ms delay on the
/// second small write would not be subtle here — it would be the whole figure.
fn pair() -> (TcpStream, TcpStream) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a free port");
    let addr = listener.local_addr().expect("bound");
    let client = TcpStream::connect(addr).expect("connect");
    let (server, _) = listener.accept().expect("accept");
    client.set_nodelay(true).expect("nodelay on the client");
    server.set_nodelay(true).expect("nodelay on the server");
    (client, server)
}

fn main() {
    harness::suite(|b| {
        // 8/8 is the floor and 8192/8192 the lever; the two between them are
        // `tools/w2w`'s own measured sizes, admin then app.
        for (name, in_len, out_len) in [
            ("TCP loopback, 8 in 8 out", 8usize, 8usize),
            ("TCP loopback, 83 in 87 out", 83, 87),
            ("TCP loopback, 149 in 191 out", 149, 191),
            ("TCP loopback, 8192 in 8192 out", 8192, 8192),
        ] {
            let (mut client, mut server) = pair();
            // Distinct fill bytes each way, so a round trip that read back its
            // own request — the shape a half-duplex mistake takes — fails the
            // assertion below instead of timing at half price.
            let req = vec![b'q'; in_len];
            let rep = vec![b'r'; out_len];
            let mut at_server = vec![0u8; in_len];
            let mut at_client = vec![0u8; out_len];

            // Assert the path before timing it, and assert the BYTES, not the
            // call's exit status. `read_exact` already fails on a short read;
            // the comparison is what catches a round trip that moved the right
            // number of the wrong bytes.
            //
            // `[measured 2026-09-05]` two `parse` cases in this repository timed
            // a message the parser rejects for a month, because they checked
            // their output and never their input —
            // `docs/reference/a-benchmark-parsed-a-message-the-parser-rejects.md`.
            client.write_all(&req).expect("the request goes out");
            server
                .read_exact(&mut at_server)
                .expect("and arrives whole");
            assert_eq!(at_server, req, "{name}: the server read other bytes");
            server.write_all(&rep).expect("the reply goes out");
            client
                .read_exact(&mut at_client)
                .expect("and arrives whole");
            assert_eq!(at_client, rep, "{name}: the client read other bytes");

            b.bench(name, || {
                client.write_all(black_box(&req)).expect("write");
                server.read_exact(black_box(&mut at_server)).expect("read");
                server.write_all(black_box(&rep)).expect("write");
                client.read_exact(black_box(&mut at_client)).expect("read");
            });
        }
    });
}
