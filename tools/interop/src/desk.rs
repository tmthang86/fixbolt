//! The application behind `--role acceptor`: fills orders, answers nothing
//! else.
//!
//! # Why this is not `crates/library/examples/shared/order_handler.rs`
//!
//! That file is an **example**, and an example's job is to be readable and to
//! change when the API changes. A gate that fails because an example was
//! rewritten fails for a reason other than the thing under test, which is the
//! shape `docs/reference/` keeps collecting. So this handler is the tool's own,
//! and it is written for one reader: `libquickfix`'s `FIX44.xml`.
//!
//! # What FIX44.xml requires of a 35=8, and why every field below is here
//!
//! The C++ initiator runs with `UseDataDictionary=Y`, so an
//! `ExecutionReport` missing a required field is rejected **by the
//! counterparty** before the application sees it — which is exactly the
//! independent opinion this gate exists to collect. The required set for FIX
//! 4.4 is `37` OrderID, `17` ExecID, `150` ExecType, `39` OrdStatus, `55`
//! Symbol, `54` Side, `151` LeavesQty, `14` CumQty and `6` AvgPx.
//!
//! `11` ClOrdID is **not** required by the dictionary and is the one field the
//! gate cannot do without: it is how step 2 pairs a reply with the order that
//! asked for it. Reversal A in the plan removes it and step 2 must go red.
//!
//! Nothing here is ordered by hand — non-negotiable 5. The fields go out in the
//! dictionary's order because [`fixbolt::Reply`] puts them there.

use fixbolt::{Answer, GroupData, GroupEntryData, Handler, Incoming, Peer, Reply};

/// Fills every `NewOrderSingle` at the quantity it was sent with.
///
/// `150=0` / `39=0` — **New**, not filled. This end is a protocol
/// counterparty, not a matching engine, and `LeavesQty = OrderQty` with
/// `CumQty = 0` is the one self-consistent report that needs no price.
#[derive(Default)]
pub struct Desk {
    orders: u32,
}

impl Handler for Desk {
    /// Two `35=B` News the moment a session comes up, which is what steps 2 and
    /// 5 of `--role initiator` have been asking this engine for.
    ///
    /// **This is the whole of `STATUS.md` item 46 as a user sees it**, and it is
    /// three lines — the same three the C++ acceptor spends in `onLogon`. Until
    /// [ADR-0048] there was no method to put them in:
    /// `fixbolt::serve` could only answer.
    ///
    /// **`FIX44.xml` requires two things of a `35=B`, not one**: `148`
    /// Headline **and** `LinesOfTextGrp` — the `33` NoLinesOfText group with a
    /// `58` Text in each entry (`spec/FIX44.xml:294`, `required='Y'`). A News
    /// carrying only `148=` is refused by the counterparty's dictionary and
    /// never reaches its application, which is
    /// [a-message-on-the-wire-is-not-a-message-delivered] — found here, by this
    /// step going red while the resend step behind it went green on the very
    /// same two messages.
    ///
    /// [ADR-0048]: ../../../docs/decisions/ADR-0048-an-engine-that-can-speak-first-has-two-doors.md
    /// [a-message-on-the-wire-is-not-a-message-delivered]: ../../../docs/reference/a-message-on-the-wire-is-not-a-message-delivered.md
    fn on_logon(&mut self, _who: Peer<'_>, nth: u32, reply: Reply<'_>) -> Answer {
        let headline: &[u8] = match nth {
            0 => b"fixbolt desk is up",
            1 => b"and open for orders",
            _ => return reply.silent(),
        };
        let text: [(u32, &[u8]); 1] = [(58, headline)];
        let entries = [GroupEntryData {
            fields: &text,
            groups: &[],
        }];
        reply
            .message(b"B")
            .field(148, headline)
            .group(33)
            .send_with_groups(&[GroupData {
                counter: 33,
                entries: &entries,
            }])
    }

    fn on_message(&mut self, msg: &Incoming<'_>, reply: Reply<'_>) -> Answer {
        if msg.msg_type() != b"D" {
            return reply.silent();
        }
        let Some(cl_ord_id) = msg.get(11) else {
            // An order with no `11=` cannot be answered in a way the other end
            // can match up. Silence rather than a report nobody can pair.
            return reply.silent();
        };

        self.orders += 1;
        let mut buf = [0u8; 16];
        let id = ident(self.orders, &mut buf);
        let qty = msg.get(38).unwrap_or(b"0");

        reply
            .message(b"8")
            .field(37, id) // OrderID
            .field(17, id) // ExecID
            .field(150, b"0") // ExecType — New
            .field(39, b"0") // OrdStatus — New
            .field(11, cl_ord_id) // echoed, borrowed from the read buffer
            .field(55, msg.get(55).unwrap_or(b"")) // Symbol
            .field(54, msg.get(54).unwrap_or(b"")) // Side
            .field(151, qty) // LeavesQty — nothing filled, so all of it
            .field(14, b"0") // CumQty
            .field(6, b"0") // AvgPx
            .send()
    }
}

/// `ORD-<n>` into `buf`, with no allocation.
///
/// Ten digits is `u32::MAX` and the prefix is four bytes, so sixteen is always
/// enough and the slice is always in range.
fn ident(n: u32, buf: &mut [u8; 16]) -> &[u8] {
    buf[..4].copy_from_slice(b"ORD-");
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
    buf[4..4 + len].copy_from_slice(&digits[i..]);
    &buf[..4 + len]
}

#[cfg(test)]
mod tests {
    use super::ident;

    #[test]
    fn ident_renders_without_allocating() {
        let mut buf = [0u8; 16];
        assert_eq!(ident(1, &mut buf), b"ORD-1");
        let mut buf = [0u8; 16];
        assert_eq!(ident(4_294_967_295, &mut buf), b"ORD-4294967295");
    }
}
