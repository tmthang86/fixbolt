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
| `interop-acceptor:` | a `libquickfix` `SocketInitiator` | `fixbolt::serve` in `standard` mode: the poller, the pre-session table, the settings file, the library `Handler` and the session layer under all of it — **and, since 2026-09-05, `Admin::shutdown` through the front door**: the process is asked to stop rather than killed, and `serve` returns a `Shutdown` (ADR-0054, `STATUS.md` item 47) | **8 / 8** |
| `interop-reconnect:` | a `libquickfix` `SocketAcceptor` **killed with `SIGKILL` and restarted on the same `FileStore`** | `fixbolt_engine::connect_and_serve`: the reconnect ladder, the `Recovery` seam, `add_resumed`, the engine turn, and — since ADR-0054 — the durable count against the live one read through an `Observer` | **6 / 6** |
| `interop-reconnect-logout:` | the same, **stopped with `SIGTERM`** so it says goodbye first | the same, after a clean logout — ADR-0043 decision 5 | **6 / 6** |
| `interop-reconnect-beat:` | the same as the `SIGKILL` row, at **`HeartBtInt=1` with a pause before the kill**, so a `Heartbeat` is guaranteed between the last application message and the death | the same, with the last number spent belonging to a message no journal holds bytes for — ADR-0053 | **6 / 6** |

The seven steps of the acceptor direction: `logon` (with `141=Y` echoed); `order` (two
`35=D`, two `35=8`, paired by `11=`); `heartbeat` (an unprompted `35=0` with **no** `112=`,
within a deadline read from the `108=` on the wire); `testrequest`; `resend` (**the two
original sequence numbers replayed with `43=Y`**, not merely something carrying `43=Y`);
`gapfill` (the acceptor asks `35=2 7=n 16=0`, and a fresh TestRequest after the gap fill is
answered); `logout`. The eighth is the script's own, not the C++ initiator's: `shutdown` — a
line on stdin reaches `Admin::shutdown` through `fixbolt::serve`, and the process **returns**
and prints its `Shutdown` instead of being killed. `[measured 2026-09-05]` removing the
`admin.shutdown` call makes it read *"this engine's acceptor did not return from serve within
10s"* and fail in 29 seconds — the bound is there because a reversal that hangs proves nothing
([a-reversal-can-fail-by-hanging](reference/a-reversal-can-fail-by-hanging.md)).

**A stop line, and only a line.** `[measured 2026-09-05]` the first version of that trigger
treated end-of-input as the signal too, and this job is what refuted it: a background process on
a runner has no terminal on stdin, so it read `interop: stopping on ""` and the acceptor
**stopped before the counterparty connected**. Only a non-empty line stops it now; `< /dev/null`
leaves it serving and says so
([a-control-channel-the-launcher-already-closed](reference/a-control-channel-the-launcher-already-closed.md)).

The six steps of the `SIGKILL` scenario: `dropped` (no `35=5` in the first transcript, so the
ending really was abrupt); `back` (a Logon reaches the restarted acceptor, and **nothing told
this engine to send it** — `reconnect::Policy` did); `next_out` (**relational**: that Logon's
`34=` is one past the last number this engine sent before the kill, read off the transcript
rather than written as a literal); `next_in` (every `35=B` the restarted acceptor sends is
*delivered to the application*, which a session whose inbound count had restarted would have
gap-requested instead); `no_resend` (no `35=2`, no `141=Y`, no `MsgSeqNum too low`, printed beside the number of times
the engine resumed — see below); and `two_sources`, which needs
ADR-0054's handles to exist at all — the number the journal resumes from is never **behind** a
number an `Observer` already saw spent.

**`two_sources` is an inequality, and that is not a weakened equality.** `[measured
2026-09-05]` the first version demanded `live == resumed + 1` and read `resumed 4, live 6`: the
application also speaks first on logon, so the constant was wrong, and a gate resting on a
constant like that breaks when the *application* changes. The deeper reason is ADR-0053's own
argument — an observer knows the number *when somebody asks*, so a message sent between the
last poll and the ending is spent, durable and invisible on that side. On a clean logout it
always is: answering the counterparty's `35=5` and dropping the link happen inside one turn.
Sampling can only make the live number **low**, so the direction is safe where equality is a
race. Its teeth are measured: with the outbound mark removed, `interop-reconnect-beat` reads
`BEHIND (resumed 3, already seen live 5)` — and the logout scenario stays green, which is the
limit above, stated by the gate itself.

The `SIGTERM` scenario makes the **same five**, with `goodbye` in place of `dropped`, plus
`two_sources`. `[measured
2026-09-05]` **until this week it could only make three**, and the third was `known_gap`, which
pinned a known limitation on purpose: after a clean logout this engine answered the venue's
`35=5`, spending an outbound number the journal did not record, so the resumed session was
refused by exactly one — `STATUS.md` item 48. The journal now records that count
([ADR-0053](decisions/ADR-0053-the-journal-answers-two-questions-and-the-second-is-a-number.md)),
so the assertion that pinned the gap is gone and the scenario asserts continuation like the
other two.

**And the third scenario exists because the first two were green for a reason that was not the
engine.** Both ran at `HeartBtInt=30`, chosen so no `Heartbeat` could fall inside their few-second
window — which is exactly the condition item 48 was about, chosen by the fixture. The
`interop-reconnect-beat` round removes it: `HeartBtInt=1` and a deliberate 2.5 s pause before the
kill, so the last number spent belongs to a `Heartbeat`. It came back at `34=5` having sent up to
`34=4`, which is what says the fix is about *every* administrative message rather than about
`35=5`.

**`no_resend` has a premise, and it is printed now.** `[measured 2026-09-05]` the step asserts
no `35=2` in the restarted venue's transcript, which assumes the engine reconnected **once**.
`SIGTERM` makes QuickFIX log its sessions out and *then* shut down, so its listening socket
outlives the goodbye — and on a loaded runner the reconnect ladder's 200 ms first rung dialled
into a venue that was already stopping, spent a number on a `Logon` nobody would answer, and came
back one higher than the restarted venue expected. The venue asked for a resend: **both ends
correct, and the gate red with a message about numbering that had not happened.** The fixture now
takes the listener away as soon as the goodbye is answered, and the step prints `resumes: N`.

**The `SIGTERM` scenario's `no_resend` step is what found a second defect**, in the inbound
direction: the counterparty's `Logout` was consumed and never marked, so a resumed session
expected it again and sent a `ResendRequest` for a message it already had. `35=2: 1` where it
wanted none. The gate found it; this repository's own tests did not.

**Machine and run for these three.** The **3 / 3** the `SIGTERM` row used to carry was gated on
`ubuntu-latest` in the blocking `interop` job: job
[`101218167551`](https://github.com/tmthang86/fixbolt/actions/runs/33933972142/job/101218167551)
of run [`33933972142`](https://github.com/tmthang86/fixbolt/actions/runs/33933972142), commit
`c839854` — **the merge commit on `main`, not the branch tip**, because a branch being green
says nothing about the commit where it meets `main`. 11 jobs of 11.

**The 6 / 6 + 6 / 6 + 6 / 6 is CI's, on the commit that changed it.** `[measured 2026-09-05]`
`ubuntu-latest`, cmake 3.31.6, g++ 13.3.0, libquickfix at the pinned `386ce46e`: job
[`101302253512`](https://github.com/tmthang86/fixbolt/actions/runs/33964556891/job/101302253512)
of run [`33964556891`](https://github.com/tmthang86/fixbolt/actions/runs/33964556891) on
`3d16527`, **22 checks of 22 across that run and
[`33964529347`](https://github.com/tmthang86/fixbolt/actions/runs/33964529347)**. The job's own
log was read rather than its conclusion, and the lines that matter are byte-identical to the
development run — which is what says they are not one machine's accident:

```
interop-reconnect-logout: next_out    ok    sent up to 34=3 before the kill, came back at 34=4, wanted 34=4
interop-reconnect-logout: no_resend   ok    35=2: 0, 141=Y: 0, 'MsgSeqNum too low': 0
interop-reconnect-beat:   next_out    ok    sent up to 34=4 before the kill, came back at 34=5, wanted 34=5
interop: 7 / 7 + 8 / 8 + 6 / 6 + 6 / 6 + 6 / 6 against libquickfix @ 386ce46e…
```

**This is a run on the PR branch, not on a merge commit on `main`**, which is the stronger thing
and is owed when it merges.

**These two scenarios are the first evidence for `connect_and_serve` that this repository did
not write.** ADR-0043 said so in its own *Consequences*: *"every test of this is invented … only
an interop scenario driving a real counterparty through a disconnect would close that"*.

**Machine and run.** Recorded on macOS 15 (Apple M5) and gated on `ubuntu-latest`
(cmake 3.31.6, g++ 13.3.0) in the blocking `interop` CI job: job
[`100900997589`](https://github.com/tmthang86/fixbolt/actions/runs/33833427382/job/100900997589)
of run [`33833427382`](https://github.com/tmthang86/fixbolt/actions/runs/33833427382), commit
`f94e36e`. The job's own log was read rather than its conclusion: `interop: PASS 7/7`,
`interop-acceptor: PASS 7/7`, `==> the run added nothing git can see`.

### What these 33 cases do not buy

- **They are not a second corpus.** Seven and eight cases in the two directions, eighteen
  across the three reconnect scenarios, against 59 definitions.
- **The reconnect scenarios do not cover a fixbolt process that restarts.** Only the venue
  dies; this engine stays up throughout. Recovery across *this* process ending is
  `crates/engine/tests/on_disk.rs`, and it has no independent opinion.
- **`hft` mode is not covered.** `serve_hft` spins a core at 100%, and a shared CI runner is
  the wrong place for it. The three `hft` entry points still have no gate.
- **One counterparty, one identity, no TLS, no schedule, no shards.**
- **The scoring reads raw wire strings**, not `libquickfix`'s application callbacks. That is
  deliberate: QuickFIX drops a PossDup replay of a number it has already seen before the
  application sees it, and the `resend` step asks for exactly that. A judge written on the
  callbacks would report a correct answer as a missing message.
