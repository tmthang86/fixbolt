//! Reading a counterparty's identity off a `Logon`, before any session exists.
//!
//! Step 2 of [pre-session-routing]. [ADR-0020] decision 2: this stage reads
//! bytes and never becomes a second session layer — `49=` and `56=` come off
//! the buffer by direct scan, the way `msg_type_is_logon` already does, with no
//! dictionary and no parse.
//!
//! # The bytes are real, and the corpus is more adversarial than an invention
//!
//! Every message here comes out of the acceptance corpus through
//! `fixbolt_conformance::script::load_all`. `CLAUDE.md` §7: a hand-written
//! packet proves the parser handles a packet nobody sends.
//!
//! `[measured 2026-09-01]` the first draft of this file asserted that every
//! corpus `Logon` is `49=TW44` / `56=ISLD`, and it went **red on the real
//! bytes**: the corpus logs on as `49=WT` in `1c_InvalidSenderCompID.def` and
//! as `56=DLSI` in `2k_CompIDDoesNotMatchProfile.def`, because reversed comp
//! IDs are things it deliberately tests. The reader was right and the
//! assumption was wrong.
//!
//! Better still, **five of the 289 messages the corpus sends have no readable
//! identity at all**, and they are the interesting ones:
//!
//! | File | Why |
//! |---|---|
//! | `14b_RequiredFieldMissing.def` | has `49=`, has no `56=` |
//! | `2d_GarbledMessage.def`, `3c_GarbledMessage.def` | the **tag** is corrupted — `garbled9=TW`, `49garbled=TW` |
//!
//! That is a supply of malformed headers nobody had to invent, and the count
//! is what these tests assert: a reader that got more lenient would find
//! identities in the garbled ones and the count would drop.
//!
//! The only invented bytes are in
//! [`a_field_value_that_looks_like_an_identity_is_not_one`], for a shape the
//! corpus does not have — a **well-formed** field whose value contains `49=`.
//! It is built by inserting one field INTO a real message.
//!
//! [pre-session-routing]: ../../../docs/plans/2026-08-31-pre-session-routing.md
//! [ADR-0020]: ../../../docs/decisions/ADR-0020-a-pre-session-stage-owns-the-socket-until-logon.md
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::{BTreeMap, BTreeSet};

use fixbolt_conformance::script::{Kind, load_all};
use fixbolt_engine::frame::{Cut, Framer};
use fixbolt_engine::presession::{Identity, identity_of, is_logon};

const RX: usize = 4096;

/// Every `Logon` the corpus sends to the engine, as wire bytes.
fn corpus_logons() -> Vec<Vec<u8>> {
    load_all()
        .expect("the corpus is fetched — scripts/fetch-quickfix-assets.sh")
        .into_iter()
        .filter_map(|s| match s.kind {
            Kind::Send(m) if is_logon(&m.wire) => Some(m.wire),
            _ => None,
        })
        .collect()
}

/// A complete message the corpus sends that is NOT a `Logon`.
fn corpus_not_a_logon() -> Vec<u8> {
    load_all()
        .expect("the corpus is fetched")
        .into_iter()
        .find_map(|s| match s.kind {
            Kind::Send(m) if !is_logon(&m.wire) && !m.wire.is_empty() => Some(m.wire),
            _ => None,
        })
        .expect("the corpus sends something other than a Logon")
}

/// What the corpus actually carries, asserted rather than assumed.
///
/// Counts, not spot checks: this is the assertion that notices a reader which
/// got more lenient. A scan matching `49=` anywhere in the message rather than
/// at the start of a field would find one in the garbled headers, and `without
/// an identity` would fall below 5.
#[test]
fn the_corpus_identities_are_read_exactly_as_the_wire_carries_them() {
    let mut pairs: BTreeMap<(Vec<u8>, Vec<u8>), usize> = BTreeMap::new();
    let mut unreadable: BTreeSet<String> = BTreeSet::new();
    let mut total = 0usize;

    for step in load_all().expect("the corpus is fetched — scripts/fetch-quickfix-assets.sh") {
        let Kind::Send(m) = &step.kind else { continue };
        total += 1;
        match identity_of(&m.wire) {
            Some(id) => {
                *pairs
                    .entry((id.sender.to_vec(), id.target.to_vec()))
                    .or_default() += 1
            }
            None => {
                unreadable.insert(step.file().to_owned());
            }
        }
    }

    assert_eq!(total, 289, "the corpus sends this many messages");
    let readable: usize = pairs.values().sum();
    assert_eq!(
        total - readable,
        5,
        "five sent messages name no identity; got {unreadable:?}"
    );
    assert_eq!(
        unreadable,
        BTreeSet::from([
            "14b_RequiredFieldMissing.def".to_owned(),
            "2d_GarbledMessage.def".to_owned(),
            "3c_GarbledMessage.def".to_owned(),
        ]),
        "and they are exactly the ones with a missing or corrupted header"
    );

    let mut got: Vec<(String, String, usize)> = pairs
        .iter()
        .map(|((s, t), n)| {
            (
                String::from_utf8_lossy(s).into_owned(),
                String::from_utf8_lossy(t).into_owned(),
                *n,
            )
        })
        .collect();
    got.sort();
    assert_eq!(
        got,
        vec![
            ("TW44".to_owned(), String::new(), 1),
            ("TW44".to_owned(), "DLSI".to_owned(), 3),
            ("TW44".to_owned(), "ISLD".to_owned(), 277),
            ("WT".to_owned(), "DLSI".to_owned(), 1),
            ("WT".to_owned(), "ISLD".to_owned(), 2),
        ],
        "the corpus's identities, reversed comp IDs and an empty 56= included"
    );
}

/// An empty `56=` is an identity the wire carries, not an absent one.
///
/// Reporting it faithfully is the job here; whether to serve it is the
/// session's, and the session rejects it. A reader that folded empty into
/// `None` would be making a protocol decision in a module that ADR-0020
/// decision 2 says makes none.
#[test]
fn an_empty_comp_id_is_read_as_empty_and_not_as_missing() {
    let wire = b"8=FIX.4.4\x0135=A\x0149=TW44\x0156=\x0110=000\x01";
    let id = identity_of(wire).expect("both fields are present");
    assert_eq!(id.sender, b"TW44");
    assert_eq!(id.target, b"", "present and empty");
}

#[test]
fn every_logon_the_corpus_sends_names_both_sides() {
    let logons = corpus_logons();
    assert_eq!(logons.len(), 65, "the corpus sends this many Logons");
    let mut odd = 0usize;
    for wire in &logons {
        let id = identity_of(wire).expect("every Logon in the corpus names both sides");
        if id.sender != b"TW44" || id.target != b"ISLD" {
            odd += 1;
        }
    }
    assert_eq!(
        odd, 2,
        "two Logons carry a deliberately wrong comp ID — 1c_InvalidSenderCompID \
         and 2k_CompIDDoesNotMatchProfile"
    );
}

/// The trap the plan named: a truncated message must say "not yet", never an
/// identity read out of half a message.
#[test]
fn no_prefix_of_a_logon_yields_an_identity() {
    let wire = corpus_logons().into_iter().next().expect("a Logon");
    for cut_at in 0..wire.len() {
        let mut framer: Framer<RX> = Framer::new();
        let head = &wire[..cut_at];
        framer.spare()[..head.len()].copy_from_slice(head);
        framer.filled(head.len());
        assert_eq!(
            framer.cut(),
            Cut::Need,
            "{cut_at} of {} bytes should not be a whole message",
            wire.len()
        );
    }
    // And the whole thing is.
    let mut framer: Framer<RX> = Framer::new();
    framer.spare()[..wire.len()].copy_from_slice(&wire);
    framer.filled(wire.len());
    let Cut::Message(n) = framer.cut() else {
        panic!("the whole message must cut as one");
    };
    assert_eq!(n, wire.len(), "and it must be exactly this long");
    let id = identity_of(framer.bytes(n)).expect("a Logon names both sides");
    assert_eq!(id.sender, b"TW44");
    assert_eq!(id.target, b"ISLD");
}

#[test]
fn a_message_that_is_not_a_logon_says_so_and_still_names_its_sides() {
    let wire = corpus_not_a_logon();
    assert!(!is_logon(&wire), "{}", String::from_utf8_lossy(&wire));
    // Reading the identity is not the same question as being a Logon, and this
    // stage answers them separately: the caller drops a non-Logon regardless.
    let id = identity_of(&wire).expect("a session message names both sides");
    assert_eq!(id.sender, b"TW44");
    assert_eq!(id.target, b"ISLD");
}

#[test]
fn bytes_that_name_neither_side_have_no_identity() {
    assert!(identity_of(b"").is_none());
    assert!(
        identity_of(b"8=FIX.4.4\x0135=A\x01").is_none(),
        "no 49= or 56="
    );
    // The corpus supplies this one for real: 14b_RequiredFieldMissing.def sends
    // a message with 49= and no 56=.
    assert!(
        identity_of(b"8=FIX.4.4\x0135=A\x0149=TW44\x01").is_none(),
        "49= alone is not an identity"
    );
    assert!(
        identity_of(b"8=FIX.4.4\x0135=A\x0156=ISLD\x01").is_none(),
        "56= alone is not an identity"
    );
    // A corrupted TAG, the shape 2d_GarbledMessage.def carries.
    assert!(
        identity_of(b"8=FIX.4.4\x0135=0\x0149garbled=TW\x0156=ISLD\x01").is_none(),
        "a tag that is not exactly 49= does not name a sender"
    );
}

/// `49=` inside a field VALUE is not the sender.
///
/// Built by inserting `58=` — Text, a free-form field — into a real corpus
/// Logon, so everything except the inserted field is bytes a counterparty
/// actually sent. A scan that looked for `49=` anywhere rather than at the
/// start of a field would read `EVIL` here.
#[test]
fn a_field_value_that_looks_like_an_identity_is_not_one() {
    let wire = corpus_logons().into_iter().next().expect("a Logon");
    let at = wire
        .windows(4)
        .position(|w| w == b"\x0149=")
        .expect("the Logon has a 49= field");
    let mut evil = Vec::with_capacity(wire.len() + 32);
    evil.extend_from_slice(&wire[..=at]);
    evil.extend_from_slice(b"58=49=EVIL56=EVIL\x01");
    evil.extend_from_slice(&wire[at + 1..]);

    let id = identity_of(&evil).expect("the real fields are still there");
    assert_eq!(id.sender, b"TW44", "a value is not a field");
    assert_eq!(id.target, b"ISLD", "a value is not a field");
}

/// The identity borrows the buffer it was read from — no copy, no allocation.
#[test]
fn the_identity_borrows_and_does_not_own() {
    let wire = corpus_logons().into_iter().next().expect("a Logon");
    let id: Identity<'_> = identity_of(&wire).expect("a Logon");
    assert!(
        wire.as_ptr_range().contains(&id.sender.as_ptr()),
        "sender must point into the caller's buffer"
    );
    assert!(wire.as_ptr_range().contains(&id.target.as_ptr()));
}
