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
    /// A plan that names no shard core at all.
    ///
    /// Anticipated by [ADR-0019](../../../docs/decisions/ADR-0019-two-unsafe-blocks-and-an-error-the-enum-can-hold.md)
    /// decision 5, which made this enum `#[non_exhaustive]` because steps 3 to 5
    /// would add variants. This is one of them.
    EmptyPlan,
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
            Self::EmptyPlan => write!(f, "the plan names no shard core"),
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

// ---------------------------------------------------------------------------
// What the machine says about itself, and what a plan is allowed to ask for
// ---------------------------------------------------------------------------

/// The three CPU lists and the SMT pairs, as `/sys` reports them.
///
/// Separate from [`ShardPlan`] so that the rules can be tested against a
/// topology this machine does not have. [ADR-0015] says so in as many words:
/// §9 requires SMT off, so on a correctly tuned box every online CPU is its own
/// only sibling and the sibling rule **cannot fire**. It fires on the machine
/// that is set up wrong, which is where the mistake gets made — so the rule is
/// worth having and the only way to exercise it is to synthesise the topology.
///
/// [ADR-0015]: ../../../docs/decisions/ADR-0015-explicit-cores-pinned-from-inside-and-read-back.md
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Topology {
    present: Vec<CoreId>,
    online: Vec<CoreId>,
    isolated: Vec<CoreId>,
    siblings: Vec<(CoreId, Vec<CoreId>)>,
}

const PRESENT: &str = "/sys/devices/system/cpu/present";
const ONLINE: &str = "/sys/devices/system/cpu/online";
const ISOLATED: &str = "/sys/devices/system/cpu/isolated";

impl Topology {
    /// Read this machine.
    ///
    /// Sibling lists are read only for the cores that are **online**: an offline
    /// CPU has no `topology/` directory, and asking for one is how a reader ends
    /// up reporting a missing file as a missing feature.
    ///
    /// # Errors
    ///
    /// [`Unreadable`](AffinityError::Unreadable) naming the file that could not
    /// be read or parsed.
    pub fn read() -> Result<Self, AffinityError> {
        let present = read_cpu_list(PRESENT)?;
        let online = read_cpu_list(ONLINE)?;
        // `isolated` is empty on a machine with no `isolcpus`, and an empty file
        // is an answer rather than a failure.
        let isolated = read_cpu_list(ISOLATED)?;

        let mut siblings = Vec::new();
        for core in &online {
            let path = format!(
                "/sys/devices/system/cpu/cpu{}/topology/thread_siblings_list",
                core.0
            );
            // A kernel without CONFIG_SMP, or a container hiding topology, has
            // no such file. A core with no sibling list is its own only
            // sibling, which is the truth on a machine with SMT off.
            let list = match std::fs::read_to_string(&path) {
                Ok(text) => parse_cpu_list(text.trim()).ok_or(AffinityError::Unreadable(
                    "/sys/devices/system/cpu/cpuN/topology/thread_siblings_list",
                ))?,
                Err(_) => vec![*core],
            };
            siblings.push((*core, list));
        }

        Ok(Self {
            present,
            online,
            isolated,
            siblings,
        })
    }

    /// Build a topology from the text those files contain.
    ///
    /// Public on purpose: it is what makes the refusals testable, and it lets a
    /// caller on a machine whose `/sys` differs supply the answer rather than
    /// have this crate guess. `siblings` is `(cpu, its thread_siblings_list)`;
    /// a core absent from it is its own only sibling.
    ///
    /// # Errors
    ///
    /// [`Unreadable`](AffinityError::Unreadable) naming which of the three
    /// strings did not parse.
    pub fn from_sysfs(
        present: &str,
        online: &str,
        isolated: &str,
        siblings: &[(usize, &str)],
    ) -> Result<Self, AffinityError> {
        Ok(Self {
            present: parse_cpu_list(present).ok_or(AffinityError::Unreadable(PRESENT))?,
            online: parse_cpu_list(online).ok_or(AffinityError::Unreadable(ONLINE))?,
            isolated: parse_cpu_list(isolated).ok_or(AffinityError::Unreadable(ISOLATED))?,
            siblings: siblings
                .iter()
                .map(|(cpu, list)| {
                    parse_cpu_list(list).map(|l| (CoreId(*cpu), l)).ok_or(
                        AffinityError::Unreadable(
                            "/sys/devices/system/cpu/cpuN/topology/thread_siblings_list",
                        ),
                    )
                })
                .collect::<Result<Vec<_>, _>>()?,
        })
    }

    /// Every CPU this machine has, online or not.
    pub fn present(&self) -> &[CoreId] {
        &self.present
    }
    /// Every CPU the kernel will currently schedule on.
    pub fn online(&self) -> &[CoreId] {
        &self.online
    }
    /// Every CPU named by `isolcpus`. **Not intersected with
    /// [`online`](Self::online)** — `[measured 2026-08-31]` on the §9 desktop
    /// this lists `6-7,14-15` while `online` is `0-7`, and reporting the raw
    /// file is what lets a reader see that.
    pub fn isolated(&self) -> &[CoreId] {
        &self.isolated
    }

    /// The cores sharing a physical core with this one.
    ///
    /// Empty when the core has no entry: a core absent from the table is its own
    /// only sibling, and the only question ever asked of this list is whether it
    /// contains some *other* core — which for a lone core is always no.
    fn siblings_of(&self, core: CoreId) -> &[CoreId] {
        self.siblings
            .iter()
            .find(|(c, _)| *c == core)
            .map_or(&[][..], |(_, l)| l.as_slice())
    }

    /// Refuse a plan this machine cannot honour — **before any thread exists**.
    ///
    /// ADR-0015 decision 6: half a runtime that then refuses leaves threads to
    /// join and sockets to close on an error path nobody exercises.
    ///
    /// The order of the checks is the order an operator wants to hear them in:
    /// what does not exist, what cannot run, what is shared, what is not
    /// isolated.
    ///
    /// # Errors
    ///
    /// One of [`AffinityError`]'s topology variants, naming the offending core.
    pub fn validate(&self, plan: &ShardPlan) -> Result<(), AffinityError> {
        if plan.shards.is_empty() {
            return Err(AffinityError::EmptyPlan);
        }

        // Every core the plan names anywhere. Support threads are checked for
        // existence and for contention, and NOT for isolation — see below.
        let named: Vec<CoreId> = plan
            .shards
            .iter()
            .chain(plan.journal_core.iter())
            .chain(plan.consumer_cores.iter())
            .copied()
            .collect();

        for (i, core) in named.iter().enumerate() {
            if named[..i].contains(core) {
                return Err(AffinityError::DuplicateCore(*core));
            }
        }

        for core in &named {
            if !self.present.contains(core) {
                return Err(AffinityError::NoSuchCore(*core));
            }
            if !self.online.contains(core) {
                return Err(AffinityError::NotOnline(*core));
            }
        }

        // A support thread on an SMT sibling of a shard contends with it for one
        // physical core, which is the same harm as two shards sharing one — so
        // this runs over every named core, not only the shards.
        for (i, a) in named.iter().enumerate() {
            for b in &named[..i] {
                if self.siblings_of(*a).contains(b) && a != b {
                    return Err(AffinityError::SmtSiblingOf(*b, *a));
                }
            }
        }

        // Isolation is required of the SHARD cores only. A journal writer or a
        // ring consumer on an isolated core would be taking back exactly the
        // core this design isolates; requiring it would push support threads
        // onto the cores meant to stay clear.
        if !plan.allow_unisolated {
            for core in &plan.shards {
                if !self.isolated.contains(core) {
                    return Err(AffinityError::NotIsolated(*core));
                }
            }
        }

        Ok(())
    }
}

/// Which cores each thread gets, decided by the caller and checked before any
/// thread is created.
///
/// ADR-0015 decisions 1, 6 and 8.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShardPlan {
    shards: Vec<CoreId>,
    journal_core: Option<CoreId>,
    consumer_cores: Vec<CoreId>,
    allow_unisolated: bool,
}

impl ShardPlan {
    /// One engine per core, in this order.
    pub fn new(shards: Vec<CoreId>) -> Self {
        Self {
            shards,
            journal_core: None,
            consumer_cores: Vec::new(),
            allow_unisolated: false,
        }
    }

    /// Where the journal's writer thread runs.
    ///
    /// Leaving it unset is a **choice**, not a harmless default: an unpinned
    /// writer can land on the very core a shard was pinned to.
    #[must_use]
    pub fn with_journal_core(mut self, core: CoreId) -> Self {
        self.journal_core = Some(core);
        self
    }

    /// Where `RingDispatch`'s consumer threads run. Same caveat as the journal.
    #[must_use]
    pub fn with_consumer_cores(mut self, cores: Vec<CoreId>) -> Self {
        self.consumer_cores = cores;
        self
    }

    /// Accept shard cores that are not in `isolcpus`.
    ///
    /// For development machines, which have no `isolcpus`, and for CI. **It
    /// lifts exactly one rule** — a core that is absent or offline is still
    /// refused — and whatever the engine reports about itself says that it was
    /// set, because a bypassed guard that leaves no trace is bypassed
    /// permanently.
    #[must_use]
    pub fn allow_unisolated(mut self) -> Self {
        self.allow_unisolated = true;
        self
    }

    /// The shard cores, in order. Shard `i` gets `shards()[i]`.
    pub fn shards(&self) -> &[CoreId] {
        &self.shards
    }
    /// Where the journal writer runs, if it was given a home.
    pub fn journal_core(&self) -> Option<CoreId> {
        self.journal_core
    }
    /// Where the ring consumers run, if they were given homes.
    pub fn consumer_cores(&self) -> &[CoreId] {
        &self.consumer_cores
    }
    /// Whether the isolation rule was waived.
    pub fn is_unisolated_allowed(&self) -> bool {
        self.allow_unisolated
    }

    /// Check this plan against the machine it is about to run on.
    ///
    /// # Errors
    ///
    /// Anything [`Topology::read`] or [`Topology::validate`] returns.
    pub fn validate(&self) -> Result<(), AffinityError> {
        Topology::read()?.validate(self)
    }
}

/// `"0-3,7"` -> `[0, 1, 2, 3, 7]`. An empty string is an empty list, which is
/// what `/sys/devices/system/cpu/isolated` contains on a machine with no
/// `isolcpus` — an answer, not a failure.
fn parse_cpu_list(text: &str) -> Option<Vec<CoreId>> {
    let text = text.trim();
    if text.is_empty() {
        return Some(Vec::new());
    }
    let mut out = Vec::new();
    for part in text.split(',') {
        match part.split_once('-') {
            Some((lo, hi)) => {
                let lo: usize = lo.trim().parse().ok()?;
                let hi: usize = hi.trim().parse().ok()?;
                if hi < lo {
                    return None;
                }
                out.extend((lo..=hi).map(CoreId));
            }
            None => out.push(CoreId(part.trim().parse().ok()?)),
        }
    }
    Some(out)
}

fn read_cpu_list(path: &'static str) -> Result<Vec<CoreId>, AffinityError> {
    let text = std::fs::read_to_string(path).map_err(|_| AffinityError::Unreadable(path))?;
    parse_cpu_list(text.trim()).ok_or(AffinityError::Unreadable(path))
}
