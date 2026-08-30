//! `poll(2)`: the one syscall `standard` mode is built on.
//!
//! ADR-0014 decision 1. `std` exposes no readiness API at all — `set_nonblocking`
//! and `WouldBlock` are its whole surface — so blocking until a socket has
//! something to say means leaving `std`. `poll(2)` is the smallest thing that
//! does it on Linux and on macOS, it is in `libc`, and it needs no runtime.
//!
//! This module is the raw call and nothing else. The idle strategy built on it
//! is `wait::Block`, and the difference matters: everything here is about
//! *asking the kernel once*, and the timeout policy, the retry on `EINTR`, and
//! the tick granularity are decisions that live one layer up.
//!
//! # What it costs, said plainly
//!
//! `poll(2)` is **O(N) in registered descriptors per wakeup** where `epoll` is
//! O(1). At the shape `standard` serves — many sessions on one blocking thread
//! — that is a real term and it is **unmeasured**. ADR-0014 accepted it as a
//! debt and recorded it as its own open question 2 rather than calling it fine.
//!
//! # The `unsafe`
//!
//! One block, around one call. What proves it sound is not this comment but
//! `crates/engine/tests/standard.rs`, which drives real sockets through it and
//! checks the answer against which socket actually had data — including the
//! reversal, where a [`Poller`] handed the wrong descriptor must report the
//! wrong thing.

use crate::transport::Interest;

/// Why a wait did not produce an answer.
///
/// Fieldless but for the OS error number, which is `Copy` — nothing here
/// allocates, including on the failure path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PollError {
    /// A signal arrived while waiting. **Not an error**, and not a wakeup
    /// either: the caller goes back and waits again. It is reported rather than
    /// swallowed here because "how long is left of the timeout" is the caller's
    /// question, not this module's.
    Interrupted,
    /// The kernel does not know one of these descriptors. Never reported as
    /// *ready*, which is the distinction that lets a wrong-descriptor bug be
    /// caught rather than read as a quiet socket.
    BadSource,
    /// The OS refused, and this is its `errno`.
    Failed(i32),
}

/// A `poll(2)` caller that owns its scratch buffer.
///
/// The buffer exists because non-negotiable 1 forbids allocating on the idle
/// path, and building an array of `pollfd` per wakeup is exactly that. It is
/// reserved once and refilled in place.
pub struct Poller {
    fds: Vec<libc::pollfd>,
}

/// What one wait saw.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ready {
    /// How many sources are readable, writable, or have hung up.
    pub count: usize,
}

impl Poller {
    /// A poller sized for `capacity` sources.
    ///
    /// Size it for every source an idle turn can carry — one per connection,
    /// plus the listener, plus the waker. Going over is not fatal; it costs one
    /// allocation on a path that must not have any, and [`Self::wait`] says so.
    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            fds: Vec::with_capacity(capacity),
        }
    }

    /// Block until one of `interests` is ready, or `timeout_ms` passes.
    ///
    /// `timeout_ms` is milliseconds; a negative value waits forever, and `0`
    /// returns immediately — which is not a wait at all, and is how a caller
    /// asks *"is anything ready right now"*.
    ///
    /// # Errors
    ///
    /// [`PollError::Interrupted`] if a signal arrived, [`PollError::BadSource`]
    /// if the kernel does not recognise a descriptor, [`PollError::Failed`]
    /// otherwise.
    pub fn wait(&mut self, interests: &[Interest], timeout_ms: i32) -> Result<Ready, PollError> {
        self.fds.clear();
        // Reserving here would be an allocation on the idle path if it ever
        // fired. It fires only when the caller sized this poller smaller than
        // the number of sources it went on to carry, which is a startup
        // mistake rather than a steady state — and refusing to wait would turn
        // that mistake into a hung engine, which is worse than one allocation.
        if interests.len() > self.fds.capacity() {
            self.fds.reserve(interests.len() - self.fds.capacity());
        }
        for interest in interests {
            let mut events = libc::POLLIN;
            if interest.writable {
                events |= libc::POLLOUT;
            }
            self.fds.push(libc::pollfd {
                fd: interest.source.as_raw_fd(),
                events,
                revents: 0,
            });
        }

        // SAFETY: `fds` is a live, contiguous slice of `libc::pollfd` owned by
        // `self` for the duration of the call, and its length is passed as the
        // count, so the kernel writes `revents` only inside it. Every `fd` came
        // from a `Source`, which is only ever produced from a socket the caller
        // still holds. `poll` does not retain the pointer.
        //
        // What proves it: `crates/engine/tests/standard.rs`, which asserts the
        // answer against which socket actually had bytes — and its reversal,
        // where a deliberately wrong descriptor must NOT read as a quiet socket.
        #[allow(unsafe_code)]
        let rc = unsafe {
            libc::poll(
                self.fds.as_mut_ptr(),
                self.fds.len() as libc::nfds_t,
                timeout_ms,
            )
        };

        if rc < 0 {
            let errno = std::io::Error::last_os_error().raw_os_error().unwrap_or(0);
            return if errno == libc::EINTR {
                Err(PollError::Interrupted)
            } else {
                Err(PollError::Failed(errno))
            };
        }

        // **`POLLNVAL` is not readiness.** `poll` counts a descriptor the
        // kernel does not know among its return value, so taking `rc` at face
        // value would report a wrong descriptor as a ready one — and a test
        // reversing `source()` would stay green. Counting the flags instead is
        // what makes that reversal go red.
        let mut count = 0;
        for pfd in &self.fds {
            if pfd.revents & libc::POLLNVAL != 0 {
                return Err(PollError::BadSource);
            }
            if pfd.revents != 0 {
                count += 1;
            }
        }
        Ok(Ready { count })
    }
}
