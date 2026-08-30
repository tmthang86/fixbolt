//! Driving a session through a `.def` file and scoring the result.
//!
//! # The trait is per engine, not per connection
//!
//! `1b_DuplicateIdentity.def` opens connection 1, logs it on, opens connection 2
//! with the **same** identity, and expects connection 2 to be dropped. The
//! second Logon is refused *because* the first connection exists. A harness that
//! hands each connection its own session object makes that test impossible to
//! pass, so [`SessionUnderTest::step`] takes a [`Conn`] and one instance sees
//! every connection. The plan had it the other way round.
//!
//! `[measured]` 2 of the 59 files need this. The other 57 use one connection and
//! never name it.
//!
//! # `0 / 59` is not evidence
//!
//! While no session exists, a comparator that always passes and a runner that
//! runs nothing both report `0 / 59`. So [`Replay`] exists: a fake that answers
//! with exactly what the file expects and must score `59 / 59`. Without it,
//! step 4 of the plan would have been "it printed zero", which is not a result.

use core::fmt;

use crate::compare::{Mismatch, compare};
use crate::script::{
    FIXED_TIME_MILLIS, Kind, LoadError, Scenario, Step, mirrors, scenarios, scenarios_mirrored,
    with_real_checksum,
};

/// Which connection an input arrived on.
///
/// `1` unless the `.def` line says otherwise. `[measured]` only
/// `1b_DuplicateIdentity.def` and `AlreadyLoggedOn.def` ever say otherwise.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Conn(pub u32);

/// What the harness feeds a session.
///
/// No socket and no clock: time arrives as [`Input::Tick`]. That is
/// `CLAUDE.md` §2 non-negotiable 2, and it is the constraint this trait exists
/// to hold. If an implementation needs anything not in this enum, the invariant
/// is wrong and the plan says to stop rather than widen the trait.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Input<'a> {
    /// A counterparty opened a connection.
    Connect,
    /// A counterparty closed one.
    Disconnect,
    /// Bytes arrived. One whole message; framing is the transport's job.
    Bytes(&'a [u8]),
    /// Milliseconds since an arbitrary epoch. The only way time enters.
    Tick(u64),
}

/// Whether the connection survived the input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Link {
    Up,
    Dropped,
}

/// A session machine the runner can drive.
///
/// `emit` is generic rather than `dyn` so an implementation pays no vtable.
/// This is the **harness's** view; the engine's own hot-path API is D1's
/// `Input`/`Output`, and an adapter will bridge them when `session` exists.
pub trait SessionUnderTest {
    /// Feed one input on one connection, calling `emit` once per outbound
    /// message, in order.
    fn step<F: FnMut(&[u8])>(&mut self, conn: Conn, input: Input<'_>, emit: F) -> Link;
}

/// A session that never answers. The state of the world until `session` exists.
pub struct NullSession;

impl SessionUnderTest for NullSession {
    fn step<F: FnMut(&[u8])>(&mut self, _conn: Conn, _input: Input<'_>, _emit: F) -> Link {
        Link::Up
    }
}

/// Why one step of one file failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Failure {
    pub file: String,
    pub line_no: usize,
    pub reason: Reason,
}

/// What went wrong.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reason {
    /// An `E` line, and the session said nothing.
    NoOutput,
    /// An `E` line, and what came back differed.
    Mismatch(Mismatch),
    /// An `e…DISCONNECT`, and the connection is still up.
    StillConnected,
    /// The session sent something no `E` line asked for.
    Unexpected(Vec<u8>),
}

impl fmt::Display for Reason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoOutput => write!(f, "expected a message, got silence"),
            Self::Mismatch(m) => write!(f, "{m:?}"),
            Self::StillConnected => write!(f, "expected a disconnect, the link is up"),
            Self::Unexpected(b) => write!(
                f,
                "unexpected output: {}",
                String::from_utf8_lossy(b).replace('\u{1}', "|")
            ),
        }
    }
}

/// The score, and every reason it is not higher.
#[derive(Debug, Clone, Default)]
pub struct Report {
    pub scenarios: usize,
    pub passed: usize,
    /// Files with no failures, in corpus order.
    pub passed_files: Vec<String>,
    pub failures: Vec<Failure>,
}

impl Report {
    #[must_use]
    pub const fn failed_scenarios(&self) -> usize {
        self.scenarios - self.passed
    }
}

impl fmt::Display for Report {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "{} / {}", self.passed, self.scenarios)?;
        // Enough to find the problem, not so much that the real one scrolls off.
        for x in self.failures.iter().take(20) {
            writeln!(f, "  {}:{} {}", x.file, x.line_no, x.reason)?;
        }
        if self.failures.len() > 20 {
            writeln!(f, "  … and {} more", self.failures.len() - 20)?;
        }
        Ok(())
    }
}

/// Run the 50 mirrorable scenarios, this engine playing the **initiator**.
///
/// The corpus is `crates/conformance/src/script.rs`'s
/// [`scenarios_mirrored`](crate::script::scenarios_mirrored), and the eight
/// files that cannot mirror are dropped by
/// [`mirrors`](crate::script::mirrors) rather than by name — `ADR-0004`
/// decision 6 as amended by `ADR-0006`.
///
/// `Report::scenarios` is 50, not 59: a score out of 59 here would be a score
/// against files this side has no analogue for.
///
/// # Errors
///
/// [`LoadError`] if the corpus cannot be read.
pub fn run_mirrored<S: SessionUnderTest>(
    mut make: impl FnMut(&Scenario) -> S,
) -> Result<Report, LoadError> {
    let all: Vec<Scenario> = scenarios_mirrored()?.into_iter().filter(mirrors).collect();
    let mut report = Report {
        scenarios: all.len(),
        ..Report::default()
    };
    for s in &all {
        let mut session = make(s);
        let failures = run_scenario(s, &mut session);
        if failures.is_empty() {
            report.passed += 1;
            report.passed_files.push(s.file.clone());
        }
        report.failures.extend(failures);
    }
    Ok(report)
}

/// Run every scenario, building one session per file.
///
/// # Errors
///
/// [`LoadError`] if the corpus cannot be read.
pub fn run<S: SessionUnderTest>(mut make: impl FnMut(&Scenario) -> S) -> Result<Report, LoadError> {
    let all = scenarios()?;
    let mut report = Report {
        scenarios: all.len(),
        ..Report::default()
    };
    for s in &all {
        let mut session = make(s);
        let failures = run_scenario(s, &mut session);
        if failures.is_empty() {
            report.passed += 1;
            report.passed_files.push(s.file.clone());
        }
        report.failures.extend(failures);
    }
    Ok(report)
}

/// How many heartbeat intervals the harness will wait for output the file
/// expects and the session has not produced.
///
/// A `.def` file has no "wait" directive: an `E` line with no `I` line in front
/// of it *is* the wait. Three is enough for every file in the corpus —
/// `6_SendTestRequest`, the one that waits most, needs one per line.
const WAITS: usize = 3;

/// Drive one file. Returns every failure in it, in line order.
pub fn run_scenario<S: SessionUnderTest>(s: &Scenario, session: &mut S) -> Vec<Failure> {
    let mut failures = Vec::new();
    // Outbound messages the session has produced and no `E` line has claimed.
    let mut pending: Vec<Vec<u8>> = Vec::new();
    let mut dropped: Vec<Conn> = Vec::new();
    // A session has no clock, so the harness is its clock. It starts at the
    // instant the corpus writes into every `I` line and only ever moves
    // forward, one `HeartBtInt` at a time and only when the file is waiting.
    let mut now = FIXED_TIME_MILLIS;
    let beat = heart_bt_ms(s);

    for step in &s.steps {
        let conn = Conn(step.session.unwrap_or(1));
        let at = |reason| Failure {
            file: s.file.clone(),
            line_no: step.line_no,
            reason,
        };
        match &step.kind {
            Kind::Connect => {
                dropped.retain(|c| *c != conn);
                feed(session, conn, Input::Connect, &mut pending, &mut dropped);
                feed(session, conn, Input::Tick(now), &mut pending, &mut dropped);
            }
            Kind::Disconnect => {
                feed(session, conn, Input::Disconnect, &mut pending, &mut dropped);
            }
            Kind::Send(m) => {
                feed(session, conn, Input::Tick(now), &mut pending, &mut dropped);
                feed(
                    session,
                    conn,
                    Input::Bytes(&m.wire),
                    &mut pending,
                    &mut dropped,
                );
            }
            Kind::Expect(m) => {
                // `[measured]` 33 of the 250 `E` lines do not follow an `I`
                // line. They are the engine speaking on its own — a heartbeat
                // that came due, a test request after silence — so the only
                // thing that can produce them is time passing.
                for _ in 0..WAITS {
                    if !pending.is_empty() {
                        break;
                    }
                    now += beat;
                    feed(session, conn, Input::Tick(now), &mut pending, &mut dropped);
                }
                match pop_front(&mut pending) {
                    None => failures.push(at(Reason::NoOutput)),
                    Some(actual) => {
                        if let Err(e) = compare(&m.wire, &actual) {
                            failures.push(at(Reason::Mismatch(e)));
                        }
                    }
                }
            }
            Kind::ExpectDisconnect => {
                // Same rule, and `6_SendTestRequest` is why: its last line is a
                // disconnect the session reaches by running out of patience,
                // with no message in between.
                for _ in 0..WAITS {
                    if dropped.contains(&conn) {
                        break;
                    }
                    now += beat;
                    feed(session, conn, Input::Tick(now), &mut pending, &mut dropped);
                }
                if !dropped.contains(&conn) {
                    failures.push(at(Reason::StillConnected));
                }
            }
        }
    }

    // Anything the session said that no line asked for is a failure too: a
    // session that answers every input twice would otherwise score full marks.
    if let Some(extra) = pending.into_iter().next() {
        failures.push(Failure {
            file: s.file.clone(),
            line_no: s.steps.last().map_or(0, |x| x.line_no),
            reason: Reason::Unexpected(extra),
        });
    }
    failures
}

fn feed<S: SessionUnderTest>(
    session: &mut S,
    conn: Conn,
    input: Input<'_>,
    pending: &mut Vec<Vec<u8>>,
    dropped: &mut Vec<Conn>,
) {
    let link = session.step(conn, input, |bytes| pending.push(bytes.to_vec()));
    if link == Link::Dropped && !dropped.contains(&conn) {
        dropped.push(conn);
    }
}

/// The `HeartBtInt` this file's counterparty asked for, in milliseconds.
///
/// Taken from the file's own Logon rather than configured, because the corpus
/// varies it deliberately: `[measured]` `108=30` in most files, `108=6` in
/// `4a_NoDataSentDuringHeartBtInt` and `6_SendTestRequest` — the only two whose
/// output depends on it — and `108=2` in the initiator definitions this crate
/// does not run. 30 seconds if the file never says.
fn heart_bt_ms(s: &Scenario) -> u64 {
    for step in &s.steps {
        if let Kind::Send(m) | Kind::Expect(m) = &step.kind
            && field(&m.wire, 35) == Some(b"A")
            && let Some(v) = field(&m.wire, 108)
            && let Ok(secs) = core::str::from_utf8(v).unwrap_or("").parse::<u64>()
        {
            return secs * 1_000;
        }
    }
    30_000
}

/// The value of one field of a wire message, by tag.
fn field(wire: &[u8], tag: u32) -> Option<&[u8]> {
    let mut needle = [0u8; 12];
    needle[0] = 0x01;
    let mut n = 1;
    let mut digits = [0u8; 10];
    let mut at = digits.len();
    let mut t = tag;
    loop {
        at -= 1;
        digits[at] = b'0' + u8::try_from(t % 10).unwrap_or(0);
        t /= 10;
        if t == 0 {
            break;
        }
    }
    for &d in &digits[at..] {
        needle[n] = d;
        n += 1;
    }
    needle[n] = b'=';
    n += 1;
    let needle = &needle[..n];
    // The first field has no SOH in front of it, so try it separately.
    let start = if wire.starts_with(&needle[1..]) {
        needle.len() - 1
    } else {
        wire.windows(needle.len()).position(|w| w == needle)? + needle.len()
    };
    let end = wire[start..].iter().position(|&b| b == 0x01)? + start;
    Some(&wire[start..end])
}

fn pop_front(v: &mut Vec<Vec<u8>>) -> Option<Vec<u8>> {
    if v.is_empty() {
        None
    } else {
        Some(v.remove(0))
    }
}

// ---- the fake that proves the runner ------------------------------------

/// Answers a scenario with exactly what the scenario expects.
///
/// Exists so `0 / 59` from [`NullSession`] means something. A runner that never
/// compares, or a comparator that always passes, both give `0 / 59` while no
/// session exists — and both give less than `59 / 59` here.
///
/// It replays the file's own `E` lines with a **recomputed checksum**: 240 of
/// the corpus's 244 expected `10=` values are placeholders and rule 4 matches
/// the received value, so feeding them back unmodified would fail and make the
/// runner look broken. See [`crate::script::with_real_checksum`].
pub struct Replay {
    /// The scenario's steps from the current position on.
    remaining: Vec<Step>,
    at: usize,
    corrupt: bool,
    corrupted: bool,
    chatty: bool,
}

impl Replay {
    #[must_use]
    pub fn new(s: &Scenario) -> Self {
        Self {
            remaining: s.steps.clone(),
            at: 0,
            corrupt: false,
            corrupted: false,
            chatty: false,
        }
    }

    /// Change one field of the first message this replays.
    ///
    /// The reversal for the runner: one wrong byte in one message of every file
    /// must take the score from 59 to 0.
    #[must_use]
    pub const fn corrupt_first_expect(mut self) -> Self {
        self.corrupt = true;
        self
    }

    /// Send one message nobody asked for, at the end.
    ///
    /// The reversal for the leftover-output check: a session that answers every
    /// line correctly *and* says one extra thing has not passed. Without this,
    /// deleting that check leaves every test green.
    #[must_use]
    pub const fn with_extra_output(mut self) -> Self {
        self.chatty = true;
        self
    }

    /// What the file expects next, if anything: a message to send back, or a
    /// disconnect on this connection.
    ///
    /// Separate from [`drain`](Self::drain) so the borrow of `remaining` ends
    /// before `answer` needs `&mut self`.
    fn next_action(&self, conn: Conn) -> Option<Action> {
        match &self.remaining.get(self.at)?.kind {
            Kind::Expect(m) => Some(Action::Emit(m.wire.clone())),
            Kind::ExpectDisconnect
                if Conn(self.remaining.get(self.at)?.session.unwrap_or(1)) == conn =>
            {
                Some(Action::Drop)
            }
            _ => None,
        }
    }

    /// Emit the expected messages that follow the input just consumed, and say
    /// whether the file expects the link to drop.
    fn drain<F: FnMut(&[u8])>(&mut self, conn: Conn, mut emit: F) -> Link {
        let mut link = Link::Up;
        while let Some(action) = self.next_action(conn) {
            self.at += 1;
            match action {
                Action::Emit(wire) => {
                    let answer = self.answer(&wire);
                    emit(&answer);
                }
                Action::Drop => link = Link::Dropped,
            }
        }
        if self.chatty && self.at >= self.remaining.len() {
            self.chatty = false;
            emit(&with_real_checksum(b"8=FIX.4.4\x019=5\x0135=0\x0110=0\x01"));
        }
        link
    }

    fn answer(&mut self, template: &[u8]) -> Vec<u8> {
        let mut wire = template.to_vec();
        if self.corrupt && !self.corrupted {
            self.corrupted = true;
            corrupt_one_value(&mut wire);
        }
        with_real_checksum(&wire)
    }
}

impl SessionUnderTest for Replay {
    fn step<F: FnMut(&[u8])>(&mut self, conn: Conn, input: Input<'_>, emit: F) -> Link {
        // A `Tick` is the harness's clock, not a line of the file. Consuming a
        // step for it would shift `Replay` one line out of phase with the
        // scenario and take `59 / 59` to nonsense.
        if matches!(input, Input::Tick(_)) {
            return Link::Up;
        }
        // Consume the input step this call corresponds to, then answer.
        if self
            .remaining
            .get(self.at)
            .is_some_and(|s| matches!(s.kind, Kind::Connect | Kind::Disconnect | Kind::Send(_)))
        {
            self.at += 1;
        }
        self.drain(conn, emit)
    }
}

/// One thing [`Replay`] does next.
enum Action {
    Emit(Vec<u8>),
    Drop,
}

/// Flip one byte of one value that the comparator actually compares.
///
/// Not "any byte": the frame (`8`, `9`, `35`) is a different failure, the five
/// tags in `fields.fmt` are matched by shape so a changed digit still passes,
/// and `10=` is recomputed on the way out. `[measured]` picking the eleventh
/// byte from the end instead left 6 of the 59 scenarios passing, which is how
/// this ended up surgical.
fn corrupt_one_value(wire: &mut [u8]) {
    let mut at = 0usize;
    while at < wire.len() {
        let end = wire[at..]
            .iter()
            .position(|&b| b == 0x01)
            .map_or(wire.len(), |i| at + i);
        if let Some(eq) = wire[at..end].iter().position(|&b| b == b'=') {
            let tag: u32 = wire[at..at + eq].iter().fold(0u32, |a, &d| {
                a.wrapping_mul(10).wrapping_add(u32::from(d - b'0'))
            });
            let value = at + eq + 1;
            let comparable = !matches!(tag, 8 | 9 | 10 | 35)
                && !crate::compare::LOOSE_TAGS.contains(&tag)
                && value < end;
            if comparable {
                wire[value] = if wire[value] == b'X' { b'Y' } else { b'X' };
                return;
            }
        }
        at = end + 1;
    }
}
