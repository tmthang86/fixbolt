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
use fixbolt_engine::settings::{Problem, Settings};
use fixbolt_engine::transport::{Io, Loopback, Transport};
use fixbolt_session::Config;
use fixbolt_session::schedule::{Schedule, Weekday, Weekdays};

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
    Settings::load(path)
        .and_then(Settings::into_table)
        .unwrap_or_else(|e| panic!("{e}"))
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

// --- step 2: the parser, and what it refuses ---------------------------------

/// Parse and expect a failure, returning it.
fn refused(text: &str) -> fixbolt_engine::settings::SettingsError {
    match Settings::parse(text) {
        Ok(s) => panic!(
            "this should not have parsed; it produced {} configuration(s)",
            s.configs().len()
        ),
        Err(e) => e,
    }
}

/// **The most important refusal in this module.** A key this engine does not
/// know is an error, not a shrug.
///
/// QuickFIX ignores settings it does not recognise. Here the cost of that is
/// specific: a mistyped `Starttime` falls back to `Schedule::always()`, so a
/// session that should close at five stays open all night and nothing says so.
#[test]
fn a_mistyped_key_is_refused_and_not_ignored() {
    let e = refused(
        "[DEFAULT]\nBeginString=FIX.4.4\nSenderCompID=ISLD\n\
         [SESSION]\nTargetCompID=TW44\nStarttime=08:00:00\n",
    );
    assert_eq!(*e.problem(), Problem::UnknownKey);
    assert_eq!(e.line(), 6, "and it points at the line: {e}");
    assert!(
        e.to_string().contains("Starttime"),
        "the message quotes what was written: {e}"
    );
}

/// A file naming no counterparty is refused, because an empty table refuses
/// every connection — indistinguishable from a firewall dropping the port.
#[test]
fn a_file_with_no_session_block_is_refused() {
    let e = refused("[DEFAULT]\nBeginString=FIX.4.4\nSenderCompID=ISLD\n");
    assert_eq!(*e.problem(), Problem::NoSessions);
}

/// So is an empty file, for the same reason and by the same path.
#[test]
fn an_empty_file_is_refused() {
    assert_eq!(*refused("").problem(), Problem::NoSessions);
}

/// A setting above the first header has no block to belong to, and guessing
/// `[DEFAULT]` would apply it to counterparties the writer never looked at.
#[test]
fn a_setting_before_the_first_section_is_refused() {
    let e = refused("BeginString=FIX.4.4\n[SESSION]\nTargetCompID=TW44\n");
    assert_eq!(*e.problem(), Problem::KeyOutsideSection);
    assert_eq!(e.line(), 1);
}

/// The same key twice in one block has no meaning to give it, and picking one
/// silently means half the file is a comment.
#[test]
fn the_same_key_twice_in_one_block_is_refused() {
    let e = refused(
        "[DEFAULT]\nBeginString=FIX.4.4\nSenderCompID=ISLD\n\
         [SESSION]\nTargetCompID=TW44\nTargetCompID=BETA\n",
    );
    assert_eq!(*e.problem(), Problem::RepeatedKey);
    assert_eq!(e.line(), 6);
}

/// **A value too long is refused, not truncated.**
///
/// `Config` keeps names in a fixed buffer and records an over-long one as *not
/// fitting*, which matches nothing — so truncating here would produce an
/// acceptor that starts cleanly and serves nobody. One byte over the limit is
/// the case, because a test with a wildly long value would also pass against an
/// implementation that got the boundary wrong.
#[test]
fn a_comp_id_one_byte_over_the_limit_is_refused() {
    let long = "X".repeat(fixbolt_session::MAX_COMP_ID_LEN + 1);
    let e = refused(&format!(
        "[DEFAULT]\nBeginString=FIX.4.4\nSenderCompID=ISLD\n[SESSION]\nTargetCompID={long}\n"
    ));
    assert_eq!(*e.problem(), Problem::ValueTooLong);

    let exact = "X".repeat(fixbolt_session::MAX_COMP_ID_LEN);
    let ok = Settings::parse(&format!(
        "[DEFAULT]\nBeginString=FIX.4.4\nSenderCompID=ISLD\n[SESSION]\nTargetCompID={exact}\n"
    ))
    .expect("exactly the limit fits");
    assert!(
        ok.configs()[0].serves(exact.as_bytes(), US),
        "and the configuration it built really does serve that identity"
    );
}

/// A missing required key names itself, so the fix is one line rather than a
/// hunt.
#[test]
fn a_missing_required_key_names_itself() {
    let e = refused("[DEFAULT]\nBeginString=FIX.4.4\n[SESSION]\nTargetCompID=TW44\n");
    assert_eq!(*e.problem(), Problem::MissingKey);
    assert!(
        e.to_string().contains("SenderCompID"),
        "it says which one: {e}"
    );
}

/// Two blocks naming one identity is a mistake in the file, not a precedence
/// rule to invent. A table holding both would answer from the first and leave
/// the second as dead configuration nobody notices.
#[test]
fn two_sessions_naming_the_same_identity_are_refused() {
    let e = refused(
        "[DEFAULT]\nBeginString=FIX.4.4\nSenderCompID=ISLD\n\
         [SESSION]\nTargetCompID=TW44\n\
         [SESSION]\nTargetCompID=TW44\nHeartBtInt=60\n",
    );
    assert_eq!(*e.problem(), Problem::DuplicateSession);
}

/// A number that is not one is refused where it is written.
#[test]
fn a_setting_that_wants_a_number_says_so() {
    let e = refused(
        "[DEFAULT]\nBeginString=FIX.4.4\nSenderCompID=ISLD\n\
         [SESSION]\nTargetCompID=TW44\nHeartBtInt=thirty\n",
    );
    assert_eq!(*e.problem(), Problem::NotANumber);
    assert_eq!(e.line(), 6);
}

/// An unknown section is refused rather than skipped: `[SESSIONS]` with an `s`
/// would otherwise take every counterparty in the file with it.
#[test]
fn an_unknown_section_is_refused() {
    let e = refused("[SESSIONS]\nTargetCompID=TW44\n");
    assert_eq!(*e.problem(), Problem::UnknownSection);
    assert_eq!(e.line(), 1);
}

/// **A file edited on Windows parses.** `lines()` keeps the `\r`, and a CompID
/// carrying one matches nothing — a configuration that looks right in an editor
/// and serves nobody.
#[test]
fn a_file_with_crlf_line_endings_parses() {
    let text = TWO_COUNTERPARTIES.replace('\n', "\r\n");
    let s = Settings::parse(&text).expect("CRLF is a text file too");
    assert_eq!(s.configs().len(), 2);
    assert!(
        s.configs()[0].serves(CORPUS_SENDER, US),
        "and the names carry no carriage return"
    );
}

/// `[DEFAULT]` supplies, `[SESSION]` overrides, and the two are distinguishable.
///
/// One counterparty takes the default `HeartBtInt` and the other sets its own,
/// so an implementation that ignored either half fails this.
#[test]
fn a_session_overrides_the_default_block() {
    let s = Settings::parse(
        "[DEFAULT]\nBeginString=FIX.4.4\nSenderCompID=ISLD\nHeartBtInt=30\n\
         [SESSION]\nTargetCompID=TW44\n\
         [SESSION]\nTargetCompID=BETA\nHeartBtInt=60\n",
    )
    .expect("a legal file");

    // Compared as whole `Config`s rather than through a getter: it asserts
    // every field at once, including the ones this file never mentions.
    assert_eq!(s.configs().len(), 2);
    assert_eq!(
        s.configs()[0],
        Config::acceptor(b"FIX.4.4", US, CORPUS_SENDER).with_heart_bt_int(30),
        "TW44 inherits the default block's HeartBtInt"
    );
    assert_eq!(
        s.configs()[1],
        Config::acceptor(b"FIX.4.4", US, OTHER_SENDER).with_heart_bt_int(60),
        "BETA overrides it"
    );
    assert_ne!(
        s.configs()[0],
        s.configs()[1],
        "and the two are distinguishable, which is what says either assertion \
         above could have failed"
    );
}

/// Comments and blank lines are not settings.
#[test]
fn comments_and_blank_lines_are_skipped() {
    let s = Settings::parse(
        "# our UAT acceptor\n\n[DEFAULT]\nBeginString=FIX.4.4\n; theirs\n\
         SenderCompID=ISLD\n\n[SESSION]\nTargetCompID=TW44\n",
    )
    .expect("a legal file");
    assert_eq!(s.configs().len(), 1);
}

/// A file that cannot be read says so, with the path in the message.
#[test]
fn a_missing_file_says_which_one() {
    let e = Settings::load(tmp("never-written")).expect_err("no such file");
    assert_eq!(*e.problem(), Problem::Unreadable);
    assert_eq!(e.line(), 0, "a problem about the file, not about a line");
    assert!(
        e.to_string().contains("never-written"),
        "and it names the path: {e}"
    );
}

// --- step 3: hours out of the file -------------------------------------------

/// **The specification for step 3.** A counterparty's trading hours come out of
/// the file, and the session layer gets the schedule it would have been given
/// by hand.
///
/// Compared against a `Schedule` built directly rather than probed through
/// `contains`: the arithmetic is ADR-0033's and already has its own tests, and
/// what this file is responsible for is passing the right two numbers into it.
#[test]
fn trading_hours_come_out_of_the_file() {
    let s = Settings::parse(
        "[DEFAULT]\nBeginString=FIX.4.4\nSenderCompID=ISLD\n\
         [SESSION]\nTargetCompID=TW44\nStartTime=08:00:00\nEndTime=17:00:00\n",
    )
    .expect("a legal file");

    let want = Config::acceptor(b"FIX.4.4", US, CORPUS_SENDER)
        .with_schedule(Schedule::daily(8 * 3_600, 17 * 3_600).expect("legal hours"));
    assert_eq!(s.configs()[0], want);
    assert_ne!(
        s.configs()[0],
        Config::acceptor(b"FIX.4.4", US, CORPUS_SENDER),
        "and it is not simply the default schedule, which is what an ignored \
         StartTime would leave behind"
    );
}

/// A session that opens in the evening and closes in the morning is **one**
/// session, and the file must carry that rather than flattening it.
#[test]
fn a_session_that_crosses_midnight_survives_the_file() {
    let s = Settings::parse(
        "[DEFAULT]\nBeginString=FIX.4.4\nSenderCompID=ISLD\n\
         [SESSION]\nTargetCompID=TW44\nStartTime=22:00:00\nEndTime=06:00:00\n",
    )
    .expect("a legal file");
    assert_eq!(
        s.configs()[0],
        Config::acceptor(b"FIX.4.4", US, CORPUS_SENDER)
            .with_schedule(Schedule::daily(22 * 3_600, 6 * 3_600).expect("legal"))
    );
}

/// A week-long session — Sunday evening to Friday evening — is one interval,
/// and nothing resets on Tuesday night.
#[test]
fn a_weekly_window_comes_out_of_the_file() {
    let s = Settings::parse(
        "[DEFAULT]\nBeginString=FIX.4.4\nSenderCompID=ISLD\n\
         [SESSION]\nTargetCompID=TW44\n\
         StartDay=Sunday\nStartTime=21:00:00\nEndDay=Fri\nEndTime=21:00:00\n",
    )
    .expect("a legal file");
    assert_eq!(
        s.configs()[0],
        Config::acceptor(b"FIX.4.4", US, CORPUS_SENDER).with_schedule(
            Schedule::weekly(Weekday::Sunday, 21 * 3_600, Weekday::Friday, 21 * 3_600)
                .expect("legal")
        ),
        "full names and three-letter ones name the same day"
    );
}

/// Weekdays narrow a daily window.
#[test]
fn weekdays_narrow_a_daily_window() {
    let s = Settings::parse(
        "[DEFAULT]\nBeginString=FIX.4.4\nSenderCompID=ISLD\n\
         [SESSION]\nTargetCompID=TW44\nStartTime=08:00:00\nEndTime=17:00:00\n\
         Weekdays=Mon,Tue,Wed,Thu,Fri\n",
    )
    .expect("a legal file");

    let days = Weekdays::NONE
        .and(Weekday::Monday)
        .and(Weekday::Tuesday)
        .and(Weekday::Wednesday)
        .and(Weekday::Thursday)
        .and(Weekday::Friday);
    assert_eq!(
        s.configs()[0],
        Config::acceptor(b"FIX.4.4", US, CORPUS_SENDER).with_schedule(
            Schedule::daily(8 * 3_600, 17 * 3_600)
                .and_then(|s| s.with_weekdays(days))
                .expect("legal")
        )
    );
}

/// **A half-written schedule is refused, not completed.** `StartTime` with no
/// `EndTime` means the writer meant something and the parser does not know
/// what; filling in midnight would be a guess that reads as a decision.
#[test]
fn half_a_schedule_is_refused() {
    for (text, missing) in [
        ("StartTime=08:00:00\n", "EndTime"),
        ("EndTime=17:00:00\n", "StartTime"),
    ] {
        let e = refused(&format!(
            "[DEFAULT]\nBeginString=FIX.4.4\nSenderCompID=ISLD\n\
             [SESSION]\nTargetCompID=TW44\n{text}"
        ));
        assert_eq!(*e.problem(), Problem::MissingKey, "{text}");
        assert!(e.to_string().contains(missing), "it names it: {e}");
    }
}

/// A day with no hours describes nothing, and ignoring it is the failure
/// `UnknownKey` exists to prevent — the key is spelled correctly and still has
/// no effect.
#[test]
fn a_day_without_hours_is_refused() {
    let e = refused(
        "[DEFAULT]\nBeginString=FIX.4.4\nSenderCompID=ISLD\n\
         [SESSION]\nTargetCompID=TW44\nStartDay=Monday\n",
    );
    assert_eq!(*e.problem(), Problem::MissingKey);
    assert_eq!(e.line(), 6, "it points at the day, not at the block: {e}");
}

/// A time that is nearly right is refused rather than guessed at.
#[test]
fn a_time_that_is_not_hh_mm_ss_is_refused() {
    for bad in [
        "8:00:00",
        "08:00",
        "08:00:00.000",
        "24:00:00",
        "08:60:00",
        "morning",
    ] {
        let e = refused(&format!(
            "[DEFAULT]\nBeginString=FIX.4.4\nSenderCompID=ISLD\n\
             [SESSION]\nTargetCompID=TW44\nStartTime={bad}\nEndTime=17:00:00\n"
        ));
        assert_eq!(*e.problem(), Problem::BadTime, "StartTime={bad}");
        assert_eq!(e.line(), 6, "StartTime={bad}: {e}");
    }
}

/// A window of zero length is open never, which is not what anyone means by
/// writing the same time twice.
#[test]
fn a_zero_length_window_is_refused() {
    let e = refused(
        "[DEFAULT]\nBeginString=FIX.4.4\nSenderCompID=ISLD\n\
         [SESSION]\nTargetCompID=TW44\nStartTime=08:00:00\nEndTime=08:00:00\n",
    );
    assert_eq!(*e.problem(), Problem::ImpossibleSchedule);
}

/// A day name this parser does not know says so, and quotes what was written.
#[test]
fn an_unknown_day_name_is_refused() {
    let e = refused(
        "[DEFAULT]\nBeginString=FIX.4.4\nSenderCompID=ISLD\n\
         [SESSION]\nTargetCompID=TW44\n\
         StartDay=Sunnday\nStartTime=21:00:00\nEndDay=Friday\nEndTime=21:00:00\n",
    );
    assert_eq!(*e.problem(), Problem::BadWeekday);
    assert!(e.to_string().contains("Sunnday"), "{e}");
}

/// `Weekdays` on a weekly window is refused rather than silently dropped: the
/// weekly period already chooses its days, and the session layer answers `None`
/// to the combination.
#[test]
fn weekdays_on_a_weekly_window_is_refused() {
    let e = refused(
        "[DEFAULT]\nBeginString=FIX.4.4\nSenderCompID=ISLD\n\
         [SESSION]\nTargetCompID=TW44\n\
         StartDay=Sunday\nStartTime=21:00:00\nEndDay=Friday\nEndTime=21:00:00\n\
         Weekdays=Mon,Tue\n",
    );
    assert_eq!(*e.problem(), Problem::ImpossibleSchedule);
}

/// The default block can carry the hours, and each `[SESSION]` inherits them —
/// which is the ordinary case: one venue, one calendar, many counterparties.
#[test]
fn hours_in_the_default_block_reach_every_session() {
    let s = Settings::parse(
        "[DEFAULT]\nBeginString=FIX.4.4\nSenderCompID=ISLD\n\
         StartTime=08:00:00\nEndTime=17:00:00\n\
         [SESSION]\nTargetCompID=TW44\n\
         [SESSION]\nTargetCompID=BETA\nStartTime=00:00:00\nEndTime=23:59:59\n",
    )
    .expect("a legal file");

    let venue = Schedule::daily(8 * 3_600, 17 * 3_600).expect("legal");
    assert_eq!(
        s.configs()[0],
        Config::acceptor(b"FIX.4.4", US, CORPUS_SENDER).with_schedule(venue),
        "TW44 inherits the venue calendar"
    );
    assert_ne!(
        s.configs()[1],
        Config::acceptor(b"FIX.4.4", US, OTHER_SENDER).with_schedule(venue),
        "BETA does not — it set its own hours"
    );
}

/// A file that says nothing about hours leaves `Schedule::always`, which is the
/// default the 59 acceptance definitions run under.
#[test]
fn a_file_with_no_hours_leaves_the_neutral_schedule() {
    let s = Settings::parse(TWO_COUNTERPARTIES).expect("a legal file");
    assert_eq!(
        s.configs()[0],
        Config::acceptor(b"FIX.4.4", US, CORPUS_SENDER),
        "exactly what Config::acceptor builds, schedule included"
    );
}

// ---------------------------------------------------------------------------
// `FileLogPath` — the key that turns the message log on.
//
// It is `[DEFAULT]`-only on purpose: an engine writes **one** log, and two
// `[SESSION]` blocks asking for two files is a configuration that cannot be
// honoured. Refusing beats resolving it by picking one, because the operator
// would never learn which.
// ---------------------------------------------------------------------------

/// The key is read, and a misspelling next to it is still an error.
///
/// Both halves matter. ADR-0040's whole rule is that an unknown key is a
/// **failure with a line number**, and a new key added carelessly is exactly
/// how a parser starts shrugging at the ones it does not know.
#[test]
fn file_log_path_is_read_and_an_unknown_key_beside_it_is_still_an_error() {
    let good = "\
[DEFAULT]
BeginString=FIX.4.4
SenderCompID=ISLD
FileLogPath=/tmp/fixbolt-messages.log

[SESSION]
TargetCompID=TW44
";
    let s = Settings::parse(good).expect("a legal file");
    assert_eq!(
        s.log().map(std::path::Path::to_path_buf),
        Some(std::path::PathBuf::from("/tmp/fixbolt-messages.log")),
    );
    assert_eq!(s.configs().len(), 1, "the session is still built");

    let typo = good.replace("FileLogPath=", "FileLogPth=");
    let e = Settings::parse(&typo).expect_err("a misspelled key is a failure");
    assert_eq!(e.problem(), &Problem::UnknownKey, "left: {e}");
    assert_eq!(e.line(), 4, "and it names the line: {e}");
}

/// A file that names no log asks for none. `None` is not a default path.
#[test]
fn no_file_log_path_means_no_log_rather_than_a_guessed_one() {
    let s = Settings::parse(
        "\
[DEFAULT]
BeginString=FIX.4.4
SenderCompID=ISLD

[SESSION]
TargetCompID=TW44
",
    )
    .expect("a legal file");
    assert!(s.log().is_none());
}

/// One engine, one log. A `[SESSION]` asking for its own is refused, by name.
#[test]
fn file_log_path_in_a_session_block_is_refused_rather_than_quietly_ignored() {
    let e = Settings::parse(
        "\
[DEFAULT]
BeginString=FIX.4.4
SenderCompID=ISLD

[SESSION]
TargetCompID=TW44
FileLogPath=/tmp/one-counterparty.log
",
    )
    .expect_err("a per-session log cannot be honoured");
    assert_eq!(e.problem(), &Problem::DefaultOnly, "left: {e}");
    assert_eq!(e.line(), 7, "and it names the line: {e}");
}

/// The same key twice is meaningless, and it is meaningless here too.
#[test]
fn file_log_path_twice_is_the_same_error_as_any_other_repeat() {
    let e = Settings::parse(
        "\
[DEFAULT]
BeginString=FIX.4.4
SenderCompID=ISLD
FileLogPath=/tmp/a.log
FileLogPath=/tmp/b.log

[SESSION]
TargetCompID=TW44
",
    )
    .expect_err("two paths name no file");
    assert_eq!(e.problem(), &Problem::RepeatedKey, "left: {e}");
}
