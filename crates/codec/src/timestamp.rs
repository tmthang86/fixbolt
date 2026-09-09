//! `SendingTime`, formatted once a minute instead of once a message.
//!
//! Naive formatting costs 50-100 ns — as much as parsing a whole
//! `NewOrderSingle` (139 ns, `reference/measured-costs.md`). The first 15 bytes
//! of `YYYYMMDD-HH:MM:SS.sss` change once a minute; only `SS.sss` changes per
//! message, and that is six digits and a dot.
//!
//! No clock here. `codec` is `no_std` and takes time as an argument, the same
//! way the session layer takes it as `Input::Tick` (D1).

// `[measured 2026-09-08]` 6 `clippy::indexing_slicing` sites in this file on
// the day `indexing_slicing = "deny"` went into the workspace lints. Debt, not
// permission: `scripts/check-indexing-debt.sh` counts these with `--force-warn`,
// which overrides this line, and its ceiling only ever goes down. STATUS.md
// item 55.
#![allow(clippy::indexing_slicing)]

/// The `YYYYMMDD-HH:MM:SS.sss` form — [`Precision::Millis`], and the width this
/// engine wrote at every configuration until ADR-0057.
pub const TIMESTAMP_LEN: usize = 21;

/// The widest form this cache can write: [`Precision::Nanos`].
///
/// A caller that keeps its own copy of a stamp sizes the buffer with this and
/// remembers the length, because the width is now the configuration's and no
/// longer a constant.
pub const TIMESTAMP_MAX_LEN: usize = 27;

/// How many fractional digits a `UTCTimestamp` carries on the way **out**.
///
/// FIX 4.4 documents `.sss`; FIX 5.0 SP2 EP allows `.ssssss` and `.sssssssss`,
/// and MiFID II RTS 25 requires a clock synchronised to the microsecond for
/// high-frequency trading, so a European venue asks for six digits and means
/// it.
///
/// **Seconds — no fractional part at all — is deliberately absent.** QuickFIX
/// has it and nobody has asked for it here; it is named in the plan's
/// *out of scope* rather than forgotten.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Precision {
    /// `.sss` — 21 bytes. The default, and QuickFIX C++'s default too.
    #[default]
    Millis,
    /// `.ssssss` — 24 bytes.
    Micros,
    /// `.sssssssss` — 27 bytes.
    Nanos,
}

impl Precision {
    /// Digits after the decimal point: 3, 6 or 9.
    #[must_use]
    pub const fn fractional_digits(self) -> usize {
        match self {
            Self::Millis => 3,
            Self::Micros => 6,
            Self::Nanos => 9,
        }
    }

    /// Bytes a stamp at this precision occupies: 21, 24 or 27.
    ///
    /// **`bytes` and not `len`**: this is a width on the wire, not the length
    /// of a container, and clippy is right that a type with a `len` and no
    /// `is_empty` reads like one.
    #[must_use]
    pub const fn bytes(self) -> usize {
        18 + self.fractional_digits()
    }

    /// The precision that writes `digits` fractional digits, or `None`.
    ///
    /// **`None` is a configuration error and never a clamp.** QuickFIX C++'s
    /// `TimestampPrecision` takes any integer 0–9, so a `.cfg` shared between
    /// the two ends can name a width this engine does not write; answering
    /// with the nearest one it does would put a number on the wire that the
    /// operator did not choose. ADR-0057 open question 3.
    #[must_use]
    pub const fn from_fractional_digits(digits: u32) -> Option<Self> {
        match digits {
            3 => Some(Self::Millis),
            6 => Some(Self::Micros),
            9 => Some(Self::Nanos),
            _ => None,
        }
    }

    /// The coarser of the two.
    ///
    /// **This is how ADR-0057's rejection of a padded stamp is held by the
    /// type rather than by a paragraph.** A caller that supplies only
    /// milliseconds cannot be made to publish six digits, three of which would
    /// be zeroes it invented — `.123000` to a venue measuring divergence is
    /// worse than `.123`, which claims millisecond resolution truthfully.
    #[must_use]
    pub const fn coarser(self, other: Self) -> Self {
        if (self.fractional_digits() as u32) <= (other.fractional_digits() as u32) {
            self
        } else {
            other
        }
    }
}

/// Caches the minute prefix of a UTC timestamp.
pub struct TimestampCache {
    buf: [u8; TIMESTAMP_MAX_LEN],
    /// Minutes since the Unix epoch that `buf[..15]` was built for.
    /// `u64::MAX` means nothing has been built yet.
    minute: u64,
    /// How wide a stamp this cache writes. Fixed for the cache's life.
    ///
    /// **A field and not a `const` parameter**, decided in the plan's Sửa 2:
    /// the precision arrives from a configuration file at run time, so a const
    /// generic would force the session and the engine to be monomorphised
    /// three ways behind a runtime `match`. The branch it costs instead takes
    /// the same arm on every message of a session's life.
    ///
    /// **What that costs, measured rather than argued** — ADR-0057 decision 5
    /// made this the constraint. `[measured 2026-09-09]`, Intel Xeon @ 2.80GHz,
    /// a shared cloud VM and **not** the §9 desktop, three runs of
    /// `benches/serialize.rs` per arm, arm `SendingTime from the cache`:
    ///
    /// | | ns/op |
    /// |---|---|
    /// | before this change, same machine, same run conditions | **4.7** |
    /// | returning a slice instead of `&[u8; 21]`, no precision branch | **4.8** |
    /// | as shipped | **5.4** |
    ///
    /// So **+0.7 ns, of which the runtime branch is 0.6 and the slice return is
    /// 0.1**, one variable at a time. The published 4.9 ns baseline
    /// (`benches/baselines.tsv:141`) is an AMD Ryzen 7 3700X figure and **was
    /// not re-measured**; these numbers stand beside it, not against it.
    precision: Precision,
}

impl Default for TimestampCache {
    fn default() -> Self {
        Self::new()
    }
}

impl TimestampCache {
    /// An empty cache writing milliseconds. The first call to [`Self::format`]
    /// fills it.
    #[must_use]
    pub const fn new() -> Self {
        Self::with_precision(Precision::Millis)
    }

    /// An empty cache writing `precision`.
    #[must_use]
    pub const fn with_precision(precision: Precision) -> Self {
        Self {
            buf: [b'0'; TIMESTAMP_MAX_LEN],
            minute: u64::MAX,
            precision,
        }
    }

    /// What width this cache writes.
    #[must_use]
    pub const fn precision(&self) -> Precision {
        self.precision
    }

    /// UTC `YYYYMMDD-HH:MM:SS.` plus this cache's fractional digits, for
    /// `millis` since the Unix epoch and `sub_ms_nanos` nanoseconds within that
    /// millisecond.
    ///
    /// Rebuilds the first 15 bytes only when the minute changes, which also
    /// covers the hour, day, month and year rolling over — they cannot change
    /// without the minute changing.
    ///
    /// **`sub_ms_nanos` is ignored at [`Precision::Millis`]**, which is what
    /// lets a caller with no sub-millisecond clock pass `0` without saying
    /// anything untrue. Anything at or above 1 000 000 is a caller error and is
    /// clamped to the last nanosecond of the millisecond rather than carried
    /// into the next second, because a stamp one millisecond in the future
    /// fails a skew check at the far end and a truncated one does not.
    #[inline]
    pub fn format(&mut self, millis: u64, sub_ms_nanos: u32) -> &[u8] {
        let secs = millis / 1_000;
        let minute = secs / 60;

        if minute != self.minute {
            self.rebuild_prefix(minute);
            self.minute = minute;
        }

        let s = (secs % 60) as u32;
        let ms = (millis % 1_000) as u32;

        // **The millisecond arm is straight-line, and that is a measurement and
        // not a preference.** `[measured 2026-09-09]` folding it into the
        // general loop below cost **4.7 ns -> 6.4 ns**, three runs each, one
        // machine, one session — a 36% regression on the default precision, for
        // digits the default never writes. `benches/serialize.rs`, arm
        // `SendingTime from the cache`.
        //
        // **Constant subscripts here, `get_mut` below, and the difference is
        // not style.** A constant index into a fixed-size array cannot panic
        // and `clippy::indexing_slicing` does not flag it; the general arm's
        // index is *computed*, which is a real site and a real panic in a
        // library crate (non-negotiable 7). `[measured 2026-09-09]` the first
        // version of this function used a computed index and a computed slice
        // and took `scripts/check-indexing-debt.sh` from **184 to 186** — the
        // ratchet caught it, and the two forms measure the same, so the safe
        // one is free here.
        if matches!(self.precision, Precision::Millis) {
            self.buf[15] = b'0' + (s / 10) as u8;
            self.buf[16] = b'0' + (s % 10) as u8;
            self.buf[17] = b'.';
            self.buf[18] = b'0' + (ms / 100) as u8;
            self.buf[19] = b'0' + ((ms / 10) % 10) as u8;
            self.buf[20] = b'0' + (ms % 10) as u8;
            return &self.buf[..TIMESTAMP_LEN];
        }

        // Everything wider. The fraction is one integer at this precision,
        // written most-significant digit first: `millis % 1000` is its leading
        // three digits and the rest comes from the nanoseconds inside that
        // millisecond.
        let nanos = if sub_ms_nanos >= 1_000_000 {
            999_999
        } else {
            sub_ms_nanos
        };
        let end = 18 + self.precision.fractional_digits();
        let mut fraction = match self.precision {
            Precision::Millis => ms,
            Precision::Micros => ms * 1_000 + nanos / 1_000,
            Precision::Nanos => ms * 1_000_000 + nanos,
        };
        if let Some([s0, s1, dot, frac @ ..]) = self.buf.get_mut(15..end) {
            *s0 = b'0' + (s / 10) as u8;
            *s1 = b'0' + (s % 10) as u8;
            *dot = b'.';
            for slot in frac.iter_mut().rev() {
                *slot = b'0' + (fraction % 10) as u8;
                fraction /= 10;
            }
        }
        self.buf.get(..end).unwrap_or(&self.buf)
    }

    fn rebuild_prefix(&mut self, minute: u64) {
        let days = (minute / 1_440) as i64;
        let mins_of_day = (minute % 1_440) as u32;
        let (y, m, d) = civil_from_days(days);

        write4(&mut self.buf[0..4], y);
        write2(&mut self.buf[4..6], m);
        write2(&mut self.buf[6..8], d);
        self.buf[8] = b'-';
        write2(&mut self.buf[9..11], mins_of_day / 60);
        self.buf[11] = b':';
        write2(&mut self.buf[12..14], mins_of_day % 60);
        self.buf[14] = b':';
    }
}

fn write2(out: &mut [u8], v: u32) {
    out[0] = b'0' + (v / 10) as u8;
    out[1] = b'0' + (v % 10) as u8;
}

fn write4(out: &mut [u8], v: u32) {
    out[0] = b'0' + (v / 1000) as u8;
    out[1] = b'0' + ((v / 100) % 10) as u8;
    out[2] = b'0' + ((v / 10) % 10) as u8;
    out[3] = b'0' + (v % 10) as u8;
}

/// Days since 1970-01-01 to a civil date. Howard Hinnant's `civil_from_days`,
/// which is exact for the whole range and uses no division by a non-constant.
fn civil_from_days(z: i64) -> (u32, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = doy - (153 * mp + 2) / 5 + 1; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 }; // [1, 12]
    let y = if m <= 2 { y + 1 } else { y };
    (y as u32, m as u32, d as u32)
}
