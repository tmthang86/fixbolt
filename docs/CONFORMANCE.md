# Conformance Results

What this engine has been measured to do, with the command, the machine and, where it ran in
CI, the run id. **Nothing appears here that is not also listed under *Proven* in
[STATUS.md](../STATUS.md).** A figure without a command that was run is not a result.

Correctness figures need a command and a machine, not OS tuning, because they do not depend
on how the box was set up. Latency is a different axis and lives in [DESIGN.md §8](DESIGN.md)
and the [HFT playbook](hft-playbook.md), never on this page.

---

## 1. The 59 QuickFIX acceptance definitions

The 59 definitions are the session layer's primary gate. They pass through four independent
paths, so the score measures four code paths rather than one path four times:

| Path | Command | Result |
|---|---|---|
| In process, pure session machine | `cargo test -p fixbolt-session --test score` | **59 / 59** |
| Through a real kernel-TCP socket | `cargo test -p fixbolt-engine --test wire` | **59 / 59** |
| In `standard` mode, engine blocking between steps | the same wire test, second case | **59 / 59** |
| Through two shards, routed by counterparty | `cargo test -p fixbolt-engine --features affinity --test shard_wire` | **59 / 59** |

Measured on an Apple M5 **and** on Linux x86_64 (AMD Ryzen 7 3700X). The control that gives
the score meaning: a fake that replays each file's own expected output also scores 59 / 59,
so the runner is not scoring itself.

`[CI]` run [`33623429649`](https://github.com/tmthang86/fixbolt/actions/runs/33623429649) on
commit `cdd6fba`, 10 checks of 10.

---

## 2. Dictionary agreement with QuickFIX

The generated tables were checked against QuickFIX's own generated C++ `[measured 2026-08-28]`:

| What | Result |
|---|---|
| Tag numbers | **912 / 912** |
| Field types | **898 / 912**; the 14 differences are each named by tag |
| (message, tag) pairs | **12 524 / 12 524**, checked as 84 816 answers in both directions |
| Enum values | **1 708 / 1 708** |
| Message types | **93** |

Eight reversals were run against these tables and all eight went red, so the tables are
proven able to disagree, not merely observed to agree.

---

## 3. Repeating groups

`[measured]` Field order inside groups agrees with QuickFIX's generated C++ on **730 / 730**
groups (731 with `NoHops(627)`, which lives in the header). Groups are read and written nested
to depth 4 with **0 allocations** walking all four levels.

---

## 4. Zero allocation on the hot path

`benches/alloc.rs` runs a counting allocator over every hot path and asserts zero:

| Crate | Paths counted | Allocations |
|---|---|---|
| `codec` | 6 | **0** |
| `session` | 16 | **0** |
| `engine` | 24 | **0** |

Each case asserts that its own path is live, so a case that stopped exercising its path fails
instead of passing silently. `[measured]` The guard is proven by reversal: one `to_vec()` in
the timed loop takes the counter from 0 to 2 000 over 2 000 messages.

---

## 5. Codec micro-figures

`[measured 2026-08-27, Apple M5, a development laptop and not a tuned machine]` parse
**77.0 ns**, encode **93.8 ns**, **0 allocations**, and **304 million** fuzz executions with no
failure. These say the code is fast on that laptop, not that it meets a budget on a tuned box.
The tuned-box figures are in [DESIGN.md §6](DESIGN.md).

---

## 6. What is not proven here

- **No latency figure is on this page.** Round-trip numbers belong to [DESIGN.md §8](DESIGN.md)
  and the playbook, each with its machine and settings.
- **The corpus is not adversarial.** 59 / 59 is agreement with a known oracle, not proof
  against inputs nobody wrote down
  ([reference/a-conformance-corpus-is-not-an-adversarial-one.md](reference/a-conformance-corpus-is-not-an-adversarial-one.md)).
- **The 14 field-type differences are real.** Agreement is 898 / 912, not 912 / 912.
- **The independent check is narrow.** Both roles are interop-green against a real
  `libquickfix` (§7), but that is **7 cases each, not 59**. Everything else on this page is
  this repository's own runner reading QuickFIX's definitions.

---

## 7. Interop against a real `libquickfix`, both directions

`[measured 2026-09-04]` **The only evidence on this page that this repository did not write.**
Another engine sits at the other end of a kernel socket and either agrees or does not.

Both directions run in **one script and one CI job**, against `libquickfix` built from source
at commit `386ce46e917ae494ab6e90b1be90fd421cdbe3f9`. That is the same commit
`scripts/fetch-quickfix-assets.sh` pins for the acceptance corpus, and the script checks the
two pins agree before running anything, so a disagreement between corpus and counterparty is
always attributable.

```
scripts/interop.sh
```

| Direction | Driver | Under test | Score |
|---|---|---|---|
| `interop:` | this engine's `Session<Initiator, 256>` over a blocking socket | a `libquickfix` `SocketAcceptor` | **7 / 7** |
| `interop-acceptor:` | a `libquickfix` `SocketInitiator` | `fixbolt::serve` in `standard` mode: the poller, the pre-session table, the settings file, the library `Handler` and the session layer under all of it | **7 / 7** |

The seven steps of the acceptor direction: `logon` (with `141=Y` echoed); `order` (two
`35=D`, two `35=8`, paired by `11=`); `heartbeat` (an unprompted `35=0` with **no** `112=`,
within a deadline read from the `108=` on the wire); `testrequest`; `resend` (**the two
original sequence numbers replayed with `43=Y`**, not merely something carrying `43=Y`);
`gapfill` (the acceptor asks `35=2 7=n 16=0`, and a fresh TestRequest after the gap fill is
answered); `logout`.

**Machine and run.** Recorded on macOS 15 (Apple M5) and gated on `ubuntu-latest`
(cmake 3.31.6, g++ 13.3.0) in the blocking `interop` CI job: job
[`100900997589`](https://github.com/tmthang86/fixbolt/actions/runs/33833427382/job/100900997589)
of run [`33833427382`](https://github.com/tmthang86/fixbolt/actions/runs/33833427382), commit
`f94e36e`. The job's own log was read rather than its conclusion: `interop: PASS 7/7`,
`interop-acceptor: PASS 7/7`, `==> the run added nothing git can see`.

### What these 14 cases do not buy

- **They are not a second corpus.** Seven cases per direction against 59 definitions.
- **`hft` mode is not covered.** `serve_hft` spins a core at 100%, and a shared CI runner is
  the wrong place for it. The three `hft` entry points still have no gate.
- **One counterparty, one identity, no TLS, no schedule, no shards.**
- **The scoring reads raw wire strings**, not `libquickfix`'s application callbacks. That is
  deliberate: QuickFIX drops a PossDup replay of a number it has already seen before the
  application sees it, and the `resend` step asks for exactly that. A judge written on the
  callbacks would report a correct answer as a missing message.
