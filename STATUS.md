# Current state

One screen. A pointer, not a store. Detail lives in the ADRs and the plan files.
**A stale status page is worse than none.**

Last updated: **2026-08-31** — `ktls-spike` closed, open item 10 answered. Before that: Re-verified on Linux the same day — see the wire-gate entry under **Proven** and open item 17. Later that day the whole suite ran for the first time on **the owner's own Linux desktop** (AMD Ryzen 7 3700X, Linux 7.0.0-30), which also **unblocked open item 10** and exposed two defects in the scripts that were supposed to be telling us so.

## Start here — 2026-08-31, end of session

**Five decisions signed and two plans approved on 2026-08-30, and from that day the owner
delegated plan-writing and plan approval to the agent working here.** Nothing is blocked on a
signature; everything below is blocked on work. **What the delegation did not change is
`CLAUDE.md` §10** — it removed a signature, not the evidence each unit of work owes, and the
approval gate was never the safety net here in the first place.

| | |
|---|---|
| **[ADR-0010](docs/decisions/ADR-0010-a-reconnect-is-not-a-restart.md)** `Accepted`, **implemented 2026-08-31** | a reconnect resumes, `141=Y` resets. `Session::resume(cfg, next_out, next_in)` carries numbers across a restart; a session from `Session::new` still resets. **[session-recovery](docs/plans/2026-08-30-session-recovery.md) is CLOSED** — all six steps, and [ADR-0017](docs/decisions/ADR-0017-the-inbound-count-is-persisted-after-delivery.md) came out of step 5 |
| **[ADR-0011](docs/decisions/ADR-0011-a-full-ring-disconnects.md)** `Accepted`, **implemented 2026-08-31** | a full ring disconnects, the refusal is never silent, capacity → 4 MiB. The engine sends `58=slow application` — deliberately **not** D10's `slow consumer`, because here the counterparty is faultless. **[ring-full-policy](docs/plans/2026-08-30-ring-full-policy.md) is CLOSED**, and open item 5 with it |
| **[ADR-0012](docs/decisions/ADR-0012-latency-first-and-one-session-per-polling-thread.md)** `Accepted` | latency beats density; every figure names its `N`. Decisions 1–2 re-scoped to `hft` by ADR-0013 |
| **[ADR-0013](docs/decisions/ADR-0013-two-modes-standard-and-hft.md)** `Accepted` | **two modes.** `standard` blocks and runs anywhere and **is the default**; `hft` spins, pins and burns a core. **Amended `CLAUDE.md` §2 rule 4** — it is now mode-scoped and its `standard` half **has no machine check yet** |
| **[threads-and-affinity](docs/plans/2026-08-30-threads-and-affinity.md)** approved | 6 steps, `hft`-scoped. Step 1 is the affinity API ADR — **it is ADR-0015, not 0014**: `standard-mode` wrote its ADR first and §5 forbids reusing a number. That plan's own text still says 0014 |
| **[standard-mode](docs/plans/2026-08-30-standard-mode.md)** **closed** | 8 steps, `standard`-scoped. Built the default mode and closed rule 4's unenforced half. One thing deferred to a §9 machine, deliberately |
| **[ADR-0014](docs/decisions/ADR-0014-standard-mode-blocks-on-poll.md)** `Accepted`, **implemented** | `poll(2)` through `libc` behind a default-on `standard` feature; **Windows refused at compile time**, never a silent spin; `Waiting` is given the sources and `Transport` names its own; `Park` → `Yield`, neither mode, **fails both gates**. Answers **all four** of ADR-0013's open questions. Accepted **by standing delegation** — the owner delegated plan-writing and approval on 2026-08-30, so nobody read these nine decisions on their behalf; the ADR says so in its own header |

**On a fresh clone, before anything else:** `scripts/fetch-quickfix-assets.sh`. `vendor/` is
gitignored (`CLAUDE.md` §8) and the 59 acceptance definitions live there, so without it the
gate that decides every session-layer change cannot run at all. Then, on a machine you intend
to *measure* on: `fixbolt-machine on` and read `scripts/check-machine.sh` — five of its ten
rows do not survive a reboot.

**The next session's starting points, in the order that costs least:**

1. ~~**A plan for `standard` mode**~~ — **CLOSED and MERGED 2026-08-30** (`6d35b75`).
   [standard-mode](docs/plans/2026-08-30-standard-mode.md), 8 steps, and
   **[ADR-0014](docs/decisions/ADR-0014-standard-mode-blocks-on-poll.md)**. `standard` exists,
   **is the default** (`serve` blocks, `serve_hft` spins), and passes the 59 definitions.
   **One thing was left and it is not a slip**: the `standard` wakeup cost needs a §9 machine,
   and the plan's risk table said from the start it would keep `DESIGN.md` §8's *from the
   literature* label rather than close on a number from the wrong box.
2. ~~**The `standard` half of rule 4 has no gate**~~ — **CLOSED.**
   `scripts/check-standard-gives-the-core-back.sh`, four assertions, red halves `hft` **and**
   `yield`. `CLAUDE.md`'s machine-checked list now reads rule 4 as both halves.
3. ~~**`wait::Park` belongs to neither mode**~~ — **CLOSED.** It is `wait::Yield` now, and
   `[measured 2026-08-30]` it is **shown** to fail both gates rather than said to: 99.7% CPU,
   sleeping 0 of 20 samples.

4. ~~**Serialise misses its published 60 ns target and item 11 says why**~~ — **the "why" was
   wrong, and steps 1–4 of
   [serialise-and-the-60ns-target](docs/plans/2026-08-31-serialise-and-the-60ns-target.md)
   closed 2026-08-31** (`314d2b1`, `ebff188`). The linear slot scan is **~24% at most**, not
   the bulk. **~31 ns is spent before a single variable field is written** — 51% of the whole
   target on a message carrying nothing — plus **~7 ns per field in `put`**. The fix item 11
   proposed was then **written, measured and reverted**: predicted −36 ns, measured **+5.2 ns
   (+3.4%)** over 30 runs per arm. Kept from it: `crates/codec/tests/slot_order.rs`, the
   guard the body path never had for non-negotiable 5.

**Two things need the owner, and neither is blocked on work:**

- ~~**Is 60 ns the right target?**~~ — **ANSWERED 2026-08-31 by the owner, and built.**
  *"Hạ mục tiêu xuống mức với tới được, theo baseline từng máy"*, scoped to the whole of §6
  rather than the serialise row, with the absolute target column **removed** rather than
  lowered. That is
  **[ADR-0016](docs/decisions/ADR-0016-per-machine-baselines-replace-absolute-targets.md)**,
  and open items **11 and 20 both close** with it.
- **Branch convention for an agent session.** Work here has been going on one branch per plan
  per `CLAUDE.md` §8; a remote session is handed a single designated branch. They disagree and
  the repo should say which wins.

**What is next, and it is a choice rather than a queue.** `[2026-08-31]` four of the six plans
that were open this morning are now **closed**: `standard-mode`, `serialise-and-the-60ns-target`
(steps 5–6 done, §9 re-measurement **239.1 ns**, and ADR-0016 answers the target question),
**`session-recovery`** (all six steps) and **`ring-full-policy`** (steps 3–4, closing open item 5).

`[2026-08-31, later]` **`ktls-spike` closed too**, all four remaining steps, and with it **open
item 10** — the one ADR-0005 called load-bearing. `ktls-core` **can** be driven from a plain
non-blocking socket with no async runtime; the answer is *yes, with four conditions*, and
[ADR-0018](docs/decisions/ADR-0018-ktls-on-a-plain-socket-answers-adr-0005.md) records them.
A TLS plan is now unblocked and is deliberately not written yet.

What is left of the six: **`threads-and-affinity`** (approved, `hft`-scoped, its step 1 is
**ADR-0015**). It is work, not a decision, and it closes a **contradiction rather than adding a
feature** — `DESIGN.md` D8 says the engine thread is pinned to an isolated core and
`[measured 2026-08-30]` nothing in `crates/` or `tools/` pins anything, which is open item 21. **The `standard`-mode
measurements and open items 6, 11 and 13 all want the same machine — **and it already exists.**
`[measured 2026-08-30]` the desktop reached **`pass 10  fail 0  unknown 1`** and
`scripts/bench.sh --strict` **ran rather than refusing**, which is recorded under **Proven**.
What those items wait on is **time at that box**, not tuning. The one standing catch:
**five of the ten rows are runtime settings that do not survive a reboot** — governor, turbo,
SMT, THP and `busy_poll` — so a measuring session begins with `fixbolt-machine on` and a
`check-machine.sh` read, never with an assumption.

**The upstream contribution is open.** `[2026-08-31]` the owner authorised it explicitly — *"PR
ngược lên testing skill"* — and it went up as **two** stacked draft pull requests rather than one,
because two pull requests were already open upstream and the obvious single PR would have clobbered
both. [PR #6](https://github.com/tmthang86/testing-skills/pull/6) carries the eight cases this plan
found — seven where the evidence broke while the code was right, and one real bug (a wake after
shutdown raising `SIGPIPE`, found by a review bot) whose `unsafe` block had a SAFETY comment that
was correct and insufficient: it proved memory safety and said nothing about the signal contract.
[PR #7](https://github.com/tmthang86/testing-skills/pull/7) carries the protocol reference.
**`[2026-08-31]` [PR #8](https://github.com/tmthang86/testing-skills/pull/8) is the third**, stacked on
#6, carrying the three cases the `per-machine-baselines` plan found — all about a **benchmark** rather
than a test: the instrument that was 80% of its own smallest reading, a baseline recorded through a
shorter path than the gate that judges it, and a measuring loop that was part of what it measured. It
adds §20, §21, §21a and **§22** and renumbers the checklist's red half to 26–33. `[2026-08-31]` **Revised after review**: the owner's note is that `testing-skills` is a *general* testing skill, so a case must read as a shape any project can recognise — this project's case names, constant names and Rust-only mechanism talk were stripped, every measured number kept. §22 was added in that pass and is the strongest of the four, because it happened **twice through two different hardware mechanisms** — a rate carried across a branch-prediction boundary and one carried across a cache boundary — which is what makes it a shape rather than an anecdote. **Opened without running
`npm run validate`** — this machine has no Node — and the PR says so and lists what was checked by
hand instead. See the
contribution entry below for why they are stacked and what was deliberately left out.

**On the delegation itself, recorded so it is not inferred later.** `[2026-08-30]` the owner
delegated plan-writing and plan approval — *"tự lên plan và tự duyệt, tự chạy"* — and then, on
this branch, merging when the tests pass. Three things stayed out of it and stay out: amending
one of `CLAUDE.md` §2's ten non-negotiables, anything that leaves this repository for a public
one, and any claim that a gate is green without the run that says so.

**Read before touching the engine:** [GUIDE.md](docs/GUIDE.md) §0 for the mode split, and
[reference/measured-costs.md](docs/reference/measured-costs.md) for why the numbers are what
they are — including four instruments that could not see what they were pointed at, and eight
refuted hypotheses about a 324 ns mode that is still unexplained.

## Where the work is

**Next, and each needs its own plan before any code (Rule Zero):**

**Seven plans were written and approved on 2026-08-30**, and the seventh — `standard-mode` — is
**closed**. All of it comes with the owner's standing permission to
revise a plan mid-flight when reality disagrees with it — each revision recorded in that plan's
delivery log. In dependency order:

1. ~~**gates-that-can-be-trusted**~~ — **closed 2026-08-30.** Items 7, 17, 18 and 19 are gone,
   CI is green, and the whole suite passes on Linux for the first time. Everything below was
   waiting on it, because every plan closes by quoting a gate.
2. **[w2w-and-linux-numbers](docs/plans/2026-08-30-w2w-and-linux-numbers.md)** — **half done.**
   `tools/w2w` runs and **item 15 is closed**. Items 6, 11, 13 and the decision on 12 are
   **blocked on a machine matching `DESIGN.md` §9**, and the plan stops there rather than
   lowering the bar to close.
3. ~~**[ktls-spike](docs/plans/2026-08-30-ktls-spike.md)**~~ — **CLOSED 2026-08-31**, item 10
   with it. **The answer is yes, with conditions**, and the `tokio`-shaped documentation that
   made the question look hard belongs to `ktls`, a *different* crate: `ktls-core` 0.0.5 has no
   async feature at all and every entry point is generic over `AsFd`. `[measured 2026-08-31]`
   `strace -f` over 1000 round trips on an offloaded socket, attributed by tid: `recvfrom` and
   `sendto` and **nothing else**; the gate's red arm puts `poll(2)` in the same loop and must
   trip. Four conditions came out of it, each measured —
   [ADR-0018](docs/decisions/ADR-0018-ktls-on-a-plain-socket-answers-adr-0005.md),
   [reference/ktls-on-a-plain-socket.md](docs/reference/ktls-on-a-plain-socket.md),
   `scripts/check-ktls-on-a-plain-socket.sh`, `spikes/ktls`. **No TLS code is merged and no
   latency number is published** — both were out of scope on purpose, and stayed out.
4. ~~**data-fields**~~ — **closed 2026-08-30**, items 8 and 9.
5. **[session-recovery](docs/plans/2026-08-30-session-recovery.md)** — item 16. **Steps 1–3 done
   2026-08-30**: the journal reads back. Steps 4–5 wait on
   [ADR-0010](docs/decisions/ADR-0010-a-reconnect-is-not-a-restart.md), `Proposed`.
6. **[ring-full-policy](docs/plans/2026-08-30-ring-full-policy.md)** — item 5. **Steps 1–2 done
   2026-08-30**; steps 3–4 wait on [ADR-0011](docs/decisions/ADR-0011-a-full-ring-disconnects.md),
   which is **`Accepted` 2026-08-30** and awaits implementation, not a signature.
7. ~~**[standard-mode](docs/plans/2026-08-30-standard-mode.md)**~~ — **CLOSED 2026-08-30**, 8
   steps, plus **[ADR-0014](docs/decisions/ADR-0014-standard-mode-blocks-on-poll.md)**. Built
   the mode ADR-0013 made the default and nobody had written, and closed non-negotiable 4's
   unenforced half. `serve` blocks and `serve_hft` spins; the 59 definitions pass in both modes;
   all four of ADR-0014 decision 6's latency cliffs are shut. **Left open on purpose**: the
   wakeup cost, which needs a §9 machine.

Still unplanned, and deliberately: **`library`** (§7 step 8) and **steps 3–4 of the paused
initiator plan**, whose gate is interop against `libquickfix` rather than the mirrored corpus
(ADR-0004, ADR-0006).

| | |
|---|---|
| Branch | **`ktls-spike-steps-2-5`** — PR [#10](https://github.com/tmthang86/fixbolt/pull/10), `ktls-spike` closed. `[measured 2026-08-31]` **CI green on the commit being closed, `1b9b356`, run [`33386125577`](https://github.com/tmthang86/fixbolt/actions/runs/33386125577), 9 / 9 jobs.** Before that: **`main`.** PR #2 (`claude/project-status-hdx7k1`) merged 2026-08-30 as **`6d35b75`**, no-ff, 12 commits — `standard-mode` closed. `[measured 2026-08-30]` **CI green on the merge commit itself, run [`33326803468`](https://github.com/tmthang86/fixbolt/actions/runs/33326803468)**, and on the merged head `ae3d78a` before it, run `33325530208`, 9 / 9 jobs. `git diff ae3d78a 6d35b75` is **empty**, so the branch's green transfers to the merge exactly rather than by assumption. Before that: PR #1 merged as `76d6989`, run `33307963879` |
| Milestone | **M3 — the engine, closed, with one gate now known to be machine-dependent.** `[measured 2026-08-30]` the same 59 definitions pass **through a real socket** on the M5: `cargo test -p fixbolt-engine --test wire` → **59 / 59**. **On Linux the same command scores 39 / 59** — the harness's settle criterion is a spin count, not the engine. Open item 17; the in-process gate is 59 / 59 on both machines. `codec`, `dict`, `conformance` and `session` are closed behind it. What remains of `DESIGN.md` §7: step 7 `tools/w2w`, step 8 `library` |
| Scope | **[PRD.md](docs/PRD.md)** — phase 1 = FIX 4.4 tag=value both sides; phase 2 = SBE / FAST / FIXML + FIX 5.0. **TLS has ADR-0005 (Accepted), now supplemented by [ADR-0018](docs/decisions/ADR-0018-ktls-on-a-plain-socket-answers-adr-0005.md), and still no plan — but it is no longer blocked**: open item 10 closed 2026-08-31 |
| Plan in flight | **None.** `[2026-08-31]` five closed that day — `standard-mode`, `serialise-and-the-60ns-target`, `session-recovery`, `ring-full-policy`, `ktls-spike` — plus `gates-that-can-be-trusted` and `data-fields` on 2026-08-30. **One approved and not started: [threads-and-affinity](docs/plans/2026-08-30-threads-and-affinity.md)**, whose step 1 is ADR-0015. `w2w-and-linux-numbers` is **half done** (15 closed); its half B wants figures nobody has run on the tuned desktop |
| Last closed | **[2026-08-30-engine.md](docs/plans/2026-08-30-engine.md)** — closed 2026-08-30. **All six steps done.** `DESIGN.md` §7 step 6, taken before step 5 by decision. The gate that matters — the same 59 definitions **through a real socket** — went green at step 3 and did not move afterwards. Two ADRs came out of it: [ADR-0007](docs/decisions/ADR-0007-spsc-ring-without-unsafe.md) and [ADR-0008](docs/decisions/ADR-0008-journal-is-a-trait.md) |
| Paused | **[2026-08-29-session-initiator.md](docs/plans/2026-08-29-session-initiator.md)** — steps 1–2 done and merged 2026-08-30; steps 3–4 not started. Paused because the mirrored gate measures less than the plan assumed — see the two measurements below |
| Last closed | **[2026-08-28-session-layer.md](docs/plans/2026-08-28-session-layer.md)** — closed 2026-08-29. **All six steps done: 59 / 59.** Steps 1, 3, 4, 5 and 6b hit their prediction; step 2 missed it low (18 predicted) and step 6a missed it high (52 predicted), both for reasons written down in the plan. Eleven revisions recorded there |
| Last closed | **[2026-08-28-dict-validation.md](docs/plans/2026-08-28-dict-validation.md)** — closed 2026-08-28. Four validation tables, agreed with QuickFIX's own generated C++ on 912/912 tag numbers, 12 524/12 524 message-tag pairs and 1 708/1 708 enum values |
| Last closed | **[2026-08-28-conformance-runner.md](docs/plans/2026-08-28-conformance-runner.md)** — closed 2026-08-28. The 59 definitions run in process; a replaying fake scores 59 / 59, which is what makes the real score mean something |
| Last closed | **[2026-08-27-repeating-groups.md](docs/plans/2026-08-27-repeating-groups.md)** — closed 2026-08-28. Groups read and written, nested to depth 4; field order agreed with QuickFIX's own generated C++ on 730/730 groups |
| Last closed | **[2026-08-27-codec-dict.md](docs/plans/2026-08-27-codec-dict.md)** — closed and merged 2026-08-28. 54 tests, 0 allocations, 304M fuzz executions |
| Last closed | Design reviewed against the HFT latency budget and revised: positioning fixed to "fastest acceptor on kernel TCP", ADR-0002 default reversed (inline dispatch, ring optional), D8 busy-poll, D9 template encoder, D10 send backpressure, §8 latency budget, §9 OS checklist, wire-to-wire gate added |

## Proven — the command was run and its output read

**`[measured 2026-08-30]` A review bot found a real one, and it killed the process.** If the
engine is dropped while another thread still holds a `WakeHandle`, the self-pipe's read end
closes and the write end does not; `libc::write` into it raises `SIGPIPE`, whose default action
terminates. Reproduced before fixing — `signal: 13, SIGPIPE: write on a pipe with no one to
read` — and it is **invisible from any ordinary Rust test**, because the runtime sets `SIG_IGN`
before `main` and the ignored return value swallows the `EPIPE`. A library cannot assume its host
does that. Fixed by holding both ends jointly, so a writer always has a reader. The test lives in
its own binary because it changes a process-global disposition.

The `unsafe` block's SAFETY comment was **correct and insufficient**: it proved memory safety —
live pointer, right length, nothing retained — and said nothing about the signal contract.
`CLAUDE.md` §2 rule 8 asks for "a comment naming what proves it sound", and what was proven was
the wrong kind of soundness.


**`[measured 2026-08-30]` The `standard` gate agrees with itself on two different machines, with
two orders of magnitude of headroom.** Its ceilings are not the marginal kind open item 20 is
about. Same commit, same script:

| | this session's container | GitHub runner (`33324622355`) |
|---|---|---|
| `standard` | 0.00% CPU, sleeping 20/20, p50 **10 917 ns** | 0% CPU, sleeping 20/20, p50 **24 848 ns** |
| `hft` | 98.81% CPU, sleeping 0/20 | 98.83% CPU, sleeping 0/20 |
| `yield` | 99.70% CPU, sleeping 0/20 | 99.74% CPU, sleeping 0/20 |

The p50 ceiling is 1 000 000 ns and the two machines differ by 2.3× while sitting **40× and 100×
below it**; the CPU figures differ by hundredths of a percent. `DESIGN.md` §6's *timing* ceilings
swing 5–232% run to run on one box and 1.7× between two — this gate is measuring a different kind
of thing, a ratio and a count, and it is worth saying which kind before somebody assumes the
worst about both.


**`[measured 2026-08-30]` The 59 acceptance definitions pass with the engine actually blocking.**
`cargo test -p fixbolt-engine --test wire` now runs the corpus twice, once per idle strategy, and
scores **59 / 59** both ways — ADR-0013's *"two modes is two things to test, for ever"*, paid.
The blocking run takes 3.00 s of wall time and **0.25 s of user time**, which is the mode doing
what it says.

**`[measured 2026-08-30]` And a claim written into that test's own documentation was refuted by
reversal.** It said a wiring failure — the listener left out, writability never asked for, the
waker undrained — would show up as the run taking minutes instead of seconds. It does not: with
`Block` made to ignore readiness entirely the run took **3.30 s**, with the listener removed from
the poll set **3.34 s**, against a baseline of **3.28 s**. The settle criterion is 1 ms and the
blocking timeout is 5 ms, so **one block satisfies it whether it was woken by data or by its own
timeout**, and the harness cannot tell those apart. The test proves the protocol is unchanged
under blocking, which is real and worth having; the wiring is proven by `tests/standard.rs`
reading the interest list directly and by the p50 assertion in the `standard` gate. The comment
now says so.


**`[measured 2026-08-30]` Non-negotiable 4's second half is machine-checked, and it took four
assertions rather than the one ADR-0013 asked for.** CPU near zero is also what a **dead** thread
reports, what a run that **never reached the mode** reports, and what an engine woken by its own
100 ms timeout reports. The last is not hypothetical: with `Block` made to ignore readiness,
`scripts/check-standard-gives-the-core-back.sh` measured **0% CPU**, found the thread sleeping in
**20 of 20 samples** — assertions 2 and 3 both green — and a round-trip p50 of **99 046 599 ns**,
one whole timeout. Only the fourth assertion saw it. On this shared container: `standard` 0.00%
CPU / 20-of-20 sleeping / p50 10 917 ns; `hft` 98.81% / 0-of-20 / 19 909 ns; `yield` 99.70% /
0-of-20 / 18 096 ns — **the first time `wait::Yield` has been shown to fail both gates rather
than said to.**

**`[measured 2026-08-30]` And that gate was green for the wrong reason twice before it was
believed.** A missing pair of braces (`$12` is `${1}2`) broke every measurement, and because
*failed the policy* and *could not be measured* shared one exit code, **both red halves reported
`RED ok`** while nothing had been measured at all — a red half that is red because the harness is
broken proves as much as a green half that is green because nothing ran. And the p50 was read
with `grep -oE '[0-9]+' | head -1`, which returned **50** every time: the digits in the *label*
`p50`. The assertion that is the only thing able to distinguish "woken by the data" from "woken
by the clock" was comparing a constant against its ceiling, and passing in all three arms, which
is exactly why nothing looked wrong.


**`[measured 2026-08-30]` A tool that accepted a mode, announced it, and did not run it.**
`tools/w2w --mode standard` printed `mode: standard`, exited 0, and never entered its timed loop:
the branch selecting the blocking strategy sat behind `#[cfg(feature = "standard")]`, and
**features are per-crate** — `w2w` declared none of its own, so the condition was false and every
such branch took its `else`. The engine's feature being on by default did not reach it. `cargo
build` had warned on that exact line the whole time (`unexpected_cfg_condition_value`); what
found it was running the tool and noticing the latency block was missing. This is `CLAUDE.md` §2
rule 6 **inverted** — that rule guards a feature in the manifest with no `#[cfg]`, which makes a
crate unbuildable; this is a `#[cfg]` with no feature in the manifest, and everything builds
while a code path disappears. `scripts/check-no-kernel-sleep.sh` would have been green about two
runs of the same mode, so `w2w` now states the mode it took and the script **reads it back**
(reversal: `w2w ran mode 'yield' when 'hft' was asked for`, exit 1). Write-up:
[reference/feature-flags-unify-across-a-workspace.md](docs/reference/feature-flags-unify-across-a-workspace.md).


**`[measured 2026-08-30]` A CI job was green about a build that never happened.**
`cargo test --all --no-default-features`, the machine check for non-negotiable 6, **still built
`libc`**: `tools/w2w` is a workspace member depending on `fixbolt-engine` with defaults, and
cargo unifies features across one invocation, so the flag under test was switched back on by a
sibling crate. `cargo tree --workspace --no-default-features -i libc` prints the edge. It was
caught by a **test count** — the run should have reported 210 and reported 214, the four tests
of a `cfg`-gated file that should have vanished — and **a module carrying no tests of its own
would have hidden it completely**. Fixed by `scripts/check-no-optional-deps.sh`, which asks per
crate and is proven by reversal (drop `optional = true` → red, with the graph printed).
Write-up: [reference/feature-flags-unify-across-a-workspace.md](docs/reference/feature-flags-unify-across-a-workspace.md).

**`[measured 2026-08-30]` A test that was green because another thread had not reused a file
descriptor yet.** `crates/engine/tests/standard.rs` closed a socket and asserted that `poll`
called its descriptor unknown. It went red on the first cold run and then passed **30 runs in a
row**; the panic site named the `Ok(count == 0)` branch, which says what happened — another test
thread in the same binary had been handed that number, so the descriptor was live and quiet, and
quiet is indistinguishable from closed at that layer. Asking about `i32::MAX` instead is
deterministic: **0 failures in 40 runs**. A retry would have buried it.


- **The whole suite runs on the owner's desktop, 2026-08-30 — the first time any of it has.**
  `[measured]` rustc 1.98.0 on AMD Ryzen 7 3700X / Linux 7.0.0-30: `cargo fmt --check`,
  `cargo clippy --all-targets -D warnings`, `cargo test --all` and
  `cargo test --all --no-default-features` all exit 0, **0 failed**. The two gates that carry a
  number: `-p fixbolt-session --test score` 4 passed, and **`-p fixbolt-engine --test wire`
  passed** — the same 59 definitions through a real socket. That last one is the point:
  open item 17 recorded this gate at **39 / 59 on Linux**, and it is now green on a *second,
  unrelated* Linux machine. The fix from `gates-that-can-be-trusted` holds off the CI runner.
- **`benches/alloc.rs` holds on x86_64, not only on the M5.** `[measured 2026-08-30]`
  `scripts/bench.sh` on the desktop: **8 of 8 targets measuring, 0 silent, invariant failures
  0** — `codec` 6 paths, `session` 13 paths, `engine` 7 paths, every one **0 allocations**.
  Non-negotiable 1 is machine-checked on two architectures now. Three timing groups are over
  their ceilings on this untuned box — see open item 20, where the interesting part is *which
  direction* they miss in.
- **`§9 satisfied`, for the first time in this project — 2026-08-30.** `[measured]` after
  `isolcpus=6,7,14,15 nohz_full=6,7,14,15 rcu_nocbs=6,7,14,15 processor.max_cstate=1` on the
  kernel command line plus the five runtime rows, `scripts/check-machine.sh` reports
  **`pass 10  fail 0  unknown 1`** and `scripts/bench.sh --strict` **ran for the first time**
  rather than refusing. These are the first figures this repository has that non-negotiable 10
  permits publishing. `EXIT=1`, on three ceilings and one marginal:
  `walk 4 levels` **354.6** vs 300, `encode 1 group` **103.5** vs 75,
  `encode ExecutionReport` **240.0** vs 190, `ring, round trip` **500.5** vs 500;
  `ring, one way` **259.6** came in *under* its 260. Clean: `parse NewOrderSingle` 125.5 / 150,
  `parse Heartbeat` 55.6 / 70, `SendingTime from the cache` 4.9 / 5,
  `inline deliver + reply` 6.3 / 15. All eight targets measured, **0 invariant failures**.
  The three over-ceiling cases are the same three that were over in every machine state
  measured that day, which is what makes them real rather than noise.
  **A prediction failed here and is recorded rather than quietly dropped**: `fail 2` was
  predicted after the reboot and the answer was **`fail 5`** — governor, turbo, SMT, THP and
  `busy_poll` are all *runtime* settings that do not survive a reboot, and only the two
  command-line rows do.
- **Isolating a core does not remove the 324 ns mode — hypotheses six and seven.** `[measured
  2026-08-30]` pinned to cores 6,7 under `isolcpus` + `nohz_full` + `rcu_nocbs`, where the
  scheduler places nothing else and the tick is off: **5/60 against 4/60 unpinned**, medians
  259.0 and 259.4. Not the scheduler. Interrupts do still reach an isolated core (15.7k on
  each of 6 and 7 against 512k on CPU0), so those were counted per run too — **two outliers
  carried ~1300 interrupts and three carried exactly zero**, while a normal run carried 1085.
  Not interrupts either. **Hypothesis eight, ASLR, went the same way and refutes itself twice:**
  250 runs with `setarch --addr-no-randomize` against 250 without gave **14/250 vs 14/250,
  z = 0.00** — and with ASLR off the layout is *fixed* across runs, so a layout-dependent
  effect would have to be 0% or 100%, not 5.6%. At n=60 the same comparison had read 8.3%
  against 3.3%; **the third time in one day a small-sample rate difference dissolved.**
- **The box does not drift, which is what makes it publishable.** `[measured 2026-08-30]` a
  7-minute soak — 379 runs of `ring, one way`, §9 satisfied, pinned to the isolated cores, with
  temperature sampled per run — moves the median from **259.0 to 258.9 ns**, 0.2% between the
  first window and the last, while Tctl rises 59 → 64 °C in the first minute and then sits
  there. `r(temp, ns) = +0.060`. The 3700X throttles at 95 °C, so the **31 °C of headroom**
  means the Mini-ITX case and its `silent` fan profile constrain nothing at this load; the
  91 °C seen earlier came from an eight-spinner stress test, and even there the frequency never
  stepped down. Also settled: **GNOME Power Mode `Balanced` does not reach the measurement** —
  the driver is `amd-pstate-epp` and EPP already reads `performance` under `Balanced`, while a
  measurement's `governor=performance` collapses the available preferences to `performance`
  alone. The A/B meant to prove that **failed and is recorded as failed** (`powerprofilesctl`
  errored, so both arms ran `balanced`); the state readings are what answer it. And the failure
  found a real side effect: **`SMT off` breaks `power-profiles-daemon`**, which writes
  `policy11` — a CPU that is offline while SMT is off. Harmless, reverts, worth knowing.
- **`scaling_cur_freq` is frozen on a `nohz_full` core and reads 41% low.** `[measured
  2026-08-30]` it reported the isolated core at **2240 MHz** — exactly `scaling_min_freq` —
  while that core executed **7,895,418 loops/s against an ordinary core's 7,958,092, 0.8%
  apart**. `amd-pstate-epp` refreshes that file from a periodic tick and `nohz_full` stops the
  tick on precisely the cores worth measuring on. **The isolation that makes a core worth
  measuring is what breaks the instrument pointed at it.** Measure work per unit time, or
  `aperf`/`mperf`; `check-machine.sh` is unaffected, it never reads a per-core current
  frequency. Fourth instrument in one day that could not see what it was pointed at.
- **The 324 ns mode is now characterised, even though eight hypotheses about its cause have
  all failed.** `[measured 2026-08-30]` pooling those 500 §9-satisfied runs of
  `ring, one way`: **two discrete states with an empty gap** — main mode n=472, median
  **258.4**, stdev 1.98; second mode n=**28** (5.6%), median **323.7**, stdev **1.25**; and
  **exactly one value out of 500 lies between them**. Both clusters are equally tight, so this
  is not a perturbed run — **a process picks one of two states at startup and keeps it for
  life**, the two differing by a factor of **1.2527**, near 5/4. The practical consequence,
  which is what makes this worth having without a cause: **any single run of this case has a
  5.6% chance of being 25% wrong**, and every `DESIGN.md` §6 ceiling is read off single runs.
  [reference/measured-costs.md](docs/reference/measured-costs.md).
- **A cloud VM cannot be the §9 machine, and the checklist could not say so.**
  `[measured 2026-08-30]` `governor`, `turbo`, `C-states`, `SMT` and NIC IRQ affinity are
  **host** properties. A guest does not fail those rows loudly — the `/sys` files are absent,
  so it collects `unknown` and reads as under-configured rather than structurally unable.
  `check-machine.sh` gained a **`not virtualised`** row (`systemd-detect-virt` + steal over
  the same window), guarded by `scripts/check-machine-verdicts.sh` — 11 cases, no VM and no
  root needed, in CI. **The GitHub runner is itself a guest**, so the bench job's machine
  block now says so on every run — `[measured 2026-08-30]` run `33314721411` prints
  `FAIL not virtualised  guest under 'microsoft', 0% steal` and
  `FAIL machine is quiet  6% CPU busy — Runner.Worker 11% of a core`. That closes the
  commit's own "not proven": the GUEST path had been tested as logic only and has now run on
  a real guest, correctly naming Hyper-V and attributing the load to the Actions worker.
  **Every bench figure CI has ever produced came from a machine that is both a guest and not
  quiet, and from this commit the output says so.** Consequence for the plan: **development can move to cloud;
  measurement stays on the desk.** The first version of the row reported this bare-metal box
  as a guest — `systemd-detect-virt` **exits 1 when the answer is `none`**, so
  `$(... || echo unknown)` ran both halves and set the variable to two lines. The verdict test
  could not catch it, because it feeds the function directly and never sees how the argument
  is obtained; running the real script did.
- **Tuning a machine to §9 moves every bench median by under 2%.** `[measured 2026-08-30]`
  30 full `scripts/bench.sh` runs on the desktop, 15 with §9 tuning on and 15 with it off —
  the first time any machine in this project could be measured **in both states**. Three cases
  are over their ceiling on **15 of 15 runs in both**; the two ring cases are **coin flips**
  with the ceiling at the median. **The first write-up of this overstated it and a repeat
  sample caught that** — see the entry, which is kept with its correction rather than
  rewritten: `over 260` went `1/15` → `9/15` between two samples of the same command, so the
  supportable numbers are the pooled **33% tuned vs 93% untuned**, not a flip from red to
  green. The median moves **0.8%** and reproduces; the pass rate does not. A second mode near
  **324 ns** — five sightings, **all tuned**, none in ~45 untuned runs — is **unexplained**;
  the Zen-2 L3 hypothesis was **tested and refuted** (~259 ns in all three arms), and SMT-off
  is a suspect that has **not** been isolated.
  [reference/measured-costs.md](docs/reference/measured-costs.md).
- **The kTLS blocker was the diagnostic, not the kernel.** `[measured 2026-08-30]`
  `setsockopt(TCP_ULP, "tls")` is **ACCEPTED** on the owner's desktop; open item 10 is
  unblocked and `ktls-spike` is unpaused. The script had been reporting `CONFIG_TLS=m` and
  "it was built without CONFIG_TLS" in the same run. Both that and a second defect found while
  fixing it — `lsmod | grep -q` under `set -o pipefail` reporting **failure on the run where
  the module is found**, because `grep -q` SIGPIPEs `lsmod` — are written up in
  [reference/ktls-on-a-plain-socket.md](docs/reference/ktls-on-a-plain-socket.md) and guarded
  by `scripts/check-ktls-classify.sh`, 8 cases, which scores 3 / 8 against the old logic.
- **The 512-entry inline field array in `matthart1983/nanofix` costs 4–6× the parse time.**
  Changing `MAX_FIELDS` 512 → 64 took the heartbeat from 565.0 ns to **95.4 ns** and
  `NewOrderSingle` from 605.1 ns to **138.8 ns**, on an Apple M5 on 2026-08-27. Method and
  caveats: [docs/reference/measured-costs.md](docs/reference/measured-costs.md).
- `cargo test --no-default-features --lib` on that project **fails to link** — its `aeron`
  feature does not gate `mod aeron_c;`. Its "238 tests" cannot be run as shipped.
- `scripts/fetch-quickfix-assets.sh` runs and reports **59 acceptance definitions** on disk.
- The `.def` format was decoded from the files and `Comparator.rb`: 7 directives, one
  `<TIME>` placeholder, literal `0x01` separators. It pins **field ordering** positionally.
- **The ordering rule is tag-ascending within header and body, not XML order** — checked by
  script over all 247 expected lines, zero violations. Open item 4 closed by data.
- **`test/definitions/client/` holds exactly one file** — `Normal.def`, 271 bytes, 6 lines,
  FIX 4.2, logon then logout. **470 of the 471 `.def` files test the acceptor.** It also uses
  two directives absent from the seven recorded for the server format: a bare `eCONNECT`, and
  `R` for a reply the harness sends. This answers ADR-0001 open question 1: there is no
  initiator conformance suite to run, and that is the central cost priced in ADR-0004.
- The QuickFIX Software License was read directly: BSD-3 in shape, plus attribution and a
  naming restriction.
- `git check-ignore -v` blocks `vendor/`, `testdata/recordings/`, `*.docx`, `target/`,
  `.DS_Store`.
- Repository is **private** as of 2026-08-27 (it was public at creation). `cargo 1.95.0`.
- **ADR-0001, -0002, -0003 accepted 2026-08-27** by the owner, after the latency-budget review.
- **Step 1 of the codec plan is CLOSED, 2026-08-28.** 54 tests green. The closing condition —
  533 real messages through a simulated TCP read loop under 5 chunk patterns and byte-at-a-time
  — passes. `[measured]` parse 77.0 ns, encode 93.8 ns, **0 allocations**, 304 million fuzz
  executions with no crash. Full method and machine in
  [reference/measured-costs.md §5](docs/reference/measured-costs.md).
- **The serialise target is missed: 93.8 ns against a published 60 ns.** Cause identified
  (linear slot lookup, slots × parts), deliberately not optimised — the number that decides is
  the Linux one at the `engine` step.
- **New open item: Criterion is deferred.** `DESIGN.md` §6 names it; the benches use a 24-line
  dependency-free harness instead, because the benches must *assert* and Criterion measures
  without asserting. The cost is outlier detection and confidence intervals. Revisit when
  hot-path work moves to Linux.
- **`codec` parses FIX as of 2026-08-28.** 29 tests green; all 539 `.def` lines classified.
  Two more plan defects surfaced and were decided by the owner: the plan contradicted itself
  about `2t` (the boundary diagram gave it to the session, the parse algorithm to the codec —
  resolved by refusing only what cannot be **framed**), and `14a_BadField` could not pass with
  a plain `Err`, so `ParseError::BadTag` now carries a byte offset and the index keeps every
  field read before the failure.
- **Nothing in the corpus can be checksum-validated.** `[measured]` 244 `E` lines carry `10=`
  and **0** are the real checksum of their own bytes; 238 are literally `10=0`. The comparator
  matches tag 10 by regex. **This constrains the conformance plan**: frame validation belongs
  on the `I` side and on the engine's own output, never on expected lines.
- **`codec` + `dict` step 0 and step 1 landed 2026-08-28** on branch `plan/codec-dict`.
  `dict` generates 5 tables from `FIX44.xml`: 912 tag constants, 93 message types, 30 header
  tags, 16 DATA→length pairs, 84 required-field arms. 11/11 tests green. The plan was wrong in
  three places about the dictionary and was revised and re-approved — see its delivery log and
  the new **[reference/fix44-dictionary-traps.md](docs/reference/fix44-dictionary-traps.md)**,
  which records all four traps with the test that guards each.
- **New open item: the 3 trailer tags are unclassified.** `is_header(89)` and `is_header(93)`
  return `false`, so a written `Signature` would sort into the body. Nothing writes one — no
  `.def` carries a signature — so this is pinned by a test rather than fixed.
- **CI exists as of 2026-08-28**, and closes a gap that had been asserted eight times. "CI
  proves it" appeared in `CLAUDE.md` §2, ADR-0004, ADR-0005, `DESIGN.md` D11 and `PRD.md`
  while `.github/` did not exist. It now has three jobs. Two of them cannot do anything yet —
  there are no crates — and they emit a *skipped, not passed* annotation rather than a silent
  green tick.
- **`[workspace.lints.clippy] all = "warn"` did not enforce non-negotiable 7.** `[measured]`
  2026-08-28: a library crate containing `unwrap()`, `expect()` and `panic!()` passed
  `cargo clippy --all-targets -- -D warnings` with **exit 0**, because those three are
  `clippy::restriction` lints and `clippy::all` never includes them. `CLAUDE.md` §2 had
  claimed the workspace lints enforced this since the repository was initialised. Fixed, and
  `scripts/check-lint-config.sh` now proves it by reversal so it cannot silently regress.
  `priority = -1` on the group is mandatory — cargo rejects the config without it.
- **DESIGN.md, README.md, ADR-0003 and three reference pages swept 2026-08-27** to match the
  accepted decisions: bidirectional positioning, one latency floor figure (10–20 µs, was two
  conflicting), `MessageView` corrected to 24 bytes in all four places that claimed 16 or
  "two words", the real `parse_into` signature, D9 rewritten as a parts list, D11 added for
  TLS, an 8-step build order with the initiator in it, and three new gate rows. The
  `codec-dict` plan's doc checklist records which items this closed and which still need code.
- **ADR-0004 and ADR-0005 accepted 2026-08-27** by the owner. -0004 makes the engine
  bidirectional; -0005 makes TLS a transport with the hot-path guarantee stated per mode.
  Both accepted on reasoning, not measurement — ADR-0005's open question 1 is unanswered and
  load-bearing (open item 10).

- **Repeating groups, 2026-08-28.** `dict` generates group tables keyed by `(msg_type,
  counter)` — **59 counters, 731 positions**, from **93** `<group>` declarations of which
  **91 sit inside `<component>`**. `codec` reads groups nested to FIX 4.4's full depth of 4
  and writes them ordered by the dictionary. `group_roundtrip.rs`: **357 top-level positions
  round-trip byte-identical**, exercising all 59 counters. `benches/alloc.rs`:
  **0 allocations** walking four levels.
- **The group field order agrees with QuickFIX's own generated C++ on 730 / 730 groups**
  — delimiter exact everywhere, and QuickFIX's `message_order` an exact subsequence of this
  crate's member list in every case, with the 7 extra tags all being nested group counters.
  `crates/dict/tests/interop_quickfix_order.rs`. The one group QuickFIX has no file for is
  `NoHops(627)`, which lives in `<header>`: 730 + 1 = 731.
- **That interop test catches what the round-trip cannot, demonstrated by reversal.**
  Swapping two adjacent members in every generated group leaves `group_roundtrip.rs`
  **green** — it generates its messages from the same table — and turns the interop test
  **red**. Run 2026-08-28.

- **The conformance runner, 2026-08-28.** The 59 definitions load as **669 steps** — 289 `I`,
  250 `E`, 65 connects, 1 disconnect, 64 expected disconnects — and run in process against a
  `SessionUnderTest`. `NullSession` scores **0 / 59**, which is the true state of the world;
  `Replay`, which answers with each file's own expected output, scores **59 / 59**, which is
  what makes the zero evidence rather than a coincidence.
- **The echo application the corpus assumes.** `[measured]` 42 of the 250 `E` lines carry
  `35=D` and there are **22 application `(I, E)` pairs**; all 22 are reproduced, `9=101`
  included. A session state machine alone cannot pass this suite.
- **`<TIME>` is 17 bytes on an `I` line and 21 on an `E` line**, solved from the corpus's own
  `9=` values. The single-width substitution both loaders used before was wrong by 4 bytes per
  timestamp, and one loader now serves both crates.
- **The session layer scores 14 / 59, 2026-08-28 — step 2 of six.** The plan predicted 18. Its
  classification table said 12 files expect only `{A, 5}` back; solving that off the corpus
  gives **9**, and two of the reachable set turn on refusing a second connection with the same
  identity, which is step 6. The ceiling for step 2 is 14 and it scores 14. Eleven reversals
  were run; nine take the score down.
- **Two files had been passing by accident, and only a step that could *lose* points found it.**
  `1b_DuplicateIdentity` and `AlreadyLoggedOn` were passing because the second Logon was refused
  as *sequence too low* rather than as a duplicate identity — right answer, wrong reason. Making
  `connect` reset the sequence numbers, which `2i_BeginStringValueUnexpected` requires, exposed
  it. The score gate now names all 14 files.
- **The corpus cannot see the value of `52=`.** Stamping `SendingTime` from a constant instead of
  from the clock leaves the score at 14 / 59 with every test green: `52` is one of the five tags
  `fields.fmt` matches by shape. Held by `crates/session/tests/logon.rs` instead.
- **The step-1 six are still in the fourteen** — asserted separately, because a count that only
  goes up cannot say so.
- **Step 1 scored 6 / 59, 2026-08-28, exactly as predicted.**
  The six are `1c_InvalidSenderCompID`, `1c_InvalidTargetCompID`, `1d_InvalidLogonBadSendingTime`,
  `1d_InvalidLogonLengthInvalid`, `1d_InvalidLogonWrongBeginString`, `1e_NotLogonMessage` —
  named in the assertion, because a different six scoring 6 is a different result. Six
  reversals were run: five take the score to 5.
- **The sixth reversal did not.** Deleting "the first message must be a Logon" leaves 6 / 59,
  because `1e_NotLogonMessage.def` also carries a wrong `56=`. Two rules, one observation —
  written up in
  [reference/quickfix-acceptance-def-format.md](docs/reference/quickfix-acceptance-def-format.md)
  and now held by `crates/session/tests/logon.rs`.
- **The session layer allocates nothing**, on the accept path and on the refusal path, counted
  separately. `[measured]` `accept 0 refuse 0 tick 0 clock 0 text 0`; the reversal — one
  `format!` on the error path — reports `refuse 30000`.
- **`00000000-00:00:00` is not a date, and this loader was substituting it for `<TIME>`.**
  It is the corpus's placeholder for output the comparator never reads by value. Fixed to a
  real instant, which also fixed `<TIME-121>` running 86 279 seconds *forward*.
- **The four dictionary validation tables exist and agree with an independent generator,
  2026-08-28.** `[measured]` 912/912 tag numbers, 898/912 field types with 14 differences each
  named by tag, 12 524/12 524 (message, tag) pairs checked as 84 816 exhaustive answers, and
  1 708/1 708 enum values. Eight reversals were run and all eight go red.
- **The plan called the enum oracle weak and the plan was wrong.** A scouting script matched
  `const char Name_X = 'v';` and missed `const char Name_X[] = "vv";`, so it reported 228 of 245
  fields covered. The array form is 17 fields — including `SecurityType(167)`, the one field
  `14e_IncorrectEnumValue.def` actually tests. Read properly the oracle covers 245/245 and
  1 708/1 708, with zero exceptions. Written up in
  [reference/fix44-dictionary-traps.md](docs/reference/fix44-dictionary-traps.md).
- **`dict` grew about 33 KB of static data** and its build script still runs in under a second.
- **The session scores 27 / 59, 2026-08-28 — step 3, and the revised prediction was 27.** All
  thirteen `Reject (35=3)` files. Thirteen reversals were run and all thirteen go red.
- **All twelve `373` codes are produced, and that is asserted rather than inferred.** The test
  reads the codes out of the corpus's own `E` lines and checks the session emits each. The file
  count cannot say it: `14a_BadField.def` holds four cases, and answering all four with one code
  still passes the file.
- **One reversal was worthless and it is worth recording why.** It deleted a
  `next_in = seq + 1` that could never be reached, and nothing changed — which says nothing
  about the guard. A reversal has to alter behaviour before its green means anything.
- **The session scores 37 / 59, 2026-08-29 — step 4, and the revised prediction was 37.**
  Heartbeat, TestRequest, inbound `SequenceReset`, `ResetSeqNumFlag`, and a garbled frame that
  is ignored rather than fatal. Thirty-two reversals were run and all thirty-two go red.
- **Whether a message's sequence number is checked is per `MsgType`, not one rule.** A Logout is
  never checked; a `SequenceReset` never advances the count at all. Applying one rule to
  everything costs a file whichever rule is picked — measured four ways. Written up in
  [reference/quickfix-acceptance-def-format.md](docs/reference/quickfix-acceptance-def-format.md).
- **A rule this project invented was refuted by the corpus.** `FieldType::SeqNum` refused `34=0`,
  and its comment cited `11c_NewSeqNoLess.def` — which it had misread. `11a`, `11b` and `11c` all
  send `34=0` and QuickFIX processes them; restoring the rule costs three files. The refused case
  lived in the block whose own doc comment says the cases there are invented. **An invented test
  agreeing with an invented rule is one guess written twice.**
- **Three reversals were worthless in step 4, all the same shape: two guards covering one rule.**
  Each pair was reduced to the single guard that can actually be broken. A fourth was a test that
  checked the output and not the link, so a reversal that dropped the connection stayed green.
- **The session scores 42 / 59, 2026-08-29 — step 5, and the prediction was 42.** A message running
  ahead of the count is held and the gap is asked for; an inbound `ResendRequest` over
  administrative messages is answered with one `SequenceReset` gap fill. Nineteen reversals were
  run and all nineteen go red.
- **A gap is asked for once, and a Logon that runs ahead is answered before it is asked.** Both are
  stated exactly once in the corpus, by `10_MsgSeqNumGreater` and `1a_ValidLogonMsgSeqNumTooHigh`.
  Written up in
  [reference/quickfix-acceptance-def-format.md](docs/reference/quickfix-acceptance-def-format.md).

- **The session scores 55 / 59, 2026-08-29 — step 6a, and the prediction was 52.** The session
  now owns seven administrative message types and hands everything else to an `Application`,
  giving it the outbound sequence number and the clock and sending back whatever it returns.
  Eight reversals into the score go red; two more behaviours the corpus cannot see are held by
  `crates/session/tests/application.rs`.
- **The prediction was beaten for a measurable reason.** The 6a/6b split was drawn from the
  expected `35=` sets of the remaining files, and an expected set cannot tell an *echo* from a
  *replay*: `2d`, `3b` and `3c` look like they need an outbound store and do not — the
  counterparty resends and this end only echoes.
- **QuickFIX reads a tag as a *signed* integer**, so `-1=x` is a field and is Rejected, while
  `4garbled9=x` is not a field at all and the whole message is ignored. Three files turn on
  that one distinction. Written up in
  [reference/quickfix-acceptance-def-format.md](docs/reference/quickfix-acceptance-def-format.md).
- **"One identity, one connection" is an engine rule, not a session rule.** `1b_DuplicateIdentity`
  and `AlreadyLoggedOn` need it, and `engine` does not exist, so `crates/session/tests/score.rs`
  plays the smallest engine that can hold two connections. That is stated there and here.

- **The session scores 59 / 59, 2026-08-29 — step 6b, and the plan's last four files came in as
  predicted.** A resend replays the application messages this end sent, at the numbers they were
  sent with and spending none, and fills over every contiguous run it cannot replay. Nine
  reversals into the score go red; three more behaviours the corpus cannot see are held by
  `crates/session/tests/journal.rs`.
- **`2m_BodyLengthValueNotCorrect` turned out to be about framing, not about a store.** `9=` is
  taken at its word: a body length that does not land on a `10=` trailer discards the whole
  receive buffer, which is why a too-long one swallows the message after it. That is the
  engine's job, so it lives in `tests/score.rs` next to the identity rule — and the rubbish is
  still handed to the session once, so "fatal only if it claims to be a Logon" stays in one
  place.
- **The outbound journal is a stopgap, and it is in the wrong crate.** Eight 512-byte slots
  inside the session, in memory, lost on restart. `DESIGN.md` D1 has the session emitting a
  `Store` action and the engine holding the journal; that is what `engine`'s plan has to build.

- **The mirrored acceptance gate tops out at 45 of 50, not 50, and it is worth less than it
  looks.** `[measured 2026-08-30]` 46 of the 50 mirrorable files need this end to *originate* a
  message no state machine can invent — 42 a Logout, 19 an application message, 14 an unprompted
  Heartbeat, 13 a TestRequest with a given `112=`, 6 a SequenceReset, 4 a ResendRequest — so the
  harness has to play the operator, and wherever it does the gate measures numbering and framing
  rather than a protocol decision. A further 5 ask this end to send a message that is wrong on
  purpose, which a correct engine cannot do.
- **`ADR-0004`'s mirroring criterion is syntactic; the corpus's client-side badness is
  semantic.** `ADR-0006` fixed one gap in it (`iDISCONNECT`). The five files above are a second,
  and they cannot be detected by looking at bytes: all five are syntactically perfect. Interop
  against `libquickfix` is the answer, and this is the evidence for why ADR-0004 made it the
  primary gate rather than the secondary one.

- **The 59 definitions pass through a real socket, first run.** `[measured 2026-08-30]`
  `cargo test -p fixbolt-engine --test wire` → **59 / 59**. Kernel TCP, the real
  framer, the real session, the real application; the clock is the only injected part,
  because every `I` line in the corpus carries a fixed instant. No background thread and no
  sleep: `Engine::turn` is one non-blocking pass and the test drives it by hand, so it is as
  deterministic as the in-process gate.
- **The engine allocates nothing on the byte path.** `[measured 2026-08-30]` six paths at
  zero — idle, send, recv, frame, turn, busy — each proven by injection
  (`crates/engine/benches/alloc.rs`).
- **Non-negotiable 4 has no machine check, and the attempt to build one failed.** `dtruss` is
  refused by macOS SIP; reading undefined symbols out of the compiled rlib passes even with a
  `thread::sleep` added, because `Engine` and `serve` are generic and are never
  code-generated into the library. The script was deleted rather than shipped. Hand-check
  until `tools/w2w` runs on Linux — open item 15.
- **A bench reporting "1 allocation per 1000" was measuring a dropped connection.** The busy
  case replayed one Logon, which the session refuses the second time as a sequence number
  already used; from iteration three the engine held no connections and the count was the
  test double's queue doubling. Every case now asserts its own path is live.
  [reference/measured-costs.md](docs/reference/measured-costs.md).

- **The ring hop costs ~50x the inline call, and the number is published rather than
  assumed.** `[measured 2026-08-30]` inline **2.7 ns**, ring **128.0 ns** one way, **242.5 ns**
  round trip, on a 163-byte `NewOrderSingle`, Apple M5, macOS 25.6, unpinned.
  `crates/engine/benches/dispatch.rs` asserts 15 / 260 / 500 ns and was proven to assert by
  lowering a ceiling and watching it go red.
- **The dispatch chooses a thread, not a protocol.** The same message produces byte-identical
  output under `InlineDispatch` and `RingDispatch` — `crates/engine/tests/dispatch.rs`. And a
  reply for a connection that has hung up is dropped rather than delivered to whoever took its
  slot, because routing is by id and `swap_remove` reuses indices.
- **The ring is `AtomicU8`, not `unsafe`, and that was a decision with a price.**
  [ADR-0007](docs/decisions/ADR-0007-spsc-ring-without-unsafe.md): the plan authorised neither
  `unsafe` nor a dependency, so the copy is byte-at-a-time and costs ~0.8 ns per byte. The
  reversal is cheap and is written down.

- **A slow consumer ends with a reason, not in silence.** `DESIGN.md` D10 built:
  `Disconnect` (default), `Queue { max_bytes }`, `Block`.
  `crates/engine/tests/backpressure.rs`, six tests, and both guards proven by reversal —
  truncating instead of refusing turns three red, and removing the dead-socket check turns one
  red.
- **Two real bugs came out of writing those tests, not out of reading the code.** A socket that
  died with bytes still queued left the connection `Up` for ever, because "finished" meant
  *closing and the queue is empty*. And `Queue { max_bytes }` smaller than one Logout would
  have ended the session silently — the message that says why is now written into the whole
  `TX` buffer after the queue is discarded.
- **The tick moved to the front of `Connection::turn`, and the wire gate did not move.**
  `[measured 2026-08-30]` still **59 / 59**. `received_with` has no clock (D1), so a session
  that has never ticked judges `SendingTime` against zero and refuses the first message on
  every connection. It had been worked around in three places; the workarounds were deleted in
  the same commit as the fix.

- **The journal left the session, and D1's debt is paid.** `[measured 2026-08-30]`
  `fixbolt_session::journal::Journal` is a trait the caller supplies, like `Application`; the
  session holds no bytes it did not generate. Three D7 tiers as three types: `NoJournal`,
  `MemJournal`, `FileJournal` with `Durability::{Async, Fsync}`.
  [ADR-0008](docs/decisions/ADR-0008-journal-is-a-trait.md) records why a trait and not the
  emitted `Action::Store` D1 sketched — **a resend has to read, and an action cannot answer.**
- **The acceptance score depends on the journal, and the reversal proves it.** Making
  `MemJournal::put` keep nothing turns four of `tests/journal.rs`'s seven red **and** drops
  `--test score` below 59. Restored: 59 / 59, and 59 / 59 over a socket.

- **The wire gate was 39 / 59 on Linux, the cause was Nagle, and the first diagnosis of it was
  wrong.** `[measured 2026-08-30]` `cargo test -p fixbolt-engine --test wire` scored 39 / 59
  while `--test score` over the same corpus on the same machine scored 59 / 59. Walking the
  harness's `quiet` bound walked the score — 200 → 39, 2 000 → 43, 20 000 → 59 — and that was
  read as "a spin count is not a settle criterion". **It was Nagle on the harness's own client
  socket.** `2m_BodyLengthValueNotCorrect` sends a frame that produces no reply, so no
  piggybacked ACK; the peer's delayed ACK holds; four `I` lines coalesce into one 477-byte read
  and the framer discards all four. The longer timeouts were outwaiting the delayed ACK. The
  engine already sets `TCP_NODELAY` (`transport.rs:68`); the harness did not, which made the
  test rig the only Nagle-enabled peer. **Fixed with one line, and proven by a 2 × 2**: spin
  count and wall-clock bound both score 39 / 59 without `set_nodelay` and 59 / 59 with it.
  Removing that line from the finished fix returns exactly 39 / 59. **Item 17 closed**;
  full write-up, including what the wrong diagnosis cost, in
  [reference/measured-costs.md](docs/reference/measured-costs.md).
- **The wire gate is 59 / 59 on Linux, and its bounds are flat.** `[measured 2026-08-30]`
  59 / 59 at a 1 ms and a 20 ms quiet window; only the run time moves, 0.8 s against 14.5 s.
  The `settle` hook an earlier draft added to `fixbolt_conformance`'s public trait was
  **deleted rather than shipped**: the reversal meant to prove it left the gate at 59 / 59 with
  it disabled.
- **The toolchain is pinned and the lint that turned `main` red is fixed.** `[measured
  2026-08-30]` `clippy::byte_char_slices` reproduces on `1.98.0` — installable here, so it did
  not need CI to prove — and `rust-toolchain.toml` now pins 1.98.0, with an advisory
  `clippy-latest-stable` CI job that never blocks a merge. **Item 19 closed.**
- **The rest of the suite is green on Linux, on this toolchain.** `[measured 2026-08-30]`
  same box, `cargo 1.94.1`: `cargo fmt --check` clean, `cargo clippy --all-targets -- -D
  warnings` clean **here and red on the runner's newer clippy — see the next entry**,
  `scripts/check-lint-config.sh` green in both directions, `cargo test --all` **158 passed /
  1 failed** across 30 test binaries — the one failure being the wire gate above — and
  `cargo test --no-default-features` fails on that one and nothing else.
- **CI had been red on `main` for both of those, and nobody read it.** `[measured 2026-08-30]`
  GitHub Actions run `33291318638`, commit `9986890` — the tip this branch was cut from —
  fails two jobs. *Builds with nothing optional installed* fails on
  `the_fifty_nine_definitions_pass_through_a_real_socket`, which is the wire gate above: **the
  runner had been saying 39 / 59 within a minute of the engine merging**, while this page, the
  `README`, `DESIGN.md` §6 and `PRD.md` all said 59 / 59 on the strength of a laptop run.
  *fmt · clippy · test* fails on `clippy::byte_char_slices` at
  `crates/dict/tests/interop_quickfix_fields.rs:133` — a lint that does not exist in
  `clippy 0.1.94` here or `1.95.0` on the M5, and does in the runner's `1.98.0`. **CI installs
  whatever stable is current and there is no `rust-toolchain.toml`**, so `-D warnings` denies
  lints that had not been written when the code was. Both written up in
  [reference/measured-costs.md](docs/reference/measured-costs.md). **Open items 18 and 19.**
- **Item 7 closed: the corpus is pinned and verified.** `[measured 2026-08-30]`
  `scripts/fetch-quickfix-assets.sh` defaults to commit `386ce46e917a` instead of `master` and
  checks the three counts the documents quote — **59 definitions, 539 message lines, 244 `E`
  lines carrying `10=`** — failing at the fetch rather than three layers away. Proven by
  reversal: adding one `.def` gives `CORPUS MISMATCH: acceptance definitions is 60`, EXIT=1;
  removing it, EXIT=0. **The first reversal was worthless and it is worth saying why**:
  `QUICKFIX_REF=master` passed, because `master` currently *is* the pinned commit. And the pin
  buys nothing today — upstream has not drifted. Its value is entirely future; only the
  mechanism is proven.
- **Item 18 closed, and the plan was wrong about how.** The two dead rustdoc links were
  **external URLs**, which `check-links.py` skips by design, so walking `.rs` files would not
  have caught them. The rule that does, and needs no network: **a file in this repository must
  be linked by relative path, never by absolute URL.** It found **three**, not two. Scanning
  `.rs` first produced **13 false positives**, all rustdoc intra-doc links naming Rust items
  rather than files — the trap the plan had named in advance, caught by the guard it named.
  `CLAUDE.md` §9 now requires a green CI run named by id for the commit being closed, and §10
  gains two more failures no gate can see.
- **Item 15 closed: *the engine thread never sleeps in the kernel* has a machine check, and
  the check proves itself.** `[measured 2026-08-30]` `tools/w2w` exists and runs;
  `scripts/check-no-kernel-sleep.sh` traces it with `strace -f` and attributes syscalls to the
  engine thread **by tid** — the client blocks on purpose and would mask everything. The engine
  thread made **3111 `recvfrom`, 3111 `accept4`, 351 `sendto`, and zero** of `epoll_wait`,
  `poll`, `select`, `futex`, `nanosleep`, `sched_yield`. **The script then runs the binary again
  with `wait::Park` and fails if that does not trip it** — `RED ok — --park trips it: 1749
  sched_yield`. That second half is the point: this rule had two machine checks before and both
  were green with a `sleep` present. The zero is only accepted because the same run separately
  proves the thread did socket work.
- **`tools/w2w` measures an administrative round trip, not an application echo.**
  `TestRequest` out, `Heartbeat` back — the session owns `35=1`, so no application is involved
  and the number cannot be contaminated by the tool's own message building. An application echo
  comes with the half of its plan that needs a §9 machine.
- **`[measured 2026-08-30]` w2w on this box: min 14 967 ns, p50 29 745 ns, p99 67 943 ns**,
  5 000 samples, 4 vCPU container. **Not publishable and the binary says so itself on every
  run** — no `isolcpus`, no pinning, no frequency control, so it does not match `DESIGN.md` §9.
  **No row of §8 was changed on the strength of it.**
- **The blocker on item 10 was wrong, and knowing that is most of the value.** `[measured
  2026-08-30]` it had been recorded as needing the §9 machine of item 6. kTLS is a **kernel
  feature, not a latency property**, so it needs no such machine — but a Linux box is not
  enough either: it needs a kernel built with `CONFIG_TLS`. This one is not.
  `setsockopt(TCP_ULP, "tls")` on a real connected socket returns **`ENOENT` on both ends**,
  and the kernel config reads `# CONFIG_TLS is not set`. **The config line alone would not
  have been enough** — a config says what was compiled, not what a container may do; the
  syscall says both, which is why `scripts/check-ktls-available.sh` makes the call rather than
  grepping.
- **Items 8 and 9 closed: the DATA write path has invariants, and they are refusals.**
  `[measured 2026-08-30]` `TemplateBuilder::build` refuses a DATA field declared without its
  length field (`EncodeError::DataWithoutLength`); `encode_with` refuses the same inside a
  repeating group before a byte is written; and **the encoder computes the length from the
  data**, ignoring what the caller passed — `0x01` inside the value included. Six tests in
  `crates/codec/tests/data_encode.rs`, each rule proven by reversal with the injection
  confirmed present first.
- **The ordering defect was real and had shipped.** `[measured 2026-08-30]` fifteen of FIX
  4.4's sixteen DATA pairs have `length == data - 1`, so sorting body tags ascending put them
  right **by arithmetic accident**. `Signature(89)` takes `SignatureLength(93)` and was emitted
  **before** its length — unframable by any reader. Field order now places a DATA field one
  place behind its length field's tag, which fixes all sixteen without a special case. Written
  up in [reference/fix44-dictionary-traps.md](docs/reference/fix44-dictionary-traps.md).
- **In groups the order was already right, and that is not the same as tested.** `[measured
  2026-08-30]` **66 DATA members across the group tables, all 66 with the length declared
  immediately in front.** `group_roundtrip.rs` no longer skips them: it writes **508 DATA
  members, each with a separator inside its value**, and asserts that count is non-zero —
  a round-trip that covered no DATA member would look exactly like one that did.
- **`benches/alloc.rs` writes a DATA field at zero allocations**, and the case asserts its own
  path is live before the zero counts. Proven by injection: one `format!` in that loop reports
  **10 000**.
- **The ring holds 56.7 µs of slack, and ADR-0002 assumed milliseconds.** `[measured
  2026-08-30]` `crates/engine/benches/ring_full.rs`, Linux 6.18 x86_64: at the 65 536-byte
  capacity `benches/dispatch.rs` measures the hop at, a stalled application costs the engine
  **352 messages and 56.7 µs** before the ring starts refusing. ADR-0002 bought the ring so that
  *an application that stalls does not stall the session layer*, priced against a stall of
  "milliseconds" — **one millisecond overflows this ring about eighteen times over.** The ring
  as sized does not buy what it was bought for, which makes capacity part of the policy decision
  rather than a tuning detail. The bench asserts the ring **accepted** before it refused: one
  that rejected everything from the first message would print a plausible number.
- **A gate has been red on Linux the whole time and nothing ran it.** `[measured 2026-08-30]`
  `cargo bench -p fixbolt-engine --bench dispatch` fails here: **ring one way 332.5 ns against
  its 260 ns ceiling**, while inline is 5.4 ns against 15 and passes. The ceilings came from the
  M5 (2.7 ns and 128.0 ns) with ~2× headroom; a cross-thread hop on a shared 4 vCPU host is
  2.6× the M5's, and **nothing is regressing** — the gate is measuring the machine.
  **`cargo test --all` does not run a `harness = false` bench and no CI job runs `cargo bench`,
  so this has never been reported by anything.** Found by hand, while doing something else.
  It also means the commit before this one stated its gates without this one, because
  `CLAUDE.md` §7 names `alloc` and the Criterion suite together and only `alloc` was run.
  **Open item 20**; not fixed here, because every fix changes how a `DESIGN.md` §6 gate is
  measured.
- **The journal reads back, and it could not have before.** `[measured 2026-08-30]` the
  on-disk record was `seq(4) || message` with **no length**, so records could not be separated
  on read: the file was append-only by construction and item 16 could not be closed without
  changing it. Now `seq(4) || len(4) || bytes`. `FileJournal::open` reads the file before
  appending, `Journal::highest()` says what is held — **deliberately with no default
  implementation**, because a default `None` would let a journal that holds messages report
  that it holds none — and **a torn tail is dropped rather than half-read**. Four tests in
  `crates/engine/tests/recovery.rs`, each using `FileJournal` and dropping it between the write
  and the read: a `MemJournal` there would prove nothing, since the restart is the question.
- **A reversal reported PASS because it inserted nothing, and `grep` is what caught it.**
  Changing `max()` to `min()` in `highest()` silently failed to apply — `cargo fmt` had joined
  the line, so the replacement string did not match. The count came back **0** and that is the
  only reason it was not read as "this guard cannot fail". Re-injected properly it goes red
  (`left: Some(7), right: Some(8)`). This is `false-greens.md` §5's own case, hit while quoting
  it. **Confirm the injection is in the file before reading the result** — every time.
- **The corpus resets on every connect, and it cannot settle whether that is right.**
  `[measured 2026-08-30]` three files reconnect — `2i` (2 connects), `2k` (3), `2o` (2) — and
  **all seven expect `34=1` back**, with **no `141=Y` on any of those Logons**; one file in the
  corpus mentions `141=` at all. But FIX numbers a *session*, not a *connection*, and QuickFIX
  persists across a reconnect in a deployment while its harness starts each `iCONNECT` from a
  clean store. **The corpus and a real deployment want opposite behaviour, and `connect` cannot
  tell which it is in.** [ADR-0010](docs/decisions/ADR-0010-a-reconnect-is-not-a-restart.md)
  proposes separating the two and is `Proposed`.
- **Seven cases went back to `testing-skills`, and the queue now matches the rule.**
  [PR #2](https://github.com/tmthang86/testing-skills/pull/2), draft, on branch
  `claude/false-greens-from-a-protocol-engine`: three new numbered cases in `false-greens.md`
  (the knob that moved with the fix and was not the cause; the check nobody ran; fifteen out of
  sixteen), a fourth on instruments that cannot see what they were aimed at, two more ways a
  reversal inserts nothing added to its §5, and two foldings into its §2 and §11. **The first
  commit contributed four of the seven due and the second fixed that** — `CLAUDE.md` §11 says a
  case goes back when the plan that found it closes, and three came from closed plans.
  `[measured]` upstream's own checks on the branch: `validate-repo.mjs` 0 errors 0 warnings,
  `compare-design.mjs --self-test` 33/33, `check-theme-contract.mjs --self-test` all passed.
  **One case is correctly still held**: the `ktls` blocker case, because that plan is paused
  rather than closed.
- **The rest went back as two stacked pull requests, because a third would have collided with
  two that were already open.** `[2026-08-31]` the obvious move — one branch off `main` with
  everything on it — was written first and **rejected on push**: upstream already had
  `claude/false-greens-from-a-protocol-engine` at PR #2, the exact branch name, with §10–13 of
  `false-greens.md` already occupied. So:
  [**PR #6**](https://github.com/tmthang86/testing-skills/pull/6) is based on **PR #2's branch,
  not `main`**, and renumbers this plan's six cases to **§14–19** — the negative result that was
  negative for the wrong reason (and the gate whose own red halves both printed `RED ok`); the
  configuration under test that was never built (and the A/B harness that verified the result but
  not the selection); the number parsed from the label `p50`; the test that assembled what it
  checked (and the doc comment refuted by reversal); the identifier already given back; and green
  because the runtime was masking it. The checklist gains a **second half**, 22–29, for red
  results and for gates that contain their own red half.
  [**PR #7**](https://github.com/tmthang86/testing-skills/pull/7) is based on **PR #1's branch**
  and carries the protocol reference into `docs/drafts/`, **not** into the skill's `references/`.
  **That placement is the point.** PR #1 proposes `protocol-e2e-testing` as a *sibling* skill and
  states the design decision *"one skill owns one medium"*; the first draft here had added a sixth
  row to the `e2e-testing` router, which would have answered an open design question by accident.
  A draft in `docs/`, linked only from the roadmap item, lands the measurement and leaves the
  decision where it belongs. `[measured]` on both branches: `validate-repo.mjs` 0 errors 0
  warnings, `test:unit` all passed, TOC anchors 24/24 and section numbers `1..19` and `1..29` each
  exactly once. `test:integration` was **not run** — it wants a Playwright browser build the
  container does not have, and both diffs are markdown-only.
- **The markers stay until the pull requests merge.** `CLAUDE.md` §11 says
  `[to testing-skills]` is replaced by the pull-request link *when it lands*; #2, #6 and #7 are
  all open drafts, so `grep -rn '\[to testing-skills\]' docs/` is still the backlog and still
  reads honestly.
- **The contribution is evidence upstream could not previously have.** Its roadmap names its
  biggest gap as *nothing proven against a real system*, with everything measured on one Tauri
  app through a UI. These came from a system with **no UI at all** — no browser, no locators, no
  screenshots — and the same shapes appear, which is what makes them not browser problems.

## Not proven — claimed, researched, or simply not yet run

- **Every figure in [prior-art.md](docs/reference/prior-art.md) is someone else's claim**,
  including all of fix8's and Artio's. Nothing from those projects was run here.
- The **150 ns gates** in `DESIGN.md` §6 are anchored to one measurement on one macOS
  laptop. macOS gives no thread pinning and schedules across three core types — these rank
  designs against each other, they are **not an SLA**.
- **Every figure in `DESIGN.md` §8 (latency budget) is from the literature**, not measured.
  The `tools/w2w` harness replaces that table; nothing in it is evidence until then.
- The ring-buffer hop (200–500 ns) and the busy-poll saving (2–5 µs) are literature figures.
  `benches/dispatch.rs` and `tools/w2w` are what turn them into numbers.
- `MAX_FIELDS = 64` is a starting number. No real message population has been surveyed.
- **In-group order is agreed with one other implementation, not with a counterparty.**
  QuickFIX's generator reads the same `FIX44.xml`. Two programs agreeing on how to read one
  file is real evidence and is not the same as a venue accepting the bytes. Nothing here has
  been sent to a real FIX peer.
- **DATA fields inside a repeating group are untested** on both paths — open item 8.
- **None of the three heartbeat thresholds is visible to the corpus.** The acceptance harness
  can only tick a whole `HeartBtInt` at a time, so any test-request threshold in (1×, 2×] and any
  timeout in (2×, 3×] reproduces `6_SendTestRequest.def` exactly. The numbers 1.0, 1.2 and 2.4
  are QuickFIX's, and `crates/session/tests/heartbeat.rs` is the only thing holding them.
- **Nothing about a *second* gap is visible to the corpus.** Every file that opens one ends before
  opening another, and the deepest any of them holds is two messages. Closing a filled gap,
  replaying held messages in sequence order, and dropping one there is no room for are all held by
  `crates/session/tests/resend.rs` alone.
- **No application message has ever been replayed.** The inbound `ResendRequest` path answers with
  a gap fill because everything this session has sent so far is administrative. A `MessageStore`
  and a real replay are step 6 — see the plan's "Sửa 9".
- **Whether a Reject consumes the inbound sequence number is invisible to the corpus.** The
  *too high* branch does not exist yet, so a message running ahead is read as if it were in
  order and a sequence number that never advanced looks exactly like one that did. Held by
  `crates/session/tests/reject.rs`.
- **What each of the 23 field types accepts is invented, not captured.** The corpus supplies two
  cases — `38=+200.00` and `126=20040415`. The other 21 types are held by hand-written rows in
  `crates/dict/tests/field_types.rs`, and that is the weakest evidence in this crate.
- **32 of the 59 definitions still fail.** No Heartbeat, no TestRequest, no ResendRequest, no
  SequenceReset, no application echo, and no second-connection identity check.
- **`Input::Tick` is sent but never advances.** The runner seeds one fixed instant before every
  message. Nothing moves time forward yet, so `4a_NoDataSentDuringHeartBtInt.def` cannot pass —
  the advance rule is step 4 of the session plan.
- **The 120-second `SendingTime` skew is QuickFIX's documented default, not a measured one.**
  `1d_InvalidLogonBadSendingTime` is 2001 years out, so nothing in the corpus distinguishes 120
  seconds from any other bound.
- **`Role` is parameterised but only `Acceptor` is exercised.** `Initiator::SPEAKS_FIRST` is
  read and does nothing yet; ADR-0004's cost is not paid until an initiator has something to say.
- **Sequence numbers reset on every connect.** Persisting them across a reconnect is the
  journal's job and the journal belongs to `engine`. Nothing in the corpus requires persistence,
  so nothing here proves the reset is right for a real deployment.
- The ADRs are accepted on the strength of the reasoning in them, **not on measurement** — see the §8 caveat above.

## Open items

Every one of these is either inside a plan or has a stated reason for not being in one. All six
plans are **approved**; the first is in progress.

| Plan | Closes |
|---|---|
| ~~[gates-that-can-be-trusted](docs/plans/2026-08-30-gates-that-can-be-trusted.md)~~ | **CLOSED 2026-08-30** — 7, 17, 18, 19 |
| [w2w-and-linux-numbers](docs/plans/2026-08-30-w2w-and-linux-numbers.md) | **15 closed 2026-08-30**; 6, 11, 13 blocked on a §9 machine; **decides** 12 |
| ~~[ktls-spike](docs/plans/2026-08-30-ktls-spike.md)~~ | **CLOSED 2026-08-31** — 10 |
| ~~[data-fields](docs/plans/2026-08-30-data-fields.md)~~ | **CLOSED 2026-08-30** — 8, 9 |
| [session-recovery](docs/plans/2026-08-30-session-recovery.md) | 16 — **journal read-back done 2026-08-30; blocked on [ADR-0010](docs/decisions/ADR-0010-a-reconnect-is-not-a-restart.md)** |
| [ring-full-policy](docs/plans/2026-08-30-ring-full-policy.md) | 5 — **measured 2026-08-30; **ADR-0011 accepted 2026-08-30; steps 3–4 are now work, not a decision** |

**The two with no plan, and why.** **1** (the final name) is a decision for the owner, not a
piece of work — when it is made it is an ADR and a rename, and neither can be planned around
an undecided name. **14** (kernel bypass) is phase 3, is excluded by PRD §5 as it stands, and
needs a Solarflare/AMD X2-class NIC that nobody here has; planning it would be planning
against hardware that does not exist.

| # | Item | Blocks |
|---|---|---|
| 1 | ~~**Final name**~~ — **closed 2026-08-30: `fixbolt`.** The placeholder `nanofixengine` was not merely undecided, it was **a near-collision with the project this repository measures itself against**: `matthart1983/nanofix` is *"Ultra-low-latency FIX protocol engine in Rust"* (item 12; `CLAUDE.md` §2 rule 7's "276 unwraps"), and `LMAX-Exchange/nanofix` is a Java FIX test client with 40 stars in the same domain. `DESIGN.md` had said so since it was written and the rename kept being deferred. `fixbolt` was checked free on crates.io **and** GitHub — checking only the global registry is how `nanofix` itself nearly got re-adopted. Rejected, with reasons: `rustfix` (taken — the crate behind `cargo fix`, and literally a repair tool), `tesla-fix` (TSLA is a ticker; the engine would be named after an instrument it carries), `swiftfix` (SWIFT is the interbank network), `flashfix` ("flash crash"), `fixarc` (`Arc<T>`), `fleetfix` (37 fleet-repair apps), and the whole `<speed-word>fix` family, because *quick fix* is an English idiom for a shoddy repair — putting FIX **first** is what makes it read as the protocol. **Fully closed 2026-08-30**: the GitHub repository is renamed, `github.com/tmthang86/fixbolt`, verified by `git ls-remote` returning the merge commit through the new URL. `Cargo.toml`'s `repository` field had pointed at `.../fixbolt` since the rename commit and was **wrong until this happened**. GitHub redirects the old URL. The five surviving occurrences of `nanofixengine` in the tree are deliberate history — *"the old name was X"* — not leftovers | Nothing. Publishing to crates.io is unblocked |
| 5 | ~~**Ring-buffer policy when the application behind the ring falls behind**~~ — **CLOSED 2026-08-31.** [ADR-0011](docs/decisions/ADR-0011-a-full-ring-disconnects.md) is **implemented**: a full ring ends the connection with `Logout(58=slow application)` — a different text from D10's `slow consumer` on purpose, because here the counterparty is faultless — `ring::DEFAULT_CAPACITY` is **4 MiB**, and `Block` is not offered on this side. **The ADR's open question 2 is answered too**: the refusal reaches the outside in two places, the `58=` the counterparty reads and `Engine::refused_connections()` for whoever embeds the engine. The signal from dispatch to engine is a defaulted `Dispatch::take_refusal()`, carrying no id because the engine asks straight after one connection's turn — the session layer's `Application` trait could not be touched (non-negotiable 2). Proven by three reversals: engine ignoring the refusal, the Logout carrying D10's text, and `RingDispatch` never reporting; each red, then green. **Open questions 1 and 3 of that ADR stay open** — per-connection rings, and whether the slack is enough — and both need a real application nobody has yet. `[measured 2026-08-31]` **the slack itself was re-measured rather than extrapolated: 5.05–5.36 ms at 4 MiB, not the 3.6 ms four documents were carrying**, because the per-message cost goes 135 → ~230 ns once the buffer leaves cache and a ring that fills slower gives more time, not less. The decision is unaffected and its margin is bigger than it claimed — [measured-costs.md](docs/reference/measured-costs.md). | closed |
| 20 | ~~**The bench ceilings are tuned to a machine no gate runs on.**~~ — **CLOSED 2026-08-31 by [ADR-0016](docs/decisions/ADR-0016-per-machine-baselines-replace-absolute-targets.md)**, which implements this item's own conclusion: *a per-machine baseline is viable, keyed on the CPU model that `check-machine.sh` prints; a single absolute ceiling across the pool is not.* Ceilings are gone; `benches/baselines.tsv` holds one line per (CPU, case) with its margin, sample size, date and machine verdict, and an unknown CPU reads **NO BASELINE**, counted on its own summary row and fatal under `--strict`. Two of this item's findings were **re-measured and both held**: the unexplained second mode on `ring, one way` is still there at **+24%**, 2 runs in 21 with the old harness and 2 in 30 with the new — and `[measured 2026-08-31]` it appeared on the **quietest** runs of the set, not the loaded ones, which confirms from the other side that load is *sufficient* to produce the mode and not *necessary*. **What was wrong was the instrument, not this item**: dropping one parameter from `Suite::bench` moved `inline deliver + reply` 6.3 → **1.3 ns** with the timed loop byte-identical, and the three discrete clusters that case showed were the harness's, not the engine's — see [measured-costs.md](docs/reference/measured-costs.md). | closed |
| 22 | **`[2026-08-30]` decided by [ADR-0012](docs/decisions/ADR-0012-latency-first-and-one-session-per-polling-thread.md), `Accepted`, and **re-scoped to `hft` by [ADR-0013](docs/decisions/ADR-0013-two-modes-standard-and-hft.md)** — in `standard` mode the engine blocks instead of sweeping, so this whole term is replaced by an `epoll`-class wakeup and is **unmeasured**. The tension is resolved in favour of latency: one session per polling thread is the shape the design is optimised, budgeted and measured for; many-per-thread is supported as a labelled **`density`** mode carrying `N × 703 ns` instead of the latency figures; and **every published latency number names its `N`**. `PRD.md` §1, `DESIGN.md` §1 and §8, `README.md` and the new **[GUIDE.md](docs/GUIDE.md)** follow from it. `DESIGN.md` §8 priced busy-poll at **`~0`** and that row is now **703 ns × N**; its bottom line said `< 1 µs` for "everything this design controls" while measuring only user space, and now reads `< 1 µs + N × 703 ns`. **The trade D8 makes — `epoll`'s 2–5 µs wakeup for a 703 ns poll — wins at N=1 and loses by N=8**, a sentence the table did not contain until the poll was measured. What remains open is the measuring, not the deciding. The original finding: |
| 22 | **The engine is syscall-bound, and "many sessions on one core" costs 703 ns each.** `[measured 2026-08-30]` D8 makes an idle turn one non-blocking `read` per connection. On the §9 box that read costs **703 ns and is flat from N=1 to N=256**, so a turn is exactly `N × 703 ns` and a session's added latency is one whole turn. Of that, **353.8 ns is kernel entry and exit doing nothing** — `syscall(getpid)` measured beside it — against `parse NewOrderSingle` at **125.5 ns** and a vDSO `clock_gettime` at **22.9 ns**. *The syscall that discovers there is nothing to parse costs 5.6× the parse.* `DESIGN.md` §8 budgets the whole user-space path under 1 µs; **two sessions on one polling thread exceed that budget in polling alone**, and `PRD.md` targets *"many sessions on one core"*. Both can be true of different products, but **not of one polling thread**, and that choice has not been made. It also reorders the open items: item 12 defers SIMD for 20–40 ns, while removing a syscall is worth **703**. Levers in measured order — fewer sessions per thread (free); `mitigations=off` (**`[unproven]`**, full mitigations are on, `vmscape` alone does an IBPB on every syscall return, needs a reboot and is a security decision); `recvmmsg` or `io_uring` with `SQPOLL`; then item 14. Write-up: [reference/measured-costs.md](docs/reference/measured-costs.md) | `DESIGN.md` §8 budget; `PRD.md`'s deployment shape; the priority of items 11, 12, 14 |
| 21 | `[2026-08-31]` **half closed. Pinning exists; the refusals do not.** Steps 1–2 of [threads-and-affinity](docs/plans/2026-08-30-threads-and-affinity.md) are done: [ADR-0015](docs/decisions/ADR-0015-explicit-cores-pinned-from-inside-and-read-back.md) decided the shape, [ADR-0019](docs/decisions/ADR-0019-two-unsafe-blocks-and-an-error-the-enum-can-hold.md) corrected three things writing the code found, and `fixbolt_engine::affinity` pins a thread and **asks the kernel back** rather than trusting the call. `[measured 2026-08-31]` proven by a **second** reversal, not the first: removing `sched_setaffinity` made the tests red at the read-back guard, which said nothing about whether the residency assertion was worth having — so the read-back was removed too, and the thread was then observed on **cpu0, cpu4 and cpu5** in one run. An unpinned thread really does move. `[2026-08-31]` **step 3 landed too**: `Topology` and `ShardPlan::validate()` refuse a core that is absent, offline, duplicated, an SMT sibling of another in the plan, or — for shard cores — outside `isolcpus`, **before any thread is created**. Proven by reversing all three topology checks at once: exactly 5 of 18 tests went red, and the two *acceptance* tests stayed green, which is what shows the suite is not simply refusing everything. **Still open, and this is why the item is not closed**: nothing shards (step 4), and the journal writer and ring consumer are still unpinned — `ShardPlan` can say where they go and nothing puts them there (step 5). `DESIGN.md` D8 now says exactly this rather than claiming the whole sentence. **Two facts the machine changed while reading it**: `isolated` lists `6-7,14-15` while `online` is `0-7`, so a validator reading `isolated` alone would accept a core that cannot run anything; and §9 turns SMT off, so the SMT-sibling rule can never fire on a correctly tuned box — it fires on the one set up wrong, which is where the mistake gets made. The original finding: | `DESIGN.md` D8 and §8; every jitter claim; item 20 |
| 21 | **`DESIGN.md` D8 says "the engine thread is pinned to an isolated core". Nothing pins it.** `[measured 2026-08-30]` `grep` for `sched_setaffinity`, `affinity`, `core_affinity` or `libc` across `crates/` and `tools/` returns **nothing**: no dependency, no call, no test. §8's latency budget and §9's `isolcpus` row both assume a pinned engine thread, so the design's central jitter defence is **asserted in prose and absent from the code** — `CLAUDE.md` §4's "prose does not hold a constraint", on the one claim it would cost most. Either the engine pins and something proves it, or D8's sentence is wrong and must say so. Cheap to settle, and it is the mechanism the 324 ns mode most plausibly needs, since load amplifies that mode 5% → 92% and pinning is what isolates a thread from load. **NUMA is deliberately NOT part of this**: this machine reports **1 NUMA node**, cross-L3 placement was measured and had **no effect** (~259 ns in all three `taskset` arms), and topology-aware allocation would be designing against hardware nobody here has — the same reason item 14 keeps kernel bypass out. Revisit if the box ever becomes multi-socket | `DESIGN.md` D8 and §8; every jitter claim; item 20 |
| 6 | A Linux box for `tools/w2w`. The design's own §9 says a latency number from a macOS laptop is not a number. **`[measured 2026-08-30]` SATISFIED, later the same day than the reading below: the kernel command line took `isolcpus=6,7,14,15 nohz_full=6,7,14,15 rcu_nocbs=6,7,14,15 processor.max_cstate=1`, and after the five runtime rows the box reads `pass 10  fail 0  unknown 1` with `bench.sh --strict` running. See the `§9 satisfied` entry under Proven — it is the authority, and everything after this sentence in this row is the state *before* that reboot, kept because its A/B is still the evidence that the tuning is worth little.** What remains open here is not the machine but the **`tools/w2w` wire-to-wire figures themselves**, which nobody has run on it. **`[measured 2026-08-30]` the desktop exists, has a toolchain, and has been read: AMD Ryzen 7 3700X, 16 logical cores, Linux 7.0.0-30-generic, rustc 1.98.0 — `check-machine.sh` says `pass 1  fail 7  unknown 1`.** The seven are `isolcpus`/`nohz_full`, governor `powersave`, turbo on, C-states uncapped, SMT on, THP `madvise`, `net.core.busy_poll=0`. **The gap is configuration, not hardware.** `[measured 2026-08-30]` five of the seven were applied and the box now reads **`pass 6  fail 2`**; the two left — `isolcpus`/`nohz_full` and capped C-states — need a kernel command line and a reboot, so `--strict` still refuses and **nothing here is publishable yet**. Toggling is no longer a per-run password prompt: `/usr/local/sbin/fixbolt-machine on|off|tls|status`, root-owned, reachable through a `NOPASSWD` rule scoped to those five verbs, which is what made the same-machine A/B in [measured-costs.md](docs/reference/measured-costs.md) possible at all. **The A/B says the tuning is worth little**: every bench median moves under 2%. | Every gate in §6 that matters |
| 23 | **A gate can be green in CI and red on the machine doing the work.** `[measured 2026-08-31]` `scripts/check-lint-config.sh` built its throwaway crate in `mktemp -d`, where `rust-toolchain.toml` does not reach, and on a desktop with no `rustup default` it exited 1 saying *"the workspace lints do not deny: unwrap_used expect_used panic"* — while `cargo clippy` had not run at all. A **false red about the system under test**, and the same construction had a quieter twin: on any machine that *did* have a default, the gate was checking the workspace's lint config against a different clippy from the one the workspace pins, which `rust-toolchain.toml`'s own comment calls load-bearing. Fixed by copying `rust-toolchain.toml` into the scratch crate; proven by reversal — commenting out `unwrap_used = "deny"` names that one lint and exits 1, restoring it exits 0. **Found only because §9's checklist requires every gate to be run here and its output read.** Nothing in CI could have shown it: CI is the environment where it passes. Write-up: [reference/a-scratch-fixture-inherits-the-machine.md](docs/reference/a-scratch-fixture-inherits-the-machine.md), marked `[to testing-skills]`. **The other scripts were then audited and are clean**: three more use `mktemp -d`, but only for output files — `check-no-kernel-sleep.sh` and `check-standard-gives-the-core-back.sh` run a binary built in the tree, and `check-ktls-on-a-plain-socket.sh` runs `cargo build` inside `spikes/ktls`, where rustup still walks up to the repository's `rust-toolchain.toml`. One script was affected and it is fixed. **What stays open is the class, not an instance**: nothing prevents the next fixture from being written the same way | closed as an instance; kept as a shape to watch |
| 7 | **`scripts/fetch-quickfix-assets.sh` tracks mutable `master`.** Every acceptance number in the codec plan (539 lines, 247 with `9=`, 244 with `10=`, 8 tag-set patterns for `35=3`) can change silently upstream. Pin a commit and verify it | Reproducibility of every step-1 gate |
| 10 | ~~**Can `ktls-core` be driven from a plain non-blocking socket with no async runtime?**~~ — **CLOSED 2026-08-31: yes, with four conditions.** [ADR-0018](docs/decisions/ADR-0018-ktls-on-a-plain-socket-answers-adr-0005.md) supplements ADR-0005 rather than superseding it; `DESIGN.md` D11 keeps its shape and its kTLS row goes from reasoned to measured. **The question looked hard because ADR-0005 cited three crates as one.** `ktls` 6.0.2 (rustls org) really is `tokio-rustls`-specific and `ktls-stream` defaults to a `tokio` feature — but `ktls-core` 0.0.5, the crate the question names, depends on `bitfield-struct`, `libc`, `nix`, `zeroize`, has **no async feature at all**, and exposes only synchronous entry points generic over `AsFd`. `Context::handle_io_error` is exactly the shape a spin loop wants. `[measured 2026-08-31]` Ryzen 7 3700X / Linux 7.0.0-30-generic / `ktls-core` 0.0.5 / `rustls` 0.23.43 / TLS 1.3 AES-128-GCM: **15 assertions, `fail 0`**, and `strace -f` over 1000 round trips attributed by tid shows `recvfrom` 3033 + `sendto` 1000 and nothing else — against the red arm's `poll` 1000, which is what proves the green arm could fail. Spinning costs ~3.0 `recvfrom` per message where blocking costs 1.0; that is ADR-0013's bargain, unchanged by TLS. **The four conditions, each measured:** every read error goes to `ktls_core::Context` (`EIO` once per connection from session tickets); the transport **never** reads the socket outside the offload (hand-draining desynchronises the kernel permanently — `EBADMSG`, and `TlsDecryptError` moves); the handshake hands over with an empty buffer; `setup_ulp` needs `ESTABLISHED`. **Not answered, and not to be inferred:** any latency number (§8's TLS row stays empty on purpose), key update under kTLS, TLS 1.2, mutual TLS, the kernel/suite floor, and what asserts which mode is live — ADR-0005 open questions 2, 3, 4, 5, 6. **A false green of my own is written up with it**: the wire check asserted a record's *shape* and passed against a session-ticket record the test never sent. [reference/ktls-on-a-plain-socket.md](docs/reference/ktls-on-a-plain-socket.md), `spikes/ktls`, `scripts/check-ktls-on-a-plain-socket.sh`. | closed — the TLS plan is unblocked and unwritten |
| 11 | ~~**Serialise misses its gate: 93.8 ns against 60 ns**~~ — **CLOSED 2026-08-31 by [ADR-0016](docs/decisions/ADR-0016-per-machine-baselines-replace-absolute-targets.md).** The item asked how to reach 60 ns and the answer is that **60 ns was never a measurement of this engine** — `DESIGN.md` §4 D9 says it is how the fastest commercial engines are *reported* to perform. Three steps got here. The cause this item named — the linear slot scan — was measured at **~24% at most**, not the bulk; the fix it proposed was written, measured at **+5.2 ns** against a predicted −36, and **reverted**; and the split that does hold is **~31 ns of fixed cost before the first variable field** plus **~7 ns per field** in `put`, so a perfect scan still leaves **~116 ns**. §6 now gates on a per-machine baseline: `[measured 2026-08-31]` **239.1 ns** on the §9 desktop, median of 24 qualifying `bench.sh` runs, margin 1.10, and **`bench.sh --strict` exits 0** there — 0 of 12 cases over, 0 without a baseline. The ~116 ns floor survives as a **Stretch row that is explicitly not a gate**. | closed |
| 12 | **SIMD / SWAR for SOH scan and checksum — deliberately not done.** `matthart1983/nanofix` has NEON/SSE2 SOH scanning and still parsed 4–6× slower than this codec, because its 512-entry index blew L1 ([measured-costs.md](docs/reference/measured-costs.md)). Layout won; SIMD did not. Estimated gain here is 20–40 ns per message on a 10–20 µs floor — under 0.5%. **Do it only when `benches/parse.rs` on the Linux box shows parse on the critical path.** If done: 8-byte SWAR in `codec`, no `memchr` (zero-dependency rule), `core::arch` only behind a measurement. **Start with `scripts/bench.sh --strict`**: this is a same-machine A/B, so it needs the box to be quiet, not to be a particular box. | Nothing until open item 6 is answered |
| 13 | **Release profile is default.** No `lto = "fat"`, no `codegen-units = 1`, no PGO, no `#[cold]` on error paths. Cheap, but each is a number to be measured before and after, not a setting to be assumed. **Start with `scripts/bench.sh --strict`**, once before any profile change and once after — same machine, same settings, or the comparison means nothing. | The `engine` step; every §6 number published from Linux |
| 14 | `[measured 2026-08-30]` **and it is not a cure for session density, which was the reason most often given for wanting it.** Bypass removes the 703 ns syscall — the largest single term, and worth doing for that alone — leaving a sweep still linear in N, a cache hierarchy that costs **1.05 ns in L1 against 78.5 ns from RAM (75×)** on a `Connection` measured at **53.3 KiB** when `L1d` is 32 KiB, and head-of-line blocking of `(k-1) × ~465 ns` that nothing removes but fewer sessions. **What is unmeasured and decides the cache wall is how much of that 53.3 KiB a message touches** — the wall is at N≈9 if all of it and N≈128 if 4 KiB. Worth more than another guess at the 324 ns mode. [reference/measured-costs.md](docs/reference/measured-costs.md). The original entry: |
| 14 | **Kernel bypass path, if PRD §5 is ever reversed: Onload first, `ef_vi` second, DPDK never.** Onload runs the engine unchanged (`onload ./engine`, socket API, TCP in userspace) — D8 spin already fits it; the first measurement is `tools/w2w` twice on the same box, kernel vs `onload`, and that difference decides whether an `ef_vi` L0 is worth writing. `ef_vi`/TCPDirect is a second `impl Transport` behind a real feature flag (D5). DPDK ships no TCP stack — it means writing or embedding one (smoltcp, F-Stack), which is what fixbolt claims and does not do. Any bypass path is plaintext: it and D11 exclude each other. Needs a Solarflare/AMD X2-class NIC — none available | Phase 3, and open item 6 before it |
| 16 | ~~**The journal is written and never read back**~~ — **CLOSED 2026-08-31.** All six steps of [session-recovery](docs/plans/2026-08-30-session-recovery.md) are done. Steps 1–3 made the journal readable; step 4 is [ADR-0010](docs/decisions/ADR-0010-a-reconnect-is-not-a-restart.md) — `Session::resume`, and `connect` keeps the count for a session that persisted something; step 5 is **[ADR-0017](docs/decisions/ADR-0017-the-inbound-count-is-persisted-after-delivery.md)** — `Journal::mark_in`/`highest_in`, written **after** the application sees the message. `Durability::Fsync` is now a recovery mechanism rather than an audit trail, in both directions. **59/59 unchanged and no corpus file exempted**; forcing `connect` to never reset drops it to **56/59**, which proves the corpus exercises that branch. Six reversals across the two steps, and one of them found a worthless test of mine — see the plan's log. **Two things are NOT proven and are named rather than implied**: nothing has measured what the extra `sync_data` costs the inbound path, and no `.def` file restarts a process, so every test here is one this project invented. | closed |
| 18 | **A plan can close on a laptop's word while CI is red.** The engine plan closed and merged with its gates reported green from an Apple M5; the GitHub run on that same commit failed and was not read, and four documents carried the laptop's number for a day. Nothing in `CLAUDE.md` §9 requires the closing evidence to name a CI run | Every "gates green" claim in a merge commit |
