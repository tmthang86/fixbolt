//! The venue's dialect, over a real socket (ADR-0207; plan
//! `2026-09-26-docs-for-embedders` step 27).
//!
//! **What this proves:** a type `fixbolt_dict::codegen::generate` wrote in
//! this crate's `build.rs` from `venue.xml` reaches both halves of a running
//! acceptor through `fixbolt::serve_over` — the session validates by it and
//! the handler reads and writes by it. Each thing `venue.xml` adds is sent
//! through TCP and answered: a custom tag, a custom value on a standard enum, a
//! custom repeating group (echoed, and its order on the wire is `venue.xml`'s,
//! not the handler's), a custom header field, a custom message type, and a
//! field the venue makes required. And the dialect does not loosen what it
//! does not define: an undefined tag below 5000 is still `373=0`, and one at or
//! above it passes only when the counterparty is configured to skip them.
//!
//! **What it does not prove:** anything about a real venue's rules of
//! engagement — `venue.xml` is invented and none may be committed here
//! (ADR-0207 *Consequences*). That a generated type scores the 59 QuickFIX
//! definitions is a separate question, asked of an empty overlay.
#![cfg(all(feature = "standard", unix))]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
// A test binary: non-negotiable 7 is about what ships.
#![allow(clippy::indexing_slicing)]

use std::io::{ErrorKind, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::{Duration, Instant};

use custom_dictionary::Desk;
use custom_dictionary::venue::Venue;
use fixbolt::dict::{Fix44, TagValue};
use fixbolt::{App, Config, DictionaryChecks, Handles, Limits, NoLog, NoRecovery, Store, Table};

/// An acceptor over `Venue` on a free port, serving `TW44` as `ISLD` with
/// `checks`; the address to dial.
fn serve(checks: DictionaryChecks) -> String {
    let l = TcpListener::bind("127.0.0.1:0").expect("a free port");
    let addr = l.local_addr().expect("bound").to_string();
    drop(l);
    let a = addr.clone();
    std::thread::spawn(move || {
        let cfg = Config::acceptor(b"FIX.4.4", b"ISLD", b"TW44").with_validation(checks);
        let _ = fixbolt::serve_over::<256, 4096, 8192, 1024, TagValue<Venue, 256>, _, Store, _, _>(
            &a,
            Table::new().serving(cfg),
            App::<Desk, 256, 64, 1024, Venue>::with_sizes(Desk::default()),
            4,
            Limits::new(8, 30_000).expect("both above zero"),
            NoRecovery,
            NoLog,
            Handles::new(),
        );
    });
    addr
}

/// A logged-on counterparty.
struct Client {
    sock: TcpStream,
    buf: Vec<u8>,
    seq: u32,
}

impl Client {
    fn logged_on(checks: DictionaryChecks) -> Self {
        Self::logged_on_at(&serve(checks))
    }

    fn logged_on_at(addr: &str) -> Self {
        let deadline = Instant::now() + Duration::from_secs(5);
        let sock = loop {
            if let Ok(s) = TcpStream::connect(addr) {
                break s;
            }
            assert!(
                Instant::now() < deadline,
                "the acceptor never came up on {addr}"
            );
            std::thread::sleep(Duration::from_millis(10));
        };
        sock.set_nodelay(true).expect("nodelay");
        sock.set_read_timeout(Some(Duration::from_millis(100)))
            .expect("timeout");
        let mut c = Self {
            sock,
            buf: Vec::new(),
            seq: 0,
        };
        c.send("A", "98=0\u{1}108=30\u{1}");
        let logon = c.next();
        assert!(
            logon.contains("|35=A|"),
            "the logon was not answered: {logon}"
        );
        c
    }

    /// Send one message of type `msg_type`: the header is written here, and
    /// `header_extra` goes into it, after `56=`.
    fn send_with_header(&mut self, msg_type: &str, header_extra: &str, body: &str) {
        self.seq += 1;
        let inner = format!(
            "35={msg_type}\u{1}34={}\u{1}49=TW44\u{1}52={}\u{1}56=ISLD\u{1}{header_extra}{body}",
            self.seq,
            stamp()
        );
        let head = format!("8=FIX.4.4\u{1}9={}\u{1}{inner}", inner.len());
        let sum: u32 = head.bytes().map(u32::from).sum();
        let wire = format!("{head}10={:03}\u{1}", sum % 256);
        self.sock.write_all(wire.as_bytes()).expect("send");
    }

    fn send(&mut self, msg_type: &str, body: &str) {
        self.send_with_header(msg_type, "", body);
    }

    /// The next whole message from the acceptor, `|`-separated.
    fn next(&mut self) -> String {
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut chunk = [0u8; 8192];
        loop {
            if let Some(end) = message_end(&self.buf) {
                let msg: Vec<u8> = self.buf.drain(..end).collect();
                return String::from_utf8_lossy(&msg).replace('\u{1}', "|");
            }
            assert!(
                Instant::now() < deadline,
                "no whole message in five seconds; have: {}",
                String::from_utf8_lossy(&self.buf).replace('\u{1}', "|")
            );
            match self.sock.read(&mut chunk) {
                Ok(0) => panic!("the acceptor closed the connection"),
                Ok(n) => self.buf.extend_from_slice(&chunk[..n]),
                Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
                Err(e) => panic!("read: {e}"),
            }
        }
    }
}

/// Where the first whole message in `buf` ends: after `\x0110=NNN\x01`.
fn message_end(buf: &[u8]) -> Option<usize> {
    let at = buf.windows(4).position(|w| w == b"\x0110=")?;
    let end = at + 4 + buf[at + 4..].iter().position(|&b| b == 1)? + 1;
    Some(end)
}

fn stamp() -> String {
    let ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("after 1970")
        .as_millis() as u64;
    let mut cache = fixbolt_codec::timestamp::TimestampCache::new();
    let full = cache.format(ms, 0);
    String::from_utf8_lossy(&full[..17]).into_owned()
}

/// A valid FIX 4.4 order body, with the venue's required `VenueClientID`.
fn order(extra: &str) -> String {
    format!(
        "11=ORD-1\u{1}21=1\u{1}38=100\u{1}40=2\u{1}44=10\u{1}54=1\u{1}55=IBM\u{1}\
         60={}\u{1}5001=ACCT-7\u{1}{extra}",
        stamp()
    )
}

fn checked() -> DictionaryChecks {
    DictionaryChecks::new()
}

#[test]
fn a_custom_tag_reaches_the_handler() {
    let mut c = Client::logged_on(checked());
    c.send("D", &order(""));
    let got = c.next();
    assert!(
        got.contains("|35=8|") && got.contains("|5001=ACCT-7|"),
        "VenueClientID (5001) did not reach the handler and come back: {got}"
    );
}

#[test]
fn a_custom_enum_value_is_not_rejected() {
    let mut c = Client::logged_on(checked());
    c.send("D", &order("").replace("40=2\u{1}", "40=Z\u{1}"));
    let got = c.next();
    assert!(
        got.contains("|35=8|") && got.contains("|40=Z|"),
        "OrdType=Z, which venue.xml adds, was not accepted: {got}"
    );
}

/// The entries go out amount first — `venue.xml`'s declared order — although
/// `Desk` names each one type first. The comparison is positional, which is
/// the whole of non-negotiable 5.
#[test]
fn a_custom_group_is_read_and_echoed_in_declared_order() {
    let mut c = Client::logged_on(checked());
    c.send(
        "D",
        &order("5003=2\u{1}5005=0.25\u{1}5004=1\u{1}5005=0.10\u{1}5004=2\u{1}"),
    );
    let got = c.next();
    assert!(
        got.contains("|35=8|") && got.contains("|5003=2|5005=0.25|5004=1|5005=0.10|5004=2|"),
        "NoVenueFees was not read and echoed in its declared order: {got}"
    );
}

/// Sent in the header, accepted there, and written back into the header —
/// after `56=`, before the first body field — because `Venue::is_header` says
/// so, not because the handler put it there.
#[test]
fn a_custom_header_field_is_written_in_the_header() {
    let mut c = Client::logged_on(checked());
    c.send_with_header("D", "5002=S-1\u{1}", &order(""));
    let got = c.next();
    assert!(
        got.contains("|56=TW44|5002=S-1|6=0|"),
        "VenueSessionTag (5002) was not written in the header: {got}"
    );
}

#[test]
fn a_custom_message_type_is_delivered() {
    let mut c = Client::logged_on(checked());
    c.send("U1", "11=FEE-1\u{1}");
    let got = c.next();
    assert!(
        got.contains("|35=U2|") && got.contains("|11=FEE-1|"),
        "VenueFeeReport (U1) was not delivered to the handler: {got}"
    );
}

#[test]
fn a_missing_venue_required_field_is_rejected_373_1() {
    let mut c = Client::logged_on(checked());
    c.send("D", &order("").replace("5001=ACCT-7\u{1}", ""));
    let got = c.next();
    assert!(
        got.contains("|35=3|") && got.contains("|371=5001|") && got.contains("|373=1|"),
        "an order without VenueClientID was not rejected 373=1 naming 5001: {got}"
    );
}

#[test]
fn an_undefined_tag_is_still_rejected_373_0() {
    let mut c = Client::logged_on(checked());
    c.send("D", &order("4000=X\u{1}"));
    let got = c.next();
    assert!(
        got.contains("|35=3|") && got.contains("|371=4000|") && got.contains("|373=0|"),
        "tag 4000, which neither FIX 4.4 nor venue.xml defines, was not rejected 373=0: {got}"
    );
}

/// `ValidateUserDefinedFields=N`'s meaning is unchanged by a dialect: it
/// governs the tags the dialect does **not** define (ADR-0207 decision 7).
#[test]
fn an_undefined_user_tag_passes_when_user_defined_fields_are_skipped() {
    let mut c = Client::logged_on(DictionaryChecks::new().skipping_user_defined_fields());
    c.send("D", &order("5999=X\u{1}"));
    let got = c.next();
    assert!(
        got.contains("|35=8|"),
        "tag 5999, undefined but user-defined, was refused while skipped: {got}"
    );
}

/// **The `App`'s dictionary and the door's are not tied by the types**
/// (ADR-0207 decision 5, `GUIDE.md` §3a). An `App` over `Venue` behind a door
/// told `TagValue<Fix44, 256>` compiles — this test is that proof — and the
/// session, validating by FIX 4.4, refuses the venue's own tag before the
/// handler that knows it is asked.
#[test]
fn a_venue_app_behind_a_fix44_door_compiles_and_the_session_rejects_the_venue_tag_373_0() {
    let l = TcpListener::bind("127.0.0.1:0").expect("a free port");
    let addr = l.local_addr().expect("bound").to_string();
    drop(l);
    let a = addr.clone();
    std::thread::spawn(move || {
        let _ = fixbolt::serve_over::<256, 4096, 8192, 1024, TagValue<Fix44, 256>, _, Store, _, _>(
            &a,
            Table::new().serving(Config::acceptor(b"FIX.4.4", b"ISLD", b"TW44")),
            // The mismatch: the handler reads and writes by `Venue`.
            App::<Desk, 256, 64, 1024, Venue>::with_sizes(Desk::default()),
            4,
            Limits::new(8, 30_000).expect("both above zero"),
            NoRecovery,
            NoLog,
            Handles::new(),
        );
    });
    let mut c = Client::logged_on_at(&addr);
    c.send("D", &order(""));
    let got = c.next();
    assert!(
        got.contains("|35=3|") && got.contains("|371=5001|") && got.contains("|373=0|"),
        "behind a FIX 4.4 door, VenueClientID (5001) must be refused 373=0 by the session: {got}"
    );
}
