# An `alloc` followed by zeroing can fold into `alloc_zeroed`

`[2026-09-24]` phase 4 row 5, the `io_uring` transport plan, non-negotiable 1 — U1 in
`crates/engine/src/transport/uring.rs`'s `Region::new`.

The provided-buffer ring's memory must be **resident** — every page backed by a real page
frame — before the first message can arrive, because faulting a fresh page in for the first
time on the hot path is exactly the cost this design's budget has no room for. The obvious way
to write that is `std::alloc::alloc` followed by a loop that stores zero into every page. LLVM
is licensed to recognise that pattern and rewrite it into `alloc_zeroed` — which lowers to
`calloc` or an anonymous `mmap`, either of which can hand back pages the kernel has not yet
backed with real memory, deferring the fault rather than paying it. The two forms are
observably identical in the values they produce, so the optimiser owes the program nothing
about *how* the zeroes got there. A "pre-touch" loop written as plain stores can therefore be
rewritten by the very optimisation pass it exists to defeat, and the page fault it was written
to avoid still happens — on the hot path, invisibly to any test that checks only the buffer's
**contents**, since an untouched `alloc_zeroed` page reads back as zero exactly like a touched
one.

## The fix

Every write in `Region::new`'s pre-touch loop is `NonNull::write_volatile`, not a plain store.
A volatile write is a side effect the compiler must preserve and must not substitute a
differently-implemented zero-fill for, so the loop cannot be folded into `alloc_zeroed` or
elided outright.

## Guard

- `crates/engine/tests/uring.rs::buffers_are_resident_before_the_first_message` — asserts
  residency directly rather than merely that the bytes read back as zero, which is the one
  observation that can tell a touched page apart from an untouched one that happens to read as
  zero.
- Plan step 7, reversal **R10**: removing the pre-touch loop turns this test red.
- The ASan run of `tests/uring.rs` (plan step 7) exercises the same allocation under a
  different allocator; it cannot itself prove residency, but has produced no report against it.
