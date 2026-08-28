//! Turning QuickFIX `.def` files into a runnable script.
//!
//! A `.def` line is a **template**, not a message. `[measured 2026-08-28]` of
//! the 289 `I` lines only 8 carry `9=` and 7 carry `10=`; `Reflector.rb`
//! computes both at send time. `<TIME>` is a 6-byte placeholder standing in for
//! a 17- or 21-byte timestamp, so a body length or checksum computed over the
//! raw line is meaningless.
//!
//! Five steps, in this order. Getting step 2 wrong makes the very first field a
//! `BadTag`; getting step 3 wrong makes every length and checksum wrong.
//!
//! 1. read the directive letter — `I`, `E`, `i`, `e`;
//! 2. strip an `I1,` / `e2,` session prefix;
//! 3. substitute `<TIME>` and `<TIME±N>`;
//! 4. `fixify!` — insert `9=` and append `10=` when the template omits them;
//! 5. classify.
//!
//! # Read the files one at a time. `cat *.def` corrupts the corpus.
//!
//! `[measured 2026-08-28]` **35 of the 59 files do not end in a newline**, so
//! concatenating them glues the last line of one file to the first line of the
//! next — and the first line of most of these files is a `#` comment. The
//! symptom is a corpus that appears to carry comments on the same line as a
//! directive: `eDISCONNECT# If message is garbled, it should be ignored` looks
//! like one line and is two, from two different files. Counting `eDISCONNECT`
//! that way gives **28** instead of 64.
//!
//! This is not hypothetical. It is where a claim in this crate's own plan came
//! from, and the claim was wrong. `tests/script.rs::
//! concatenating_the_files_corrupts_the_corpus` reproduces it deliberately.
//!
//! # Nothing is dropped in silence
//!
//! A line beginning `I`, `E`, `i` or `e` that this loader cannot understand is a
//! [`LoadError`], not a skipped line. A loader that drops what it does not
//! recognise still produces a plausible transcript, and the test that would
//! catch it is a count — which is why the counts are asserted, and why this is
//! belt as well as braces.

use std::fmt;
use std::path::PathBuf;

/// A fixed instant, so a checksum computed here is the same on every run and on
/// every machine. The expected-output lines use this exact text.
pub const FIXED_TIME: &str = "00000000-00:00:00.000";

const SOH: u8 = 0x01;

/// Why the corpus could not be loaded.
///
/// A typed error rather than a panic: `CLAUDE.md` §2 rule 7 denies `panic!` in a
/// library crate, and the loud failure belongs at the call site, which is a test
/// and may panic freely. [`fmt::Display`] carries the instruction.
#[derive(Debug)]
pub enum LoadError {
    /// The definitions directory could not be read.
    NoCorpus { path: PathBuf, cause: String },
    /// A `.def` file could not be read.
    UnreadableFile { path: PathBuf, cause: String },
    /// The directory held a number of `.def` files other than 59.
    WrongFileCount(usize),
    /// A line began with a directive letter and could not be understood.
    UnknownDirective {
        file: String,
        line_no: usize,
        line: String,
    },
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoCorpus { path, cause } => write!(
                f,
                "cannot read {}: {cause}\n\nrun scripts/fetch-quickfix-assets.sh\n\
                 These tests exist to run on real QuickFIX data. Skipping them would \
                 make the suite green while proving nothing.",
                path.display()
            ),
            Self::UnreadableFile { path, cause } => {
                write!(f, "cannot read {}: {cause}", path.display())
            }
            Self::WrongFileCount(n) => write!(
                f,
                "expected 59 FIX 4.4 acceptance definitions, found {n}. \
                 The corpus tracks mutable master — see STATUS.md open item 7."
            ),
            Self::UnknownDirective {
                file,
                line_no,
                line,
            } => write!(
                f,
                "{file}:{line_no}: cannot read this directive:\n  {line}\n\n\
                 Refusing to skip it. A loader that drops what it does not \
                 recognise still produces a plausible transcript."
            ),
        }
    }
}

impl std::error::Error for LoadError {}

/// A message line and what the template already carried.
///
/// `had_body_length` and `had_checksum` matter because six lines carry a
/// deliberately wrong `9=` and 238 carry the literal `10=0`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    /// Wire bytes: `<TIME>` substituted, `9=` and `10=` computed if absent.
    pub wire: Vec<u8>,
    pub had_body_length: bool,
    pub had_checksum: bool,
}

/// What one directive line asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kind {
    /// `iCONNECT` — open a session.
    Connect,
    /// `i…DISCONNECT` — the counterparty goes away.
    Disconnect,
    /// `e…DISCONNECT` — the engine is expected to drop the connection.
    ExpectDisconnect,
    /// `I…` — bytes to feed the engine.
    Send(Message),
    /// `E…` — bytes the engine is expected to produce.
    Expect(Message),
}

/// One directive, with where it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    pub file: String,
    pub line_no: usize,
    /// Session number from an `I1,` / `e2,` prefix. `[measured]` 16 of the 669
    /// steps carry one, all in two files.
    pub session: Option<u32>,
    pub kind: Kind,
}

impl Step {
    #[must_use]
    pub fn file(&self) -> &str {
        &self.file
    }

    #[must_use]
    pub fn message(&self) -> Option<&Message> {
        match &self.kind {
            Kind::Send(m) | Kind::Expect(m) => Some(m),
            _ => None,
        }
    }
}

/// One `.def` file: the steps in it, in order.
#[derive(Debug, Clone)]
pub struct Scenario {
    pub file: String,
    pub steps: Vec<Step>,
}

/// Where the 59 FIX 4.4 acceptance definitions live.
#[must_use]
pub fn definitions_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../vendor/quickfix/test/definitions/server/fix44")
}

/// Every `.def` file, in sorted order, parsed.
///
/// # Errors
///
/// [`LoadError`] when `vendor/` is missing, unreadable, or holds a number of
/// definitions other than 59. Never skips: a suite that quietly runs on zero
/// real messages is worse than one that fails.
pub fn scenarios() -> Result<Vec<Scenario>, LoadError> {
    let dir = definitions_dir();
    let entries = std::fs::read_dir(&dir).map_err(|e| LoadError::NoCorpus {
        path: dir.clone(),
        cause: e.to_string(),
    })?;

    let mut files: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "def"))
        .collect();
    files.sort();
    if files.len() != 59 {
        return Err(LoadError::WrongFileCount(files.len()));
    }

    let mut out = Vec::with_capacity(59);
    for path in files {
        let name = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or_default()
            .to_string();
        let text = std::fs::read_to_string(&path).map_err(|e| LoadError::UnreadableFile {
            path: path.clone(),
            cause: e.to_string(),
        })?;
        let mut steps = Vec::new();
        for (i, raw) in text.lines().enumerate() {
            if let Some(step) = parse_line(&name, i + 1, raw)? {
                steps.push(step);
            }
        }
        out.push(Scenario { file: name, steps });
    }
    Ok(out)
}

/// Every step across all 59 files, flattened.
///
/// # Errors
///
/// As [`scenarios`].
pub fn load_all() -> Result<Vec<Step>, LoadError> {
    Ok(scenarios()?.into_iter().flat_map(|s| s.steps).collect())
}

fn parse_line(file: &str, line_no: usize, raw: &str) -> Result<Option<Step>, LoadError> {
    let unknown = || LoadError::UnknownDirective {
        file: file.to_string(),
        line_no,
        line: raw.replace('\u{1}', "|"),
    };
    let Some(letter) = raw.chars().next() else {
        return Ok(None); // blank line
    };
    if !matches!(letter, 'I' | 'E' | 'i' | 'e') {
        return Ok(None); // comments, blank lines
    }
    let rest = &raw[1..];

    // Step 2: an `I1,` / `e2,` session prefix. Feeding it to the parser makes
    // the very first field a BadTag.
    let (session, body) = match rest.find(',') {
        Some(c) if c > 0 && rest[..c].chars().all(|ch| ch.is_ascii_digit()) => {
            (rest[..c].parse::<u32>().ok(), &rest[c + 1..])
        }
        _ => (None, rest),
    };

    let kind = match (letter, body) {
        ('i', "CONNECT") => Kind::Connect,
        ('i', "DISCONNECT") => Kind::Disconnect,
        ('e', "DISCONNECT") => Kind::ExpectDisconnect,
        ('i' | 'e', _) => return Err(unknown()),
        _ => {
            let msg = message(body).ok_or_else(unknown)?;
            if letter == 'I' {
                Kind::Send(msg)
            } else {
                Kind::Expect(msg)
            }
        }
    };

    Ok(Some(Step {
        file: file.to_string(),
        line_no,
        session,
        kind,
    }))
}

fn message(body: &str) -> Option<Message> {
    if !(body.starts_with("8=") || body.starts_with("9=") || body.starts_with("35=")) {
        return None;
    }
    // Step 3: fixed timestamps, before anything is measured over these bytes.
    let template = substitute_times(body);
    let had_body_length = has_field(&template, "9");
    let had_checksum = has_field(&template, "10");
    // Step 4: fixify! — supply what the reflector would have supplied.
    Some(Message {
        wire: fixify(&template, had_body_length, had_checksum),
        had_body_length,
        had_checksum,
    })
}

/// Replace every `<TIME>` and `<TIME+N>` / `<TIME-N>` with a fixed 21-byte
/// timestamp, `N` being seconds.
///
/// The offset forms are easy to miss: there are only four of them against 352
/// bare `<TIME>`, and a loader that leaves them alone produces a 6-byte
/// placeholder where a 21-byte value belongs, so the body length is wrong and
/// nothing says why. They exist to test `SendingTime` accuracy — `<TIME+121>` is
/// two minutes into the future.
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

    if !had_body_length && let Some(first_soh) = bytes.iter().position(|&b| b == SOH) {
        let body_start = first_soh + 1;
        let body_end = trailer_start(&bytes).unwrap_or(bytes.len());
        let len = body_end.saturating_sub(body_start);
        let insert = format!("9={len}\u{1}").into_bytes();
        bytes.splice(body_start..body_start, insert);
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

/// Replace a message's `10=` with the real checksum of its own bytes.
///
/// `[measured 2026-08-28]` 240 of the corpus's 244 expected checksums are not
/// three digits — 238 are the literal `10=0` and 2 are `10=7`. Only 4 look like
/// a checksum at all. `Comparator.rb` matches tag 10 by shape against the **received**
/// value, so the placeholder never had to be real — which means a corpus `E`
/// line cannot stand in for engine output without this. Used by the runner's
/// own fake session, where feeding back an unmodified `E` line would fail rule 4
/// and make the runner look broken.
#[must_use]
pub fn with_real_checksum(wire: &[u8]) -> Vec<u8> {
    let Some(at) = trailer_start(wire) else {
        return wire.to_vec();
    };
    let mut out = wire[..at].to_vec();
    let sum: u8 = out.iter().fold(0u8, |a, &b| a.wrapping_add(b));
    out.extend_from_slice(format!("10={sum:03}\u{1}").as_bytes());
    out
}
