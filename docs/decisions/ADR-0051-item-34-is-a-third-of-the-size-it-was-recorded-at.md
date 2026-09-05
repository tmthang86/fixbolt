# ADR-0051 — Item 34 is a third of the size it was recorded at, and the 40 ns was never a measurement

> **Status:** Accepted (2026-09-05) · **Amends:** [ADR-0041](ADR-0041-the-library-layer-buys-an-api-with-a-template-per-message.md)
> (the ratio it published) and [ADR-0044](ADR-0044-a-builder-that-is-not-moved-per-field.md)
> (the ratio it moved) · **Closes:** ADR-0041 open questions 1 and 2
> **Plan:** [the-baselines-and-the-pass-nobody-timed](../plans/2026-09-02-the-baselines-and-the-pass-nobody-timed.md), step 3

## Context

ADR-0041 shipped the library layer with a sentence that every user-facing document then
carried: *a reply through `Handler` costs ~2 µs against **40 ns** for a template built once —
about 50×*. ADR-0044 halved the numerator and the sentence became *~956 ns against 40 ns,
about 24×*. `STATUS.md` item 34 has held that ratio as the size of the problem since, and the
wave-D draft `const-templates-in-dict` was scoped against it.

Both ADRs said their absolutes came from a shared 4-vCPU VM that fails §9 and that **the ratio
is what transfers**. Step 3a of the owning plan is the first time the three `library` cases
have been recorded on a §9 machine, and it is the first time anybody has asked where the
denominator came from.

## What was measured

§9 desktop, AMD Ryzen 7 3700X, `scripts/check-machine.sh` `pass 12 fail 0 unknown 1`, bench
build with function alignment pinned (ADR-0049), medians of 20 qualifying `scripts/bench.sh`
runs, `benches/baselines.tsv` 2026-09-05:

| Case | ns/op |
|---|---|
| `library, parse only` | **159.6** |
| `library, reply only` | **804.1** |
| `library, on_message` | **1028.6** |
| `encode ExecutionReport (template)` — D9's shape, a template built once and patched | **237.6** |

**1. The ratio, from committed benchmarks only.** `reply only` against `encode ExecutionReport
(template)` is **3.4×**. `on_message` against it is 4.3×. Not 24×, not 50×.

The two shapes are comparable in the direction that matters: the library's reply is three fixed
fields, two session slots and ten handler fields; the codec's template is three fixed fields
and fourteen slots. If anything the template case does *more* work per encode.

**2. The 40 ns has no committed benchmark, and the committed one disagrees with it by 4–5×.**
ADR-0041 attributes the figure to a *"`crates/library/benches/attrib` experiment, 2026-09-02"*.
That file was never committed; `git log --all` has no trace of it. The committed benchmark for
exactly that shape — `encode ExecutionReport (template)` in `crates/codec/benches/serialize.rs`
— read **177.6–199.4 ns on the same VM class** in the same week (ADR-0016's table), and reads
237.6 here. Whatever the experiment timed in 40 ns, it was not the work the sentence describes.

Non-negotiable 10 is exact about this: a number without the committed benchmark that produced
it is somebody else's claim. **The 40 ns is withdrawn as a denominator.** It is not replaced
with a guess about what it measured — the experiment is gone and cannot be re-read — only with
the committed figure.

**3. The cost of materialising a `Template` per message, by inference.** `reply only` minus the
committed encode: **804.1 − 237.6 ≈ 570 ns.** Labelled as inference: `reply only` also holds
`Reply::new` and `message()`, and the codec case encodes without a dictionary while the library
encodes through `encode_with::<Fix44>`; neither is measured apart and neither is expected to
be large. No new case is added to prove it, because item 34's *decision* does not turn on
whether it is 520 or 600.

**4. ADR-0041 open question 2 closes: on a §9 machine the three cases add up.** parse 159.6 +
reply 804.1 = 963.7 against `on_message` 1028.6 — the whole is **65 ns *more* than the sum,
6.7%**, which is a handler's `msg.get()` calls and the dispatch. On the VM the whole read
~200 ns *less* than the sum, and ADR-0041 recorded it without explaining it. It was the VM.

**5. What it is worth on the wire.** `tools/w2w --path app` reads **19 908 ns** p50 on this box.
The ~570 ns that a per-message materialisation costs is **2.9%** of that round trip, and the
whole convenience layer over a hand-written `Application` is at most ~800 ns, **4%**.

## Decision

**The owner chose (A) on 2026-09-05. Item 34 closes as measured and accepted.** The sentence in every document becomes
*"a reply through `Handler` costs about 3.4× a template built once — roughly 570 ns more on the
§9 desktop, 2.9% of an application round trip"*. The wave-D draft `const-templates-in-dict` is
re-scoped from *"close item 34"* to *"an optimisation worth ≤ 570 ns per reply"* and takes its
place in wave D on that footing, or is dropped. ADR-0041's *"the front door is 50× slower than
the house"* is amended to *3.4×*, which is a different sentence about a different decision.

**Not chosen — (B), item 34 open with the honest target.** 570 ns to remove and 2.9% the
ceiling of what removing it buys. Rejected because a codec hot-path change (`Template` and
`TemplateBuilder`, D9) for at most 2.9% of a round trip is the wrong trade *today*, and the
draft can be picked up on its re-scoped footing if that changes.

**Regardless of the choice**: every document carrying *40 ns*, *50×* or *24×* is corrected in this ADR's
commit — those sentences are false today regardless of what is decided about the work.

## Consequences

**Good**

- The library layer's cost is stated from committed benchmarks on a §9 machine for the first
  time, and the number is a fifth of what the documentation said.
- ADR-0041 open questions 1 and 2 are answered rather than carried.
- A draft plan is re-scoped before it becomes a plan, which is what item 45's *re-verify every
  draft against the code of the day* rule is for.

**Bad, and named**

- **Four user-facing documents and two rustdoc comments carried a ratio whose denominator had
  no benchmark for three days**, through two ADRs and one plan revision, and the number was
  repeated with a `[measured]` tag each time. The tag was true of the numerator. Nobody asked
  the denominator the question non-negotiable 10 exists to ask.
- **What the 40 ns experiment measured is unknowable now.** The likeliest shape — an encode
  whose output was never read, so the optimiser removed the copy — is the one
  [a-benchmark-can-delete-its-own-work](../reference/a-benchmark-can-delete-its-own-work.md)
  had recorded the *previous day*, on 2026-09-01. That is a guess and this ADR does not carry it
  as a finding.
- **The inference in point 3 is an inference.** A `TemplateBuilder::build` case would make it
  a measurement; it is not added here because no decision turns on it.

## Alternatives rejected

- **Add a `build only` case and record it before deciding.** Would sharpen 570 to a measured
  figure. Rejected for this ADR because the choice between (A) and (B) is the same at 520 or at
  600, and adding a case to the same bench binary moves the other three (ADR-0049, and
  `baselines.tsv`'s note of 2026-09-05 records exactly that happening to the `validate` cases).
- **Leave the 40 ns with a caveat.** A caveated number is still quoted without the caveat. It
  is withdrawn.
