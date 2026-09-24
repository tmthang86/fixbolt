//! **The soak: `--rate` messages a second for `--seconds`, then every row
//! checked.** The instrument for phase 4 row 4a's kill line (50 000 msg/s ×
//! 60 s with nothing lost), built in row 3. Plan
//! `docs/plans/2026-09-24-p4-sqlite-store.md`, *Chia việc* row 5.
//!
//! ```text
//! cargo run --release -p fixbolt-store-sqlite --example soak -- \
//!     --rate 50000 --seconds 60 --synchronous normal --dir target/soak [--body-bytes 200]
//! ```
//!
//! - A **stand-in engine thread** keeps the pace by spinning on `Instant`
//!   (it never sleeps, as an `hft` engine thread would not) and, per message
//!   `k`, calls `mark_in(k)` then `put(k, body(k))` — two ring records.
//! - A **monitor thread** reads `progress()` and the `-wal` file's size
//!   (`stat`, never an open: ADR-0180 decision 5) every 10 ms.
//! - At the end the store is closed, the database is opened with SQLite and
//!   **every row** is compared with what the generator made.
//!
//! One line, then the exit code:
//!
//! ```text
//! soak: fs … synchronous … rate … seconds … messages … records … unwritten … rows …
//!       mismatched … behind_max_ms … max_batch … wal_bytes_max … db_bytes … pace_missed …
//! ```
//!
//! - `records`: ring records committed (two per message); `unwritten`: the
//!   store's own count of records the database will not have.
//! - `behind_max_ms`: the largest gap, sampled every 10 ms, between the last
//!   message put and the highest one committed, in milliseconds of traffic at
//!   `--rate`.
//! - `max_batch`: the most records committed between two 10 ms samples — an
//!   **upper bound** on the largest single batch (a batch adds its whole count
//!   at once), not the batch itself, which the store does not expose.
//! - `pace_missed`: messages the stand-in engine put more than 1 ms after their
//!   due time.
//!
//! **Exit 1** if `unwritten > 0`, `rows ≠ messages`, `mismatched > 0`, or
//! more than 1 % of messages missed their pace; **exit 2** on a usage error or
//! a `--dir` on `tmpfs`/`ramfs` (read from `/proc/self/mounts`), where a
//! database lives in memory and the run would measure nothing about a disk.
//!
//! **What it is not:** a latency figure. Its numbers are a count and a pass —
//! run on the machine and the grub line row 4a names, with
//! `scripts/check-machine.sh` beside them (non-negotiable 10). The CI step that
//! runs it for 2 s asserts the count, and nothing about speed.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
// An example, not a library crate's source: non-negotiable 7 is about
// `crates/*/src`.
#![allow(clippy::indexing_slicing)]

#[cfg(not(feature = "sqlite"))]
fn main() {}

#[cfg(feature = "sqlite")]
fn main() {
    std::process::exit(soak::run());
}

#[cfg(feature = "sqlite")]
mod soak {
    use std::path::{Path, PathBuf};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use std::time::{Duration, Instant};

    use fixbolt_engine::journal::SLOT_LEN;
    use fixbolt_session::Config;
    use fixbolt_session::journal::Journal;
    use fixbolt_store_sqlite::{SqliteOptions, SqliteStore, Synchronous};

    struct Args {
        rate: u64,
        seconds: u64,
        dir: PathBuf,
        synchronous: Synchronous,
        body_bytes: usize,
    }

    const USAGE: &str = "usage: soak --rate <msg/s> --seconds <s> --dir <path> \
                         [--synchronous normal|full] [--body-bytes <n>]";

    fn args() -> Result<Args, String> {
        let mut a = Args {
            rate: 50_000,
            seconds: 5,
            dir: PathBuf::from("target/soak"),
            synchronous: Synchronous::Normal,
            body_bytes: 200,
        };
        let mut it = std::env::args().skip(1);
        while let Some(flag) = it.next() {
            let value = it
                .next()
                .ok_or_else(|| format!("{flag} needs a value; {USAGE}"))?;
            let bad = |_| format!("{flag} {value}: not a number; {USAGE}");
            match flag.as_str() {
                "--rate" => a.rate = value.parse().map_err(bad)?,
                "--seconds" => a.seconds = value.parse().map_err(bad)?,
                "--body-bytes" => a.body_bytes = value.parse().map_err(bad)?,
                "--dir" => a.dir = PathBuf::from(value),
                "--synchronous" => {
                    a.synchronous = match value.as_str() {
                        "normal" => Synchronous::Normal,
                        "full" => Synchronous::Full,
                        _ => return Err(format!("--synchronous {value}; {USAGE}")),
                    }
                }
                _ => return Err(format!("unknown flag {flag}; {USAGE}")),
            }
        }
        if a.rate == 0 || a.seconds == 0 {
            return Err(format!("--rate and --seconds must be above zero; {USAGE}"));
        }
        if !(MIN_BODY..=SLOT_LEN).contains(&a.body_bytes) {
            return Err(format!(
                "--body-bytes {} is outside {MIN_BODY}..={SLOT_LEN} (the store's slot)",
                a.body_bytes
            ));
        }
        Ok(a)
    }

    /// `/proc/self/mounts` writes a space in a path as `\040`, and so on.
    fn unescape(field: &str) -> String {
        let bytes = field.as_bytes();
        let mut out = Vec::with_capacity(bytes.len());
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'\\'
                && i + 3 < bytes.len()
                && let Ok(n) = u8::from_str_radix(&field[i + 1..i + 4], 8)
            {
                out.push(n);
                i += 4;
            } else {
                out.push(bytes[i]);
                i += 1;
            }
        }
        String::from_utf8_lossy(&out).into_owned()
    }

    /// The type of the filesystem `dir` is on: the mount with the longest
    /// mount point that is a prefix of it. `None` when `/proc/self/mounts`
    /// cannot be read (not Linux).
    fn fs_type(dir: &Path) -> Option<String> {
        let mounts = std::fs::read_to_string("/proc/self/mounts").ok()?;
        let mut best: Option<(usize, String)> = None;
        for line in mounts.lines() {
            let mut f = line.split(' ');
            let (_dev, Some(point), Some(kind)) = (f.next(), f.next(), f.next()) else {
                continue;
            };
            let point = unescape(point);
            if dir.starts_with(&point) && best.as_ref().is_none_or(|(len, _)| point.len() >= *len) {
                best = Some((point.len(), kind.to_owned()));
            }
        }
        best.map(|(_, kind)| kind)
    }

    /// The smallest body [`fill`] can lay out.
    const MIN_BODY: usize = 48;
    /// Where the ten digits of `k` go in a body.
    const DIGITS_AT: usize = PREFIX.len();
    const PREFIX: &[u8] = b"8=FIX.4.4\x0135=8\x0134=";

    /// Message `k`, exactly `buf.len()` bytes: a fixed prefix, `k` in ten
    /// digits, a `58=` of filler that varies with `k`, an SOH.
    fn fill(buf: &mut [u8], k: u64) {
        buf[..PREFIX.len()].copy_from_slice(PREFIX);
        let mut n = k;
        for d in (0..10).rev() {
            buf[DIGITS_AT + d] = b'0' + (n % 10) as u8;
            n /= 10;
        }
        let at = DIGITS_AT + 10;
        buf[at..at + 4].copy_from_slice(b"\x0158=");
        let last = buf.len() - 1;
        for (i, b) in buf[at + 4..last].iter_mut().enumerate() {
            *b = b'a' + ((k as usize + i) % 26) as u8;
        }
        buf[last] = 0x01;
    }

    fn remove(path: &Path) {
        let _ = std::fs::remove_file(path);
    }

    fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
        let mut s = path.as_os_str().to_owned();
        s.push(suffix);
        PathBuf::from(s)
    }

    #[derive(Default)]
    struct Watched {
        behind_max_ms: u64,
        max_batch: u64,
        wal_bytes_max: u64,
    }

    pub fn run() -> i32 {
        let a = match args() {
            Ok(a) => a,
            Err(why) => {
                eprintln!("soak: {why}");
                return 2;
            }
        };
        let created = !a.dir.exists();
        if let Err(e) = std::fs::create_dir_all(&a.dir) {
            eprintln!("soak: create {}: {e}", a.dir.display());
            return 2;
        }
        let dir = match a.dir.canonicalize() {
            Ok(d) => d,
            Err(e) => {
                eprintln!("soak: {}: {e}", a.dir.display());
                return 2;
            }
        };
        let fs = fs_type(&dir).unwrap_or_else(|| "unknown".to_owned());
        if fs == "tmpfs" || fs == "ramfs" {
            eprintln!(
                "soak: refused: {} is on {fs}, so the database would live in memory and the run \
                 would measure nothing about a disk; pass a --dir on a real filesystem",
                dir.display()
            );
            if created {
                let _ = std::fs::remove_dir(&dir);
            }
            return 2;
        }
        let db = dir.join("soak.db");
        let wal = with_suffix(&db, "-wal");
        for p in [&db, &wal, &with_suffix(&db, "-shm")] {
            remove(p);
        }

        let cfg = Config::acceptor(b"FIX.4.4", b"ISLD", b"SOAK");
        let opts = SqliteOptions {
            synchronous: a.synchronous,
            ..SqliteOptions::default()
        };
        let j = match SqliteStore::open(&db, &cfg, opts) {
            Ok(j) => j,
            Err(e) => {
                eprintln!("soak: open {}: {e}", db.display());
                return 1;
            }
        };
        let progress = j.progress();
        let total = a.rate * a.seconds;
        let sent = Arc::new(AtomicU64::new(0));
        let stop = Arc::new(AtomicBool::new(false));

        // The monitor: progress and the WAL's size every 10 ms.
        let monitor = {
            let (progress, sent, stop, wal) = (
                progress.clone(),
                Arc::clone(&sent),
                Arc::clone(&stop),
                wal.clone(),
            );
            let rate = a.rate;
            std::thread::Builder::new()
                .name("soak-monitor".to_owned())
                .spawn(move || {
                    let mut w = Watched::default();
                    let mut last_committed = progress.committed();
                    while !stop.load(Ordering::Acquire) {
                        std::thread::sleep(Duration::from_millis(10));
                        let committed_out = u64::from(progress.highest_out().unwrap_or(0));
                        let behind = sent.load(Ordering::Acquire).saturating_sub(committed_out);
                        w.behind_max_ms = w.behind_max_ms.max(behind * 1000 / rate);
                        let committed = progress.committed();
                        w.max_batch = w.max_batch.max(committed - last_committed);
                        last_committed = committed;
                        if let Ok(m) = std::fs::metadata(&wal) {
                            w.wal_bytes_max = w.wal_bytes_max.max(m.len());
                        }
                    }
                    w
                })
                .expect("spawn the monitor")
        };

        // The stand-in engine thread: spin to each message's due time, then
        // `mark_in` and `put`. It owns the journal, as an engine thread does.
        let engine = {
            let sent = Arc::clone(&sent);
            let (rate, body_bytes) = (a.rate, a.body_bytes);
            std::thread::Builder::new()
                .name("soak-engine".to_owned())
                .spawn(move || {
                    let mut j = j;
                    let mut body = vec![0u8; body_bytes];
                    let late = Duration::from_millis(1);
                    let mut missed = 0u64;
                    let start = Instant::now();
                    for k in 1..=total {
                        let due = start
                            + Duration::from_nanos((k - 1).saturating_mul(1_000_000_000) / rate);
                        let mut now = Instant::now();
                        while now < due {
                            std::hint::spin_loop();
                            now = Instant::now();
                        }
                        if now.duration_since(due) > late {
                            missed += 1;
                        }
                        let seq = u32::try_from(k).expect("fewer than 2^32 messages");
                        j.mark_in(seq);
                        fill(&mut body, k);
                        assert!(j.put(seq, &body), "the store refused message {k}");
                        sent.store(k, Ordering::Release);
                    }
                    (j, missed)
                })
                .expect("spawn the engine")
        };

        let (mut j, pace_missed) = engine.join().expect("the engine thread");
        j.close();
        let unwritten = j.unwritten();
        let records = progress.committed();
        drop(j);
        stop.store(true, Ordering::Release);
        let watched = monitor.join().expect("the monitor thread");

        // Every row, against the generator.
        let (mut rows, mut mismatched) = (0u64, 0u64);
        {
            let conn = rusqlite::Connection::open(&db).expect("open the database with SQLite");
            let mut stmt = conn
                .prepare("SELECT seq, body FROM messages ORDER BY seq")
                .expect("prepare");
            let mut want = vec![0u8; a.body_bytes];
            let mut it = stmt.query([]).expect("query");
            while let Some(row) = it.next().expect("a row") {
                let seq: i64 = row.get(0).expect("seq");
                let seq = u64::try_from(seq).unwrap_or(0);
                let got: Vec<u8> = row.get(1).expect("body");
                rows += 1;
                if seq == 0 || seq > total {
                    mismatched += 1;
                    continue;
                }
                fill(&mut want, seq);
                if got != want {
                    mismatched += 1;
                }
            }
        }
        let db_bytes = std::fs::metadata(&db).map(|m| m.len()).unwrap_or(0);

        let sync = match a.synchronous {
            Synchronous::Normal => "normal",
            Synchronous::Full => "full",
        };
        println!(
            "soak: fs {fs} synchronous {sync} rate {} seconds {} messages {total} records {records} \
             unwritten {unwritten} rows {rows} mismatched {mismatched} behind_max_ms {} \
             max_batch {} wal_bytes_max {} db_bytes {db_bytes} pace_missed {pace_missed}",
            a.rate, a.seconds, watched.behind_max_ms, watched.max_batch, watched.wal_bytes_max
        );
        let mut failed = Vec::new();
        if unwritten > 0 {
            failed.push(format!("{unwritten} records unwritten"));
        }
        if rows != total {
            failed.push(format!("{rows} rows for {total} messages"));
        }
        if mismatched > 0 {
            failed.push(format!("{mismatched} rows mismatched"));
        }
        if pace_missed * 100 > total {
            failed.push(format!(
                "the engine thread missed its pace on {pace_missed} of {total} messages (over 1 %)"
            ));
        }
        if failed.is_empty() {
            0
        } else {
            eprintln!("soak: FAIL: {}", failed.join("; "));
            1
        }
    }
}
