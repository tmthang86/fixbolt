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

use fixbolt_codec::TimestampCache;

/// A fixed instant, so a checksum computed here is the same on every run and on
/// every machine.
///
/// **Two widths, and the corpus's own `9=` values say which goes where.**
/// `[measured 2026-08-28]` solving `9=` for the length of `<TIME>` over every
/// line that carries its own body length:
///
/// | Line | `<TIME>` width | Evidence |
/// |---|---|---|
/// | `I` | **17** — `YYYYMMDD-HH:MM:SS` | `2d_GarbledMessage` and `3c_GarbledMessage`, 2 lines each, all four consistent |
/// | `E` | **21** — with `.mmm` | `SessionReset.def` lines 18 and 27 |
///
/// An `E` line is the engine's own output and FIX 4.4 `SendingTime` carries
/// milliseconds; an `I` line is what the reflector sends, and it does not.
///
/// Getting this wrong costs 4 bytes per timestamp, which is invisible until
/// something compares a `9=`.
/// # Why not `00000000-00:00:00`
///
/// That is the corpus's own placeholder for output it does not compare, and it
/// was this loader's substitution until a session tried to validate it. **It is
/// not a date** — month 00, day 00 — so a `SendingTime` check that accepts it
/// accepts nothing a real engine would. QuickFIX's reflector substitutes the
/// real clock; substituting a real instant is what makes this loader behave the
/// same way, deterministically.
///
/// Midday, so the four `<TIME±N>` offsets in the corpus stay inside the day,
/// and years away from every hard-coded `52=` in it — 2001, 2002, 2004 — so
/// none of those becomes accidentally fresh.
pub const FIXED_TIME_IN: &str = "20260828-12:00:00";
/// See [`FIXED_TIME_IN`].
pub const FIXED_TIME_OUT: &str = "20260828-12:00:00.000";

/// [`FIXED_TIME_IN`] as Unix milliseconds. The base every `<TIME±N>` offset is
/// measured from.
const BASE_UNIX_MS: u64 = 1_787_918_400_000;

/// [`FIXED_TIME_IN`] on the scale `Input::Tick` carries: milliseconds since
/// 0000-01-01.
///
/// The runner feeds this before every message, so a session under test has a
/// "now" that agrees with the `52=` it is about to read. Advancing it is the
/// heartbeat rule, and that arrives with step 4 of the session plan.
///
/// Not checked here — `conformance` has no timestamp parser and must not grow
/// one to check its own constant. `crates/session/tests/score.rs` sees both
/// crates and proves this equals `clock::parse_utc(FIXED_TIME_IN)`.
pub const FIXED_TIME_MILLIS: u64 = BASE_UNIX_MS + 719_528 * 86_400_000;

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
    load(false)
}

/// Every `.def` file, read from the other side.
///
/// `I` lines become what this engine is expected to **send** and `E` lines
/// become what arrives. Two things change with them, and both are load-bearing:
///
/// * **`iDISCONNECT` and `eDISCONNECT` swap** — the side that hangs up is the
///   other one now.
/// * **an `I` line's `<TIME>` grows to 21 bytes.** The plain corpus gives it 17
///   because QuickFIX's reflector writes it and writes no milliseconds; here
///   *this engine* writes it, and it writes milliseconds. The `9=` computed
///   over those bytes is compared exactly, so getting this wrong is four bytes
///   on every message in the suite.
///
/// `iCONNECT` does not swap: both sides see a connection open.
///
/// # Errors
///
/// As [`scenarios`].
pub fn scenarios_mirrored() -> Result<Vec<Scenario>, LoadError> {
    let mut all = load(true)?;
    for s in &mut all {
        for step in &mut s.steps {
            step.kind = match core::mem::replace(&mut step.kind, Kind::Connect) {
                Kind::Send(m) => Kind::Expect(m),
                Kind::Expect(m) => Kind::Send(m),
                Kind::Disconnect => Kind::ExpectDisconnect,
                Kind::ExpectDisconnect => Kind::Disconnect,
                Kind::Connect => Kind::Connect,
            };
        }
    }
    Ok(all)
}

/// Can this engine play the other side of this file?
///
/// `ADR-0004` decision 6, applied rather than quoted: a file mirrors when every
/// line **this engine would have to send** is something a correct engine would
/// actually send. Anything else is a file whose whole purpose is the acceptor
/// refusing rubbish, and an initiator has no analogue for it.
///
/// Takes a scenario in mirrored form — the lines to check are its
/// [`Kind::Expect`] ones.
#[must_use]
pub fn mirrors(s: &Scenario) -> bool {
    s.steps.iter().all(|step| match &step.kind {
        Kind::Expect(m) => sendable(&m.wire),
        // Mirrored, an `ExpectDisconnect` is **this engine** hanging up — and
        // it can only come from an `iDISCONNECT` in the original, because
        // every file's own `eDISCONNECT` mirrors the other way, into an input.
        // `[measured 2026-08-30]` exactly one file has one:
        // `1b_DuplicateIdentity.def`, whose last line hangs up the *first*
        // connection after the second was refused. Nothing on the wire says to
        // — no message arrived, no timer fired. It is the harness tidying up
        // after itself, and no initiator has an analogue for it.
        Kind::ExpectDisconnect => false,
        _ => true,
    })
}

/// Would a correct engine ever put these bytes on the wire?
fn sendable(wire: &[u8]) -> bool {
    if !wire.starts_with(b"8=FIX.4.4\x01") {
        return false;
    }
    wire.split(|b| *b == SOH).all(|f| {
        f.is_empty() || {
            match f.iter().position(|c| *c == b'=') {
                None => false,
                Some(eq) => {
                    let (tag, value) = (&f[..eq], &f[eq + 1..]);
                    !tag.is_empty() && tag.iter().all(u8::is_ascii_digit) && !value.is_empty()
                }
            }
        }
    })
}

fn load(mirrored: bool) -> Result<Vec<Scenario>, LoadError> {
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
            if let Some(step) = parse_line(&name, i + 1, raw, mirrored)? {
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

fn parse_line(
    file: &str,
    line_no: usize,
    raw: &str,
    mirrored: bool,
) -> Result<Option<Step>, LoadError> {
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
            // An `E` line is engine output and carries milliseconds; an `I`
            // line is what the reflector sends and does not. Mirrored, this
            // engine writes both.
            let msg = message(body, mirrored || letter == 'E').ok_or_else(unknown)?;
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

fn message(body: &str, millis: bool) -> Option<Message> {
    if !(body.starts_with("8=") || body.starts_with("9=") || body.starts_with("35=")) {
        return None;
    }
    // Step 3: fixed timestamps, before anything is measured over these bytes.
    let template = substitute_times(body, millis);
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
fn substitute_times(body: &str, millis: bool) -> String {
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
        out.push_str(&at_offset(offset, millis));
        rest = &after[close + 1..];
    }
    out.push_str(rest);
    out
}

/// [`FIXED_TIME_IN`] shifted by `seconds`, in whichever of the two widths the
/// line calls for.
///
/// Real arithmetic on a real instant. The previous version wrapped the offset
/// with `rem_euclid` because the base was midnight of year zero and there was
/// nowhere to go backwards to — which turned `<TIME-121>` into *121 seconds
/// before tomorrow midnight*, 86 279 seconds in the wrong direction, in the one
/// file that exists to test `SendingTime` accuracy.
fn at_offset(seconds: i64, millis: bool) -> String {
    let mut cache = TimestampCache::new();
    let at = BASE_UNIX_MS.saturating_add_signed(seconds.saturating_mul(1_000));
    let full = cache.format(at);
    let width = if millis {
        full.len()
    } else {
        full.len() - 4 // drop `.sss`
    };
    String::from_utf8_lossy(&full[..width]).into_owned()
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
