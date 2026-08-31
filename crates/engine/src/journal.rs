//! `DESIGN.md` D7: persistence is a policy, and it is off the hot path.
//!
//! QuickFIX's `FileStore` calls `Sync()` on every write, across three files,
//! and that single choice is the dominant latency source in its default
//! configuration. Here the choice is the deployment's:
//!
//! | Policy | Type | Survives a restart |
//! |---|---|---|
//! | `None` | [`fixbolt_session::journal::NoJournal`] | no — and nothing is kept at all |
//! | in memory | [`MemJournal`] | no |
//! | `Async` | [`FileJournal`] with [`Durability::Async`] | yes, once the writer thread catches up |
//! | `Fsync` | [`FileJournal`] with [`Durability::Fsync`] | yes, before the message is on the wire |
//!
//! # Why a file journal still keeps a ring in memory
//!
//! A `ResendRequest` has to be answered *now*, on the engine thread, from a
//! `&[u8]`. Reading it back off disk would mean a blocking `read` on the thread
//! non-negotiable 4 protects. So the ring answers `get` and the file answers
//! the restart — memory index, durable log, the shape every real engine uses.
//!
//! **`Async` is the default D7 names, and it is the one that keeps the engine
//! thread clean:** the bytes go into an [`crate::ring`] and a writer thread does
//! the I/O. `Fsync` deliberately blocks, because a deployment that is required
//! to fsync is buying exactly that.

use std::fs::File;
use std::io::Write;
use std::path::Path;

use fixbolt_session::journal::Journal;

use crate::ring::{Consumer, Producer};

/// How many messages a [`MemJournal`] keeps by default, and the ring inside a
/// [`FileJournal`].
///
/// `[measured]` the acceptance corpus never asks for more than three at once.
/// Eight is the smallest power of two comfortably above that; a real acceptor
/// sets its own.
pub const SLOTS: usize = 8;

/// The longest message a slot holds, by default.
///
/// `[measured]` the longest application reply in the corpus is 177 body bytes.
pub const SLOT_LEN: usize = 512;

/// One kept message.
struct Slot<const LEN: usize> {
    seq: u32,
    len: u16,
    buf: [u8; LEN],
}

/// A ring of `N` slots, oldest overwritten first. Keeps nothing across a
/// restart, and says so.
///
/// This is the store that used to live inside `fixbolt_session`, moved out
/// under D1: the session says *keep this* and asks *do you still have it*, and
/// owns neither the bytes nor the policy.
pub struct MemJournal<const N: usize, const LEN: usize> {
    slots: [Slot<LEN>; N],
    at: usize,
    /// The highest inbound sequence number delivered to the application.
    ///
    /// Not a slot: nothing is kept about an inbound message except that it was
    /// consumed, so one number is the whole of it. ADR-0017.
    highest_in: Option<u32>,
}

impl<const N: usize, const LEN: usize> Default for MemJournal<N, LEN> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const N: usize, const LEN: usize> MemJournal<N, LEN> {
    /// An empty journal.
    ///
    /// Sequence number 0 is never used by FIX, so it is the empty marker and
    /// no separate `occupied` flag is needed.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            slots: [const {
                Slot {
                    seq: 0,
                    len: 0,
                    buf: [0; LEN],
                }
            }; N],
            at: 0,
            highest_in: None,
        }
    }
}

impl<const N: usize, const LEN: usize> Journal for MemJournal<N, LEN> {
    fn put(&mut self, seq: u32, bytes: &[u8]) {
        if bytes.len() > LEN || N == 0 {
            // Refused rather than truncated. A truncated replay is a message
            // that does not checksum; a refusal becomes a gap fill, which is
            // legal.
            return;
        }
        let slot = &mut self.slots[self.at % N];
        slot.seq = seq;
        slot.len = u16::try_from(bytes.len()).unwrap_or(0);
        slot.buf[..bytes.len()].copy_from_slice(bytes);
        self.at += 1;
    }

    fn get(&self, seq: u32) -> Option<&[u8]> {
        self.slots
            .iter()
            .find(|s| s.seq == seq && s.len > 0)
            .map(|s| &s.buf[..usize::from(s.len)])
    }

    fn highest(&self) -> Option<u32> {
        self.slots.iter().filter(|s| s.len > 0).map(|s| s.seq).max()
    }

    fn mark_in(&mut self, seq: u32) {
        // `max`, not assignment: `drain` releases held messages in sequence
        // order but `judge` may have already moved the count past them, and a
        // mark must never go backwards.
        self.highest_in = Some(self.highest_in.map_or(seq, |h| h.max(seq)));
    }

    fn highest_in(&self) -> Option<u32> {
        self.highest_in
    }
}

/// A [`MemJournal`] at the sizes [`SLOTS`] and [`SLOT_LEN`] name.
///
/// Const generics have no inference from defaults, so `MemJournal::new()` on
/// its own does not compile; this alias is what a caller with no opinion
/// writes. `TcpAcceptorEngine` uses it.
pub type Store = MemJournal<SLOTS, SLOT_LEN>;

/// When a [`FileJournal`] considers a message written.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Durability {
    /// Handed to a writer thread and returned from immediately.
    ///
    /// **D7's default.** Nothing blocks on the engine thread, and a crash can
    /// lose whatever the writer had not reached.
    #[default]
    Async,
    /// Written and `fsync`ed before `put` returns.
    ///
    /// **This blocks the engine thread, on purpose.** A deployment required to
    /// fsync is buying exactly that, and it is the one place non-negotiable 4
    /// is traded away by the user rather than by the engine.
    Fsync,
}

/// A journal that survives a restart.
///
/// The ring answers `get`; the file answers the restart. See the module
/// comment for why both.
pub struct FileJournal<const N: usize, const LEN: usize> {
    mem: MemJournal<N, LEN>,
    how: Durability,
    /// `Fsync` writes here. `Async` leaves it `None` and uses the ring.
    file: Option<File>,
    to_writer: Option<Producer>,
    /// The writer thread, joined on drop so a test can read the file after.
    writer: Option<std::thread::JoinHandle<()>>,
}

/// The header a `FileJournal` appends before each message: the sequence number
/// and the message's length, both little-endian `u32`.
///
/// **The length is what makes the file readable.** `[measured 2026-08-30]` the
/// first version wrote the sequence number and then the bytes, with nothing to
/// say where one record ended — so the file could be appended to and never
/// parsed, and open item 16 could not be closed without changing it. Splitting
/// records by re-framing FIX would work and would couple the journal to the
/// codec, and would still leave a torn tail ambiguous.
const RECORD_SEQ: usize = 4;
const RECORD_LEN: usize = 4;
const RECORD_HEADER: usize = RECORD_SEQ + RECORD_LEN;

/// A record whose length is zero is an **inbound mark**, not a message.
///
/// ADR-0017 needs the inbound count on disk beside the outbound messages, and
/// this is the whole of the encoding: a FIX message is never zero bytes, so a
/// zero length cannot be confused with one. It keeps the format unchanged and
/// the reader one branch longer, rather than adding a record-type byte that
/// every existing record would have to grow.
const INBOUND_MARK: usize = 0;

impl<const N: usize, const LEN: usize> FileJournal<N, LEN> {
    /// Append to `path`, creating it if it is not there.
    ///
    /// # Errors
    ///
    /// Whatever opening the file returns.
    pub fn open(path: &Path, how: Durability) -> std::io::Result<Self> {
        // **Read before appending.** Everything already in the file is put back
        // into the in-memory ring, so `get` and `highest` answer for messages
        // this process never sent. That is the difference between an audit trail
        // and a recovery mechanism.
        let mut mem_recovered: MemJournal<N, LEN> = MemJournal::new();
        let mut torn = 0usize;
        if let Ok(bytes) = std::fs::read(path) {
            let mut at = 0usize;
            while at + RECORD_HEADER <= bytes.len() {
                let mut s4 = [0u8; 4];
                let mut l4 = [0u8; 4];
                s4.copy_from_slice(&bytes[at..at + RECORD_SEQ]);
                l4.copy_from_slice(&bytes[at + RECORD_SEQ..at + RECORD_HEADER]);
                let seq = u32::from_le_bytes(s4);
                let len = u32::from_le_bytes(l4) as usize;
                let end = at + RECORD_HEADER + len;
                if end > bytes.len() {
                    // A process killed mid-write. The tail is dropped rather
                    // than half-read: replaying bytes that never went on the
                    // wire is worse than replaying nothing, because a gap fill
                    // is a legal answer and a corrupt message is not.
                    torn += 1;
                    break;
                }
                if len == INBOUND_MARK {
                    mem_recovered.mark_in(seq);
                } else {
                    mem_recovered.put(seq, &bytes[at + RECORD_HEADER..end]);
                }
                at = end;
            }
        }
        let _ = torn;
        let file = File::options().create(true).append(true).open(path)?;
        let (mem, mut this) = (
            mem_recovered,
            Self {
                mem: MemJournal::new(),
                how,
                file: None,
                to_writer: None,
                writer: None,
            },
        );
        this.mem = mem;
        match how {
            Durability::Fsync => this.file = Some(file),
            Durability::Async => {
                // Room for a healthy burst; a full ring means the message is
                // not journalled, which becomes a gap fill rather than a lie.
                let (to_writer, from_engine) = crate::ring::pair(1 << 20);
                this.to_writer = Some(to_writer);
                this.writer = Some(std::thread::spawn(move || write_loop(file, from_engine)));
            }
        }
        Ok(this)
    }

    /// Stop the writer thread and wait for it, so everything accepted is on
    /// disk.
    ///
    /// Called by `Drop`; public because a test that wants to read the file
    /// needs to say when.
    pub fn close(&mut self) {
        // An empty record is the stop signal: `push(&[])` writes a zero length,
        // which `write_loop` recognises and nothing else produces.
        if let Some(p) = self.to_writer.as_mut() {
            while !p.push(&[]) {
                std::hint::spin_loop();
            }
        }
        if let Some(h) = self.writer.take() {
            let _ = h.join();
        }
        self.to_writer = None;
    }
}

impl<const N: usize, const LEN: usize> Drop for FileJournal<N, LEN> {
    fn drop(&mut self) {
        self.close();
    }
}

/// The writer thread: everything the ring hands over, appended in order.
fn write_loop(mut file: File, mut from_engine: Consumer) {
    let mut buf = [0u8; 4096];
    loop {
        match from_engine.pop(&mut buf) {
            // The stop signal. Everything before it has already been written,
            // because the ring is ordered.
            Some(0) => {
                let _ = file.flush();
                return;
            }
            Some(n) => {
                let _ = file.write_all(&buf[..n]);
            }
            None => std::hint::spin_loop(),
        }
    }
}

impl<const N: usize, const LEN: usize> Journal for FileJournal<N, LEN> {
    fn put(&mut self, seq: u32, bytes: &[u8]) {
        self.mem.put(seq, bytes);
        match self.how {
            Durability::Async => {
                if let Some(p) = self.to_writer.as_mut() {
                    // A full ring is a message that is not journalled. It is
                    // dropped rather than waited on: waiting would put a
                    // disk's latency on the engine thread, which is the whole
                    // thing `Async` exists to avoid.
                    let n = u32::try_from(bytes.len()).unwrap_or(0);
                    let _ = p.push(&[&seq.to_le_bytes(), &n.to_le_bytes(), bytes]);
                }
            }
            Durability::Fsync => {
                if let Some(f) = self.file.as_mut() {
                    let mut rec = [0u8; RECORD_HEADER];
                    rec[..RECORD_SEQ].copy_from_slice(&seq.to_le_bytes());
                    let n = u32::try_from(bytes.len()).unwrap_or(0);
                    rec[RECORD_SEQ..].copy_from_slice(&n.to_le_bytes());
                    let _ = f.write_all(&rec);
                    let _ = f.write_all(bytes);
                    let _ = f.sync_data();
                }
            }
        }
    }

    fn get(&self, seq: u32) -> Option<&[u8]> {
        self.mem.get(seq)
    }

    fn highest(&self) -> Option<u32> {
        self.mem.highest()
    }

    fn mark_in(&mut self, seq: u32) {
        self.mem.mark_in(seq);
        // The same two tiers as `put`, and the same reasoning: `Async` must not
        // put a disk on the engine thread, `Fsync` must be on disk before the
        // call returns. **This is the cost ADR-0017 names**: under `Fsync` the
        // inbound path now pays a `sync_data` per message where it used to pay
        // nothing.
        match self.how {
            Durability::Async => {
                if let Some(p) = self.to_writer.as_mut() {
                    let _ = p.push(&[&seq.to_le_bytes(), &0u32.to_le_bytes()]);
                }
            }
            Durability::Fsync => {
                if let Some(f) = self.file.as_mut() {
                    let mut rec = [0u8; RECORD_HEADER];
                    rec[..RECORD_SEQ].copy_from_slice(&seq.to_le_bytes());
                    rec[RECORD_SEQ..].copy_from_slice(&0u32.to_le_bytes());
                    let _ = f.write_all(&rec);
                    let _ = f.sync_data();
                }
            }
        }
    }

    fn highest_in(&self) -> Option<u32> {
        self.mem.highest_in()
    }
}
