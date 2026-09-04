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
- **Both roles have an independent check, and both are narrow.** `[measured 2026-09-04]`
  Each is interop-green against a real `libquickfix`, **7 / 7 and 7 / 7**, blocking in
  CI — a second implementation, which is the only opinion this project does not write
  itself, but **7 cases each, not 59**. See §7 below for the commands and the machine.
  Everything else on this page is still this repository's runner reading QuickFIX's
  definitions.

---

## 7. Interop against a real `libquickfix`, both directions

`[measured 2026-09-04]` **The only evidence on this page that this repository did not
write.** Everything above is this project's runner reading QuickFIX's `.def` files; this
section is another engine, at the other end of a kernel socket, disagreeing or not.

Both directions run in **one script and one CI job**, against `libquickfix` built from
source at the pinned commit `386ce46e917ae494ab6e90b1be90fd421cdbe3f9` — the same commit
`scripts/fetch-quickfix-assets.sh` pins for the acceptance corpus, checked by the script
before anything else runs. Two pins that can drift apart would make a disagreement
between the corpus and the counterparty unattributable.

```
scripts/interop.sh
```

| Direction | Driver | Under test | Score |
|---|---|---|---|
| `interop:` | this engine's `Session<Initiator, 256>` over a blocking socket | a `libquickfix` `SocketAcceptor` | **7 / 7** |
| `interop-acceptor:` | a `libquickfix` `SocketInitiator` | **`fixbolt::serve` in `standard` mode** — the poller, the pre-session table, the settings file, the library `Handler`, and the session layer's acceptor under all of it | **7 / 7** |

The seven steps of the acceptor direction: `logon` (including that `141=Y` is echoed),
`order` (two `35=D`, two `35=8`, paired by `11=`), `heartbeat` (an unprompted `35=0`
carrying **no** `112=`, within a deadline read from the `108=` on the wire), `testrequest`,
`resend` (**the two original sequence numbers replayed with `43=Y`**, not merely something
carrying `43=Y`), `gapfill` (the acceptor asks `35=2 7=n 16=0`, and a **fresh** `TestRequest`
after the gap fill is answered), `logout`.

**Machine and run.** Recorded on macOS 15 (Apple M5) and gated on `ubuntu-latest`
(cmake 3.31.6, g++ 13.3.0) in the blocking `interop` CI job — job
[`100900997589`](https://github.com/tmthang86/fixbolt/actions/runs/33833427382/job/100900997589) of run
[`33833427382`](https://github.com/tmthang86/fixbolt/actions/runs/33833427382), commit `f94e36e`,
**and the job's own log was read back rather than its conclusion**: `interop: PASS 7/7`,
`interop-acceptor: PASS 7/7`, `==> the run added nothing git can see`.

### What these 14 cases do not buy

- **They are not a second corpus.** 7 cases per direction against 59 definitions. A
  `libquickfix` counterparty agreeing on seven exchanges says nothing about the other 52.
- **`hft` mode is not covered.** `serve_hft` spins a core at 100%; a shared CI runner is
  the wrong place for it, and the three `hft` entry points still have no gate.
- **One counterparty, one identity, no TLS, no schedule, no shards.**
- **The scoring reads raw wire strings**, not `libquickfix`'s application callbacks —
  deliberately: QuickFIX drops a `PossDup` replay of a sequence number it has already seen
  before the application sees it, and the `resend` step asks for exactly that. A judge
  written on the callbacks would report a **correct** answer as a missing message.
