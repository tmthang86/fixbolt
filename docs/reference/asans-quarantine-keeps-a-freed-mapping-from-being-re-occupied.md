# ASan's quarantine keeps a freed mapping from being re-occupied

`[measured 2026-09-24]` phase 4 row 5, the `io_uring` transport plan, step 7's ASan run of
`crates/engine/tests/uring.rs::unregistered_buffers_are_not_written_after_the_ring_is_dropped`
(U2's proof).

That test's technique needs the buffer ring's memory region — 64 MiB, chosen to be above
glibc's largest `mmap` threshold so it is its own mapping — to be truly unmapped once the
`Uring` is dropped: it re-`mmap`s the exact same address range with `MAP_FIXED_NOREPLACE`,
fills it with a canary byte, lets the old peer keep writing, and reads the canary back
afterwards. Under AddressSanitizer that precondition fails on its own, before the test's
assertion is ever reached: ASan's allocator does not unmap a freed allocation immediately — it
holds the chunk in a **quarantine**, still mapped, precisely so that a real use-after-free has
something detectable to land on. That is exactly the memory this test needs handed back to the
kernel. The failure is not a false report against the transport: the test's own re-occupation
step fails first, `mmap` returning `MAP_FAILED` with `EEXIST` ("File exists"), before the
canary is ever written.

## The fix

Run this test with
`ASAN_OPTIONS=quarantine_size_mb=0:thread_local_quarantine_size_kb=0`, which tells ASan not to
hold freed allocations back. With both quarantines disabled the range is unmapped promptly, the
test's own precondition (`p != libc::MAP_FAILED && p as usize == addr`) passes, and what would
then turn the test red is a real defect — the kernel still writing into memory this process no
longer owns — never the allocator's own bookkeeping.

## Guard

- `crates/engine/tests/uring.rs::unregistered_buffers_are_not_written_after_the_ring_is_dropped`
  — its own rustdoc records the exact environment variable needed, and it is red on its own
  precondition without it, never a false green.
- Plan step 7: the ASan run of `tests/uring.rs` is recorded as run with this option set, or as
  *SKIPPED, NOT PASSED* when nightly is unavailable — never reported green without it.
