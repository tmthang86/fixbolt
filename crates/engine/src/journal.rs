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
        // **Addressed by the number, not by a write cursor.** One slot can
        // hold one sequence number at a time, so `get` is an index and a
        // comparison rather than a scan of all `N` — which at 4096 slots is
        // what makes a 1000-message resend affordable instead of four million
        // comparisons on the engine thread. ADR-0046 decision 3.
        let slot = &mut self.slots[(seq as usize) % N];
        slot.seq = seq;
        slot.len = u16::try_from(bytes.len()).unwrap_or(0);
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
        let slot = &self.slots[(seq as usize) % N];
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
    /// `Fsync` writes here. `Async` leaves it `None` and uses the ring.
    file: Option<File>,
    to_writer: Option<Producer>,
    /// The writer thread, joined on drop so a test can read the file after.
    writer: Option<std::thread::JoinHandle<()>>,
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
    /// # Errors
    ///
    /// Whatever opening the file returns.
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
        let existing = std::fs::read(path).unwrap_or_default();
        // A file that is not there yet is version 1; one that is there is
        // whatever its first five bytes say, for ever.
        let has_header =
            existing.len() >= HEADER_V1.len() && &existing[..HEADER_V1.len()] == HEADER_V1;
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
                s4.copy_from_slice(&bytes[at..at + RECORD_SEQ]);
                l4.copy_from_slice(&bytes[at + RECORD_SEQ..at + RECORD_HEADER]);
                let seq = u32::from_le_bytes(s4);
                let len = u32::from_le_bytes(l4) as usize;
                let end = at + RECORD_HEADER + len;
                // A version-1 record carries its checksum after the payload, so
                // "the whole record" is four bytes longer.
                let whole = if format == Format::V1 {
                    end + RECORD_CRC
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
                    c4.copy_from_slice(&bytes[end..whole]);
                    if u32::from_le_bytes(c4) != crc32(&[&bytes[at..end]]) {
                        corrupt = 1;
                        torn = bytes.len() - at;
                        break;
                    }
                }
                if seq == ACTIVITY_MARK && len == ACTIVITY_LEN {
                    let mut t = [0u8; ACTIVITY_LEN];
                    t.copy_from_slice(&bytes[at + RECORD_HEADER..end]);
                    // **The latest wins, not the first.** They are appended in
                    // order, so the last one is the one that describes the
                    // session at the moment it stopped.
                    last_active = Some(u64::from_le_bytes(t));
                } else if seq == ACTIVITY_MARK && len == OUTBOUND_LEN {
                    let mut n = [0u8; OUTBOUND_LEN];
                    n.copy_from_slice(&bytes[at + RECORD_HEADER..end]);
                    // `mark_out` takes the max, so the order these are read in
                    // does not matter and a wound-back count does not lower it.
                    mem_recovered.mark_out(u32::from_le_bytes(n));
                } else if len == INBOUND_MARK {
                    mem_recovered.mark_in(seq);
                } else {
                    mem_recovered.put(seq, &bytes[at + RECORD_HEADER..end]);
                }
                at = whole;
            }
            if at + RECORD_HEADER > bytes.len() && at < bytes.len() {
                // A tail too short to even hold a header is torn too.
                torn = bytes.len() - at;
            }
        }
        let mut file = File::options().create(true).append(true).open(path)?;
        // The header goes on a file that had nothing in it. Written before any
        // record, so a reader never sees a record without one.
        if format == Format::V1 && existing.is_empty() {
            file.write_all(HEADER_V1)?;
            file.flush()?;
        }
        let (mem, mut this) = (
            mem_recovered,
            Self {
                mem: MemJournal::new(),
                how,
                file: None,
                to_writer: None,
                writer: None,
                #[cfg(all(feature = "affinity", target_os = "linux"))]
                writer_core: None,
                torn,
                format,
                corrupt,
                last_active,
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
                #[cfg(all(feature = "affinity", target_os = "linux"))]
                if let Some(core) = core {
                    let (handle, on) =
                        crate::affinity::spawn_pinned("fixbolt-journal", core, move || {
                            write_loop(file, from_engine, format)
                        })?;
                    this.writer = Some(handle);
                    this.writer_core = Some(on);
                    return Ok(this);
                }
                this.writer = Some(std::thread::spawn(move || {
                    write_loop(file, from_engine, format)
                }));
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
fn write_loop(mut file: File, mut from_engine: Consumer, format: Format) {
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
                // **The checksum is computed here, not on the engine thread.**
                // `Async` exists to keep work off that thread, and a CRC over a
                // 200-byte record is ~100 ns of it. `Fsync` has no writer to
                // hand it to and pays it inline, which is the smaller half of
                // what that mode already costs.
                if format == Format::V1 {
                    let _ = file.write_all(&crc32(&[&buf[..n]]).to_le_bytes());
                }
            }
            None => std::hint::spin_loop(),
        }
    }
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
                    let _ = p.push(&[
                        &ACTIVITY_MARK.to_le_bytes(),
                        &n.to_le_bytes(),
                        &at_ms.to_le_bytes(),
                    ]);
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
                if let Some(p) = self.to_writer.as_mut() {
                    let n = u32::try_from(OUTBOUND_LEN).unwrap_or(0);
                    let _ = p.push(&[
                        &ACTIVITY_MARK.to_le_bytes(),
                        &n.to_le_bytes(),
                        &seq.to_le_bytes(),
                    ]);
                }
            }
            Durability::Fsync => {
                if let Some(f) = self.file.as_mut() {
                    let mut rec = [0u8; RECORD_HEADER + OUTBOUND_LEN];
                    rec[..RECORD_SEQ].copy_from_slice(&ACTIVITY_MARK.to_le_bytes());
                    let n = u32::try_from(OUTBOUND_LEN).unwrap_or(0);
                    rec[RECORD_SEQ..RECORD_HEADER].copy_from_slice(&n.to_le_bytes());
                    rec[RECORD_HEADER..].copy_from_slice(&seq.to_le_bytes());
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
        let format = if bytes.len() >= HEADER_V1.len() && &bytes[..HEADER_V1.len()] == HEADER_V1 {
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
            l4.copy_from_slice(&bytes[at + RECORD_SEQ..at + RECORD_HEADER]);
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
                c4.copy_from_slice(&bytes[end..whole]);
                if u32::from_le_bytes(c4) != crc32(&[&bytes[at..end]]) {
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
