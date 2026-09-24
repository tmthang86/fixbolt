# An exporter thread inherits its spawner's CPU affinity

`[2026-09-24]` found while building `tools/w2w --metrics`, phase 4 row 1 step 5 of
[the metrics exporter plan](../plans/2026-09-24-p4-metrics-exporter.md)
([ADR-0170](../decisions/ADR-0170-the-metrics-exporter-holds-an-observer-and-nothing-else-allocates-nothing-per-scrape-and-publishes-only-after-its-kill-line.md)
decision 5).

**`Exporter::builder(...).spawn()` never pins its own thread.** `std::thread::Builder::spawn`
only names it (`fixbolt-metrics`, so `ps -L` can find it); the new thread starts with whatever
CPU affinity mask the calling thread had at that instant, because a new thread inherits its
creator's mask unless something changes it. On a deployment that isolates cores for the engine
(`isolcpus`, `ADR-0015`'s `ShardPlan`/`CorePin`), spawning the exporter **after** the calling
thread has pinned itself to an isolated core puts the exporter — a thread that sleeps, wakes,
does I/O, and is not supposed to be latency-critical — on that same isolated core, right next
to the thread the isolation exists to protect.

## Why this is not a "restricted machine only" problem

On a machine with no `isolcpus` at all, there is no isolation to violate, but the exporter can
still land on whatever core the general scheduler happens to be running the engine thread on,
and the two then contend for that core under load — quieter than sharing an isolated core, but
not nothing.

## What is built to avoid it

- [`crates/metrics/src/lib.rs`](../../crates/metrics/src/lib.rs)'s crate rustdoc and
  `Builder::spawn`'s own doc comment say, in the words the code carries:

  > The thread inherits the CPU affinity of the thread that calls this: **call it before the
  > engine thread pins itself**, or the exporter shares the engine's core.

- `tools/w2w --metrics <addr>` spawns the exporter on the **main** thread, before
  `--engine-core` / `--client-core` pin anything (`tools/w2w/src/main.rs`, `--metrics` module
  doc: "Spawned on the main thread before any thread is pinned").
- [GUIDE.md §8a](../GUIDE.md), "Exporting this to Prometheus," repeats the same ordering
  requirement at the point a caller is most likely to read it.

## What guards it, and what does not

Two guards since senior review F4 (ii):

- **`crates/metrics/tests/inherits_affinity.rs`**
  (`the_exporter_thread_takes_the_cpu_mask_of_the_thread_that_spawns_it`, Linux): pins a thread
  to one core, spawns an exporter from it, finds the `fixbolt-metrics` thread under
  `/proc/self/task` and asserts its `Cpus_allowed_list` is exactly that core. It holds the
  inheritance itself — the premise every sentence on this page rests on. Its own test binary,
  because every test in `tests/exporter.rs` runs an exporter of the same name in parallel.
- **`tools/w2w`, after the pins** (`exporter_placement`, `placement_verdict`): with `--metrics`
  and `--engine-core`/`--client-core`, it reads the exporter thread's mask back and refuses the
  run if the exporter may run on a measured core that `isolcpus` isolates, or may run *only* on
  measured cores. **Not** "the mask contains the engine core": with no `isolcpus` the inherited
  mask is every core, and that rule would refuse every correctly ordered `--allow-unisolated`
  run. The reversal — `pin_client` moved before the exporter's spawn — was refused with "may run
  only on measured cores [3] — it inherited the mask of a thread that was already pinned",
  exit 1 (`[2026-09-24]`, `tmt-B450-I-AORUS-PRO-WIFI`, desktop grub line, cpu2/cpu3
  `--allow-unisolated`). The correct order printed `metrics-thread: cpus [0, …, 15]` and ran.

Still unguarded: an application that spawns its own `Exporter` after pinning its engine thread.
Nothing in this repository runs that application; `GUIDE.md` §8a is its protection. **Not
proven on the §9 line**: that under `isolcpus` the main thread's default mask — and so the
exporter's — excludes the isolated cores; the desk was on its desktop grub line when this was
written.

## What an operator does about it

1. **Spawn the `Exporter` first, before pinning any thread** — including the one that is about
   to call `serve`/`serve_hft`/`serve_sharded_hft` or pin itself with
   `affinity::pin_current_thread`. The crate's own example in `lib.rs` and `GUIDE.md §8a` show
   the order.
2. **If the exporter must be created later** (for example, added to an already-running
   deployment), pin it yourself afterward, the same way you would pin any other thread —
   `affinity::pin_current_thread` from inside a wrapper around `.spawn()`'s caller, or `taskset`
   from outside the process — to a core **outside** the isolated set.
3. **Check placement rather than assume it**, on any deployment where the exporter's
   correctness does not matter but the engine's latency does:
   `ps -L -o tid,psr,comm -p <pid> | grep fixbolt-metrics` and compare its `psr` against the
   engine thread's and against `isolcpus`.
4. **On a shard deployment** (`serve_sharded_hft`), one `Exporter` can watch every shard
   through repeated `.engine(name, observer)` calls — spawn that one exporter from the
   orchestrating thread before any shard thread pins, never from inside a shard thread.

## Sources

- [`crates/metrics/src/lib.rs`](../../crates/metrics/src/lib.rs) — `Builder::spawn`.
- [`tools/w2w/src/main.rs`](../../tools/w2w/src/main.rs) — the `--metrics` module doc and its
  spawn-before-pin ordering.
- [ADR-0015](../decisions/ADR-0015-explicit-cores-pinned-from-inside-and-read-back.md) — why
  this project treats core placement as verified, never assumed, for every other thread.
