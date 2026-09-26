//! The facade is generic over the dictionary, and says FIX 4.4 when told
//! nothing (ADR-0207 decision 5; plan `2026-09-26-docs-for-embedders`,
//! steps 25 and 26).
//!
//! **What this proves:** a [`Reply`] over a dictionary other than FIX 4.4 lays
//! a repeating group out in *that* dictionary's declared order, keyed by
//! `(msg_type, counter)` — so the dialect, and not `Fix44`, reached the
//! encoder; and an [`App`] that names no dictionary is an `App` over
//! [`Fix44`], parsing and answering exactly as before the parameter existed.
//!
//! **What it does not prove:** that a dictionary *generated* by
//! `fixbolt_dict::codegen::generate(…, Paths::facade())` compiles against
//! `::fixbolt::dict` and answers the wire. That is the example crate's job
//! (plan steps 27 and 28), which has a `build.rs` of its own.
//!
//! # Why the dialect here is written by hand
//!
//! A generated file always carries the whole of FIX 4.4 beside what the
//! overlay adds. `[measured 2026-09-26]` `codegen::generate` over
//! `crates/dict/tests/fixtures/overlay-invented.xml` writes **727 433 bytes**
//! (its tag 20000 widens every bitset), and over an overlay holding only the
//! group below, **282 347 bytes**. A fixture that size, committed and pinned so
//! it cannot rot, would be the largest file in this crate to test four lines of
//! type plumbing. And generating it in a build script would make
//! `fixbolt-dict/codegen` — and `roxmltree` — a build-dependency of `fixbolt`
//! for every user. So [`Venue`] is `Fix44` with one invented group changed,
//! written against `fixbolt::dict` the way a generated file is; the generated
//! path is proven end to end where a real `build.rs` exists.
//!
//! The expected bytes are literals, computed once outside this crate (body
//! length and checksum by hand), for the reason `tests/reply.rs` gives:
//! building them with `TemplateBuilder` would agree with the implementation by
//! construction.

// A test binary, not a library crate: non-negotiable 7 is about what ships.
#![allow(clippy::indexing_slicing)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use fixbolt::dict::{Dictionary, FieldType, Fix44, Tables};
use fixbolt::{Answer, App, Application, GroupData, GroupEntryData, Handler, Incoming, Reply, app};
use fixbolt_session::Header;

/// `52`, as the session hands it over.
const STAMP: &[u8] = b"20260902-10:00:00.123";

fn show(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).replace('\x01', "|")
}

// ---------------------------------------------------------------------------
// The dialect. INVENTED FOR TESTS, as `overlay-invented.xml` is: no venue's
// rules of engagement, and the same tags and message types as that fixture.
// ---------------------------------------------------------------------------

const NO_VENUE_FEES: u32 = 5003;
const VENUE_FEE_TYPE: u32 = 5004;
const VENUE_FEE_AMT: u32 = 5005;
const CURRENCY: u32 = 15;

/// `VenueFeeReport (35=U1)` declares `NoVenueFees` amount first; `NewOrderSingle
/// (35=D)` reaches the same counter through a component that declares the type
/// first. One counter, two delimiters — the trap
/// `a_group_reused_in_two_messages_keeps_each_delimiter` guards in the
/// generator.
const U1_FEES: &[u32] = &[VENUE_FEE_AMT, VENUE_FEE_TYPE, CURRENCY];
const D_FEES: &[u32] = &[VENUE_FEE_TYPE, VENUE_FEE_AMT];

/// FIX 4.4, plus `NoVenueFees` in two messages and the message type `U1`.
struct Venue;

impl Venue {
    fn fees(msg_type: &[u8], counter: u32) -> Option<&'static [u32]> {
        match (msg_type, counter) {
            (b"U1", NO_VENUE_FEES) => Some(U1_FEES),
            (b"D", NO_VENUE_FEES) => Some(D_FEES),
            _ => None,
        }
    }

    const fn is_venue_tag(tag: u32) -> bool {
        matches!(tag, NO_VENUE_FEES | VENUE_FEE_TYPE | VENUE_FEE_AMT)
    }
}

impl Dictionary for Venue {
    fn is_header(tag: u32) -> bool {
        <Fix44 as Dictionary>::is_header(tag)
    }

    fn data_length_tag(tag: u32) -> Option<u32> {
        <Fix44 as Dictionary>::data_length_tag(tag)
    }

    fn group_delimiter(msg_type: &[u8], counter: u32) -> Option<u32> {
        match Self::fees(msg_type, counter) {
            Some(members) => members.first().copied(),
            None => <Fix44 as Dictionary>::group_delimiter(msg_type, counter),
        }
    }

    fn group_members(msg_type: &[u8], counter: u32) -> &'static [u32] {
        Self::fees(msg_type, counter)
            .unwrap_or_else(|| <Fix44 as Dictionary>::group_members(msg_type, counter))
    }

    fn group_order(msg_type: &[u8], counter: u32) -> &'static [u32] {
        Self::fees(msg_type, counter)
            .unwrap_or_else(|| <Fix44 as Dictionary>::group_order(msg_type, counter))
    }
}

impl Tables for Venue {
    fn is_defined_tag(tag: u32) -> bool {
        Self::is_venue_tag(tag) || <Fix44 as Tables>::is_defined_tag(tag)
    }

    fn is_defined_tag_for(_msg_type: &[u8], tag: u32) -> bool {
        Self::is_defined_tag(tag)
    }

    fn required_header() -> &'static [u32] {
        <Fix44 as Tables>::required_header()
    }

    fn required(msg_type: &[u8]) -> &'static [u32] {
        match msg_type {
            b"U1" => &[11],
            _ => <Fix44 as Tables>::required(msg_type),
        }
    }

    fn allows(msg_type: &[u8], tag: u32) -> bool {
        match msg_type {
            b"U1" => <Self as Dictionary>::is_header(tag) || tag == 11 || U1_FEES.contains(&tag),
            b"D" if Self::is_venue_tag(tag) => true,
            _ => <Fix44 as Tables>::allows(msg_type, tag),
        }
    }

    fn enum_allows(tag: u32, value: &[u8]) -> Option<bool> {
        <Fix44 as Tables>::enum_allows(tag, value)
    }

    fn field_type(tag: u32) -> Option<FieldType> {
        match tag {
            NO_VENUE_FEES => Some(FieldType::NumInGroup),
            VENUE_FEE_TYPE => Some(FieldType::Char),
            VENUE_FEE_AMT => Some(FieldType::Amt),
            _ => <Fix44 as Tables>::field_type(tag),
        }
    }

    fn is_msg_type(msg_type: &[u8]) -> bool {
        msg_type == b"U1" || <Fix44 as Tables>::is_msg_type(msg_type)
    }

    fn is_admin(msg_type: &[u8]) -> bool {
        <Fix44 as Tables>::is_admin(msg_type)
    }
}

/// Two fee entries, each supplied in an order neither message declares.
fn fee_entries() -> [GroupEntryData<'static>; 2] {
    [
        GroupEntryData {
            fields: &[
                (CURRENCY, b"USD"),
                (VENUE_FEE_TYPE, b"1"),
                (VENUE_FEE_AMT, b"0.25"),
            ],
            groups: &[],
        },
        GroupEntryData {
            fields: &[
                (VENUE_FEE_TYPE, b"2"),
                (CURRENCY, b"USD"),
                (VENUE_FEE_AMT, b"0.10"),
            ],
            groups: &[],
        },
    ]
}

/// **The specification of the parameter.** A reply over [`Venue`] writes
/// `NoVenueFees` in `U1`'s declared order — amount, type, currency — and the
/// same counter in `D` in *its* declared order — type, amount. `Fix44` knows
/// neither group, so a reply that still went through `Fix44` is
/// `Answer::Failed(Encode(UnknownGroup(5003)))`, and one that ordered by tag
/// would put `5004` first in `U1`.
#[test]
fn a_reply_over_a_dialect_orders_its_custom_group_by_the_dialect() {
    let entries = fee_entries();

    // --- 35=U1: amount first ---
    let mut out = [0u8; 512];
    let answer =
        Reply::<64, 1024, Venue>::new(b"FIX.4.4", 7, STAMP, None, b"US", b"ALPHA", &mut out)
            .message(b"U1")
            .field(11, b"ORD1")
            .group(NO_VENUE_FEES)
            .send_with_groups(&[GroupData {
                counter: NO_VENUE_FEES,
                entries: &entries,
            }]);
    const U1_EXPECTED: &[u8] = b"8=FIX.4.4\x019=114\x0135=U1\x0134=7\x0149=US\x0152=20260902-10:00:00.123\x0156=ALPHA\x0111=ORD1\x015003=2\x015005=0.25\x015004=1\x0115=USD\x015005=0.10\x015004=2\x0115=USD\x0110=107\x01";
    assert_eq!(
        wire(&answer, &out),
        show(U1_EXPECTED),
        "35=U1 over Venue was not written in Venue's order"
    );

    // --- 35=D: the same counter, type first, and no currency member ---
    let d_entries = [GroupEntryData {
        fields: &[(VENUE_FEE_AMT, b"0.25"), (VENUE_FEE_TYPE, b"1")],
        groups: &[],
    }];
    let mut out = [0u8; 512];
    let answer =
        Reply::<64, 1024, Venue>::new(b"FIX.4.4", 7, STAMP, None, b"US", b"ALPHA", &mut out)
            .message(b"D")
            .field(11, b"ORD1")
            .group(NO_VENUE_FEES)
            .send_with_groups(&[GroupData {
                counter: NO_VENUE_FEES,
                entries: &d_entries,
            }]);
    const D_EXPECTED: &[u8] = b"8=FIX.4.4\x019=82\x0135=D\x0134=7\x0149=US\x0152=20260902-10:00:00.123\x0156=ALPHA\x0111=ORD1\x015003=1\x015004=1\x015005=0.25\x0110=220\x01";
    assert_eq!(
        wire(&answer, &out),
        show(D_EXPECTED),
        "35=D over Venue was not written in Venue's order"
    );
}

/// What reached the wire, or the answer itself when nothing did — so a red
/// names `Failed(Encode(UnknownGroup(5003)))` rather than an empty string.
fn wire(answer: &Answer, out: &[u8]) -> String {
    match answer {
        Answer::Sent(r) => show(&out[r.clone()]),
        other => format!("{other:?}"),
    }
}

/// A handler written the way every handler before ADR-0207 was: it names no
/// dictionary, and echoes the parties of an order back in an order FIX 4.4
/// does not declare.
struct Parties;

impl Handler for Parties {
    fn on_message(&mut self, msg: &Incoming<'_>, reply: Reply<'_, 64, 1024>) -> Answer {
        reply
            .message(b"8")
            .field(37, b"E1")
            .field(11, msg.get(11).unwrap_or_default())
            .group(453)
            .send_with_groups(&[GroupData {
                counter: 453,
                entries: &[GroupEntryData {
                    fields: &[(452, b"1"), (447, b"D"), (448, b"P1")],
                    groups: &[],
                }],
            }])
    }
}

/// **The default is FIX 4.4, and nothing that named no dictionary changes.**
///
/// The type identity is the half the compiler checks: before the parameter
/// existed, `App<H, 256, 64, 1024, Fix44>` did not name a type at all. The
/// bytes are the half it cannot: an order parsed by FIX 4.4 and an
/// `ExecutionReport` whose `NoPartyIDs (453)` comes out in FIX 4.4's declared
/// order — `448`, `447`, `452` — whatever order the handler named them in.
#[test]
fn an_app_over_the_default_is_fix44() {
    fn is_over_fix44<H: Handler>(a: App<H>) -> App<H, 256, 64, 1024, Fix44> {
        a
    }
    let mut app = is_over_fix44(app(Parties));

    const ORDER: &[u8] = b"8=FIX.4.4\x019=83\x0135=D\x0134=2\x0149=ALPHA\x0152=20260902-10:00:00.123\x0156=US\x0111=ORD1\x01453=1\x01448=P1\x01447=D\x01452=1\x0110=049\x01";
    let mut out = [0u8; 512];
    let range = app.on_message(
        ORDER,
        Header {
            seq: 7,
            stamp: STAMP,
            last_processed: None,
        },
        &mut out,
    );

    const EXPECTED: &[u8] = b"8=FIX.4.4\x019=89\x0135=8\x0134=7\x0149=US\x0152=20260902-10:00:00.123\x0156=ALPHA\x0111=ORD1\x0137=E1\x01453=1\x01448=P1\x01447=D\x01452=1\x0110=078\x01";
    assert_eq!(
        (app.unparsable(), app.failed_replies()),
        (0, 0),
        "the default App could not parse the order or write its reply"
    );
    let Some(range) = range else {
        panic!("the default App answered nothing");
    };
    assert_eq!(
        show(&out[range]),
        show(EXPECTED),
        "the default App did not answer in FIX 4.4's order"
    );
}
