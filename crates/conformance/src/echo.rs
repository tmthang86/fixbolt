//! The application the acceptance corpus assumes.
//!
//! `[measured 2026-08-28]` 42 of the 250 `E` lines carry `35=D`: the QuickFIX
//! acceptance server sends application messages straight back. Without that,
//! fifteen of the 59 files cannot pass, and the plan's "a session machine is
//! enough" would have been wrong.
//!
//! # It re-orders, and that is the interesting part
//!
//! `15_HeaderAndBodyFieldsOrderedDifferently.def` sends the same `NewOrderSingle`
//! twice — once in order, once with header and body fields shuffled — and
//! expects the **same bytes** back both times. So the echo cannot copy the
//! input's layout. It rebuilds the message through [`fixbolt_codec::Template`],
//! which sorts from the dictionary: `35`, then header tags ascending, then body
//! tags ascending. That is `CLAUDE.md` §2 non-negotiable 5, seen from the
//! application side.
//!
//! # Two timestamp widths, and `9=101` depends on both
//!
//! The expected reply declares `9=101`, and tag `9` is not matched by shape. It
//! only comes out right when:
//!
//! * `52` **SendingTime** is the engine's own, 21 bytes with milliseconds;
//! * `60` **TransactTime** is echoed **verbatim** from the input, 17 bytes.
//!
//! Regenerating `60`, or dropping the milliseconds from `52`, moves the body by
//! four bytes and fails a test that says nothing about time.
//!
//! # This is a measuring instrument, not a product
//!
//! It lives here and not in `engine`. It is the application the corpus assumes,
//! and nothing else should depend on it.

use core::ops::Range;

use fixbolt_codec::{EncodeError, FieldIndex, ParseError, TemplateBuilder, Validation, parse_into};
use fixbolt_dict::Fix44;

/// Fields the acceptor writes itself rather than echoing.
///
/// **Every other header field IS echoed, and the corpus is emphatic about
/// which.** `[measured 2026-08-28]`:
///
/// * `19b_PossResendMessageThatHasNotBeenSent.def` sends `97=Y` **PossResend**
///   and expects it back. Dropping every header tag loses it — 21 of the 22
///   echo pairs pass and that one does not.
/// * `2m_BodyLengthValueNotCorrect.def` sends `122` **OrigSendingTime** and
///   expects it **gone**. Echoing every header tag adds it — and that one fails
///   instead.
///
/// The line is between resend metadata, which belongs to the transmission
/// (`43`, `122`), and a flag the counterparty set about the order itself
/// (`97`). Guessing "all header fields" either way gets one of these wrong.
const REGENERATED: &[u32] = &[8, 9, 10, 34, 35, 43, 49, 52, 56, 122];

/// Why an echo could not be produced.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EchoError {
    /// The incoming bytes are not a message.
    Parse(ParseError),
    /// The reply could not be laid out or written.
    Encode(EncodeError),
    /// The incoming message has no `35`, `49` or `56`.
    MissingHeader(u32),
}

/// Echo an application message back to its sender.
///
/// `seq` and `sending_time` come from the session: an application does not own
/// the sequence number or the clock. `sending_time` must be 21 bytes with
/// milliseconds — see the module note on `9=101`.
///
/// Returns the range of `out` the reply occupies; the message does **not** start
/// at `out[0]`.
///
/// # Errors
///
/// [`EchoError`] if the input does not parse, lacks a routable header, or the
/// reply does not fit.
pub fn echo(
    incoming: &[u8],
    out: &mut [u8],
    seq: u32,
    sending_time: &[u8],
) -> Result<Range<usize>, EchoError> {
    let mut idx: FieldIndex<256> = FieldIndex::new();
    // The frame was already checked when this message was accepted; re-checking
    // a body length here would reject the deliberately-wrong ones the corpus
    // sends on purpose.
    parse_into::<Fix44, 256>(incoming, &mut idx, Validation::NONE).map_err(EchoError::Parse)?;
    let view = idx.view(incoming);

    let msg_type = view.get(35).ok_or(EchoError::MissingHeader(35))?;
    let sender = view.get(49).ok_or(EchoError::MissingHeader(49))?;
    let target = view.get(56).ok_or(EchoError::MissingHeader(56))?;

    let mut seq_buf = [0u8; 10];
    let seq_bytes = render(seq, &mut seq_buf);

    let mut b = TemplateBuilder::<128, 4096>::new(b"FIX.4.4")
        .field(35, msg_type)
        .field(34, seq_bytes)
        // Routed back: this side's sender is the other side's target.
        .field(49, target)
        .field(56, sender)
        .field(52, sending_time);

    for i in 0..view.len() {
        let Some((tag, value)) = view.field_at(i) else {
            continue;
        };
        if REGENERATED.contains(&tag) {
            continue;
        }
        b = b.field(tag, value);
    }

    let t = b.build::<Fix44>().map_err(EchoError::Encode)?;
    t.encode_with::<Fix44>(out, &[], &[])
        .map_err(EchoError::Encode)
}

/// ASCII digits of `v`, right-aligned in `buf`.
fn render(mut v: u32, buf: &mut [u8; 10]) -> &[u8] {
    if v == 0 {
        buf[9] = b'0';
        return &buf[9..];
    }
    let mut i = 10;
    while v > 0 && i > 0 {
        i -= 1;
        buf[i] = b'0' + u8::try_from(v % 10).unwrap_or(0);
        v /= 10;
    }
    &buf[i..]
}

/// The text `2r_UnregisteredMsgType.def` expects, byte for byte.
///
/// It belongs to the application, not to the session: `fixbolt_session::text`
/// holds the seventeen strings a *session* says, and "unsupported message type"
/// is a statement about what this end trades, not about the protocol.
const UNSUPPORTED: &[u8] = b"Unsupported Message Type";

/// `BusinessMessageReject (35=j)` — the application does not handle this type.
///
/// `[measured]` `2r_UnregisteredMsgType.def` is the only file that asks for one.
/// It sends `35=8` **ExecutionReport**, which is a perfectly good FIX 4.4
/// message type — `Fix44::is_msg_type` says so and the session therefore
/// accepts it. Only the application knows it has nothing to do with one, which
/// is exactly why `373=11` is the wrong answer and `380=3` is the right one.
///
/// # Errors
///
/// As [`echo`].
pub fn business_reject(
    incoming: &[u8],
    out: &mut [u8],
    seq: u32,
    sending_time: &[u8],
) -> Result<Range<usize>, EchoError> {
    let mut idx: FieldIndex<256> = FieldIndex::new();
    parse_into::<Fix44, 256>(incoming, &mut idx, Validation::NONE).map_err(EchoError::Parse)?;
    let view = idx.view(incoming);

    let msg_type = view.get(35).ok_or(EchoError::MissingHeader(35))?;
    let sender = view.get(49).ok_or(EchoError::MissingHeader(49))?;
    let target = view.get(56).ok_or(EchoError::MissingHeader(56))?;
    let ref_seq = view.get(34).unwrap_or(b"0");

    let mut seq_buf = [0u8; 10];
    let seq_bytes = render(seq, &mut seq_buf);

    let t = TemplateBuilder::<16, 256>::new(b"FIX.4.4")
        .field(35, b"j")
        .field(34, seq_bytes)
        .field(49, target)
        .field(56, sender)
        .field(52, sending_time)
        .field(45, ref_seq)
        .field(58, UNSUPPORTED)
        .field(372, msg_type)
        // `BusinessRejectReason = 3`, "Unsupported Message Type".
        .field(380, b"3")
        .build::<Fix44>()
        .map_err(EchoError::Encode)?;
    t.encode_with::<Fix44>(out, &[], &[])
        .map_err(EchoError::Encode)
}

/// The acceptance server's application logic, in one place.
///
/// `[2026-08-31]` **This existed twice before this type did** — identically, in
/// `crates/session/tests/score.rs` and `crates/engine/tests/wire.rs` — and step
/// 4 of `plans/2026-08-30-threads-and-affinity.md` was about to make it three.
/// Two copies of a test oracle are two oracles that will eventually disagree,
/// and the one that disagrees is the one nobody is looking at.
///
/// It is deliberately **not** an `Application` impl. That trait belongs to
/// `fixbolt_session`, and this crate does not depend on it — `DESIGN.md` §3 has
/// `conformance` depending on `codec` and `dict` only, and a shared test
/// fixture is not a reason to change a crate's dependency graph. Each caller
/// writes the five-line impl that forwards to [`Echo::reply`].
#[derive(Debug, Default)]
pub struct Echo {
    seen: Vec<Vec<u8>>,
}

impl Echo {
    /// What the acceptance server answers with, if anything.
    ///
    /// Orders and security definitions are echoed; everything else gets a
    /// business reject, because `2r_UnregisteredMsgType.def` sends `35=8`, which
    /// FIX 4.4 defines and this application does not want.
    ///
    /// A `35=D` carrying `97=Y` whose `11=` has been seen before is a
    /// **possible duplicate the application has already processed**, and it is
    /// answered with silence. That is the only place this fixture remembers
    /// anything.
    pub fn reply(
        &mut self,
        msg: &[u8],
        seq: u32,
        stamp: &[u8],
        out: &mut [u8],
    ) -> Option<Range<usize>> {
        let msg_type = tag(msg, 35)?;
        if msg_type != b"D" && msg_type != b"d" {
            return business_reject(msg, out, seq, stamp).ok();
        }
        if let Some(id) = tag(msg, 11) {
            let already = self.seen.iter().any(|s| s == id);
            if tag(msg, 97) == Some(b"Y") && already {
                return None;
            }
            if !already {
                self.seen.push(id.to_vec());
            }
        }
        echo(msg, out, seq, stamp).ok()
    }
}

/// One field off the wire, without a dictionary.
///
/// The two copies this replaces both built the needle with `format!`. This one
/// does not allocate, which matters only because it makes the fixture usable
/// from a test that is also watching allocations.
fn tag(wire: &[u8], want: u32) -> Option<&[u8]> {
    let mut at = 0;
    while at < wire.len() {
        let end = wire[at..].iter().position(|b| *b == 1)? + at;
        let field = &wire[at..end];
        if let Some(eq) = field.iter().position(|b| *b == b'=') {
            if core::str::from_utf8(&field[..eq])
                .ok()
                .and_then(|t| t.parse::<u32>().ok())
                == Some(want)
            {
                return Some(&field[eq + 1..]);
            }
        }
        at = end + 1;
    }
    None
}
