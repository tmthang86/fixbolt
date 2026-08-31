//! Pinning a thread to a core, and proving it happened.
//!
//! [ADR-0015] is the decision. Two of its clauses shape everything here:
//!
//! * **Pin from inside the thread, as its first act.** `sched_setaffinity(0, …)`
//!   called by the thread itself has no window in which it runs unpinned.
//! * **A call returning `Ok` is not evidence.** [`pin_current_thread`] asks the
//!   kernel back with `sched_getaffinity` and compares, and
//!   [`ReadbackMismatch`](AffinityError::ReadbackMismatch) is a real outcome
//!   rather than a defensive gesture.
//!
//! Everything in this module runs at startup. It allocates (a `Vec` of cores, a
//! `String` read from `/proc`), and that is deliberate: non-negotiable 1 is
//! about the parse, serialise, session and dispatch hot paths, and none of this
//! is on one. `benches/alloc.rs` still has to read zero for a `turn()`.
//!
//! [ADR-0015]: ../../../docs/decisions/ADR-0015-explicit-cores-pinned-from-inside-and-read-back.md

use core::fmt;

/// A logical CPU as the kernel numbers it — what `taskset -c` takes and what
/// `/sys/devices/system/cpu/online` lists.
///
/// ADR-0015 decision 1: the caller names these. The engine never picks one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CoreId(pub usize);

impl fmt::Display for CoreId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "cpu{}", self.0)
    }
}

/// Why an affinity request was refused.
///
/// **Carries the offending core**, unlike the fieldless errors elsewhere in this
/// workspace. `CLAUDE.md` §6 asks for fieldless errors *where they sit on a hot
/// path*; this one is raised once, at startup, and `NotIsolated(cpu3)` tells an
/// operator what to change where `NotIsolated` does not — ADR-0015 decision 4.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum AffinityError {
    /// The kernel will not schedule anything on this id.
    NoSuchCore(CoreId),
    /// Present on this machine but currently offline.
    ///
    /// Distinct from [`NoSuchCore`](Self::NoSuchCore) on purpose:
    /// `[measured 2026-08-31]` the §9 desktop has `present 0-15` and
    /// `online 0-7`, so this is the one it actually hits.
    NotOnline(CoreId),
    /// Not in `isolcpus`, so the scheduler will put other work there.
    /// Escapable with an explicit `allow_unisolated`.
    NotIsolated(CoreId),
    /// Two shards on two SMT threads of one physical core.
    SmtSiblingOf(CoreId, CoreId),
    /// The same core named twice.
    DuplicateCore(CoreId),
    /// `EPERM`.
    Denied(CoreId),
    /// The call succeeded and the kernel's answer disagrees with the request.
    ReadbackMismatch(CoreId),
    /// A syscall failed for a reason this enum does not model, with its `errno`.
    ///
    /// Deliberately not folded into one of the variants above: reclassifying an
    /// unknown failure as a known one is how a diagnostic ends up confidently
    /// wrong, which this repository has already paid for once
    /// (`reference/ktls-on-a-plain-socket.md`).
    Failed(i32),
    /// A `/proc` or `/sys` file could not be read or made sense of. The `&str`
    /// is the path, so the message names the file rather than the symptom.
    Unreadable(&'static str),
}

impl fmt::Display for AffinityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoSuchCore(c) => write!(f, "{c} does not exist on this machine"),
            Self::NotOnline(c) => write!(f, "{c} exists but is offline"),
            Self::NotIsolated(c) => write!(
                f,
                "{c} is not in isolcpus, so the scheduler will put other work on it"
            ),
            Self::SmtSiblingOf(a, b) => {
                write!(f, "{a} and {b} are two threads of one physical core")
            }
            Self::DuplicateCore(c) => write!(f, "{c} was named more than once"),
            Self::Denied(c) => write!(f, "not permitted to set affinity to {c}"),
            Self::ReadbackMismatch(c) => write!(
                f,
                "setting affinity to {c} returned success and the kernel reports otherwise"
            ),
            Self::Failed(errno) => write!(f, "the affinity syscall failed with errno {errno}"),
            Self::Unreadable(path) => write!(f, "could not read {path}"),
        }
    }
}

impl std::error::Error for AffinityError {}

/// Room for 1024 CPUs. The kernel copies `min(len, its own mask size)` on the
/// way in and refuses a `get` whose buffer is smaller than that size, so this
/// is sized generously rather than exactly: `nr_cpu_ids` is a build-time
/// constant of the running kernel and is not ours to predict.
const MASK_WORDS: usize = 16;
const MASK_BYTES: usize = MASK_WORDS * core::mem::size_of::<u64>();

type Mask = [u64; MASK_WORDS];

/// Pin the calling thread to one core, then ask the kernel to confirm it.
///
/// Call it from inside the thread being pinned, before it does anything else —
/// ADR-0015 decision 2. Pinning from the parent needs the child's tid, which
/// needs a handshake, which leaves a window in which the new thread runs
/// unpinned.
///
/// # Errors
///
/// [`NoSuchCore`](AffinityError::NoSuchCore) if the kernel rejects the mask,
/// [`Denied`](AffinityError::Denied) on `EPERM`,
/// [`ReadbackMismatch`](AffinityError::ReadbackMismatch) if the call succeeded
/// and the mask it reads back afterwards is not exactly this core, and
/// [`Failed`](AffinityError::Failed) with the raw `errno` for anything else.
///
/// **A refused call leaves the thread's affinity untouched**, because a failing
/// `sched_setaffinity` changes nothing — proven by
/// `tests/affinity.rs::failing_to_pin_leaves_the_thread_where_it_was`.
pub fn pin_current_thread(core: CoreId) -> Result<(), AffinityError> {
    if core.0 >= MASK_WORDS * 64 {
        // Beyond what the mask can express. Reported as the same thing the
        // kernel would say rather than as a separate "too big" case, because to
        // the caller it is the same mistake.
        return Err(AffinityError::NoSuchCore(core));
    }

    let mut wanted: Mask = [0; MASK_WORDS];
    wanted[core.0 / 64] = 1u64 << (core.0 % 64);

    // SAFETY: `wanted` is a live, 8-aligned `[u64; 16]` owned by this frame, and
    // its size in bytes is passed as `cpusetsize`, so the kernel reads only
    // inside it. `cpu_set_t` is a bitmask of `c_ulong` with the same alignment,
    // which is why the cast is sound rather than merely convenient. pid 0 means
    // *this thread*, which is the whole point of decision 2. The kernel does not
    // retain the pointer.
    //
    // What proves it: `tests/affinity.rs::pinning_reads_back_as_the_core_that_was_asked_for`
    // compares the kernel's own mask, and `a_pinned_thread_stays_on_its_core_while_it_works`
    // watches the scheduler's `processor` field while the thread is busy.
    #[allow(unsafe_code)]
    let rc = unsafe { libc::sched_setaffinity(0, MASK_BYTES, wanted.as_ptr().cast()) };

    if rc != 0 {
        let errno = last_errno();
        return Err(match errno {
            libc::EINVAL => AffinityError::NoSuchCore(core),
            libc::EPERM => AffinityError::Denied(core),
            other => AffinityError::Failed(other),
        });
    }

    // Decision 2's second half. Without this the function reports what it asked
    // for rather than what happened.
    if current_mask()? != [core] {
        return Err(AffinityError::ReadbackMismatch(core));
    }
    Ok(())
}

/// The cores the calling thread is currently allowed to run on, ascending.
///
/// # Errors
///
/// [`Failed`](AffinityError::Failed) with the raw `errno` if `sched_getaffinity`
/// refuses.
pub fn current_mask() -> Result<Vec<CoreId>, AffinityError> {
    let mut got: Mask = [0; MASK_WORDS];

    // SAFETY: same argument as above, in the other direction — `got` is a live,
    // 8-aligned `[u64; 16]` owned by this frame and `cpusetsize` is its size in
    // bytes, so the kernel writes only inside it. pid 0 is this thread.
    //
    // What proves it: the same two tests. A buffer the kernel overran would
    // corrupt the frame rather than return a plausible mask, and a mask it did
    // not fill would not equal the core that was just requested.
    #[allow(unsafe_code)]
    let rc = unsafe { libc::sched_getaffinity(0, MASK_BYTES, got.as_mut_ptr().cast()) };

    if rc != 0 {
        return Err(AffinityError::Failed(last_errno()));
    }

    let mut cores = Vec::new();
    for (word, bits) in got.iter().enumerate() {
        for bit in 0..64 {
            if bits & (1u64 << bit) != 0 {
                cores.push(CoreId(word * 64 + bit));
            }
        }
    }
    Ok(cores)
}

/// Where the scheduler last ran this thread.
///
/// Read from `/proc/thread-self/stat`, which the kernel writes and this crate
/// does not — an observation rather than a restatement of what was asked for.
/// It is the check ADR-0015 open question 1 names, and it is why this module
/// does not offer `sched_getcpu`: that would be one more `unsafe` block for an
/// answer this file can get without any.
///
/// **`scaling_cur_freq` is not an alternative.** `[measured 2026-08-30]` it
/// freezes on a `nohz_full` core, so a check built on it cannot fail.
///
/// # Errors
///
/// [`Unreadable`](AffinityError::Unreadable) if the file is missing or does not
/// have the shape this parses.
pub fn running_on() -> Result<CoreId, AffinityError> {
    const PATH: &str = "/proc/thread-self/stat";
    let text = std::fs::read_to_string(PATH).map_err(|_| AffinityError::Unreadable(PATH))?;

    // Field 2 is the executable name in parentheses and may contain spaces and
    // parentheses of its own, so everything before the LAST ')' is skipped
    // rather than split on. After it, field 3 (state) is the first token, which
    // puts field 39 (processor) at index 36.
    //
    // `[measured 2026-08-31]` verified against `taskset -c 3`, which moves that
    // token to 3 and nothing else.
    let tail = text
        .rsplit_once(')')
        .map(|(_, tail)| tail)
        .ok_or(AffinityError::Unreadable(PATH))?;

    tail.split_whitespace()
        .nth(36)
        .and_then(|f| f.parse::<usize>().ok())
        .map(CoreId)
        .ok_or(AffinityError::Unreadable(PATH))
}

fn last_errno() -> i32 {
    std::io::Error::last_os_error().raw_os_error().unwrap_or(0)
}
