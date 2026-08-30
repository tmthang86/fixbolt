//! How a thread that is not the engine says *"look again"*.
//!
//! ADR-0014 decision 6. In `hft` this problem does not exist: the engine is
//! spinning, so anything another thread produces is seen on the next turn,
//! microseconds later. In `standard` the engine is **asleep inside `poll`**, and
//! `poll` wakes for descriptors — not for a ring buffer, a queue, or a flag.
//!
//! So a reply produced by [`crate::dispatch::RingApp`] on the application's
//! thread would sit until the engine's timeout expired. Not lost, not wrong:
//! **up to 100 ms late**, on the path an application that chose an out-of-band
//! dispatch cares most about. That is the third of the four latency cliffs
//! ADR-0014 decision 6 names, and it is the one that needs a mechanism rather
//! than a list entry.
//!
//! # A self-pipe, not `eventfd`
//!
//! `eventfd` is Linux-only and would split this path across two platforms for
//! nothing. A pipe is POSIX, is one descriptor at each end, and `poll` already
//! knows how to wait on it.
//!
//! # Which end writes, and a correction to ADR-0014
//!
//! **`[2026-08-30]` ADR-0014 decision 6 says *"`RingDispatch` writes one byte on
//! push"*. That names the wrong end, and it is recorded here rather than
//! quietly fixed** — `CLAUDE.md` §5 forbids editing an accepted ADR's
//! substance, and this is a factual error in it, not a change of mind. The
//! mechanism, the reason and the requirement are all unchanged.
//!
//! [`crate::dispatch::RingDispatch`] runs on the **engine** thread: `deliver`
//! and `collect` are both called from `Engine::turn`, at a moment when the
//! engine is by definition awake. It never needs to wake anybody. The thread
//! that must do the waking is the **application's**, when
//! [`crate::dispatch::RingApp::pump`] pushes a reply back.

use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::sync::Arc;

use crate::transport::Source;

/// Both ends, held together.
///
/// **They are not separable, and that is the whole point.** Writing to a pipe
/// whose read end has closed raises `SIGPIPE`, whose **default action
/// terminates the process** — and a library cannot assume otherwise. Rust's
/// runtime sets `SIGPIPE` to `SIG_IGN` before `main`, which makes the bug
/// invisible from an ordinary Rust binary; a `cdylib` loaded into a C program,
/// or a `main` that restores the default, gets the default.
///
/// So the read end is owned jointly: dropping the [`Waker`] while any
/// [`WakeHandle`] survives does **not** close it, and a wake that arrives after
/// the engine has gone lands in a pipe nobody will read. That is not a leak —
/// the handle is the thing keeping it open, deliberately, and the descriptors
/// go when the last one does. `[measured 2026-08-30]` before this, the same
/// sequence killed the test binary with `signal: 13`.
struct Pipe {
    read: OwnedFd,
    write: OwnedFd,
}

/// The engine's end of the self-pipe: the descriptor to wait on, and the drain.
pub struct Waker {
    pipe: Arc<Pipe>,
}

/// The other threads' end. Cheap to clone, and safe to hold anywhere —
/// **including after the engine it belonged to has been dropped**.
#[derive(Clone)]
pub struct WakeHandle {
    pipe: Arc<Pipe>,
}

impl Waker {
    /// A new self-pipe. The [`WakeHandle`] goes to whoever will produce work.
    ///
    /// Both ends are non-blocking and close-on-exec.
    ///
    /// # Errors
    ///
    /// Whatever `pipe` or `fcntl` returns.
    pub fn new() -> io::Result<(Self, WakeHandle)> {
        let mut fds = [0 as libc::c_int; 2];
        // SAFETY: `pipe` writes exactly two `c_int`s into the array it is given,
        // and `fds` is a live array of exactly two. It retains no pointer.
        // Proven by `crates/engine/tests/standard.rs`, which wakes a real
        // blocked `poll` through the pair this returns.
        #[allow(unsafe_code)]
        let rc = unsafe { libc::pipe(fds.as_mut_ptr()) };
        if rc != 0 {
            return Err(io::Error::last_os_error());
        }

        // SAFETY: `pipe` returned 0, so both descriptors are open and owned by
        // this process and by nothing else. `OwnedFd` closes them on drop.
        #[allow(unsafe_code)]
        let (read, write) = unsafe { (OwnedFd::from_raw_fd(fds[0]), OwnedFd::from_raw_fd(fds[1])) };

        // `pipe2` would do this in the same call, but it is Linux-only and this
        // mode promises Linux **and** macOS. Two `fcntl`s at startup cost
        // nothing that matters.
        set_nonblocking_cloexec(&read)?;
        set_nonblocking_cloexec(&write)?;

        let pipe = Arc::new(Pipe { read, write });
        Ok((
            Self {
                pipe: Arc::clone(&pipe),
            },
            WakeHandle { pipe },
        ))
    }

    /// The descriptor to put in the poll set.
    #[must_use]
    pub fn source(&self) -> Source {
        Source::from_raw_fd(self.pipe.read.as_raw_fd())
    }

    /// Throw away everything queued in the pipe.
    ///
    /// **Forgetting this turns `standard` into a spin**, and does it silently:
    /// a pipe with an unread byte in it stays readable, so every subsequent
    /// `poll` returns immediately, for ever. The engine still works — it is
    /// simply burning a core again, which is the one thing this mode exists to
    /// avoid. `crates/engine/tests/standard.rs` has the test, and its reversal
    /// is deleting this call.
    pub fn drain(&self) {
        let mut buf = [0u8; 64];
        loop {
            // SAFETY: reads at most `buf.len()` bytes into a live local buffer
            // through a descriptor this struct owns and keeps open for the call.
            #[allow(unsafe_code)]
            let n = unsafe {
                libc::read(
                    self.pipe.read.as_raw_fd(),
                    buf.as_mut_ptr().cast::<libc::c_void>(),
                    buf.len(),
                )
            };
            // A full buffer means there may be more; anything else — a short
            // read, zero, or the `EAGAIN` a non-blocking empty pipe returns as
            // -1 — means it is empty now. `try_from` rather than a cast: this
            // is a signed count that can legitimately be negative, and `as`
            // would turn that into an enormous length.
            match usize::try_from(n) {
                Ok(got) if got == buf.len() => {}
                _ => return,
            }
        }
    }
}

impl WakeHandle {
    /// Tell the engine to look again. Never blocks.
    ///
    /// **A refused write is not an error and is not dropped work.** The pipe is
    /// non-blocking, so once it holds unread bytes a further write returns
    /// `EAGAIN` — and a pipe with unread bytes in it is already readable, which
    /// is the entire signal. One pending wake and a thousand mean the same
    /// thing: *look again*.
    ///
    /// **Safe after the engine has gone**, which is a shutdown race that will
    /// happen: the application thread pushes one last reply while the engine is
    /// being dropped. The read end is held jointly with this handle (see
    /// [`Pipe`]), so there is always a reader and the write can never raise
    /// `SIGPIPE`. Those wakes accumulate in a pipe nobody drains and then stop
    /// at `EAGAIN`, which is exactly right — nobody is listening.
    pub fn wake(&self) {
        let byte = 1u8;
        // SAFETY: writes exactly one byte from a live local through a
        // descriptor this handle owns via `Arc<OwnedFd>`, which keeps it open
        // for at least the duration of the call. The return value is
        // deliberately ignored — see above.
        #[allow(unsafe_code)]
        unsafe {
            libc::write(
                self.pipe.write.as_raw_fd(),
                std::ptr::from_ref(&byte).cast::<libc::c_void>(),
                1,
            );
        }
    }
}

fn set_nonblocking_cloexec(fd: &OwnedFd) -> io::Result<()> {
    let raw = fd.as_raw_fd();

    // SAFETY: each call reads or writes only the descriptor flags of `raw`,
    // which is open and owned by the caller for the whole of this function.
    // `fcntl` retains nothing.
    #[allow(unsafe_code)]
    let flags = unsafe { libc::fcntl(raw, libc::F_GETFL) };
    if flags < 0 {
        return Err(io::Error::last_os_error());
    }

    #[allow(unsafe_code)]
    // SAFETY: as above.
    let rc = unsafe { libc::fcntl(raw, libc::F_SETFL, flags | libc::O_NONBLOCK) };
    if rc < 0 {
        return Err(io::Error::last_os_error());
    }

    #[allow(unsafe_code)]
    // SAFETY: as above.
    let rc = unsafe { libc::fcntl(raw, libc::F_SETFD, libc::FD_CLOEXEC) };
    if rc < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}
