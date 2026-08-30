//! The 50 mirrorable definitions, this engine playing the **initiator**.
//!
//! The acceptor's gate (`tests/score.rs`) is the primary one and it is met.
//! This is the secondary gate `ADR-0004` decision 6 defines and `ADR-0006`
//! corrects: the same files read from the other side, with `I` lines as this
//! engine's output.
//!
//! **What this gate cannot do is check its own reading.** Mirroring is this
//! project's interpretation of a suite written for the other direction; a wrong
//! interpretation stays green here. Interop against `libquickfix` — step 4 of
//! the plan — is what stands in front of that.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use nanofix_conformance::runner::{Conn, Input, Link, SessionUnderTest, run_mirrored};
use nanofix_conformance::script::{Kind, Scenario};
use nanofix_session::{Config, Initiator, Session};

fn link(l: nanofix_session::Link) -> Link {
    match l {
        nanofix_session::Link::Up => Link::Up,
        nanofix_session::Link::Dropped => Link::Dropped,
    }
}

/// One connection's state machine, plus the identity this end plays.
///
/// Mirrored, the CompIDs swap: the files are written from `ISLD`'s side and
/// this engine is `TW44`.
struct Adapter {
    conns: Vec<(Conn, Session<Initiator, 256>)>,
    heart_bt_int: u32,
}

impl Adapter {
    fn new(s: &Scenario) -> Self {
        Self {
            conns: Vec::new(),
            heart_bt_int: proposed_heart_bt_int(s),
        }
    }

    fn at(&mut self, conn: Conn) -> usize {
        if let Some(i) = self.conns.iter().position(|(c, _)| *c == conn) {
            return i;
        }
        self.conns.push((
            conn,
            Session::new(
                Config::initiator(b"FIX.4.4", b"TW44", b"ISLD")
                    .with_heart_bt_int(self.heart_bt_int),
            ),
        ));
        self.conns.len() - 1
    }
}

/// The `108=` this file's own Logon asks for.
///
/// Mirrored, that Logon is **this engine's** output, so the number is a
/// configuration input rather than something read off the wire. 13 of the files
/// ask for 2 and 2 for 6; the rest for 30.
fn proposed_heart_bt_int(s: &Scenario) -> u32 {
    for step in &s.steps {
        if let Kind::Expect(m) = &step.kind
            && field(&m.wire, 35) == Some(b"A")
            && let Some(v) = field(&m.wire, 108)
            && let Ok(secs) = core::str::from_utf8(v).unwrap_or("").parse::<u32>()
        {
            return secs;
        }
    }
    30
}

fn field(wire: &[u8], tag: u32) -> Option<&[u8]> {
    let mut needle = [0u8; 12];
    let mut n = 0;
    let mut t = tag;
    let mut digits = [0u8; 10];
    let mut d = 0;
    if t == 0 {
        digits[0] = b'0';
        d = 1;
    }
    while t > 0 {
        digits[d] = b'0' + u8::try_from(t % 10).unwrap_or(0);
        d += 1;
        t /= 10;
    }
    for i in 0..d {
        needle[n] = digits[d - 1 - i];
        n += 1;
    }
    needle[n] = b'=';
    n += 1;
    let needle = &needle[..n];

    let mut at = 0;
    while at < wire.len() {
        let start = at;
        let end = wire[at..].iter().position(|b| *b == 1)? + at;
        if wire[start..end].starts_with(needle) {
            return Some(&wire[start + needle.len()..end]);
        }
        at = end + 1;
    }
    None
}

impl SessionUnderTest for Adapter {
    fn step<F: FnMut(&[u8])>(&mut self, conn: Conn, input: Input<'_>, emit: F) -> Link {
        let i = self.at(conn);
        let s = &mut self.conns[i].1;
        link(match input {
            Input::Connect => s.connect(emit),
            Input::Disconnect => s.disconnect(emit),
            Input::Bytes(b) => s.received(b, emit),
            Input::Tick(ms) => s.tick(ms, emit),
        })
    }
}

/// Step 2: the initiator speaks first, and **that is all it can do alone**.
///
/// `[measured 2026-08-30]` **0 / 50**, and the reason is worth more than the
/// number. Every mirrored Logon this engine sends is accepted — the failures in
/// every file start at the line *after* it. What they ask for next is a message
/// only an operator can order:
///
/// | Must be originated | Files |
/// |---|---|
/// | `5` Logout | 42 |
/// | `D` / `d` / `8` application | 19 |
/// | `0` Heartbeat, unprompted | 14 |
/// | `1` TestRequest, with a chosen `112=` | 13 |
/// | `4` SequenceReset | 6 |
/// | `2` ResendRequest | 4 |
///
/// 46 of the 50 need at least one. A session state machine cannot invent any of
/// them: nothing on the wire asks for them, and no timer produces a Logout.
/// Step 3 is where the API an initiator actually needs gets written.
#[test]
fn step_two_speaks_first_and_can_do_nothing_else_alone() {
    let report = run_mirrored(Adapter::new).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(report.scenarios, 50, "50 mirrorable files:\n{report}");
    assert_eq!(report.passed, 0, "step 2 of the initiator plan:\n{report}");

    // The Logon this engine sends is right, and the proof is which files do
    // **not** fail on the line that carries it: 45 of the 50.
    //
    // The five that do are the ones whose first `I` line is wrong **on
    // purpose** — a CompID that does not match, a `SendingTime` 2001 years
    // out, a `9=` that is 23 bytes short, and one that is not a Logon at all.
    // Mirrored, they ask this engine to send those, and a correct engine
    // cannot. `sendable()` cannot see it either: every one of them is
    // syntactically perfect, which is exactly what makes them interesting.
    let mut early: Vec<&str> = report
        .failures
        .iter()
        .filter(|f| f.line_no <= 4)
        .map(|f| f.file.as_str())
        .collect();
    early.dedup();
    assert_eq!(
        early,
        [
            "1c_InvalidSenderCompID.def",
            "1c_InvalidTargetCompID.def",
            "1d_InvalidLogonBadSendingTime.def",
            "1d_InvalidLogonLengthInvalid.def",
            "1e_NotLogonMessage.def",
        ],
        "the five whose Logon is deliberately wrong, and nothing else:\n{report}"
    );
}
