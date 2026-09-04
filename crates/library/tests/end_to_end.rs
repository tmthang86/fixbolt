//! A file on disk, a real listener, a real socket, and an order that gets
//! filled.
//!
//! **Step 4 of [the-library-layer].** `tests/reply.rs` proves the bytes a
//! [`fixbolt::Reply`] lays out; this proves the whole path — `Settings::load`,
//! `Table`, `serve`, kernel sockets, the framer, the session, `App`, the
//! handler, and back out. `crates/engine/tests/registry.rs` and
//! `crates/engine/tests/on_disk.rs` both found that a layer can be finished
//! while the seam above it is never asked about, and this crate is nothing but
//! a seam.
//!
//! # It runs the example's own handler
//!
//! `#[path]` below pulls in `examples/shared/order_handler.rs`, the same file
//! `examples/acceptor.rs` compiles. A second copy written for the test would
//! prove that the copy works.
//!
//! **What this does not cover, said out loud:** `examples/acceptor.rs`'s own
//! `main` — its two command-line arguments and the two lines it prints. Every
//! other line of that example is exercised here.
//!
//! # The checksum is computed here rather than imported
//!
//! `fixbolt_codec::checksum` is on the other side of this conversation. A
//! client that agrees with the acceptor by construction cannot catch the
//! acceptor being wrong, so the three lines are written out.
//!
//! [the-library-layer]: ../../../docs/plans/2026-09-02-the-library-layer.md
#![cfg(all(feature = "standard", unix))]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "../examples/shared/order_handler.rs"]
mod order_handler;

use std::io::{ErrorKind, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::time::Duration;

use fixbolt::{Limits, Settings};

const SOH: char = '\u{1}';

/// `8=`, `9=` and `10=` around a body — the client's own framing.
fn frame(body: &str) -> Vec<u8> {
    let prefix = format!("8=FIX.4.4{SOH}9={}{SOH}", body.len());
    let whole = format!("{prefix}{body}");
    let sum: u32 = whole.bytes().map(u32::from).sum();
    format!("{whole}10={:03}{SOH}", sum % 256).into_bytes()
}

fn now_stamp() -> String {
    let ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("after 1970")
        .as_millis() as u64;
    let mut cache = fixbolt_codec::timestamp::TimestampCache::new();
    let full = *cache.format(ms);
    // 17 bytes: the corpus's own width for `52` on the way in.
    core::str::from_utf8(&full[..17]).expect("ascii").to_owned()
}

/// A `Logon`, stamped **now**.
///
/// Not the corpus's fixed instant: the serving loop reads the real clock, and
/// `max_skew_ms` would refuse a stamp from 2024 — with the same silence as an
/// unconfigured identity, which is the confusion
/// `docs/reference/two-time-rules-share-one-observable.md` was written about.
fn logon(who: &str, seq: u32) -> Vec<u8> {
    frame(&format!(
        "35=A{SOH}34={seq}{SOH}49={who}{SOH}52={}{SOH}56=ISLD{SOH}98=0{SOH}108=30{SOH}",
        now_stamp()
    ))
}

/// A `NewOrderSingle`. Body fields in ascending order, as a real client sends
/// them — the handler's own ordering is tested in `tests/reply.rs`.
fn new_order(who: &str, seq: u32, cl_ord_id: &str, qty: &str, price: &str) -> Vec<u8> {
    let stamp = now_stamp();
    frame(&format!(
        "35=D{SOH}34={seq}{SOH}49={who}{SOH}52={stamp}{SOH}56=ISLD{SOH}\
         11={cl_ord_id}{SOH}21=1{SOH}38={qty}{SOH}40=2{SOH}44={price}{SOH}\
         54=1{SOH}55=IBM{SOH}59=0{SOH}60={stamp}{SOH}"
    ))
}

fn scratch(name: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!(
        "fixbolt-library-e2e-{name}-{}.cfg",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&p);
    p
}

fn free_port() -> String {
    let l = TcpListener::bind("127.0.0.1:0").expect("a free port");
    let a = l.local_addr().expect("bound").to_string();
    drop(l);
    a
}

fn connect(addr: &str) -> TcpStream {
    for _ in 0..200 {
        if let Ok(s) = TcpStream::connect(addr) {
            // `[measured 2026-08-30]` without this the wire gate read 39/59 on
            // Linux, and the cause was Nagle on the HARNESS's socket rather
            // than anything in the engine. See docs/reference/measured-costs.md.
            s.set_nodelay(true).expect("nodelay");
            s.set_read_timeout(Some(Duration::from_secs(5)))
                .expect("timeout");
            return s;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("the serving loop never came up on {addr}");
}

/// One read, as a readable string. Empty means the acceptor said nothing and
/// **closed the socket** — a refusal, which is a different fault from a hung
/// connection and is not folded together with it here.
fn read_one(c: &mut TcpStream) -> String {
    let mut buf = [0u8; 8192];
    match c.read(&mut buf) {
        Ok(0) => String::new(),
        Ok(n) => String::from_utf8_lossy(&buf[..n]).replace(SOH, "|"),
        Err(e) if matches!(e.kind(), ErrorKind::ConnectionReset) => String::new(),
        Err(e) => panic!(
            "the acceptor neither answered nor closed the socket within the \
             read timeout — a hung connection, not a refusal: {e}"
        ),
    }
}

/// Start `serve` on its own thread, running the **example's** handler.
fn serving(name: &str) -> String {
    let path = scratch(name);
    std::fs::write(
        &path,
        "[DEFAULT]\nBeginString=FIX.4.4\nSenderCompID=ISLD\n\
         [SESSION]\nTargetCompID=TW44\nHeartBtInt=30\n",
    )
    .expect("the scratch directory is writable");
    let table = Settings::load(&path)
        .unwrap_or_else(|e| panic!("{e}"))
        .into_table();
    let _ = std::fs::remove_file(&path);
    assert_eq!(table.len(), 1, "the file names exactly one counterparty");

    let addr = free_port();
    let serving = addr.clone();
    std::thread::spawn(move || {
        let _ = fixbolt::serve(
            &serving,
            table,
            fixbolt::app(order_handler::Desk::default()),
            4,
            Limits::new(8, 30_000).expect("both above zero"),
            fixbolt::NoLog,
        );
    });
    addr
}

/// **The specification.** An order in, an `ExecutionReport` out, with every
/// field the application never named already on it.
#[test]
fn an_order_through_a_real_socket_comes_back_filled() {
    let addr = serving("filled");
    let mut c = connect(&addr);

    c.write_all(&logon("TW44", 1)).expect("send the logon");
    let hello = read_one(&mut c);
    assert!(
        hello.contains("|35=A|"),
        "the premise: the acceptor logs this counterparty on — got {hello:?}"
    );

    c.write_all(&new_order("TW44", 2, "ORD-1", "100", "42"))
        .expect("send the order");
    let er = read_one(&mut c);

    // The handler named none of these four. Every one of them is here because
    // `Reply` put it there.
    assert!(er.contains("|35=8|"), "an ExecutionReport: {er:?}");
    assert!(
        er.contains("|49=ISLD|"),
        "49 is this acceptor, taken from the order's 56 — not the handler's: {er:?}"
    );
    assert!(
        er.contains("|56=TW44|"),
        "56 is the counterparty, taken from the order's 49 — the reversal: {er:?}"
    );
    assert!(
        er.contains("|34=2|"),
        "34 is the session's second outbound message, not anything the desk \
         counted: {er:?}"
    );
    assert!(
        er.contains("|52=20"),
        "52 is the session's clock, and it is present: {er:?}"
    );

    // And what the handler did name, echoed and filled.
    assert!(er.contains("|11=ORD-1|"), "the client's order id: {er:?}");
    assert!(er.contains("|55=IBM|"), "the symbol: {er:?}");
    assert!(er.contains("|31=42|"), "filled at the price sent: {er:?}");
    assert!(er.contains("|32=100|"), "for the whole quantity: {er:?}");
    assert!(er.contains("|151=0|"), "with nothing left: {er:?}");
    assert!(er.contains("|37=EXEC-1|"), "the first fill: {er:?}");

    // The order the fields came out in is the dictionary's, not the handler's:
    // the handler named 37 first and 6 last, and 6 sorts before 37.
    let six = er.find("|6=").expect("AvgPx is present");
    let thirty_seven = er.find("|37=").expect("OrderID is present");
    assert!(
        six < thirty_seven,
        "the handler named 37 before 6 and the dictionary says otherwise: {er:?}"
    );
}

/// A second order gets a second exec id, which is how the handler's own state
/// is seen from the wire — it has no accessor.
#[test]
fn the_desk_keeps_counting_across_orders() {
    let addr = serving("counting");
    let mut c = connect(&addr);

    c.write_all(&logon("TW44", 1)).expect("send the logon");
    assert!(read_one(&mut c).contains("|35=A|"), "the premise");

    c.write_all(&new_order("TW44", 2, "ORD-1", "100", "42"))
        .expect("first order");
    assert!(
        read_one(&mut c).contains("|37=EXEC-1|"),
        "the first fill is EXEC-1"
    );

    c.write_all(&new_order("TW44", 3, "ORD-2", "50", "43"))
        .expect("second order");
    let second = read_one(&mut c);
    assert!(second.contains("|37=EXEC-2|"), "the second: {second:?}");
    assert!(
        second.contains("|34=3|"),
        "and the session's number moved with it: {second:?}"
    );
}

/// **The control.** A handler that answers `silent()` puts nothing on the wire.
///
/// Without this, every assertion above could be read as *"the acceptor sends an
/// ExecutionReport"* rather than *"the handler decided to"*. The order here is
/// a `NewOrderSingle` with no `44=Price`, which is the one case
/// `order_handler.rs` refuses.
#[test]
fn an_order_the_desk_refuses_puts_nothing_on_the_wire() {
    let addr = serving("silent");
    let mut c = connect(&addr);

    c.write_all(&logon("TW44", 1)).expect("send the logon");
    assert!(read_one(&mut c).contains("|35=A|"), "the premise");

    let stamp = now_stamp();
    let priceless = frame(&format!(
        "35=D{SOH}34=2{SOH}49=TW44{SOH}52={stamp}{SOH}56=ISLD{SOH}\
         11=ORD-9{SOH}21=1{SOH}38=100{SOH}40=1{SOH}54=1{SOH}55=IBM{SOH}\
         59=0{SOH}60={stamp}{SOH}"
    ));
    c.write_all(&priceless).expect("send the order");

    // A short read timeout, because what is being proven is an absence. Five
    // seconds of nothing is the same evidence as one, and thirty times slower.
    c.set_read_timeout(Some(Duration::from_millis(750)))
        .expect("timeout");
    let mut buf = [0u8; 8192];
    match c.read(&mut buf) {
        Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
        Ok(n) => panic!(
            "the desk refused this order and something was sent anyway: {:?}",
            String::from_utf8_lossy(&buf[..n]).replace(SOH, "|")
        ),
        Err(e) => panic!("unexpected error waiting for silence: {e}"),
    }
}
