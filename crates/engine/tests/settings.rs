//! Who this acceptor serves comes out of a file, not out of a rebuild.
//!
//! **Step 1 of [a-registry-from-a-file], written to be red.** `[verified
//! 2026-09-02]` the only way to put a counterparty into a `presession::Table`
//! is `Table::serving(cfg)` — Rust, compiled in. `docs/PRD.md` names this as
//! the last open line under `many counterparties`.
//!
//! # Why that is a defect and not an inconvenience
//!
//! Adding a counterparty to a running acceptor is an operator's job, usually
//! the evening before that counterparty reaches UAT. Behind a recompilation it
//! needs a Rust toolchain and the source, it makes changing a `HeartBtInt` the
//! same class of release as changing the hot path, and it leaves no way to diff
//! two environments' configuration except by reading two programs.
//!
//! # Why this is red at an assertion and not at the compiler
//!
//! [`table_from_file`] is the seam. It is called with a real file that names
//! two counterparties, and today it can only answer with what the crate can
//! build from that file — **nothing** — so it returns `Table::new()`. That is
//! not a stand-in for a missing function; it is the true answer today, and
//! ADR-0026 decision 6 makes it a precise one: an empty table refuses every
//! connection there will ever be.
//!
//! `tests/registry.rs` used exactly this shape for the same reason, and its
//! rule holds here: **step 2 changes [`table_from_file`] and nothing else.**
//! A `#[test]` edited to go green was never measuring what it claimed
//! (`CLAUDE.md` §10).
//!
//! [a-registry-from-a-file]: ../../../docs/plans/2026-09-02-a-registry-from-a-file.md
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};

use fixbolt_conformance::script::{FIXED_TIME_MILLIS, Kind, load_all};
use fixbolt_engine::presession::{Limits, PendingSet, Registry, Table, is_logon};
use fixbolt_engine::transport::{Io, Loopback, Transport};

const PRE: usize = 1024;
const T0: u64 = FIXED_TIME_MILLIS;
const US: &[u8] = b"ISLD";
const CORPUS_SENDER: &[u8] = b"TW44";
const OTHER_SENDER: &[u8] = b"BETA";

/// The configuration this whole plan is about: two counterparties, one
/// acceptor, and no Rust.
///
/// The shape is QuickFIX's, because every FIX operator alive can already read
/// it: a `[DEFAULT]` block whose values apply to each `[SESSION]` after it.
const TWO_COUNTERPARTIES: &str = "\
[DEFAULT]
BeginString=FIX.4.4
SenderCompID=ISLD

[SESSION]
TargetCompID=TW44

[SESSION]
TargetCompID=BETA
";

/// A scratch path that no other test and no other run can collide with.
///
/// Same discipline as `tests/on_disk.rs`: pid and thread id, because `cargo
/// test` runs these in parallel and a shared name makes one test's file another
/// test's input.
fn tmp(name: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!(
        "fixbolt-settings-{name}-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_file(&p);
    p
}

fn write(name: &str, text: &str) -> PathBuf {
    let p = tmp(name);
    std::fs::write(&p, text).expect("the scratch directory is writable");
    p
}

/// **The seam, and the only thing step 2 may change.**
///
/// `[verified 2026-09-02]` there is no public function anywhere in this
/// workspace that turns configuration text into a `Table`, so what a deployment
/// can build from this file is an empty table — every connection refused,
/// ADR-0026 decision 6.
fn table_from_file(path: &Path) -> Table {
    let _ = std::fs::read_to_string(path).expect("the file is there");
    Table::new()
}

/// A correct FIX.4.4 `Logon` from `TW44`, taken out of the acceptance corpus.
///
/// Real bytes rather than invented ones (`CLAUDE.md` §7), and the *correct*
/// one of the twelve: most `Logon` lines in the corpus are deliberately
/// malformed, and a refusal here must be about identity and nothing else.
fn corpus_logon() -> Vec<u8> {
    load_all()
        .expect("the corpus is fetched — scripts/fetch-quickfix-assets.sh")
        .into_iter()
        .find_map(|s| match s.kind {
            Kind::Send(m)
                if is_logon(&m.wire)
                    && contains_field(&m.wire, b"34=1")
                    && contains_field(&m.wire, b"8=FIX.4.4")
                    && contains_field(&m.wire, b"49=TW44")
                    && contains_field(&m.wire, b"56=ISLD")
                    && contains_field(&m.wire, b"108=30") =>
            {
                Some(m.wire)
            }
            _ => None,
        })
        .expect("the corpus sends a well-formed FIX.4.4 Logon from TW44")
}

/// Is `field` one whole SOH-delimited field of `msg`?
///
/// Fields, not `windows`: searching for the bytes anywhere would match inside a
/// value the counterparty controls.
fn contains_field(msg: &[u8], field: &[u8]) -> bool {
    msg.split(|b| *b == 1).any(|f| f == field)
}

/// The same `Logon` with `49=` rewritten, `9=` and `10=` recomputed.
///
/// The corpus has no file in which two counterparties talk to one acceptor, so
/// the second identity has to come from somewhere; changing exactly one field
/// of real bytes is the smallest step away from them. Held by
/// [`relabelling_to_the_same_sender_reproduces_the_corpus_bytes`].
fn relabel(wire: &[u8], sender: &[u8]) -> Vec<u8> {
    let mut head = Vec::new();
    let mut body = Vec::new();
    for field in wire.split(|b| *b == 1).filter(|f| !f.is_empty()) {
        if field.starts_with(b"9=") || field.starts_with(b"10=") {
            continue;
        }
        let out = if field.starts_with(b"8=") {
            &mut head
        } else {
            &mut body
        };
        if field.starts_with(b"49=") {
            out.extend_from_slice(b"49=");
            out.extend_from_slice(sender);
        } else {
            out.extend_from_slice(field);
        }
        out.push(1);
    }
    let mut msg = head;
    msg.extend_from_slice(b"9=");
    msg.extend_from_slice(body.len().to_string().as_bytes());
    msg.push(1);
    msg.extend_from_slice(&body);
    let sum = fixbolt_codec::checksum(&msg);
    msg.extend_from_slice(b"10=");
    msg.extend_from_slice(&fixbolt_codec::format_checksum(sum));
    msg.push(1);
    msg
}

/// One pre-session set holding one socket that has already sent `first`.
fn one_socket<R: Registry>(registry: R, first: &[u8]) -> PendingSet<Loopback, R, PRE> {
    let mut set = PendingSet::new(Limits::new(4, 30_000).expect("both above zero"), registry);
    let (near, mut far) = Loopback::pair();
    assert!(set.admit(near, T0).is_ok(), "the ceiling is four");
    assert!(
        matches!(far.send(first), Io::Ready(_)),
        "the message is on the wire"
    );
    // `far` is dropped here on purpose: the pre-session stage reads what is
    // already buffered, and nothing in this file needs a reply.
    set
}

/// **The specification.** Two counterparties named only in a file, and an
/// acceptor that serves both of them.
#[test]
fn two_counterparties_named_only_in_a_file_are_both_served() {
    let path = write("two", TWO_COUNTERPARTIES);
    let table = table_from_file(&path);
    let _ = std::fs::remove_file(&path);

    assert_eq!(
        table.len(),
        2,
        "the file names two counterparties and the acceptor must serve exactly \
         those two — adding one is an operator's edit, not a release"
    );

    let logon = corpus_logon();
    for who in [CORPUS_SENDER, OTHER_SENDER] {
        let mut set = one_socket(&table, &relabel(&logon, who));
        let p = set.turn(T0);
        assert_eq!(
            p.settled,
            1,
            "{} is named in the file and must reach a session: {p:?}",
            String::from_utf8_lossy(who)
        );
    }
}

/// The control, and the reason the assertion above can be trusted: an identity
/// the file does **not** name is still refused.
///
/// Without it, a loader that served everybody would pass the specification.
///
/// **It is green at step 1, and for the wrong reason** — an empty table refuses
/// everybody, `NOBODY` included. That is worth saying out loud: it carries no
/// weight until [`table_from_file`] actually reads the file, and it is here now
/// so that step 2 cannot quietly acquire a wildcard.
#[test]
fn an_identity_the_file_does_not_name_is_still_refused() {
    let path = write("unnamed", TWO_COUNTERPARTIES);
    let table = table_from_file(&path);
    let _ = std::fs::remove_file(&path);

    let logon = corpus_logon();
    let mut set = one_socket(&table, &relabel(&logon, b"NOBODY"));
    let p = set.turn(T0);

    assert_eq!(p.settled, 0, "NOBODY is not in the file: {p:?}");
    assert_eq!(p.unknown, 1, "and the refusal is counted: {p:?}");
}

/// The premise of [`relabel`]: rewriting `TW44` to `TW44` must give back the
/// corpus bytes exactly, `BodyLength` and `CheckSum` included.
///
/// Without this the two tests above could be red because the message-building
/// machinery is broken rather than because the registry is empty — a red that
/// names the wrong cause.
#[test]
fn relabelling_to_the_same_sender_reproduces_the_corpus_bytes() {
    let logon = corpus_logon();
    assert_eq!(relabel(&logon, CORPUS_SENDER), logon);
}

/// The other premise: this acceptor is `ISLD`, which is what the file's
/// `[DEFAULT]` block says and what the corpus `Logon` addresses.
///
/// It is asserted rather than assumed because a `SenderCompID` mismatch and an
/// unknown counterparty are refused identically.
#[test]
fn the_file_and_the_corpus_agree_on_who_this_acceptor_is() {
    assert!(
        TWO_COUNTERPARTIES.contains(&format!("SenderCompID={}", String::from_utf8_lossy(US))),
        "the file configures this acceptor as ISLD"
    );
    assert!(
        contains_field(&corpus_logon(), b"56=ISLD"),
        "and the corpus Logon is addressed to ISLD"
    );
}
