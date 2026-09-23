# Tracing a Rust binary's allocations: `ltrace` sees nothing, and `perf record` records the wrapper too

`[measured 2026-09-23]` on the `DESIGN.md` §9 desktop (desktop grub line), step P4 of
[plans/2026-09-23-closing-phase-2.md](../plans/2026-09-23-closing-phase-2.md) (`STATUS.md` item
99's residue, [ADR-0102](../decisions/ADR-0102-a-line-that-moves-while-its-instruction-count-does-not-is-a-layout-move-and-the-count-is-read-off-the-desk.md)
decision 5). perf 7.0.14, kernel 7.0.0-31, glibc from Ubuntu's `libc.so.6`, the `journal` bench
binary built by rustc 1.98. Evidence: `target/p2close-evidence/p4/`, `p4-perf/`, `p4-findings.txt`
in the plan's worktree (gitignored, desk only). The step needed an allocation trace with ASLR off;
it cost two instruments and one misreading before it produced one.

## Trap 1 — `ltrace` records none of the program's own `malloc` calls

`setarch x86_64 -R ltrace -e 'malloc+calloc+realloc+free+posix_memalign+aligned_alloc' -o … <bin>`
printed the **same three lines** at every padding `k`, on both trees: three `calloc` calls made
*from inside libc* during start-up, then `+++ exited +++`. None of the harness's allocations
appear — not the two 1 MiB read buffers, not the 2 MiB `Store` blocks — and `ltrace` exits 0.

**Why:** `ltrace` hooks calls through the PLT. The bench binary is linked with full RELRO and
immediate binding — `readelf -d` shows `FLAGS BIND_NOW` and `FLAGS_1 NOW PIE`, which rustc emits
by default on Linux — so every call to `malloc` goes through a GOT slot resolved at load time and
never passes a lazy PLT stub for `ltrace` to intercept. `ltrace`'s own list reported this in 2016
("newish bind_now binaries do not use plt relocs",
<https://alioth-lists-archive.debian.net/pipermail/ltrace-devel/2016-May/001378.html>); current
write-ups say the same of Ubuntu's full-RELRO defaults and point to `gdb`, `uftrace` or `perf`
(<https://linuxvox.com/blog/alternative-to-ltrace-that-works-on-binaries-linked-with-z-now/>).

**The rule:** an `ltrace` of a Rust binary that shows only libc-internal calls saw nothing. Use
`perf` uprobes on the libc symbols instead (below). Do not read a near-empty `ltrace` as "the
binary allocates little".

## Trap 2 — `perf record -- setarch -R …` holds three processes, and only one of them has ASLR off

The instrument that worked:

```sh
sudo -n perf probe -x /usr/lib/x86_64-linux-gnu/libc.so.6 'malloc size=%di'   # and __return, calloc, realloc
sudo -n perf record -e 'probe_libc:*' -o cur-<k>.data -- setarch x86_64 -R taskset -c 6 <bin>
sudo -n perf script -i cur-<k>.data
sudo -n perf probe -d 'probe_libc:*'                                           # afterwards; `perf probe -l` empty
```

Read as one flat list, the returned addresses differed on **every** invocation, even between two
values of `k` already known to be equivalent, and the first reading was "`perf record` defeats
`setarch -R`". **That reading was wrong.** The `comm` column shows three processes in each file:
`setarch` (120 events), `taskset` (134) and the bench (56). `perf` `exec`s `setarch` normally, so
`setarch`'s own allocations happen **before** it sets `ADDR_NO_RANDOMIZE`, and its heap is
randomised (`0x5c84b835c010` at one `k`, `0x5b6908acb010` at the next). `taskset` and the bench are
`exec`'d after the personality is set, and their addresses are the ASLR-off ones
(`0x55555555c010`, `0x5555555be010`), identical from run to run. Filtered to the bench's own
`comm`, the sequence of sizes *and* returned addresses was byte-identical at seven of eight `k`.
The eighth carried nine extra allocations from a reporting path, and every allocation it shared
with the others had the same address ([measured-costs](measured-costs.md) *Desk-free,
2026-09-23 … Item 99's residue*).

**The rule:** split a `perf script` of a wrapped command by `comm` (or pid) before comparing
anything, and check the wrapper's own events are not the ones that differ. The control that
exposed the difference — `sudo -n strace -e trace=brk setarch x86_64 -R <bin>` twice, identical
final `brk` — was right. What was missing was reading which process the differing events belonged
to.

## What guards it

**No automated guard.** Both traps need root `perf` or `ltrace` on the desk and a scratch build; a
CI runner has neither a PMU nor uprobes to rely on. The guards are this page, the P4 row's
evidence files, and the plan's *Bẫy đã lường trước* table, which names both.

## Sources

- `target/p2close-evidence/p4-findings.txt` (desk only): the `ltrace` result, the `strace`
  cross-check, the uprobe recording and its first reading.
- `target/p2close-evidence/p4-perf/cur-<k>.script.txt` and `rev-{0,1024}.script.txt` (desk only):
  the `comm` column, per-process sequences.
- `readelf -d target/release/deps/journal-0d5a61aaf61425bb` in the plan's worktree:
  `FLAGS BIND_NOW`, `FLAGS_1 NOW PIE`.
- The two web sources linked above.
