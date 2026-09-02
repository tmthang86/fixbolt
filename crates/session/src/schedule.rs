//! When a session is open, and when both sides start again at `34=1`.
//!
//! A FIX session does not run forever. It opens at 08:00, closes at 17:00, and
//! the next morning **both ends begin at `34=1`**. That is not an operational
//! nicety — it is the protocol: the two sides must *agree* on when the counting
//! restarts, and an end that gets it wrong spends the next morning in a
//! sequence-number dispute.
//!
//! # The calendar stops at this module's edge
//!
//! A [`Schedule`] is **arithmetic on the millisecond timeline, expressed in
//! UTC**. It holds no zone name, reads no database and knows nothing about
//! daylight saving.
//!
//! That is deliberate, and it is the decision this module exists to enforce.
//! Resolving *"17:00 America/New_York"* needs an IANA database: a dependency,
//! which allocates, in the layer non-negotiable 2 calls **pure**. Prior art
//! splits on exactly this line — QuickFIX puts the zone inside the engine and
//! its own implementation does not handle DST explicitly, while Artio keeps the
//! whole schedule outside the core and exposes only `resetSequenceNumber()`.
//! This sits between them: **the session shape is in, the Gregorian calendar is
//! out.**
//!
//! A caller wanting local time resolves it with their own zone library and
//! **rebuilds the `Schedule` when the offset changes**. [`Schedule::with_utc_offset_ms`]
//! covers the fixed-offset case; it is not, and must not be read as, DST
//! support. `GUIDE.md` says so as a warning rather than as a feature.
//!
//! # Why the reset is a comparison and not an alarm
//!
//! [`Schedule::same_session`] answers *do these two instants fall in the same
//! interval*, which is how QuickFIX's `isSameSession` decides it too. An engine
//! that slept through midnight, or a process that started at 06:00, gets no
//! alarm — it has only the last instant it remembers and now. **The moment a
//! reset matters most is the moment nobody was running to hear a bell.**
//!
//! # What the corpus cannot see
//!
//! All 59 acceptance definitions run inside one interval, so none of them can
//! tell a working schedule from one that is never consulted.
//! `crates/session/tests/schedule.rs` is the only thing holding any of this.

/// Milliseconds in a day, on the timeline `Input::Tick` carries.
const DAY_MS: u64 = 86_400_000;
/// Milliseconds in a week.
const WEEK_MS: u64 = 7 * DAY_MS;
/// The last second of a day. `86_400` is the next day's first, not this day's
/// last, and accepting it is how an off-by-one becomes a schedule that is
/// briefly open when it should be shut.
const LAST_SECOND_OF_DAY: u32 = 86_399;

/// A day of the week, Monday first.
///
/// Monday first because ISO-8601 numbers it that way and because a trading week
/// starting on Sunday is a property of some venues rather than of the calendar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Weekday {
    Monday,
    Tuesday,
    Wednesday,
    Thursday,
    Friday,
    Saturday,
    Sunday,
}

impl Weekday {
    /// Monday is 0.
    const fn index(self) -> u8 {
        self as u8
    }

    /// The weekday `days` whole days after 0000-01-01.
    ///
    /// **The epoch here is 0000-01-01, not 1970-01-01** — D13, so that every
    /// timestamp FIX can write is a non-negative `u64`. The offset that maps
    /// one to the other is **derived by a test, not recalled**:
    /// [`tests::the_weekday_offset_is_derived_not_recalled`] asks this function
    /// for 1970-01-01 and requires a Thursday.
    ///
    /// `[measured 2026-09-02]` **that one test is the only thing holding this
    /// number.** Changing `+ 5` to `+ 6` leaves every weekday case in
    /// `tests/schedule.rs` green, because those tests *find* a Monday by
    /// probing seven days rather than naming one — deliberately, so they do not
    /// depend on which day the corpus happens to fall on, and the price of that
    /// independence is that they cannot see this constant at all.
    const fn from_days_since_year_zero(days: u64) -> Self {
        match (days + 5) % 7 {
            0 => Self::Monday,
            1 => Self::Tuesday,
            2 => Self::Wednesday,
            3 => Self::Thursday,
            4 => Self::Friday,
            5 => Self::Saturday,
            _ => Self::Sunday,
        }
    }
}

/// A set of weekdays, as a seven-bit mask.
///
/// A mask rather than a slice: non-negotiable 1 forbids the allocation, and a
/// `Copy` byte keeps [`Schedule`] `Copy` too.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Weekdays(u8);

impl Weekdays {
    /// Every day. What [`Schedule::daily`] uses unless told otherwise.
    pub const ALL: Self = Self(0b0111_1111);
    /// Monday to Friday.
    pub const WEEKDAYS: Self = Self(0b0001_1111);
    /// Saturday and Sunday.
    pub const WEEKEND: Self = Self(0b0110_0000);
    /// No days at all. A schedule holding this is open never, which is why
    /// [`Schedule::with_weekdays`] refuses it.
    pub const NONE: Self = Self(0);

    /// Add one day.
    #[must_use]
    pub const fn and(self, day: Weekday) -> Self {
        Self(self.0 | (1 << day.index()))
    }

    /// Just this one day.
    #[must_use]
    pub const fn only(day: Weekday) -> Self {
        Self::NONE.and(day)
    }

    /// Is this day in the set?
    #[must_use]
    pub const fn contains(self, day: Weekday) -> bool {
        self.0 & (1 << day.index()) != 0
    }

    /// Is the set empty?
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }
}

/// One repeating interval on the timeline.
///
/// `open` and `close` are milliseconds into the period. `open > close` means
/// the interval wraps — 22:00 to 06:00 is a real trading session and a naive
/// `open <= t && t < close` is open for none of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Window {
    period_ms: u64,
    open: u64,
    close: u64,
    /// Which weekdays an interval may **start** on. Always [`Weekdays::ALL`]
    /// for a weekly window, whose period already selects.
    days: Weekdays,
}

/// When a session is open, and which instants belong to the same one.
///
/// `Copy`, and small enough to sit in [`crate::Config`] without ceremony.
/// Every constructor returns [`Option`]: a schedule that cannot be honoured is
/// refused where it is written, not discovered at 3 a.m. (non-negotiable 7).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Schedule {
    /// `None` is [`Schedule::always`] — one session with no boundary. It is the
    /// default every existing user gets, so it must be **exactly** neutral.
    window: Option<Window>,
    /// Added to an instant before the window is consulted, so a caller may give
    /// times in a fixed-offset zone. **Not daylight saving.**
    offset_ms: i64,
}

impl Default for Schedule {
    fn default() -> Self {
        Self::always()
    }
}

impl Schedule {
    /// Open forever, one session, no boundary and no reset.
    ///
    /// **The default, and it must change nothing.** Every session built before
    /// this module existed behaves as if it carries this, and the 59 acceptance
    /// definitions run under it —
    /// `tests/schedule.rs::an_always_schedule_is_open_forever_and_never_resets`
    /// is what says so.
    #[must_use]
    pub const fn always() -> Self {
        Self {
            window: None,
            offset_ms: 0,
        }
    }

    /// The same hours every day, as seconds since midnight UTC.
    ///
    /// `open > close` wraps across midnight and is legal: 22:00 to 06:00 is one
    /// session, not two. `None` when either second is not a second of a day, or
    /// when the two are equal — a zero-length interval is open never and a
    /// full-length one is [`Self::always`]; neither is what the caller meant,
    /// so neither is guessed at.
    #[must_use]
    pub const fn daily(open_sod: u32, close_sod: u32) -> Option<Self> {
        if open_sod > LAST_SECOND_OF_DAY || close_sod > LAST_SECOND_OF_DAY {
            return None;
        }
        if open_sod == close_sod {
            return None;
        }
        Some(Self {
            window: Some(Window {
                period_ms: DAY_MS,
                open: open_sod as u64 * 1_000,
                close: close_sod as u64 * 1_000,
                days: Weekdays::ALL,
            }),
            offset_ms: 0,
        })
    }

    /// One interval a week, from a weekday and time to a weekday and time.
    ///
    /// A session that opens Sunday evening and closes Friday evening is one
    /// interval spanning most of the week, and every instant inside it belongs
    /// to the **same** session — so nothing resets on Tuesday night. `None`
    /// under the same rules as [`Self::daily`], plus when both ends name the
    /// same instant of the week.
    #[must_use]
    pub const fn weekly(
        open_day: Weekday,
        open_sod: u32,
        close_day: Weekday,
        close_sod: u32,
    ) -> Option<Self> {
        if open_sod > LAST_SECOND_OF_DAY || close_sod > LAST_SECOND_OF_DAY {
            return None;
        }
        let open = open_day.index() as u64 * DAY_MS + open_sod as u64 * 1_000;
        let close = close_day.index() as u64 * DAY_MS + close_sod as u64 * 1_000;
        if open == close {
            return None;
        }
        Some(Self {
            window: Some(Window {
                period_ms: WEEK_MS,
                open,
                close,
                days: Weekdays::ALL,
            }),
            offset_ms: 0,
        })
    }

    /// Restrict which weekdays a daily interval may **open** on.
    ///
    /// The day an interval *starts* is what counts, so a Friday 22:00 session
    /// closing Saturday 06:00 runs to its end under `WEEKDAYS`. `None` on an
    /// empty set — open never is not a schedule — or on [`Self::always`], which
    /// has no interval to restrict, or on a weekly window, whose period already
    /// selects its days.
    #[must_use]
    pub const fn with_weekdays(mut self, days: Weekdays) -> Option<Self> {
        if days.is_empty() {
            return None;
        }
        match self.window {
            Some(ref mut w) if w.period_ms == DAY_MS => {
                w.days = days;
                Some(self)
            }
            _ => None,
        }
    }

    /// Read the times as belonging to a zone this many milliseconds ahead of
    /// UTC. Positive is east.
    ///
    /// **This is a fixed offset and it is not daylight saving.** A venue on
    /// `America/New_York` is `-5h` in winter and `-4h` in summer; a `Schedule`
    /// built with one of those is **wrong for half the year**, and being wrong
    /// here means resetting sequence numbers at the wrong hour on the two days
    /// a counterparty is least forgiving. Whoever needs that resolves the
    /// offset with a zone library and builds a new `Schedule` when it changes.
    ///
    /// `None` if the offset is not inside ±24 h.
    #[must_use]
    pub const fn with_utc_offset_ms(mut self, offset_ms: i64) -> Option<Self> {
        if offset_ms <= -(DAY_MS as i64) || offset_ms >= DAY_MS as i64 {
            return None;
        }
        self.offset_ms = offset_ms;
        Some(self)
    }

    /// The instant the interval containing `t` began, or `None` if `t` is
    /// inside none.
    ///
    /// The whole type is two lines of arithmetic on top of this one function,
    /// which is why it is the only place the wrap is reasoned about.
    fn session_start(self, t: u64) -> Option<u64> {
        let w = match self.window {
            // `always` is one session that began at the beginning of time.
            None => return Some(0),
            Some(w) => w,
        };
        let local = t.checked_add_signed(self.offset_ms)?;
        let into = local % w.period_ms;
        let base = local - into;

        let start_local = if w.open < w.close {
            (into >= w.open && into < w.close).then_some(base + w.open)?
        } else if into >= w.open {
            // Opened earlier in this period and has not closed yet.
            base + w.open
        } else if into < w.close {
            // Still inside the interval that opened in the previous period.
            (base + w.open).checked_sub(w.period_ms)?
        } else {
            return None;
        };

        if w.period_ms == DAY_MS
            && !w
                .days
                .contains(Weekday::from_days_since_year_zero(start_local / DAY_MS))
        {
            return None;
        }
        start_local.checked_add_signed(-self.offset_ms)
    }

    /// Is a session open at `t`?
    #[must_use]
    pub fn contains(self, t: u64) -> bool {
        self.session_start(t).is_some()
    }

    /// Do `a` and `b` belong to the same session?
    ///
    /// **`false` is the fail-safe answer and it is chosen deliberately.** An
    /// instant inside no interval is not in the same session as anything,
    /// including another such instant — so an engine that cannot place what it
    /// remembers starts a new session and resets, rather than carrying a
    /// sequence number across a boundary it could not see. Resetting when the
    /// counterparty did not is a `Logon` argument; *not* resetting when they
    /// did is a silent divergence that surfaces messages later.
    #[must_use]
    pub fn same_session(self, a: u64, b: u64) -> bool {
        match (self.session_start(a), self.session_start(b)) {
            (Some(x), Some(y)) => x == y,
            _ => false,
        }
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    reason = "a test asserting a constant is not a library call site — same as `clock`"
)]
mod tests {
    use super::{Weekday, Weekdays};

    /// **The one constant here that could be silently wrong.** The epoch is
    /// 0000-01-01 (D13), not 1970-01-01, so the `+ 5` in
    /// `from_days_since_year_zero` is not a number anybody can check by
    /// reading. It is derived here from a date whose weekday is not in dispute.
    ///
    /// Same discipline as `clock::tests::the_epoch_offset_is_derived_not_recalled`.
    #[test]
    fn the_weekday_offset_is_derived_not_recalled() {
        // 1970-01-01 was a Thursday, and `DAYS_YEAR_ZERO_TO_EPOCH` is itself
        // derived rather than remembered — see `clock`.
        let epoch_day = u64::try_from(crate::clock::DAYS_YEAR_ZERO_TO_EPOCH)
            .expect("the epoch offset is a positive number of days");
        assert_eq!(
            Weekday::from_days_since_year_zero(epoch_day),
            Weekday::Thursday,
            "1970-01-01 was a Thursday"
        );
        // And the day before it a Wednesday, so the direction is right too — an
        // offset wrong by a whole week would pass the assertion above.
        assert_eq!(
            Weekday::from_days_since_year_zero(epoch_day - 1),
            Weekday::Wednesday
        );
        assert_eq!(
            Weekday::from_days_since_year_zero(epoch_day + 1),
            Weekday::Friday
        );
    }

    #[test]
    fn a_weekday_set_holds_what_was_put_in_it_and_nothing_else() {
        assert!(Weekdays::WEEKDAYS.contains(Weekday::Monday));
        assert!(Weekdays::WEEKDAYS.contains(Weekday::Friday));
        assert!(!Weekdays::WEEKDAYS.contains(Weekday::Saturday));
        assert!(Weekdays::WEEKEND.contains(Weekday::Sunday));
        assert!(!Weekdays::WEEKEND.contains(Weekday::Friday));
        assert!(Weekdays::NONE.is_empty());
        assert!(!Weekdays::ALL.is_empty());
        assert!(
            Weekdays::only(Weekday::Wednesday).contains(Weekday::Wednesday),
            "and a single day is a set of one"
        );
        assert!(!Weekdays::only(Weekday::Wednesday).contains(Weekday::Thursday));
    }
}
