//! The binary, run as a binary.
//!
//! Step 3 of [what-the-journal-can-answer]. The library reader has its own
//! tests in `crates/engine/tests/journal_reader.rs`; **this file exists because
//! a library function is not what open item 30 (e) asked for.** The complaint
//! was that nothing outside the process could read the file, and only running
//! the executable answers it.
//!
//! [what-the-journal-can-answer]: ../../../docs/plans/2026-09-02-what-the-journal-can-answer.md
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;
use std::process::Command;

use fixbolt_engine::journal::{Durability, FileJournal};
use fixbolt_session::journal::Journal;

fn tmp(name: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!("fixbolt-jrnl-cli-{name}-{}", std::process::id()));
    let _ = std::fs::remove_file(&p);
    p
}

fn write_history(path: &std::path::Path, count: u32) {
    let mut j: FileJournal<8, 512> = FileJournal::open(path, Durability::Async).expect("open");
    for seq in 1..=count {
        j.put(
            seq,
            format!("8=FIX.4.4\u{1}35=D\u{1}34={seq}\u{1}11=ORDER-{seq}\u{1}").as_bytes(),
        );
    }
    j.mark_in(count + 1);
    drop(j);
}

/// `(stdout, stderr, exit code)`.
fn run(args: &[&str]) -> (String, String, i32) {
    let out = Command::new(env!("CARGO_BIN_EXE_jrnl"))
        .args(args)
        .output()
        .expect("run jrnl");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.code().unwrap_or(-1),
    )
}

/// **The specification, as an operator would meet it.** A message far outside
/// the ring is found by running a program against the file.
#[test]
fn the_tool_finds_a_message_the_ring_never_held() {
    let path = tmp("find");
    write_history(&path, 2_000);
    let (stdout, stderr, code) = run(&[&path.to_string_lossy(), "--seq", "3"]);
    let _ = std::fs::remove_file(&path);

    assert_eq!(code, 0, "stderr: {stderr}");
    assert!(
        stdout.contains("11=ORDER-3|"),
        "the desk asked about order 3: {stdout}"
    );
    assert_eq!(stdout.lines().count(), 1, "and only that one: {stdout}");
}

/// `--count` answers *how much is here* without printing 2 000 lines, and the
/// numbers are the real ones.
#[test]
fn the_tool_counts_what_the_file_holds() {
    let path = tmp("count");
    write_history(&path, 2_000);
    let (stdout, _, code) = run(&[&path.to_string_lossy(), "--count"]);
    let _ = std::fs::remove_file(&path);

    assert_eq!(code, 0);
    assert!(stdout.contains("messages 2000"), "{stdout}");
    assert!(stdout.contains("inbound-marks 1"), "{stdout}");
    assert!(stdout.contains("seq 1..2000"), "{stdout}");
}

/// **A torn tail is loud.** An answer of *"no, we never received it"* given
/// from a damaged file might be wrong, so the warning goes to stderr **and**
/// the exit code changes — a script that only checks the status must not read
/// success.
#[test]
fn a_torn_file_warns_and_changes_the_exit_code() {
    let path = tmp("torn");
    write_history(&path, 100);

    let (_, clean_err, clean_code) = run(&[&path.to_string_lossy(), "--count"]);
    assert_eq!(clean_code, 0, "the premise: a whole file is quiet");
    assert!(clean_err.is_empty(), "and says nothing: {clean_err}");

    let whole = std::fs::metadata(&path).expect("stat").len();
    let f = std::fs::OpenOptions::new()
        .write(true)
        .open(&path)
        .expect("open");
    f.set_len(whole - 7).expect("truncate");
    drop(f);

    let (stdout, stderr, code) = run(&[&path.to_string_lossy(), "--count"]);
    let _ = std::fs::remove_file(&path);

    assert_eq!(code, 2, "a script checking only the status must not see 0");
    assert!(stderr.contains("killed mid-write"), "{stderr}");
    assert!(
        stdout.contains("messages"),
        "and what could be read is still shown: {stdout}"
    );
}

/// A file that is not there is an error with the path in it, not a panic and
/// not an empty success.
#[test]
fn a_missing_file_is_an_error_that_names_itself() {
    let (stdout, stderr, code) = run(&["/nonexistent/fixbolt/journal.bin"]);
    assert_eq!(code, 1);
    assert!(
        stderr.contains("/nonexistent/fixbolt/journal.bin"),
        "{stderr}"
    );
    assert!(stdout.is_empty());
}

/// No arguments prints usage and fails, rather than reading something by
/// accident.
#[test]
fn no_arguments_prints_usage() {
    let (_, stderr, code) = run(&[]);
    assert_eq!(code, 1);
    assert!(stderr.contains("usage: jrnl"), "{stderr}");
}
