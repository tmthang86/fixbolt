//! **No secret reaches the disk.** ADR-0097 phase-3 exit criterion 4, and the
//! gate ADR-0110 decision 7 names.
//!
//! # What is a secret
//!
//! `554` Password and `925` NewPassword in every message; `96` RawData on a
//! `Logon` (`35=A`) or a `UserRequest` (`35=BE`). ADR-0110 decision 1 is the
//! list and `fixbolt_engine::redact::MASKED` is where it lives in code.
//!
//! # What these tests read
//!
//! **The files, as bytes.** A secret that reached the disk is a byte-string in
//! a file, whatever API did or did not put it there, so the file is what is
//! examined — both the message log (`FileLog`) and the journal (`FileJournal`,
//! under both `Durability` policies).
//!
//! # Why every test carries a positive premise
//!
//! A test that finds no secret in an empty file proves nothing. So each one
//! also shows that the secret **did** cross the wire, that the journal file
//! **does** hold the non-secret application reply, and that the log **does**
//! hold the inbound `Logon` line — the failure mode *"green because nothing was
//! written"* named in the plan's *Bẫy đã lường trước*.
//!
//! Plan: `docs/plans/2026-09-23-p3-redact-secrets.md`.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
// Not a library crate's source: non-negotiable 7 is about `crates/*/src`, and
// `scripts/check-indexing-debt.sh` counts nothing outside it.
#![allow(clippy::indexing_slicing)]

use std::path::PathBuf;

use fixbolt_engine::journal::{Durability, FileJournal, Reader, Record};
use fixbolt_engine::msglog::{Direction, FileLog, MessageLog};
use fixbolt_session::journal::Journal;

/// `554` on the `Logon`.
const S1: &[u8] = b"pwLOGONq7Xk2";
/// `96` on the `Logon`, **holding an SOH**: the two halves are checked apart,
/// because masking up to the next SOH would hide the first and leak the second.
const S2_FIRST: &[u8] = b"rawLOGONfirst";
const S2_SECOND: &[u8] = b"rawLOGONsecond";
/// `554` on the `UserRequest`.
const S3: &[u8] = b"pwUSERREQm4Tz";
/// `925` on the `UserRequest`.
const S4: &[u8] = b"newpwUSERREQv8Qa";
/// `96` on the `UserRequest`.
const S5: &[u8] = b"rawUSERREQp3Wd";

/// `S2` whole: `S2_FIRST ‖ SOH ‖ S2_SECOND`.
fn s2() -> Vec<u8> {
    let mut v = S2_FIRST.to_vec();
    v.push(0x01);
    v.extend_from_slice(S2_SECOND);
    v
}

/// Every secret this file uses, by the name a failure prints.
fn secrets() -> [(&'static str, &'static [u8]); 6] {
    [
        ("S1 (554 on Logon)", S1),
        ("S2 first half (96 on Logon)", S2_FIRST),
        ("S2 second half, after the SOH (96 on Logon)", S2_SECOND),
        ("S3 (554 on UserRequest)", S3),
        ("S4 (925 on UserRequest)", S4),
        ("S5 (96 on UserRequest)", S5),
    ]
}

fn contains(hay: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty() && hay.windows(needle.len()).any(|w| w == needle)
}

/// One sentence per secret found, naming the file it was found in.
fn leaks(file: &str, bytes: &[u8]) -> Vec<String> {
    secrets()
        .iter()
        .filter(|(_, s)| contains(bytes, s))
        .map(|(name, s)| {
            format!(
                "the {file} holds the secret `{}` — {name}",
                String::from_utf8_lossy(s)
            )
        })
        .collect()
}

fn tmp(name: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!(
        "fixbolt-secrets-{name}-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_file(&p);
    p
}

/// A whole FIX 4.4 message: `8=`, `9=` over `body`, `body`, a real `10=`.
///
/// `body` is SOH-terminated fields starting at `35=`. Built by hand rather
/// than through a builder because `96` must carry an SOH, and the length has to
/// count it.
fn frame(body: &[u8]) -> Vec<u8> {
    let mut m = format!("8=FIX.4.4\u{1}9={}\u{1}", body.len()).into_bytes();
    m.extend_from_slice(body);
    let sum: u8 = m.iter().fold(0u8, |a, &b| a.wrapping_add(b));
    m.extend_from_slice(format!("10={sum:03}\u{1}").as_bytes());
    m
}

/// `tag=value<SOH>` appended to `out`.
fn field(out: &mut Vec<u8>, tag: u32, value: &[u8]) {
    out.extend_from_slice(format!("{tag}=").as_bytes());
    out.extend_from_slice(value);
    out.push(0x01);
}

/// A `Logon` carrying `553`, `554=S1`, `95`, `96=S2`.
fn logon(seq: u32, sender: &[u8], target: &[u8], stamp: &[u8]) -> Vec<u8> {
    let s2 = s2();
    let mut b = Vec::new();
    field(&mut b, 35, b"A");
    field(&mut b, 34, seq.to_string().as_bytes());
    field(&mut b, 49, sender);
    field(&mut b, 52, stamp);
    field(&mut b, 56, target);
    field(&mut b, 95, s2.len().to_string().as_bytes());
    field(&mut b, 96, &s2);
    field(&mut b, 98, b"0");
    field(&mut b, 108, b"30");
    field(&mut b, 553, b"alice");
    field(&mut b, 554, S1);
    frame(&b)
}

/// A `UserRequest` (`35=BE`) carrying `554=S3`, `925=S4`, `95`, `96=S5`.
fn user_request(seq: u32, sender: &[u8], target: &[u8], stamp: &[u8]) -> Vec<u8> {
    let mut b = Vec::new();
    field(&mut b, 35, b"BE");
    field(&mut b, 34, seq.to_string().as_bytes());
    field(&mut b, 49, sender);
    field(&mut b, 52, stamp);
    field(&mut b, 56, target);
    field(&mut b, 95, S5.len().to_string().as_bytes());
    field(&mut b, 96, S5);
    field(&mut b, 553, b"alice");
    field(&mut b, 554, S3);
    field(&mut b, 923, b"UR1");
    field(&mut b, 924, b"3");
    field(&mut b, 925, S4);
    frame(&b)
}

/// An `ExecutionReport` with nothing secret in it.
fn execution_report(seq: u32, sender: &[u8], target: &[u8], stamp: &[u8]) -> Vec<u8> {
    let mut b = Vec::new();
    field(&mut b, 35, b"8");
    field(&mut b, 34, seq.to_string().as_bytes());
    field(&mut b, 49, sender);
    field(&mut b, 52, stamp);
    field(&mut b, 56, target);
    field(&mut b, 6, b"0");
    field(&mut b, 11, b"ORD1");
    field(&mut b, 14, b"0");
    field(&mut b, 17, b"EXEC1");
    field(&mut b, 37, b"ORDER1");
    field(&mut b, 39, b"0");
    field(&mut b, 54, b"1");
    field(&mut b, 55, b"FXB");
    field(&mut b, 150, b"0");
    field(&mut b, 151, b"100");
    frame(&b)
}

const FIXED_STAMP: &[u8] = b"20260923-10:32:07.000";

// ---------------------------------------------------------------------------
// (b) The message log, on its own.
// ---------------------------------------------------------------------------

/// A `Logon` recorded into a `FileLog` leaves no secret in the file, and the
/// line keeps its length.
#[test]
fn the_message_log_holds_no_secret_of_a_logon() {
    let path = tmp("log-alone");
    let wire = logon(1, b"TW44", b"ISLD", FIXED_STAMP);
    {
        let mut log = FileLog::open(&path).expect("a writable path");
        log.record(Direction::In, 63_900_000_000_000, 0, 7, &wire);
        log.close();
    }
    let bytes = std::fs::read(&path).expect("the file is there");
    let _ = std::fs::remove_file(&path);

    // The premise: something was written, and it is this Logon.
    let text = String::from_utf8_lossy(&bytes);
    let line = text
        .lines()
        .find(|l| l.contains(" IN  ") && l.contains("\u{1}35=A\u{1}"))
        .unwrap_or_else(|| panic!("the premise: an IN line with 35=A is in the log: {text:?}"));

    let found = leaks("message log", &bytes);
    assert!(found.is_empty(), "{}", found.join("; "));

    // The line still frames: masking keeps every byte count.
    let at = line.find("8=FIX.4.4").expect("the message is on the line");
    let logged = fixbolt_engine::msglog::unescape(&line[at..]);
    assert_eq!(
        logged.len(),
        wire.len(),
        "masking must keep the length, or `9=` stops describing the line"
    );
    let masked_pw = [b"\x01554=".as_slice(), &[b'*'; S1.len()], b"\x01"].concat();
    assert!(
        contains(&logged, &masked_pw),
        "`554=` is followed by as many `*` as the password had bytes: {:?}",
        String::from_utf8_lossy(&logged)
    );
}

/// A frame with **no `35=`** carrying `96` is masked in the log file.
///
/// ADR-0110 decision 1: a garbage frame is not known to be anything but a
/// sign-on. This is the other half of the `35=B` added to two `msglog.rs`
/// fixtures on 2026-09-23 (plan *Sửa 1*): those keep `96` because they are
/// News; this one has no type and loses it.
#[test]
fn raw_data_on_a_frame_with_no_msg_type_is_masked_in_the_log() {
    let path = tmp("log-no-type");
    let frame_no_type = [
        b"8=FIX.4.4\x0195=14\x0196=".as_slice(),
        S5,
        b"\x0110=000\x01",
    ]
    .concat();
    {
        let mut log = FileLog::open(&path).expect("a writable path");
        log.record(Direction::In, 63_900_000_000_000, 0, 7, &frame_no_type);
        log.close();
    }
    let bytes = std::fs::read(&path).expect("the file is there");
    let _ = std::fs::remove_file(&path);

    let masked = [b"\x0196=".as_slice(), &[b'*'; S5.len()], b"\x01"].concat();
    assert!(
        contains(&bytes, &masked),
        "the premise and the point: the line is there and `96=` is all `*`: {:?}",
        String::from_utf8_lossy(&bytes)
    );
    let found = leaks("message log", &bytes);
    assert!(found.is_empty(), "{}", found.join("; "));
}

/// **A garbage cut that swallows a sign-on keeps no RawData in clear.**
///
/// Senior review of PR #98, finding 1 (plan *Sửa 2*): an oversized `9=` makes
/// `Framer::cut` return `Cut::Garbage` over the whole buffer, the engine logs
/// that blob as one `In` record, and the first `35=` in it is a Heartbeat's.
/// Deciding `96` from the first `35=` alone left the `UserRequest`'s RawData
/// behind it in the file. Any `35=A`/`BE` in the record now decides.
#[test]
fn raw_data_behind_a_non_sign_on_msg_type_in_a_garbage_cut_is_masked_in_the_log() {
    use fixbolt_engine::frame::{Cut, Framer};

    let path = tmp("log-garbage-cut");
    let mut blob = b"8=FIX.4.4\x019=5000\x0135=0\x0134=5\x0110=000\x01".to_vec();
    blob.extend_from_slice(&user_request(6, b"TW44", b"ISLD", FIXED_STAMP));

    let mut framer: Framer<4096> = Framer::new();
    framer.spare()[..blob.len()].copy_from_slice(&blob);
    framer.filled(blob.len());
    // Pad to a full buffer: an oversized `9=` is garbage only once it cannot fit.
    let pad = 4096 - blob.len();
    framer.spare()[..pad].fill(b'x');
    framer.filled(pad);
    let Cut::Garbage(n) = framer.cut() else {
        panic!(
            "the premise: an oversized 9= is cut as garbage, saw {:?}",
            framer.cut()
        );
    };
    let record = framer.bytes(n);
    assert!(
        contains(record, S5),
        "the premise: the garbage cut carries the UserRequest's RawData"
    );
    {
        let mut log = FileLog::open(&path).expect("a writable path");
        log.record(Direction::In, 63_900_000_000_000, 0, 7, record);
        log.close();
    }
    let bytes = std::fs::read(&path).expect("the file is there");
    let _ = std::fs::remove_file(&path);
    assert!(
        contains(&bytes, b"\x0135=0\x01") && contains(&bytes, b"\x0135=BE\x01"),
        "the premise: the log line holds both frames of the cut"
    );
    let found = leaks("message log", &bytes);
    assert!(found.is_empty(), "{}", found.join("; "));
}

// ---------------------------------------------------------------------------
// (c) The journal, on its own, under both policies.
// ---------------------------------------------------------------------------

type Disk = FileJournal<8, 512>;

/// `put` a `UserRequest` carrying secrets at 2 and an `ExecutionReport` at 3,
/// close, then read the file as bytes and reopen it as a restart would.
fn journal_holds_no_secret(how: Durability, name: &str) {
    let path = tmp(name);
    let be = user_request(2, b"ISLD", b"TW44", FIXED_STAMP);
    let er = execution_report(3, b"ISLD", b"TW44", FIXED_STAMP);
    {
        let mut j: Disk = FileJournal::open(&path, how).expect("open");
        assert!(j.put(2, &be), "the ring keeps the UserRequest");
        assert!(j.put(3, &er), "the ring keeps the ExecutionReport");
        // In the same process the ring still answers with the bytes verbatim,
        // so a `ResendRequest` replays the real password — criterion 3.
        assert_eq!(
            j.get(2),
            Some(be.as_slice()),
            "the ring must keep the message verbatim for an in-process resend"
        );
        j.close();
    }
    let bytes = std::fs::read(&path).expect("the file is there");

    // The premise: the non-secret reply is in the file.
    assert!(
        contains(&bytes, b"\x0135=8\x01"),
        "the premise: the journal file holds the ExecutionReport ({how:?})"
    );

    let found = leaks("journal file", &bytes);
    assert!(found.is_empty(), "{how:?}: {}", found.join("; "));

    // After a restart the number has no bytes, and the count still covers it.
    let reopened: Disk = FileJournal::open(&path, how).expect("reopen");
    assert!(
        reopened.get(2).is_none(),
        "{how:?}: after a restart the UserRequest has no bytes, so it is gap-filled"
    );
    assert!(
        reopened.highest_out().is_some_and(|h| h >= 2),
        "{how:?}: the outbound count still covers the UserRequest's number: {:?}",
        reopened.highest_out()
    );
    assert!(
        reopened.get(3).is_some(),
        "{how:?}: the ExecutionReport is still replayable"
    );
    drop(reopened);

    let r = Reader::open(&path).expect("readable");
    let recs: Vec<Record<'_>> = r.records().collect();
    assert!(
        recs.contains(&Record::OutboundMark { seq: 2 }),
        "{how:?}: the file carries the outbound mark for 2 in the message's place: {recs:?}"
    );
    assert!(
        !recs
            .iter()
            .any(|x| matches!(x, Record::Message { seq: 2, .. })),
        "{how:?}: and no message record for 2"
    );
    let _ = std::fs::remove_file(&path);
}

#[test]
fn the_async_journal_file_holds_no_secret_of_a_user_request() {
    journal_holds_no_secret(Durability::Async, "journal-async");
}

#[test]
fn the_fsync_journal_file_holds_no_secret_of_a_user_request() {
    journal_holds_no_secret(Durability::Fsync, "journal-fsync");
}

// ---------------------------------------------------------------------------
// (a) Through the serving loop, over a kernel socket.
// ---------------------------------------------------------------------------

/// The serving loop with a log and a journal on disk. **`standard` only**, for
/// the reason `on_disk.rs` records: `serve*` builds the blocking engine.
#[cfg(all(feature = "standard", unix))]
mod serving {
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::ops::Range;
    use std::path::PathBuf;
    use std::time::{Duration, Instant};

    use fixbolt_engine::journal::{Durability, FileJournal, Reader, Record};
    use fixbolt_engine::msglog::FileLog;
    use fixbolt_engine::observe::Handles;
    use fixbolt_engine::presession::{Limits, Table};
    use fixbolt_engine::recovery::{Recovery, Resumed};
    use fixbolt_engine::{Application, Config};

    use super::{Disk, S3, contains, execution_report, field, frame, leaks, logon, user_request};

    /// Answers a `UserRequest` with a `UserRequest` carrying the same secrets,
    /// and a `NewOrderSingle` with an `ExecutionReport`.
    struct App;

    impl Application for App {
        fn on_message(
            &mut self,
            msg: &[u8],
            hdr: fixbolt_session::Header<'_>,
            out: &mut [u8],
        ) -> Option<Range<usize>> {
            let reply = if contains(msg, b"\x0135=BE\x01") {
                user_request(hdr.seq, b"ISLD", b"TW44", hdr.stamp)
            } else if contains(msg, b"\x0135=D\x01") {
                execution_report(hdr.seq, b"ISLD", b"TW44", hdr.stamp)
            } else {
                return None;
            };
            out.get_mut(..reply.len())?.copy_from_slice(&reply);
            Some(0..reply.len())
        }
    }

    fn cfg() -> Config {
        Config::acceptor(b"FIX.4.4", b"ISLD", b"TW44")
    }

    struct OnDisk {
        path: PathBuf,
    }

    impl Recovery<Disk> for OnDisk {
        fn fresh(&mut self, _cfg: &Config) -> Disk {
            FileJournal::open(&self.path, Durability::Async)
                .unwrap_or_else(|e| panic!("open journal: {e}"))
        }

        fn recover(&mut self, cfg: &Config) -> Option<Resumed<Disk>> {
            Resumed::from_journal(self.fresh(cfg))
        }
    }

    fn scratch_dir() -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("fixbolt-secrets-serve-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).expect("scratch dir");
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
                s.set_nodelay(true).expect("nodelay");
                s.set_read_timeout(Some(Duration::from_millis(100)))
                    .expect("timeout");
                return s;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("the serving loop never came up on {addr}");
    }

    /// `SendingTime` now, because the serving loop uses the real clock.
    fn now_stamp() -> Vec<u8> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("after 1970")
            .as_millis() as u64;
        let mut cache = fixbolt_codec::timestamp::TimestampCache::new();
        cache.format(now, 0)[..21].to_vec()
    }

    /// Read until `seen` holds `needle`, or five seconds pass.
    fn read_until(client: &mut TcpStream, seen: &mut Vec<u8>, needle: &[u8]) -> bool {
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut buf = [0u8; 4096];
        while Instant::now() < deadline {
            if contains(seen, needle) {
                return true;
            }
            match client.read(&mut buf) {
                Ok(0) => return contains(seen, needle),
                Ok(n) => seen.extend_from_slice(&buf[..n]),
                Err(_) => {}
            }
        }
        contains(seen, needle)
    }

    /// **The gate ADR-0110 decision 7 names.** A real `Logon` and a real
    /// `UserRequest`, both carrying secrets, through `serve_with_recovery` with
    /// a `FileLog` and a `FileJournal` on disk; no secret in either file.
    #[test]
    fn no_secret_reaches_the_log_or_the_journal_through_the_serving_loop() {
        let dir = scratch_dir();
        let log_path = dir.join("messages.log");
        let journal_path = dir.join("TW44.journal");
        let addr = free_port();

        let log = FileLog::open(&log_path).expect("a writable path");
        let (serving, jp) = (addr.clone(), journal_path.clone());
        std::thread::spawn(move || {
            let table = Table::with_capacity(1).serving(cfg());
            let _ = fixbolt_engine::serve_with_recovery(
                &serving,
                table,
                App,
                4,
                Limits::new(8, 30_000).expect("both above zero"),
                OnDisk { path: jp },
                log,
                Handles::new(),
            );
        });

        let mut client = connect(&addr);
        let mut seen = Vec::new();

        client
            .write_all(&logon(1, b"TW44", b"ISLD", &now_stamp()))
            .expect("send Logon");
        assert!(
            read_until(&mut client, &mut seen, b"\x0135=A\x01"),
            "the premise: logged on: {:?}",
            String::from_utf8_lossy(&seen)
        );

        client
            .write_all(&user_request(2, b"TW44", b"ISLD", &now_stamp()))
            .expect("send UserRequest");
        let s3_on_the_wire = [b"\x01554=".as_slice(), S3, b"\x01"].concat();
        assert!(
            read_until(&mut client, &mut seen, &s3_on_the_wire),
            "the premise: the reply carried the password on the wire: {:?}",
            String::from_utf8_lossy(&seen)
        );
        // The acceptor answered Logon at 1, so the UserRequest reply is 2.
        let seq_be: u32 = 2;

        let mut order = Vec::new();
        field(&mut order, 35, b"D");
        field(&mut order, 34, b"3");
        field(&mut order, 49, b"TW44");
        field(&mut order, 52, &now_stamp());
        field(&mut order, 56, b"ISLD");
        field(&mut order, 11, b"ORD1");
        field(&mut order, 21, b"1");
        field(&mut order, 38, b"100");
        field(&mut order, 40, b"1");
        field(&mut order, 54, b"1");
        field(&mut order, 55, b"FXB");
        field(&mut order, 60, &now_stamp());
        client.write_all(&frame(&order)).expect("send order");
        assert!(
            read_until(&mut client, &mut seen, b"\x0135=8\x01"),
            "the premise: the order was answered: {:?}",
            String::from_utf8_lossy(&seen)
        );

        // Criterion 3: inside the same process, a resend replays the secret
        // verbatim — the ring was never masked.
        let mut rr = Vec::new();
        field(&mut rr, 35, b"2");
        field(&mut rr, 34, b"4");
        field(&mut rr, 49, b"TW44");
        field(&mut rr, 52, &now_stamp());
        field(&mut rr, 56, b"ISLD");
        field(&mut rr, 7, seq_be.to_string().as_bytes());
        field(&mut rr, 16, seq_be.to_string().as_bytes());
        seen.clear();
        client.write_all(&frame(&rr)).expect("send ResendRequest");
        assert!(
            read_until(&mut client, &mut seen, b"\x0143=Y\x01")
                && read_until(&mut client, &mut seen, &s3_on_the_wire),
            "a resend inside the process replays the UserRequest with 43=Y and \
             the real password: {:?}",
            String::from_utf8_lossy(&seen)
        );

        // Bounded wait for both writers: the log until it holds the eight lines
        // this exchange produces, the journal until the reply after the
        // `UserRequest` is on disk (records are written in order).
        let deadline = Instant::now() + Duration::from_secs(5);
        let (mut log_bytes, mut journal_bytes) = (Vec::new(), Vec::new());
        while Instant::now() < deadline {
            log_bytes = std::fs::read(&log_path).unwrap_or_default();
            journal_bytes = std::fs::read(&journal_path).unwrap_or_default();
            let lines = String::from_utf8_lossy(&log_bytes)
                .lines()
                .filter(|l| !l.starts_with('#'))
                .count();
            if lines >= 8 && contains(&journal_bytes, b"\x0135=8\x01") {
                break;
            }
            std::thread::yield_now();
        }

        // The premises: both files hold what they should.
        let text = String::from_utf8_lossy(&log_bytes).into_owned();
        assert!(
            text.lines()
                .any(|l| l.contains(" IN  ") && l.contains("\u{1}35=A\u{1}")),
            "the premise: the log holds the inbound Logon: {text:?}"
        );
        assert!(
            contains(&journal_bytes, b"\x0135=8\x01"),
            "the premise: the journal file holds the ExecutionReport"
        );

        // Every leak in either file, in one sentence, so a reversal shows
        // exactly which file and which secret — and the Logon's secrets, which
        // never enter the journal by design, are checked there too.
        let mut found = leaks("message log", &log_bytes);
        found.extend(leaks("journal file", &journal_bytes));
        assert!(found.is_empty(), "{}", found.join("; "));

        // Criterion 4, read off the file: the UserRequest's number is an
        // outbound mark and never a message record.
        let r = Reader::open(&journal_path).expect("readable");
        let recs: Vec<Record<'_>> = r.records().collect();
        assert!(
            recs.contains(&Record::OutboundMark { seq: seq_be }),
            "the journal carries the outbound mark for {seq_be}: {recs:?}"
        );
        assert!(
            !recs
                .iter()
                .any(|x| matches!(x, Record::Message { seq, .. } if *seq == seq_be)),
            "and no message record for {seq_be}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
