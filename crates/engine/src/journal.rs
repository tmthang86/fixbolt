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
//!
//! # A message carrying a secret stays in memory only
//!
//! A message holding a field [`crate::redact::MASKED`] names is kept in the
//! ring verbatim — so a `ResendRequest` inside the same process replays it —
//! and the **file** gets only the outbound mark for its number. After a restart
//! that number has no bytes and is gap-filled. ADR-0110 decision 4.

use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use fixbolt_session::journal::Journal;

use crate::ring::{Consumer, Idle, Producer};

/// How many messages a [`MemJournal`] keeps by default, and the ring inside a
/// [`FileJournal`].
///
/// `[2026-09-04]` **4096, and it used to be 8.** Eight was the smallest power
/// of two above what the acceptance corpus asks for — *"a real acceptor sets
/// its own"*, said the rustdoc — and nothing forced a real acceptor to set
/// anything. An acceptor that sent 100 `ExecutionReport`s and was asked
/// `7=1 16=0` replayed eight and gap-filled ninety-two: legal on the wire,
/// ninety-two fills gone to the counterparty, and no counter on this side said
/// so.
///
/// 4096 is chosen to hold a normal trading day for a desk, at
/// `N × (LEN + 8)` ≈ **2 MiB per session**. A gateway holding hundreds of
/// sessions picks a smaller `N` through the const generic — `GUIDE.md` §6 has
/// the arithmetic.
/// [ADR-0046](../../../docs/decisions/ADR-0046-the-ring-is-the-resend-store-and-a-replay-goes-in-batches.md)
/// decision 2.
pub const SLOTS: usize = 4096;

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
    /// **Boxed, and allocated once in [`Self::new`].**
    ///
    /// Not non-negotiable 1 broken: this is startup, in the same class as the
    /// pre-faulted buffers D8 already asks for, and `benches/alloc.rs` counts a
    /// window that excludes construction. It is also the only shape that is
    /// *safe* — `MemJournal<4096, 512>` as an inline array builds 2 MiB on the
    /// stack and moves it, and at 65 536 slots that is 32 MiB against an 8 MiB
    /// default stack: a SIGSEGV, not a red test. The `const` assertion below
    /// makes going back a compile error.
    slots: Box<[Slot<LEN>]>,
    /// The highest number ever kept, for [`Journal::oldest`]'s floor.
    ///
    /// Monotonic on purpose: an operator winding the outbound count backwards
    /// ([ADR-0036]) must not lower the floor, because the messages above it are
    /// still in the ring.
    ///
    /// [ADR-0036]: ../../../docs/decisions/ADR-0036-one-mechanism-two-capabilities.md
    high_water: Option<u32>,
    /// The highest inbound sequence number delivered to the application.
    ///
    /// Not a slot: nothing is kept about an inbound message except that it was
    /// consumed, so one number is the whole of it. ADR-0017.
    highest_in: Option<u32>,
    /// The highest outbound number spent, whether or not its bytes were kept.
    ///
    /// Separate from [`Self::high_water`], which is the floor
    /// [`Journal::oldest`] is computed from and moves only when a message is
    /// *kept*. This one also moves for a `Heartbeat`, a `Logout` and a
    /// refused `put` — the numbers a restart needs and a replay cannot use.
    /// ADR-0053.
    highest_out: Option<u32>,
}

/// **Going back to an inline `[Slot<LEN>; N]` is a compile error, not a test.**
///
/// A test can be deleted, skipped, or quietly pass on a machine with a large
/// stack. This cannot: the struct is a fat pointer and two options, and an
/// inline 2 MiB array does not fit in 64 bytes.
const _: () = assert!(core::mem::size_of::<Store>() <= 64);

impl<const N: usize, const LEN: usize> Default for MemJournal<N, LEN> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const N: usize, const LEN: usize> MemJournal<N, LEN> {
    /// An empty journal, with its slots allocated.
    ///
    /// Sequence number 0 is never used by FIX, so it is the empty marker and
    /// no separate `occupied` flag is needed.
    ///
    /// `[2026-09-04]` **No longer `const fn`**, because the slots are boxed —
    /// see the field. Nothing in this repository built one in a `const`
    /// context, and nothing published depends on it.
    #[must_use]
    pub fn new() -> Self {
        let mut slots = Vec::with_capacity(N);
        slots.resize_with(N, || Slot {
            seq: 0,
            len: 0,
            buf: [0; LEN],
        });
        Self {
            slots: slots.into_boxed_slice(),
            high_water: None,
            highest_in: None,
            highest_out: None,
        }
    }
}

impl<const N: usize, const LEN: usize> Journal for MemJournal<N, LEN> {
    fn put(&mut self, seq: u32, bytes: &[u8]) -> bool {
        if bytes.len() > LEN || N == 0 {
            // Refused rather than truncated. A truncated replay is a message
            // that does not checksum; a refusal becomes a gap fill, which is
            // legal. **And it is now reported**, so the session can count it —
            // ADR-0046.
            return false;
        }
        // **A slot records its length as a `u16`**, so a message longer than
        // 65 535 bytes is refused whatever `LEN` is. Until 2026-09-23 it was
        // kept with a length of zero, `put` answered `true`, and `get` then
        // answered `None` — a refusal nobody counted. ADR-0150 decision 3;
        // `a_message_longer_than_a_u16_is_refused_not_kept_empty`.
        let Ok(len) = u16::try_from(bytes.len()) else {
            return false;
        };
        // **Addressed by the number, not by a write cursor.** One slot can
        // hold one sequence number at a time, so `get` is an index and a
        // comparison rather than a scan of all `N` — which at 4096 slots is
        // what makes a 1000-message resend affordable instead of four million
        // comparisons on the engine thread. ADR-0046 decision 3.
        let Some(slot) = self.slots.get_mut((seq as usize) % N) else {
            // Unreachable: `N == 0` returned above and the modulus is `% N`.
            // Written as a refusal rather than an index because the compiler
            // cannot see that, and a refusal is already a legal outcome here —
            // it becomes a gap fill. `indexing_slicing`, 2026-09-08.
            return false;
        };
        slot.seq = seq;
        slot.len = len;
        slot.buf[..bytes.len()].copy_from_slice(bytes);
        self.high_water = Some(self.high_water.map_or(seq, |h| h.max(seq)));
        // A kept message spends its number too, so this is the same fact
        // `mark_out` records — which is what makes a `mark_out` following a
        // successful `put` a no-op rather than a second write. ADR-0053.
        self.highest_out = Some(self.highest_out.map_or(seq, |h| h.max(seq)));
        true
    }

    /// One index and one comparison.
    ///
    /// **The comparison is not a formality.** `seq % N` collides every `N`
    /// numbers, so a slot holding 9 is the slot 4105 would go in; without
    /// `s.seq == seq` a resend for a number this end never sent would come back
    /// as somebody else's message, correctly numbered and correctly checksummed.
    /// `a_number_reused_after_an_admin_reset_does_not_return_the_old_bytes`
    /// is the test, and **the scan this replaced failed it**: `find` returned
    /// the *first* slot carrying the number, which after a wind-back was the
    /// stale copy.
    fn get(&self, seq: u32) -> Option<&[u8]> {
        if N == 0 {
            return None;
        }
        let slot = self.slots.get((seq as usize) % N)?;
        (slot.seq == seq && slot.len > 0).then(|| &slot.buf[..usize::from(slot.len)])
    }

    /// The lowest number this ring can still answer for. **A floor, not a
    /// promise that the number itself is here.**
    ///
    /// Exactness is not available in O(1) and is not what the caller needs.
    /// Only *application* messages are journalled, so the numbers in the ring
    /// are sparse — an acceptor answering one order in three keeps 2, 5, 8 —
    /// and the smallest number actually present cannot be found without a scan.
    /// What can be said in constant time is the useful half:
    ///
    /// > everything below this **certainly** fell out of the ring
    ///
    /// which is exactly the question a session asks before deciding whether a
    /// gap fill is worth telling an operator about. A number *above* the floor
    /// that `get` cannot answer was an administrative message or a refused one
    /// — never resendable, so never a loss. ADR-0046 decision 1.
    fn oldest(&self) -> Option<u32> {
        if N == 0 {
            return None;
        }
        let n = u32::try_from(N).unwrap_or(u32::MAX);
        self.high_water
            .map(|h| h.saturating_sub(n.saturating_sub(1)).max(1))
    }

    /// **Still a scan, deliberately.**
    ///
    /// It is asked once per connection, by recovery, and never on the message
    /// path — `get` is the one the resend loop calls. Making this O(1) means
    /// choosing between *max over the slots* and *the last number written*,
    /// which differ after an operator winds the count back, and there is no
    /// measurement saying the difference is worth the risk to a recovery path.
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

    fn mark_out(&mut self, seq: u32) {
        // `max`, for the reason `mark_in` uses it and one more: the session
        // tells this the same high-water mark on every turn, so all but the
        // first telling of a number is deliberately nothing.
        self.highest_out = Some(self.highest_out.map_or(seq, |h| h.max(seq)));
    }

    fn highest_out(&self) -> Option<u32> {
        self.highest_out
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
    /// `Fsync` writes here, and this holds the file's lock. `Async` leaves it
    /// `None`: the writer thread owns the file, and the lock with it.
    file: Option<File>,
    to_writer: Option<Producer>,
    /// The writer thread, joined on drop so a test can read the file after —
    /// unless the journal was retired, which lets it go instead.
    writer: Option<std::thread::JoinHandle<()>>,
    /// What the engine has told the writer. `Async` only; allocated at open,
    /// so [`Journal::retire`] allocates nothing. ADR-0153 decision 3, through
    /// the public handle of ADR-0181 decision 1.
    ticket: Option<WriterTicket>,
    /// Where that thread was observed running, if it was pinned.
    #[cfg(all(feature = "affinity", target_os = "linux"))]
    writer_core: Option<crate::affinity::CoreId>,
    /// Bytes at the end of the file that did not form a whole record when it
    /// was opened. See [`FileJournal::torn_tail_bytes`].
    torn: usize,
    /// The latest activity mark, read back on open and updated on write.
    last_active: Option<u64>,
    /// Which on-disk format this file is in. Decided at open, never changed.
    format: Format,
    /// Records the writer's ring had no room for under [`Durability::Async`]:
    /// kept in memory, never written to the file. Only rises. See
    /// [`Journal::unwritten`]; ADR-0154 decision 4.
    unwritten: u64,
    /// Set once the file is closed — by the writer under `Async`, by `Drop`
    /// under `Fsync`. Allocated at open. See [`Released`]; ADR-0155.
    released: Released,
    /// What sets [`Self::released`]: kept here under `Fsync`, whose `Drop`
    /// closes the file; moved to the writer thread under `Async`.
    releaser: Option<Releaser>,
    /// Records whose CRC did not match what was stored beside them.
    ///
    /// **Zero on a version-0 file, always**, because that format carries no
    /// checksums and cannot report one. See [`FileJournal::corrupt_records`].
    corrupt: usize,
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

/// The five bytes a version-1 journal starts with.
///
/// **A file without it is version 0 and is read exactly as it always was.**
/// That is the whole compatibility rule: no byte of the old format changed, and
/// the marker is the only thing that says a file has checksums. `FXBJ` is the
/// project, `\x01` is the version.
const HEADER_V1: &[u8; 5] = b"FXBJ\x01";

/// Bytes a version-1 record carries after its payload: one CRC32.
const RECORD_CRC: usize = 4;

/// Which format a file is in.
///
/// **Decided when the file is opened and never changed afterwards.** A file
/// whose first half has no checksums and whose second half does is a file no
/// reader can parse without guessing where the change happened, so a version-0
/// journal stays version 0 for as long as it is appended to. Only a file that
/// did not exist gets a header.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Format {
    /// No header, no checksums. Everything written before 2026-09-04.
    V0,
    /// `FXBJ\x01`, and a CRC32 after every record's payload.
    V1,
}

/// CRC32, IEEE polynomial, from a table built at compile time.
///
/// **Zero dependency, deliberately.** `codec` has none and this crate justifies
/// each of its own; a 256-entry table is thirty lines and a `const fn`, and a
/// crate for it would be a dependency in the dependency tree of a FIX engine
/// for the rest of its life.
// The loop is bounded by `i < 256` and the array is 256 long. `const fn` rules
// out the alternative outright: neither `slice::get_mut` nor `Option` is
// available in a const context on this toolchain. `indexing_slicing`,
// 2026-09-08.
#[allow(clippy::indexing_slicing)]
const fn crc_table() -> [u32; 256] {
    let mut table = [0u32; 256];
    let mut i = 0usize;
    while i < 256 {
        let mut c = i as u32;
        let mut k = 0;
        while k < 8 {
            c = if c & 1 == 1 {
                0xEDB8_8320 ^ (c >> 1)
            } else {
                c >> 1
            };
            k += 1;
        }
        table[i] = c;
        i += 1;
    }
    table
}

/// The table, built once at compile time.
static CRC_TABLE: [u32; 256] = crc_table();

/// CRC32 over `parts` laid end to end.
///
/// Takes the pieces rather than one slice so the caller never has to join
/// `seq`, `len` and the payload into a buffer first — on the `Fsync` path that
/// would be an allocation on the engine thread, which is the one thing this
/// module may not do.
// `[measured 2026-09-08]` **The one index in this module that stays an index.**
// `idx` is masked with `& 0xFF` on the line above, and `CRC_TABLE` is 256 long,
// so the subscript is in range by construction — but clippy cannot see a proof
// that lives one line up, and `indexing_slicing` is denied workspace-wide.
// `.get(idx).unwrap_or(&0)` would replace an impossible panic with a *silent
// wrong checksum*, which is strictly worse: the panic would at least be found.
// This runs per byte on the `Fsync` path, which is the other reason.
#[allow(clippy::indexing_slicing)]
fn crc32(parts: &[&[u8]]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for part in parts {
        for b in *part {
            let idx = ((crc ^ u32::from(*b)) & 0xFF) as usize;
            crc = CRC_TABLE[idx] ^ (crc >> 8);
        }
    }
    crc ^ 0xFFFF_FFFF
}

/// A record whose length is zero is an **inbound mark**, not a message.
///
/// ADR-0017 needs the inbound count on disk beside the outbound messages, and
/// this is the whole of the encoding: a FIX message is never zero bytes, so a
/// zero length cannot be confused with one. It keeps the format unchanged and
/// the reader one branch longer, rather than adding a record-type byte that
/// every existing record would have to grow.
const INBOUND_MARK: usize = 0;

/// A record whose **sequence number** is zero is an *activity mark* — eight
/// little-endian bytes saying when the session was last alive.
///
/// `34=0` is not a sequence number FIX has, so a zero here cannot be confused
/// with a message, exactly as a zero *length* cannot. `[2026-09-02]` that
/// symmetry is why the format did not have to change to carry this: the reader
/// is one branch longer and every file written before it still parses.
/// `STATUS.md` item 32 (c).
const ACTIVITY_MARK: u32 = 0;

/// How many bytes an activity mark carries: one `u64` of milliseconds.
const ACTIVITY_LEN: usize = 8;

/// How many bytes an *outbound mark* carries: one `u32`, the highest outbound
/// sequence number spent.
///
/// It shares the reserved `seq == 0` of [`ACTIVITY_MARK`] and is told apart by
/// its length, exactly as the inbound mark is told apart by having none. **The
/// third and last use of that escape**: it works because `34=0` is not a
/// sequence number FIX can produce and because nothing is published, and a
/// fourth shape would be a format only its own history can read. The next
/// record shape lifts the version to v2. ADR-0053.
const OUTBOUND_LEN: usize = 4;

/// The record `mark_out(seq)` writes, before its CRC: an ADR-0053 outbound mark.
///
/// **One function, three callers**: `mark_out`, and the two places a message
/// that carries a secret is replaced by the mark for its number (ADR-0110
/// decision 4) — `write_loop` under `Async`, `put` under `Fsync`. One builder
/// is what keeps *"the same bytes `mark_out` writes"* true.
fn outbound_mark(seq: u32) -> [u8; RECORD_HEADER + OUTBOUND_LEN] {
    let mut rec = [0u8; RECORD_HEADER + OUTBOUND_LEN];
    rec[..RECORD_SEQ].copy_from_slice(&ACTIVITY_MARK.to_le_bytes());
    let n = u32::try_from(OUTBOUND_LEN).unwrap_or(0);
    rec[RECORD_SEQ..RECORD_HEADER].copy_from_slice(&n.to_le_bytes());
    rec[RECORD_HEADER..].copy_from_slice(&seq.to_le_bytes());
    rec
}

/// The sequence number of a **message** record whose bytes carry a secret, or
/// `None` for anything else — a clean message, or any of the three marks.
///
/// `record` is `seq ‖ len ‖ bytes`, as `put` pushes it under `Async`.
fn carries_secret_at(record: &[u8]) -> Option<u32> {
    let mut s4 = [0u8; RECORD_SEQ];
    s4.copy_from_slice(record.get(..RECORD_SEQ)?);
    let seq = u32::from_le_bytes(s4);
    let payload = record.get(RECORD_HEADER..)?;
    // `seq == 0` is an activity or outbound mark and an empty payload is an
    // inbound mark; none of them is a message.
    (seq != ACTIVITY_MARK && !payload.is_empty() && crate::redact::carries_secret(payload))
        .then_some(seq)
}

/// Where the writer thread should be pinned, if anywhere.
///
/// Two aliases rather than two copies of `open_with`: without the `affinity`
/// feature there is no `CoreId` to name, and `Infallible` makes the `Some` arm
/// uninhabited so the one body compiles either way.
#[cfg(all(feature = "affinity", target_os = "linux"))]
type WriterCore = Option<crate::affinity::CoreId>;
/// See the other [`WriterCore`].
#[cfg(not(all(feature = "affinity", target_os = "linux")))]
type WriterCore = Option<core::convert::Infallible>;

impl<const N: usize, const LEN: usize> FileJournal<N, LEN> {
    /// Append to `path`, creating it if it is not there.
    ///
    /// **One appender per file, for the file's whole life.** `open` takes an
    /// exclusive lock on the file (`File::try_lock`, `flock` on Unix) *before*
    /// it reads it, and the lock lives as long as the file does: with the
    /// writer thread under [`Durability::Async`] — released when the writer
    /// closes the file after its last flush, which for a
    /// [retired](Journal::retire) journal is after the connection is gone —
    /// and with this journal under [`Durability::Fsync`]. A second `open` of the
    /// same path meanwhile, in this process or another, is refused **at once**;
    /// it never waits. [ADR-0154] decision 1.
    ///
    /// **`WouldBlock` does not mean "no history".** It means the history is
    /// still being written. A [`crate::recovery::Recovery`] that opens a
    /// `FileJournal` keeps its [`Self::released`] handle and answers
    /// [`crate::recovery::Recovery::ready`] with it, so the engine parks the
    /// connection until the file is whole instead of asking `recover` too
    /// early (ADR-0155).
    ///
    /// # Errors
    ///
    /// [`std::io::ErrorKind::WouldBlock`], naming the path, if another
    /// appender holds the file; any other error the lock returns (a
    /// filesystem that cannot lock is refused rather than shared unguarded);
    /// or whatever opening or reading the file returns.
    ///
    /// [ADR-0154]: ../../../docs/decisions/ADR-0154-a-journal-file-has-one-appender-a-reconnect-waits-for-it-by-parking-and-what-the-file-missed-is-counted.md
    pub fn open(path: &Path, how: Durability) -> std::io::Result<Self> {
        Self::open_with(path, how, None)
    }

    /// As [`open`](Self::open), with the writer thread **pinned to `core`**.
    ///
    /// [ADR-0015] decision 8: every thread gets a home, including the ones that
    /// are not the engine. Pinning the engine to an isolated core and leaving
    /// the journal's writer to float defeats the isolation, because the writer
    /// can land on that very core.
    ///
    /// Only [`Durability::Async`] has a writer thread. Asking to pin a `Fsync`
    /// journal is refused rather than accepted and ignored: a constructor that
    /// silently drops an argument is how a deployment ends up believing it
    /// pinned something.
    ///
    /// # Errors
    ///
    /// Whatever opening the file returns; if `how` is [`Durability::Fsync`]; or
    /// if the writer thread cannot pin itself, with the affinity error's own
    /// words.
    ///
    /// [ADR-0015]: ../../../docs/decisions/ADR-0015-explicit-cores-pinned-from-inside-and-read-back.md
    #[cfg(all(feature = "affinity", target_os = "linux"))]
    pub fn open_pinned(
        path: &Path,
        how: Durability,
        core: crate::affinity::CoreId,
    ) -> std::io::Result<Self> {
        if how == Durability::Fsync {
            return Err(std::io::Error::other(
                "Durability::Fsync has no writer thread, so there is nothing to pin",
            ));
        }
        Self::open_with(path, how, Some(core))
    }

    /// The core the writer thread was **observed on**, if it was pinned.
    ///
    /// Read back from the scheduler by that thread, not copied from the request.
    #[cfg(all(feature = "affinity", target_os = "linux"))]
    #[must_use]
    pub const fn writer_core(&self) -> Option<crate::affinity::CoreId> {
        self.writer_core
    }

    #[cfg_attr(
        not(all(feature = "affinity", target_os = "linux")),
        expect(
            unused_variables,
            reason = "`core` has no meaning without the affinity feature, and the \
                      parameter stays so the two constructors share one body"
        )
    )]
    fn open_with(path: &Path, how: Durability, core: WriterCore) -> std::io::Result<Self> {
        // **Read before appending.** Everything already in the file is put back
        // into the in-memory ring, so `get` and `highest` answer for messages
        // this process never sent. That is the difference between an audit trail
        // and a recovery mechanism.
        let mut mem_recovered: MemJournal<N, LEN> = MemJournal::new();
        let mut torn = 0usize;
        let mut corrupt = 0usize;
        let mut last_active: Option<u64> = None;
        // **The file decides the format, not this process.** A file that
        // already exists is read in whatever it is, and appended to in the
        // same, for ever. Only a file that is not there yet — or one that is
        // there and empty — gets a header.
        //
        // **The lock before the read.** A file read while another appender is
        // still writing it is a file read short: `[measured 2026-09-24]` a
        // reconnect's recovery read `highest_out=Some(1)` of a journal whose
        // retired writer had not yet flushed `Some(2)`, 50 reconnects in 50,
        // and both writers then appended to one file. ADR-0154 decision 1.
        let mut file = File::options()
            .create(true)
            .read(true)
            .append(true)
            .open(path)?;
        take_the_file(&file, path)?;
        let mut existing = Vec::new();
        file.read_to_end(&mut existing)?;
        // A file that is not there yet is version 1; one that is there is
        // whatever its first five bytes say, for ever.
        let has_header = existing.get(..HEADER_V1.len()) == Some(HEADER_V1);
        let format = if existing.is_empty() || has_header {
            Format::V1
        } else {
            Format::V0
        };
        {
            let bytes = &existing;
            let mut at = if format == Format::V1 && !bytes.is_empty() {
                HEADER_V1.len()
            } else {
                0
            };
            while at + RECORD_HEADER <= bytes.len() {
                let mut s4 = [0u8; 4];
                let mut l4 = [0u8; 4];
                // **A journal file is the one input to this module that another
                // process wrote**, and it can be torn, truncated or garbage.
                // Every read of it goes through `get`, and a `None` is treated
                // exactly as a torn tail: stop, keep everything before this
                // point, report the bytes dropped. `indexing_slicing`,
                // 2026-09-08.
                let (Some(sb), Some(lb)) = (
                    bytes.get(at..at + RECORD_SEQ),
                    bytes.get(at + RECORD_SEQ..at + RECORD_HEADER),
                ) else {
                    torn = bytes.len() - at;
                    break;
                };
                s4.copy_from_slice(sb);
                l4.copy_from_slice(lb);
                let seq = u32::from_le_bytes(s4);
                let len = u32::from_le_bytes(l4) as usize;
                // Checked, because `len` comes off the disk. On a 32-bit target
                // a corrupt length wraps this sum, and a wrapped `end` names a
                // range that is *in bounds and wrong* — which `get` cannot save
                // us from. `Reader::open` already did this; the writing side
                // did not.
                let Some(end) = at
                    .checked_add(RECORD_HEADER)
                    .and_then(|x| x.checked_add(len))
                else {
                    torn = bytes.len() - at;
                    break;
                };
                // A version-1 record carries its checksum after the payload, so
                // "the whole record" is four bytes longer.
                let whole = if format == Format::V1 {
                    match end.checked_add(RECORD_CRC) {
                        Some(w) => w,
                        None => {
                            torn = bytes.len() - at;
                            break;
                        }
                    }
                } else {
                    end
                };
                if whole > bytes.len() {
                    // A process killed mid-write. The tail is dropped rather
                    // than half-read: replaying bytes that never went on the
                    // wire is worse than replaying nothing, because a gap fill
                    // is a legal answer and a corrupt message is not.
                    //
                    // **Dropped, but not hidden.** `[2026-09-02]` this count
                    // used to end at `let _ = torn;`, so a process that had
                    // been killed mid-write left no trace an operator could
                    // find. Skipping it is a recovery decision; being silent
                    // about it was a defect.
                    torn = bytes.len() - at;
                    break;
                }
                // **A bad checksum is treated exactly as a torn tail is.** The
                // reasoning is the one already written above and it does not
                // change with the cause: a gap fill is a legal answer to a
                // `ResendRequest` and a corrupt message is not, so the read
                // stops here and everything before it stands. The difference
                // from a tear is that this one is *detected* — before version 1
                // a flipped byte was replayed as a real message, correctly
                // framed and correctly numbered.
                if format == Format::V1 {
                    let mut c4 = [0u8; RECORD_CRC];
                    let (Some(cb), Some(body)) = (bytes.get(end..whole), bytes.get(at..end)) else {
                        torn = bytes.len() - at;
                        break;
                    };
                    c4.copy_from_slice(cb);
                    if u32::from_le_bytes(c4) != crc32(&[body]) {
                        corrupt = 1;
                        torn = bytes.len() - at;
                        break;
                    }
                }
                let Some(payload) = bytes.get(at + RECORD_HEADER..end) else {
                    torn = bytes.len() - at;
                    break;
                };
                if seq == ACTIVITY_MARK && len == ACTIVITY_LEN {
                    let mut t = [0u8; ACTIVITY_LEN];
                    t.copy_from_slice(payload);
                    // **The latest wins, not the first.** They are appended in
                    // order, so the last one is the one that describes the
                    // session at the moment it stopped.
                    last_active = Some(u64::from_le_bytes(t));
                } else if seq == ACTIVITY_MARK && len == OUTBOUND_LEN {
                    let mut n = [0u8; OUTBOUND_LEN];
                    n.copy_from_slice(payload);
                    // `mark_out` takes the max, so the order these are read in
                    // does not matter and a wound-back count does not lower it.
                    mem_recovered.mark_out(u32::from_le_bytes(n));
                } else if len == INBOUND_MARK {
                    mem_recovered.mark_in(seq);
                } else {
                    mem_recovered.put(seq, payload);
                }
                at = whole;
            }
            if at + RECORD_HEADER > bytes.len() && at < bytes.len() {
                // A tail too short to even hold a header is torn too.
                torn = bytes.len() - at;
            }
        }
        // The header goes on a file that had nothing in it. Written before any
        // record, so a reader never sees a record without one.
        if format == Format::V1 && existing.is_empty() {
            file.write_all(HEADER_V1)?;
            file.flush()?;
        }
        let (releaser, released) = Released::pair();
        let (mem, mut this) = (
            mem_recovered,
            Self {
                mem: MemJournal::new(),
                how,
                file: None,
                to_writer: None,
                writer: None,
                ticket: None,
                #[cfg(all(feature = "affinity", target_os = "linux"))]
                writer_core: None,
                torn,
                format,
                corrupt,
                last_active,
                unwritten: 0,
                released,
                releaser: Some(releaser),
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
                // **`open` returns only once the writer holds its buffer.** The
                // buffer is allocated on the writer thread (ADR-0150 decision
                // 1, ADR-0037), and a writer that started late would allocate
                // it whenever the scheduler got round to it — `[measured
                // 2026-09-23]` inside `benches/alloc.rs`'s `mark-out-file-async`
                // window, whose allocator is global, in 1 run of 6 under load.
                // Waiting here makes every writer allocation happen before
                // `open` returns: startup, not the engine's path.
                let (ready, started) = std::sync::mpsc::sync_channel::<()>(1);
                let ticket = WriterTicket::new();
                this.ticket = Some(ticket.clone());
                let releaser = this.releaser.take();
                let run = move || {
                    let buf = vec![0u8; writer_buf(LEN)];
                    let _ = ready.send(());
                    drop(ready);
                    write_until_told(file, from_engine, format, buf, ticket.told());
                    // `write_until_told` took the file by value and has
                    // dropped it: **the lock went with it**, so the next
                    // `open` of this path can read a whole file (ADR-0154).
                    // Said now, after the close and before the count below,
                    // so a recovery asking `Released` never hears "released"
                    // while this thread still holds the file (ADR-0155).
                    if let Some(releaser) = releaser {
                        releaser.release();
                    }
                    //
                    // **The last act, after the file and the ring are gone.**
                    // A writer the engine retired is one somebody will wait
                    // for after serving; that wait ends when this reaches zero.
                    // An unretired ticket lowers nothing. ADR-0153 decision 3.
                    ticket.finish();
                };
                #[cfg(all(feature = "affinity", target_os = "linux"))]
                if let Some(core) = core {
                    let (handle, on) = crate::affinity::spawn_pinned("fixbolt-journal", core, run)?;
                    // `Err` only if the writer ended before it said so, and
                    // then there is nothing to wait for.
                    let _ = started.recv();
                    this.writer = Some(handle);
                    this.writer_core = Some(on);
                    return Ok(this);
                }
                // Named as the pinned one is, so a test and an operator can
                // find it in `/proc/<pid>/task/*/comm`. ADR-0150 decision 4.
                this.writer = Some(
                    std::thread::Builder::new()
                        .name("fixbolt-journal".to_owned())
                        .spawn(run)?,
                );
                let _ = started.recv();
            }
        }
        Ok(this)
    }

    /// Stop the writer thread and wait for it, so everything accepted is on
    /// disk.
    ///
    /// Called by `Drop`; public because a test that wants to read the file
    /// needs to say when.
    /// Bytes at the end of the file that did not form a whole record when this
    /// journal was opened. **Zero on a file written by a process that exited
    /// cleanly.**
    ///
    /// Anything else means a process was killed mid-write. Those bytes are
    /// **not** replayed — a gap fill is a legal answer to a `ResendRequest` and
    /// a corrupt message is not — but they are not hidden either.
    /// `[2026-09-02]` this count was computed and then discarded, so a killed
    /// process left no trace an operator could find.
    ///
    /// It describes the file **as it was opened**. Appending does not change
    /// it.
    #[must_use]
    pub const fn torn_tail_bytes(&self) -> usize {
        self.torn
    }

    /// Records whose stored checksum did not match their bytes.
    ///
    /// **Zero or one**: the read stops at the first one, exactly as it stops at
    /// a torn tail, so this says *whether* the file was corrupt rather than how
    /// often. Everything before it is held and answerable; nothing after it is
    /// trusted.
    ///
    /// **Always zero on a version-0 file** — one written before 2026-09-04, or
    /// one that has been appended to since. That format has no checksums, so a
    /// flipped byte in it is still replayed as though it were a real message.
    /// That is what version 1 buys, and `crates/engine/tests/on_disk.rs` asserts
    /// both halves so the second cannot quietly stop being true.
    #[must_use]
    pub const fn corrupt_records(&self) -> usize {
        self.corrupt
    }

    /// A handle that says when this journal's file has been let go of —
    /// by the writer, after its last flush and close, not by asking the
    /// filesystem. Take it when the journal is opened and keep it beside the
    /// path; see [`Released`]. An `Arc` clone: nothing is allocated.
    #[must_use]
    pub fn released(&self) -> Released {
        self.released.clone()
    }

    pub fn close(&mut self) {
        // A one-byte `STOP` record is the stop signal, which `write_loop`
        // recognises and nothing else produces — every other record is at
        // least `RECORD_HEADER` bytes. ADR-0150 decision 2.
        if let Some(p) = self.to_writer.as_mut() {
            while !p.push(&[&[STOP]]) {
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
    /// Joins the writer — **unless the journal was retired**, in which case
    /// there is no writer left to join. ADR-0153 decision 5.
    fn drop(&mut self) {
        self.close();
        // `Fsync` holds the file, and its lock, here; `Async`'s writer has
        // already let go of its own.
        if let Some(file) = self.file.take() {
            release_the_file(&file);
            drop(file);
            // `Fsync`: the file is closed now (ADR-0155 decision 2).
            if let Some(releaser) = self.releaser.take() {
                releaser.release();
            }
        }
    }
}

/// The writer has not been told anything: it stops at `STOP`, and nobody
/// counts it.
const RUNNING: u8 = 0;
/// Retired, with `STOP` in its ring. It stops there, and uncounts itself.
const RETIRED: u8 = 1;
/// Retired while its ring was full, so no `STOP` could be pushed. It stops the
/// first time the ring runs dry after reading this, and uncounts itself.
const RETIRED_STOP_WHEN_DRY: u8 = 2;
/// A retired writer that has lowered the count — or a writer that finished
/// unretired and was retired after. Nothing leaves this state.
const FINISHED: u8 = 3;
/// A writer that finished before anyone retired it: it lowered nothing. The
/// first `retire` to meet it moves it to [`FINISHED`] and is counted in
/// [`writers_retired`], not in what [`wait_for_retired_writers`] waits for.
/// **Internal**: [`WriterTicket::state`] reports it as
/// [`TicketState::Finished`]. ADR-0181 *Revision 2*.
const FINISHED_UNRETIRED: u8 = 4;

/// What a [`WriterTicket`] has been told, as its writer reads it with
/// [`WriterTicket::state`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TicketState {
    /// Not retired. The writer stops at its own stop record, if its journal
    /// is closed, and nobody waits for it after serving.
    Running,
    /// Retired, and the journal's stop record is in the writer's queue: stop
    /// there, then [`WriterTicket::finish`].
    Retired,
    /// Retired while the writer's queue had no room for a stop record: stop
    /// the first time the queue is found empty **after reading this**, then
    /// [`WriterTicket::finish`]. Every record was queued before it was said.
    RetiredStopWhenDry,
    /// The writer has finished — retired first, or stopped before anyone
    /// retired it. Nothing leaves this state, and a later
    /// [`WriterTicket::retire`] counts nothing.
    Finished,
}

/// **A journal writer's place in the process-wide count of retired writers**
/// that every `serve*` waits for after serving
/// ([`wait_for_retired_writers`]). ADR-0181 decision 1, over ADR-0153
/// decisions 3 and 4.
///
/// Make one where the journal is opened — **never on the engine thread**: it
/// is one `Arc` — and give a clone to the writer thread. Then:
///
/// - the journal's [`Journal::retire`], on the engine thread, pushes its
///   one-byte stop record **once**, then calls [`WriterTicket::retire`] with
///   whether that push succeeded, then detaches the writer, and pushes
///   nothing after;
/// - the writer stops at a popped stop record. When a pop finds its queue
///   empty it reads [`WriterTicket::state`]; told
///   [`TicketState::RetiredStopWhenDry`], it pops **once more** and stops
///   only if that pop is empty too — state first, then the empty queue;
/// - stopping, the writer makes its data durable, closes its storage, calls
///   [`Releaser::release`] and then [`WriterTicket::finish`], its **last
///   act**.
///
/// **No order of that push and that retire strands a count.** Two counts,
/// two questions:
///
/// - what [`wait_for_retired_writers`] waits for — **writers still to
///   finish** — is raised only by a `retire` that moves *running* to retired,
///   and lowered only by that writer's `finish`;
/// - [`writers_retired`] — **retire acts** — rises by exactly one on a
///   ticket's first `retire`, whether its writer had finished or not.
///
/// A writer that pops the stop record and finishes before the engine's
/// `retire` moves its ticket to an internal *finished, never retired* state
/// (reported as [`TicketState::Finished`]), lowering nothing; the `retire`
/// that follows counts one retire act, leaves nothing to wait for, and
/// returns `false`. Each transition happens at most once, by
/// compare-and-swap. ADR-0181 decision 1, *Revisions 1 and 2*.
/// `crates/engine/tests/writer_hooks.rs` holds each half; [`FileJournal`]
/// uses nothing else, so its eight test binaries hold it too.
#[derive(Debug, Clone)]
pub struct WriterTicket(Arc<AtomicU8>);

impl Default for WriterTicket {
    fn default() -> Self {
        Self::new()
    }
}

impl WriterTicket {
    /// A ticket for a writer that has not been retired. Allocates one `Arc`.
    #[must_use]
    pub fn new() -> Self {
        Self(Arc::new(AtomicU8::new(RUNNING)))
    }

    /// Retire the writer this ticket belongs to: **engine thread**.
    ///
    /// - From *running*: counts the writer among those
    ///   [`wait_for_retired_writers`] waits for, raises [`writers_retired`]
    ///   by one, and returns `true`.
    /// - The first call after the writer finished unretired (it popped the
    ///   stop record and finished before this call): raises
    ///   [`writers_retired`] by one, leaves nothing to wait for, returns
    ///   `false`.
    /// - Any later call: changes nothing, returns `false`.
    ///
    /// **`true` means only "this call left a writer to wait for"** — never
    /// use it as evidence that a retire happened; [`writers_retired`] is that
    /// evidence.
    ///
    /// `stop_pushed` is the result of the journal's one push of its stop
    /// record, made just before this call. `false` tells the writer to stop
    /// the first time its queue runs dry.
    ///
    /// Atomics only: no system call, no allocation, no loop. The count is
    /// raised **before** the state is published, so the writer's `finish`
    /// can never lower it first (ADR-0153 decision 3).
    pub fn retire(&self, stop_pushed: bool) -> bool {
        let to = if stop_pushed {
            RETIRED
        } else {
            RETIRED_STOP_WHEN_DRY
        };
        RETIRED_WRITERS.fetch_add(1, Ordering::AcqRel);
        if self
            .0
            .compare_exchange(RUNNING, to, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            // Not running: take back the raise above, which nobody could have
            // paid off, since `finish` lowers only for the state this call
            // failed to publish.
            RETIRED_WRITERS.fetch_sub(1, Ordering::AcqRel);
            // The writer finished before this, the first retire: a retire act
            // all the same, and nothing to wait for. Once, by CAS.
            if self
                .0
                .compare_exchange(
                    FINISHED_UNRETIRED,
                    FINISHED,
                    Ordering::AcqRel,
                    Ordering::Acquire,
                )
                .is_ok()
            {
                WRITERS_RETIRED.fetch_add(1, Ordering::Relaxed);
            }
            return false;
        }
        WRITERS_RETIRED.fetch_add(1, Ordering::Relaxed);
        true
    }

    /// What this ticket has been told: **writer thread**. One `Acquire` load.
    #[must_use]
    pub fn state(&self) -> TicketState {
        match self.0.load(Ordering::Acquire) {
            RUNNING => TicketState::Running,
            RETIRED => TicketState::Retired,
            RETIRED_STOP_WHEN_DRY => TicketState::RetiredStopWhenDry,
            _ => TicketState::Finished,
        }
    }

    /// The writer's **last act**: the ticket becomes
    /// [`TicketState::Finished`], once, by compare-and-swap. From a retired
    /// state it lowers what [`wait_for_retired_writers`] waits for; from
    /// *running* it lowers nothing — the writer stopped before anyone retired
    /// it — and the ticket is *finished, never retired* inside, so the first
    /// later [`WriterTicket::retire`] still counts one retire act and leaves
    /// nothing to wait for. On a finished ticket it does nothing.
    pub fn finish(&self) {
        let moved = self
            .0
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |s| match s {
                RUNNING => Some(FINISHED_UNRETIRED),
                RETIRED | RETIRED_STOP_WHEN_DRY => Some(FINISHED),
                _ => None,
            });
        if matches!(moved, Ok(RETIRED | RETIRED_STOP_WHEN_DRY)) {
            RETIRED_WRITERS.fetch_sub(1, Ordering::AcqRel);
        }
    }

    /// The flag the writer loop reads. Private: the loop takes a bare
    /// `AtomicU8` so the unit test at the end of this file can drive it.
    fn told(&self) -> &AtomicU8 {
        &self.0
    }
}

/// Writers a [`FileJournal`] retired that have not yet finished — **one count
/// for the whole process**, shared by every engine in it (ADR-0153
/// *Consequences*). Raised by [`Journal::retire`], lowered by the writer as the
/// last thing it does.
static RETIRED_WRITERS: AtomicUsize = AtomicUsize::new(0);

/// Every writer [`Journal::retire`] has let go in this process, finished or
/// not. Only ever rises. See [`writers_retired`].
static WRITERS_RETIRED: AtomicUsize = AtomicUsize::new(0);

/// How many `Async` writers a [`FileJournal`] has retired in this process,
/// ever — finished or not.
///
/// **Only ever rises**, so a caller can tell that a retire actually happened
/// without racing the writer, which lowers the count
/// [`wait_for_retired_writers`] waits on as soon as it is done.
/// `benches/alloc.rs` case `retire` reads it to prove its path is live.
#[must_use]
pub fn writers_retired() -> usize {
    WRITERS_RETIRED.load(Ordering::Relaxed)
}

/// Wait until every writer a retired [`FileJournal`] let go has written its
/// last byte and closed its file, or until `timeout` passes. `true` if they
/// all finished.
///
/// **Teardown only: this sleeps**, 1 ms between looks at the count, which is
/// exactly what the engine thread may not do while it serves. Every `serve*`
/// and `connect_and_serve*` function, and the shard's serve, calls it after
/// its loop has returned. **A caller driving an [`crate::Engine`] directly
/// must call it before the process exits**, or an `Async` journal can lose
/// what its writer had not reached — the compiler cannot say so, `GUIDE.md`
/// does. ADR-0153 decision 4.
///
/// The count is process-wide, so this also waits for writers another engine in
/// the same process retired. That only makes it wait longer.
#[must_use]
pub fn wait_for_retired_writers(timeout: Duration) -> bool {
    let start = Instant::now();
    loop {
        if RETIRED_WRITERS.load(Ordering::Acquire) == 0 {
            return true;
        }
        if start.elapsed() >= timeout {
            return false;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
}

/// Take the exclusive lock a [`FileJournal`] holds on its file for the file's
/// whole life, or say who has it. ADR-0154 decision 1.
fn take_the_file(file: &File, path: &Path) -> std::io::Result<()> {
    match file.try_lock() {
        Ok(()) => Ok(()),
        Err(std::fs::TryLockError::WouldBlock) => Err(std::io::Error::new(
            std::io::ErrorKind::WouldBlock,
            format!(
                "journal {} is held by another appender: a writer still flushing, or another process",
                path.display()
            ),
        )),
        Err(std::fs::TryLockError::Error(e)) => Err(e),
    }
}

/// Whether a [`FileJournal`]'s file has been let go of — **answered by the
/// journal's own writer, not by the filesystem.**
///
/// A clone of one flag allocated when the journal was opened. It turns `true`
/// once, and stays: under [`Durability::Async`] the writer thread stores it
/// **after it has closed the file** (so after the lock of ADR-0154 decision 1
/// is gone) and before it lowers the retired-writer count; under
/// [`Durability::Fsync`] the journal's `Drop` stores it after dropping its
/// file. [`Released::is_released`] is one atomic load — no system call, no
/// allocation, nothing that can sleep — which is what lets a
/// [`crate::recovery::Recovery::ready`] answer from it on the engine thread.
///
/// **What it is for.** A recovery keeps the handle of the journal it handed
/// out for each counterparty and answers `ready` with `is_released()`; a
/// reconnect whose last session's writer is still flushing is then parked,
/// and admitted the first time the engine asks after the writer is done.
/// [ADR-0155] decisions 2 and 3; `tests/one_appender.rs::ready_is_answered_by_the_writer_not_the_filesystem`.
///
/// [ADR-0155]: ../../../docs/decisions/ADR-0155-a-recovery-learns-its-writer-let-go-from-the-writer-not-from-the-filesystem.md
#[derive(Debug, Clone)]
pub struct Released(Arc<AtomicBool>);

impl Released {
    /// A flag nobody has set, and the one [`Releaser`] that can set it.
    ///
    /// For a journal outside this crate whose writer lets go of its storage
    /// the way [`FileJournal`]'s does: make the pair at open (one `Arc`, off
    /// the engine thread), move the [`Releaser`] to the writer, hand out
    /// clones of the [`Released`]. ADR-0181 decision 2.
    #[must_use]
    pub fn pair() -> (Releaser, Self) {
        let flag = Arc::new(AtomicBool::new(false));
        (Releaser(Arc::clone(&flag)), Self(flag))
    }

    /// `true` once the journal this came from has closed its file. One
    /// `Acquire` load.
    #[must_use]
    pub fn is_released(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

/// The one handle that can turn a [`Released`] `true`, made with it by
/// [`Released::pair`].
///
/// Not `Clone`, and [`Releaser::release`] consumes it, so only its owner —
/// the writer — can say it let go, and only once. The writer calls it **after
/// its storage is closed** (so after any lock that storage held is gone) and
/// **before** [`WriterTicket::finish`]: ADR-0155 decision 2's order.
#[derive(Debug)]
pub struct Releaser(Arc<AtomicBool>);

impl Releaser {
    /// Say the storage has been let go. One `Release` store.
    pub fn release(self) {
        self.0.store(true, Ordering::Release);
    }
}

/// Let go of the lock [`take_the_file`] took, **explicitly, before the file is
/// closed**.
///
/// Closing the descriptor would release it too — unless another copy of that
/// descriptor exists, and one does, briefly, whenever any thread of this
/// process spawns a child: the child holds a duplicate of every descriptor
/// until its `exec` closes them (`O_CLOEXEC`), and a lock taken with `flock`
/// belongs to the open file, not to the descriptor. Unlocking releases it for
/// every copy at once. `[measured 2026-09-24]` `tests/one_appender.rs` saw
/// `WouldBlock` from an `open` that `file_busy` had just called free, in 2 runs
/// of 15, in a binary where a sibling test spawns a process; the spawn is the
/// `[inferred]` cause — docs/reference/a-reconnect-reopened-a-journal-its-retired-writer-still-owned.md.
/// ADR-0154 decision 1.
fn release_the_file(file: &File) {
    let _ = file.unlock();
}

/// Whether another appender holds the journal at `path` — so that
/// [`FileJournal::open`] would refuse it with `WouldBlock` right now.
///
/// **Not for [`crate::recovery::Recovery::ready`] in the single-engine
/// `serve*` loops**, which ask `ready` on the engine thread: `open(2)` walks a
/// path and can sleep in the kernel (a directory lock, an allocation, metadata
/// not in cache), and at once per millisecond per parked connection that is a
/// sleep on the `hft` hot path. Answer `ready` from [`FileJournal::released`]
/// instead. This is for tools and for the sharded runtime's acceptor thread,
/// which may block. [ADR-0155] decision 4.
///
/// It never *waits for the lock*: open, `try_lock`, unlock, close. A path that does not exist, or cannot be opened, is not busy
/// (`open` will say what is wrong with it). A free file is locked for the
/// instant between the `try_lock` and the unlock, so a second process looking
/// at exactly that moment may see it busy once.
///
/// [ADR-0155]: ../../../docs/decisions/ADR-0155-a-recovery-learns-its-writer-let-go-from-the-writer-not-from-the-filesystem.md
#[must_use]
pub fn file_busy(path: &Path) -> bool {
    let Ok(file) = File::open(path) else {
        return false;
    };
    match file.try_lock() {
        // Free: it is this look's for an instant, and let go explicitly — see
        // `release_the_file` for why closing is not enough.
        Ok(()) => {
            release_the_file(&file);
            false
        }
        Err(std::fs::TryLockError::WouldBlock) => true,
        Err(std::fs::TryLockError::Error(_)) => false,
    }
}

/// The record that means *stop*, and nothing else: one byte.
///
/// **Not a zero-length record.** `Consumer::pop` reports a record longer than
/// the buffer it was handed as `Some(0)`, and until 2026-09-23 the writer read
/// that as this signal — so one message longer than its fixed 4 096-byte buffer
/// stopped it for good while `put` kept answering `true`. Every record this
/// journal writes is at least `RECORD_HEADER` (8) bytes, so one byte cannot be
/// mistaken for one. The message log's `STOP` is the same rule. ADR-0150
/// decision 2; `writer_tests::a_record_the_writer_cannot_hold_does_not_stop_it`.
const STOP: u8 = 0xFF;

/// The writer's buffer for a journal whose slot holds `len` bytes: the largest
/// record the ring can carry, header included.
///
/// `MemJournal::put` refuses a message longer than `len` before the ring is
/// touched, so every message record fits. The two marks are fixed-size and
/// counted too, so a slot shorter than a mark does not drop the mark. ADR-0150
/// decision 1; `an_async_journal_keeps_a_message_longer_than_four_kilobytes_and_all_that_follow`.
const fn writer_buf(len: usize) -> usize {
    let mut most = len;
    if ACTIVITY_LEN > most {
        most = ACTIVITY_LEN;
    }
    if OUTBOUND_LEN > most {
        most = OUTBOUND_LEN;
    }
    RECORD_HEADER + most
}

/// The writer thread: everything the ring hands over, appended in order.
///
/// `buf` is `writer_buf(LEN)` bytes from `FileJournal`, allocated once on the
/// writer thread before this is called — not the engine thread (ADR-0037), and
/// a stack array sized by `LEN` needs an unstable feature. A test passes a
/// smaller one to reach the `Some(0)` arm. ADR-0150 decision 1.
///
/// It returns at `STOP`, or — once `told` says [`RETIRED_STOP_WHEN_DRY`] — the
/// first time the ring is found empty after that was read. ADR-0153 decision 3.
fn write_until_told(
    mut file: File,
    mut from_engine: Consumer,
    format: Format,
    mut buf: Vec<u8>,
    told: &AtomicU8,
) {
    // Spin briefly, then sleep 1 ms per empty poll: the writer gives its core
    // back when there is nothing to write, in every mode. ADR-0150 decision 4.
    let mut idle = Idle::new();
    // Set once `told` has been read as `RETIRED_STOP_WHEN_DRY`. Everything the
    // engine pushed happened before it said so, so after one more empty pop
    // nothing is left to come.
    let mut last_look = false;
    loop {
        let popped = from_engine.pop(&mut buf);
        if popped.is_some() {
            idle.reset();
        }
        match popped {
            // **Not the stop signal.** `pop` says *"a record longer than this
            // buffer was dropped"* this way. Unreachable from `FileJournal`,
            // whose buffer holds its largest record; if it is reached anyway,
            // one record is lost and the writer carries on, which is a gap
            // fill on a resend rather than a journal that silently stops.
            Some(0) => continue,
            // The stop signal. Everything before it has already been written,
            // because the ring is ordered.
            Some(1) if buf.first() == Some(&STOP) => {
                let _ = file.flush();
                release_the_file(&file);
                return;
            }
            Some(n) => {
                // `pop` never writes more than the buffer it was handed, so
                // this is `Some` — but a `None` here would be a silent partial
                // record on disk, and skipping the pop is the only answer that
                // does not write one. `indexing_slicing`, 2026-09-08.
                let Some(record) = buf.get(..n) else { continue };
                // **A message carrying a secret never reaches the file**, not
                // even masked: replayed after a restart, `554=********` would be
                // a wrong password on the wire. The mark for its number goes in
                // its place, so a restart still knows the number was spent and
                // gap-fills it. Decided here, on the writer, so the engine
                // thread under `Async` is unchanged. ADR-0110 decision 4.
                let mark;
                let record = match carries_secret_at(record) {
                    Some(seq) => {
                        mark = outbound_mark(seq);
                        mark.as_slice()
                    }
                    None => record,
                };
                let _ = file.write_all(record);
                // **The checksum is computed here, not on the engine thread.**
                // `Async` exists to keep work off that thread, and a CRC over a
                // 200-byte record is ~100 ns of it. `Fsync` has no writer to
                // hand it to and pays it inline, which is the smaller half of
                // what that mode already costs.
                if format == Format::V1 {
                    let _ = file.write_all(&crc32(&[record]).to_le_bytes());
                }
            }
            None if last_look => {
                let _ = file.flush();
                release_the_file(&file);
                return;
            }
            None if told.load(Ordering::Acquire) == RETIRED_STOP_WHEN_DRY => last_look = true,
            None => idle.wait(),
        }
    }
}

/// The writer loop as a writer nobody retires runs it: until `STOP`.
#[cfg(test)]
fn write_loop(file: File, from_engine: Consumer, format: Format, buf: Vec<u8>) {
    write_until_told(file, from_engine, format, buf, &AtomicU8::new(RUNNING));
}

impl<const N: usize, const LEN: usize> Journal for FileJournal<N, LEN> {
    fn put(&mut self, seq: u32, bytes: &[u8]) -> bool {
        let kept = self.mem.put(seq, bytes);
        if !kept {
            // Refused by the ring is refused outright: a message this journal
            // cannot answer `get` for must not reach the file either, or a
            // recovery would read back a message the running engine could never
            // have replayed.
            return false;
        }
        match self.how {
            Durability::Async => {
                if let Some(p) = self.to_writer.as_mut() {
                    // A full ring is a message that does not reach the file.
                    // It is dropped rather than waited on: waiting would put a
                    // disk's latency on the engine thread, which is the whole
                    // thing `Async` exists to avoid. **Counted, and `put` still
                    // answers `true`**: memory holds it and a resend while this
                    // process runs replays it, so telling the session it was
                    // not kept would make `puts_refused` lie. What is missing
                    // is the restart's copy, and `unwritten` says so. ADR-0154
                    // decision 4.
                    let n = u32::try_from(bytes.len()).unwrap_or(0);
                    if !p.push(&[&seq.to_le_bytes(), &n.to_le_bytes(), bytes]) {
                        self.unwritten += 1;
                    }
                }
            }
            Durability::Fsync => {
                if let Some(f) = self.file.as_mut() {
                    // ADR-0110 decision 4, the same rule `write_loop` applies
                    // under `Async`, made here because `Fsync` has no writer:
                    // a message carrying a secret leaves only the mark for its
                    // number. One scan on the engine thread, which allocates
                    // nothing (`benches/alloc.rs` case `redact-scan`); what it
                    // costs in time is `[unmeasured]`.
                    if crate::redact::carries_secret(bytes) {
                        let rec = outbound_mark(seq);
                        let _ = f.write_all(&rec);
                        if self.format == Format::V1 {
                            let _ = f.write_all(&crc32(&[&rec]).to_le_bytes());
                        }
                        let _ = f.sync_data();
                        return true;
                    }
                    let mut rec = [0u8; RECORD_HEADER];
                    rec[..RECORD_SEQ].copy_from_slice(&seq.to_le_bytes());
                    let n = u32::try_from(bytes.len()).unwrap_or(0);
                    rec[RECORD_SEQ..].copy_from_slice(&n.to_le_bytes());
                    let _ = f.write_all(&rec);
                    let _ = f.write_all(bytes);
                    if self.format == Format::V1 {
                        // Over the pieces, never over a joined buffer: joining
                        // them here would allocate on the engine thread, which
                        // is the one thing this module may not do.
                        let _ = f.write_all(&crc32(&[&rec, bytes]).to_le_bytes());
                    }
                    let _ = f.sync_data();
                }
            }
        }
        true
    }

    fn get(&self, seq: u32) -> Option<&[u8]> {
        self.mem.get(seq)
    }

    fn oldest(&self) -> Option<u32> {
        self.mem.oldest()
    }

    fn highest(&self) -> Option<u32> {
        self.mem.highest()
    }

    /// Tell the writer to finish and let it go, **without waiting for it**.
    ///
    /// Called on the engine thread when a connection ends mid-serving
    /// (`Connection`'s `Drop`), so it makes no syscall, allocates nothing and
    /// never spins: `STOP` is pushed **once**, and if the ring is full the
    /// writer is told instead to stop when it next finds the ring empty. The
    /// writer's handle is dropped (the thread is detached) and it is counted
    /// among the retired writers [`wait_for_retired_writers`] waits for after
    /// serving. A second call, `Fsync`, or a journal already closed does
    /// nothing. ADR-0153 decision 3.
    fn retire(&mut self) {
        let (Some(ticket), Some(writer)) = (self.ticket.as_ref(), self.writer.take()) else {
            return;
        };
        // *The rule for a journal*, ADR-0181 decision 1: push `STOP` once,
        // then retire with the push's result. A writer that pops `STOP` and
        // finishes before the `retire` below finishes a *running* ticket,
        // which lowers nothing, and the `retire` then counts nothing. Told
        // `RetiredStopWhenDry` instead — after every record this journal
        // pushed — the writer that then finds the ring empty has written them
        // all.
        let pushed = self.to_writer.as_mut().is_some_and(|p| p.push(&[&[STOP]]));
        let _ = ticket.retire(pushed);
        // Detached: nobody joins it; `wait_for_retired_writers` waits for it.
        drop(writer);
        self.to_writer = None;
    }

    fn mark_active(&mut self, at_ms: u64) {
        self.last_active = Some(at_ms);
        // The same two tiers as everything else here. This is written at logon
        // and at an ordered shutdown, **never per message**, so even `Fsync`'s
        // `sync_data` is paid twice in a session's life rather than per
        // message — which is what makes it affordable at all.
        match self.how {
            Durability::Async => {
                if let Some(p) = self.to_writer.as_mut() {
                    let n = u32::try_from(ACTIVITY_LEN).unwrap_or(0);
                    // A mark the ring had no room for is a hole in the file
                    // like a message is, and counted the same way.
                    if !p.push(&[
                        &ACTIVITY_MARK.to_le_bytes(),
                        &n.to_le_bytes(),
                        &at_ms.to_le_bytes(),
                    ]) {
                        self.unwritten += 1;
                    }
                }
            }
            Durability::Fsync => {
                if let Some(f) = self.file.as_mut() {
                    let mut rec = [0u8; RECORD_HEADER + ACTIVITY_LEN];
                    rec[..RECORD_SEQ].copy_from_slice(&ACTIVITY_MARK.to_le_bytes());
                    let n = u32::try_from(ACTIVITY_LEN).unwrap_or(0);
                    rec[RECORD_SEQ..RECORD_HEADER].copy_from_slice(&n.to_le_bytes());
                    rec[RECORD_HEADER..].copy_from_slice(&at_ms.to_le_bytes());
                    let _ = f.write_all(&rec);
                    if self.format == Format::V1 {
                        let _ = f.write_all(&crc32(&[&rec]).to_le_bytes());
                    }
                    let _ = f.sync_data();
                }
            }
        }
    }

    fn last_active(&self) -> Option<u64> {
        self.last_active
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
                if let Some(p) = self.to_writer.as_mut()
                    && !p.push(&[&seq.to_le_bytes(), &0u32.to_le_bytes()])
                {
                    self.unwritten += 1;
                }
            }
            Durability::Fsync => {
                if let Some(f) = self.file.as_mut() {
                    let mut rec = [0u8; RECORD_HEADER];
                    rec[..RECORD_SEQ].copy_from_slice(&seq.to_le_bytes());
                    rec[RECORD_SEQ..].copy_from_slice(&0u32.to_le_bytes());
                    let _ = f.write_all(&rec);
                    if self.format == Format::V1 {
                        let _ = f.write_all(&crc32(&[&rec]).to_le_bytes());
                    }
                    let _ = f.sync_data();
                }
            }
        }
    }

    fn highest_in(&self) -> Option<u32> {
        self.mem.highest_in()
    }

    fn mark_out(&mut self, seq: u32) {
        // **The guard is here, not at the call site.** The session tells this
        // the same high-water mark on every turn, so without the comparison a
        // quiet session under `Fsync` would `sync_data` once per turn for a
        // number that has not moved. ADR-0053.
        if self.mem.highest_out().is_some_and(|h| h >= seq) {
            return;
        }
        self.mem.mark_out(seq);
        // The same two tiers as `mark_in`, and the same cost: under `Fsync` an
        // administrative message now pays a `sync_data` where it used to pay
        // nothing. That is ADR-0017's price arriving on the outbound side, and
        // `Async` — the default — keeps it off the engine thread.
        match self.how {
            Durability::Async => {
                if let Some(p) = self.to_writer.as_mut()
                    && !p.push(&[&outbound_mark(seq)])
                {
                    self.unwritten += 1;
                }
            }
            Durability::Fsync => {
                if let Some(f) = self.file.as_mut() {
                    let rec = outbound_mark(seq);
                    let _ = f.write_all(&rec);
                    if self.format == Format::V1 {
                        let _ = f.write_all(&crc32(&[&rec]).to_le_bytes());
                    }
                    let _ = f.sync_data();
                }
            }
        }
    }

    fn highest_out(&self) -> Option<u32> {
        self.mem.highest_out()
    }

    /// Records kept in memory whose push to the writer's ring found it full,
    /// so the file never got them — messages and the three marks alike.
    /// Always zero under [`Durability::Fsync`], which writes before it returns.
    /// A counter bump on the engine thread: no syscall, no allocation
    /// (`benches/alloc.rs`). ADR-0154 decision 4.
    fn unwritten(&self) -> u64 {
        self.unwritten
    }
}

// --- reading the file from outside the engine ----------------------------

/// One record in a journal file.
///
/// The four shapes the format has: a message, ADR-0017's inbound mark,
/// ADR-0033's activity mark, and ADR-0053's outbound mark.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Record<'a> {
    /// A message this session sent, with the number it went out under.
    Message {
        /// `34=` on the message.
        seq: u32,
        /// The bytes exactly as they went on the wire.
        bytes: &'a [u8],
    },
    /// The highest inbound sequence number seen at that point.
    ///
    /// Encoded as a record of length zero, which a FIX message can never be —
    /// see `INBOUND_MARK`. [ADR-0017] needs this beside the outbound
    /// messages rather than in a file of its own.
    ///
    /// [ADR-0017]: ../../../docs/decisions/ADR-0017-the-inbound-count-is-persisted-after-delivery.md
    InboundMark {
        /// The inbound number recorded.
        seq: u32,
    },
    /// When the session was last known to be alive, in milliseconds on the
    /// engine's clock.
    ///
    /// Encoded as a record whose **sequence number** is zero — see
    /// `ACTIVITY_MARK`. A file written before this existed simply has none,
    /// and reads exactly as it always did.
    ActivityMark {
        /// The instant recorded.
        at_ms: u64,
    },
    /// The highest **outbound** sequence number spent at that point, including
    /// the administrative messages the journal holds no bytes for.
    ///
    /// Encoded as a record whose sequence number is zero and whose length is
    /// four — see `OUTBOUND_LEN`. This is what a restart's `next_out` is
    /// derived from, and a file written before it existed simply has none.
    /// [ADR-0053]
    ///
    /// [ADR-0053]: ../../../docs/decisions/ADR-0053-the-journal-answers-two-questions-and-the-second-is-a-number.md
    OutboundMark {
        /// The highest outbound number spent.
        seq: u32,
    },
}

impl Record<'_> {
    /// The sequence number, whichever shape this is.
    ///
    /// An [`Record::ActivityMark`] answers `0`, which is the number it is
    /// written under and is not a sequence number FIX has. An
    /// [`Record::OutboundMark`] answers the **number it carries**, not the
    /// zero it is written under: that number is the point of the record.
    #[must_use]
    pub const fn seq(&self) -> u32 {
        match *self {
            Self::Message { seq, .. } | Self::InboundMark { seq } | Self::OutboundMark { seq } => {
                seq
            }
            Self::ActivityMark { .. } => ACTIVITY_MARK,
        }
    }
}

/// Reads a journal file from outside the process that wrote it.
///
/// # Why this is not [`FileJournal`]
///
/// `FileJournal` exists for **recovery**: it reloads the file into a fixed ring
/// of `N` messages, because what it has to answer is the next `ResendRequest`,
/// and that is about recent traffic. This exists for the other question — *"we
/// sent order X at 10:32, did you receive it?"* — which is about a message that
/// may be very old, and which the ring dropped long ago. No `N`, no `LEN`, no
/// bound.
///
/// # It allocates, and that is allowed
///
/// The whole file is read into memory. **Nothing here runs on the engine
/// thread or on any hot path** — non-negotiable 1 is about the engine, and this
/// is a tool. A file too large to hold is a real limit and it is named in
/// `GUIDE.md` rather than worked around.
///
/// # It does not interpret FIX
///
/// Records come back as bytes. Interpreting them needs a dictionary, and a
/// program that reads a file has no business pulling one in.
#[derive(Debug)]
pub struct Reader {
    bytes: Vec<u8>,
    torn: usize,
    format: Format,
    corrupt: usize,
}

impl Reader {
    /// Read the whole file.
    ///
    /// # Errors
    ///
    /// Whatever reading the file returns.
    pub fn open(path: &Path) -> std::io::Result<Self> {
        let bytes = std::fs::read(path)?;
        let format = if bytes.get(..HEADER_V1.len()) == Some(HEADER_V1) {
            Format::V1
        } else {
            Format::V0
        };
        let mut at = if format == Format::V1 {
            HEADER_V1.len()
        } else {
            0
        };
        let mut corrupt = 0usize;
        while at + RECORD_HEADER <= bytes.len() {
            let mut l4 = [0u8; 4];
            let Some(lb) = bytes.get(at + RECORD_SEQ..at + RECORD_HEADER) else {
                break;
            };
            l4.copy_from_slice(lb);
            let len = u32::from_le_bytes(l4) as usize;
            let Some(end) = at
                .checked_add(RECORD_HEADER)
                .and_then(|x| x.checked_add(len))
            else {
                break;
            };
            let whole = if format == Format::V1 {
                match end.checked_add(RECORD_CRC) {
                    Some(w) => w,
                    None => break,
                }
            } else {
                end
            };
            if whole > bytes.len() {
                break;
            }
            if format == Format::V1 {
                let mut c4 = [0u8; RECORD_CRC];
                let (Some(cb), Some(body)) = (bytes.get(end..whole), bytes.get(at..end)) else {
                    break;
                };
                c4.copy_from_slice(cb);
                if u32::from_le_bytes(c4) != crc32(&[body]) {
                    corrupt = 1;
                    break;
                }
            }
            at = whole;
        }
        let torn = bytes.len() - at;
        Ok(Self {
            bytes,
            torn,
            format,
            corrupt,
        })
    }

    /// Records whose stored checksum did not match their bytes.
    ///
    /// **Zero or one**, and always zero on a version-0 file — see
    /// [`FileJournal::corrupt_records`], which says the same thing for the
    /// writing side. Non-zero means everything after that point is not shown
    /// and not to be trusted, exactly as a torn tail is not.
    #[must_use]
    pub const fn corrupt_records(&self) -> usize {
        self.corrupt
    }

    /// Every whole record, in the order they were written.
    #[must_use]
    pub fn records(&self) -> Records<'_> {
        Records {
            bytes: &self.bytes,
            at: if self.format == Format::V1 {
                HEADER_V1.len()
            } else {
                0
            },
            format: self.format,
        }
    }

    /// Bytes at the end that do not form a whole record.
    ///
    /// **Zero on a file written by a process that exited cleanly.** Anything
    /// else is a process killed mid-write, and an audit that does not mention
    /// it is an audit that quietly lost something.
    #[must_use]
    pub const fn torn_tail_bytes(&self) -> usize {
        self.torn
    }

    /// How many bytes the file holds, torn tail included.
    #[must_use]
    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    /// Whether the file holds nothing to read.
    ///
    /// `[changed 2026-09-04]` **This is "no records and no torn tail", not
    /// "zero bytes".** A version-1 journal opened and never written to is five
    /// bytes of header, and a session that has sent nothing must not read as a
    /// file with something in it — the distinction the caller actually wants is
    /// *is there anything here to look at*, and the header is not.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.bytes.len() <= self.header_len() && self.torn == 0
    }

    /// How many bytes precede the first record. Five for version 1, zero for
    /// version 0.
    const fn header_len(&self) -> usize {
        match self.format {
            Format::V1 => HEADER_V1.len(),
            Format::V0 => 0,
        }
    }
}

/// Every whole record in a [`Reader`]'s file. See [`Reader::records`].
#[derive(Debug, Clone)]
pub struct Records<'a> {
    bytes: &'a [u8],
    at: usize,
    format: Format,
}

impl<'a> Iterator for Records<'a> {
    type Item = Record<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let at = self.at;
        if at + RECORD_HEADER > self.bytes.len() {
            return None;
        }
        let mut s4 = [0u8; 4];
        let mut l4 = [0u8; 4];
        s4.copy_from_slice(self.bytes.get(at..at + RECORD_SEQ)?);
        l4.copy_from_slice(self.bytes.get(at + RECORD_SEQ..at + RECORD_HEADER)?);
        let seq = u32::from_le_bytes(s4);
        let len = u32::from_le_bytes(l4) as usize;
        let end = at.checked_add(RECORD_HEADER)?.checked_add(len)?;
        let whole = if self.format == Format::V1 {
            end.checked_add(RECORD_CRC)?
        } else {
            end
        };
        if whole > self.bytes.len() {
            // The torn tail. `Reader::torn_tail_bytes` is where it is reported;
            // stopping here without saying so is the defect this whole type
            // exists to avoid, and the reader carries the count for exactly
            // that reason.
            return None;
        }
        if self.format == Format::V1 {
            let mut c4 = [0u8; RECORD_CRC];
            c4.copy_from_slice(self.bytes.get(end..whole)?);
            if u32::from_le_bytes(c4) != crc32(&[self.bytes.get(at..end)?]) {
                // Same rule as `FileJournal::open_with`: stop, and let
                // `Reader::corrupt_records` be what says why. `tools/jrnl`
                // turns that into a warning and exit 2, the same as a tear.
                return None;
            }
        }
        self.at = whole;
        if seq == ACTIVITY_MARK && len == ACTIVITY_LEN {
            let mut t = [0u8; ACTIVITY_LEN];
            t.copy_from_slice(self.bytes.get(at + RECORD_HEADER..end)?);
            Some(Record::ActivityMark {
                at_ms: u64::from_le_bytes(t),
            })
        } else if seq == ACTIVITY_MARK && len == OUTBOUND_LEN {
            let mut n = [0u8; OUTBOUND_LEN];
            n.copy_from_slice(self.bytes.get(at + RECORD_HEADER..end)?);
            Some(Record::OutboundMark {
                seq: u32::from_le_bytes(n),
            })
        } else if len == INBOUND_MARK {
            Some(Record::InboundMark { seq })
        } else {
            Some(Record::Message {
                seq,
                bytes: self.bytes.get(at + RECORD_HEADER..end)?,
            })
        }
    }
}

/// `write_loop` called directly, because the first half of ADR-0150 makes the
/// case it guards unreachable through `FileJournal`.
///
/// Once the writer's buffer holds the largest record the slot allows, `pop`
/// never answers `Some(0)` for a journal record, so a `write_loop` that went
/// back to stopping on `Some(0)` would leave every test through the public API
/// green. Here the buffer is small on purpose and the record that does not fit
/// is put in front of one that does. ADR-0150 decision 2.
#[cfg(test)]
mod writer_tests {
    use super::{Format, RECORD_HEADER, RETIRED_STOP_WHEN_DRY, STOP, write_loop, write_until_told};
    use crate::ring::pair;
    use std::sync::atomic::AtomicU8;

    /// A writer retired while its ring was full had no `STOP` pushed. It stops
    /// the first time it finds the ring empty — having written everything in
    /// it. On this thread, so a writer that never stops hangs the test rather
    /// than passing it. ADR-0153 decision 3.
    #[test]
    fn a_writer_retired_with_a_full_ring_stops_once_it_is_dry() -> std::io::Result<()> {
        let path = std::env::temp_dir().join(format!(
            "fixbolt-journal-writer-tests-dry-{}.log",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        let file = std::fs::File::create(&path)?;

        let (mut to_writer, from_engine) = pair(1 << 12);
        let mut expected = Vec::new();
        for seq in 1u32..=3 {
            let body = *b"35=0";
            let len = u32::try_from(body.len()).unwrap_or(0).to_le_bytes();
            assert!(to_writer.push(&[&seq.to_le_bytes(), &len, &body]));
            expected.extend_from_slice(&seq.to_le_bytes());
            expected.extend_from_slice(&len);
            expected.extend_from_slice(&body);
        }
        let told = AtomicU8::new(RETIRED_STOP_WHEN_DRY);
        write_until_told(file, from_engine, Format::V0, vec![0u8; 64], &told);

        let on_disk = std::fs::read(&path)?;
        let _ = std::fs::remove_file(&path);
        assert_eq!(
            on_disk, expected,
            "a writer told to stop when dry wrote everything before it stopped"
        );
        Ok(())
    }

    /// `?` rather than `expect`: non-negotiable 7 is a workspace lint and this
    /// module is inside the library crate.
    #[test]
    fn a_record_the_writer_cannot_hold_does_not_stop_it() -> std::io::Result<()> {
        let path = std::env::temp_dir().join(format!(
            "fixbolt-journal-writer-tests-{}.log",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        let file = std::fs::File::create(&path)?;

        let small = RECORD_HEADER + 16;
        let (mut to_writer, from_engine) = pair(1 << 12);
        let too_long = [b'x'; 64];
        let long_len = u32::try_from(too_long.len()).unwrap_or(0).to_le_bytes();
        assert!(to_writer.push(&[&7u32.to_le_bytes(), &long_len, &too_long]));
        let fits = *b"35=D";
        let fits_len = u32::try_from(fits.len()).unwrap_or(0).to_le_bytes();
        assert!(to_writer.push(&[&8u32.to_le_bytes(), &fits_len, &fits]));
        assert!(to_writer.push(&[&[STOP]]));

        // On this thread: the loop returns at `STOP`, so a writer that stops
        // anywhere else returns early and leaves the file short, rather than
        // hanging the test.
        write_loop(file, from_engine, Format::V0, vec![0u8; small]);

        let on_disk = std::fs::read(&path)?;
        let _ = std::fs::remove_file(&path);
        let mut expected = Vec::new();
        expected.extend_from_slice(&8u32.to_le_bytes());
        expected.extend_from_slice(&fits_len);
        expected.extend_from_slice(&fits);
        assert_eq!(
            on_disk, expected,
            "the writer stopped at the record it could not hold, and the one after it never reached the file"
        );
        Ok(())
    }
}
