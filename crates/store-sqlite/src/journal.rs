//! [`SqliteJournal`]: the engine thread's half, which is `FileJournal`
//! `Async`'s — a `MemJournal` answers `get`, a ring carries every record to
//! the writer thread — and never touches SQLite. ADR-0180 decisions 1, 3, 9,
//! 11.

use std::io;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::thread::JoinHandle;

use fixbolt_engine::journal::{MemJournal, Released, SLOT_LEN, SLOTS, WriterTicket};
use fixbolt_engine::ring::Producer;
use fixbolt_session::Config;
use fixbolt_session::journal::Journal;

use crate::schema;
use crate::writer::{self, ACTIVITY_LEN, Counters, Marks, STOP, Writer};

/// How hard a commit presses the WAL onto the disk. **Neither makes `put`
/// wait**: there is no mode in which the engine thread waits for a commit
/// (ADR-0180 decision 4). A deployment that must have a message on disk
/// before it is sent keeps `FileJournal` with `Durability::Fsync`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Synchronous {
    /// `PRAGMA synchronous = NORMAL`: a committed batch survives the process
    /// being killed, and may be lost to a power loss or an OS crash — the
    /// class `FileJournal` `Async` is in.
    #[default]
    Normal,
    /// `PRAGMA synchronous = FULL`: the WAL is synced at every commit, so a
    /// committed batch survives a power loss. Slows the writer only.
    Full,
}

/// The store's settings. `docs/CONFIGURATION.md` lists them with their
/// defaults.
#[derive(Debug, Clone)]
pub struct SqliteOptions {
    /// See [`Synchronous`]. Default [`Synchronous::Normal`].
    pub synchronous: Synchronous,
    /// Bytes in the ring between the engine thread and the writer, rounded up
    /// to a power of two. Default [`fixbolt_engine::ring::DEFAULT_CAPACITY`]
    /// (4 MiB): a commit and a WAL checkpoint stall the writer longer than a
    /// file append does. A record that does not fit is kept in memory and
    /// counted in `unwritten`.
    pub ring_bytes: usize,
    /// Records per transaction, at most. Default 4 096. A batch otherwise
    /// ends when the ring runs dry, so it is one record when idle and up to
    /// this under a burst.
    pub batch_max: usize,
    /// A ceiling on the database file, in pages (`PRAGMA max_page_count`).
    /// Default none. A batch that would grow the file past it fails, is
    /// rolled back and is counted in `unwritten`.
    pub max_db_pages: Option<u32>,
}

impl Default for SqliteOptions {
    fn default() -> Self {
        Self {
            synchronous: Synchronous::Normal,
            ring_bytes: fixbolt_engine::ring::DEFAULT_CAPACITY,
            batch_max: 4096,
            max_db_pages: None,
        }
    }
}

/// How far the writer has got: a clone of two atomics allocated at open, for
/// an operator's *how far behind is the database*, the crash test and the
/// soak harness (ADR-0180 decision 11). Reading it is one atomic load.
#[derive(Debug, Clone)]
pub struct Progress(Arc<Counters>);

impl Progress {
    /// Records committed by this journal's writer since it was opened —
    /// messages and marks alike.
    #[must_use]
    pub fn committed(&self) -> u64 {
        self.0.committed.load(Ordering::Acquire)
    }

    /// The highest outbound number the database holds as committed — from
    /// before this open, too — or `None` if it holds none.
    #[must_use]
    pub fn highest_out(&self) -> Option<u32> {
        match self.0.highest_out.load(Ordering::Acquire) {
            0 => None,
            n => u32::try_from(n).ok(),
        }
    }
}

/// **A journal whose durable copy is a SQLite database**, one database per
/// session. Implements the session's `Journal` as `FileJournal` does, with the
/// same engine-thread cost: `put` stores in memory and pushes one record onto
/// a ring; a writer thread, `fixbolt-sqlite`, commits.
///
/// **It holds no `rusqlite` type**: the connection is opened by [`Self::open`]
/// and moved into the writer thread, so nothing called on the engine thread
/// can reach SQLite, whose C heap no Rust counting allocator sees.
///
/// See `GUIDE.md` for what a deployment must honour: never open the database
/// file with `std::fs` while the store runs, one SQLite per process, and a
/// `Recovery` that answers `ready` from [`Self::released`].
pub struct SqliteJournal<const N: usize, const LEN: usize> {
    mem: MemJournal<N, LEN>,
    to_writer: Option<Producer>,
    /// Joined by [`Self::close`] and `Drop` — unless the journal was retired,
    /// which lets the writer go.
    writer: Option<JoinHandle<()>>,
    ticket: WriterTicket,
    released: Released,
    counters: Arc<Counters>,
    /// Records the ring had no room for. Engine thread only.
    unwritten: u64,
    last_active: Option<u64>,
}

/// [`SqliteJournal`] at the engine's default sizes, as `FileStore` is for
/// `FileJournal`.
pub type SqliteStore = SqliteJournal<SLOTS, SLOT_LEN>;

impl<const N: usize, const LEN: usize> SqliteJournal<N, LEN> {
    /// Open — or create — the database at `path` for the session `cfg`
    /// names, read back what a restart needs, and start the writer.
    ///
    /// **Not for the engine thread's hot path**: it opens a file, reads it
    /// and spawns a thread. It runs where a `Recovery::recover` runs.
    ///
    /// # Errors
    ///
    /// - `WouldBlock` at once, naming the path, when another connection —
    ///   this process or another — holds the database (ADR-0180 decision 5);
    /// - `InvalidData` for a database of another session, of an unknown
    ///   schema version, or that is not a fixbolt store;
    /// - any other SQLite or thread error, as `Other`.
    pub fn open(path: &Path, cfg: &Config, opts: SqliteOptions) -> io::Result<Self> {
        let (conn, loaded) = schema::open(path, cfg, &opts, N)?;
        let mut mem = MemJournal::new();
        for (seq, body) in &loaded.messages {
            let _ = mem.put(*seq, body);
        }
        if let Some(seq) = loaded.highest_in {
            mem.mark_in(seq);
        }
        if let Some(seq) = loaded.highest_out {
            mem.mark_out(seq);
        }
        let (to_writer, from_engine) = fixbolt_engine::ring::pair(opts.ring_bytes);
        let ticket = WriterTicket::new();
        let (releaser, released) = Released::pair();
        let counters = Arc::new(Counters::starting_at(loaded.highest_out));
        let w = Writer {
            conn,
            from_engine,
            ticket: ticket.clone(),
            releaser,
            counters: Arc::clone(&counters),
            marks: Marks {
                highest_in: loaded.highest_in,
                highest_out: loaded.highest_out,
                last_active: loaded.last_active,
            },
            batch_max: u64::try_from(opts.batch_max).unwrap_or(u64::MAX),
            buf_len: writer::buf_len(LEN),
        };
        let (ready, started) = std::sync::mpsc::sync_channel(1);
        let handle = std::thread::Builder::new()
            .name("fixbolt-sqlite".to_owned())
            .spawn(move || writer::run(w, &ready))?;
        match started.recv() {
            Ok(Ok(())) => {}
            Ok(Err(e)) => {
                let _ = handle.join();
                return Err(e);
            }
            Err(_) => {
                let _ = handle.join();
                return Err(io::Error::other(
                    "sqlite store: the writer ended before it started",
                ));
            }
        }
        Ok(Self {
            mem,
            to_writer: Some(to_writer),
            writer: Some(handle),
            ticket,
            released,
            counters,
            unwritten: 0,
            last_active: loaded.last_active,
        })
    }

    /// A handle that turns `true` once the writer has closed the database —
    /// after its last commit and checkpoint, so after SQLite's lock is gone.
    /// A `Recovery` keeps it and answers `ready` with it (ADR-0155 decision
    /// 3). An `Arc` clone: nothing is allocated.
    #[must_use]
    pub fn released(&self) -> Released {
        self.released.clone()
    }

    /// See [`Progress`]. An `Arc` clone.
    #[must_use]
    pub fn progress(&self) -> Progress {
        Progress(Arc::clone(&self.counters))
    }

    /// Stop the writer and **wait for it**: everything accepted is committed
    /// and the database closed when this returns. Called by `Drop`. Not for
    /// the engine thread while it serves — that is `Journal::retire`.
    pub fn close(&mut self) {
        if let Some(p) = self.to_writer.as_mut() {
            while !p.push(&[&[STOP]]) {
                // A writer that has ended will never make room.
                if self.writer.as_ref().is_none_or(JoinHandle::is_finished) {
                    break;
                }
                std::hint::spin_loop();
            }
        }
        if let Some(h) = self.writer.take() {
            let _ = h.join();
        }
        self.to_writer = None;
    }

    /// One record onto the ring, or one more in `unwritten`.
    fn push(&mut self, parts: &[&[u8]]) {
        if let Some(p) = self.to_writer.as_mut()
            && !p.push(parts)
        {
            self.unwritten += 1;
        }
    }
}

impl<const N: usize, const LEN: usize> Drop for SqliteJournal<N, LEN> {
    /// Joins the writer — unless the journal was retired, which let it go.
    fn drop(&mut self) {
        self.close();
    }
}

impl<const N: usize, const LEN: usize> Journal for SqliteJournal<N, LEN> {
    /// Kept in memory, then one record onto the writer's ring. **A full ring
    /// is counted and `put` still answers `true`** (ADR-0154 decision 4):
    /// memory holds the message and a resend in this process replays it.
    fn put(&mut self, seq: u32, bytes: &[u8]) -> bool {
        if !self.mem.put(seq, bytes) {
            return false;
        }
        let n = u32::try_from(bytes.len()).unwrap_or(0);
        self.push(&[&seq.to_le_bytes(), &n.to_le_bytes(), bytes]);
        true
    }

    fn get(&self, seq: u32) -> Option<&[u8]> {
        self.mem.get(seq)
    }

    fn highest(&self) -> Option<u32> {
        self.mem.highest()
    }

    fn oldest(&self) -> Option<u32> {
        self.mem.oldest()
    }

    fn mark_in(&mut self, seq: u32) {
        self.mem.mark_in(seq);
        self.push(&[&seq.to_le_bytes(), &0u32.to_le_bytes()]);
    }

    fn highest_in(&self) -> Option<u32> {
        self.mem.highest_in()
    }

    /// A high-water mark: a number already known pushes nothing (ADR-0053).
    fn mark_out(&mut self, seq: u32) {
        if self.mem.highest_out().is_some_and(|h| h >= seq) {
            return;
        }
        self.mem.mark_out(seq);
        let n = u32::try_from(writer::OUTBOUND_LEN).unwrap_or(0);
        self.push(&[&0u32.to_le_bytes(), &n.to_le_bytes(), &seq.to_le_bytes()]);
    }

    fn highest_out(&self) -> Option<u32> {
        self.mem.highest_out()
    }

    fn mark_active(&mut self, at_ms: u64) {
        self.last_active = Some(at_ms);
        let n = u32::try_from(ACTIVITY_LEN).unwrap_or(0);
        self.push(&[&0u32.to_le_bytes(), &n.to_le_bytes(), &at_ms.to_le_bytes()]);
    }

    fn last_active(&self) -> Option<u64> {
        self.last_active
    }

    /// Tell the writer to finish and let it go, **without waiting**: engine
    /// thread, mid-serving. *The rule for a journal* (ADR-0181 decision 1):
    /// push `STOP` once, then `ticket.retire(pushed)`, then detach — no
    /// syscall, no allocation, no spin. `serve*`'s `wait_for_retired_writers`
    /// waits for the writer after serving. A second call does nothing.
    fn retire(&mut self) {
        let Some(writer) = self.writer.take() else {
            return;
        };
        let pushed = self.to_writer.as_mut().is_some_and(|p| p.push(&[&[STOP]]));
        let _ = self.ticket.retire(pushed);
        drop(writer);
        self.to_writer = None;
    }

    /// Records the ring had no room for, plus records of batches the database
    /// refused (ADR-0180 decision 8). One relaxed load; no syscall.
    fn unwritten(&self) -> u64 {
        self.unwritten
            .saturating_add(self.counters.failed.load(Ordering::Relaxed))
    }
}
