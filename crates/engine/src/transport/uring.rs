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
//! - [`UringSpin`] (`hft`) or [`UringBlock`] (`standard`): **the idle strategy
//!   is the reaper.** `idle` submits what is queued, enters the kernel —
//!   never waiting in `hft`, waiting with a timeout in `standard` — and moves
//!   every completion into its connection's staging list.
//!
//! # Blocked means refused, never a fallback
//!
//! [`Uring::hft`] and [`Uring::standard`] return [`UringRefused`] when the
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
//! | U2 | `register_buf_ring_with_flags` | explicit `Drop` for `Inner`; `unregistered_buffers_are_not_written_after_the_ring_is_dropped` |
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
/// and the ring's entry count is at most `1 << 15`.
pub const MAX_BUFFERS: u16 = 1 << 15;

/// How big the ring's receive side is. **No field has a hidden default**
/// (`CLAUDE.md` §6): the caller names all three and they are checked here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UringConfig {
    buffers: u16,
    buffer_len: u32,
    connections: u16,
}

/// Why a [`UringConfig`] was refused. Fieldless: nothing here allocates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UringConfigError {
    /// The buffer count is not a power of two in `1..=`[`MAX_BUFFERS`] — the
    /// provided-buffer ring's entry count must be one.
    Buffers,
    /// A buffer of zero bytes can hold nothing.
    BufferLen,
    /// A ring that can serve no connection.
    Connections,
}

impl fmt::Display for UringConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Buffers => "the io_uring buffer count must be a power of two, at most 32768",
            Self::BufferLen => "an io_uring buffer must be at least one byte long",
            Self::Connections => "an io_uring transport must serve at least one connection",
        })
    }
}

impl std::error::Error for UringConfigError {}

impl UringConfig {
    /// `buffers` provided buffers of `buffer_len` bytes each, shared by at most
    /// `connections` registered connections.
    ///
    /// # Errors
    ///
    /// [`UringConfigError`] naming the first field that is out of range.
    pub const fn new(
        buffers: u16,
        buffer_len: u32,
        connections: u16,
    ) -> Result<Self, UringConfigError> {
        if !buffers.is_power_of_two() || buffers > MAX_BUFFERS {
            return Err(UringConfigError::Buffers);
        }
        if buffer_len == 0 {
            return Err(UringConfigError::BufferLen);
        }
        if connections == 0 {
            return Err(UringConfigError::Connections);
        }
        Ok(Self {
            buffers,
            buffer_len,
            connections,
        })
    }

    /// How many provided buffers.
    #[must_use]
    pub const fn buffers(&self) -> u16 {
        self.buffers
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

    /// Every provided buffer together, in bytes — what is allocated and
    /// pre-faulted when the ring is made.
    #[must_use]
    pub const fn buffer_bytes(&self) -> u64 {
        self.buffers as u64 * self.buffer_len as u64
    }
}

/// How an `hft` ring is reaped. [`HftArm::Enter`] unless the caller names the
/// other one — ADR-0190 decision 4, the owner's Q8.
///
/// **[`HftArm::Sqpoll`] exists only when the `affinity` feature is on too.**
/// Its core is an [`crate::affinity::CoreId`], the type every other pinned
/// core in this crate has, and it is validated through
/// [`crate::affinity::Topology`] by the serving entry point before any socket
/// exists — the same check `serve_hft_pinned` makes. Without `affinity` the
/// variant is not there, so an unpinned SQ thread cannot be asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HftArm {
    /// Every idle turn is one `io_uring_enter(to_submit, 0, GETEVENTS)`: the
    /// engine thread enters the kernel and never waits there.
    #[default]
    Enter,
    /// A kernel thread polls the submission queue, pinned to `core`, and the
    /// engine thread only reads the completion queue. **Burns a second core.**
    #[cfg(feature = "affinity")]
    Sqpoll {
        /// The CPU the kernel's SQ thread is bound to (`sq_thread_cpu`).
        core: crate::affinity::CoreId,
    },
}

/// Which reaping the ring is **actually** doing, read back from what the
/// kernel returned at setup rather than from what was asked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UringArm {
    /// `hft`, [`HftArm::Enter`].
    Enter,
    /// `hft`, [`HftArm::Sqpoll`].
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
            Self::Other(kind) => write!(f, "io_uring could not be set up: {kind}"),
        }
    }
}

impl std::error::Error for UringRefused {}

/// The one provided-buffer group this transport registers.
const BGID: u16 = 0;
/// The buffer memory's alignment: a page on every Linux target (4, 16 and
/// 64 KiB pages), which the kernel requires of a buffer ring it maps.
const ALIGN: usize = 1 << 16;
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

/// The buffer ring's entries and the buffers themselves, in one page-aligned
/// allocation, every page written once when it is made.
struct Region {
    base: NonNull<u8>,
    layout: Layout,
    /// Where the buffers start: after the entries, on a 4 KiB boundary.
    data_at: usize,
    buffer_len: usize,
}

impl Region {
    fn new(config: &UringConfig) -> Result<Self, UringRefused> {
        let too_big = UringRefused::Other(io::ErrorKind::OutOfMemory);
        let entries = usize::from(config.buffers) * core::mem::size_of::<BufRingEntry>();
        let data_at = entries
            .checked_next_multiple_of(TOUCH_STRIDE)
            .ok_or(too_big)?;
        let data = usize::try_from(config.buffer_bytes()).map_err(|_| too_big)?;
        let total = data_at.checked_add(data).ok_or(too_big)?;
        let layout = Layout::from_size_align(total, ALIGN).map_err(|_| too_big)?;
        let buffer_len = usize::try_from(config.buffer_len).map_err(|_| too_big)?;

        // SAFETY (U1): `layout` has a non-zero size — at least one entry and
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
            data_at,
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

    /// The first buffer-ring entry. Entry 0's `resv` is the tail.
    const fn entries(&self) -> *mut BufRingEntry {
        self.base.as_ptr().cast()
    }

    /// The address the kernel writes buffer `bid` at. `bid < buffers`.
    fn buffer_addr(&self, bid: u16) -> u64 {
        let at = self.data_at + usize::from(bid) * self.buffer_len;
        self.base.as_ptr().wrapping_add(at) as u64
    }

    /// The first `len` bytes of buffer `bid`.
    ///
    /// **Called only with a span `staged_span` accepted**: `bid < buffers`
    /// and `len <= buffer_len`, so the slice is inside the allocation.
    fn staged(&self, bid: u16, len: u32) -> &[u8] {
        let at = self.data_at + usize::from(bid) * self.buffer_len;
        let len = (len as usize).min(self.buffer_len);
        // SAFETY (U5): `bid` and `len` passed `staged_span` when the
        // completion was reaped, and the buffer is on a staging list — the
        // ledger's promise that the kernel no longer owns it and will not
        // write it until `provide` hands it back, which happens only after the
        // last byte is copied out. So `at + len` is inside the allocation and
        // the memory is not written while this borrow lives. Proved by
        // `staged_span`'s unit test and by
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
        // freed once. `Inner`'s `Drop` has unregistered the buffer ring and
        // closed the ring before this runs (field order), so the kernel holds
        // no address inside it. Proved by
        // `unregistered_buffers_are_not_written_after_the_ring_is_dropped`.
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
    #[expect(dead_code, reason = "step 1 skeleton: `standard` arrives in step 3")]
    Block,
}

/// Everything but the ring itself.
struct State {
    region: Region,
    config: UringConfig,
    /// `buffers - 1`: a ring index from a free-running tail.
    mask: u16,
    /// The buffer-ring tail this side has published.
    tail: u16,
    /// Per buffer: [`KERNEL`], or the slot whose staging list holds it.
    owner: Box<[u32]>,
    /// Per buffer: how many bytes its completion delivered.
    len: Box<[u32]>,
    /// Per buffer: the next buffer on the same staging list.
    next: Box<[u16]>,
    slots: Box<[Slot]>,
    /// Free slots, a stack.
    free: Box<[u32]>,
    free_len: usize,
    /// How many buffers the ledger says the kernel has.
    kernel_owned: u32,
    /// How many slots have `rearm` set.
    rearm_pending: u32,
    arm: UringArm,
    cqes: u64,
    bytes: u64,
    enobufs: u64,
    rearms: u64,
    stale: u64,
    enter_errors: u64,
}

impl State {
    fn new(region: Region, config: UringConfig, arm: UringArm) -> Self {
        let buffers = usize::from(config.buffers);
        let connections = usize::from(config.connections);
        Self {
            region,
            config,
            mask: config.buffers.wrapping_sub(1),
            tail: 0,
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
            kernel_owned: 0,
            rearm_pending: 0,
            arm,
            cqes: 0,
            bytes: 0,
            enobufs: 0,
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
        }
    }

    /// Hand buffer `bid` to the kernel: write its ring entry at the tail and
    /// publish the tail.
    fn provide(&mut self, bid: u16) {
        let idx = usize::from(self.tail & self.mask);
        let addr = self.region.buffer_addr(bid);
        let entries = self.region.entries();
        // SAFETY (U6): `idx < buffers`, so the entry is inside the ring's
        // memory. The ledger never has more than `buffers` buffers with the
        // kernel, so the entry at the tail is one the kernel has already
        // consumed and does not read until the tail below says so. The writes
        // go through raw offsets of the kernel ABI layout (asserted above),
        // never through a `&mut` spanning entry 0's `resv`, which is the tail.
        // The tail is a `u16` inside that allocation, aligned (offset 14 of an
        // 8-aligned entry), and published with `Release` so the kernel sees
        // the entry first. Proved by the ledger and the byte-exact tests.
        #[allow(unsafe_code)]
        unsafe {
            let entry = entries.add(idx).cast::<u8>();
            entry.cast::<u64>().write(addr);
            entry
                .add(ENTRY_LEN_AT)
                .cast::<u32>()
                .write(self.config.buffer_len);
            entry.add(ENTRY_BID_AT).cast::<u16>().write(bid);
            self.tail = self.tail.wrapping_add(1);
            let tail = BufRingEntry::tail(entries).cast::<AtomicU16>();
            (*tail).store(self.tail, Ordering::Release);
        }
        if let Some(o) = self.owner.get_mut(usize::from(bid)) {
            *o = KERNEL;
        }
        self.kernel_owned += 1;
    }

    /// Take buffer `bid` from the kernel for `to`. `false` if the ledger does
    /// not have it with the kernel — a completion naming a buffer it was not
    /// given, which is never staged.
    fn take(&mut self, bid: u16, to: u32) -> bool {
        match self.owner.get_mut(usize::from(bid)) {
            Some(o) if *o == KERNEL => {
                *o = to;
                self.kernel_owned = self.kernel_owned.saturating_sub(1);
                true
            }
            _ => false,
        }
    }

    /// A buffer the kernel selected for a completion nobody will read: back
    /// to the kernel at once.
    fn give_back(&mut self, bid: Option<u16>) {
        // `take` to `KERNEL`, then `provide`: the ledger's count drops and
        // rises by one, and the ring gets the entry back.
        if let Some(b) = bid
            && b < self.config.buffers
            && self.take(b, KERNEL)
        {
            self.provide(b);
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
        let b = usize::from(bid);
        if let Some(l) = self.len.get_mut(b) {
            *l = len;
        }
        if let Some(n) = self.next.get_mut(b) {
            *n = NIL;
        }
        let Some(s) = self.slots.get_mut(slot) else {
            return;
        };
        let last = s.tail;
        s.tail = bid;
        if last == NIL {
            s.head = bid;
        } else if let Some(n) = self.next.get_mut(usize::from(last)) {
            *n = bid;
        }
    }

    /// One completion.
    fn complete(&mut self, ud: u64, res: i32, flags: u32) {
        self.cqes += 1;
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
            // bytes reach nobody, and the buffer goes straight back.
            self.stale += 1;
            self.give_back(bid);
            return;
        }
        if res > 0 {
            match staged_span(bid, res, self.config.buffers, self.config.buffer_len) {
                Some((b, len)) if self.take(b, slot as u32) => {
                    self.append(slot, b, len);
                    self.bytes += u64::from(len);
                }
                _ => {
                    self.give_back(bid);
                    self.end(slot, Io::Failed(io::ErrorKind::InvalidData));
                }
            }
            if !more {
                self.mark_rearm(slot);
            }
        } else if res == 0 {
            self.give_back(bid);
            self.end(slot, Io::Closed);
        } else if res == -libc::ENOBUFS {
            // Every buffer is out. The bytes stay in the socket — TCP's own
            // backpressure — and the receive is submitted again as soon as a
            // buffer is back with the kernel.
            self.enobufs += 1;
            if !more {
                self.mark_rearm(slot);
            }
        } else {
            let kind = io::Error::from_raw_os_error(res.saturating_neg()).kind();
            self.end(slot, Io::Failed(kind));
        }
    }

    /// Copy what `slot` has staged into `buf`, handing each emptied buffer
    /// back to the kernel. No system call.
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
            let b = usize::from(head);
            let len = self.len.get(b).copied().unwrap_or(0);
            let src = self
                .region
                .staged(head, len)
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
                head = self.next.get(b).copied().unwrap_or(NIL);
                if head == NIL {
                    tail = NIL;
                }
                at = 0;
                self.provide(done);
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
        Some((slot, s.generation))
    }

    /// Free `slot`: its staged buffers go back to the kernel, its generation
    /// moves on, and it can be handed out again at once.
    fn release(&mut self, slot: u32) {
        let ix = slot as usize;
        let Some(&s) = self.slots.get(ix) else {
            return;
        };
        let mut b = s.head;
        while b != NIL {
            let next = self.next.get(usize::from(b)).copied().unwrap_or(NIL);
            self.provide(b);
            b = next;
        }
        if s.rearm {
            self.rearm_pending = self.rearm_pending.saturating_sub(1);
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

    fn accounted_for(&self) -> bool {
        let buffers = usize::from(self.config.buffers);
        let mut seen = vec![0u32; buffers];
        for (ix, s) in self.slots.iter().enumerate() {
            if (s.head == NIL) != (s.tail == NIL) || (!s.live && s.head != NIL) {
                return false;
            }
            let mut b = s.head;
            let mut steps = 0;
            while b != NIL {
                steps += 1;
                let (Some(&owner), Some(count)) =
                    (self.owner.get(usize::from(b)), seen.get_mut(usize::from(b)))
                else {
                    return false;
                };
                if steps > buffers || owner as usize != ix {
                    return false;
                }
                *count += 1;
                b = self.next.get(usize::from(b)).copied().unwrap_or(NIL);
            }
        }
        let mut with_kernel = 0u32;
        for (owner, count) in self.owner.iter().zip(seen.iter()) {
            match (*owner, *count) {
                (KERNEL, 0) => with_kernel += 1,
                (KERNEL, _) => return false,
                (_, 1) => {}
                _ => return false,
            }
        }
        with_kernel == self.kernel_owned
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
        let sq = (2 * connections + 8).next_power_of_two();
        let cq = (u32::from(config.buffers) + 4 * connections + 64).next_power_of_two();
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

        let arm = match setup {
            _ if ring.params().is_setup_sqpoll() => UringArm::Sqpoll,
            #[cfg(feature = "standard")]
            Setup::Block => UringArm::Block,
            _ => UringArm::Enter,
        };
        let mut state = State::new(Region::new(&config)?, config, arm);
        for bid in 0..config.buffers {
            state.provide(bid);
        }
        // SAFETY (U2): the entries are `buffers` (a power of two, at most
        // 32768) `io_uring_buf`s at a 64 KiB-aligned address, inside `state`'s
        // `Region`, which lives until `Inner` is dropped — and `Inner`'s
        // `Drop` unregisters this group before either the ring or the memory
        // goes. Proved by
        // `unregistered_buffers_are_not_written_after_the_ring_is_dropped`.
        #[allow(unsafe_code)]
        let registered = unsafe {
            ring.submitter().register_buf_ring_with_flags(
                state.region.entries() as u64,
                config.buffers,
                BGID,
                0,
            )
        };
        registered.map_err(|e| UringRefused::from_setup(&e))?;
        Ok(Self { ring, state })
    }

    /// Submit what is queued without waiting for anything. Under SQPOLL, wait
    /// — spinning, never sleeping — until the kernel thread has taken it.
    fn flush(&mut self) {
        if self.ring.submission().is_empty() {
            return;
        }
        if self.state.arm == UringArm::Sqpoll {
            for _ in 0..SQPOLL_FLUSH_SPINS {
                let sq = self.ring.submission();
                if sq.is_empty() {
                    return;
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
        if let Err(e) = r
            && e.raw_os_error() != Some(libc::EINTR)
        {
            self.state.enter_errors += 1;
        }
    }

    /// Queue a fresh multishot `recv` for every slot whose last one ended,
    /// while the kernel has a buffer to give it.
    fn rearm(&mut self) {
        if self.state.rearm_pending == 0 || self.state.kernel_owned == 0 {
            return;
        }
        for ix in 0..self.state.slots.len() {
            let Some(s) = self.state.slots.get(ix).copied() else {
                break;
            };
            if !s.rearm {
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
        let Self { ring, state } = self;
        for cqe in ring.completion() {
            state.complete(cqe.user_data(), cqe.result(), cqe.flags());
        }
    }
}

impl Drop for Inner {
    fn drop(&mut self) {
        // U2's other half: the group goes before the memory. The ring field
        // is dropped next (closing it cancels whatever is pending), and only
        // then `state`, whose `Region` frees the buffers.
        let _ = self.ring.submitter().unregister_buf_ring(BGID);
    }
}

/// A multishot, buffer-selected `recv` on `fd` for `slot` at `generation`.
fn recv(fd: std::os::fd::RawFd, slot: u32, generation: u32) -> squeue::Entry {
    opcode::RecvMulti::new(types::Fd(fd), BGID)
        .build()
        .user_data(user_data(KIND_RECV, slot, generation))
}

/// One ring, its provided buffers and its connection slab. `!Send`: one per
/// engine thread, made on that thread.
pub struct Uring {
    inner: Rc<RefCell<Inner>>,
}

impl Uring {
    /// An `hft` ring, and the idle strategy that reaps it.
    ///
    /// **Make it on the engine thread**, before any listener is bound: the
    /// ring is `SINGLE_ISSUER`, and a refusal then leaves nothing half-open.
    ///
    /// [`HftArm::Sqpoll`]'s core is handed to the kernel as `sq_thread_cpu`
    /// here and is **not** checked against `isolcpus` here: the serving entry
    /// point does that through [`crate::affinity::Topology`] before calling
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
            HftArm::Sqpoll { core } => Setup::Sqpoll(
                u32::try_from(core.0)
                    .map_err(|_| UringRefused::Other(io::ErrorKind::InvalidInput))?,
            ),
        };
        let inner = Rc::new(RefCell::new(Inner::new(config, setup)?));
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
    /// thread cannot be written (ADR-0190 decision 4).
    ///
    /// # Errors
    ///
    /// [`UringRefused`], as for [`Self::hft`].
    #[cfg(feature = "standard")]
    pub fn standard(config: UringConfig) -> Result<(Self, UringBlock), UringRefused> {
        let _ = config;
        Err(UringRefused::Other(io::ErrorKind::Unsupported))
    }

    /// Put `transport` on the ring: its multishot `recv` is queued and goes to
    /// the kernel on the next idle turn. `None` when every slot is taken — the
    /// socket is then dropped, the answer `pump` already gives a connection it
    /// has no room for. No allocation.
    #[must_use]
    pub fn register(&self, transport: TcpTransport) -> Option<UringTransport> {
        let mut inner = self.inner.try_borrow_mut().ok()?;
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
            tcp: transport,
        })
    }

    /// What this ring has done so far.
    #[must_use]
    pub fn report(&self) -> UringReport {
        self.inner.try_borrow().map_or(
            UringReport {
                arm: UringArm::Enter,
                cqes: 0,
                bytes: 0,
                enobufs: 0,
                rearms: 0,
                stale: 0,
                enter_errors: 0,
            },
            |i| i.state.report(),
        )
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
        self.inner.try_borrow().map_or((0, 0), |i| {
            (
                i.state.region.base.as_ptr() as usize,
                i.state.region.layout.size(),
            )
        })
    }
}

/// One connection on a [`Uring`].
///
/// Dropping it shuts the socket down (`shutdown(2)`, which ends the pending
/// `recv` and sends the peer its FIN at once), hands its staged buffers back,
/// moves its slot's generation on, and submits an `ASYNC_CANCEL` for its
/// `recv` before the descriptor is closed.
pub struct UringTransport {
    inner: Rc<RefCell<Inner>>,
    slot: u32,
    generation: u32,
    tcp: TcpTransport,
}

impl Transport for UringTransport {
    const POLLABLE: bool = true;

    fn recv(&mut self, buf: &mut [u8]) -> Io {
        if buf.is_empty() {
            return Io::Idle;
        }
        match self.inner.try_borrow_mut() {
            Ok(mut inner) => inner.state.read(self.slot as usize, self.generation, buf),
            Err(_) => Io::Failed(io::ErrorKind::WouldBlock),
        }
    }

    fn send(&mut self, buf: &[u8]) -> Io {
        self.tcp.send(buf)
    }

    fn source(&self) -> Option<Source> {
        self.tcp.source()
    }
}

impl Drop for UringTransport {
    fn drop(&mut self) {
        // U7, with no `unsafe`: `TcpStream::shutdown`. The socket is still
        // owned here — `tcp` is dropped after this body. `SHUT_RDWR` ends the
        // multishot `recv` and puts the FIN on the wire now, even though that
        // request still holds a reference to the file.
        let _ = self.tcp.socket().shutdown(std::net::Shutdown::Both);
        let Ok(mut inner) = self.inner.try_borrow_mut() else {
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
        // Nothing queued may still name this descriptor once `tcp` closes it
        // and the number is reused.
        inner.flush();
    }
}

/// `hft`'s idle strategy over a [`Uring`]: enter the kernel once, never wait,
/// reap.
pub struct UringSpin {
    inner: Rc<RefCell<Inner>>,
}

impl Waiting for UringSpin {
    const SLEEPS: bool = false;
    const NEEDS_SOURCES: bool = false;

    fn idle(&mut self, _interests: &[Interest]) {
        if let Ok(mut inner) = self.inner.try_borrow_mut() {
            inner.reap_hft();
        }
    }
}

/// `standard`'s idle strategy over a [`Uring`]: wait in the kernel for one
/// completion or the timeout, reap.
#[cfg(feature = "standard")]
pub struct UringBlock {
    #[expect(
        dead_code,
        reason = "step 1 skeleton: the reap arrives in step 3 of the plan"
    )]
    inner: Rc<RefCell<Inner>>,
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

    fn idle(&mut self, _interests: &[Interest]) {}
}

#[cfg(test)]
mod tests {
    use super::staged_span;

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
