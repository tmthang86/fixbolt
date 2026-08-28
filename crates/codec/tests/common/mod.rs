//! Turning QuickFIX `.def` lines into wire bytes.
//!
//! A `.def` line is a **template**, not a message. Of the 289 `I` lines only 8
//! carry `9=` and 7 carry `10=`; `Reflector.rb` computes both at send time.
//! `<TIME>` is a 6-byte placeholder standing in for a 17- or 21-byte timestamp,
//! so a body length or checksum computed over the raw line is meaningless.
//!
//! Five steps, in this order. Getting step 2 wrong makes the very first field a
//! `BadTag`; getting step 3 wrong makes every length and checksum wrong.
//!
//! Shared with the conformance runner when that lands.

// Each test binary compiles this module separately and uses a different part of
// it, so every field looks dead to at least one of them.
#![allow(dead_code)]

use std::path::PathBuf;

/// A fixed instant, so a checksum computed here is the same on every run and on
/// every machine. The expected-output lines use this exact text.
pub const FIXED_TIME: &str = "00000000-00:00:00.000";

pub const SOH: u8 = 0x01;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// `I` — sent to the engine under test.
    In,
    /// `E` — expected back from it.
    Expect,
}

#[derive(Debug, Clone)]
pub struct DefLine {
    pub file: String,
    pub line_no: usize,
    pub direction: Direction,
    /// Session number from an `I1,` / `E2,` prefix, when present.
    pub session: Option<u32>,
    /// Wire bytes: `<TIME>` substituted, `9=` and `10=` computed if absent.
    pub wire: Vec<u8>,
    pub had_body_length: bool,
    pub had_checksum: bool,
}

/// The 59 FIX 4.4 acceptance definitions.
pub fn definitions_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../vendor/quickfix/test/definitions/server/fix44")
}

/// Every `I` and `E` line across the 59 files, normalised.
///
/// Panics rather than skipping when `vendor/` is missing. A suite that quietly
/// runs on zero real messages is worse than one that fails.
pub fn load_all() -> Vec<DefLine> {
    let dir = definitions_dir();
    let entries = std::fs::read_dir(&dir).unwrap_or_else(|e| {
        panic!(
            "cannot read {}: {e}\n\nrun scripts/fetch-quickfix-assets.sh\n\
             These tests exist to run on real QuickFIX data. Skipping them would \
             make the suite green while proving nothing.",
            dir.display()
        )
    });

    let mut files: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "def"))
        .collect();
    files.sort();
    assert_eq!(
        files.len(),
        59,
        "expected 59 FIX 4.4 acceptance definitions"
    );

    let mut out = Vec::with_capacity(600);
    for path in files {
        let name = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("?")
            .to_string();
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
        for (i, raw) in text.lines().enumerate() {
            if let Some(l) = parse_line(&name, i + 1, raw) {
                out.push(l);
            }
        }
    }
    out
}

fn parse_line(file: &str, line_no: usize, raw: &str) -> Option<DefLine> {
    let direction = match raw.chars().next()? {
        'I' => Direction::In,
        'E' => Direction::Expect,
        _ => return None, // comments, blank lines, CONNECT directives
    };
    let rest = &raw[1..];

    // Step 2: an `I1,` / `E2,` session prefix. Eight lines carry one, and feeding
    // it to the parser makes the very first field a BadTag.
    let (session, body) = match rest.find(',') {
        Some(c) if c > 0 && rest[..c].chars().all(|ch| ch.is_ascii_digit()) => {
            (rest[..c].parse::<u32>().ok(), &rest[c + 1..])
        }
        _ => (None, rest),
    };

    // Directives such as iCONNECT are not messages.
    if !body.starts_with("8=") && !body.starts_with("35=") && !body.starts_with("9=") {
        return None;
    }

    // Step 3: fixed timestamps, before anything is measured over these bytes.
    let template = substitute_times(body);

    let had_body_length = has_field(&template, "9");
    let had_checksum = has_field(&template, "10");

    // Step 4: fixify! — supply what the reflector would have supplied.
    let wire = fixify(&template, had_body_length, had_checksum);

    Some(DefLine {
        file: file.to_string(),
        line_no,
        direction,
        session,
        wire,
        had_body_length,
        had_checksum,
    })
}

/// Replace every `<TIME>` and `<TIME+N>` / `<TIME-N>` with a fixed 21-byte
/// timestamp, `N` being seconds.
///
/// The offset forms are easy to miss: there are only four of them against 352
/// bare `<TIME>`, and a loader that leaves them alone produces a 9-byte
/// placeholder where a 21-byte value belongs, so the body length is wrong and
/// nothing says why. They exist to test `SendingTime` accuracy — `<TIME+121>` is
/// two minutes into the future.
///
/// The clock wraps within the zero date, which is enough to make lengths and
/// checksums deterministic. Real offset semantics belong to the conformance
/// runner, which has to compare them against a live engine.
fn substitute_times(body: &str) -> String {
    let mut out = String::with_capacity(body.len() + 64);
    let mut rest = body;
    while let Some(i) = rest.find("<TIME") {
        out.push_str(&rest[..i]);
        let after = &rest[i + 5..];
        let Some(close) = after.find('>') else {
            out.push_str(&rest[i..]);
            return out;
        };
        let offset: i64 = after[..close].parse().unwrap_or(0);
        out.push_str(&at_offset(offset));
        rest = &after[close + 1..];
    }
    out.push_str(rest);
    out
}

fn at_offset(seconds: i64) -> String {
    if seconds == 0 {
        return FIXED_TIME.to_string();
    }
    let s = seconds.rem_euclid(86_400);
    format!(
        "00000000-{:02}:{:02}:{:02}.000",
        s / 3600,
        (s % 3600) / 60,
        s % 60
    )
}

/// Is `tag=` present as a field, rather than as a substring of some value?
fn has_field(s: &str, tag: &str) -> bool {
    let needle = format!("{tag}=");
    s.starts_with(&needle) || s.contains(&format!("\u{1}{needle}"))
}

/// Insert `9=` and append `10=` when the template omits them. A template that
/// already carries one keeps it, wrong or not — several are wrong deliberately.
fn fixify(template: &str, had_body_length: bool, had_checksum: bool) -> Vec<u8> {
    let mut bytes: Vec<u8> = template.bytes().collect();

    if !had_body_length {
        if let Some(first_soh) = bytes.iter().position(|&b| b == SOH) {
            let body_start = first_soh + 1;
            let body_end = trailer_start(&bytes).unwrap_or(bytes.len());
            let len = body_end.saturating_sub(body_start);
            let insert = format!("9={len}\u{1}").into_bytes();
            bytes.splice(body_start..body_start, insert);
        }
    }

    if !had_checksum {
        let sum: u8 = bytes.iter().fold(0u8, |a, &b| a.wrapping_add(b));
        bytes.extend_from_slice(format!("10={sum:03}\u{1}").as_bytes());
    }

    bytes
}

/// Offset of the `10=` field, if the bytes already carry one.
fn trailer_start(bytes: &[u8]) -> Option<usize> {
    bytes
        .windows(4)
        .position(|w| w == [SOH, b'1', b'0', b'='])
        .map(|i| i + 1)
}
