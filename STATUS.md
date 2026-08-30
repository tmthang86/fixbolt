# Current state

One screen. A pointer, not a store. Detail lives in the ADRs and the plan files.
**A stale status page is worse than none.**

Last updated: **2026-08-30**. Re-verified on Linux the same day — see the wire-gate entry under **Proven** and open item 17. Later that day the whole suite ran for the first time on **the owner's own Linux desktop** (AMD Ryzen 7 3700X, Linux 7.0.0-30), which also **unblocked open item 10** and exposed two defects in the scripts that were supposed to be telling us so.

## Where the work is

**Next, and each needs its own plan before any code (Rule Zero):**

**Six plans were written and approved on 2026-08-30**, with the owner's standing permission to
revise a plan mid-flight when reality disagrees with it — each revision recorded in that plan's
delivery log. In dependency order:

1. ~~**gates-that-can-be-trusted**~~ — **closed 2026-08-30.** Items 7, 17, 18 and 19 are gone,
   CI is green, and the whole suite passes on Linux for the first time. Everything below was
   waiting on it, because every plan closes by quoting a gate.
2. **[w2w-and-linux-numbers](docs/plans/2026-08-30-w2w-and-linux-numbers.md)** — **half done.**
   `tools/w2w` runs and **item 15 is closed**. Items 6, 11, 13 and the decision on 12 are
   **blocked on a machine matching `DESIGN.md` §9**, and the plan stops there rather than
   lowering the bar to close.
3. **[ktls-spike](docs/plans/2026-08-30-ktls-spike.md)** — item 10. **Unpaused 2026-08-30.**
   It needed Linux rather than a §9 machine, which was right; it then needed a kernel built
   with `CONFIG_TLS`, which was also right — and the machine that has one was already on the
   desk. `[measured 2026-08-30]` the owner's desktop **accepts `setsockopt(TCP_ULP, "tls")`**.
   What kept it shut for a day was `scripts/check-ktls-available.sh` reporting `CONFIG_TLS=m`
   and *"it was built without CONFIG_TLS"* in the same run — now fixed and guarded, see
   [reference/ktls-on-a-plain-socket.md](docs/reference/ktls-on-a-plain-socket.md). **Steps 2–5
   are still to do**, and nothing about `ktls-core` or ADR-0005 is concluded yet.
4. ~~**data-fields**~~ — **closed 2026-08-30**, items 8 and 9.
5. **[session-recovery](docs/plans/2026-08-30-session-recovery.md)** — item 16. **Steps 1–3 done
   2026-08-30**: the journal reads back. Steps 4–5 wait on
   [ADR-0010](docs/decisions/ADR-0010-a-reconnect-is-not-a-restart.md), `Proposed`.
6. **[ring-full-policy](docs/plans/2026-08-30-ring-full-policy.md)** — item 5. **Steps 1–2 done
   2026-08-30**; steps 3–4 wait on [ADR-0011](docs/decisions/ADR-0011-a-full-ring-disconnects.md),
   which is `Proposed` and needs the owner's signature.

Still unplanned, and deliberately: **`library`** (§7 step 8) and **steps 3–4 of the paused
initiator plan**, whose gate is interop against `libquickfix` rather than the mirrored corpus
(ADR-0004, ADR-0006).

| | |
|---|---|
| Branch | **`main`.** PR #1 (`claude/project-status-irgurb`) merged 2026-08-30 as `76d6989`, no-ff. `[measured 2026-08-30]` **CI green on the merge commit itself**, run `33307963879`, all seven jobs — the first green `main` has had; the run before it, `9986890`, failed. In flight: **`plan/ktls-unblocked`** |
| Milestone | **M3 — the engine, closed, with one gate now known to be machine-dependent.** `[measured 2026-08-30]` the same 59 definitions pass **through a real socket** on the M5: `cargo test -p fixbolt-engine --test wire` → **59 / 59**. **On Linux the same command scores 39 / 59** — the harness's settle criterion is a spin count, not the engine. Open item 17; the in-process gate is 59 / 59 on both machines. `codec`, `dict`, `conformance` and `session` are closed behind it. What remains of `DESIGN.md` §7: step 7 `tools/w2w`, step 8 `library` |
| Scope | **[PRD.md](docs/PRD.md)** — phase 1 = FIX 4.4 tag=value both sides; phase 2 = SBE / FAST / FIXML + FIX 5.0. **TLS has ADR-0005 (Accepted) but no plan — blocked on open item 10** |
| Plan in flight | **[ring-full-policy](docs/plans/2026-08-30-ring-full-policy.md)** — measured, blocked on ADR-0011. `gates-that-can-be-trusted` **closed** (7, 17, 18, 19); `w2w-and-linux-numbers` **half A done** (15), half B needs the desktop tuned to §9; `ktls-spike` **unpaused** (10) — the desktop has `CONFIG_TLS`. Three plans approved and not started: `data-fields`, `session-recovery`, `ring-full-policy` |
| Last closed | **[2026-08-30-engine.md](docs/plans/2026-08-30-engine.md)** — closed 2026-08-30. **All six steps done.** `DESIGN.md` §7 step 6, taken before step 5 by decision. The gate that matters — the same 59 definitions **through a real socket** — went green at step 3 and did not move afterwards. Two ADRs came out of it: [ADR-0007](docs/decisions/ADR-0007-spsc-ring-without-unsafe.md) and [ADR-0008](docs/decisions/ADR-0008-journal-is-a-trait.md) |
| Paused | **[2026-08-29-session-initiator.md](docs/plans/2026-08-29-session-initiator.md)** — steps 1–2 done and merged 2026-08-30; steps 3–4 not started. Paused because the mirrored gate measures less than the plan assumed — see the two measurements below |
| Last closed | **[2026-08-28-session-layer.md](docs/plans/2026-08-28-session-layer.md)** — closed 2026-08-29. **All six steps done: 59 / 59.** Steps 1, 3, 4, 5 and 6b hit their prediction; step 2 missed it low (18 predicted) and step 6a missed it high (52 predicted), both for reasons written down in the plan. Eleven revisions recorded there |
| Last closed | **[2026-08-28-dict-validation.md](docs/plans/2026-08-28-dict-validation.md)** — closed 2026-08-28. Four validation tables, agreed with QuickFIX's own generated C++ on 912/912 tag numbers, 12 524/12 524 message-tag pairs and 1 708/1 708 enum values |
| Last closed | **[2026-08-28-conformance-runner.md](docs/plans/2026-08-28-conformance-runner.md)** — closed 2026-08-28. The 59 definitions run in process; a replaying fake scores 59 / 59, which is what makes the real score mean something |
| Last closed | **[2026-08-27-repeating-groups.md](docs/plans/2026-08-27-repeating-groups.md)** — closed 2026-08-28. Groups read and written, nested to depth 4; field order agreed with QuickFIX's own generated C++ on 730/730 groups |
| Last closed | **[2026-08-27-codec-dict.md](docs/plans/2026-08-27-codec-dict.md)** — closed and merged 2026-08-28. 54 tests, 0 allocations, 304M fuzz executions |
| Last closed | Design reviewed against the HFT latency budget and revised: positioning fixed to "fastest acceptor on kernel TCP", ADR-0002 default reversed (inline dispatch, ring optional), D8 busy-poll, D9 template encoder, D10 send backpressure, §8 latency budget, §9 OS checklist, wire-to-wire gate added |

## Proven — the command was run and its output read

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
| [ktls-spike](docs/plans/2026-08-30-ktls-spike.md) | 10 — **paused 2026-08-30**, blocked on a kernel with `CONFIG_TLS` |
| ~~[data-fields](docs/plans/2026-08-30-data-fields.md)~~ | **CLOSED 2026-08-30** — 8, 9 |
| [session-recovery](docs/plans/2026-08-30-session-recovery.md) | 16 — **journal read-back done 2026-08-30; blocked on [ADR-0010](docs/decisions/ADR-0010-a-reconnect-is-not-a-restart.md)** |
| [ring-full-policy](docs/plans/2026-08-30-ring-full-policy.md) | 5 — **measured 2026-08-30; blocked on [ADR-0011](docs/decisions/ADR-0011-a-full-ring-disconnects.md) being accepted** |

**The two with no plan, and why.** **1** (the final name) is a decision for the owner, not a
piece of work — when it is made it is an ADR and a rename, and neither can be planned around
an undecided name. **14** (kernel bypass) is phase 3, is excluded by PRD §5 as it stands, and
needs a Solarflare/AMD X2-class NIC that nobody here has; planning it would be planning
against hardware that does not exist.

| # | Item | Blocks |
|---|---|---|
| 1 | ~~**Final name**~~ — **closed 2026-08-30: `fixbolt`.** The placeholder `nanofixengine` was not merely undecided, it was **a near-collision with the project this repository measures itself against**: `matthart1983/nanofix` is *"Ultra-low-latency FIX protocol engine in Rust"* (item 12; `CLAUDE.md` §2 rule 7's "276 unwraps"), and `LMAX-Exchange/nanofix` is a Java FIX test client with 40 stars in the same domain. `DESIGN.md` had said so since it was written and the rename kept being deferred. `fixbolt` was checked free on crates.io **and** GitHub — checking only the global registry is how `nanofix` itself nearly got re-adopted. Rejected, with reasons: `rustfix` (taken — the crate behind `cargo fix`, and literally a repair tool), `tesla-fix` (TSLA is a ticker; the engine would be named after an instrument it carries), `swiftfix` (SWIFT is the interbank network), `flashfix` ("flash crash"), `fixarc` (`Arc<T>`), `fleetfix` (37 fleet-repair apps), and the whole `<speed-word>fix` family, because *quick fix* is an English idiom for a shoddy repair — putting FIX **first** is what makes it read as the protocol. **Fully closed 2026-08-30**: the GitHub repository is renamed, `github.com/tmthang86/fixbolt`, verified by `git ls-remote` returning the merge commit through the new URL. `Cargo.toml`'s `repository` field had pointed at `.../fixbolt` since the rename commit and was **wrong until this happened**. GitHub redirects the old URL. The five surviving occurrences of `nanofixengine` in the tree are deliberate history — *"the old name was X"* — not leftovers | Nothing. Publishing to crates.io is unblocked |
| 5 | **Ring-buffer policy when the application behind the ring falls behind: block, drop, or disconnect?** Not `DESIGN.md` D10, which answered the socket side. `[measured 2026-08-30]` the ring holds **352 messages / 56.7 µs** at its current 64 KiB, against the "milliseconds" ADR-0002 assumed — so capacity is part of the decision. **[ADR-0011](docs/decisions/ADR-0011-a-full-ring-disconnects.md) proposes disconnect + a 4 MiB default and needs signing** | ADR-0002; steps 3–4 of the ring plan |
| 20 | **The bench ceilings are tuned to a machine no gate runs on.** The half of this item that said *nothing runs them* is closed: `scripts/bench.sh` and the `bench` CI job run all eight targets on every push, and `benches/alloc.rs` — non-negotiable 1's machine check — now runs automatically for the first time. What is left is the ceilings themselves. `[measured 2026-08-30]` five runs of each of the twelve timing cases on a shared 4 vCPU Linux container: run-to-run spread is **5–232%**, three cases flip colour between runs (`encode ExecutionReport` 2/5, `walk 4 levels` 3/5, `encode 1 group` 4/5) and **not one case is over its ceiling in all five**. So there is no regression here to fix — the ceilings are simply inside this machine's noise. Then the same commit ran on the CI runner — AMD EPYC 7763, **2 cores**, run 33304774414 — and put **six of the twelve over**, with `ring, one way` at **328.3 ns**, within 1.3% of the 332.5 ns that originally named this item. So the spread is both kinds at once: 5–232% run to run on one machine, and up to **1.7× between two shared machines**, with the ceiling sitting between them. `[measured 2026-08-30]` **The CI runner pool is two CPU generations**, and once each sample carries its CPU the five runs are two tight distributions, not one noisy one: EPYC 7763 gives `ring, one way` 327.2–331.1 (**1.2%**), EPYC 9V74 gives 270.7–272.9 (**0.8%**), and they differ **21%** — against **3%** on the single-threaded cases, because the ring crosses cores and Zen generations differ in inter-core latency. So **a per-machine baseline is viable**, keyed on the CPU model that `scripts/check-machine.sh` now prints with every figure; a single absolute ceiling across the pool is not. `[measured 2026-08-30]` **a third machine, and it straddles the ceiling from the other side**: on the owner's Ryzen 7 3700X — **untuned, governor `powersave`, SMT on** — `ring, one way` is **260.9 ns** against a 260 ns ceiling, and `ring, round trip` **502.7 ns** against 500. So the three machines read 260.9 / 270.7–272.9 / 327.2–331.1, and **the ceiling now sits 0.3% below the fastest of them** — that is a ceiling no machine passes, which is a ceiling that has stopped saying anything. Note the Ryzen is the *untuned* number; tuning it to §9 can only move it down. `[measured 2026-08-30]` **and then the same machine was measured in both states, 15 full `bench.sh` runs each — the first same-machine A/B in this whole thread.** It splits the cases in two. **Three are over their ceiling on 15 of 15 runs in BOTH states** — `walk 4 levels` 347.6 vs 300, `encode 1 group` 104.7 vs 75, `encode ExecutionReport` 241.4 vs 190 — real gaps that no machine state explains, and the only rows a §6 gate can honestly fail on today. **The two ring cases are coin flips**: 5/15 and 9/15 over, 7/15 and 8/15, with the ceiling sitting *at* the median. **`[measured 2026-08-30]` and then that sample was repeated, and corrected the entry that had just been written.** Running `dispatch` alone, sample 1 gave tuned `over 260: 1/15` against untuned `14/15`, and this file said a 0.5% median shift flipped unchanged code from red to green. Sample 2, same command same binary, gave tuned **9/15**. **The medians reproduce to 1.4 ns; the pass rate does not reproduce at all.** Pooled 30 runs per state: median 259.7 vs 261.8 (**−0.8%**), over the ceiling **10/30 (33%) tuned vs 28/30 (93%) untuned**. Tuning helps and does not rescue it — **neither state gives a stable verdict**, which is the actual finding. **`[measured 2026-08-30]` and then the cause was found, and it is not on this checklist at all.** The second mode near **324 ns** was blamed in turn on Zen-2 L3 placement, on SMT-off, and on thermal throttling; **all three were tested and all three refuted** — a 2 × 2 over (governor·boost) × SMT put the mode in **all four** machine states at 2–5 per 50, and at 91 °C the frequency never moved off ~3790 MHz. What does move it is **competing CPU load**: on a quiet box the mode runs ~5%, and with eight spinners added it runs **92%**, with the median going 262 → **449 ns (+71%)**. Against **0.8%** for every §9 tuning row combined. So `check-machine.sh` gained the row that was missing — **`machine is quiet`**, CPU busy over a one-second window, FAIL above 3%, processes attributed by delta — and `DESIGN.md` §9 now leads with it. Clean baselines, quiet box, 60 runs each: untuned **med 260.6, over 43/60**; tuned **med 259.7, over 13/60**. The ceiling still decides nothing — it sits at the median in every configuration measured — but **the load row is now visible to the gate**, and a residual **5–10%** rate on a quiet machine remains **unexplained**: measuring load *per run* rather than per batch shows the outliers carry the **same** busy figure as every other run (13% — the benchmark itself), and shutting the desktop's LLM down left it at **6/60 either way**. Load is *sufficient* to produce the mode and is **not** what produces the natural ones. Five hypotheses measured away: L3, SMT, governor/boost, thermal, load | `DESIGN.md` §6 ceilings; item 6 |
| 21 | **`DESIGN.md` D8 says "the engine thread is pinned to an isolated core". Nothing pins it.** `[measured 2026-08-30]` `grep` for `sched_setaffinity`, `affinity`, `core_affinity` or `libc` across `crates/` and `tools/` returns **nothing**: no dependency, no call, no test. §8's latency budget and §9's `isolcpus` row both assume a pinned engine thread, so the design's central jitter defence is **asserted in prose and absent from the code** — `CLAUDE.md` §4's "prose does not hold a constraint", on the one claim it would cost most. Either the engine pins and something proves it, or D8's sentence is wrong and must say so. Cheap to settle, and it is the mechanism the 324 ns mode most plausibly needs, since load amplifies that mode 5% → 92% and pinning is what isolates a thread from load. **NUMA is deliberately NOT part of this**: this machine reports **1 NUMA node**, cross-L3 placement was measured and had **no effect** (~259 ns in all three `taskset` arms), and topology-aware allocation would be designing against hardware nobody here has — the same reason item 14 keeps kernel bypass out. Revisit if the box ever becomes multi-socket | `DESIGN.md` D8 and §8; every jitter claim; item 20 |
| 6 | A Linux box for `tools/w2w`. The design's own §9 says a latency number from a macOS laptop is not a number. **`[measured 2026-08-30]` the desktop exists, has a toolchain, and has been read: AMD Ryzen 7 3700X, 16 logical cores, Linux 7.0.0-30-generic, rustc 1.98.0 — `check-machine.sh` says `pass 1  fail 7  unknown 1`.** The seven are `isolcpus`/`nohz_full`, governor `powersave`, turbo on, C-states uncapped, SMT on, THP `madvise`, `net.core.busy_poll=0`. **The gap is configuration, not hardware.** `[measured 2026-08-30]` five of the seven were applied and the box now reads **`pass 6  fail 2`**; the two left — `isolcpus`/`nohz_full` and capped C-states — need a kernel command line and a reboot, so `--strict` still refuses and **nothing here is publishable yet**. Toggling is no longer a per-run password prompt: `/usr/local/sbin/fixbolt-machine on|off|tls|status`, root-owned, reachable through a `NOPASSWD` rule scoped to those five verbs, which is what made the same-machine A/B in [measured-costs.md](docs/reference/measured-costs.md) possible at all. **The A/B says the tuning is worth little**: every bench median moves under 2%. | Every gate in §6 that matters |
| 7 | **`scripts/fetch-quickfix-assets.sh` tracks mutable `master`.** Every acceptance number in the codec plan (539 lines, 247 with `9=`, 244 with `10=`, 8 tag-set patterns for `35=3`) can change silently upstream. Pin a commit and verify it | Reproducibility of every step-1 gate |
| 10 | **Can `ktls-core` be driven from a plain non-blocking socket with no async runtime?** Its documented usage is `tokio-rustls`-shaped. If not, ADR-0005's central claim collapses to "userspace rustls only" and the hot-path guarantee goes with it. **`[measured 2026-08-30]` the blocker was recorded wrongly and is now known**: it needs a kernel built with **`CONFIG_TLS`**, *not* the §9 machine of item 6 — kTLS is a kernel feature, not a latency property. The container it was first tried on genuinely lacks it (`# CONFIG_TLS is not set`), and the GitHub runner has it (CI run 33307245558). **`[measured 2026-08-30]` so does the owner's desktop, and this item is UNBLOCKED**: `CONFIG_TLS=m`, `setsockopt(TCP_ULP, "tls")` **ACCEPTED**, `check-ktls-available.sh` exits 0. **It was unblocked all along and the diagnostic said otherwise.** The script printed `config: CONFIG_TLS=m` and, four lines later, *"the kernel has no `tls` ULP at all: it was built without CONFIG_TLS"* — both from one run — because it emitted that fixed paragraph for every `errno` and never re-read the config it had just printed. `ENOENT` from `TCP_ULP` means no ULP is **registered**, and the kernel autoloads one only for a caller with `CAP_NET_ADMIN`; an unprivileged probe sees `ENOENT` with `tls.ko` sitting on disk. Fixed, and now guarded by `scripts/check-ktls-classify.sh` (CI job `script-logic`), which fails 5 of 8 against the old logic. Proven by reversal on the desktop: module unloaded → `LOADABLE`, exit 2; after `modprobe tls` → `READY`, exit 0. Write-up: [reference/ktls-on-a-plain-socket.md](docs/reference/ktls-on-a-plain-socket.md). **Steps 2–5 of the spike are still to do; nothing about `ktls-core` or ADR-0005 is concluded.** | The TLS plan; ADR-0005 acceptance |
| 11 | **Serialise misses its gate: 93.8 ns against 60 ns** on the M5, and `[measured 2026-08-30]` **177.6–199.4 ns across five runs on Linux x86_64** — a 3.0–3.3× miss there, not 1.6×. (DESIGN §6). Cause is known — `Template::encode` finds each slot by a linear scan of the caller's list, so cost is slots × parts. Fix candidates: index slots by tag at template build, or require the caller to hand slots in parts order. The only red gate that does not need the Linux box. **Start with `scripts/bench.sh --strict`** on the §9 box: the number to optimise against is the one from that machine, not the 93.8 ns or the 177.6-199.4 ns above. | DESIGN §6 serialise row; the `engine` step, where the number is re-measured |
| 12 | **SIMD / SWAR for SOH scan and checksum — deliberately not done.** `matthart1983/nanofix` has NEON/SSE2 SOH scanning and still parsed 4–6× slower than this codec, because its 512-entry index blew L1 ([measured-costs.md](docs/reference/measured-costs.md)). Layout won; SIMD did not. Estimated gain here is 20–40 ns per message on a 10–20 µs floor — under 0.5%. **Do it only when `benches/parse.rs` on the Linux box shows parse on the critical path.** If done: 8-byte SWAR in `codec`, no `memchr` (zero-dependency rule), `core::arch` only behind a measurement. **Start with `scripts/bench.sh --strict`**: this is a same-machine A/B, so it needs the box to be quiet, not to be a particular box. | Nothing until open item 6 is answered |
| 13 | **Release profile is default.** No `lto = "fat"`, no `codegen-units = 1`, no PGO, no `#[cold]` on error paths. Cheap, but each is a number to be measured before and after, not a setting to be assumed. **Start with `scripts/bench.sh --strict`**, once before any profile change and once after — same machine, same settings, or the comparison means nothing. | The `engine` step; every §6 number published from Linux |
| 14 | **Kernel bypass path, if PRD §5 is ever reversed: Onload first, `ef_vi` second, DPDK never.** Onload runs the engine unchanged (`onload ./engine`, socket API, TCP in userspace) — D8 spin already fits it; the first measurement is `tools/w2w` twice on the same box, kernel vs `onload`, and that difference decides whether an `ef_vi` L0 is worth writing. `ef_vi`/TCPDirect is a second `impl Transport` behind a real feature flag (D5). DPDK ships no TCP stack — it means writing or embedding one (smoltcp, F-Stack), which is what fixbolt claims and does not do. Any bypass path is plaintext: it and D11 exclude each other. Needs a Solarflare/AMD X2-class NIC — none available | Phase 3, and open item 6 before it |
| 16 | **A journal is written and never read back** — *half closed 2026-08-30*. It now reads back: length-prefixed records, `Journal::highest()`, a torn tail dropped. What remains is the session: `connect` resets unconditionally, so a resumed session's numbers are wiped before they can be used. **[ADR-0010](docs/decisions/ADR-0010-a-reconnect-is-not-a-restart.md) needs signing** | A restart that resumes a session rather than starting one |
| 18 | **A plan can close on a laptop's word while CI is red.** The engine plan closed and merged with its gates reported green from an Apple M5; the GitHub run on that same commit failed and was not read, and four documents carried the laptop's number for a day. Nothing in `CLAUDE.md` §9 requires the closing evidence to name a CI run | Every "gates green" claim in a merge commit |
