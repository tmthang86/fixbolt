//! How long it takes one thread to wake another that is sleeping in
//! `epoll_wait` or `poll`, wake-to-return, one direction.
//!
//! Plan step 4.2 of `plans/2026-09-18-closing-the-open-items.md`, closing PRD
//! §6 row 7 (item "engine tự cấu hình"). [ADR-0025]'s *Context* rests
//! `hft`'s session ceiling of four on an "epoll-class wakeup" of **2-5 µs from
//! the literature, never measured here** (ADR-0014 open question 1). This
//! bench is the instrument; the number itself is taken at boot C on the
//! `DESIGN.md` §9 desk and recorded in `benches/baselines.tsv` there, per
//! `docs/plans/2026-09-18-closing-the-open-items.md` row **C-PRD7**. Until
//! then every row below prints `NO BASELINE` and nothing here moves the 4 in
//! [ADR-0025].
//!
//! [ADR-0025]: ../../../docs/decisions/ADR-0025-hft-has-a-hard-session-ceiling-and-the-engine-advises-rather-than-applies.md
//!
//! # Shape
//!
//! Two threads, one socket pair. Thread **A** writes one byte and, right
//! before the write, stamps an `Instant` shared with thread **B**. Thread
//! **B** sleeps in the syscall under test (`epoll_wait` for the `wakeup
//! epoll` arm, `poll` for `wakeup poll`), and the instant it returns it stamps
//! its own `Instant` from the **same clock** and drains the byte. The
//! difference is the wake-to-return latency, one direction — never the round
//! trip, which is a different number `benches/turn.rs` and `tools/w2w`
//! already answer.
//!
//! # Rendezvous, and why it needs one
//!
//! If A wrote before B called the wait syscall, a level-triggered
//! `epoll_wait`/`poll` returns immediately because the fd is already
//! readable — a correct answer to a different question, and one that reads
//! artificially fast. So B raises an `AtomicBool` immediately before calling
//! the wait syscall; A spins on it, then waits a fixed margin
//! (`RENDEZVOUS_MARGIN`) before taking its own `Instant` and writing, biasing
//! toward "B is already asleep in the kernel" rather than "B has set a flag in
//! userspace and not yet reached the syscall". The margin sits **before** A's
//! `Instant`, so it does not enter the measured interval; it only makes the
//! interval mean what this file says it means.
//!
//! # Iteration count
//!
//! 20 000 per arm, as the plan asks, so `--test` from `cargo bench -- --test`
//! (`scripts/bench.sh`'s smoke pass) does not wait on 40 000 blocking
//! round-trips: [`iterations`] returns 200 under `--test`, enough to exercise
//! both arms and both threads without the CI smoke pass paying for the real
//! sweep.
//!
//! # Core pinning
//!
//! `WAKEUP_CORES=<a>,<b>` pins thread A to cpu `a` and thread B to cpu `b`,
//! each from inside itself (ADR-0015 decision 2) and read back with
//! `sched_getaffinity` before anything is timed — a pin that failed to take
//! and went unnoticed would print a wakeup number for the wrong pair of
//! cores. Unset, the bench runs unpinned and says so.
//!
//! # Why not `harness.rs`
//!
//! `codec/benches/harness.rs` (`benches/turn.rs`'s harness) times a closure it
//! calls itself, best-of-7, and compares one figure to a per-machine baseline.
//! This bench needs the opposite shape: 20 000 *individual* cross-thread
//! samples, because the wake latency is exactly the thing that closure-timing
//! cannot see (the closure runs on one thread; the wait it would time spans
//! two). So the percentile block below is written directly, in the row format
//! `tools/w2w/src/main.rs`'s `print_figures` already uses (`min`, `p50`,
//! `p99`, `p99.9`), and each arm is printed as its own `NO BASELINE` case
//! rather than run through `Suite::bench` — there is no `baselines.tsv` entry
//! for it to compare against until boot C.
//!
//! # `unsafe`
//!
//! Every block here is a single `libc` call operating on a stack buffer this
//! function owns, sized to exactly what the call is told is available. What
//! proves each one sound is named on the block itself.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
#![allow(clippy::indexing_slicing)]

#[cfg(target_os = "linux")]
fn main() {
    linux::run();
}

/// `epoll_wait` does not exist off Linux, and this bench is entirely about it
/// and its `poll` sibling on this kernel — there is nothing to measure
/// elsewhere, so the stub says so rather than failing to build.
#[cfg(not(target_os = "linux"))]
fn main() {
    println!("wakeup: epoll_wait/poll wake latency is Linux-only; nothing to measure here.");
}

#[cfg(target_os = "linux")]
mod linux {
    use std::os::unix::io::RawFd;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use std::thread;
    use std::time::{Duration, Instant};

    /// The full sweep the plan asks for.
    const FULL_ITERATIONS: usize = 20_000;

    /// `cargo bench --bench wakeup -- --test` runs this many instead. 20 000
    /// blocking cross-thread round trips per arm is the right number for the
    /// real sweep at boot C; it is not the right number for a smoke pass that
    /// must finish in CI. 200 still runs the rendezvous, both threads and both
    /// arms at least once each, which is everything `--test` is for.
    const TEST_ITERATIONS: usize = 200;

    /// Fixed pause between A seeing B's "about to wait" flag and A taking its
    /// own timestamp and writing — see the module doc's "Rendezvous" section.
    /// Outside the measured interval by construction: it happens strictly
    /// before A's `Instant::now()`.
    const RENDEZVOUS_MARGIN: Duration = Duration::from_micros(50);

    fn iterations() -> usize {
        if std::env::args().any(|a| a == "--test") {
            TEST_ITERATIONS
        } else {
            FULL_ITERATIONS
        }
    }

    /// Stamps the run with the engine's own clock reading, for cross
    /// reference against other bench output — and, not incidentally, the
    /// reason this bench links real `fixbolt_engine` code rather than only
    /// `libc`: `scripts/check-bench-alignment.sh` reads the alignment flag
    /// back off this workspace's own symbols in the binary, and a bench that
    /// never calls into `fixbolt_engine` would have none for it to find.
    fn print_engine_clock() {
        let mut clock = fixbolt_engine::clock::SystemClock;
        let ms = fixbolt_engine::clock::Clock::now_ms(&mut clock);
        println!("clock     fixbolt_engine::clock::SystemClock now_ms {ms}");
    }

    pub fn run() {
        print_engine_clock();
        let n = iterations();
        let cores = parse_cores();
        match cores {
            Some((a, b)) => println!("pinned A cpu{a}, B cpu{b}"),
            None => println!("unpinned"),
        }
        println!("wakeup: n = {n} per arm");

        let epoll = measure(n, Arm::Epoll, cores);
        print_arm("wakeup epoll", &epoll);

        let poll = measure(n, Arm::Poll, cores);
        print_arm("wakeup poll", &poll);
    }

    /// `WAKEUP_CORES=<a>,<b>`, or `None` when unset. Exits the process with a
    /// message on anything that parses but is not exactly two numbers — a
    /// bench that silently ignored a malformed request would report a number
    /// for whatever cores the scheduler happened to choose instead.
    fn parse_cores() -> Option<(usize, usize)> {
        let raw = std::env::var("WAKEUP_CORES").ok()?;
        let mut parts = raw.split(',').map(str::trim);
        let a = parts.next();
        let b = parts.next();
        let rest = parts.next();
        let (Some(a), Some(b), None) = (a, b, rest) else {
            eprintln!("wakeup: WAKEUP_CORES must be \"<a>,<b>\", got {raw:?}");
            std::process::exit(1);
        };
        let parse_one = |s: &str| {
            s.parse::<usize>().unwrap_or_else(|_| {
                eprintln!("wakeup: WAKEUP_CORES: {s:?} is not a cpu number");
                std::process::exit(1);
            })
        };
        Some((parse_one(a), parse_one(b)))
    }

    /// Pin the calling thread to `cpu` and read the mask back before
    /// returning — ADR-0015's own rule (pin from inside, as the thread's
    /// first act; a call returning success is not evidence), reimplemented
    /// directly against `libc` here rather than through
    /// `fixbolt_engine::affinity`: that module sits behind the opt-in
    /// `affinity` feature, and this bench pins with the `libc` dependency the
    /// crate already carries unconditionally, on every feature combination.
    ///
    /// Exits the process, with a message, on anything that does not read back
    /// as exactly `cpu` — reporting a wakeup number measured on the wrong
    /// core would be worse than refusing to run.
    fn pin_self(cpu: usize) {
        const WORDS: usize = 16;
        const BYTES: usize = WORDS * 8;

        if cpu >= WORDS * 64 {
            eprintln!("wakeup: cpu{cpu} is out of range for this bench's mask");
            std::process::exit(1);
        }
        let mut mask = [0u64; WORDS];
        mask[cpu / 64] = 1u64 << (cpu % 64);

        // SAFETY: `mask` is a live, 8-byte-aligned `[u64; 16]` owned by this
        // frame, and `BYTES` is exactly its size in bytes, so the kernel
        // writes only inside it. pid 0 means "this thread" (ADR-0015 decision
        // 2). What proves it: the readback three lines down, which is the
        // reason this function exists rather than trusting the return code.
        #[allow(unsafe_code)]
        let rc = unsafe { libc::sched_setaffinity(0, BYTES, mask.as_ptr().cast()) };
        if rc != 0 {
            eprintln!(
                "wakeup: sched_setaffinity(cpu{cpu}) failed: {}",
                std::io::Error::last_os_error()
            );
            std::process::exit(1);
        }

        let mut got = [0u64; WORDS];
        // SAFETY: same argument as above, other direction: `got` is a live,
        // 8-byte-aligned `[u64; 16]` owned by this frame and `BYTES` is its
        // size, so the kernel writes only inside it.
        #[allow(unsafe_code)]
        let rc = unsafe { libc::sched_getaffinity(0, BYTES, got.as_mut_ptr().cast()) };
        if rc != 0 || got != mask {
            eprintln!(
                "wakeup: pinning to cpu{cpu} did not read back (rc={rc}); refusing to report \
                 a wakeup number measured on the wrong core"
            );
            std::process::exit(1);
        }
    }

    /// Which syscall thread B sleeps in.
    #[derive(Clone, Copy)]
    enum Arm {
        Epoll,
        Poll,
    }

    /// A connected `AF_UNIX`/`SOCK_STREAM` pair, closed by [`close_pair`].
    struct Pair {
        rd: RawFd,
        wr: RawFd,
    }

    fn open_pair() -> Pair {
        let mut fds = [0 as RawFd; 2];
        // SAFETY: `fds` is a live, 2-element `[i32; 2]` this frame owns;
        // `socketpair` either fills both elements and returns 0, or returns
        // -1 and leaves them untouched — checked immediately below.
        #[allow(unsafe_code)]
        let rc = unsafe {
            libc::socketpair(libc::AF_UNIX, libc::SOCK_STREAM, 0, fds.as_mut_ptr())
        };
        if rc != 0 {
            eprintln!(
                "wakeup: socketpair failed: {}",
                std::io::Error::last_os_error()
            );
            std::process::exit(1);
        }
        Pair {
            rd: fds[0],
            wr: fds[1],
        }
    }

    fn close_pair(pair: &Pair) {
        // SAFETY: both fds were returned by `socketpair` above and are closed
        // exactly once, here, after both threads have joined.
        #[allow(unsafe_code)]
        unsafe {
            libc::close(pair.rd);
            libc::close(pair.wr);
        }
    }

    /// Block until `rd` is readable, the way `arm` says to.
    fn wait_readable(arm: Arm, rd: RawFd, epfd: RawFd) {
        match arm {
            Arm::Epoll => {
                // SAFETY: `libc::epoll_event` is a plain-old-data struct (two
                // integer fields); the all-zero bit pattern is a valid value
                // of it, and the buffer is immediately handed to
                // `epoll_wait` below, which only ever writes into it.
                #[allow(unsafe_code)]
                let mut events: [libc::epoll_event; 1] = unsafe { std::mem::zeroed() };
                // SAFETY: `epfd` is a live epoll instance owned by this
                // thread, `events` is a live 1-element buffer and `1` is
                // exactly its length, so the kernel writes at most one event
                // into it. A timeout of -1 blocks, which is the case under
                // measurement; the return value is not otherwise inspected
                // because there is exactly one fd registered.
                #[allow(unsafe_code)]
                let rc = unsafe { libc::epoll_wait(epfd, events.as_mut_ptr(), 1, -1) };
                if rc < 0 {
                    eprintln!(
                        "wakeup: epoll_wait failed: {}",
                        std::io::Error::last_os_error()
                    );
                    std::process::exit(1);
                }
            }
            Arm::Poll => {
                let mut fds = [libc::pollfd {
                    fd: rd,
                    events: libc::POLLIN,
                    revents: 0,
                }];
                // SAFETY: `fds` is a live 1-element buffer this frame owns and
                // `1` is exactly its length, so the kernel writes only inside
                // it. A timeout of -1 blocks, which is the case under
                // measurement.
                #[allow(unsafe_code)]
                let rc = unsafe { libc::poll(fds.as_mut_ptr(), 1, -1) };
                if rc < 0 {
                    eprintln!("wakeup: poll failed: {}", std::io::Error::last_os_error());
                    std::process::exit(1);
                }
            }
        }
    }

    fn measure(n: usize, arm: Arm, cores: Option<(usize, usize)>) -> Vec<u64> {
        let pair = open_pair();
        let epoch = Instant::now();
        let b_waiting = Arc::new(AtomicBool::new(false));
        let t0_nanos = Arc::new(AtomicU64::new(0));

        let rd = pair.rd;
        let wr = pair.wr;
        let b_waiting_for_b = Arc::clone(&b_waiting);
        let t0_for_b = Arc::clone(&t0_nanos);
        let core_b = cores.map(|(_, b)| b);

        let b_handle = thread::spawn(move || -> Vec<u64> {
            if let Some(cpu) = core_b {
                pin_self(cpu);
            }

            let epfd = match arm {
                Arm::Epoll => {
                    // SAFETY: no arguments to misuse; the fd this returns is
                    // owned by this thread and closed at the end of the
                    // function, on every arm (the `Arm::Poll` arm never opens
                    // one and never closes one).
                    #[allow(unsafe_code)]
                    let epfd = unsafe { libc::epoll_create1(0) };
                    if epfd < 0 {
                        eprintln!(
                            "wakeup: epoll_create1 failed: {}",
                            std::io::Error::last_os_error()
                        );
                        std::process::exit(1);
                    }
                    let mut ev = libc::epoll_event {
                        events: libc::EPOLLIN as u32,
                        u64: 0,
                    };
                    // SAFETY: `epfd` was just created above and is still
                    // live; `ev` is a live local this call reads from, never
                    // retained past the call.
                    #[allow(unsafe_code)]
                    let rc = unsafe { libc::epoll_ctl(epfd, libc::EPOLL_CTL_ADD, rd, &mut ev) };
                    if rc != 0 {
                        eprintln!(
                            "wakeup: epoll_ctl(ADD) failed: {}",
                            std::io::Error::last_os_error()
                        );
                        std::process::exit(1);
                    }
                    epfd
                }
                Arm::Poll => -1,
            };

            let mut samples = Vec::with_capacity(n);
            for _ in 0..n {
                b_waiting_for_b.store(true, Ordering::Release);
                wait_readable(arm, rd, epfd);
                let t1 = epoch.elapsed().as_nanos() as u64;
                let t0 = t0_for_b.load(Ordering::Acquire);
                samples.push(t1.saturating_sub(t0));

                let mut byte = [0u8; 1];
                // SAFETY: `byte` is a live 1-byte buffer this frame owns and
                // `1` is exactly its length; the wait above proved `rd` is
                // readable, so this drains the byte A just sent.
                #[allow(unsafe_code)]
                let rc = unsafe { libc::read(rd, byte.as_mut_ptr().cast(), 1) };
                if rc != 1 {
                    eprintln!(
                        "wakeup: expected to read exactly 1 byte, got {rc}: {}",
                        std::io::Error::last_os_error()
                    );
                    std::process::exit(1);
                }
            }

            if let Arm::Epoll = arm {
                // SAFETY: `epfd` was created by this same thread above and is
                // not used again after this point.
                #[allow(unsafe_code)]
                unsafe {
                    libc::close(epfd);
                }
            }
            samples
        });

        if let Some((a, _)) = cores {
            pin_self(a);
        }
        for _ in 0..n {
            while !b_waiting.load(Ordering::Acquire) {
                std::hint::spin_loop();
            }
            b_waiting.store(false, Ordering::Relaxed);
            thread::sleep(RENDEZVOUS_MARGIN);

            let t0 = epoch.elapsed().as_nanos() as u64;
            t0_nanos.store(t0, Ordering::Release);

            let byte = [1u8; 1];
            // SAFETY: `byte` is a live 1-byte buffer this frame owns and `1`
            // is exactly its length; `wr` is the write end of the pair opened
            // above and is not closed until after this thread and B have both
            // finished.
            #[allow(unsafe_code)]
            let rc = unsafe { libc::write(wr, byte.as_ptr().cast(), 1) };
            if rc != 1 {
                eprintln!(
                    "wakeup: expected to write exactly 1 byte, got {rc}: {}",
                    std::io::Error::last_os_error()
                );
                std::process::exit(1);
            }
        }

        let samples = b_handle.join().unwrap_or_else(|_| {
            eprintln!("wakeup: the waiter thread panicked");
            std::process::exit(1);
        });
        close_pair(&pair);
        samples
    }

    /// `min`/`p50`/`p99`/`p99.9`/`max`, in the row format
    /// `tools/w2w/src/main.rs`'s `print_figures` uses, prefixed `NO BASELINE`:
    /// there is no `benches/baselines.tsv` entry for this case until boot C
    /// (plan row C-PRD7) records one.
    fn print_arm(name: &str, samples: &[u64]) {
        let mut s = samples.to_vec();
        s.sort_unstable();
        let pick = |q: f64| s[((s.len() as f64 - 1.0) * q) as usize];
        println!("{name}   NO BASELINE   n={}", s.len());
        println!("     min    {:>9} ns", s[0]);
        println!("     p50    {:>9} ns", pick(0.50));
        println!("     p99    {:>9} ns", pick(0.99));
        println!("     p99.9  {:>9} ns", pick(0.999));
        println!("     max    {:>9} ns", s[s.len() - 1]);
    }
}
