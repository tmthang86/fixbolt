# fixbolt — Engineering Rules

A FIX 4.4 engine in **Rust**, acceptor-first, positioned as **a FIX acceptor on kernel TCP
whose latency is a published, reproduced number** ([ADR-0077](docs/decisions/ADR-0077-acceptor-first-stays-and-fastest-is-said-only-beside-a-reproduced-pair.md)
decision 2). Not a port of QuickFIX ([ADR-0001](docs/decisions/ADR-0001-relationship-to-quickfix.md)).
The design is **[docs/DESIGN.md](docs/DESIGN.md)** — D1–D10 are the decisions, §8 is the
latency budget, §9 is the OS checklist. Where the work stands is **[STATUS.md](STATUS.md)**, never
this file — read it before picking up work, update it when a plan phase closes.

**This file holds rules only.** No status, no dated history, no measurements: those go to
`STATUS.md`, `docs/reference/` and the ADRs. A rule may carry one sentence of *why*; the story
behind it lives where this file points. **One rule, one place** — a rule restated in two places is
two rules that will disagree. Editing this file mid-session does not affect that session: say out
loud which rule you changed.

**This repository is meant to be open-sourced.** Treat every commit as already public. Nothing
confidential enters it — no exchange specifications, no captures, no counterparty configuration,
nothing from `shadow-exchange`. `.gitignore` is a safety net, not the control. **The control is
you, before `git add`.**

Section numbers are referenced from hundreds of documents, so they never shift. §11 was removed
on 2026-09-13 and its number is not reused.

---

## 1. Rule Zero: plan first, then build

**No code without an approved plan.** No exception for "this one is small".
Naming: `docs/plans/YYYY-MM-DD-<topic>.md`, following `docs/plans/_template.md`.

| Situation | Required action |
|---|---|
| New crate, codec change, session-layer change, dispatch or transport change, public API change | Write a plan, wait for approval |
| Typo, comment, doc link repair | No plan, but still update docs if behaviour changes |
| Plan turns out wrong mid-build | **Stop. Fix the plan. Get it re-approved.** Never silently diverge |
| Part of the plan is blocked | Finish everything else in full, then say plainly what was left out and why |
| Handed an approved plan to build | Coordinate it as §12 says |

Crates are added to the workspace **one at a time**, in the order of `DESIGN.md` §7, each behind
its own plan. The gate for a step exists before the step.

## 2. Non-negotiables — do not violate

Every change touching `codec`, `session`, `engine` or `transport` is checked against this list by
hand. Each names the decision it enforces.

1. **No heap allocation on the parse, serialise, session or dispatch hot path.** Proven by the
   counting allocator in each crate's `benches/alloc.rs`, never by reading the code. (D2, D9)
2. **The session layer is pure.** No socket, no clock, no allocation, no `format!`. Time arrives
   as `Input::Tick`. Errors are fieldless enums. (D1)
3. **The 59 QuickFIX acceptance definitions are the session layer's gate.** A session change that
   has not run them to 59/59 is not done. (D1, ADR-0001)
4. **Mode-scoped, and both halves are rules.** In `hft` mode the engine thread never sleeps in the
   kernel on the hot path — no `epoll_wait`, no futex, no blocking `read`. In `standard` mode (the
   default) the engine thread **must** block when idle. A `standard` engine that spins is as much a
   defect as an `hft` engine that sleeps. Any measurement, claim or gate that does not name its
   mode is incomplete. (D8, ADR-0012, ADR-0013)
5. **Field ordering comes from generated tables, never from a call site.** The acceptance
   comparator is positional. (D3)
6. **A feature flag gates the `mod` declaration itself**, and `build.rs` invokes no external
   toolchain unless that feature is on. CI builds `--no-default-features` on a machine with
   nothing optional installed. (D5)
7. **No `panic!`, `unwrap()` or `expect()` in a library crate.** Enforced by workspace clippy
   lints, not by discipline. A panicking index `a[i..j]` names none of the three and is held
   separately by `indexing_slicing = "deny"` and its debt ratchet. (D6)
8. **`unsafe` needs a plan and a comment naming what proves it sound** — a Miri run, a fuzz
   target, a test. `unsafe_code = "warn"` is on at the workspace level.
9. **No QuickFIX source is copied.** Its XML and `.def` files are data and a test oracle, fetched
   into gitignored `vendor/`. If that ever changes, `NOTICE` becomes mandatory. (ADR-0001)
10. **No performance number without the committed benchmark that produced it, the machine it ran
    on, and the §9 settings in force.** A number missing any of the three is someone else's claim
    and is labelled as such.

### Machine checks

A rule not in this table is a hand-check on every relevant PR — say explicitly that you walked
the list. Each script's header states what it cannot see; read it before trusting a green.

| Rule | Check | Note |
|---|---|---|
| 1 | `crates/*/benches/alloc.rs`, run by the `bench` CI job via `scripts/bench.sh`, each case asserting its own path is live; `tools/w2w` counts allocations on both threads over its timed window and asserts zero | `cargo test` does not run a `harness = false` bench — only the job does |
| 3 | `crates/conformance`, in process and over a socket; behind `fix50sp2`, the FIXT corpus in the `gates` job, with `scripts/check-feature-gated-tests-ran.sh` proving the named tests ran | `cargo test --all` compiles none of the `fix50sp2` tests; the expected FIXT score and its one asserted divergence are in `docs/CONFORMANCE.md` §9 |
| 3 | `scripts/check-socket-corpus-under-contention.sh 2 8` in the `gates` job: the prebuilt `wire` and `wire_fixt` binaries run `rounds × copies` times, `copies` at a time, exiting non-zero on any red ([ADR-0087](docs/decisions/ADR-0087-a-socket-harness-settles-on-counted-records-and-the-clock-waits-for-the-engine.md) decision 5) | a **count of runs**, never a latency number, and non-negotiable 10 is about the second kind; `copies` is pressure on *that* machine, so a green on a two-vCPU runner bounds nothing about the §9 desk; a test binary that runs no test exits 0, and the harness's own `lifeline hit:` line is the only signal of a slow green. **Its reversal is owed on macOS** — the race is unreachable on Linux ([ADR-0091](docs/decisions/ADR-0091-the-socket-harness-race-is-the-loopback-stacks-not-the-schedulers-and-its-reversal-runs-on-macos.md)) |
| 4 | `scripts/check-no-kernel-sleep.sh` (`hft`), `scripts/check-standard-gives-the-core-back.sh` (`standard`), `the_dial_loop_sleeps_rather_than_spins_while_the_handshake_waits` (initiator dial loop), `scripts/check-no-kernel-sleep-by-ctxt.sh` (`hft` voluntary-context-switch count, tracer-free, [ADR-0072](docs/decisions/ADR-0072-a-tracer-free-check-that-the-hft-engine-thread-never-sleeps.md)) | each script must also be tripped by the wrong mode; `hft` under TLS is unchecked |
| 6 | `no-default-features` CI job **and** `scripts/check-no-optional-deps.sh`, per crate | cargo unifies features across one invocation — [feature-flags-unify-across-a-workspace](docs/reference/feature-flags-unify-across-a-workspace.md) |
| 7 | `scripts/check-lint-config.sh` (lints deny, proven by reversal); `scripts/check-indexing-debt.sh` (ratchet: the count may only go down); `scripts/check-no-crate-root-allow.sh` (no crate-root `allow`/`expect`, no `warn` lowering a denied lint); `scripts/check-scratch-fixtures.sh` (a scratch crate outside the tree gets the pinned toolchain) | known gaps of the scratch-fixture gate are open by decision, ADR-0061 |

## 3. Read before you touch the code

**[docs/DESIGN.md](docs/DESIGN.md)** is mandatory before anything touching the codec, session,
engine or dispatch — by section, not end to end. §4 D1–D10 say *what* was decided; the ADRs say
*why* and *at what cost*; **[docs/reference/measured-costs.md](docs/reference/measured-costs.md)**
holds the measurements, and
**[docs/reference/quickfix-acceptance-def-format.md](docs/reference/quickfix-acceptance-def-format.md)**
holds the first trap already paid for.

## 4. Documentation set and sync rules

Docs-as-code: Markdown, in this repo, changed **in the same commit** as the code it describes.
**A stale document is worse than no document.**

| File / directory | Answers |
|---|---|
| `docs/PRD.md` | what the product must do, in which phase, and how far it is from QuickFIX |
| `docs/GUIDE.md` | how to embed this engine without losing latency or messages — every constraint the type system cannot enforce |
| `docs/GETTING-STARTED.md` | 3-step quickstart using the `fixbolt` library crate |
| `docs/TUTORIAL.md` | step-by-step tutorial building a working acceptor around tested examples |
| `docs/INTRODUCTION.md` | FIX 4.4 concepts, why the acceptor role is hard, prior art |
| `docs/CONFIGURATION.md` | every setting: `settings.rs` keys, `Limits`, const generics, timeouts |
| `docs/SESSION-BEHAVIOUR.md` | session boundary behaviour mapped to `.def`s and tests |
| `docs/CONFORMANCE.md` | published conformance results with commands, machines, CI run ids |
| `docs/best-practices-standard.md` / `docs/best-practices-hft.md` | operational recommendations per mode |
| `docs/hft-playbook.md` | tuning procedure: hardware, BIOS, kernel, NIC, app, acceptance |
| `docs/DESIGN.md` | how the system is built, and the latency budget |
| `docs/internals/` | one page per crate: which file holds what, the order to read them in, the test guarding each |
| `docs/reference/` | protocol facts, prior art, measured costs, traps |
| `docs/decisions/` | ADRs: who decided what, why, at what cost |
| `docs/plans/` | what is about to be built |
| `STATUS.md` | where the work stands, open items, what is not proven |

| When you change… | You must update |
|---|---|
| Move work between phases, or change what a phase must deliver | `PRD.md` §2, and the ADR that moved it |
| Add / remove / rename a crate | `DESIGN.md` §3 + `README.md` layout + `Cargo.toml` members + its `docs/internals/` page |
| The public API of any crate | `DESIGN.md`, the crate's rustdoc, `CHANGELOG.md` |
| A constraint a user must honour and the compiler cannot check | `GUIDE.md` |
| A user-visible constant, default, or config key | `docs/CONFIGURATION.md` |
| Session boundary behaviour (reset, resend, gap fill, reject codes, `DropReason`) | `docs/SESSION-BEHAVIOUR.md`, naming the `.def` or test guarding it |
| A conformance number or gate result | `docs/CONFORMANCE.md`, naming command, machine, CI run id |
| An operational recommendation | `docs/best-practices-<mode>.md`, naming the mode |
| Hardware / BIOS / kernel / NIC tuning | `docs/hft-playbook.md`; an OS row updates `DESIGN.md` §9 first |
| Codec, session, dispatch, transport or backpressure behaviour | `DESIGN.md` §4, and re-walk §2 |
| A gate's target, or how it is measured | `DESIGN.md` §6, and the benchmark that asserts it, same commit |
| Anything that moves a row of the latency budget | `DESIGN.md` §8, with the measurement |
| A protocol trap, a wrong assumption, a measured surprise | `docs/reference/` ← **highest priority** |
| A dependency, a technique, a reversed decision | new ADR in `docs/decisions/` |
| Prove something listed as unproven | strike the bullet in `STATUS.md` *Not proven*, same commit |

No row is machine-checked; walk it row by row before a plan is closed.

**If it cost you, write it down.** An hour lost to a trap goes into `docs/reference/` or an ADR
immediately, and **every recorded trap gets a regression test**.

**Prose does not hold a constraint.** A comment asserting runtime behaviour names the thing that
proves it — a test, a benchmark, a lint.

## 5. ADRs

Every **expensive, hard-to-reverse or contested** decision gets one. Numbered sequentially, never
reused. `Proposed` → `Accepted` → (`Superseded by ADR-NNNN` | `Deprecated`).

- **Never edit an accepted ADR's substance.** Changed your mind? New ADR, supersede the old. A
  `Proposed` ADR may be revised in place, with the revision recorded in the text (ADR-0002 shows
  the shape).
- The most important section is **Consequences**, good and bad. Only-upsides is useless.

## 6. Code standards

**Rust**
- `cargo fmt` and `cargo clippy --all-targets -- -D warnings` clean before commit.
- Errors are typed: fieldless on a hot path, `thiserror` elsewhere. Never `Box<dyn Error>` in a
  public API.
- **Public API takes borrowed views into the caller's buffer.** `MessageView` is 24 bytes and
  `Copy`; an owned decoded struct on the hot path is a design regression.
- **`FieldIndex<const N>`** — the caller picks `N`. Aliases for common sizes; no hidden constant.
- Per-connection state is cache-line aligned, hot fields first. Buffers are pre-faulted at startup.
- `#![no_std]` for `codec` is a **goal, not yet a rule** — notice when you reach for `std`.
- Logging via `tracing` behind a feature flag. **The engine never logs on the hot path.**

**Dependencies**
- `codec` has **zero** runtime dependencies. Every other crate justifies each dependency in its
  plan. A dependency that pulls in an async runtime needs an ADR.

**Language**
- File names, identifiers, paths and code are always English.
- *Describing the system → English*: code, comments, `README.md`, `DESIGN.md`, `reference/`, ADRs,
  `CHANGELOG.md`, `STATUS.md`, commit messages, this file.
- *A plan, addressed to a person → Vietnamese, plain language*: every `docs/plans/` file. Jargon
  the reader must decode is a defect in a plan even when it is correct.
- Replies to the owner are in Vietnamese; identifiers, commands and file names stay English.

## 7. Testing

- **TDD for pure logic**: field parsing, checksum, body length, repeating groups, sequence numbers,
  the session state machine, template patching, timestamp caching.
- **The acceptance definitions are the primary gate**, run against the pure session machine.
- **Real captures over invented messages.** A counterparty capture is never committed here.
- **Never claim green without running it**, and **read the output, not the exit status** — in a
  pipeline the exit status is the last command's.
- **A guard is proven by reversal**: break it, see it red on the assertion you meant, restore it,
  see it green. Write the expected FAIL sentence down before running the reversal. A set of red
  reversals proves only what was tried.
- **Benchmarks assert their bound.** A target that lives in a comment is a wish.
- `vendor/` must be fetched (`scripts/fetch-quickfix-assets.sh`) before `cargo test --all`
  compiles, and `scripts/fetch-sbe-assets.sh` before `cargo clippy --all-targets` does.

| When | Run |
|---|---|
| Every step, every commit | `cargo test --all`, and `cargo test --no-default-features` |
| Any session-layer change | The 59 acceptance definitions, and the FIXT corpus with `--features fix50sp2` |
| Any hot-path change | The Criterion suite **and** `benches/alloc.rs` |
| Any dispatch, transport, or engine-thread change | `benches/dispatch.rs`, then `tools/w2w` on Linux |
| Any change to the wait strategy, readiness, or mode split | **Both modes** — proven in one is proven in neither (ADR-0013) |
| Closing a plan, before merging `main` | All of the above, with the §9 settings recorded |

Widening scope means **naming more cases**, never "run everything because it feels risky".

## 8. Branches, commits and releases

- Conventional Commits. One commit = one coherent change **including its documentation**; the body
  records what was measured (machine, OS settings, command) and what was *not* proven.
- **Never implement on `main`** — one branch per plan, merged only when its exit criteria are met.
  **Commit and push at every step that ends green.**
- Gates must be green **for that commit**, not merely for the branch tip.
- **CI runs on `pull_request` and on `push` to `main` only**, so a branch with no pull request has
  no CI. **Open the pull request as a draft at the first commit of a branch.**
- A push cancels the in-progress run for the same branch: wait for the closing commit's run to
  finish before pushing a handoff commit.
- `vendor/` is gitignored. **Never commit its contents** — that pulls QuickFIX's attribution clause
  into this repository.

## 9. Definition of Done

Done only when **all** hold. Any unchecked box → report it as **not done**, and say which.

- [ ] Built to the approved plan (or the plan was revised and re-approved)
- [ ] `cargo fmt`, `cargo clippy -D warnings` clean; `--no-default-features` builds
- [ ] New logic has tests, and the tests §7 requires were run and are green
- [ ] The §2 list was walked, and the §4 sync table was walked row by row
- [ ] An ADR exists if an architectural decision was made
- [ ] Every performance claim names its benchmark, its machine, and its §9 settings
- [ ] **Hot-path changes were measured on Linux**, not only on the development laptop
- [ ] **A green CI run is named, by id, for the commit being closed.** A laptop says the gates pass
      for you; only CI says they pass for the commit

## 10. Evidence, not promises

Every unit of work owes this evidence:

- **The failing test first**, shown **red against the unwritten code**, output quoted.
- **The gates quoted, not summarised.**
- **Existing tests stay green unmodified.** A fixture edited so new work can pass is the failure
  mode to watch for.
- **Name every trap the work can hit before starting**, each with the test that guards it.

**Failures no gate can see — check by hand, every time:**

- An allocation, a `format!`, or a `String` on a hot path or an error path.
- A blocking call on the engine thread — `epoll_wait` or a futex on an `hft` hot path; a mutex, or a `read` without `O_NONBLOCK`, in either mode.
- A timestamp formatted from scratch per message instead of patched from the cache.
- A `mod` behind a feature in `Cargo.toml` but not behind `#[cfg]` in `lib.rs`.
- A feature combination no gate builds (e.g. `--no-default-features --features <one>`).
- A number quoted from the laptop as though it were from the Linux box.
- A plan closed, or a branch merged, while CI was red on that commit.
- A cause accepted because a knob moved with it. A score that responds to a timeout says something
  is waited on and **nothing about what** — isolate one variable before naming a cause.
- Elapsed time inferred from your own activity instead of read from a clock.
- Docs not updated in the same commit as the code, per §4.

**A check proves nothing until something reads it.** Any green result that was *inferred* rather
than *observed* is not a result. **Review of a diff catches almost nothing**; bugs are caught by
running something and reading the output.

## 12. Who does what: one model per role

This repository is built with Claude Code. **The main session is the manager and never
implements.** Everything else is a subagent with one role, one model, and a brief.

| Role | Runs as | Model | Owns | Does not |
|---|---|---|---|---|
| **Manager** | main session | Opus | the branch (§8), splitting the approved plan into steps, choosing the model per step, **verifying every finding** (below), running the gates (§7), the delivery log, the pull request, every word the owner reads | write code, write the design, or change the plan |
| **Architect** | subagent, background | Fable | `DESIGN.md`, ADRs, `docs/reference/`, the plan (§1) — **written to disk**. **Internet research before any decision** — when designing, and again whenever a problem is hard or the way to solve it is not already clear: prior art, the spec, what sibling engines measured, how others solved it; the plan's *Những gì đã biết chắc* and every ADR cite what was found or say the search found nothing | touch `crates/`; decide from memory |
| **Senior reviewer** | subagent, fresh context, **escalation only** | Fable | a finding the senior developer could not close: open after one fix round, disputed between reviewer and author, or needing the spec, the design and the code held at once. Its verdict on that finding is final | write the fix — it goes back to the senior developer with the verdict; review a whole PR |
| **Senior developer** | subagent, fresh context | Opus | reviewing a step against the plan and gates, fixing verified findings, any step touching `codec`, `session`, `engine` or `transport` | re-design — a design problem goes to the architect through the manager |
| **Developer** | subagent | Sonnet | one step with named files and named tests | choose a design, or touch a file the brief did not name |
| **Runner** | subagent | Haiku | a mechanical task with one right answer: run and quote, grep, fetch `vendor/`, repair doc links | anything that needs judgement |

**Routing reads the step, not its label**, and routes *up* on any one signal: more than one module
or an invariant spanning modules; a spec the developer would have to interpret; a wrong answer that
costs more than a re-run; reasoning the brief cannot spell out. The architect is never a worker.

### Delegation

- **The brief is the spec.** Goal and why, exact files to touch and not touch, the §2 items in
  play, the gate command, what to quote back. A step that cannot be briefed that precisely goes
  back to the architect.
- **Inline what exists nowhere else; point at what is on disk by section or line range**, never at
  a whole file (`DESIGN.md §4 D9`, `ADR-0041 Consequences`, `crates/engine/src/x.rs:120-180`). A
  Haiku brief is fully self-contained.

  ```text
  Role: developer (sonnet). Step 3 of docs/plans/2026-09-xx-<topic>.md, table Chia việc.
  Why: <one sentence>.
  Read first, exactly here: DESIGN.md §4 D9; ADR-0041 Consequences; crates/engine/src/x.rs:120-180.
  Touch: crates/engine/src/y.rs, crates/engine/tests/y.rs. Do not touch: lib.rs, crates/session/.
  §2 items: 1 (benches/alloc.rs case y reads 0), 7 (no unwrap).
  Done when: `y_does_z` green; `cargo test -p fixbolt-engine y_` and clippy -D warnings clean.
  Report: diff per file; both commands' output verbatim; anything ambiguous — stop and say so.
  Do not commit.
  ```
- **One file, one writer at a time.** Parallel developers get disjoint files or a worktree each.
- **A subagent's green is a claim** (§10). The manager re-runs the gate that closes a step on the
  commit it closes, and commits; developers do not commit.
- **A surprise is reported and written down.** A subagent that hit a surprise reports it; the
  manager writes it into `docs/reference/` in the same commit (§4).
- **A reviewer is a different lens, not a second copy**: a fresh context given the plan, not the
  manager's reasoning. One senior review per step that touches §2, one per pull request otherwise.
- **Review escalates too.** A finding the Opus reviewer cannot close — open after one fix round,
  disputed, or needing spec + design + code held together — goes to the Fable reviewer, fresh
  context, briefed with the finding, both sides' evidence and the plan row. One Fable review per
  finding, never per PR; *confirmed* → senior developer, *refuted* → evidence in the PR, *design*
  → architect.
- **Escalate, do not re-brief.** A developer that reports ambiguity or exceeds its brief goes one
  tier up. The same model is never briefed a third time on one step.
- **The owner sees only what the manager writes**, in Vietnamese, with evidence — never "the agent
  said it passed".

### Verifying a finding before acting on it

**A review finding is a claim, not a defect, until the manager has verified it.** A review can be
wrong in either direction — including reading a documented, spec-correct behaviour as a bug. For
every finding, before it is routed anywhere:

1. **Reproduce it.** Run the command, test or probe the finding describes and read the output. A
   finding that cannot be reproduced is not acted on.
2. **Read the code and its documentation where the behaviour lives** — the rustdoc, comments,
   tests and ADRs that may already say the behaviour is deliberate.
3. **When the finding rests on a protocol, spec, library or tool claim, research it on the
   internet** — the FIX specification, the crate or tool's documentation, what other engines do.
   Where a document and the code disagree, neither wins by default; the specification decides.
4. **Classify and record it** with the evidence: *confirmed* (route to fix), *refuted* (not fixed;
   the evidence goes in the PR and, if it cost time, `docs/reference/`), or *design* (goes to the
   architect).

### Running an approved plan

**Once a plan is approved, the manager runs it to delivery without stopping to ask.**

1. **Build every step**, routed per the table, re-running the gate that closes each step and
   committing each step that ends green (§8).
2. **Then a senior review**, fresh context, given the plan and the gates.
3. **Verify each finding** as above. A confirmed finding goes back to the senior developer on the
   same branch, gate re-run afterwards — not to the owner as a question, and the manager does not
   fix it. A design finding goes to the architect: stop, fix the plan, get it re-approved.
4. **No confirmed findings left, all green — merge**, and in the same pass update `STATUS.md` and
   every document §4 names, citing the CI run id for the closing commit (§9).

**What stops the run**, and nothing else: a gate that will not go green, a plan that turns out
wrong, a step needing the §9 machine while another session measures on it, or anything §8 reserves
for the owner. The owner is told what happened, not asked whether to continue.

### Sessions and handoff

- **A session is one pull request; the handoff is written, not remembered.** The manager's memory
  is `STATUS.md`, the plan's *Nhật ký giao hàng*, and the CI run id for the commit closed.
- **At the start, read `STATUS.md` by section**: the newest *Start here*, *Where the work is*,
  *Open items*, then the delivery log of the plan in flight. Never end to end.
- **At the end, write the handoff as the next manager's brief** — what is in flight, branch and
  commit, gate command, CI run id, what is not proven — in *Start here* and the delivery log, in
  the same commit as the work.
- **A step lives inside one session.** When a plan spans pull requests, the boundary is a row of
  the plan's *Chia việc* table.
- **Two sessions, one working tree:** ask which session owns the tree before staging or switching
  branches; a new branch gets a `git worktree`. Run repo-wide scripts (e.g. `check-links.py`) from
  a checkout with no other worktrees nested under it. `DESIGN.md` §9 figures come from a machine
  nothing else is loading.

No line of §12 is machine-checked; walk it by hand like §4's table.
