//! Every message this engine saw or sent, one line each, in a file.
//!
//! The first question a FIX desk asks in a dispute is *"at 10:32:07, what did
//! we receive and what did we send?"* Until this existed fixbolt could not
//! answer it: the journal keeps **outbound application** messages only, to
//! serve a `ResendRequest` (D7), the inbound side kept a number and no bytes
//! (ADR-0017), and a frame refused before the session saw it vanished
//! immediately.
//!
//! # Why this is not the journal
//!
//! Because the journal's key is `seq`, and the three things this file exists
//! for have none: an inbound frame not yet judged, a `Cut::Garbage` frame, and
//! a frame refused pre-session. The full argument, with the four questions it
//! had to answer, is in
//! `docs/reference/why-the-message-log-is-not-the-journal.md`.
//!
//! # What it costs, and where
//!
//! On the engine thread: **one `Producer::push` per message per direction**,
//! which copies the bytes into a ring one at a time (ADR-0007, no `unsafe`).
//! At ~1.7 ns/byte a 200-byte message is ~340 ns, and a request/reply pair pays
//! it twice. `[unproven]` — that is arithmetic from `DESIGN.md` §6, not a
//! measurement of this module; the §9 machine settles it.
//!
//! No formatting, no allocation and no syscall happen on the engine thread.
//! Everything below the ring runs on the writer, which **is** allowed to
//! allocate, for the same reason `journal::Reader` is (ADR-0037).
//!
//! # What it does not promise
//!
//! * `OUT` means **queued for sending**, not on the wire. A socket that dies
//!   with bytes still in `tx` discards them, and step 3 of the plan counts that
//!   as `EventKind::MessageLogUnsent` rather than pretending it did not happen.
//! * Every `OUT` line written during one engine turn carries the **same**
//!   millisecond, because a turn has one clock read. Order is the order of the
//!   lines, never the timestamp column.
//! * A full ring **drops and counts**. The log is never the reason a session
//!   stalls — ADR-0011's rule, pointed the other way.

use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{Seek, SeekFrom, Write};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread::JoinHandle;

use fixbolt_codec::timestamp::TimestampCache;

use crate::dispatch::ConnId;
use crate::ring::{Consumer, Producer};

/// Milliseconds between year zero and the Unix epoch.
///
/// The engine's `now_ms` counts from year zero (D13); [`TimestampCache`] wants
/// Unix milliseconds. Re-exported here so a caller formatting a log time does
/// not have to reach into the session crate for the constant.
pub const MILLIS_YEAR_ZERO_TO_EPOCH: u64 = fixbolt_session::clock::MILLIS_YEAR_ZERO_TO_EPOCH;

/// The longest message this log will carry, in bytes.
///
/// Sized from `RX`, the per-connection receive buffer that `TcpAcceptorEngine`
/// fixes at 4096: a frame longer than `RX` cannot be cut in the first place, so
/// a record longer than this cannot arrive from the read path. Anything that
/// does is refused and counted rather than silently dropped — see
/// [`FileLog::lost`].
pub const MAX_RECORD: usize = 4096;

/// `dir(1) ‖ at_ms(8) ‖ shard(2) ‖ conn(8)`, then the bytes.
const REC_HEADER: usize = 1 + 8 + 2 + 8;

/// What the writer's scratch buffer must hold: a full record, header included.
const WRITER_BUF: usize = REC_HEADER + MAX_RECORD;

/// The tag that means *stop*, and nothing else produces it.
///
/// **Not a zero-length record.** `FileJournal` uses an empty `push` as its stop
/// signal, which is safe there because its records can never exceed the
/// writer's buffer. Here they can, and `Consumer::pop` reports *"dropped,
/// oversized"* as `Some(0)` — the same value. A distinct tag keeps *stop* and
/// *lost a record* from being the same event.
const STOP: u8 = 0xFF;

/// Which way a message was going.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    /// Read off the socket, before the session judged it. Garbage included.
    In,
    /// Handed to the outbound queue. See the module note on what `OUT` means.
    Out,
    /// Not a message: a connection was accepted, and these bytes are its peer
    /// address.
    ///
    /// Pushed **once per connection**, so the address can appear on every line
    /// without the engine thread copying it every time.
    Open,
}

impl Direction {
    const fn tag(self) -> u8 {
        match self {
            Self::In => 0,
            Self::Out => 1,
            Self::Open => 2,
        }
    }

    const fn from_tag(tag: u8) -> Option<Self> {
        match tag {
            0 => Some(Self::In),
            1 => Some(Self::Out),
            2 => Some(Self::Open),
            _ => None,
        }
    }

    const fn label(self) -> &'static str {
        match self {
            Self::In => "IN ",
            Self::Out => "OUT",
            Self::Open => "OPN",
        }
    }
}

/// Somewhere for the engine to put what it saw.
///
/// Implemented by [`NoLog`] (the default, which compiles away) and [`FileLog`].
/// It lives in `engine` rather than `session` because the session never sees a
/// pre-session refusal and must not learn about files (D1).
pub trait MessageLog {
    /// `false` when this log records nothing, so a call site can fold the whole
    /// hook away instead of branching on it.
    const LOGS: bool = true;

    /// One message, going one way, at `at_ms` on the engine's year-zero clock.
    ///
    /// Must not block, must not allocate, and must not touch the disk — see
    /// `record_touches_no_file_until_the_writer_runs`.
    fn record(&mut self, dir: Direction, at_ms: u64, shard: u16, id: ConnId, bytes: &[u8]);

    /// Records that never reached the file, for any reason.
    fn lost(&self) -> u64 {
        0
    }
}

/// The default: records nothing, costs nothing.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoLog;

impl MessageLog for NoLog {
    const LOGS: bool = false;

    #[inline]
    fn record(&mut self, _dir: Direction, _at_ms: u64, _shard: u16, _id: ConnId, _bytes: &[u8]) {}
}

/// A text file, one line per message, written by a thread that is not the
/// engine.
pub struct FileLog {
    to_writer: Option<Producer>,
    writer: Option<JoinHandle<()>>,
    lost: Arc<AtomicU64>,
    torn: usize,
}

impl FileLog {
    /// See [`MAX_RECORD`]. Named on the type so a test can say what it means.
    pub const MAX_RECORD: usize = MAX_RECORD;

    /// Append to `path`, creating it if it is not there.
    ///
    /// A file whose last byte is not a newline was left by a process that died
    /// mid-write; the tear is marked rather than merged with whatever is
    /// appended next. See [`Self::torn_tail_bytes`].
    ///
    /// # Errors
    ///
    /// Whatever opening, seeking or marking the file returns.
    pub fn open(path: &Path) -> std::io::Result<Self> {
        Self::open_with(path, crate::ring::DEFAULT_CAPACITY, true)
    }

    /// As [`open`](Self::open), with a ring of `bytes` rather than the default.
    ///
    /// # Errors
    ///
    /// Whatever opening, seeking or marking the file returns.
    pub fn with_capacity(path: &Path, bytes: usize) -> std::io::Result<Self> {
        Self::open_with(path, bytes, true)
    }

    /// As [`open`](Self::open), with the writer thread **pinned to `core`**.
    ///
    /// ADR-0015 decision 8: every thread gets a home, including the ones that
    /// are not the engine. A writer left to float can land on the very core the
    /// engine was isolated onto.
    ///
    /// # Errors
    ///
    /// Whatever opening the file returns, or the affinity error in its own
    /// words if the writer cannot pin itself.
    #[cfg(all(feature = "affinity", target_os = "linux"))]
    pub fn open_pinned(path: &Path, core: crate::affinity::CoreId) -> std::io::Result<Self> {
        let (file, torn) = Self::prepare(path)?;
        let (to_writer, from_engine) = crate::ring::pair(crate::ring::DEFAULT_CAPACITY);
        let lost = Arc::new(AtomicU64::new(0));
        let counted = Arc::clone(&lost);
        let (handle, _on) = crate::affinity::spawn_pinned("fixbolt-msglog", core, move || {
            write_loop(file, from_engine, &counted);
        })?;
        Ok(Self {
            to_writer: Some(to_writer),
            writer: Some(handle),
            lost,
            torn,
        })
    }

    /// The log and the consumer that would have been the writer's.
    ///
    /// **No writer thread runs**, so nothing drains the ring and nothing
    /// reaches the file. It exists so two guards can be direct rather than
    /// inferred: that a full ring drops instead of waiting, and that `record`
    /// never touches the disk itself. Both are invisible to the allocation
    /// bench, which cannot see a syscall.
    ///
    /// # Errors
    ///
    /// Whatever opening, seeking or marking the file returns.
    pub fn deferred(path: &Path, bytes: usize) -> std::io::Result<(Self, Consumer)> {
        let (_file, torn) = Self::prepare(path)?;
        let (to_writer, from_engine) = crate::ring::pair(bytes);
        Ok((
            Self {
                to_writer: Some(to_writer),
                writer: None,
                lost: Arc::new(AtomicU64::new(0)),
                torn,
            },
            from_engine,
        ))
    }

    /// The shared loss counter, for a reader that samples rather than asks.
    ///
    /// `observe::Snapshot` holds one of these and does a relaxed load when
    /// somebody asks for a snapshot, the same way every other counter in
    /// ADR-0032 is read.
    ///
    /// It is also the only way to observe **that the writer thread has ended**:
    /// the writer owns a clone, so once it returns the strong count drops back
    /// to what the callers hold. `dropping_a_file_log_without_close_still_writes_what_was_queued`
    /// leans on that, because the obvious version of that test passes whether
    /// or not `Drop` exists — the writer drains in the background either way,
    /// and the assertion measures a race rather than the thing it names.
    #[must_use]
    pub fn counter(&self) -> Arc<AtomicU64> {
        Arc::clone(&self.lost)
    }

    /// Bytes at the end of the file that were not a whole line when it was
    /// opened. **Zero for a file a process closed cleanly.**
    ///
    /// Anything else means a process was killed mid-write. Those bytes are left
    /// exactly as they were and a `# torn tail` line is written after them, so
    /// the next record cannot be read as a continuation of half of an older
    /// one. It describes the file **as it was opened**; appending does not
    /// change it.
    #[must_use]
    pub const fn torn_tail_bytes(&self) -> usize {
        self.torn
    }

    /// Stop the writer and wait for it, so everything accepted is on disk.
    ///
    /// Called by `Drop`; public because a test that wants to read the file has
    /// to say when. **Takes `&mut self`** for that reason — a by-value `close`
    /// could not be called from `Drop`, and then a process that ended without
    /// calling it would write nothing and count nothing.
    pub fn close(&mut self) {
        if self.writer.is_some() {
            if let Some(p) = self.to_writer.as_mut() {
                while !p.push(&[&[STOP]]) {
                    std::hint::spin_loop();
                }
            }
        }
        if let Some(h) = self.writer.take() {
            let _ = h.join();
        }
        self.to_writer = None;
    }

    fn open_with(path: &Path, bytes: usize, spawn: bool) -> std::io::Result<Self> {
        let (file, torn) = Self::prepare(path)?;
        let (to_writer, from_engine) = crate::ring::pair(bytes);
        let lost = Arc::new(AtomicU64::new(0));
        let writer = if spawn {
            let counted = Arc::clone(&lost);
            Some(std::thread::spawn(move || {
                write_loop(file, from_engine, &counted);
            }))
        } else {
            None
        };
        Ok(Self {
            to_writer: Some(to_writer),
            writer,
            lost,
            torn,
        })
    }

    /// Open for append, and mark a torn tail if there is one.
    fn prepare(path: &Path) -> std::io::Result<(File, usize)> {
        let mut file = OpenOptions::new().create(true).append(true).open(path)?;
        let len = file.metadata()?.len();
        if len == 0 {
            return Ok((file, 0));
        }
        // Only the last byte decides. A text log is torn exactly when it does
        // not end in a newline.
        let mut last = [0u8; 1];
        {
            let mut reader = File::open(path)?;
            reader.seek(SeekFrom::End(-1))?;
            std::io::Read::read_exact(&mut reader, &mut last)?;
        }
        if last[0] == b'\n' {
            return Ok((file, 0));
        }
        let torn = torn_len(path, len)?;
        writeln!(
            file,
            "\n# torn tail, {torn} bytes, not a whole line when reopened"
        )?;
        file.flush()?;
        Ok((file, torn))
    }
}

impl Drop for FileLog {
    fn drop(&mut self) {
        self.close();
    }
}

impl MessageLog for FileLog {
    fn record(&mut self, dir: Direction, at_ms: u64, shard: u16, id: ConnId, bytes: &[u8]) {
        let Some(p) = self.to_writer.as_mut() else {
            return;
        };
        let pushed = p.push(&[
            &[dir.tag()],
            &at_ms.to_le_bytes(),
            &shard.to_le_bytes(),
            &id.to_le_bytes(),
            bytes,
        ]);
        if !pushed {
            // ADR-0011's rule, pointed the other way: losing a log line is
            // accepted, and counted. Stalling the engine on a disk is not.
            self.lost.fetch_add(1, Ordering::Relaxed);
        }
    }

    fn lost(&self) -> u64 {
        self.lost.load(Ordering::Relaxed)
    }
}

/// How many bytes sit after the last newline, or the whole file if there is
/// none.
fn torn_len(path: &Path, len: u64) -> std::io::Result<usize> {
    // Bounded: a torn tail longer than one record is not a torn line, it is a
    // different problem, and reading the whole file to say so is not worth it.
    let window = u64::try_from(WRITER_BUF).unwrap_or(u64::MAX).min(len);
    let mut reader = File::open(path)?;
    reader.seek(SeekFrom::End(-(i64::try_from(window).unwrap_or(i64::MAX))))?;
    let mut buf = vec![0u8; usize::try_from(window).unwrap_or(WRITER_BUF)];
    std::io::Read::read_exact(&mut reader, &mut buf)?;
    Ok(buf
        .iter()
        .rposition(|b| *b == b'\n')
        .map_or(usize::try_from(len).unwrap_or(usize::MAX), |at| {
            buf.len() - at - 1
        }))
}

/// The writer thread: format, escape, append.
///
/// **Allowed to allocate**, for the reason `journal::Reader` is (ADR-0037): it
/// is not the engine thread, and nothing on the hot path waits for it.
fn write_loop(mut file: File, mut from_engine: Consumer, lost: &AtomicU64) {
    let mut buf = vec![0u8; WRITER_BUF];
    let mut clock = TimestampCache::new();
    let mut peers: HashMap<(u16, ConnId), String> = HashMap::new();
    let mut line: Vec<u8> = Vec::with_capacity(WRITER_BUF * 2);
    // Flush when the ring runs dry, not per line: a `grep` two seconds later
    // still sees everything, and a busy engine does not pay a `write` syscall
    // per message on the writer either.
    let mut dirty = false;
    loop {
        match from_engine.pop(&mut buf) {
            None => {
                if dirty {
                    let _ = file.flush();
                    dirty = false;
                }
                std::thread::yield_now();
            }
            // Not the stop signal: `pop` says *"a record was dropped because it
            // did not fit"* this way, and the stop signal is a `STOP` tag.
            Some(0) => {
                lost.fetch_add(1, Ordering::Relaxed);
            }
            Some(n) => {
                if n == 1 && buf[0] == STOP {
                    let _ = file.flush();
                    return;
                }
                if n < REC_HEADER {
                    lost.fetch_add(1, Ordering::Relaxed);
                    continue;
                }
                let Some(dir) = Direction::from_tag(buf[0]) else {
                    lost.fetch_add(1, Ordering::Relaxed);
                    continue;
                };
                let at_ms = u64::from_le_bytes(buf[1..9].try_into().unwrap_or([0; 8]));
                let shard = u16::from_le_bytes(buf[9..11].try_into().unwrap_or([0; 2]));
                let id = ConnId::from_le_bytes(buf[11..19].try_into().unwrap_or([0; 8]));
                let payload = &buf[REC_HEADER..n];

                line.clear();
                if dir == Direction::Open {
                    let peer = String::from_utf8_lossy(payload).into_owned();
                    let stamp = clock.format(at_ms.saturating_sub(MILLIS_YEAR_ZERO_TO_EPOCH));
                    line.extend_from_slice(b"# conn=");
                    push_num(id, &mut line);
                    line.extend_from_slice(b" shard=");
                    push_num(u64::from(shard), &mut line);
                    line.extend_from_slice(b" peer=");
                    line.extend_from_slice(peer.as_bytes());
                    line.extend_from_slice(b" opened at ");
                    line.extend_from_slice(stamp);
                    peers.insert((shard, id), peer);
                } else {
                    let stamp = clock.format(at_ms.saturating_sub(MILLIS_YEAR_ZERO_TO_EPOCH));
                    line.extend_from_slice(stamp);
                    line.push(b' ');
                    line.extend_from_slice(dir.label().as_bytes());
                    line.extend_from_slice(b" shard=");
                    push_num(u64::from(shard), &mut line);
                    line.extend_from_slice(b" conn=");
                    push_num(id, &mut line);
                    line.push(b' ');
                    if let Some(peer) = peers.get(&(shard, id)) {
                        line.extend_from_slice(b"peer=");
                        line.extend_from_slice(peer.as_bytes());
                        line.push(b' ');
                    }
                    escape_into(payload, &mut line);
                }
                line.push(b'\n');
                if file.write_all(&line).is_err() {
                    // The disk is gone. Everything from here would be lost
                    // anyway, and counting it is the only honest thing left.
                    lost.fetch_add(1, Ordering::Relaxed);
                    return;
                }
                dirty = true;
            }
        }
    }
}

/// A `u64` written into `out` without allocating.
///
/// **`to_string()` allocates, once per number, once per line.**
/// `[measured 2026-09-04]` the `log-busy` allocation case read **4** over two
/// logged messages — the connection id and the shard id of each — and the
/// counting allocator is global, so writer-thread allocations land in the same
/// number as engine-thread ones. ADR-0037 permits the writer to allocate; it
/// does not require it to, and two allocations per line on a busy log is a
/// steady-state cost with a twenty-line fix.
fn push_num(mut n: u64, out: &mut Vec<u8>) {
    // 20 digits is `u64::MAX`, so the buffer can never be too small.
    let mut digits = [0u8; 20];
    let mut at = digits.len();
    loop {
        at -= 1;
        digits[at] = b'0' + u8::try_from(n % 10).unwrap_or(0);
        n /= 10;
        if n == 0 {
            break;
        }
    }
    out.extend_from_slice(digits.get(at..).unwrap_or(b"0"));
}

/// `\` → `\\`, newline → `\n`, carriage return → `\r`. Everything else, and
/// `SOH` in particular, goes through untouched.
///
/// **The backslash rule is not decoration.** A DATA field may legally carry
/// `0x0A`, `0x0D` and `0x5C` (D3), and without escaping the backslash a field
/// holding the two bytes `\` `n` is indistinguishable in the file from a real
/// newline that was escaped — at which point the line has stopped being a
/// record of what arrived. `a_backslash_in_a_data_field_round_trips` is the
/// guard.
fn escape_into(bytes: &[u8], out: &mut Vec<u8>) {
    for b in bytes {
        match *b {
            b'\\' => out.extend_from_slice(b"\\\\"),
            b'\n' => out.extend_from_slice(b"\\n"),
            b'\r' => out.extend_from_slice(b"\\r"),
            other => out.push(other),
        }
    }
}

/// The inverse of the escape, for anything reading the file back.
///
/// Allocates, and says so: nothing on the hot path calls it.
#[must_use]
pub fn unescape(line: &str) -> Vec<u8> {
    let src = line.as_bytes();
    let mut out = Vec::with_capacity(src.len());
    let mut at = 0;
    while at < src.len() {
        if src[at] == b'\\' && at + 1 < src.len() {
            match src[at + 1] {
                b'\\' => out.push(b'\\'),
                b'n' => out.push(b'\n'),
                b'r' => out.push(b'\r'),
                other => {
                    out.push(b'\\');
                    out.push(other);
                }
            }
            at += 2;
        } else {
            out.push(src[at]);
            at += 1;
        }
    }
    out
}

/// Where shard `i`'s log lives, given the path the operator named.
///
/// `messages.log` becomes `messages.log.0`, `messages.log.1`, and so on.
///
/// **One file per shard is not a tidiness preference.** Every engine numbers
/// its connections from zero, so N shards sharing a path write `conn=0` for N
/// different sockets, and N writer threads interleave into one descriptor. The
/// suffix is what keeps a line's `conn=` meaningful, and `shard=` on the line
/// is what lets somebody who concatenated the files put them back in order.
#[must_use]
pub fn shard_path(base: &Path, shard: usize) -> std::path::PathBuf {
    let mut p = base.as_os_str().to_os_string();
    p.push(format!(".{shard}"));
    std::path::PathBuf::from(p)
}

/// A log that may or may not be there, decided at run time.
///
/// **`Option<FileLog>` cannot implement [`MessageLog`] usefully**: `LOGS` is a
/// constant, so an `Option` would have to claim either that it always logs or
/// that it never does. This claims it always might, which is the truth for a
/// runtime-configured engine — `FileLogPath` present or absent — and costs one
/// branch per message on an engine that turned it off.
///
/// A deployment that will never log should use [`NoLog`] and pay nothing at
/// all; this exists for the paths where the answer is only known once a
/// configuration file has been read.
pub struct MaybeLog(pub Option<FileLog>);

impl MessageLog for MaybeLog {
    fn record(&mut self, dir: Direction, at_ms: u64, shard: u16, id: ConnId, bytes: &[u8]) {
        if let Some(l) = self.0.as_mut() {
            l.record(dir, at_ms, shard, id, bytes);
        }
    }

    fn lost(&self) -> u64 {
        self.0.as_ref().map_or(0, FileLog::lost)
    }
}
