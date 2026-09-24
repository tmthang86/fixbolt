//! The Prometheus text exposition format 0.0.4, written into a buffer reserved
//! once — ADR-0170 decision 4.
//!
//! **Nothing here allocates.** Every write goes through [`Out::put`], which
//! refuses rather than grows: a buffer too small is an [`Overflow`] the caller
//! turns into a `500`, never a reallocation on the exporter thread. The buffer
//! is sized by [`capacity_for`], an upper bound worked out from the series
//! table rather than measured, and `a_full_snapshot_fits_the_reserved_buffer`
//! is what proves the bound holds at its worst case.
//!
//! Numbers are written digit by digit. A clock skew or an age is milliseconds
//! printed as seconds with three decimals — no float, so no rounding and no
//! `core::fmt` float machinery.
//!
//! The encoder reads an [`EngineView`] rather than an engine's `Snapshot`, so a
//! test can build the worst case — every session, every value at its longest —
//! without an engine that has sixty-four sessions and `u64::MAX` in every
//! counter.

use fixbolt_engine::observe::{MAX_SESSIONS, SessionSnapshot, Snapshot};

use crate::series::{self, ALL, EVENT_KINDS, REASONS, Scope, Series, Source};

/// The longest value this encoder prints: `-9223372036854775.808`, an `i64`
/// of milliseconds as seconds. A `u64` is at most 20 digits.
pub(crate) const VALUE_MAX: usize = 21;

/// One session, as the encoder needs it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct SessionView {
    pub(crate) conn: u64,
    pub(crate) logged_on: bool,
    pub(crate) next_out: u32,
    pub(crate) next_in: u32,
    pub(crate) skew_ms: Option<i64>,
    pub(crate) pending_output: bool,
    pub(crate) journal_refused: u32,
    pub(crate) resend_beyond: u32,
}

impl SessionView {
    pub(crate) const fn of(s: &SessionSnapshot) -> Self {
        Self {
            conn: s.id(),
            logged_on: s.logged_on(),
            next_out: s.next_out(),
            next_in: s.next_in(),
            skew_ms: s.last_skew_ms(),
            pending_output: s.has_pending_output(),
            journal_refused: s.puts_refused(),
            resend_beyond: s.resend_beyond_journal(),
        }
    }
}

/// The engine-wide numbers a snapshot carries.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Numbers {
    pub(crate) healthy: bool,
    pub(crate) connections: u64,
    pub(crate) logged_on: u64,
    pub(crate) truncated: bool,
    pub(crate) refused: u64,
    pub(crate) unframeable: u64,
    pub(crate) sources_missing: u64,
    pub(crate) log_lost: u64,
    /// `(used, capacity)`, bytes. `None`: nothing reported a ring.
    pub(crate) ring: Option<(u64, u64)>,
    /// `(used, capacity)`, slots. `None`: no front door reported them.
    pub(crate) presession: Option<(u64, u64)>,
}

impl Numbers {
    pub(crate) fn of(s: &Snapshot) -> Self {
        let n = |v: usize| u64::try_from(v).unwrap_or(u64::MAX);
        Self {
            healthy: s.healthy(),
            connections: n(s.connections()),
            logged_on: n(s.sessions().iter().filter(|x| x.logged_on()).count()),
            truncated: s.truncated(),
            refused: n(s.refused_connections()),
            unframeable: n(s.unframeable_prelogon()),
            sources_missing: n(s.sources_missing()),
            log_lost: s.log_lost(),
            ring: s.ring_to_app().map(|o| (n(o.used()), n(o.capacity()))),
            presession: s.presession_slots().map(|o| (n(o.used()), n(o.capacity()))),
        }
    }
}

/// Event counts, by `kind` and by `reason`. Fixed arrays: the label sets are.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Counts {
    pub(crate) kinds: [u64; EVENT_KINDS.len()],
    pub(crate) reasons: [u64; REASONS],
}

impl Default for Counts {
    fn default() -> Self {
        Self {
            kinds: [0; EVENT_KINDS.len()],
            reasons: [0; REASONS],
        }
    }
}

/// One engine, as a scrape prints it.
#[derive(Debug, Clone, Copy)]
pub(crate) struct EngineView<'a> {
    pub(crate) name: &'a str,
    pub(crate) published: u64,
    pub(crate) events_lost: u64,
    /// Milliseconds since the exporter last saw `published` move; `None`
    /// when it never has.
    pub(crate) age_ms: Option<u64>,
    /// `None` until the engine has published a snapshot.
    pub(crate) numbers: Option<Numbers>,
    pub(crate) sessions: &'a [SessionView],
    /// `None` unless the exporter owns the event stream.
    pub(crate) events: Option<&'a Counts>,
}

/// Anything the encoder can print as one engine. The exporter's per-engine
/// state is one; an [`EngineView`] built by a test is another.
pub(crate) trait View {
    fn view(&self) -> EngineView<'_>;
}

impl View for EngineView<'_> {
    fn view(&self) -> EngineView<'_> {
        *self
    }
}

/// The exporter's own two counters.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct ExporterView {
    pub(crate) scrapes: u64,
    pub(crate) bad_requests: u64,
}

/// The reserved buffer was too small. Never a reallocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Overflow;

/// A value, as one sample prints it.
#[derive(Debug, Clone, Copy)]
enum Value {
    Int(u64),
    /// Milliseconds, signed, printed as seconds.
    Millis(i64),
    /// Milliseconds, unsigned, printed as seconds.
    UMillis(u64),
}

/// The labels after `engine`, if any.
#[derive(Debug, Clone, Copy)]
enum Extra {
    None,
    Conn(u64),
    Kind(&'static str),
    Reason(&'static str),
}

const fn int(b: bool) -> Value {
    Value::Int(if b { 1 } else { 0 })
}

/// Every sample `series` has for engine `e`, in order. **The one place that
/// decides what is printed and what is left out**, used both to ask whether a
/// family has any sample at all and to print it.
fn each_sample<F: FnMut(Extra, Value)>(series: &Series, e: &EngineView<'_>, mut f: F) {
    let numbers = e.numbers;
    let engine = |f: &mut F, v: Option<Value>| {
        if let Some(v) = v {
            f(Extra::None, v);
        }
    };
    match series.source {
        Source::SnapshotAvailable => engine(&mut f, Some(int(numbers.is_some()))),
        Source::SnapshotAge => engine(&mut f, e.age_ms.map(Value::UMillis)),
        Source::Published => engine(&mut f, Some(Value::Int(e.published))),
        Source::EventsLost => engine(&mut f, Some(Value::Int(e.events_lost))),
        Source::Healthy => engine(&mut f, numbers.map(|n| int(n.healthy))),
        Source::Connections => engine(&mut f, numbers.map(|n| Value::Int(n.connections))),
        Source::SessionsLoggedOn => engine(&mut f, numbers.map(|n| Value::Int(n.logged_on))),
        Source::Truncated => engine(&mut f, numbers.map(|n| int(n.truncated))),
        Source::Refused => engine(&mut f, numbers.map(|n| Value::Int(n.refused))),
        Source::Unframeable => engine(&mut f, numbers.map(|n| Value::Int(n.unframeable))),
        Source::SourcesMissing => engine(&mut f, numbers.map(|n| Value::Int(n.sources_missing))),
        Source::LogLost => engine(&mut f, numbers.map(|n| Value::Int(n.log_lost))),
        Source::RingUsed => engine(
            &mut f,
            numbers.and_then(|n| n.ring).map(|r| Value::Int(r.0)),
        ),
        Source::RingCapacity => {
            engine(
                &mut f,
                numbers.and_then(|n| n.ring).map(|r| Value::Int(r.1)),
            );
        }
        Source::PresessionUsed => {
            engine(
                &mut f,
                numbers.and_then(|n| n.presession).map(|r| Value::Int(r.0)),
            );
        }
        Source::PresessionCapacity => {
            engine(
                &mut f,
                numbers.and_then(|n| n.presession).map(|r| Value::Int(r.1)),
            );
        }
        Source::SessionLoggedOn
        | Source::SessionNextOut
        | Source::SessionNextIn
        | Source::SessionSkew
        | Source::SessionPendingOutput
        | Source::SessionJournalRefused
        | Source::SessionResendBeyond => {
            if numbers.is_none() {
                return;
            }
            for s in e.sessions {
                let v = match series.source {
                    Source::SessionLoggedOn => Some(int(s.logged_on)),
                    Source::SessionNextOut => Some(Value::Int(u64::from(s.next_out))),
                    Source::SessionNextIn => Some(Value::Int(u64::from(s.next_in))),
                    Source::SessionSkew => s.skew_ms.map(Value::Millis),
                    Source::SessionPendingOutput => Some(int(s.pending_output)),
                    Source::SessionJournalRefused => Some(Value::Int(u64::from(s.journal_refused))),
                    _ => Some(Value::Int(u64::from(s.resend_beyond))),
                };
                if let Some(v) = v {
                    f(Extra::Conn(s.conn), v);
                }
            }
        }
        Source::Events => {
            if let Some(c) = e.events {
                for (label, n) in EVENT_KINDS.iter().zip(c.kinds) {
                    f(Extra::Kind(label), Value::Int(n));
                }
            }
        }
        Source::SessionEnds => {
            if let Some(c) = e.events {
                for (i, n) in c.reasons.iter().enumerate() {
                    f(Extra::Reason(series::reason_label(i)), Value::Int(*n));
                }
            }
        }
        // Not per engine; printed once, by `encode`.
        Source::Scrapes | Source::BadRequests => {}
    }
}

/// Print every series for every engine into `out`, which is cleared first.
///
/// A family with no sample anywhere is left out whole — no `# HELP`, no
/// `# TYPE` — so a scrape never carries a family with nothing in it.
pub(crate) fn encode<V: View>(
    out: &mut Vec<u8>,
    engines: &[V],
    exporter: ExporterView,
) -> Result<(), Overflow> {
    out.clear();
    let mut o = Out { buf: out };
    for series in ALL {
        if series.scope == Scope::Exporter {
            let v = match series.source {
                Source::Scrapes => exporter.scrapes,
                _ => exporter.bad_requests,
            };
            o.header(series)?;
            o.put(series.name().as_bytes())?;
            o.put(b" ")?;
            o.value(Value::Int(v))?;
            o.put(b"\n")?;
            continue;
        }
        let mut any = false;
        for e in engines {
            each_sample(series, &e.view(), |_, _| any = true);
        }
        if !any {
            continue;
        }
        o.header(series)?;
        for e in engines {
            let view = e.view();
            let mut result = Ok(());
            each_sample(series, &view, |extra, v| {
                if result.is_ok() {
                    result = o.sample(series.name(), view.name, extra, v);
                }
            });
            result?;
        }
    }
    Ok(())
}

/// An upper bound on the bytes [`encode`] can write for engines named `names`,
/// every one at [`MAX_SESSIONS`] sessions and every value at its longest.
///
/// **Worked out from the series table, not measured**: a measured size is only
/// as long as the values the measurement happened to see. Every engine name is
/// counted twice over, because escaping can at most double it.
pub(crate) fn capacity_for<'a, I: IntoIterator<Item = &'a str>>(names: I, events: bool) -> usize {
    let max_kind = EVENT_KINDS.iter().map(|k| k.len()).max().unwrap_or(0);
    let max_reason = (0..REASONS)
        .map(|i| series::reason_label(i).len())
        .max()
        .unwrap_or(0);
    let names: (usize, usize) = names
        .into_iter()
        .fold((0, 0), |(n, len), name| (n + 1, len + 2 * name.len()));
    let (engines, escaped_names) = names;

    let mut total = 0usize;
    for s in ALL {
        let name = s.name().len();
        // "# HELP <name> <help>\n# TYPE <name> <type>\n"
        total += 7 + name + 1 + s.help().len() + 1 + 7 + name + 1 + s.kind().as_str().len() + 1;
        // "<name>" … " <value>\n"
        let tail = name + 1 + VALUE_MAX + 1;
        // `{engine="` … `"` … `}`
        let engine_label = 9 + 1 + 1;
        let (per_engine, extra) = match s.scope {
            Scope::Exporter => {
                total += tail;
                continue;
            }
            Scope::Engine => (1, 0),
            Scope::Session => (MAX_SESSIONS, 7 + 20 + 1),
            Scope::EventKind if events => (EVENT_KINDS.len(), 7 + max_kind + 1),
            Scope::Reason if events => (REASONS, 9 + max_reason + 1),
            Scope::EventKind | Scope::Reason => (0, 0),
        };
        total += per_engine * (engines * (tail + engine_label + extra) + escaped_names);
    }
    total
}

/// A writer over a reserved `Vec` that refuses to grow it.
struct Out<'a> {
    buf: &'a mut Vec<u8>,
}

impl Out<'_> {
    fn put(&mut self, bytes: &[u8]) -> Result<(), Overflow> {
        if self.buf.capacity() - self.buf.len() < bytes.len() {
            return Err(Overflow);
        }
        self.buf.extend_from_slice(bytes);
        Ok(())
    }

    fn header(&mut self, s: &Series) -> Result<(), Overflow> {
        self.put(b"# HELP ")?;
        self.put(s.name().as_bytes())?;
        self.put(b" ")?;
        self.put(s.help().as_bytes())?;
        self.put(b"\n# TYPE ")?;
        self.put(s.name().as_bytes())?;
        self.put(b" ")?;
        self.put(s.kind().as_str().as_bytes())?;
        self.put(b"\n")
    }

    fn sample(&mut self, name: &str, engine: &str, extra: Extra, v: Value) -> Result<(), Overflow> {
        self.put(name.as_bytes())?;
        self.put(b"{engine=\"")?;
        self.escaped(engine)?;
        self.put(b"\"")?;
        match extra {
            Extra::None => {}
            Extra::Conn(id) => {
                self.put(b",conn=\"")?;
                self.uint(id)?;
                self.put(b"\"")?;
            }
            Extra::Kind(k) => {
                self.put(b",kind=\"")?;
                self.put(k.as_bytes())?;
                self.put(b"\"")?;
            }
            Extra::Reason(r) => {
                self.put(b",reason=\"")?;
                self.put(r.as_bytes())?;
                self.put(b"\"")?;
            }
        }
        self.put(b"} ")?;
        self.value(v)?;
        self.put(b"\n")
    }

    /// A label value, escaped as the text format requires: `\` `"` and a
    /// line feed.
    fn escaped(&mut self, s: &str) -> Result<(), Overflow> {
        for b in s.bytes() {
            match b {
                b'\\' => self.put(b"\\\\")?,
                b'"' => self.put(b"\\\"")?,
                b'\n' => self.put(b"\\n")?,
                _ => self.put(&[b])?,
            }
        }
        Ok(())
    }

    fn value(&mut self, v: Value) -> Result<(), Overflow> {
        match v {
            Value::Int(n) => self.uint(n),
            Value::Millis(ms) => {
                if ms < 0 {
                    self.put(b"-")?;
                }
                self.seconds(ms.unsigned_abs())
            }
            Value::UMillis(ms) => self.seconds(ms),
        }
    }

    /// `ms` as seconds with exactly three decimals.
    fn seconds(&mut self, ms: u64) -> Result<(), Overflow> {
        self.uint(ms / 1000)?;
        let frac = ms % 1000;
        let digits = [
            b'.',
            b'0' + (frac / 100) as u8,
            b'0' + (frac / 10 % 10) as u8,
            b'0' + (frac % 10) as u8,
        ];
        self.put(&digits)
    }

    fn uint(&mut self, mut n: u64) -> Result<(), Overflow> {
        let mut digits = [0u8; 20];
        let mut at = digits.len();
        loop {
            at -= 1;
            if let Some(d) = digits.get_mut(at) {
                *d = b'0' + (n % 10) as u8;
            }
            n /= 10;
            if n == 0 || at == 0 {
                break;
            }
        }
        self.put(digits.get(at..).unwrap_or(&[]))
    }
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::indexing_slicing
    )]

    use super::*;

    fn worst_session(i: u64) -> SessionView {
        SessionView {
            conn: u64::MAX - i,
            logged_on: true,
            next_out: u32::MAX,
            next_in: u32::MAX,
            skew_ms: Some(i64::MIN),
            pending_output: true,
            journal_refused: u32::MAX,
            resend_beyond: u32::MAX,
        }
    }

    fn worst_numbers() -> Numbers {
        Numbers {
            healthy: true,
            connections: u64::MAX,
            logged_on: u64::MAX,
            truncated: true,
            refused: u64::MAX,
            unframeable: u64::MAX,
            sources_missing: u64::MAX,
            log_lost: u64::MAX,
            ring: Some((u64::MAX, u64::MAX)),
            presession: Some((u64::MAX, u64::MAX)),
        }
    }

    fn text(buf: &[u8]) -> &str {
        core::str::from_utf8(buf).expect("the format is text")
    }

    #[test]
    fn a_full_snapshot_fits_the_reserved_buffer() {
        let sessions: Vec<SessionView> = (0..MAX_SESSIONS as u64).map(worst_session).collect();
        let counts = Counts {
            kinds: [u64::MAX; EVENT_KINDS.len()],
            reasons: [u64::MAX; REASONS],
        };
        // Every byte of each name needs escaping, so each prints at double length.
        let names = ["\"\"\"\"\"\"\"\"", "\\\\\\\\\\\\\\\\\\\\\\\\"];
        let engines: Vec<EngineView<'_>> = names
            .iter()
            .map(|name| EngineView {
                name,
                published: u64::MAX,
                events_lost: u64::MAX,
                age_ms: Some(u64::MAX),
                numbers: Some(worst_numbers()),
                sessions: &sessions,
                events: Some(&counts),
            })
            .collect();
        let exporter = ExporterView {
            scrapes: u64::MAX,
            bad_requests: u64::MAX,
        };

        let capacity = capacity_for(names, true);
        let mut buf = Vec::with_capacity(capacity);
        let reserved = buf.capacity();
        encode(&mut buf, &engines, exporter).expect("the worst case fits");
        assert_eq!(
            buf.capacity(),
            reserved,
            "the buffer was never grown: a grown buffer is an allocation per scrape"
        );
        let body = text(&buf);
        assert_eq!(
            body.matches("fixbolt_session_next_out_seq_num{").count(),
            2 * MAX_SESSIONS,
            "every session of both engines was printed"
        );
        assert!(
            body.contains("-9223372036854775.808\n"),
            "the longest value was printed"
        );
        // And twice over is not an accident of a loose bound: it is within a
        // small factor of what was written.
        assert!(
            buf.len() * 2 > capacity,
            "the bound is {capacity} for {} written — a bound this loose hides a mistake",
            buf.len()
        );
    }

    #[test]
    fn a_buffer_too_small_is_refused_not_grown() {
        let empty: [EngineView<'_>; 0] = [];
        let mut buf = Vec::with_capacity(16);
        let reserved = buf.capacity();
        assert_eq!(
            encode(&mut buf, &empty, ExporterView::default()),
            Err(Overflow)
        );
        assert_eq!(buf.capacity(), reserved);
    }

    #[test]
    fn label_values_are_escaped() {
        let view = EngineView {
            name: "a\"b\\c\nd",
            published: 7,
            events_lost: 0,
            age_ms: None,
            numbers: None,
            sessions: &[],
            events: None,
        };
        let mut buf = Vec::with_capacity(capacity_for([view.name], false));
        encode(&mut buf, &[view], ExporterView::default()).expect("fits");
        assert!(
            text(&buf)
                .contains("fixbolt_snapshots_published_total{engine=\"a\\\"b\\\\c\\nd\"} 7\n"),
            "{}",
            text(&buf)
        );
    }

    #[test]
    fn skew_is_printed_as_seconds_without_a_float() {
        let cases: [(i64, &str); 6] = [
            (0, "0.000"),
            (7, "0.007"),
            (-1_500, "-1.500"),
            (120_000, "120.000"),
            (-999, "-0.999"),
            (i64::MIN, "-9223372036854775.808"),
        ];
        for (ms, want) in cases {
            let mut buf = Vec::with_capacity(32);
            Out { buf: &mut buf }
                .value(Value::Millis(ms))
                .expect("fits");
            assert_eq!(text(&buf), want, "{ms} ms");
        }
        let mut buf = Vec::with_capacity(32);
        Out { buf: &mut buf }
            .value(Value::UMillis(u64::MAX))
            .expect("fits");
        assert_eq!(text(&buf), "18446744073709551.615");
        let mut buf = Vec::with_capacity(32);
        Out { buf: &mut buf }.value(Value::Int(0)).expect("fits");
        assert_eq!(text(&buf), "0");
    }

    #[test]
    fn a_family_with_no_sample_prints_no_header() {
        let view = EngineView {
            name: "a",
            published: 0,
            events_lost: 0,
            age_ms: None,
            numbers: None,
            sessions: &[],
            events: None,
        };
        let mut buf = Vec::with_capacity(capacity_for(["a"], false));
        encode(&mut buf, &[view], ExporterView::default()).expect("fits");
        let body = text(&buf);
        assert!(
            body.contains("# TYPE fixbolt_snapshot_available gauge\n"),
            "{body}"
        );
        assert!(!body.contains("fixbolt_connections"), "{body}");
        assert!(!body.contains("fixbolt_snapshot_age_seconds"), "{body}");
        for s in ALL {
            assert!(
                body.matches(&format!("# TYPE {} ", s.name())).count() <= 1,
                "{}: one TYPE line at most",
                s.name()
            );
        }
    }
}
