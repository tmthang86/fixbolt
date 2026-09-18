//! What the session's own `Heartbeat` reply costs, timed on its own.
//!
//! `STATUS.md` open item 49, `DESIGN.md` §8 *The 3 898 ns, added back*, the row
//! "the session's own `Heartbeat` serialise, which the application path does
//! *not* do" — until this file existed that row carried `−?` and was never
//! subtracted. `crates/codec/benches/serialize.rs`'s `encode ExecutionReport
//! (template)` is the pattern this file follows: a per-message serialise cost,
//! timed in isolation.
//!
//! # What is inside the timed region
//!
//! [`Session::received_with`], fed the exact bytes `tools/w2w --path admin`
//! sends (`test_request` in `tools/w2w/src/main.rs`, copied field for field),
//! against a session already logged on with that same counterparty's identity.
//! This is the same public entry point `crates/engine/src/conn.rs` calls on
//! every inbound message (`session.received_with(rx.bytes(taken), app,
//! journal, |b| ...)`), so the timed call is the same path `w2w`'s admin round
//! trip exercises for the reply half, not a reimplementation of it.
//!
//! # Why `set_next_in` / `set_next_out` are inside the timed closure
//!
//! A `TestRequest` answered by [`Session::received_with`] advances `next_in`,
//! so the same wire bytes cannot be replayed unmodified — a second call would
//! read as a sequence number that has already been consumed and the session
//! would stop replying with a plain `Heartbeat`. There is no public way to
//! snapshot and restore a whole [`Session`] (it is not `Clone`; `FieldIndex`
//! alone is 3 KiB of offsets nobody asked to copy per iteration), so the two
//! numbers this case depends on are put back by hand, through the same public
//! setters an operator uses to correct a session's counters
//! ([`Session::set_next_in`], [`Session::set_next_out`]). Both are `const fn`
//! field stores — no allocation, no branch on message content — so they add a
//! small, fixed amount to every timed call rather than change what is being
//! measured; `crates/session/tests/heartbeat.rs` and
//! `crates/session/tests/logon.rs` are what actually prove those setters'
//! semantics, not this file.
//!
//! # Fault-free, on purpose
//!
//! The bytes are asserted clean and the reply asserted to be exactly the
//! `35=0` carrying `112=W1` before the loop starts, for the same reason
//! `validate.rs` asserts its two messages clean: a benchmark that times a
//! reject measures a different, shorter path and says nothing about the one
//! item 49 asks for.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
#[path = "../../codec/benches/harness.rs"]
mod harness;

use fixbolt_session::journal::NoJournal;
use fixbolt_session::{Acceptor, Config, Link, Session, Silent};
use std::hint::black_box;

/// `8=`/`9=`/`10=` framed around `body`, checksum computed the same way FIX
/// requires it: the sum of every byte up to (not including) the `10=` field,
/// mod 256. No allocation left in the timed region — this runs only in setup.
fn frame(begin_string: &[u8], body: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    out.push(b'8');
    out.push(b'=');
    out.extend_from_slice(begin_string);
    out.push(0x01);
    out.extend_from_slice(format!("9={}\x01", body.len()).as_bytes());
    out.extend_from_slice(body);
    let sum: u8 = out.iter().fold(0u8, |a, &b| a.wrapping_add(b));
    out.extend_from_slice(format!("10={sum:03}\x01").as_bytes());
    out
}

fn main() {
    harness::suite(|b| {
        // `Config::acceptor(b"FIX.4.4", b"ISLD", b"W2W")` is `tools/w2w`'s own
        // acceptor config (`tools/w2w/src/main.rs:2613`), so the Logon and
        // TestRequest bytes below, both stamped `49=W2W`/`56=ISLD`, are this
        // session's own counterparty rather than an invented one.
        let cfg = Config::acceptor(b"FIX.4.4", b"ISLD", b"W2W");
        let mut session: Session<Acceptor, 256> = Session::new(cfg);
        session.connect(|_| {});
        // `Session::tick` counts milliseconds since 0000-01-01, not the Unix
        // epoch (see its rustdoc). 2026-09-05T12:00:00Z is Unix ms
        // 1_788_609_600_000; `+ 719_528 * 86_400_000` is the same proleptic-
        // Gregorian offset `crates/conformance/src/script.rs`'s
        // `FIXED_TIME_MILLIS` uses. Matches the `52=` stamp below exactly, so
        // the skew check that guards a real `SendingTime` has nothing to
        // reject.
        const NOW_MS: u64 = 1_788_609_600_000 + 719_528 * 86_400_000;
        session.tick(NOW_MS, |_| {});

        // `tools/w2w`'s own Logon shape, `tools/w2w/src/main.rs:4223`, `34=1`.
        let logon = frame(
            b"FIX.4.4",
            b"35=A\x0134=1\x0149=W2W\x0152=20260905-12:00:00.000\x0156=ISLD\x0198=0\x01108=30\x01",
        );
        let link = session.received(&logon, |_| {});
        assert_eq!(
            link,
            Link::Up,
            "the Logon should have been accepted: {:?}",
            session.last_drop_reason()
        );
        assert!(session.is_logged_on(), "session did not reach LoggedOn");

        // `tools/w2w`'s own admin-path TestRequest, `tools/w2w/src/main.rs:4228`
        // (`test_request(2, 1)`), copied field for field: `35=1`, `34=2`,
        // `112=W1`. This is the exact bytes `--path admin` sends.
        let test_request = frame(
            b"FIX.4.4",
            b"35=1\x0134=2\x0149=W2W\x0152=20260905-12:00:00.000\x0156=ISLD\x01112=W1\x01",
        );

        // Proven once, outside the loop: the reply is the plain `35=0`
        // carrying the same `112=W1` back, `4b_ReceivedTestRequest.def`'s rule
        // (`crates/session/src/lib.rs`, the `is_test_request` arm).
        session.set_next_in(2);
        session.set_next_out(2);
        let mut reply = Vec::new();
        let link = session.received_with(&test_request, &mut Silent, &mut NoJournal, |bytes| {
            reply.extend_from_slice(bytes);
        });
        assert_eq!(link, Link::Up, "a TestRequest is not an error");
        let text = String::from_utf8(reply).expect("ascii");
        assert!(text.contains("\x0135=0\x01"), "not a Heartbeat: {text:?}");
        assert!(text.contains("\x01112=W1\x01"), "112 not echoed: {text:?}");

        b.bench("serialize Heartbeat (session)", || {
            // See the module doc: the two setters below are the stand-in for
            // a per-iteration session reset, not part of the cost item 49
            // asks about.
            session.set_next_in(black_box(2));
            session.set_next_out(black_box(2));
            let link = session.received_with(
                black_box(&test_request),
                &mut Silent,
                &mut NoJournal,
                |bytes| {
                    black_box(bytes);
                },
            );
            black_box(link);
        });
    });
}
