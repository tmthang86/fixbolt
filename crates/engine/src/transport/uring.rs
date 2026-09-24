//! An `io_uring` transport: receive is a multishot `recv` into a provided
//! buffer ring, reaped by the idle strategy — [ADR-0190].
//!
//! # The shape
//!
//! A [`Uring`] owns one ring, one provided-buffer ring and a fixed slab of
//! per-connection staging lists. It is `!Send` — one ring per engine thread,
//! which is what `IORING_SETUP_SINGLE_ISSUER` demands anyway: **the ring must
//! be made on the thread that will reap it.** From it come:
//!
//! - [`UringTransport`], one per connection, made by [`Uring::register`] from a
//!   [`TcpTransport`]. `recv` copies from what was already reaped and **makes
//!   no system call**; `send` is the same `write(2)` as before.
//! - [`UringSpin`] (`hft`) or `UringBlock` (`standard`): **the idle strategy
//!   is the reaper.** `idle` submits what is queued, enters the kernel —
//!   never waiting in `hft`, waiting with a timeout in `standard` — and moves
//!   every completion into its connection's staging list.
//!
//! # Blocked means refused, never a fallback
//!
//! [`Uring::hft`] and `Uring::standard` return [`UringRefused`] when the
//! kernel, a sysctl or a seccomp filter will not give this process a ring.
//! Nothing in this module falls back to `read(2)`: a transport that silently
//! became the kernel arm would publish a number about the wrong code path
//! (ADR-0190 decision 7).
//!
//! # Who owns each buffer — the ledger
//!
//! Every provided buffer is in exactly one place: **with the kernel** (in the
//! buffer ring, or selected by a `recv` whose completion has not been reaped),
//! or **in exactly one connection's staging list**. A completion moves one
//! from the first place to the second; [`UringTransport::recv`] moves it back
//! once every byte has been copied out. [`Uring::buffers_accounted_for`] checks
//! that, and `crates/engine/tests/uring.rs` checks it after every reap.
//!
//! # The `unsafe`, and what proves each block
//!
//! Miri cannot run `io_uring` and no sanitizer sees the kernel write into this
//! memory, so each block names the test that proves it (plan
//! `docs/plans/2026-09-24-p4-io-uring-transport.md`, *Bất biến* 8):
//!
//! | # | Where | Proved by |
//! |---|---|---|
//! | U1 | `Region`: page-aligned `alloc`/`dealloc`, pre-touch | `buffers_are_resident_before_the_first_message`; ASan (step 7) |
//! | U2 | `register_buf_ring_with_flags` (one ring per slot) | structural: the `Rc` every `UringTransport` holds, whose drop shuts its socket and cancels its `recv` first; `Drop` for `Inner` unregistering before the memory goes is a second line no test can tell apart; the canary checks only the end state |
//! | U3 | `SubmissionQueue::push` | `a_ring_that_runs_out_of_buffers_rearms_and_loses_nothing` |
//! | U4 | `Submitter::enter` | `a_message_arrives_through_the_ring`; the `standard` wake tests (step 3) |
//! | U5 | a slice from a CQE's `(buffer id, length)` | `staged_span` and its unit test; `sixty_four_connections_interleaved_are_byte_exact` |
//! | U6 | writing a buffer-ring entry and its `tail` | the ledger; the byte-exact tests |
//! | U7 | `shutdown(2)` on a connection's socket | **no `unsafe`**: `std::net::TcpStream::shutdown`; `a_closed_connection_is_seen_as_closed_and_its_peer_sees_fin` |
//!
//! [ADR-0190]: ../../../../docs/decisions/ADR-0190-the-io-uring-transport-is-reaped-by-the-idle-strategy-and-an-hft-turn-enters-the-kernel-once-without-waiting.md

use std::alloc::Layout;
use std::cell::RefCell;
use std::fmt;
use std::io;
use std::os::fd::AsRawFd;
use std::ptr::NonNull;
use std::rc::Rc;
use std::sync::atomic::{AtomicU16, Ordering};

use io_uring::types::BufRingEntry;
use io_uring::{IoUring, cqueue, opcode, squeue, types};

use crate::transport::{Interest, Io, Source, TcpTransport, Transport};
use crate::wait::Waiting;

/// The largest provided-buffer ring the kernel accepts: buffer ids are 16 bits
/// and a ring's entry count is at most `1 << 15`. The bound on
/// [`UringConfig::buffers_per_connection`].
pub const MAX_BUFFERS: u16 = 1 << 15;

/// How big the ring's receive side is: `connections × buffers_per_connection ×
/// buffer_len` bytes, allocated and pre-faulted when the ring is made
/// ([ADR-0192]). **No field has a hidden default** (`CLAUDE.md` §6): the caller
/// names all three and they are checked here. `tools/w2w` uses **8 × 4 096
/// bytes per connection** — twice the engine's default `RX` in flight —
/// and `docs/CONFIGURATION.md` says why.
///
/// [ADR-0192]: ../../../../docs/decisions/ADR-0192-each-io-uring-connection-draws-from-its-own-provided-buffer-ring.md
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UringConfig {
    buffers_per_connection: u16,
    buffer_len: u32,
    connections: u16,
}

/// Why a [`UringConfig`] was refused. Fieldless: nothing here allocates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UringConfigError {
    /// The buffers per connection are not a power of two in
    /// `2..=`[`MAX_BUFFERS`] — each connection's provided-buffer ring's entry
    /// count must be one, and a ring of one buffer cannot receive while that
    /// buffer is being read.
    Buffers,
    /// A buffer of zero bytes can hold nothing.
    BufferLen,
    /// A ring that can serve no connection.
    Connections,
}

impl fmt::Display for UringConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Buffers => {
                "the io_uring buffers per connection must be a power of two from 2 to 32768"
            }
            Self::BufferLen => "an io_uring buffer must be at least one byte long",
            Self::Connections => "an io_uring transport must serve at least one connection",
        })
    }
}

impl std::error::Error for UringConfigError {}

impl UringConfig {
    /// `buffers_per_connection` provided buffers of `buffer_len` bytes each,
    /// **for each** of at most `connections` registered connections — every
    /// connection draws from its own ring, so one that nobody reads cannot
    /// starve another (ADR-0192).
    ///
    /// # Errors
    ///
    /// [`UringConfigError`] naming the first field that is out of range.
    pub const fn new(
        buffers_per_connection: u16,
        buffer_len: u32,
        connections: u16,
    ) -> Result<Self, UringConfigError> {
        if !buffers_per_connection.is_power_of_two()
            || buffers_per_connection < 2
            || buffers_per_connection > MAX_BUFFERS
        {
            return Err(UringConfigError::Buffers);
        }
        if buffer_len == 0 {
            return Err(UringConfigError::BufferLen);
        }
        if connections == 0 {
            return Err(UringConfigError::Connections);
        }
        Ok(Self {
            buffers_per_connection,
            buffer_len,
            connections,
        })
    }

    /// How many provided buffers each connection's ring holds.
    #[must_use]
    pub const fn buffers_per_connection(&self) -> u16 {
        self.buffers_per_connection
    }

    /// How long each provided buffer is, in bytes.
    #[must_use]
    pub const fn buffer_len(&self) -> u32 {
        self.buffer_len
    }

    /// How many connections may be registered at once.
    #[must_use]
    pub const fn connections(&self) -> u16 {
        self.connections
    }

    /// Every connection's provided buffers together, in bytes —
    /// `connections × buffers_per_connection × buffer_len`, allocated and
    /// pre-faulted when the ring is made (the rings' own entries come on top;
    /// [`UringReport::buffer_bytes`] reads the whole allocation back).
    #[must_use]
    pub const fn buffer_bytes(&self) -> u64 {
        self.connections as u64 * self.buffers_per_connection as u64 * self.buffer_len as u64
    }
}

/// How an `hft` ring is reaped. [`HftArm::Enter`] unless the caller names the
/// other one — ADR-0190 decision 4, the owner's Q8.
///
/// **`HftArm::Sqpoll` exists only when the `affinity` feature is on too.**
/// Its core is an `crate::affinity::CoreId`, the type every other pinned
/// core in this crate has, and it is validated through
/// `crate::affinity::Topology` by the serving entry point before any socket
/// exists — the same check `serve_hft_pinned` makes. Without `affinity` the
/// variant is not there, so an unpinned SQ thread cannot be asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HftArm {
    /// Every idle turn is one `io_uring_enter(to_submit, 0, GETEVENTS)`: the
    /// engine thread enters the kernel and never waits there.
    #[default]
    Enter,
    /// A kernel thread polls the submission queue, pinned to `pin`'s core,
    /// and the engine thread only reads the completion queue. **Burns a second
    /// core.** *(ADR-0190 R1)* A [`crate::affinity::CorePin`], so the one
    /// waiver the engine's own core has — `allow_unisolated` — goes with it
    /// and is reported ([`UringReport::unisolated`]).
    #[cfg(feature = "affinity")]
    Sqpoll {
        /// The CPU the kernel's SQ thread is bound to (`sq_thread_cpu`), and
        /// whether it may be outside `isolcpus`.
        pin: crate::affinity::CorePin,
    },
}

/// Which reaping the ring is **actually** doing, read back from what the
/// kernel returned at setup rather than from what was asked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UringArm {
    /// `hft`, [`HftArm::Enter`].
    Enter,
    /// `hft`, `HftArm::Sqpoll`.
    Sqpoll,
    /// `standard`: the idle turn waits in `io_uring_enter` with a timeout.
    Block,
}

/// What a ring has done so far. Counts, never latencies (non-negotiable 10).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UringReport {
    /// The arm the kernel set up.
    pub arm: UringArm,
    /// Completions reaped.
    pub cqes: u64,
    /// Bytes delivered into staging lists.
    pub bytes: u64,
    /// Times a multishot `recv` ended on `-ENOBUFS` because every buffer was
    /// out.
    pub enobufs: u64,
    /// Times a multishot `recv` was submitted again after it ended.
    pub rearms: u64,
    /// Completions discarded because the connection they were for had been
    /// dropped — its slot's generation had moved on.
    pub stale: u64,
    /// `io_uring_enter` calls that returned an error other than `EINTR`.
    /// [`Waiting::idle`] returns `()`, so an error that cannot be returned is
    /// counted here rather than lost — the principle `block::Block` keeps.
    pub enter_errors: u64,
    /// `standard` only: sources a `POLL_ADD` could not be armed for because
    /// its table or the submission queue was full. Each one wakes the engine
    /// only by the timeout that turn — correct, and late, so it is counted.
    pub unarmed: u64,
    /// Completions the kernel could not post because the completion queue was
    /// full: the overflow flag seen at a reap plus the kernel's own dropped
    /// count. The queue is sized so this stays 0 (ADR-0190 R4), and the tests
    /// assert it.
    pub cq_overflow: u64,
    /// `true` only for [`UringArm::Sqpoll`] whose core was pinned with
    /// `CorePin::allow_unisolated` — a figure from that arm says so beside it
    /// (ADR-0190 R1).
    pub unisolated: bool,
    /// Times a flush could not empty the submission queue — an enter that
    /// failed, or an SQPOLL thread that did not take the entries within its
    /// bound. A connection dropped then keeps its descriptor open until a
    /// later flush succeeds, so no queued entry can name a reused number.
    pub unflushed: u64,
    /// Times a `UringTransport` was dropped while the ring was borrowed — a
    /// structural impossibility today; its descriptor is then leaked rather
    /// than closed under a live slot. Counted so it cannot happen unseen.
    pub drop_conflicts: u64,
    /// *(ADR-0192)* How many distinct connection slots have run out of their
    /// own buffers at least once — beside `enobufs`, so one connection starved
    /// of its buffers reads apart from many.
    pub enobufs_slots: u64,
    /// *(ADR-0192)* The provided-buffer memory this ring allocated and
    /// pre-faulted — every slot's ring and buffers — so a large `connections`
    /// is visible rather than discovered.
    pub buffer_bytes: u64,
}

/// Why this process was not given a ring. Each variant says where the
/// operator goes next — ADR-0190 decision 7.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UringRefused {
    /// `kernel.io_uring_disabled` is set: 1 lets only `kernel.io_uring_group`
    /// in, 2 lets nobody in.
    Disabled {
        /// The value the sysctl read.
        sysctl: u8,
    },
    /// `EPERM` with the sysctl at 0 or unreadable: a seccomp filter.
    Blocked,
    /// `ENOSYS`: no `CONFIG_IO_URING`, or a sandbox answering `ENOSYS`.
    NotInKernel,
    /// `EINVAL` at setup, a probe without `RECV`/`POLL_ADD`/`ASYNC_CANCEL`, or
    /// a buffer ring the kernel will not register. 6.1 is the floor.
    KernelTooOld,
    /// *(ADR-0190 R4)* The ring has fewer connection slots than the serving
    /// loop can hold at once — the engine's capacity plus the pre-session
    /// limit, because a socket is registered when it is accepted, before its
    /// `Logon`. A configuration error, not the kernel's.
    TooSmall {
        /// `UringConfig::connections()`.
        have: usize,
        /// capacity + `presession::Limits::pending()`.
        need: usize,
    },
    /// Anything else.
    Other(io::ErrorKind),
}

impl UringRefused {
    /// Classify the `errno` that `io_uring_setup` (or the probe, or the
    /// buffer-ring registration) answered, given what
    /// `/proc/sys/kernel/io_uring_disabled` read — `None` if it could not be.
    ///
    /// Pure, so the table is tested without a kernel that refuses.
    #[must_use]
    pub fn classify(errno: i32, io_uring_disabled: Option<u8>) -> Self {
        match errno {
            libc::EPERM => match io_uring_disabled {
                Some(sysctl) if sysctl != 0 => Self::Disabled { sysctl },
                _ => Self::Blocked,
            },
            libc::ENOSYS => Self::NotInKernel,
            libc::EINVAL => Self::KernelTooOld,
            other => Self::Other(io::Error::from_raw_os_error(other).kind()),
        }
    }

    /// Classify an error from setting the ring up, reading the sysctl now.
    fn from_setup(e: &io::Error) -> Self {
        match e.raw_os_error() {
            Some(errno) => Self::classify(errno, io_uring_disabled()),
            None => Self::Other(e.kind()),
        }
    }
}

impl fmt::Display for UringRefused {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Disabled { sysctl: 1 } => f.write_str(
                "io_uring is disabled for this process: kernel.io_uring_disabled = 1 \
                 (only members of kernel.io_uring_group may create a ring)",
            ),
            Self::Disabled { sysctl } => write!(
                f,
                "io_uring is disabled on this machine: kernel.io_uring_disabled = {sysctl}"
            ),
            Self::Blocked => f.write_str(
                "io_uring_setup was refused with EPERM and kernel.io_uring_disabled is 0: \
                 a seccomp filter blocks it (Docker >= 25 / containerd RuntimeDefault, \
                 systemd SystemCallFilter)",
            ),
            Self::NotInKernel => f.write_str(
                "io_uring_setup answered ENOSYS: the kernel has no CONFIG_IO_URING, \
                 or a sandbox answers ENOSYS for it",
            ),
            Self::KernelTooOld => f.write_str(
                "this kernel's io_uring lacks what the transport needs \
                 (DEFER_TASKRUN, RECV, POLL_ADD, ASYNC_CANCEL, buffer rings): 6.1 is the floor",
            ),
            Self::TooSmall { have, need } => write!(
                f,
                "the io_uring ring has {have} connection slots and this acceptor can hold \
                 {need} sockets at once (capacity + pending): raise UringConfig's connections"
            ),
            Self::Other(kind) => write!(f, "io_uring could not be set up: {kind}"),
        }
    }
}

impl std::error::Error for UringRefused {}

/// The pre-touch stride: the smallest page size, so every page of any size is
/// written at least once.
const TOUCH_STRIDE: usize = 4096;
/// No buffer: the end of a staging list. Buffer ids stop at `MAX_BUFFERS - 1`.
const NIL: u16 = u16::MAX;
/// The ledger's name for "with the kernel".
const KERNEL: u32 = u32::MAX;
/// How long the SQPOLL thread spins without work before it sleeps and sets
/// `IORING_SQ_NEED_WAKEUP`.
#[cfg(feature = "affinity")]
const SQPOLL_IDLE_MS: u32 = 1_000;
/// How many times a flush under SQPOLL re-reads the submission queue waiting
/// for the kernel thread to take what was queued.
const SQPOLL_FLUSH_SPINS: u32 = 10_000_000;

/// What a completion is for: the top two bits of its `user_data`.
const KIND_SHIFT: u32 = 62;
const KIND_RECV: u64 = 0;
const KIND_CANCEL: u64 = 1;
#[cfg(feature = "standard")]
const KIND_POLL: u64 = 2;
/// Sources a `standard` wait may be shown besides the connections: the
/// listener, the dispatch waker, and room to spare.
const EXTRA_SOURCES: u32 = 16;
/// An empty cell of [`FdTable`].
const NO_FD: std::os::fd::RawFd = -1;
/// The slot index: 30 bits above the generation's 32.
const SLOT_SHIFT: u32 = 32;
const SLOT_MASK: u64 = (1 << 30) - 1;

// `io_uring_buf` is kernel ABI: `addr: u64 @0, len: u32 @8, bid: u16 @12,
// resv: u16 @14`, and entry 0's `resv` is the ring's tail. The entry writes
// below go through these offsets rather than through `&mut BufRingEntry`,
// because a `&mut` to entry 0 would claim the tail the kernel reads.
const _: () = assert!(core::mem::size_of::<BufRingEntry>() == 16);
const ENTRY_LEN_AT: usize = 8;
const ENTRY_BID_AT: usize = 12;

fn user_data(kind: u64, slot: u32, generation: u32) -> u64 {
    (kind << KIND_SHIFT) | ((u64::from(slot) & SLOT_MASK) << SLOT_SHIFT) | u64::from(generation)
}

/// What `/proc/sys/kernel/io_uring_disabled` reads, if it can be read.
fn io_uring_disabled() -> Option<u8> {
    std::fs::read_to_string("/proc/sys/kernel/io_uring_disabled")
        .ok()?
        .trim()
        .parse()
        .ok()
}

/// **U5's check, pure**: the buffer a completion names and how many bytes it
/// says are in it, or `None` if either is out of range.
///
/// Only a span that passes this is ever turned into a slice. `bid` must name
/// one of `buffers`, and the length must be positive and fit one buffer.
fn staged_span(bid: Option<u16>, res: i32, buffers: u16, buffer_len: u32) -> Option<(u16, u32)> {
    let bid = bid?;
    let len = u32::try_from(res).ok()?;
    (bid < buffers && len > 0 && len <= buffer_len).then_some((bid, len))
}

/// This machine's page size — the kernel requires each registered buffer ring
/// to start on a page. 4 096 if the answer is not a power of two.
fn page_size() -> usize {
    // SAFETY (U1, the layout it feeds): `sysconf` takes an integer and reads
    // nothing of this process's memory. Proved by every ring registration in
    // `tests/uring.rs`, which the kernel refuses with `EINVAL` for a ring
    // that is not page-aligned.
    #[allow(unsafe_code)]
    let p = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
    usize::try_from(p)
        .ok()
        .filter(|p| p.is_power_of_two())
        .unwrap_or(TOUCH_STRIDE)
}

/// Every connection slot's buffer ring and every slot's buffers, in one
/// page-aligned allocation, every page written once when it is made
/// (ADR-0192).
///
/// `[ring 0][ring 1]…[ring C−1][slot 0's buffers]…[slot C−1's buffers]`: each
/// ring starts on its own page and holds `buffers_per_connection` entries;
/// slot `s`'s buffer `b` is at `data_at + (s × per + b) × buffer_len`.
struct Region {
    base: NonNull<u8>,
    layout: Layout,
    /// Bytes from one slot's ring to the next: its entries, rounded to a page.
    ring_stride: usize,
    /// Where the buffers start: after every ring.
    data_at: usize,
    /// Buffers per connection.
    per: usize,
    buffer_len: usize,
}

impl Region {
    fn new(config: &UringConfig) -> Result<Self, UringRefused> {
        let too_big = UringRefused::Other(io::ErrorKind::OutOfMemory);
        let page = page_size();
        let per = usize::from(config.buffers_per_connection);
        let connections = usize::from(config.connections);
        let ring_stride = (per * core::mem::size_of::<BufRingEntry>())
            .checked_next_multiple_of(page)
            .ok_or(too_big)?;
        let data_at = ring_stride.checked_mul(connections).ok_or(too_big)?;
        let data = usize::try_from(config.buffer_bytes()).map_err(|_| too_big)?;
        let total = data_at.checked_add(data).ok_or(too_big)?;
        let layout = Layout::from_size_align(total, page).map_err(|_| too_big)?;
        let buffer_len = usize::try_from(config.buffer_len).map_err(|_| too_big)?;

        // SAFETY (U1): `layout` has a non-zero size — at least one ring and
        // one buffer of at least one byte. The pointer is checked for null
        // below and freed exactly once, with this same `layout`, in `Drop`.
        // Proved by `buffers_are_resident_before_the_first_message` and the
        // ASan run of `tests/uring.rs` (plan step 7).
        #[allow(unsafe_code)]
        let p = unsafe { std::alloc::alloc(layout) };
        let base = NonNull::new(p).ok_or(too_big)?;
        let region = Self {
            base,
            layout,
            ring_stride,
            data_at,
            per,
            buffer_len,
        };

        // Pre-touch: one volatile write per 4 KiB, so every page is resident
        // before the first message rather than faulted in on the hot path
        // (non-negotiable 1). Volatile, because the compiler may otherwise
        // fold an allocation followed by zeroing into `alloc_zeroed`, which
        // can hand back pages nobody has touched.
        let mut at = 0;
        while at < total {
            // SAFETY (U1): `at < total`, the allocation's size, so the write
            // is inside memory this `Region` owns and nothing else refers to.
            #[allow(unsafe_code)]
            unsafe {
                base.as_ptr().add(at).write_volatile(0);
            }
            at += TOUCH_STRIDE;
        }
        Ok(region)
    }

    /// Slot `slot`'s first buffer-ring entry, on a page of its own. Entry 0's
    /// `resv` is that ring's tail. `slot < connections`.
    fn entries(&self, slot: usize) -> *mut BufRingEntry {
        self.base
            .as_ptr()
            .wrapping_add(slot * self.ring_stride)
            .cast()
    }

    /// Where slot `slot`'s buffer `bid` starts, from the base.
    const fn at(&self, slot: usize, bid: u16) -> usize {
        self.data_at + (slot * self.per + bid as usize) * self.buffer_len
    }

    /// The address the kernel writes slot `slot`'s buffer `bid` at.
    fn buffer_addr(&self, slot: usize, bid: u16) -> u64 {
        self.base.as_ptr().wrapping_add(self.at(slot, bid)) as u64
    }

    /// The first `len` bytes of slot `slot`'s buffer `bid`.
    ///
    /// **Called only with a span `staged_span` accepted**: `bid <
    /// buffers_per_connection` and `len <= buffer_len`, for a slot below
    /// `connections`, so the slice is inside the allocation.
    fn staged(&self, slot: usize, bid: u16, len: u32) -> &[u8] {
        let at = self.at(slot, bid);
        let len = (len as usize).min(self.buffer_len);
        // SAFETY (U5): `bid` and `len` passed `staged_span` when the
        // completion was reaped, `slot` is a live slot's index, and the buffer
        // is on that slot's staging list — the ledger's promise that the
        // kernel no longer owns it and will not write it until `provide` hands
        // it back, which happens only after the last byte is copied out. So
        // `at + len` is inside the allocation and the memory is not written
        // while this borrow lives. Proved by `staged_span`'s unit test and by
        // `sixty_four_connections_interleaved_are_byte_exact`, which checks
        // the ledger after every reap.
        #[allow(unsafe_code)]
        unsafe {
            core::slice::from_raw_parts(self.base.as_ptr().add(at), len)
        }
    }
}

impl Drop for Region {
    fn drop(&mut self) {
        // SAFETY (U1): `base` came from `alloc` with this `layout` and is
        // freed once. By the time this runs no request can select a buffer —
        // U2's structural argument at registration — and `Inner`'s `Drop` has
        // unregistered every ring and closed the ring (field order).
        // `unregistered_buffers_are_not_written_after_the_ring_is_dropped`
        // checks the end state.
        #[allow(unsafe_code)]
        unsafe {
            std::alloc::dealloc(self.base.as_ptr(), self.layout);
        }
    }
}

/// One connection's place on the ring.
#[derive(Clone, Copy)]
struct Slot {
    live: bool,
    /// Bumped each time the slot is freed, so a completion for its previous
    /// tenant is recognised as stale.
    generation: u32,
    fd: std::os::fd::RawFd,
    /// The staging list, oldest buffer first.
    head: u16,
    tail: u16,
    /// Bytes of `head` already copied out.
    at: u32,
    /// What `recv` answers once the list is empty: end of stream, or an error.
    end: Option<Io>,
    /// Its multishot `recv` ended and must be submitted again.
    rearm: bool,
}

impl Slot {
    const FREE: Self = Self {
        live: false,
        generation: 0,
        fd: -1,
        head: NIL,
        tail: NIL,
        at: 0,
        end: None,
        rearm: false,
    };
}

/// How the ring was asked to be set up.
#[derive(Clone, Copy)]
enum Setup {
    Enter,
    #[cfg(feature = "affinity")]
    Sqpoll(u32),
    #[cfg(feature = "standard")]
    Block,
}

/// The descriptors of the registered connections, so a `standard` wait can
/// tell a connection its multishot `recv` already covers from a source it must
/// arm a `POLL_ADD` for. Open addressing, linear probing, at most half full;
/// allocated once.
struct FdTable {
    keys: Box<[std::os::fd::RawFd]>,
    /// The slot each key's connection is in.
    slots: Box<[u32]>,
    mask: usize,
}

impl FdTable {
    fn new(connections: usize) -> Self {
        let cap = (2 * connections).next_power_of_two().max(2);
        Self {
            keys: vec![NO_FD; cap].into_boxed_slice(),
            slots: vec![0; cap].into_boxed_slice(),
            mask: cap - 1,
        }
    }

    fn home(&self, fd: std::os::fd::RawFd) -> usize {
        (fd as u32).wrapping_mul(0x9E37_79B1) as usize & self.mask
    }

    fn find(&self, fd: std::os::fd::RawFd) -> Option<usize> {
        let mut i = self.home(fd);
        for _ in 0..self.keys.len() {
            match self.keys.get(i).copied() {
                Some(k) if k == fd => return Some(i),
                Some(NO_FD) | None => return None,
                Some(_) => i = (i + 1) & self.mask,
            }
        }
        None
    }

    fn insert(&mut self, fd: std::os::fd::RawFd, slot: u32) {
        let mut i = self.home(fd);
        for _ in 0..self.keys.len() {
            match (self.keys.get_mut(i), self.slots.get_mut(i)) {
                (Some(k), Some(v)) if *k == NO_FD || *k == fd => {
                    *k = fd;
                    *v = slot;
                    return;
                }
                _ => i = (i + 1) & self.mask,
            }
        }
    }

    /// Remove `fd`, shifting back whatever probed past it so every remaining
    /// key is still reachable from its home.
    fn remove(&mut self, fd: std::os::fd::RawFd) {
        let Some(mut hole) = self.find(fd) else {
            return;
        };
        let mut j = hole;
        loop {
            j = (j + 1) & self.mask;
            let Some(k) = self.keys.get(j).copied() else {
                break;
            };
            if k == NO_FD || j == hole {
                break;
            }
            let home = self.home(k);
            // `k` may move into the hole only if its home is not strictly
            // between the hole and `j`, cyclically.
            let stays = if hole <= j {
                hole < home && home <= j
            } else {
                hole < home || home <= j
            };
            if !stays {
                let v = self.slots.get(j).copied().unwrap_or(0);
                if let (Some(h), Some(hv)) = (self.keys.get_mut(hole), self.slots.get_mut(hole)) {
                    *h = k;
                    *hv = v;
                }
                hole = j;
            }
        }
        if let Some(h) = self.keys.get_mut(hole) {
            *h = NO_FD;
        }
    }

    /// The slot of the registered connection on `fd`, if there is one.
    #[cfg(feature = "standard")]
    fn slot(&self, fd: std::os::fd::RawFd) -> Option<u32> {
        self.slots.get(self.find(fd)?).copied()
    }
}

/// `standard`'s one-shot `POLL_ADD`s: a fixed table, each entry with a
/// generation so a late answer for an entry already reused is recognised.
///
/// **Every poll armed for a turn is cancelled when that turn's wait is over**
/// if it has not fired, and armed afresh on the next turn. A table keyed by
/// descriptor *number* and kept armed across turns cannot tell a listener that
/// was closed — its `POLL_ADD` still holding the old file — from a new one the
/// kernel handed the same number (plan *Bẫy*: the reused descriptor). Arming
/// per turn costs two submission entries per extra source per idle turn —
/// **and, on a turn whose polls did not fire, one more `io_uring_enter` that
/// does not wait**, to reap the cancellations in the same turn; left for the
/// next turn's wait, they wake it at once and `standard` spins (see
/// `Inner::reap_standard`).
#[cfg(feature = "standard")]
struct Polls {
    generation: Box<[u32]>,
    pending: Box<[bool]>,
    free: Box<[u32]>,
    free_len: usize,
    /// This turn's, to cancel after the wait.
    armed: Box<[u32]>,
    armed_len: usize,
}

#[cfg(feature = "standard")]
impl Polls {
    fn new(cap: u32) -> Self {
        let n = cap as usize;
        Self {
            generation: vec![0; n].into_boxed_slice(),
            pending: vec![false; n].into_boxed_slice(),
            free: (0..cap).rev().collect::<Vec<_>>().into_boxed_slice(),
            free_len: n,
            armed: vec![0; n].into_boxed_slice(),
            armed_len: 0,
        }
    }

    fn open(&mut self) -> Option<(u32, u32)> {
        self.free_len = self.free_len.checked_sub(1)?;
        let ix = *self.free.get(self.free_len)?;
        let p = self.pending.get_mut(ix as usize)?;
        *p = true;
        Some((ix, self.generation.get(ix as usize).copied().unwrap_or(0)))
    }

    fn close(&mut self, ix: u32) {
        let i = ix as usize;
        if let (Some(p), Some(g)) = (self.pending.get_mut(i), self.generation.get_mut(i)) {
            *p = false;
            *g = g.wrapping_add(1);
        }
        if let Some(f) = self.free.get_mut(self.free_len) {
            *f = ix;
            self.free_len += 1;
        }
    }

    /// Its answer arrived — fired, or cancelled. The entry is free again.
    fn answered(&mut self, ix: u32, generation: u32) {
        let i = ix as usize;
        if self.pending.get(i) == Some(&true) && self.generation.get(i) == Some(&generation) {
            self.close(ix);
        }
    }

    fn is_pending(&self, ix: u32) -> Option<u32> {
        let i = ix as usize;
        (self.pending.get(i) == Some(&true))
            .then(|| self.generation.get(i).copied())
            .flatten()
    }
}

/// One connection slot's provided-buffer ring (ADR-0192): what this side has
/// published, and what the ledger says the kernel holds. It outlives every
/// tenant of the slot — the ring is registered once, when `Uring` is built.
#[derive(Clone, Copy)]
struct Ring {
    /// The ring's tail this side has published.
    tail: u16,
    /// How many of this slot's buffers the ledger says the kernel has.
    with_kernel: u32,
    /// Whether this slot has ever run dry — counted once in `enobufs_slots`.
    ran_dry: bool,
}

/// Everything but the ring itself.
///
/// **Buffers are per slot** (ADR-0192): slot `s`'s buffer `b` is ledger
/// index `s × per + b`; its id on the wire (`bid`) is `b`, in group `s`.
struct State {
    region: Region,
    config: UringConfig,
    /// `buffers_per_connection - 1`: a ring index from a free-running tail.
    mask: u16,
    /// Per slot: its ring's tail and count.
    rings: Box<[Ring]>,
    /// Per buffer (ledger index): [`KERNEL`], or the slot whose staging list
    /// holds it — only ever its own.
    owner: Box<[u32]>,
    /// Per buffer: how many bytes its completion delivered.
    len: Box<[u32]>,
    /// Per buffer: the next buffer (a `bid` of the same slot) on its list.
    next: Box<[u16]>,
    slots: Box<[Slot]>,
    /// Free slots, a stack.
    free: Box<[u32]>,
    free_len: usize,
    /// How many slots have `rearm` set.
    rearm_pending: u32,
    fds: FdTable,
    #[cfg(feature = "standard")]
    polls: Polls,
    unarmed: u64,
    /// Reaps that found the kernel's CQ-overflow flag raised.
    cq_overflow_seen: u64,
    /// The kernel's own count of completions it dropped.
    kernel_overflow: u32,
    unisolated: bool,
    unflushed: u64,
    /// *(Senior review of PR #110, M2)* Sockets of dropped connections whose
    /// flush could not empty the submission queue: kept open — the number
    /// cannot be reused while an entry in the queue may still name it — and
    /// closed by the next flush that empties it. Capacity is reserved once.
    deferred: Vec<TcpTransport>,
    arm: UringArm,
    cqes: u64,
    bytes: u64,
    enobufs: u64,
    enobufs_slots: u64,
    rearms: u64,
    stale: u64,
    enter_errors: u64,
}

impl State {
    fn new(region: Region, config: UringConfig, arm: UringArm) -> Self {
        let connections = usize::from(config.connections);
        let buffers = connections * usize::from(config.buffers_per_connection);
        Self {
            region,
            config,
            mask: config.buffers_per_connection.wrapping_sub(1),
            rings: vec![
                Ring {
                    tail: 0,
                    with_kernel: 0,
                    ran_dry: false,
                };
                connections
            ]
            .into_boxed_slice(),
            owner: vec![KERNEL; buffers].into_boxed_slice(),
            len: vec![0; buffers].into_boxed_slice(),
            next: vec![NIL; buffers].into_boxed_slice(),
            slots: vec![Slot::FREE; connections].into_boxed_slice(),
            // Highest index first, so slot 0 is handed out first.
            free: (0..u32::from(config.connections))
                .rev()
                .collect::<Vec<_>>()
                .into_boxed_slice(),
            free_len: connections,
            rearm_pending: 0,
            fds: FdTable::new(connections),
            #[cfg(feature = "standard")]
            polls: Polls::new(2 * (u32::from(config.connections) + EXTRA_SOURCES)),
            unarmed: 0,
            cq_overflow_seen: 0,
            kernel_overflow: 0,
            unisolated: false,
            unflushed: 0,
            deferred: Vec::with_capacity(connections),
            arm,
            cqes: 0,
            bytes: 0,
            enobufs: 0,
            enobufs_slots: 0,
            rearms: 0,
            stale: 0,
            enter_errors: 0,
        }
    }

    const fn report(&self) -> UringReport {
        UringReport {
            arm: self.arm,
            cqes: self.cqes,
            bytes: self.bytes,
            enobufs: self.enobufs,
            rearms: self.rearms,
            stale: self.stale,
            enter_errors: self.enter_errors,
            unarmed: self.unarmed,
            cq_overflow: self.cq_overflow_seen + self.kernel_overflow as u64,
            unisolated: self.unisolated,
            unflushed: self.unflushed,
            drop_conflicts: 0,
            enobufs_slots: self.enobufs_slots,
            buffer_bytes: self.region.layout.size() as u64,
        }
    }

    /// The ledger index of slot `slot`'s buffer `bid`.
    const fn index(&self, slot: usize, bid: u16) -> usize {
        slot * self.config.buffers_per_connection as usize + bid as usize
    }

    /// Hand slot `slot`'s buffer `bid` to the kernel: write its entry at that
    /// slot's ring tail and publish the tail.
    fn provide(&mut self, slot: usize, bid: u16) {
        let Some(ring) = self.rings.get(slot).copied() else {
            return;
        };
        let idx = usize::from(ring.tail & self.mask);
        let addr = self.region.buffer_addr(slot, bid);
        let entries = self.region.entries(slot);
        let tail = ring.tail.wrapping_add(1);
        // SAFETY (U6): `slot < connections` (checked by the `get` above) and
        // `idx < buffers_per_connection`, so the entry is inside that slot's
        // ring. The ledger never has more than `buffers_per_connection` of a
        // slot's buffers with the kernel, so the entry at its tail is one the
        // kernel has already consumed and does not read until the tail below
        // says so. The writes go through raw offsets of the kernel ABI layout
        // (asserted above), never through a `&mut` spanning entry 0's `resv`,
        // which is the tail. The tail is a `u16` inside that ring's page,
        // aligned (offset 14 of an 8-aligned entry), and published with
        // `Release` so the kernel sees the entry first. Proved by the ledger
        // and the byte-exact tests.
        #[allow(unsafe_code)]
        unsafe {
            let entry = entries.add(idx).cast::<u8>();
            entry.cast::<u64>().write(addr);
            entry
                .add(ENTRY_LEN_AT)
                .cast::<u32>()
                .write(self.config.buffer_len);
            entry.add(ENTRY_BID_AT).cast::<u16>().write(bid);
            let published = BufRingEntry::tail(entries).cast::<AtomicU16>();
            (*published).store(tail, Ordering::Release);
        }
        let g = self.index(slot, bid);
        if let Some(o) = self.owner.get_mut(g) {
            *o = KERNEL;
        }
        if let Some(r) = self.rings.get_mut(slot) {
            r.tail = tail;
            r.with_kernel += 1;
        }
    }

    /// Take slot `slot`'s buffer `bid` from the kernel for `to`. `false` if
    /// the ledger does not have it with the kernel — a completion naming a
    /// buffer it was not given, which is never staged.
    fn take(&mut self, slot: usize, bid: u16, to: u32) -> bool {
        let g = self.index(slot, bid);
        match self.owner.get_mut(g) {
            Some(o) if *o == KERNEL => {
                *o = to;
                if let Some(r) = self.rings.get_mut(slot) {
                    r.with_kernel = r.with_kernel.saturating_sub(1);
                }
                true
            }
            _ => false,
        }
    }

    /// A buffer the kernel selected from slot `slot`'s ring for a completion
    /// nobody will read: back to that ring at once.
    fn give_back(&mut self, slot: usize, bid: Option<u16>) {
        // `take` to `KERNEL`, then `provide`: the slot's count drops and rises
        // by one, and its ring gets the entry back.
        if let Some(b) = bid
            && b < self.config.buffers_per_connection
            && slot < self.rings.len()
            && self.take(slot, b, KERNEL)
        {
            self.provide(slot, b);
        }
    }

    fn mark_rearm(&mut self, slot: usize) {
        if let Some(s) = self.slots.get_mut(slot)
            && !s.rearm
        {
            s.rearm = true;
            self.rearm_pending += 1;
        }
    }

    fn end(&mut self, slot: usize, how: Io) {
        if let Some(s) = self.slots.get_mut(slot)
            && s.end.is_none()
        {
            s.end = Some(how);
        }
    }

    fn append(&mut self, slot: usize, bid: u16, len: u32) {
        let g = self.index(slot, bid);
        if let Some(l) = self.len.get_mut(g) {
            *l = len;
        }
        if let Some(n) = self.next.get_mut(g) {
            *n = NIL;
        }
        let Some(s) = self.slots.get_mut(slot) else {
            return;
        };
        let last = s.tail;
        s.tail = bid;
        if last == NIL {
            s.head = bid;
        } else {
            let gl = self.index(slot, last);
            if let Some(n) = self.next.get_mut(gl) {
                *n = bid;
            }
        }
    }

    /// One completion.
    fn complete(&mut self, ud: u64, res: i32, flags: u32) {
        self.cqes += 1;
        #[cfg(feature = "standard")]
        if ud >> KIND_SHIFT == KIND_POLL {
            let ix = ((ud >> SLOT_SHIFT) & SLOT_MASK) as u32;
            self.polls.answered(ix, ud as u32);
            return;
        }
        if ud >> KIND_SHIFT != KIND_RECV {
            // A cancel's own answer: there is nothing to do with it.
            return;
        }
        let slot = ((ud >> SLOT_SHIFT) & SLOT_MASK) as usize;
        let generation = ud as u32;
        let bid = cqueue::buffer_select(flags);
        let more = cqueue::more(flags);
        let current = self
            .slots
            .get(slot)
            .is_some_and(|s| s.live && s.generation == generation);
        if !current {
            // Its connection is gone and the slot may have a new tenant: the
            // bytes reach nobody, and the buffer goes straight back — to the
            // same slot's ring, which is the group it came from.
            self.stale += 1;
            self.give_back(slot, bid);
            return;
        }
        if res > 0 {
            match staged_span(
                bid,
                res,
                self.config.buffers_per_connection,
                self.config.buffer_len,
            ) {
                Some((b, len)) if self.take(slot, b, slot as u32) => {
                    self.append(slot, b, len);
                    self.bytes += u64::from(len);
                }
                _ => {
                    self.give_back(slot, bid);
                    self.end(slot, Io::Failed(io::ErrorKind::InvalidData));
                }
            }
            if !more {
                self.mark_rearm(slot);
            }
        } else if res == 0 {
            self.give_back(slot, bid);
            self.end(slot, Io::Closed);
        } else if res == -libc::ENOBUFS {
            // *(ADR-0192 decision 3)* This connection's own buffers are all
            // out: backpressure on it alone. The bytes stay in its socket —
            // TCP's own — and its receive is submitted again when one of its
            // buffers is back with the kernel.
            self.enobufs += 1;
            if let Some(r) = self.rings.get_mut(slot)
                && !r.ran_dry
            {
                r.ran_dry = true;
                self.enobufs_slots += 1;
            }
            if !more {
                self.mark_rearm(slot);
            }
        } else {
            // *(Senior review of PR #110, L2)* A failed completion may still
            // carry a buffer; it goes back, or the slot's ring is one short
            // for ever while the ledger — which never saw it leave — reads
            // clean.
            self.give_back(slot, bid);
            let kind = io::Error::from_raw_os_error(res.saturating_neg()).kind();
            self.end(slot, Io::Failed(kind));
        }
    }

    /// Copy what `slot` has staged into `buf`, handing each emptied buffer
    /// back to its ring. No system call.
    fn read(&mut self, slot: usize, generation: u32, buf: &mut [u8]) -> Io {
        let Some(&s) = self.slots.get(slot) else {
            return Io::Failed(io::ErrorKind::NotFound);
        };
        if !s.live || s.generation != generation {
            return Io::Failed(io::ErrorKind::NotFound);
        }
        let (mut head, mut tail, mut at) = (s.head, s.tail, s.at);
        let mut n = 0;
        while n < buf.len() && head != NIL {
            let g = self.index(slot, head);
            let len = self.len.get(g).copied().unwrap_or(0);
            let src = self
                .region
                .staged(slot, head, len)
                .get(at as usize..)
                .unwrap_or(&[]);
            let dst = buf.get_mut(n..).unwrap_or(&mut []);
            let k = src.len().min(dst.len());
            if let (Some(d), Some(c)) = (dst.get_mut(..k), src.get(..k)) {
                d.copy_from_slice(c);
            }
            n += k;
            at += k as u32;
            if at >= len {
                let done = head;
                head = self.next.get(g).copied().unwrap_or(NIL);
                if head == NIL {
                    tail = NIL;
                }
                at = 0;
                self.provide(slot, done);
            }
        }
        if let Some(s) = self.slots.get_mut(slot) {
            s.head = head;
            s.tail = tail;
            s.at = at;
        }
        if n > 0 {
            Io::Ready(n)
        } else {
            s.end.unwrap_or(Io::Idle)
        }
    }

    fn open(&mut self, fd: std::os::fd::RawFd) -> Option<(u32, u32)> {
        self.free_len = self.free_len.checked_sub(1)?;
        let slot = *self.free.get(self.free_len)?;
        let s = self.slots.get_mut(slot as usize)?;
        *s = Slot {
            live: true,
            fd,
            generation: s.generation,
            ..Slot::FREE
        };
        let generation = s.generation;
        self.fds.insert(fd, slot);
        Some((slot, generation))
    }

    /// Free `slot`: its staged buffers go back to its ring, its generation
    /// moves on, and it can be handed out again at once — with the same ring.
    fn release(&mut self, slot: u32) {
        let ix = slot as usize;
        let Some(&s) = self.slots.get(ix) else {
            return;
        };
        let mut b = s.head;
        while b != NIL {
            let g = self.index(ix, b);
            let next = self.next.get(g).copied().unwrap_or(NIL);
            self.provide(ix, b);
            b = next;
        }
        if s.rearm {
            self.rearm_pending = self.rearm_pending.saturating_sub(1);
        }
        if s.live {
            self.fds.remove(s.fd);
        }
        if let Some(s) = self.slots.get_mut(ix) {
            *s = Slot {
                generation: s.generation.wrapping_add(1),
                ..Slot::FREE
            };
        }
        if let Some(f) = self.free.get_mut(self.free_len) {
            *f = slot;
            self.free_len += 1;
        }
    }

    /// The ownership ledger, **per slot** (ADR-0192 decision 4): each of slot
    /// `s`'s buffers is in exactly one place — `s`'s ring, or `s`'s staging
    /// list — and `s`'s count of buffers with the kernel agrees.
    fn accounted_for(&self) -> bool {
        let per = usize::from(self.config.buffers_per_connection);
        let mut seen = vec![0u32; self.owner.len()];
        for (ix, s) in self.slots.iter().enumerate() {
            if (s.head == NIL) != (s.tail == NIL) || (!s.live && s.head != NIL) {
                return false;
            }
            let mut b = s.head;
            let mut steps = 0;
            while b != NIL {
                steps += 1;
                if usize::from(b) >= per {
                    return false;
                }
                let g = self.index(ix, b);
                let (Some(&owner), Some(count)) = (self.owner.get(g), seen.get_mut(g)) else {
                    return false;
                };
                if steps > per || owner as usize != ix {
                    return false;
                }
                *count += 1;
                b = self.next.get(g).copied().unwrap_or(NIL);
            }
        }
        for (ix, ring) in self.rings.iter().enumerate() {
            let mut with_kernel = 0u32;
            for g in ix * per..(ix + 1) * per {
                match (self.owner.get(g).copied(), seen.get(g).copied()) {
                    (Some(KERNEL), Some(0)) => with_kernel += 1,
                    (Some(o), Some(1)) if o as usize == ix => {}
                    _ => return false,
                }
            }
            if with_kernel != ring.with_kernel {
                return false;
            }
        }
        true
    }
}

/// What a [`Uring`] and everything made from it share: the ring and its
/// state, and a counter that lives **outside** the `RefCell`, so a drop that
/// finds the ring borrowed can still record it (senior review of PR #110, L4).
struct Shared {
    cell: RefCell<Inner>,
    drop_conflicts: std::cell::Cell<u64>,
}

impl Shared {
    fn new(inner: Inner) -> Rc<Self> {
        Rc::new(Self {
            cell: RefCell::new(inner),
            drop_conflicts: std::cell::Cell::new(0),
        })
    }
}

/// The ring and its state. **Field order is teardown order**: the ring is
/// closed before the buffer memory in `state` is freed.
struct Inner {
    ring: IoUring,
    state: State,
}

impl Inner {
    fn new(config: UringConfig, setup: Setup) -> Result<Self, UringRefused> {
        let connections = u32::from(config.connections);
        // Room for one turn's re-arms, cancels and `standard` polls without a
        // flush; a full queue is flushed without waiting, never an error.
        let sq = (4 * connections + 2 * EXTRA_SOURCES).next_power_of_two();
        let buffers = connections * u32::from(config.buffers_per_connection);
        let cq = (buffers + 6 * connections + 2 * EXTRA_SOURCES + 64).next_power_of_two();
        let mut builder = IoUring::builder();
        builder.setup_cqsize(cq).setup_clamp();
        match setup {
            Setup::Enter => {
                builder.setup_single_issuer().setup_defer_taskrun();
            }
            #[cfg(feature = "standard")]
            Setup::Block => {
                builder.setup_single_issuer().setup_defer_taskrun();
            }
            #[cfg(feature = "affinity")]
            Setup::Sqpoll(cpu) => {
                builder.setup_sqpoll(SQPOLL_IDLE_MS).setup_sqpoll_cpu(cpu);
            }
        }
        let ring = builder
            .build(sq)
            .map_err(|e| UringRefused::from_setup(&e))?;

        let mut probe = io_uring::Probe::new();
        ring.submitter()
            .register_probe(&mut probe)
            .map_err(|e| UringRefused::from_setup(&e))?;
        let needs = [
            opcode::RecvMulti::CODE,
            opcode::PollAdd::CODE,
            opcode::AsyncCancel::CODE,
        ];
        if !needs.iter().all(|op| probe.is_supported(*op)) {
            return Err(UringRefused::KernelTooOld);
        }
        // `standard` waits with a timeout passed through `EXT_ARG` (5.11).
        #[cfg(feature = "standard")]
        if matches!(setup, Setup::Block) && !ring.params().is_feature_ext_arg() {
            return Err(UringRefused::KernelTooOld);
        }

        // *(Senior review of PR #110, M1)* **Enter once, here, before
        // anything is allocated or registered.** A sandbox may allow
        // `io_uring_setup` and refuse `io_uring_enter`; without this the ring
        // is made, connections are registered, and every idle turn's enter
        // fails — an engine that runs deaf instead of refusing (ADR-0190
        // decision 7). The error is classified exactly as a setup error is.
        // SAFETY (U4): no argument pointer (`None`); the kernel reads only the
        // three integers, and the descriptor is `ring`'s own. Proved by
        // `a_blocked_io_uring_enter_refuses_to_start_and_names_seccomp`.
        #[allow(unsafe_code)]
        let entered = unsafe {
            ring.submitter().enter::<libc::sigset_t>(
                0,
                0,
                io_uring::EnterFlags::GETEVENTS.bits(),
                None,
            )
        };
        entered.map_err(|e| UringRefused::from_setup(&e))?;

        let arm = match setup {
            _ if ring.params().is_setup_sqpoll() => UringArm::Sqpoll,
            #[cfg(feature = "standard")]
            Setup::Block => UringArm::Block,
            _ => UringArm::Enter,
        };
        let mut state = State::new(Region::new(&config)?, config, arm);
        for slot in 0..usize::from(config.connections) {
            for bid in 0..config.buffers_per_connection {
                state.provide(slot, bid);
            }
        }
        // Built before the rings are registered, so a registration that fails
        // part-way unregisters the ones that took (`Inner`'s `Drop`).
        let inner = Self { ring, state };
        for slot in 0..config.connections {
            // SAFETY (U2): slot `slot`'s entries are `buffers_per_connection`
            // (a power of two, at most 32768) `io_uring_buf`s on a page of
            // their own inside `state`'s `Region`, which lives as long as
            // `inner`. What keeps the kernel from writing a buffer after that
            // memory is freed is **structural**: every `UringTransport` holds
            // the `Rc` that owns `Inner`, so `Inner` — and the `Region` — can
            // only be dropped after the last connection has been dropped, and
            // each of those shut its socket down (`shutdown(2)` ends its
            // `recv`), cancelled that `recv` and flushed the cancel before
            // its descriptor closed. No request can select a buffer after
            // that. `Inner`'s `Drop` unregistering every ring before the
            // `Region` is freed is a second line behind it, and no test can
            // tell it from its absence (senior review of PR #110, L1: fields
            // reordered and the unregister deleted, the canary stayed green);
            // `unregistered_buffers_are_not_written_after_the_ring_is_dropped`
            // checks the end state, not this order.
            #[allow(unsafe_code)]
            let registered = unsafe {
                inner.ring.submitter().register_buf_ring_with_flags(
                    inner.state.region.entries(usize::from(slot)) as u64,
                    config.buffers_per_connection,
                    slot,
                    0,
                )
            };
            registered.map_err(|e| UringRefused::from_setup(&e))?;
        }
        Ok(inner)
    }

    /// Submit what is queued without waiting for anything. Under SQPOLL, wait
    /// — spinning, never sleeping — until the kernel thread has taken it.
    ///
    /// `true` if the submission queue is empty afterwards: every entry reached
    /// the kernel and resolved its descriptor. `false` — an enter that failed,
    /// or an SQ thread that did not take the entries within
    /// [`SQPOLL_FLUSH_SPINS`] — is counted in [`UringReport::unflushed`]
    /// (senior review of PR #110, M2).
    fn flush(&mut self) -> bool {
        if !self.ring.submission().is_empty() {
            if self.state.arm == UringArm::Sqpoll {
                for _ in 0..SQPOLL_FLUSH_SPINS {
                    let sq = self.ring.submission();
                    if sq.is_empty() {
                        break;
                    }
                    if sq.need_wakeup() {
                        drop(sq);
                        self.enter(0, 0, io_uring::EnterFlags::SQ_WAKEUP.bits());
                    }
                    core::hint::spin_loop();
                }
            } else {
                let n = self.ring.submission().len() as u32;
                self.enter(n, 0, 0);
            }
        }
        let empty = self.ring.submission().is_empty();
        if empty {
            self.settle();
        } else {
            self.state.unflushed += 1;
        }
        empty
    }

    /// Close the sockets kept open by a flush that could not empty the queue,
    /// once the queue is empty — nothing queued can name them any more.
    /// Rare; the `Vec` keeps its capacity, so nothing is freed or allocated.
    fn settle(&mut self) {
        if !self.state.deferred.is_empty() && self.ring.submission().is_empty() {
            self.state.deferred.clear();
        }
    }

    /// Queue `entry`, flushing once if the submission queue is full.
    fn push(&mut self, entry: &squeue::Entry) -> bool {
        for _ in 0..2 {
            // SAFETY (U3): the entries this module builds are `RecvMulti`
            // (buffer-selected: it names no user memory, only the group whose
            // memory U2 keeps alive), `AsyncCancel` (a `user_data`, no
            // memory) and, in step 3, `PollAdd` (no memory). The descriptor a
            // `RecvMulti` names is held open by its `UringTransport` until
            // after `Drop` has flushed the queue, so it is never submitted
            // against a reused number. Proved by
            // `a_ring_that_runs_out_of_buffers_rearms_and_loses_nothing` and
            // `a_late_completion_for_a_dropped_connection_reaches_nobody`.
            #[allow(unsafe_code)]
            let pushed = unsafe { self.ring.submission().push(entry) };
            if pushed.is_ok() {
                return true;
            }
            self.flush();
        }
        false
    }

    fn enter(&mut self, to_submit: u32, min_complete: u32, flags: u32) {
        // SAFETY (U4): no argument pointer is passed (`None`), so the kernel
        // reads only the three integers; the ring's descriptor is owned by
        // `self.ring`. Proved by `a_message_arrives_through_the_ring` (a
        // `GETEVENTS` enter is what makes the completion visible) and, in
        // step 3, the `standard` wake tests.
        #[allow(unsafe_code)]
        let r = unsafe {
            self.ring
                .submitter()
                .enter::<libc::sigset_t>(to_submit, min_complete, flags, None)
        };
        self.note(r);
    }

    /// `EINTR` is a wake and `ETIME` the timeout: neither is an error.
    fn note(&mut self, r: io::Result<usize>) {
        if let Err(e) = r
            && !matches!(e.raw_os_error(), Some(libc::EINTR | libc::ETIME))
        {
            self.state.enter_errors += 1;
        }
    }

    /// Submit, then wait in the kernel for `min_complete` completions or
    /// `timeout_ms`, whichever is first. `standard` only.
    #[cfg(feature = "standard")]
    fn enter_waiting(&mut self, to_submit: u32, min_complete: u32, timeout_ms: u32) {
        let ts = types::Timespec::from(std::time::Duration::from_millis(u64::from(timeout_ms)));
        let args = types::SubmitArgs::new().timespec(&ts);
        let flags = io_uring::EnterFlags::GETEVENTS.bits() | io_uring::EnterFlags::EXT_ARG.bits();
        // SAFETY (U4): with `EXT_ARG` the kernel reads one
        // `io_uring_getevents_arg` (`SubmitArgs` is `repr(transparent)` over
        // it, and its size is what `enter` passes) and, through it, the
        // `Timespec` — both on this stack frame, alive for the whole
        // synchronous call, and not retained after it returns. Proved by
        // `standard_wakes_on_its_own_timeout_to_tick` (the timeout is read
        // correctly: 50 ms returns in [40, 500] ms) and
        // `standard_is_woken_by_the_data_not_the_timeout`.
        #[allow(unsafe_code)]
        let r = unsafe {
            self.ring
                .submitter()
                .enter(to_submit, min_complete, flags, Some(&args))
        };
        self.note(r);
    }

    /// Queue a fresh multishot `recv` for every slot whose last one ended,
    /// while that slot's own ring has a buffer to give it.
    fn rearm(&mut self) {
        if self.state.rearm_pending == 0 {
            return;
        }
        for ix in 0..self.state.slots.len() {
            let Some(s) = self.state.slots.get(ix).copied() else {
                break;
            };
            // *(ADR-0192 decision 3)* Re-armed when **its own** ring has a
            // buffer back — the engine read it — not when some other
            // connection's did.
            let has_buffer = self.state.rings.get(ix).is_some_and(|r| r.with_kernel > 0);
            if !s.rearm || !has_buffer {
                continue;
            }
            let e = recv(s.fd, ix as u32, s.generation);
            if !self.push(&e) {
                return;
            }
            if let Some(s) = self.state.slots.get_mut(ix) {
                s.rearm = false;
            }
            self.state.rearm_pending = self.state.rearm_pending.saturating_sub(1);
            self.state.rearms += 1;
        }
    }

    /// One `hft` idle turn: submit, enter without waiting (or, under SQPOLL,
    /// only when the kernel thread asks), reap every completion.
    fn reap_hft(&mut self) {
        self.rearm();
        if self.state.arm == UringArm::Sqpoll {
            let (wake, overflow) = {
                let sq = self.ring.submission();
                (sq.need_wakeup(), sq.cq_overflow())
            };
            if wake {
                self.enter(0, 0, io_uring::EnterFlags::SQ_WAKEUP.bits());
            }
            if overflow {
                self.enter(0, 0, io_uring::EnterFlags::GETEVENTS.bits());
            }
        } else {
            let n = self.ring.submission().len() as u32;
            self.enter(n, 0, io_uring::EnterFlags::GETEVENTS.bits());
        }
        self.drain();
        self.settle();
    }

    /// Move every posted completion to where it belongs.
    fn drain(&mut self) {
        if self.ring.submission().cq_overflow() {
            self.state.cq_overflow_seen += 1;
        }
        let Self { ring, state } = self;
        let cq = ring.completion();
        state.kernel_overflow = cq.overflow();
        for cqe in cq {
            state.complete(cqe.user_data(), cqe.result(), cqe.flags());
        }
    }

    /// One `standard` idle turn — ADR-0190 decision 5.
    ///
    /// 1. Re-arm the `recv`s that ended.
    /// 2. Arm a one-shot `POLL_ADD` for every source the ring does not already
    ///    cover: a readable source that is not a registered connection (the
    ///    listener, the waker's pipe, …), and every `writable` interest.
    /// 3. Submit and wait in `io_uring_enter` for one completion or the
    ///    timeout — **unless bytes are already staged**, which the kernel
    ///    cannot see; then it does not wait at all.
    /// 4. Reap, and cancel this turn's polls that did not fire.
    #[cfg(feature = "standard")]
    fn reap_standard(&mut self, interests: &[Interest], timeout_ms: u32) {
        self.rearm();
        self.state.polls.armed_len = 0;
        // *(ADR-0190 R3)* Bytes already staged for a connection **in this
        // turn's list** are ready, and the kernel cannot see them: the wait is
        // skipped. A connection that is not listed — parked by
        // `Recovery::ready`, say — is not being read, and its bytes must not
        // turn this wait into a spin.
        let mut listed_ready = false;
        for i in interests {
            let fd = i.source.as_raw_fd();
            let connection = self.state.fds.slot(fd);
            if let Some(slot) = connection {
                listed_ready |= self
                    .state
                    .slots
                    .get(slot as usize)
                    .is_some_and(|s| s.head != NIL);
            }
            let events = match (connection.is_some(), i.writable) {
                (true, false) => continue,
                (true, true) => libc::POLLOUT,
                (false, false) => libc::POLLIN,
                (false, true) => libc::POLLIN | libc::POLLOUT,
            };
            self.arm_poll(fd, events as u32);
        }
        let min_complete = u32::from(!listed_ready);
        let n = self.ring.submission().len() as u32;
        self.enter_waiting(n, min_complete, timeout_ms);
        self.drain();
        let mut cancelled = false;
        for k in 0..self.state.polls.armed_len {
            let Some(ix) = self.state.polls.armed.get(k).copied() else {
                break;
            };
            if let Some(generation) = self.state.polls.is_pending(ix) {
                let cancel = opcode::AsyncCancel::new(user_data(KIND_POLL, ix, generation))
                    .build()
                    .user_data(user_data(KIND_CANCEL, ix, generation));
                // If it cannot be queued the poll stays armed until it fires,
                // which frees its entry then.
                cancelled |= self.push(&cancel);
            }
        }
        // **The cancels are submitted and their completions reaped in this
        // turn**, by one more enter that does not wait (`min_complete = 0`).
        // `[measured 2026-09-24]` left for the next turn's enter, the cancel's
        // own completion and the poll's `-ECANCELED` were already there when
        // that enter began, satisfied its `min_complete = 1` at once, and the
        // turn after cancelled again: `standard` spun, 7385 waits where a
        // handful were due (`standard_with_a_quiet_listener_waits_out_its_
        // timeout_every_turn`). A completion that still lands late wakes one
        // wait once — it cannot feed itself, because each turn cancels only
        // its own polls.
        if cancelled {
            let n = self.ring.submission().len() as u32;
            self.enter(n, 0, io_uring::EnterFlags::GETEVENTS.bits());
            self.drain();
        }
        self.settle();
    }

    #[cfg(feature = "standard")]
    fn arm_poll(&mut self, fd: std::os::fd::RawFd, events: u32) {
        let Some((ix, generation)) = self.state.polls.open() else {
            self.state.unarmed += 1;
            return;
        };
        let e = opcode::PollAdd::new(types::Fd(fd), events)
            .build()
            .user_data(user_data(KIND_POLL, ix, generation));
        if !self.push(&e) {
            self.state.polls.close(ix);
            self.state.unarmed += 1;
            return;
        }
        let at = self.state.polls.armed_len;
        if let Some(a) = self.state.polls.armed.get_mut(at) {
            *a = ix;
            self.state.polls.armed_len += 1;
        }
    }
}

impl Drop for Inner {
    fn drop(&mut self) {
        // U2's second line (its first is structural — see the SAFETY comment
        // at registration): every slot's ring is unregistered before the
        // memory goes. The ring field is dropped next (closing it cancels
        // whatever is pending), and only then `state`, whose `Region` frees
        // the buffers. A slot whose registration never took answers with an
        // error, which changes nothing.
        for slot in 0..self.state.config.connections {
            let _ = self.ring.submitter().unregister_buf_ring(slot);
        }
    }
}

/// A multishot `recv` on `fd` for `slot` at `generation`, selecting from
/// **that slot's own** buffer group — its group id is its index (ADR-0192).
fn recv(fd: std::os::fd::RawFd, slot: u32, generation: u32) -> squeue::Entry {
    opcode::RecvMulti::new(types::Fd(fd), slot as u16)
        .build()
        .user_data(user_data(KIND_RECV, slot, generation))
}

/// One ring, its provided buffers and its connection slab. `!Send`: one per
/// engine thread, made on that thread.
pub struct Uring {
    inner: Rc<Shared>,
}

impl Uring {
    /// An `hft` ring, and the idle strategy that reaps it.
    ///
    /// **Make it on the engine thread**, before any listener is bound: the
    /// ring is `SINGLE_ISSUER`, and a refusal then leaves nothing half-open.
    ///
    /// `HftArm::Sqpoll`'s core is handed to the kernel as `sq_thread_cpu`
    /// here and is **not** checked against `isolcpus` here: the serving entry
    /// point does that through `crate::affinity::Topology` before calling
    /// this, as `serve_hft_pinned` does for the engine's own core.
    ///
    /// # Errors
    ///
    /// [`UringRefused`] naming why the kernel will not give this process a
    /// ring.
    pub fn hft(config: UringConfig, arm: HftArm) -> Result<(Self, UringSpin), UringRefused> {
        let setup = match arm {
            HftArm::Enter => Setup::Enter,
            #[cfg(feature = "affinity")]
            HftArm::Sqpoll { pin } => Setup::Sqpoll(
                u32::try_from(pin.core().0)
                    .map_err(|_| UringRefused::Other(io::ErrorKind::InvalidInput))?,
            ),
        };
        #[cfg_attr(
            not(feature = "affinity"),
            expect(unused_mut, reason = "set only for SQPOLL")
        )]
        let mut inner = Inner::new(config, setup)?;
        #[cfg(feature = "affinity")]
        if let HftArm::Sqpoll { pin } = arm {
            inner.state.unisolated = pin.is_unisolated_allowed();
        }
        let inner = Shared::new(inner);
        Ok((
            Self {
                inner: Rc::clone(&inner),
            },
            UringSpin { inner },
        ))
    }

    /// A `standard` ring, and the blocking idle strategy that reaps it.
    ///
    /// **There is no SQPOLL here, by type**: `standard` + a spinning kernel
    /// thread cannot be written (ADR-0190 decision 4). [`HftArm`] is the only
    /// type that can carry `Sqpoll`, and this constructor has no parameter it
    /// could go in:
    ///
    /// ```compile_fail,E0061
    /// use fixbolt_engine::transport::uring::{HftArm, Uring, UringConfig};
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let config = UringConfig::new(64, 4096, 4)?;
    /// let _ring = Uring::standard(config, HftArm::default());
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// Made on the engine thread, as for [`Self::hft`]: the ring is
    /// `SINGLE_ISSUER | DEFER_TASKRUN`.
    ///
    /// # Errors
    ///
    /// [`UringRefused`], as for [`Self::hft`]; also
    /// [`UringRefused::KernelTooOld`] without `IORING_FEAT_EXT_ARG`.
    #[cfg(feature = "standard")]
    pub fn standard(config: UringConfig) -> Result<(Self, UringBlock), UringRefused> {
        let inner = Shared::new(Inner::new(config, Setup::Block)?);
        Ok((
            Self {
                inner: Rc::clone(&inner),
            },
            UringBlock {
                inner,
                timeout_ms: crate::block::DEFAULT_TIMEOUT_MS,
            },
        ))
    }

    /// Put `transport` on the ring: its multishot `recv` is queued and goes to
    /// the kernel on the next idle turn. `None` when every slot is taken — the
    /// socket is then dropped, the answer `pump` already gives a connection it
    /// has no room for. No allocation.
    #[must_use]
    pub fn register(&self, transport: TcpTransport) -> Option<UringTransport> {
        let mut inner = self.inner.cell.try_borrow_mut().ok()?;
        let fd = transport.socket().as_raw_fd();
        let (slot, generation) = inner.state.open(fd)?;
        if !inner.push(&recv(fd, slot, generation)) {
            inner.state.release(slot);
            return None;
        }
        drop(inner);
        Some(UringTransport {
            inner: Rc::clone(&self.inner),
            slot,
            generation,
            tcp: Some(transport),
        })
    }

    /// What this ring has done so far.
    #[must_use]
    pub fn report(&self) -> UringReport {
        let mut r = self.inner.cell.try_borrow().map_or(
            UringReport {
                arm: UringArm::Enter,
                cqes: 0,
                bytes: 0,
                enobufs: 0,
                rearms: 0,
                stale: 0,
                enter_errors: 0,
                unarmed: 0,
                cq_overflow: 0,
                unisolated: false,
                unflushed: 0,
                drop_conflicts: 0,
                enobufs_slots: 0,
                buffer_bytes: 0,
            },
            |i| i.state.report(),
        );
        r.drop_conflicts = self.inner.drop_conflicts.get();
        r
    }

    /// Whether every provided buffer is in exactly one place: with the
    /// kernel, or in exactly one connection's staging list.
    ///
    /// **The ownership ledger, for tests** (plan *Bất biến* 8, U5 and U6): no
    /// sanitizer sees the kernel write into this memory, so what proves a
    /// buffer is never handed back while still read is this check, run after
    /// every reap in `crates/engine/tests/uring.rs`. It allocates; it is not
    /// for the hot path.
    #[doc(hidden)]
    #[must_use]
    pub fn buffers_accounted_for(&self) -> bool {
        self.inner
            .cell
            .try_borrow()
            .is_ok_and(|i| i.state.accounted_for())
    }

    /// The provided buffers' memory, as `(address, length)`.
    ///
    /// **For the canary test only** (plan *Bất biến* 8, U2): after the ring is
    /// dropped, the test re-occupies this range and proves the kernel does not
    /// write into it. An address, not a pointer — nothing can be read through
    /// it.
    #[doc(hidden)]
    #[must_use]
    pub fn buffer_region(&self) -> (usize, usize) {
        self.inner.cell.try_borrow().map_or((0, 0), |i| {
            (
                i.state.region.base.as_ptr() as usize,
                i.state.region.layout.size(),
            )
        })
    }
}

/// One connection on a [`Uring`].
///
/// # Only a reaping strategy can drive it
///
/// Its `recv` makes no system call: bytes reach it only when [`UringSpin`] or
/// `UringBlock` reaps the ring. Under [`crate::wait::Spin`] it would run and
/// never receive a byte, so [`crate::Engine::new`] refuses the pairing when it
/// is compiled — `Transport::NEEDS_REAPER` against `Waiting::REAPS`, ADR-0190
/// decision 1:
///
/// ```compile_fail,E0080
/// # struct App;
/// # impl fixbolt_session::Application for App {
/// #     fn on_message(&mut self, _m: &[u8], _h: fixbolt_session::Header<'_>, _o: &mut [u8])
/// #         -> Option<core::ops::Range<usize>> { None }
/// # }
/// use fixbolt_engine::{Engine, clock::SystemClock, wait::Spin};
/// use fixbolt_engine::{dispatch::InlineDispatch, journal::Store};
/// use fixbolt_engine::transport::uring::UringTransport;
///
/// let _engine: Engine<
///     UringTransport, fixbolt_session::Acceptor, InlineDispatch<App>,
///     SystemClock, Spin, Store, 256, 4096, 8192,
/// > = Engine::new(
///     fixbolt_session::Config::acceptor(b"FIX.4.4", b"ISLD", b"TEST"),
///     InlineDispatch::new(App),
///     SystemClock,
///     Spin,
///     4,
/// );
/// ```
///
/// Dropping it shuts the socket down (`shutdown(2)`, which ends the pending
/// `recv` and sends the peer its FIN at once), hands its staged buffers back,
/// moves its slot's generation on, and submits an `ASYNC_CANCEL` for its
/// `recv` before the descriptor is closed.
pub struct UringTransport {
    inner: Rc<Shared>,
    slot: u32,
    generation: u32,
    /// `Some` for the transport's whole life; `Drop` takes it, to close it or,
    /// when a queued entry may still name it, to keep it open (M2).
    tcp: Option<TcpTransport>,
}

impl Transport for UringTransport {
    const POLLABLE: bool = true;
    const NEEDS_REAPER: bool = true;

    fn carrier(&self) -> crate::transport::Carrier {
        crate::transport::Carrier::Uring
    }

    fn recv(&mut self, buf: &mut [u8]) -> Io {
        if buf.is_empty() {
            return Io::Idle;
        }
        match self.inner.cell.try_borrow_mut() {
            Ok(mut inner) => inner.state.read(self.slot as usize, self.generation, buf),
            Err(_) => Io::Failed(io::ErrorKind::WouldBlock),
        }
    }

    fn send(&mut self, buf: &[u8]) -> Io {
        match self.tcp.as_mut() {
            Some(t) => t.send(buf),
            None => Io::Closed,
        }
    }

    fn source(&self) -> Option<Source> {
        self.tcp.as_ref().and_then(Transport::source)
    }
}

impl Drop for UringTransport {
    fn drop(&mut self) {
        let tcp = self.tcp.take();
        // U7, with no `unsafe`: `TcpStream::shutdown`, on the socket this
        // still owns. `SHUT_RDWR` ends the multishot `recv` and puts the FIN on
        // the wire now, even though that request still holds a reference to
        // the file.
        if let Some(t) = &tcp {
            let _ = t.socket().shutdown(std::net::Shutdown::Both);
        }
        let Ok(mut inner) = self.inner.cell.try_borrow_mut() else {
            // *(Senior review of PR #110, L4)* Unreachable today — nothing
            // drops a transport while the ring is borrowed. If it ever
            // happens, the slot stays live, so its descriptor must not close:
            // it is leaked, never closed under a live slot, and counted.
            self.inner
                .drop_conflicts
                .set(self.inner.drop_conflicts.get().saturating_add(1));
            std::mem::forget(tcp);
            return;
        };
        let slot = self.slot as usize;
        let current = inner
            .state
            .slots
            .get(slot)
            .is_some_and(|s| s.live && s.generation == self.generation);
        if current {
            inner.state.release(self.slot);
            let cancel = opcode::AsyncCancel::new(user_data(KIND_RECV, self.slot, self.generation))
                .build()
                .user_data(user_data(KIND_CANCEL, self.slot, self.generation));
            let _ = inner.push(&cancel);
        }
        // Nothing queued may still name this descriptor once it is closed and
        // the number reused. *(Senior review of PR #110, M2)* If the queue
        // could not be emptied, the socket is kept open until a flush empties
        // it (`Inner::settle`) — or, with that list full, never closed.
        if !inner.flush()
            && let Some(t) = tcp
        {
            if inner.state.deferred.len() < inner.state.deferred.capacity() {
                inner.state.deferred.push(t);
            } else {
                std::mem::forget(t);
            }
        }
    }
}

/// `hft`'s idle strategy over a [`Uring`]: enter the kernel once, never wait,
/// reap.
pub struct UringSpin {
    inner: Rc<Shared>,
}

impl Waiting for UringSpin {
    const SLEEPS: bool = false;
    const NEEDS_SOURCES: bool = false;
    const REAPS: bool = true;

    fn idle(&mut self, _interests: &[Interest]) {
        if let Ok(mut inner) = self.inner.cell.try_borrow_mut() {
            inner.reap_hft();
        }
    }
}

/// `standard`'s idle strategy over a [`Uring`]: wait in the kernel for one
/// completion or the timeout, reap.
///
/// # `block::Block` cannot stand in for it
///
/// `Block` waits in `poll(2)` and never looks at the ring, so a
/// [`UringTransport`] under it would sleep through every completion. The
/// pairing is refused when it is compiled, as it is for `Spin`:
///
/// ```compile_fail,E0080
/// # struct App;
/// # impl fixbolt_session::Application for App {
/// #     fn on_message(&mut self, _m: &[u8], _h: fixbolt_session::Header<'_>, _o: &mut [u8])
/// #         -> Option<core::ops::Range<usize>> { None }
/// # }
/// use fixbolt_engine::{Engine, block::Block, clock::SystemClock};
/// use fixbolt_engine::{dispatch::InlineDispatch, journal::Store};
/// use fixbolt_engine::transport::uring::UringTransport;
///
/// let _engine: Engine<
///     UringTransport, fixbolt_session::Acceptor, InlineDispatch<App>,
///     SystemClock, Block, Store, 256, 4096, 8192,
/// > = Engine::new(
///     fixbolt_session::Config::acceptor(b"FIX.4.4", b"ISLD", b"TEST"),
///     InlineDispatch::new(App),
///     SystemClock,
///     Block::new(8),
///     4,
/// );
/// ```
#[cfg(feature = "standard")]
pub struct UringBlock {
    inner: Rc<Shared>,
    timeout_ms: u32,
}

#[cfg(feature = "standard")]
impl UringBlock {
    /// The same strategy, waiting at most `timeout_ms` per idle turn instead of
    /// [`crate::block::DEFAULT_TIMEOUT_MS`]. Raised to
    /// [`crate::block::MIN_TIMEOUT_MS`], as [`crate::block::Block`] does.
    #[must_use]
    pub fn with_timeout_ms(mut self, timeout_ms: u32) -> Self {
        self.timeout_ms = timeout_ms.max(crate::block::MIN_TIMEOUT_MS);
        self
    }

    /// The timeout in force.
    #[must_use]
    pub const fn timeout_ms(&self) -> u32 {
        self.timeout_ms
    }
}

#[cfg(feature = "standard")]
impl Waiting for UringBlock {
    const SLEEPS: bool = true;
    const NEEDS_SOURCES: bool = true;
    const REAPS: bool = true;

    fn idle(&mut self, interests: &[Interest]) {
        if let Ok(mut inner) = self.inner.cell.try_borrow_mut() {
            inner.reap_standard(interests, self.timeout_ms);
        }
    }
}

#[cfg(test)]
// A unit test here builds the ledger without a ring; an `expect` that fails is
// a failing test, which is what a test is for.
#[allow(clippy::expect_used)]
mod tests {
    use super::{KERNEL, KIND_RECV, Region, State, UringArm, UringConfig, staged_span, user_data};

    /// Senior review of PR #110, L2: a completion that **fails** but carries a
    /// buffer (`IORING_CQE_F_BUFFER`) hands that buffer back to the kernel.
    /// Without it the ledger still reads "with the kernel" — the check cannot
    /// see this leak — while the ring has one buffer fewer for ever; the tail
    /// is what shows it.
    #[test]
    fn a_failed_completion_that_carries_a_buffer_gives_it_back() {
        const F_BUFFER: u32 = 1;
        let config = UringConfig::new(8, 4096, 1).expect("a valid ring size");
        let region = Region::new(&config).expect("buffer memory");
        let mut state = State::new(region, config, UringArm::Enter);
        for bid in 0..config.buffers_per_connection {
            state.provide(0, bid);
        }
        let (slot, generation) = state.open(1_000).expect("a free slot");
        let tail = state.rings.first().map(|r| r.tail).expect("slot 0's ring");
        state.complete(
            user_data(KIND_RECV, slot, generation),
            -libc::ECONNRESET,
            F_BUFFER | (3 << 16),
        );
        assert_eq!(
            state.rings.first().map(|r| r.tail),
            Some(tail.wrapping_add(1)),
            "buffer 3 was not handed back to the kernel"
        );
        assert_eq!(state.owner.get(3).copied(), Some(KERNEL));
        assert!(state.accounted_for());
        assert_eq!(
            state.rings.first().map(|r| r.with_kernel),
            Some(u32::from(config.buffers_per_connection))
        );
    }

    /// U5: only a span inside one buffer is ever turned into a slice.
    #[test]
    fn a_completion_span_is_accepted_only_inside_one_buffer() {
        assert_eq!(staged_span(Some(3), 100, 8, 4096), Some((3, 100)));
        assert_eq!(staged_span(Some(7), 4096, 8, 4096), Some((7, 4096)));
        assert_eq!(staged_span(None, 100, 8, 4096), None, "no buffer named");
        assert_eq!(staged_span(Some(8), 100, 8, 4096), None, "id past the end");
        assert_eq!(staged_span(Some(u16::MAX), 1, 8, 4096), None);
        assert_eq!(
            staged_span(Some(0), 4097, 8, 4096),
            None,
            "longer than a buffer"
        );
        assert_eq!(staged_span(Some(0), 0, 8, 4096), None, "no bytes");
        assert_eq!(staged_span(Some(0), -105, 8, 4096), None, "an errno");
    }
}
