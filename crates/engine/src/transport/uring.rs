//! An `io_uring` transport: receive is a multishot `recv` into a provided
//! buffer ring, reaped by the idle strategy — [ADR-0190].
//!
//! # The shape
//!
//! A [`Uring`] owns one ring, one provided-buffer ring and a fixed slab of
//! per-connection staging lists. It is `!Send` — one ring per engine thread,
//! which is what `IORING_SETUP_SINGLE_ISSUER` demands anyway. From it come:
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
//! `[step 1 of docs/plans/2026-09-24-p4-io-uring-transport.md]` **types and
//! signatures only.** Every constructor refuses with [`UringRefused::Other`];
//! the mechanism arrives in steps 2 and 3, against the tests already written
//! red in `crates/engine/tests/uring.rs`.
//!
//! [ADR-0190]: ../../../../docs/decisions/ADR-0190-the-io-uring-transport-is-reaped-by-the-idle-strategy-and-an-hft-turn-enters-the-kernel-once-without-waiting.md

use std::fmt;
use std::io;
use std::rc::Rc;

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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HftArm {
    /// Every idle turn is one `io_uring_enter(to_submit, 0, GETEVENTS)`: the
    /// engine thread enters the kernel and never waits there.
    #[default]
    Enter,
    /// A kernel thread polls the submission queue, pinned to `core`, and the
    /// engine thread only reads the completion queue. **Burns a second core.**
    Sqpoll {
        /// The CPU the kernel's SQ thread is bound to (`sq_thread_cpu`).
        core: u32,
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
    pub const fn classify(errno: i32, io_uring_disabled: Option<u8>) -> Self {
        // Step 1: a signature. The table arrives in step 2.
        let _ = (errno, io_uring_disabled);
        Self::Other(io::ErrorKind::Unsupported)
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

/// What a [`Uring`] and everything made from it share.
///
/// `[step 1]` empty: the ring, the buffer ring and the connection slab arrive
/// in step 2.
#[expect(
    dead_code,
    reason = "step 1 skeleton: nothing constructs a ring until step 2 of the plan"
)]
struct Shared {
    config: UringConfig,
    arm: UringArm,
}

/// One ring, its provided buffers and its connection slab. `!Send`: one per
/// engine thread.
pub struct Uring {
    shared: Rc<Shared>,
}

impl Uring {
    /// An `hft` ring, and the idle strategy that reaps it.
    ///
    /// Made **before** any listener is bound, so a refusal leaves nothing
    /// half-open.
    ///
    /// # Errors
    ///
    /// [`UringRefused`] naming why the kernel will not give this process a
    /// ring.
    pub fn hft(config: UringConfig, arm: HftArm) -> Result<(Self, UringSpin), UringRefused> {
        let _ = (config, arm);
        Err(UringRefused::Other(io::ErrorKind::Unsupported))
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

    /// Put `transport` on the ring. `None` when every slot is taken — the
    /// socket is then dropped, the answer `pump` already gives a connection it
    /// has no room for.
    #[must_use]
    pub fn register(&self, transport: TcpTransport) -> Option<UringTransport> {
        let _ = (&self.shared, transport);
        None
    }

    /// What this ring has done so far.
    #[must_use]
    pub fn report(&self) -> UringReport {
        let _ = &self.shared;
        UringReport {
            arm: UringArm::Enter,
            cqes: 0,
            bytes: 0,
            enobufs: 0,
            rearms: 0,
            stale: 0,
        }
    }

    /// Whether every provided buffer is in exactly one place: with the
    /// kernel, or in exactly one connection's staging list.
    ///
    /// **The ownership ledger, for tests** (plan *Bất biến* 8, U5 and U6): no
    /// sanitizer sees the kernel write into this memory, so what proves a
    /// buffer is never handed back while still read is this check, run after
    /// every reap in `crates/engine/tests/uring.rs`.
    #[doc(hidden)]
    #[must_use]
    pub fn buffers_accounted_for(&self) -> bool {
        let _ = &self.shared;
        false
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
        let _ = &self.shared;
        (0, 0)
    }
}

/// One connection on a [`Uring`].
pub struct UringTransport {
    tcp: TcpTransport,
}

impl Transport for UringTransport {
    const POLLABLE: bool = true;

    fn recv(&mut self, buf: &mut [u8]) -> Io {
        let _ = buf;
        Io::Idle
    }

    fn send(&mut self, buf: &[u8]) -> Io {
        self.tcp.send(buf)
    }

    fn source(&self) -> Option<Source> {
        self.tcp.source()
    }
}

/// `hft`'s idle strategy over a [`Uring`]: enter the kernel once, never wait,
/// reap.
pub struct UringSpin {
    #[expect(
        dead_code,
        reason = "step 1 skeleton: the reap arrives in step 2 of the plan"
    )]
    shared: Rc<Shared>,
}

impl Waiting for UringSpin {
    const SLEEPS: bool = false;
    const NEEDS_SOURCES: bool = false;

    fn idle(&mut self, _interests: &[Interest]) {
        core::hint::spin_loop();
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
    shared: Rc<Shared>,
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
