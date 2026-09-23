# perf's dwarf unwinder fails on an LLD-linked PIE, and the report still looks like a report

`[measured 2026-09-23]` on the `DESIGN.md` §9 desktop, boot F step F7 of
[plans/2026-09-23-boot-f-closes-the-open-items-and-powers-off.md](../plans/2026-09-23-boot-f-closes-the-open-items-and-powers-off.md)
(`STATUS.md` item 96). perf 7.0.14, kernel 7.0.0-31, rustc 1.98 (links with LLD 22.1.8).
Evidence: `target/boot-f-evidence/d96f-*.data`, `d96f-analysis.txt` (gitignored, this desk only).
The manager did not re-run the read below; the reading agent's independent `perf script`
re-aggregation, which reproduced the Children figures, is the check.

## What happened

Three `perf record -e cycles -F 999 --call-graph dwarf,32768` records of the pinned `validate`
bench binary, 0 lost samples. `perf report --children` read **children ≡ self for every
symbol**, no chain reached `_start`, and the report finished in seconds — the same "garbage
callchains" boot E had blamed on a frame-pointer-less build under `-g`. Nothing in the report
said the unwinder had failed. `perf script -v` did:

```text
unwind: failed with 'address range overlaps an existing module'
```

118 259 times.

## Why

LLD's default layout puts the first `R E` segment of a PIE at `vaddr = offset + 0x1000`.
perf's libdw unwinder (`tools/perf/util/unwind-libdw.c`) reports each module to libdw at
`base = map start − pgoff`, which assumes `vaddr == offset` for the mapped segment; on an
LLD-linked binary that places the module 0x1000 off, libdw finds the range overlapping one it
already has, and every unwind of that binary fails. Whether a GNU-ld-linked
binary avoids it was not tested here.

A secondary trap in the same records: the first samples of each record belong to `taskset`
**before** it `exec`s the bench (same pid), so they are attributed to the wrong image unless
cut off by time.

## The read-time workaround — no re-record, no rebuild

1. Copy the binary into a separate tree and change the **first `PT_LOAD`'s `p_vaddr` and
   `p_paddr` from 0 to 0x1000** — two bytes; `cmp -l` against the original shows exactly two;
   build-id, sections, symbols and `.eh_frame` are unchanged, so perf accepts it as the same
   binary.
2. Read with `--symfs <that tree>` and `--time <exec timestamp>,` to drop the pre-`exec`
   `taskset` samples:

   ```text
   DEBUGINFOD_URLS= perf report -f -i d96f-<k>.data --symfs <fixed tree> --time <exec>, --children -s sym --percent-limit 0
   ```

With that, every chain reaches `_start`, children exceed self (8.49 % against 2.86 % for the
descent), and an independent `perf script` re-aggregation reproduces the Children figures.

## The rule

**children ≡ self for every symbol is not a flat profile; it is an unwinder that returned
nothing.** Before reading a `--call-graph dwarf` profile of an LLD-linked binary, run `perf
script -v` over a slice and count `unwind: failed` lines; if they are there, apply the `--symfs`
fix above or record with frame pointers instead. Boot E's "the callchains are garbage" was this
trap read as a property of the build.

## What guards it

**No automated guard.** It needs root `perf` on the §9 desk and a record file that lives only
there. The guard is the `STATUS.md` *Do not* line and this page.

## Sources

- perf's libdw unwinder, where the module base is computed:
  <https://github.com/torvalds/linux/blob/master/tools/perf/util/unwind-libdw.c>
- LKML thread on the same failure: <https://lkml.iu.edu/hypermail/linux/kernel/2601.2/05459.html>
- linux-perf-users report: <https://ratatoskr.run/linux-perf-users/2026/08/17447226/t> (read from
  a search snippet only; the page returned 403).
