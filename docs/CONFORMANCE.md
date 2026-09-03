# Conformance Results

What this engine has been measured to do, with the command, the machine, and — where
it ran in CI — the run id. **No number appears here that is not already in
[STATUS.md](../STATUS.md) under *Proven*.** A figure you cannot trace to a command that
was run is not a result; §10 of the engineering rules calls an inferred green no green
at all.

Correctness figures below need a command and a machine, not §9 tuning — they do not
depend on how the box was set up. Latency is a different axis and does **not** live on
this page; see [DESIGN.md](DESIGN.md) §8 and the HFT playbook.

---

## 1. The 59 QuickFIX acceptance definitions

The 59 definitions are the session layer's primary gate. They pass through four
independent paths, which is what makes the score mean something rather than measure one
code path four times:

| Path | Command | Result |
|---|---|---|
| In process, pure session machine | the `score` runner | **59 / 59** |
| Through a real kernel-TCP socket | `cargo test -p fixbolt-engine --test wire` | **59 / 59** |
| In `standard` (blocking) mode | the same wire test, both idle strategies | **59 / 59** |
| Through two shards | the sharded runtime, corpus routed by counterparty | **59 / 59** |

Measured on an Apple M5 **and** on Linux x86_64 (AMD Ryzen 7 3700X). The control that
gives the score meaning: a `Replay` fake that answers with each file's own expected
output also scores 59 / 59 — so the runner is not scoring itself.

`[CI]` The correctness suite is green in GitHub Actions: run
[`33623429649`](https://github.com/tmthang86/fixbolt/actions/runs/33623429649) on commit
`cdd6fba`, 10 checks of 10.

---

## 2. Dictionary agreement with QuickFIX

The generated dictionary tables were checked against QuickFIX's own generated C++
`[measured 2026-08-28]`:

| What | Result |
|---|---|
| Tag numbers | **912 / 912** agree |
| Field types | **898 / 912** agree — the 14 differences are each named by tag |
| (message, tag) pairs | **12 524 / 12 524**, checked as 84 816 exhaustive answers |
| Enum values | **1 708 / 1 708** agree |
| Message types | **93** |

Eight reversals were run against these tables and all eight went red — the tables are
proven able to disagree, not merely observed to agree.

---

## 3. Repeating groups

`[measured]` Field order within groups agrees with QuickFIX's generated C++ on
**730 / 730** groups (731 with `NoHops(627)`, which lives in the header). Groups are
read and written nested to depth 4, at **0 allocations** walking all four levels.

---

## 4. Zero allocation on the hot path

`benches/alloc.rs` runs a counting allocator over every hot path and asserts zero:

| Crate | Paths | Allocations |
|---|---|---|
| `codec` | 6 | **0** |
| `session` | 13 | **0** |
| `engine` | 7 | **0** |

Each case asserts its own path is live, so a case that stopped exercising its path
would fail rather than pass silently. `[measured]` The guard is proven by reversal: one
`to_vec()` placed inside the timed loop takes the counter from 0 to 2 000 over 2 000
messages.

---

## 5. Codec micro-figures

`[measured on an Apple M5, 2026-08-27 — a development laptop, not a §9 machine]` parse
**77.0 ns**, encode **93.8 ns**, at **0 allocations**, over **304 million** fuzz
executions with no failure. These are laptop numbers, labelled as such: they say the
code is fast on that box, not that it meets a budget on a tuned one.

---

## 6. What is not proven here

On the same page, deliberately:

- **No latency figure lives on this page.** Round-trip and wire-to-wire numbers belong
  to [DESIGN.md](DESIGN.md) §8 and the playbook, each carrying its own machine and §9
  settings. Do not read a conformance page as a performance claim.
- **The corpus is not adversarial.** The 59 files are conformance definitions, not a
  fuzzing corpus — see
  [reference/a-conformance-corpus-is-not-an-adversarial-one.md](reference/a-conformance-corpus-is-not-an-adversarial-one.md).
  A 59 / 59 score is agreement with a known oracle, not proof against inputs nobody
  wrote down.
- **The 14 field-type differences are real.** They are named by tag in the dictionary
  reference, not smoothed over. Agreement is 898 / 912, not 912 / 912.
- **The initiator's independent check is narrow.** `[measured]` It is interop-green
  against a real `libquickfix`, 7 / 7, blocking in CI — a second implementation, which
  is the only opinion this project does not write itself, but 7 cases, not 59.
