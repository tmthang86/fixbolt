//! `SendingTime`, formatted once a minute instead of once a message.
//!
//! Naive formatting costs 50-100 ns — as much as parsing a whole
//! `NewOrderSingle` (139 ns, `reference/measured-costs.md`). The first 15 bytes
//! of `YYYYMMDD-HH:MM:SS.sss` change once a minute; only `SS.sss` changes per
//! message, and that is six digits and a dot.
//!
//! No clock here. `codec` is `no_std` and takes time as an argument, the same
//! way the session layer takes it as `Input::Tick` (D1).

/// The full `YYYYMMDD-HH:MM:SS.sss` form.
pub const TIMESTAMP_LEN: usize = 21;

/// Caches the minute prefix of a UTC timestamp.
pub struct TimestampCache {
    buf: [u8; TIMESTAMP_LEN],
    /// Minutes since the Unix epoch that `buf[..15]` was built for.
    /// `u64::MAX` means nothing has been built yet.
    minute: u64,
}

impl Default for TimestampCache {
    fn default() -> Self {
        Self::new()
    }
}

impl TimestampCache {
    /// An empty cache. The first call to [`Self::format`] fills it.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            buf: [b'0'; TIMESTAMP_LEN],
            minute: u64::MAX,
        }
    }

    /// UTC `YYYYMMDD-HH:MM:SS.sss` for `millis` since the Unix epoch.
    ///
    /// Rebuilds the first 15 bytes only when the minute changes, which also
    /// covers the hour, day, month and year rolling over — they cannot change
    /// without the minute changing.
    #[inline]
    pub fn format(&mut self, millis: u64) -> &[u8; TIMESTAMP_LEN] {
        let secs = millis / 1_000;
        let minute = secs / 60;

        if minute != self.minute {
            self.rebuild_prefix(minute);
            self.minute = minute;
        }

        let s = (secs % 60) as u32;
        let ms = (millis % 1_000) as u32;
        self.buf[15] = b'0' + (s / 10) as u8;
        self.buf[16] = b'0' + (s % 10) as u8;
        self.buf[17] = b'.';
        self.buf[18] = b'0' + (ms / 100) as u8;
        self.buf[19] = b'0' + ((ms / 10) % 10) as u8;
        self.buf[20] = b'0' + (ms % 10) as u8;
        &self.buf
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
