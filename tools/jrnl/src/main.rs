//! Read a fixbolt journal file from outside the process that wrote it.
//!
//! **The question this exists for:** *"we sent order X at 10:32, did you
//! receive it?"* Before this, the only thing that could read the file was a
//! Rust process that knew the right `N` and `LEN` — and the ring those
//! parameters describe holds only the recent end anyway.
//! `STATUS.md` item 30 (e).
//!
//! # What it deliberately does not do
//!
//! It does not interpret FIX. Records come out as bytes with `SOH` shown as
//! `|`, and `grep` does the rest. Interpreting them needs a dictionary, and a
//! program that reads a file has no business pulling one in.
//!
//! # Usage
//!
//! ```text
//! jrnl <file>              every record
//! jrnl <file> --seq 4812   only that sequence number
//! jrnl <file> --count      how many, and the range, without the bytes
//! ```
#![forbid(unsafe_code)]

use std::path::PathBuf;
use std::process::ExitCode;

use fixbolt_engine::journal::{Reader, Record};

/// What the arguments asked for.
enum What {
    Everything,
    One(u32),
    Count,
}

fn usage() -> &'static str {
    "usage: jrnl <file> [--seq N | --count]"
}

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let Some(path) = args.next() else {
        eprintln!("{}", usage());
        return ExitCode::FAILURE;
    };
    let what = match args.next().as_deref() {
        None => What::Everything,
        Some("--count") => What::Count,
        Some("--seq") => match args.next().as_deref().map(str::parse::<u32>) {
            Some(Ok(n)) => What::One(n),
            _ => {
                eprintln!("--seq needs a number\n{}", usage());
                return ExitCode::FAILURE;
            }
        },
        Some(other) => {
            eprintln!("unknown option {other}\n{}", usage());
            return ExitCode::FAILURE;
        }
    };

    let path = PathBuf::from(path);
    let reader = match Reader::open(&path) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("{}: {e}", path.display());
            return ExitCode::FAILURE;
        }
    };

    match what {
        What::Everything => {
            for r in reader.records() {
                println!("{}", line(&r));
            }
        }
        What::One(n) => {
            for r in reader.records().filter(|r| r.seq() == n) {
                println!("{}", line(&r));
            }
        }
        What::Count => {
            let (mut messages, mut marks, mut activity) = (0usize, 0usize, 0usize);
            let mut out_marks = 0usize;
            let mut last_alive: Option<u64> = None;
            let (mut lowest, mut highest) = (None::<u32>, None::<u32>);
            let mut spent = None::<u32>;
            for r in reader.records() {
                match r {
                    Record::Message { seq, .. } => {
                        messages += 1;
                        lowest = Some(lowest.map_or(seq, |l| l.min(seq)));
                        highest = Some(highest.map_or(seq, |h| h.max(seq)));
                    }
                    Record::InboundMark { .. } => marks += 1,
                    Record::ActivityMark { at_ms } => {
                        activity += 1;
                        last_alive = Some(at_ms);
                    }
                    // **Not counted as a message, and it does not move
                    // `highest`.** That column answers *what can be replayed*,
                    // and an outbound mark is the other question — ADR-0053.
                    Record::OutboundMark { seq } => {
                        out_marks += 1;
                        spent = Some(spent.map_or(seq, |s: u32| s.max(seq)));
                    }
                }
            }
            println!(
                "messages {messages}  inbound-marks {marks}  activity-marks {activity}  \
                 outbound-marks {out_marks}  seq {}..{}  spent {}  last-alive {}  bytes {}",
                lowest.map_or_else(|| "-".to_owned(), |v| v.to_string()),
                highest.map_or_else(|| "-".to_owned(), |v| v.to_string()),
                spent.map_or_else(|| "-".to_owned(), |v| v.to_string()),
                last_alive.map_or_else(|| "-".to_owned(), |v| v.to_string()),
                reader.len(),
            );
        }
    }

    // **Always, on stderr, whatever was asked for.** A torn tail means a
    // process was killed mid-write; an answer of "no, we never received it"
    // given from a file with a torn tail is an answer that might be wrong, and
    // whoever is reading must be told rather than having to ask.
    // **A bad checksum is reported first, because it is the worse news.** A
    // torn tail is a process that was killed; a record whose CRC does not match
    // is a file that was *changed* after it was written, and everything after
    // that point is not shown for the same reason a torn tail is not.
    let corrupt = reader.corrupt_records();
    let torn = reader.torn_tail_bytes();
    if corrupt > 0 {
        eprintln!(
            "warning: a record in {} does not match its checksum — the file has been \
             damaged or changed since it was written, and nothing from that record \
             onwards ({torn} byte(s)) is shown above",
            path.display()
        );
        return ExitCode::from(2);
    }
    if torn > 0 {
        eprintln!(
            "warning: {torn} byte(s) at the end of {} do not form a whole record — \
             a process was killed mid-write, and anything in them is not shown above",
            path.display()
        );
        return ExitCode::from(2);
    }
    ExitCode::SUCCESS
}

/// One record as a line: kind, sequence number, and the bytes with `|` for
/// `SOH` so a terminal and a `grep` can both read it.
fn line(r: &Record<'_>) -> String {
    match *r {
        Record::Message { seq, bytes } => {
            format!(
                "msg  {seq}  {}",
                String::from_utf8_lossy(bytes).replace('\u{1}', "|")
            )
        }
        Record::InboundMark { seq } => format!("in   {seq}"),
        // Milliseconds on the engine's clock, printed raw: turning them into a
        // calendar needs a timezone, and ADR-0033 keeps the calendar outside.
        Record::ActivityMark { at_ms } => format!("live {at_ms}"),
        // The highest outbound number spent, which is not a message and cannot
        // be replayed — ADR-0053. Printed so a `jrnl one N` that finds nothing
        // but this says why the number was used.
        Record::OutboundMark { seq } => format!("out  {seq}"),
    }
}
