//! The worked handler, and **the only copy of it**.
//!
//! `examples/acceptor.rs` runs it against a real listener;
//! `tests/end_to_end.rs` pulls in this same file with `#[path]` and drives it
//! through a real socket. One file, two readers. Two copies of a worked example
//! are two examples that will eventually disagree, and the one that disagrees
//! is the one nobody runs — the reason `crates/conformance/src/echo.rs` was
//! gathered into one place on 2026-08-31.
//!
//! A directory under `examples/` with no `main.rs` in it is not a cargo target,
//! so this file is compiled only by whoever asks for it.
//!
//! # What it does
//!
//! Fills every `NewOrderSingle (35=D)` at the price it was sent at, and answers
//! with an `ExecutionReport (35=8)`. It is a demonstration, not a matching
//! engine: there is no book, no risk check and no persistence.
//!
//! # What it shows about the API
//!
//! * It names **body fields only**, and in no particular order. `8`, `9`, `10`,
//!   `34`, `49`, `52` and `56` are not mentioned anywhere in this file, and the
//!   order they and everything else come out in is the dictionary's.
//! * It **echoes** `11`, `55` and `54` straight out of the incoming view, with
//!   no copy: `Incoming::get` borrows the engine's read buffer.
//! * It allocates nothing. The one number it renders goes into a stack array.

use fixbolt::{Answer, Handler, Incoming, Reply};

/// A desk that fills whatever it is sent.
///
/// The counter is deliberately not readable from outside: `serve` takes the
/// application by value and never gives it back, so an accessor here would be
/// a method neither of this file's two consumers could call. What the count
/// does instead is reach the wire, as `EXEC-1`, `EXEC-2` — which is where
/// `tests/end_to_end.rs` asserts that it advances.
#[derive(Default)]
pub struct Desk {
    fills: u32,
}

impl Handler for Desk {
    fn on_message(&mut self, msg: &Incoming<'_>, reply: Reply<'_>) -> Answer {
        // Everything that is not an order is somebody else's business. The
        // session has already answered the seven administrative types itself.
        if msg.msg_type() != b"D" {
            return reply.silent();
        }

        // An order with no quantity or no price is not one this desk can fill.
        // Refusing here rather than filling at zero: `silent()` says the
        // decision was made, where a `35=8` with an empty `31=` would be a
        // message the counterparty has to interpret.
        let (Some(qty), Some(price), Some(cl_ord_id)) = (msg.get(38), msg.get(44), msg.get(11))
        else {
            return reply.silent();
        };

        self.fills += 1;
        let mut buf = [0u8; 16];
        let exec_id = exec_id(self.fills, &mut buf);

        reply
            .message(b"8")
            .field(37, exec_id) // OrderID
            .field(17, exec_id) // ExecID
            .field(150, b"F") // ExecType — Trade
            .field(39, b"2") // OrdStatus — Filled
            .field(11, cl_ord_id) // echoed, borrowed from the read buffer
            .field(55, msg.get(55).unwrap_or(b"")) // Symbol
            .field(54, msg.get(54).unwrap_or(b"")) // Side
            .field(38, qty) // OrderQty
            .field(32, qty) // LastQty
            .field(31, price) // LastPx
            .field(14, qty) // CumQty
            .field(151, b"0") // LeavesQty — filled, so none left
            .field(6, price) // AvgPx
            .send()
    }
}

/// `EXEC-<n>` into `buf`, with no allocation.
///
/// Ten digits is `u32::MAX` and the prefix is five bytes, so sixteen is always
/// enough and the slice below is always in range.
fn exec_id(n: u32, buf: &mut [u8; 16]) -> &[u8] {
    buf[..5].copy_from_slice(b"EXEC-");
    let mut digits = [0u8; 10];
    let mut v = n;
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
