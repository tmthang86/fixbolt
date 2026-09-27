# ADR-0211 — Each SWAR kernel is judged on its own case, and the checksum is measured against a loop the compiler already vectorises

- **Status**: **Accepted — 2026-09-27**, by the manager under the owner's delegation of that day ("tôi ủy quyền cho bạn duyệt toàn bộ"), with the plan
  [2026-09-27-p4-simd](../plans/2026-09-27-p4-simd.md). It would **supersede, in part,
  [ADR-0100](ADR-0100-simd-is-reopened-as-an-experiment-whose-kill-line-is-written-before-the-code.md)
  decision 3**: the single "kept only if **all** of" over three cases is split into one line per
  kernel, and the density arm gains a validity condition. ADR-0100 decisions 1, 2 and 4, the
  15 % / 2 % / 3 % numbers (the owner's Q7) and the "share **or** density" clause (the owner's
  Q6) are unchanged. ADR-0100 is Accepted and its text is not edited; on acceptance of this ADR its
  status block gains one line pointing here (`CLAUDE.md` §5).
- **Date**: 2026-09-27
- **Deciders**: Tran Manh Thang (by delegation to the manager, 2026-09-27). Written by the architect (Opus).
- **Related**: ADR-0100; [ADR-0045](ADR-0045-parse-is-under-one-percent-of-the-wire-and-simd-is-declined.md)
  decisions 1–4; [ADR-0090](ADR-0090-a-measurement-boot-is-pre-built-shares-one-control-arm-and-a-five-step-drift-is-closed-one-named-segment-at-a-time.md)
  (the rotation); [ADR-0102](ADR-0102-a-line-that-moves-while-its-instruction-count-does-not-is-a-layout-move-and-the-count-is-read-off-the-desk.md)
  decisions 1–2 (the instruction count); [ADR-0049](ADR-0049-bench-builds-pin-function-alignment-and-the-flag-is-read-back.md);
  [ADR-0190](ADR-0190-the-io-uring-transport-is-reaped-by-the-idle-strategy-and-an-hft-turn-enters-the-kernel-once-without-waiting.md)
  *Result*; [ADR-0204](ADR-0204-kernel-bypass-is-a-later-phase-candidate-that-reopens-on-a-named-machine.md).

## Context

ADR-0100 decision 3 keeps SIMD only if `parse NewOrderSingle (validated)`, `parse Heartbeat
(validated)` **and** a checksum-only case each improve by ≥ 15 %, **and** either parse is ≥ 2 %
of the fastest surviving round trip or `density` at N = 64 improves by ≥ 3 %. Four facts found
while planning rows 8–9 were not in front of it:

1. **The checksum is already SIMD.** `crates/codec/src/checksum.rs` is a plain
   `wrapping_add` loop. `[measured 2026-09-27]` `rustc 1.98.0 -O` (the pinned toolchain,
   `rust-toolchain.toml`) compiles that loop, copied verbatim into a scratch file outside the
   repository, to SSE2 `paddb` over two 16-byte registers per iteration with a `psadbw`
   reduction — 32 bytes per iteration on the default `x86-64` target. With
   `-C target-cpu=x86-64-v3` the same source becomes AVX2 `vpaddb` over four `ymm` registers,
   128 bytes per iteration. An 8-byte SWAR sum therefore competes with a 32-byte vector loop, not
   with a byte loop. The only public figure (klittlepage, cited by ADR-0100) is an AVX2 checksum
   about 2× faster *than compiler-vectorised code* — a `core::arch` result, the arm ADR-0100
   decision 2 reserves for a plan of its own.
2. **The SOH scan is not.** The same compile turns `find_soh` (`crates/codec/src/parse.rs:120`,
   `iter().position(|&b| b == SOH)`) into one `cmpb` per byte, with or without `x86-64-v3`. LLVM
   has an early-exit loop vectoriser enabled by default
   ([`LoopVectorize.cpp`](https://github.com/llvm/llvm-project/blob/main/llvm/lib/Transforms/Vectorize/LoopVectorize.cpp),
   `enable-early-exit-vectorization`), but it does not fire on this loop in rustc 1.98. So the SOH
   scan is the kernel where SWAR replaces a byte loop; the checksum is the kernel where it
   replaces a vector loop.
3. **The share clause cannot be met, by arithmetic, whatever SWAR does.** Items 1 and 2 of
   ADR-0098 both left: `io_uring` failed its line (ADR-0190 *Result*, U/K wire p50 1.29–1.30),
   bypass cannot run on this NIC (ADR-0203, ADR-0204). The fastest round trip left is kernel TCP:
   loopback `hft` admin p50 15 149–16 021 ns (`DESIGN.md` §8, boot B, two procedures) and the
   acceptor NIC wire p50 27 778–28 402 ns (`measured-costs.md` *Phase 4's §9 boot, 2026-09-27*).
   `parse Heartbeat (validated)` is 59.5 ns (`measured-costs.md` *A-desk*, `w0`, n = 20), so its
   share is 0.37–0.39 % of loopback and 0.21 % of the wire. Reaching 2 % needs parse ≥ 303 ns or
   a round trip ≤ 2 975 ns; SWAR can only make parse smaller. Even the unpublished
   `mitigations=off` loopback A/B (−58.4 %, item 51) leaves ~0.9 %. Only the density arm is live.
4. **A 3 % density move is the size of a layout move.** ADR-0102 shows a +10 % move of a case
   nothing touched, whose instruction count moved −0.03 %. `engine turn, 64 busy sessions` itself
   moved +4.45 % across a PR not aimed at it (`measured-costs.md` *A-desk*, `w0` → `wa`, in band;
   how much of that was work and how much layout was not separated). Mytkowicz et al. measured the same class of bias across compilers and CPUs
   (["Producing wrong data without doing anything obviously wrong!"](https://dl.acm.org/doi/10.1145/1508244.1508275),
   ASPLOS 2009). ADR-0049's pinned function alignment narrows it; it does not remove it.

Read together: under ADR-0100's joint line, an 8-byte SWAR checksum that loses to a 32-byte
compiler loop — the likely result — would also remove an SOH scan that met its own cases, and a
density gain the size of a layout move could keep code that does nothing.

## Decision

1. **Two kernels, two lines.** The SOH scan and the checksum are built as separate commits and
   measured as separate arms of one ADR-0090 rotation, against one control:
   `A` (the commit before any SWAR, with the checksum bench present), `S` (SOH scan only), `C`
   (checksum only), `SC` (both). Medians over ≥ 20 complete rounds, same boot, `DESIGN.md` §9
   line, `scripts/ab-rotation.sh`.
   - **The SOH scan passes its codec line** if, `S` against `A`, `parse NewOrderSingle
     (validated)` and `parse Heartbeat (validated)` each improve by ≥ 15 %. (The checksum is
     identical in `A` and `S`, so the difference is the scan's.)
   - **The checksum passes its codec line** if, `C` against `A`, the judged checksum case
     (`checksum NewOrderSingle`, the whole frame before `10=` of the shared bench fixture)
     improves by ≥ 15 %.
2. **The share-or-density clause is read once, on the arm made of the kernels that passed
   their codec line** (`S`, `C` or `SC`). The share half is computed and printed as ADR-0100
   wrote it (fact 3 says it will read no). The density half — `engine turn, 64 busy sessions`
   ≥ 3 % faster than `A` — counts **only if** `scripts/bench-instructions.sh` on the `density`
   binaries of `A` and that arm reads **`work-changed`** (ADR-0102 decision 1). A `same-work`
   density move is a layout move, not the kernel's, and does not keep anything.
3. **What is kept, and what is removed.** A kernel is kept only if it passed its codec line
   **and** decision 2 passed for the arm it is in, and the rest of ADR-0100 decision 3 holds
   (allocation benches 0; 59 / 59 and FIXT unchanged; the differential tests, the fuzz targets
   and Miri green). If only one kernel passed its codec line and decision 2 fails, that kernel is
   removed too. There is no search over sub-arms: one arm, read once. Everything not kept is
   removed on the same branch, and every arm's figures go to `measured-costs.md`.
4. **The checksum's "before" is the loop as the compiler builds it today**, SSE2 on the default
   target, and the plan records the disassembly fact above beside the pair. Whether an embedder
   gets an AVX2 checksum by building with `-C target-cpu=x86-64-v3` is **not** measured here: it
   is a build flag, not code in `codec`, and the rotation builds every arm with one set of
   `RUSTFLAGS` (ADR-0049). It stays a question for the plan ADR-0100 decision 2 reserves for a
   `core::arch` arm.

## Alternatives considered

- **Keep ADR-0100 decision 3 as written.** Rejected: fact 1 makes the joint line judge the SOH
  scan by whether SWAR beats a vector loop in a different function — a kill line that answers a
  question nobody asked.
- **Drop the checksum from the experiment** because fact 1 predicts its loss. Rejected: ADR-0100
  decision 2 names it, the owner chose "SOH scan + checksum", and the cost is one small commit
  and one arm. A measured loss is what ADR-0045 decision 2 said this project did not have.
- **Drop the share clause, since fact 3 settles it.** Rejected as unnecessary: it is an *or*, it
  costs nothing to print, and the owner kept it on 2026-09-23 (Q6) for a reason fact 3 does not
  touch — a codec-only line would keep code nobody on the wire can see.
- **A wider density margin instead of the instruction count.** Rejected: ADR-0095 decision 2 and
  ADR-0102 decision 2 already chose "name the cause" over "widen the band"; the count is the
  cause-naming instrument the repository has.
- **Build decoupled timed loops first (ADR-0102 decision 4).** Not needed here: the checksum
  cases go in a bench target of their own, so no existing bench binary changes before the SWAR
  commits. Decision 4 stays for the first plan that re-records the whole row.

## Consequences

**Good**

- A kernel that works is not removed because the other one did not.
- A density "gain" that is only a layout move cannot keep code.
- The likely outcome is stated before the code: the checksum loses (fact 1), the share half
  reads no (fact 3), and the SOH scan survives only if it both clears 15 % on two parse cases and
  shows up as work in `density`.

**Bad — and accepted**

- **Four arms instead of two**: a longer boot (the rotation's time grows with arms × suites),
  and one arm (`C`) lives on a local branch built from `A` plus one cherry-picked commit — a tree
  no pull request shows. Its commit is recorded by sha in the evidence and the delivery log.
- **The instruction-count condition can refuse a real gain** whose instruction count happens to
  land within 0.1 % of `A`'s. For a kernel that replaces a byte loop that is unlikely, and the
  refusal is on the safe side.
- **Arithmetic that decides a clause before the code is written** (fact 3) invites the
  question why the clause is kept. It is kept because the owner kept it; this ADR only records
  that it is settled.
- The AVX2-by-build-flag question stays open, and a reader may assume it was measured. It was
  not; the plan's *Ngoài phạm vi* says so.

## Sources

- The disassembly: `rustc 1.98.0 -O --emit asm` and `-C target-cpu=x86-64-v3`, on a scratch copy
  of the two functions, 2026-09-27, on `tmt-B450-I-AORUS-PRO-WIFI` (not a timing; no §9 line
  needed).
- klittlepage, *Accelerated FIX processing via AVX2 vector instructions*,
  <https://www.klittlepage.com/articles/accelerated-fix-processing-via-avx2-vector-instructions/>
  (via ADR-0100).
- LLVM loop vectoriser, early-exit support:
  <https://github.com/llvm/llvm-project/pull/120567>, <https://llvm.org/docs/Vectorizers.html>.
- Mytkowicz, Diwan, Hauswirth, Sweeney, ASPLOS 2009, <https://dl.acm.org/doi/10.1145/1508244.1508275>.
- A search for a SWAR (not AVX2) FIX delimiter scan with published numbers against a one-pass
  field-indexing parser (2026-09-27) found none. What it returned describes AVX2 or
  simdjson-style structural scans (ADR-0100 already cites the two with figures), and no source
  whose numbers could be compared with this codec's.
- `measured-costs.md` *A-desk* and *Phase 4's §9 boot, 2026-09-27*; `DESIGN.md` §8 *Boot B*.
