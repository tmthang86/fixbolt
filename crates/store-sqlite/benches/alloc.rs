//! Non-negotiable 1, for the SQLite store: **zero** allocations on the engine
//! thread's half — `put`, `mark_in`, `mark_out`, `mark_active` and `retire`
//! — the calls the engine makes while it serves.
//!
//! # Counted per thread, not per process
//!
//! The store's writer thread commits while the engine thread works, and it
//! allocates (ADR-0037 allows a writer to): a process-wide count would read
//! its allocations as the engine's. So [`Counting`] counts only a thread that
//! has set [`COUNTED`] — the one thread each case runs on — and the bench
//! **proves that first**: an allocation on another thread inside the window
//! counts 0, one on this thread counts 1. SQLite's own C heap (`malloc` inside
//! the library) is invisible to any Rust allocator; it is not reached from the
//! engine thread, because `SqliteJournal` holds no `rusqlite` type — which a
//! reviewer checks by reading `src/journal.rs`, not this bench.
//!
//! # Each case proves its path ran
//!
//! A zero is about nothing unless the path was taken, so every case checks
//! afterwards that what it did reached the database (read with SQLite after
//! the store has closed it), and `sqlite-retire` that `writers_retired()`
//! rose by one (ADR-0181 *Revision 2*: the counter, never `retire`'s return
//! value).
//!
//! # The `unsafe` here
//!
//! As in the engine's `benches/alloc.rs`: every method forwards to `System`
//! unchanged but for a relaxed counter; a benchmark binary, so nothing ships
//! it; proven by reversal (plan row 4, R6).
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
#![allow(unsafe_code)]
// Not a library crate's source: non-negotiable 7 is about `crates/*/src`.
#![allow(clippy::indexing_slicing)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::sync::atomic::{AtomicUsize, Ordering};

static ALLOCS: AtomicUsize = AtomicUsize::new(0);

thread_local! {
    /// Whether this thread's allocations are counted. `const`-initialised and
    /// without a destructor, so reading it from inside the allocator never
    /// allocates and never touches a destroyed slot.
    static COUNTED: Cell<bool> = const { Cell::new(false) };
}

fn counted_here() -> bool {
    COUNTED.try_with(Cell::get).unwrap_or(false)
}

struct Counting;

// SAFETY: every method forwards to `System`, a correct allocator, with the
// same pointer, layout and size it was given; the only addition is a relaxed
// counter increment on a thread that asked to be counted. See the module
// comment.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        if counted_here() {
            ALLOCS.fetch_add(1, Ordering::Relaxed);
        }
        unsafe { System.alloc(l) }
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        unsafe { System.dealloc(p, l) }
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 {
        if counted_here() {
            ALLOCS.fetch_add(1, Ordering::Relaxed);
        }
        unsafe { System.realloc(p, l, n) }
    }
    unsafe fn alloc_zeroed(&self, l: Layout) -> *mut u8 {
        if counted_here() {
            ALLOCS.fetch_add(1, Ordering::Relaxed);
        }
        unsafe { System.alloc_zeroed(l) }
    }
}

#[global_allocator]
static A: Counting = Counting;

/// Allocations `f` made **on this thread**.
fn count<F: FnOnce()>(f: F) -> usize {
    let before = ALLOCS.load(Ordering::Relaxed);
    COUNTED.with(|c| c.set(true));
    f();
    COUNTED.with(|c| c.set(false));
    ALLOCS.load(Ordering::Relaxed) - before
}

#[cfg(not(feature = "sqlite"))]
fn main() {}

#[cfg(feature = "sqlite")]
fn main() {
    use std::path::{Path, PathBuf};
    use std::time::Duration;

    use fixbolt_engine::journal::{wait_for_retired_writers, writers_retired};
    use fixbolt_session::Config;
    use fixbolt_session::journal::Journal;
    use fixbolt_store_sqlite::{SqliteOptions, SqliteStore};

    // ---- the counter counts this thread and only this thread -------------
    // The other thread already exists and is only woken inside the window,
    // by spinning on atomics: spawning a thread, or a blocking channel, would
    // allocate on this thread (`[measured 2026-09-24]` an `mpsc` rendezvous
    // counted 2 here).
    use std::sync::atomic::AtomicBool;
    static GO: AtomicBool = AtomicBool::new(false);
    static DONE: AtomicBool = AtomicBool::new(false);
    let helper = std::thread::spawn(|| {
        while !GO.load(Ordering::Acquire) {
            std::hint::spin_loop();
        }
        std::hint::black_box(Vec::<u8>::with_capacity(64));
        DONE.store(true, Ordering::Release);
    });
    let other_thread = count(|| {
        GO.store(true, Ordering::Release);
        while !DONE.load(Ordering::Acquire) {
            std::hint::spin_loop();
        }
    });
    helper.join().expect("helper");
    let this_thread = count(|| {
        std::hint::black_box(Vec::<u8>::with_capacity(64));
    });
    assert_eq!(
        other_thread, 0,
        "an allocation on another thread was counted: the count is not per thread"
    );
    assert_eq!(
        this_thread, 1,
        "an allocation on this thread was not counted: the counter is blind"
    );

    fn cfg() -> Config {
        Config::acceptor(b"FIX.4.4", b"ISLD", b"TW44")
    }
    fn fresh(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "fixbolt-store-sqlite-alloc-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("scratch dir");
        dir.join("s.db")
    }
    fn open(path: &Path) -> SqliteStore {
        SqliteStore::open(path, &cfg(), SqliteOptions::default()).expect("open")
    }
    /// The session row and the message count, read after the store closed.
    fn read_back(path: &Path) -> (Option<u32>, Option<u32>, Option<i64>, u32) {
        let db = rusqlite::Connection::open(path).expect("open with SQLite");
        let row = db
            .query_row(
                "SELECT highest_in, highest_out, last_active_ms FROM session",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .expect("the session row");
        let rows: u32 = db
            .query_row("SELECT count(*) FROM messages", [], |r| r.get(0))
            .expect("count");
        (row.0, row.1, row.2, rows)
    }
    let msg = |seq: u32| {
        format!("8=FIX.4.4\x019=5\x0135=8\x0134={seq}\x0117=E{seq}\x0110=000\x01").into_bytes()
    };

    // ---- sqlite-put ------------------------------------------------------
    let path = fresh("put");
    let mut j = open(&path);
    let (warm, measured) = (msg(1), msg(2));
    assert!(j.put(1, &warm), "warm-up put");
    let put = count(|| {
        assert!(std::hint::black_box(&mut j).put(2, &measured));
    });
    j.close();
    drop(j);
    let (_, out, _, rows) = read_back(&path);
    assert_eq!(
        rows, 2,
        "sqlite-put: the measured message reached the database"
    );
    assert_eq!(out, Some(2));

    // ---- sqlite-mark-in --------------------------------------------------
    let path = fresh("mark-in");
    let mut j = open(&path);
    j.mark_in(1);
    let mark_in = count(|| std::hint::black_box(&mut j).mark_in(7));
    j.close();
    drop(j);
    let (inb, _, _, _) = read_back(&path);
    assert_eq!(
        inb,
        Some(7),
        "sqlite-mark-in: the mark reached the database"
    );

    // ---- sqlite-mark-out -------------------------------------------------
    let path = fresh("mark-out");
    let mut j = open(&path);
    j.mark_out(1);
    let mark_out = count(|| std::hint::black_box(&mut j).mark_out(9));
    j.close();
    drop(j);
    let (_, out, _, _) = read_back(&path);
    assert_eq!(
        out,
        Some(9),
        "sqlite-mark-out: the mark reached the database"
    );

    // ---- sqlite-mark-active ----------------------------------------------
    let path = fresh("mark-active");
    let mut j = open(&path);
    j.mark_active(1);
    let at: u64 = 1_788_431_527_120;
    let mark_active = count(|| std::hint::black_box(&mut j).mark_active(at));
    j.close();
    drop(j);
    let (_, _, active, _) = read_back(&path);
    assert_eq!(
        active,
        Some(1_788_431_527_120),
        "sqlite-mark-active: the mark reached the database"
    );

    // ---- sqlite-retire ---------------------------------------------------
    let path = fresh("retire");
    let mut j = open(&path);
    assert!(j.put(1, &warm));
    let ever = writers_retired();
    let retire = count(|| std::hint::black_box(&mut j).retire());
    assert_eq!(
        writers_retired(),
        ever + 1,
        "sqlite-retire: retire() did not retire the writer, so its zero is about nothing"
    );
    drop(j);
    assert!(
        wait_for_retired_writers(Duration::from_secs(5)),
        "sqlite-retire: the retired writer finished"
    );
    let (_, _, _, rows) = read_back(&path);
    assert_eq!(rows, 1, "sqlite-retire: the retired writer committed first");

    println!(
        "allocations: sqlite-put {put} sqlite-mark-in {mark_in} sqlite-mark-out {mark_out} \
         sqlite-mark-active {mark_active} sqlite-retire {retire}"
    );
    for (case, n) in [
        ("sqlite-put", put),
        ("sqlite-mark-in", mark_in),
        ("sqlite-mark-out", mark_out),
        ("sqlite-mark-active", mark_active),
        ("sqlite-retire", retire),
    ] {
        assert_eq!(n, 0, "{case} allocated {n} times on the engine thread");
    }
}
