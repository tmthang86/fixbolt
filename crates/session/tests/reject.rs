//! Rules about `Reject (35=3)` that the 59 definitions cannot see.
//!
//! `[measured 2026-08-28]` making a Reject give the sequence number back leaves
//! the score at **27 / 59**. The corpus cannot see it, because nothing has
//! implemented the *too high* branch yet: a message whose `34=` runs ahead is
//! currently read as if it were in order, so a sequence number that never
//! advanced looks exactly like one that did.
//!
//! `14b_RequiredFieldMissing.def` states the rule in its own header comment —
//! *"Increment inbound MsgSeqNum"* — and does not test it.
//!
//! The first attempt at that reversal was worthless and worth recording: it
//! deleted a second, **unreachable** `next_in = seq + 1` inside the Reject arm,
//! and of course nothing changed. A reversal that does not alter behaviour
//! proves nothing about the guard and everything about the reverser — the check
//! is that the mutation actually failed something, not that it compiled.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use fixbolt_conformance::script::{FIXED_TIME_MILLIS, Kind, scenarios};
use fixbolt_session::{Acceptor, Config, Link, Session};

/// Every `I` line of one definition file, in order.
fn inputs(file: &str) -> Vec<Vec<u8>> {
    scenarios()
        .unwrap_or_else(|e| panic!("{e}"))
        .into_iter()
        .find(|s| s.file == file)
        .unwrap_or_else(|| panic!("{file} is not in the corpus"))
        .steps
        .into_iter()
        .filter_map(|s| match s.kind {
            Kind::Send(m) => Some(m.wire),
            _ => None,
        })
        .collect()
}

struct Driver {
    session: Session<Acceptor, 256>,
    out: Vec<String>,
}

impl Driver {
    fn logged_on() -> Self {
        let mut me = Self {
            session: Session::new(Config::acceptor(b"FIX.4.4", b"ISLD", b"TW44")),
            out: Vec::new(),
        };
        me.session.connect(|_| ());
        me.session.tick(FIXED_TIME_MILLIS, |_| ());
        let logon = inputs("14a_BadField.def").remove(0);
        let link = me.feed(&logon);
        assert_eq!(link, Link::Up, "the corpus's own Logon must be accepted");
        assert_eq!(me.out.len(), 1, "answered with a Logon");
        me.out.clear();
        me
    }

    fn feed(&mut self, wire: &[u8]) -> Link {
        let out = &mut self.out;
        self.session.tick(FIXED_TIME_MILLIS, |_| ());
        self.session.received(wire, |b| {
            out.push(String::from_utf8_lossy(b).replace('\u{1}', "|"))
        })
    }
}

#[test]
fn a_rejected_message_still_consumes_its_sequence_number() {
    // 14a's four bad messages are 34=2..5, each answered with a Reject. Replay
    // the first, then send its sequence number again: if the Reject consumed
    // it, the repeat is "too low" and the session says so and hangs up.
    let bad = inputs("14a_BadField.def");
    let mut d = Driver::logged_on();

    let link = d.feed(&bad[1]);
    assert_eq!(link, Link::Up, "a Reject does not end the session");
    assert_eq!(d.out.len(), 1, "one Reject: {:?}", d.out);
    assert!(d.out[0].contains("|373=0|"), "{}", d.out[0]);
    assert!(
        d.out[0].contains("|45=2|"),
        "45= is the rejected message: {}",
        d.out[0]
    );
    d.out.clear();

    // The same sequence number again. Expected is now 3, so this is too low.
    let link = d.feed(&bad[1]);
    assert_eq!(
        link,
        Link::Dropped,
        "34=2 was consumed by the Reject and must not be accepted twice"
    );
    assert_eq!(d.out.len(), 1, "a Logout saying why: {:?}", d.out);
    assert!(
        d.out[0].contains("|58=MsgSeqNum too low, expecting 3 but received 2|"),
        "{}",
        d.out[0]
    );
}

#[test]
fn four_rejects_in_a_row_number_themselves_in_order() {
    // The other half: the *outbound* sequence number advances once per Reject.
    // `14a` sends four bad messages and expects 34=2, 3, 4, 5 back — so a
    // session that answered all four with 34=2 would fail the file, and a
    // session that answered none would too.
    let bad = inputs("14a_BadField.def");
    let mut d = Driver::logged_on();

    for (i, wire) in bad[1..5].iter().enumerate() {
        d.out.clear();
        assert_eq!(d.feed(wire), Link::Up);
        assert_eq!(d.out.len(), 1, "message {i}: {:?}", d.out);
        let seq = i + 2;
        assert!(
            d.out[0].contains(&format!("|34={seq}|")),
            "reject {i} should be 34={seq}: {}",
            d.out[0]
        );
        assert!(
            d.out[0].contains(&format!("|45={seq}|")),
            "and refer to 45={seq}: {}",
            d.out[0]
        );
    }
}
