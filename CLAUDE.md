# fixbolt — Engineering Rules

A FIX 4.4 engine in **Rust**, acceptor-first, positioned as **the fastest acceptor that can
run on kernel TCP**. Not a port of QuickFIX ([ADR-0001](docs/decisions/ADR-0001-relationship-to-quickfix.md)).
The design is **[docs/DESIGN.md](docs/DESIGN.md)** — D1–D10 are the decisions, §8 is the
latency budget, §9 is the OS checklist. Where the work stands: **[STATUS.md](STATUS.md)** —
read it before picking up work, update it when a plan phase closes.

> **This repository is meant to be open-sourced.** Private today; treat every commit as
> already public. Nothing confidential enters it — no exchange specifications, no captures,
> no counterparty configuration, nothing from `shadow-exchange`. `.gitignore` is a safety
> net, not the control. **The control is you, before `git add`.**
>
> **The name is `fixbolt`, decided 2026-08-30.** It replaced the placeholder
> `nanofixengine`, which sat one word from `matthart1983/nanofix` — the reference project
> this repository measures itself against. Verified free on crates.io and GitHub before
> adoption, on **both**: a free global registry says nothing about who already owns the
> name in the reader's mind. Nothing is published yet, so nothing is locked in.
>
> **Editing this file mid-session does not affect that session.** Say out loud which rule
> you changed. **One rule, one place** — a rule restated in two places is two rules that
> will disagree.

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

Crates are added to the workspace **one at a time**, in the order of `DESIGN.md` §7, each
behind its own plan. The gate for a step exists before the step: the `.def` runner before
the session layer, the wire-to-wire harness before the library.

## 2. Non-negotiables — do not violate

Every change touching `codec`, `session`, `engine` or `transport` is checked against this
list by hand. Each names the decision it enforces.

1. **No heap allocation on the parse, serialise, session or dispatch hot path.** Proven by
   the counting allocator in `benches/alloc.rs`, never by reading the code. (D2, D9)
2. **The session layer is pure.** No socket, no clock, no allocation, no `format!`. Time
   arrives as `Input::Tick`. Errors are fieldless enums. (D1)
3. **The 59 QuickFIX acceptance definitions are the session layer's gate.** A session change
   that has not run them to 59/59 is not done. (D1, ADR-0001)
4. **Mode-scoped, and both halves are rules.** `[amended 2026-08-30, ADR-0013]`
   **In `hft` mode the engine thread never sleeps in the kernel on the hot path** — no
   `epoll_wait`, no futex, no blocking `read`; a blocking call on that thread is a bug, not a
   style choice. **In `standard` mode the engine thread MUST block when idle** — an engine
   that spins by default is unusable on shared hardware, and `standard` is the default.
   **The second half is not the weaker half**: a `standard` engine that spins is as much a
   defect as an `hft` engine that sleeps, and each has its own machine check. Any measurement,
   claim or gate that does not name which mode it is about is incomplete. (D8, ADR-0012,
   ADR-0013)
5. **Field ordering comes from generated tables, never from a call site.** The acceptance
   comparator is positional; a hand-ordered message is a latent conformance failure. (D3)
6. **A feature flag gates the `mod` declaration itself**, and `build.rs` invokes no external
   toolchain unless that feature is on. CI builds `--no-default-features` on a machine with
   nothing optional installed. (D5)
7. **No `panic!`, `unwrap()` or `expect()` in a library crate.** Enforced by workspace clippy
   lints, not by discipline — the reference project has 276 and discipline did not hold. (D6)
8. **`unsafe` needs a plan and a comment naming what proves it sound** — a Miri run, a fuzz
   target, a test. `unsafe_code = "warn"` is on at the workspace level.
9. **No QuickFIX source is copied.** Its XML and `.def` files are data and a test oracle,
   fetched into gitignored `vendor/`. If that ever changes, `NOTICE` becomes mandatory.
   (ADR-0001)
10. **No performance number without the committed benchmark that produced it, the machine
    it ran on, and the §9 settings in force.** A number missing any of the three is
    someone else's claim and is labelled as such.

**Machine-checked today:** 7 — `scripts/check-lint-config.sh`, run by CI, proves by reversal
that the workspace lints actually deny `unwrap`/`expect`/`panic`. 6 — the
`no-default-features` CI job **plus `scripts/check-no-optional-deps.sh`, and the second is not
a nicety**: `[measured 2026-08-30]` the CI job alone was green about a build that never
happened. `cargo test --all --no-default-features` still compiled `libc`, because `tools/w2w`
depends on `fixbolt-engine` with defaults and cargo unifies features across one invocation —
the flag under test was switched back on by a sibling crate. It was noticed by a **test count**,
not by the gate, and a module with no tests of its own would have hidden it entirely. The
script asks per crate, which is the only scope where the flag means what it reads as
(`docs/reference/feature-flags-unify-across-a-workspace.md`). 1 — `benches/alloc.rs`, each case asserting its own path is
live, run by the `bench` CI job through `scripts/bench.sh`; **`[measured 2026-08-30]` this
entry was false until that job existed** — `cargo test --all` does not run a `harness = false`
bench target and nothing else invoked `cargo bench`, so the list named a check nothing ran.
3 — the conformance runner, in process and over a socket. 4 — **both halves, since
2026-08-30.** `scripts/check-no-kernel-sleep.sh` traces `tools/w2w` on Linux and attributes
syscalls to the engine thread by tid; **it runs the binary a second time in `standard` mode and
requires that run to trip the check**, because two earlier attempts at this rule reported success
with a `sleep` present. `scripts/check-standard-gives-the-core-back.sh` is the other half:
engine-thread CPU over a wall-clock window, the thread's scheduler state sampled to prove it is
sleeping rather than dead, the mode read back from the binary rather than assumed, and the
round-trip p50 against the poll timeout — **four assertions, because CPU near zero is passable by
three different broken engines**. `[measured 2026-08-30]` an engine that ignores readiness and
waits out its timeout reads 0% CPU, is found sleeping 20 times out of 20, and has a p50 of
99 046 599 ns; only the fourth assertion sees it. It requires `hft` **and** `yield` to trip it,
which is also the first thing in this repository to *demonstrate* rather than assert that
`wait::Yield` fails both gates. **The rest are hand-checks** on every relevant PR until a lint or test
exists — say explicitly that you walked the list.

## 3. Read before you touch the code

**[docs/DESIGN.md](docs/DESIGN.md)** is mandatory before anything touching the codec,
session, engine or dispatch. §4 D1–D10 say *what* was decided; the ADRs say *why* and *at
what cost*; **[docs/reference/measured-costs.md](docs/reference/measured-costs.md)** holds
the measurements that justified them, and
**[docs/reference/quickfix-acceptance-def-format.md](docs/reference/quickfix-acceptance-def-format.md)**
holds the one trap already paid for.

## 4. Documentation set and sync rules

Docs-as-code: Markdown, in this repo, changed **in the same commit** as the code it
describes. **A stale document is worse than no document.**

| Directory | Answers |
|---|---|
| `docs/PRD.md` | what the product must do, in which phase, and how far it is from QuickFIX |
| `docs/GUIDE.md` | **how to embed this engine without losing latency or messages** — written for a user of the framework, not a builder of it. Every constraint it names is one the type system cannot enforce |
| `docs/DESIGN.md` | how the system is built, and the latency budget it is built against |
| `docs/reference/` | what this thing is — protocol facts, prior art, measured costs, traps |
| `docs/decisions/` | who decided what, why, at what cost (ADRs) |
| `docs/plans/` | what is about to be built |

| When you change… | You must update |
|---|---|
| Move work between phases, or change what a phase must deliver | `PRD.md` §2, and the ADR that moved it |
| Add / remove / rename a crate | `DESIGN.md` §3 + `README.md` layout + `Cargo.toml` members |
| The public API of any crate | `DESIGN.md`, the crate's rustdoc, `CHANGELOG.md` |
| A constraint a user of the engine must honour and the compiler cannot check | `GUIDE.md` |
| Codec, session, dispatch, transport or backpressure behaviour | `DESIGN.md` §4, and re-walk §2 above |
| A gate's target, or how it is measured | `DESIGN.md` §6 — and the benchmark that asserts it, in the same commit |
| Anything that moves a row of the latency budget | `DESIGN.md` §8, with the measurement that moved it |
| Discover a protocol trap, a wrong assumption, or a measured surprise | `docs/reference/` ← **highest priority** |
| Pick a dependency, change technique, reverse a decision | New ADR in `docs/decisions/` |
| The surprise was about **testing**, not about FIX | the same `docs/reference/` entry, marked `[to testing-skills]` — §11 |
| **Prove something this repository listed as unproven** | **`STATUS.md`'s *Not proven* section — strike the bullet in the same commit** |

**No row here is machine-checked.** Every one is walked by hand before a plan is closed.

`[added 2026-09-02]` **the last row exists because the list of things not yet proven is the one
document nothing pointed at.** Every closing plan walked this table and struck its own *open
item*; no row sent anybody to *Not proven*, so bullets stayed there after they stopped being
true — **eight of them were false when the section was read on 2026-09-01, and an eleventh on
2026-09-02**. A section whose whole job is honesty rots in exactly one direction: it keeps
claiming the project is *less* proven than it is, which reads as modesty and is simply wrong.
`STATUS.md` open item 27.

**"If it cost you, write it down."** An hour lost to a protocol trap or a wrong assumption
goes into `docs/reference/` or an ADR immediately. **Every recorded trap gets a regression
test.** The field-ordering trap already has its test named; the next trap gets one too.

**Prose does not hold a constraint.** A comment asserting runtime behaviour must **name the
thing that proves it** — a test name, a benchmark, a lint.

## 5. ADRs

Every **expensive, hard-to-reverse or contested** decision gets one. Numbered sequentially,
never reused. `Proposed` → `Accepted` → (`Superseded by ADR-NNNN` | `Deprecated`).

- **Never edit an accepted ADR's substance.** Changed your mind? New ADR, supersede the old.
  A `Proposed` ADR may be revised in place, **with the revision recorded in the text** — see
  ADR-0002 for the shape.
- The most important section is **Consequences**, good and bad. Only-upsides is useless.

## 6. Code standards

**Rust**
- `cargo fmt` and `cargo clippy --all-targets -- -D warnings` clean before commit.
- Errors are typed, fieldless where they sit on a hot path, `thiserror` elsewhere. Never
  `Box<dyn Error>` in a public API.
- **Public API takes borrowed views into the caller's buffer.** `MessageView` is 24 bytes
  and `Copy`; an owned decoded struct on the hot path is a design regression.
- **`FieldIndex<const N>`** — the caller picks `N`. Aliases for the common sizes; no hidden
  constant.
- Per-connection state is cache-line aligned, hot fields first. Buffers are pre-faulted at
  startup (`pool.rs` in the reference project shows how).
- `#![no_std]` compatibility for `codec` is a **goal, not yet a rule** — notice when you
  reach for `std`.
- Logging via `tracing` behind a feature flag. **The engine never logs on the hot path.**

**Dependencies**
- `codec` has **zero** runtime dependencies. Every other crate justifies each dependency in
  its plan. A dependency that pulls in an async runtime needs an ADR.

**Document language**
- **File names, headings' identifiers, paths and code are always English.** No exception.
- **The prose inside follows the reader.** *Describing the system → English*: code,
  comments, `README.md`, `DESIGN.md`, `reference/`, ADRs, `CHANGELOG.md`, `STATUS.md`,
  commit messages — this ships as a public library and its readers are worldwide.
  *A plan, addressed to a person → Vietnamese, plain language*: every `docs/plans/` file.
  **Jargon the reader must decode is a defect in a plan even when it is correct.**
- **Replies to the owner in this repo are in Vietnamese.** Identifiers, commands and file
  names stay in English.

## 7. Testing

- **TDD for pure logic**: field parsing, checksum, body length, repeating groups, sequence
  numbers, the session state machine, template patching, timestamp caching.
- **The acceptance definitions are the primary gate**, run as unit tests against the pure
  session machine — no socket. They exist before the session layer does.
- **Real captures over invented messages.** A hand-written packet proves the parser handles
  a packet nobody sends. The `.def` files are real; a FIX capture from a counterparty
  (a UAT gateway capture, once obtained — never committed here) is better.
- **Never claim green without running it**, and **read the output, not the exit status.**
- **A guard is proven by reversal**: break it, see it red, restore it, see it green. Confirm
  the reversal changed something, and that it failed on the assertion you meant to prove.
- **Benchmarks assert their bound.** A target that lives in a comment is a wish — the
  reference project missed its own commented target by 7× and nothing noticed.

| When | Run |
|---|---|
| Every step, every commit | `cargo test --all`, and `cargo test --no-default-features` |
| Any session-layer change | The 59 acceptance definitions |
| Any hot-path change | The Criterion suite **and** `benches/alloc.rs` |
| Any dispatch, transport, or engine-thread change | `benches/dispatch.rs`, then `tools/w2w` on Linux |
| Any change to the wait strategy, the transport's readiness, or the mode split | **Both modes.** A change proven in one mode and not the other is proven in neither — ADR-0013 |
| Closing a plan, before merging `main` | All of the above, with the §9 settings recorded |

Widening scope means **naming more cases**, never "run everything because it feels risky".

## 8. Branches, commits and releases

- Conventional Commits. One commit = one coherent change, **including its documentation**;
  the body records what was measured — machine, OS settings, command — and says explicitly
  what was *not* proven.
- **Never implement on `main`** — one branch per plan. Merge only when its exit criteria are
  met. **Commit and push at every step that ends green.**
- Gates must be green **for that commit**, not merely for the branch tip.
- `vendor/` is gitignored and fetched by `scripts/fetch-quickfix-assets.sh`. **Never commit
  its contents** — that pulls QuickFIX's attribution clause into this repository.

## 9. Definition of Done

Done only when **all** hold. Any unchecked box → report it as **not done**, and say which.

- [ ] Built to the approved plan (or the plan was revised and re-approved)
- [ ] `cargo fmt`, `cargo clippy -D warnings` clean; `--no-default-features` builds
- [ ] New logic has tests, and the tests §7 requires for this change were run and are green
- [ ] The §2 list was walked, and the §4 sync table was walked row by row
- [ ] An ADR exists if an architectural decision was made
- [ ] Every performance claim names its benchmark, its machine, and its §9 settings
- [ ] **Hot-path changes were measured on Linux**, not only on the development laptop
- [ ] **A green CI run is named, by id, for the commit being closed.** `[measured 2026-08-30]`
      the `engine` plan closed and merged with its gates reported green from an Apple M5 —
      truthfully — while GitHub Actions failed on the same commit within the minute and nobody
      read it. Four documents carried the laptop's number for a day. A laptop says the gates
      pass *for you*; only CI says they pass *for the commit*

## 10. Evidence, not promises

Nothing here is delegated to a second reviewer. Because nobody else will look at it, every
unit of work owes this evidence:

- **The failing test first**, shown **red against the unwritten code**, output quoted.
- **The gates quoted, not summarised.** "It passed when I ran it" is not evidence.
- **Existing tests stay green unmodified.** A fixture edited so new work can pass is the
  failure mode to watch for — and it is your own hand that would edit it.
- **Name every trap the work can hit before starting**, each with the test that guards it.

**Failures no gate can see — check by hand, every time:**

- An allocation, a `format!`, or a `String` that crept onto a hot path or an error path.
- A blocking call — `epoll_wait`, a mutex, a `read` without `O_NONBLOCK` — on the engine
  thread.
- A timestamp formatted from scratch per message instead of patched from the cache.
- A `mod` behind a feature in `Cargo.toml` but not behind `#[cfg]` in `lib.rs`.
- A number quoted from the laptop as though it were from the Linux box.
- A plan closed, or a branch merged, while CI was red on that commit and nobody looked. §9's
  last box exists because this already happened.
- A cause accepted because a knob moved with it. A score that responds to a timeout says
  something is being waited on and **nothing about what** — `[measured 2026-08-30]` a wire
  gate whose score walked 39 → 43 → 59 with its own timeout was failing on Nagle, and the
  wrong answer reached five documents before a one-variable experiment refuted it.
- Docs not updated in the same commit as the code, per §4.

**The trap that outlives every process change: a check proves nothing until something reads
it.** Any green result that was *inferred* rather than *observed* is not a result.

**Review of a diff catches almost nothing.** Bugs get caught by running something and
reading the output. Do not use review as the primary safety net.


## 11. Contributing back to `testing-skills`

**[`tmthang86/testing-skills`](https://github.com/tmthang86/testing-skills)** is the owner's
other project: two Claude Code skills — `e2e-testing` and `design-conformance-testing` — plus
the tooling that keeps them honest. **Its own roadmap names its biggest gap: nothing in it has
been proven against a real system.** Everything measured in it so far came from one Tauri
desktop app, through a UI.

**This repository is the other half of that evidence, and it is the half that has no UI at
all.** A FIX acceptor is tested end-to-end by driving real bytes through a real socket and
reading what comes back — the same loop, with a wire protocol where the skills have a screen.
That makes fixbolt the place where the *protocol* side of e2e testing gets found out,
and what is found here is owed back.

### What goes back

Only material that this repository **measured**. A case is contributed when the command was
run here, the output was read, and the numbers are the observed ones — never from reasoning
about what would probably happen. That is §10 applied to somebody else's repository.

| Found here | Goes to |
|---|---|
| A check that passed, or failed, for a reason other than the thing under test | `references/false-greens.md` |
| A gate whose result depends on the machine, the timeout, or the load | `references/false-greens.md` |
| A reversal that turned out to be a no-op, or that passed and exposed a hole in the test | `references/false-greens.md` |
| How an e2e loop is driven against a **protocol** rather than a UI — settling, framing, a corpus as oracle, an injected clock as the only test double | the protocol reference the roadmap does not yet have |
| A tool that claimed to prove something and could not — `dtruss` under SIP, `nm -u` over generic code | `references/false-greens.md` |

**Strip it to the testing lesson.** The upstream repository is **public today**, and this one
is written as though it already were. A case goes back as *the shape, the measurement, the
fix* — legible to somebody who has never heard of FIX. **No message bytes, no counterparty
anything, no exchange specification, nothing from `shadow-exchange`, no `vendor/` content.**
If a case cannot be told without one of those, it does not go back; it stays in
`docs/reference/` here.

### When

**A case is written into `docs/reference/` here first** — §4's row, unchanged, and it is what
makes the case real. **The queue is that write-up itself**: a contributable case carries the
literal marker **`[to testing-skills]`** on its own line, so `grep -rn '\[to testing-skills\]'
docs/` is the whole backlog and there is no second list to go stale. The marker is replaced by
the pull-request link when it lands.

**The upstream pull request is opened when this project is implemented** — not when a plan
closes. `[decided 2026-09-01]` the earlier rule was one pull request per plan, and it was
changed for a reason worth keeping: **a case is only worth publishing once it has survived the
work that came after it.** Two of the cases already written here were revised by a later plan
that hit the same shape again, and a per-plan pull request would have published the first
version of both.

So the marker accumulates and the backlog is allowed to grow. `grep -rn '\[to testing-skills\]'
docs/` is still the whole queue, and the generalised paragraph in each local write-up is still
the draft of the upstream one — what moves is *when* they go, not what goes or how it is
written. **A case that turns out to be a misunderstanding gets corrected here, for free, by
never having left.**

### What does not happen

- **Nothing here depends on `testing-skills` to build, test or run.** No crate links it, no
  script fetches it, no gate needs it installed. It is read by the person or the agent doing
  the work, and the workspace must stay buildable by somebody who has never heard of it.
- **The skills are not a substitute for this file.** Where the two disagree about this
  repository, `CLAUDE.md` wins — §2's ten non-negotiables in particular are not up for
  revision by an installed skill.
- **Nothing flows the other way as a rule.** A useful idea from upstream is adopted here on
  its merits, in a plan, like any other technique — not because it arrived from the sibling
  project.
