# A blocking `std::sync::mpsc` rendezvous allocates on the waiting thread

> `[measured 2026-09-24]` — found while writing `crates/store-sqlite/benches/alloc.rs` for
> [plans/2026-09-24-p4-sqlite-store.md](../plans/2026-09-24-p4-sqlite-store.md) row 4, before it
> was relied on.

## What happened

The store's `alloc.rs` bench (non-negotiable 1: zero allocations on the engine thread's half of
`put`/`mark_in`/`mark_out`/`mark_active`/`retire`) counts allocations **per thread**
(`docs/reference/...` — see the bench's own module comment): a `thread_local!` flag says which
thread is being watched, so the writer thread's own commit-time allocations (allowed, ADR-0037)
are not mistaken for the engine thread's.

That self-check needs a second, unwatched thread to hand off to the watched one without either
side allocating — otherwise the handoff itself would be measured as if it were the code under
test (the bench's own module comment explains the per-thread design; see
`crates/store-sqlite/benches/alloc.rs`). The first attempt used a
`std::sync::mpsc::sync_channel(1)` rendezvous between the two threads: one side blocks in
`recv()`, the other sends. **`[measured 2026-09-24]` that rendezvous
counted 2 allocations on the waiting thread** — enough to make the self-check's own "an
allocation on this thread counts 1" assertion read noise from the handoff as if it were the
allocation under test, and enough that a real case wrapped the same way would report allocations
that belong to channel machinery, not to the store.

This is not the same fact as
[the earlier `mpsc` measurement](measured-costs.md) (`std::sync::mpsc::try_recv`, non-blocking,
polled from the shard runtime, 2026-08-31): that one measured **zero syscalls** on the polling
side of an *unbounded* `channel()` drained by `try_recv`. This trap is about a **blocking**
`recv()`/`send()` rendezvous on a **bounded** `sync_channel`, which is a different code path
inside `std::sync::mpsc` (a park/unpark handoff, not a lock-free queue drain) and was not
covered by that earlier measurement. The two results do not contradict each other; they measure
different calls.

## The rule now

`benches/alloc.rs`'s cross-thread handoff uses two `static AtomicBool`s and a spin loop
(`GO`/`DONE`, `Ordering::Acquire`/`Release`) instead of a channel:

```rust
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
```

Atomics only, no syscall, no allocation on either side — the same shape `ring::Idle`'s spin
phase and the engine's own `benches/alloc.rs` self-check already rely on. **A blocking channel,
a condvar, or a `Mutex` used only to hand off between two benchmark threads is not free**, and a
self-check built on one can pass or fail for a reason that has nothing to do with the code it
was meant to isolate.

## The tests that guard it

| Guard | What it proves | Reversal |
|---|---|---|
| `crates/store-sqlite/benches/alloc.rs`, the module's own self-check (`other_thread == 0`, `this_thread == 1`) before any of the five cases run | the counter is per-thread and the handoff itself costs nothing on either thread | swap the `AtomicBool` spin back for `mpsc::sync_channel(1)` → `other_thread` reads 2, not 0, and the self-check fails before a single real case runs |

**Not guarded by a script**: nothing greps `benches/alloc.rs` for `mpsc`; the guard is the
self-check's own assertion, which the bench runs every time it runs.
