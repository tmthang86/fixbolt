//! The exporter's thread takes the CPU mask of the thread that spawns it —
//! the trap `docs/reference/an-exporter-thread-inherits-its-spawners-cpu-affinity.md`
//! records, held here as a test rather than a sentence (senior review F4).
//!
//! **Its own test binary, on purpose.** It finds the exporter by the thread
//! name `fixbolt-metrics` under `/proc/self/task`, and any other exporter in
//! the same process — every test in `tests/exporter.rs` starts one, in
//! parallel — would be found too.
//!
//! The pinning is done on a spawned thread, never the harness's: narrowing the
//! harness thread would narrow whatever runs on it next.
//!
//! # The `unsafe` here
//!
//! Two libc calls, `sched_getaffinity` and `sched_setaffinity` on the calling
//! thread (`pid` 0), with a `cpu_set_t` that lives on this stack and whose size
//! is passed as `size_of::<cpu_set_t>()`. The kernel writes at most that many
//! bytes and reads at most that many; nothing else is touched. What proves the
//! call did what was asked is the kernel's own answer read back from
//! `/proc/thread-self/status`, not the return value — ADR-0015 decision 2.
#![cfg(target_os = "linux")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::time::Duration;

use fixbolt_engine::observe::Handles;
use fixbolt_metrics::Exporter;

/// The last core this thread may run on, and pin this thread to it alone.
/// The last rather than the first, so the inherited mask is visibly narrower
/// than the default one on any machine with more than one core.
fn pin_to_the_last_allowed_core() -> usize {
    let mut set: libc::cpu_set_t = unsafe_zeroed_set();
    let size = std::mem::size_of::<libc::cpu_set_t>();
    // SAFETY: see the module note — `set` is ours, `size` is its size.
    #[allow(unsafe_code)]
    let got = unsafe { libc::sched_getaffinity(0, size, &raw mut set) };
    assert_eq!(got, 0, "reading this thread's own mask");
    let core = (0..libc::CPU_SETSIZE as usize)
        .rev()
        // SAFETY: `CPU_ISSET` reads bit `i` of a set we own, `i` < `CPU_SETSIZE`.
        .find(|&i| {
            #[allow(unsafe_code)]
            unsafe {
                libc::CPU_ISSET(i, &set)
            }
        })
        .expect("a thread is allowed on at least one core");
    let mut only: libc::cpu_set_t = unsafe_zeroed_set();
    // SAFETY: `CPU_SET` writes bit `core` < `CPU_SETSIZE` of a set we own; the
    // call is the one the module note describes.
    #[allow(unsafe_code)]
    let set_ok = unsafe {
        libc::CPU_SET(core, &mut only);
        libc::sched_setaffinity(0, size, &raw const only)
    };
    assert_eq!(set_ok, 0, "pinning this thread to cpu{core}");
    assert_eq!(
        allowed_list(&std::fs::read_to_string("/proc/thread-self/status").expect("procfs")),
        core.to_string(),
        "the kernel's answer, not the return value"
    );
    core
}

fn unsafe_zeroed_set() -> libc::cpu_set_t {
    // SAFETY: `cpu_set_t` is a plain bit array; all-zero is the empty set.
    #[allow(unsafe_code)]
    unsafe {
        std::mem::zeroed()
    }
}

/// `Cpus_allowed_list:` out of a `/proc/.../status` file.
fn allowed_list(status: &str) -> String {
    status
        .lines()
        .find_map(|l| l.strip_prefix("Cpus_allowed_list:"))
        .map(|v| v.trim().to_owned())
        .expect("a Cpus_allowed_list line")
}

#[test]
fn the_exporter_thread_takes_the_cpu_mask_of_the_thread_that_spawns_it() {
    std::thread::spawn(|| {
        let core = pin_to_the_last_allowed_core();
        let handles = Handles::new();
        let exporter = Exporter::builder("127.0.0.1:0".parse().expect("an address"))
            .engine("a", handles.observer())
            .tick(Duration::from_millis(10))
            .spawn()
            .expect("the exporter starts");

        // `std` names a thread from inside it, once it runs, so right after
        // `spawn` returns the name may not be there yet. Looked for, briefly.
        let find = || -> Vec<String> {
            std::fs::read_dir("/proc/self/task")
                .expect("procfs")
                .filter_map(Result::ok)
                .filter(|t| {
                    std::fs::read_to_string(t.path().join("comm"))
                        .is_ok_and(|c| c.trim_end() == "fixbolt-metrics")
                })
                .map(|t| {
                    allowed_list(&std::fs::read_to_string(t.path().join("status")).expect("status"))
                })
                .collect()
        };
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        let mut exporters = find();
        while exporters.is_empty() && std::time::Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
            exporters = find();
        }
        exporter.stop();

        assert_eq!(
            exporters,
            vec![core.to_string()],
            "one `fixbolt-metrics` thread, allowed on exactly the core its spawner was pinned \
             to: an exporter spawned after the engine thread pins itself shares the engine's \
             core, which is why GUIDE.md §8a and `tools/w2w` spawn it first"
        );
    })
    .join()
    .expect("the pinned thread did not panic");
}
