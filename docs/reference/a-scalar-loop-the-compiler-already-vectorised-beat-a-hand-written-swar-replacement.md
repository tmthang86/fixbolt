# A scalar loop the compiler already vectorised beat a hand-written SWAR replacement

`[measured 2026-09-28]` phase 4 rows 8–9, the SIMD boot
([ADR-0211](../decisions/ADR-0211-each-swar-kernel-is-judged-on-its-own-case-and-the-checksum-is-measured-against-a-loop-the-compiler-already-vectorises.md),
[ADR-0212](../decisions/ADR-0212-an-ab-boot-stops-early-only-to-discard-the-micro-benches-run-a-fixed-twenty-rounds-and-the-density-arm-gets-one-futility-look-at-six.md)
*Result*). Full figures: [measured-costs.md](measured-costs.md) *Phase 4's SIMD boot,
2026-09-28*.

## The assumption

`crates/codec/src/checksum.rs`'s FIX checksum is `bytes.iter().fold(0u8,
|acc, &b| acc.wrapping_add(b))` — a byte-at-a-time loop. The obvious read of that source is
"scalar", and an 8-byte SWAR rewrite (sum eight bytes at once as one `u64`, fold the lanes down
at the end) looks like it can only help. That reading is source-level, not codegen-level, and
this codec has no `#[no_mangle]` or inline-asm barrier stopping the compiler from doing its own
work on the loop.

## What it actually was

`rustc 1.98.0 -O` (the pinned toolchain) already auto-vectorises the `wrapping_add` fold. On the
default `x86-64` target it becomes **SSE2**: 16-byte `xmm` registers, `paddb` for the add,
`psadbw` for the horizontal reduction. `objdump` on this boot's own `A` binary counts **184**
`paddb` instructions in the checksum's disassembly. With `-C target-cpu=x86-64-v3` (not this
project's default, and not built here) the same source becomes AVX2 over 32-byte `ymm`
registers. An 8-byte SWAR sum therefore does not compete with a byte loop — it competes with a
32-byte-per-iteration vector loop the compiler already wrote for free. The SWAR kernel's own
`objdump` shows **0** `paddb`: the compiler cannot auto-vectorise a hand-rolled `u64` fold the
same way, because the reduction pattern it recognises is gone.

`find_soh` (`crates/codec/src/parse.rs`, `iter().position(|&b| b == SOH)`) is the other case:
LLVM's loop vectoriser does not fire on it in `rustc` 1.98 (it has an early-exit vectoriser, but
it is not enabled by default and does not trigger here), so that byte loop really is scalar. The
two kernels needed opposite predictions, and were judged separately for exactly this reason
(ADR-0211 decision 1).

## The numbers

24 complete rounds, §9 line, `A` (control) against `C` (checksum-SWAR) and `S` (SOH-scan-SWAR):

| Kernel | Case | `A` (compiler-vectorised / scalar) | SWAR arm | Move |
|---|---|---|---|---|
| checksum | `checksum NewOrderSingle` | 4.5 ns/op | 14.5 ns/op | **+222.2%**, slower |
| SOH scan | `parse NewOrderSingle (validated)` | 128.6 ns/op | 134.7 ns/op | +4.8%, slower |
| SOH scan | `parse Heartbeat (validated)` | 64.6 ns/op | 64.8 ns/op | +0.3%, slower |

The checksum kernel's kill line was `≤ −15%`; it landed at `+222.2%` — **3.2× slower**, not
faster. The byte-loop kernel (`find_soh`) also did not win, contrary to the "scalar loop, SWAR
should help" reading: `+4.8%` and `+0.3%`, both the wrong sign though nowhere near as large a
miss as the checksum's. Neither kernel cleared its line; both were reverted in `83ddd20`
(ADR-0211 decision 3 — no partial keep, no search over sub-arms).

## What now guards it

`benches/baselines.tsv` carries the three `checksum` cases (`checksum NewOrderSingle` 4.5 ns,
`checksum Heartbeat` 2.9 ns, `checksum 1 KiB` 11.6 ns, all margin `1.10`, recorded `27f4cdb`
from this boot's `A` rotation). `crates/codec/benches/checksum.rs` (plan row 8) measures them
on every `scripts/bench.sh --strict` run. A checksum implementation slower than the compiler's
own vectorisation of the scalar fold — SWAR or otherwise — trips `checksum NewOrderSingle` over
its `×1.10` band (`> 5.0 ns/op`) and fails `--strict`. The regression this page is about is
therefore not hypothetical: it is exactly what tripped when `C`'s tree ran `--strict` on this
boot (`stage1-summary.txt`, `checksum NewOrderSingle` `+222.2%`, `0/24` over only because the
*baseline itself* had not been recorded from `A` yet at that point in the boot — once it was,
the same number is what the band exists to catch).

## Related

- [a-benchmark-that-measures-where-the-compiler-put-it](a-benchmark-that-measures-where-the-compiler-put-it.md) —
  a different way a compiled binary's layout, not the code under test, moves a number. This page
  is about the compiler changing what instructions a loop *is*, not where it sits.
- [ADR-0045](../decisions/ADR-0045-parse-is-under-one-percent-of-the-wire-and-simd-is-declined.md) —
  declined SIMD on arithmetic (parse's share of the round trip) before either kernel was built.
  This boot adds a second, independent reason the checksum kernel specifically was never going to
  pay: the "before" was not scalar.
- [measured-costs.md](measured-costs.md) — every case, all four arms, the pre-boot instruction
  counts and the disassembly counts this page cites.
