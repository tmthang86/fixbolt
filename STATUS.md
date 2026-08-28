# Current state

One screen. A pointer, not a store. Detail lives in the ADRs and the plan files.
**A stale status page is worse than none.**

Last updated: **2026-08-27**.

## Where the work is

| | |
|---|---|
| Branch | **`main`** |
| Milestone | **M1 — the codec.** `codec` and `dict` read and write FIX 4.4 including repeating groups. No session layer, no engine, no socket |
| Scope | **[PRD.md](docs/PRD.md)** — phase 1 = FIX 4.4 tag=value both sides; phase 2 = SBE / FAST / FIXML + FIX 5.0. **TLS has ADR-0005 (Accepted) but no plan — blocked on open item 10** |
| Plan in flight | **none** — next is the `session` layer, and it has no plan yet |
| Last closed | **[2026-08-28-conformance-runner.md](docs/plans/2026-08-28-conformance-runner.md)** — closed 2026-08-28. The 59 definitions run in process and score **0 / 59**; a replaying fake scores 59 / 59, which is what makes the zero mean something |
| Last closed | **[2026-08-27-repeating-groups.md](docs/plans/2026-08-27-repeating-groups.md)** — closed 2026-08-28. Groups read and written, nested to depth 4; field order agreed with QuickFIX's own generated C++ on 730/730 groups |
| Last closed | **[2026-08-27-codec-dict.md](docs/plans/2026-08-27-codec-dict.md)** — closed and merged 2026-08-28. 54 tests, 0 allocations, 304M fuzz executions |
| Last closed | Design reviewed against the HFT latency budget and revised: positioning fixed to "fastest acceptor on kernel TCP", ADR-0002 default reversed (inline dispatch, ring optional), D8 busy-poll, D9 template encoder, D10 send backpressure, §8 latency budget, §9 OS checklist, wire-to-wire gate added |

## Proven — the command was run and its output read

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
- **`0 / 59` is the score.** No session layer exists. The runner is proven to be able to
  recognise a correct answer; nothing has produced one.
- **`Input::Tick` is defined and never sent.** Nothing advances time yet, so
  `4a_NoDataSentDuringHeartBtInt.def` cannot pass even once a session exists.
- The ADRs are accepted on the strength of the reasoning in them, **not on measurement** — see the §8 caveat above.

## Open items

| # | Item | Blocks |
|---|---|---|
| 1 | **Final name.** `nanofixengine` is a placeholder. Free on both crates.io and GitHub: `machfix`, `veloxfix`, `tachyonfix`, `luxfix`, `fixwire`, `fixbolt`, `sohwire` | Nothing yet — but renaming after a crates.io publish is expensive |
| 5 | Ring-buffer policy when the library falls behind: block, drop, or disconnect? | ADR-0002, and the `engine` plan |
| 6 | A Linux box for `tools/w2w`. The design's own §9 says a latency number from a macOS laptop is not a number | Every gate in §6 that matters |
| 7 | **`scripts/fetch-quickfix-assets.sh` tracks mutable `master`.** Every acceptance number in the codec plan (539 lines, 247 with `9=`, 244 with `10=`, 8 tag-set patterns for `35=3`) can change silently upstream. Pin a commit and verify it | Reproducibility of every step-1 gate |
| 8 | **DATA fields inside a repeating group are untested**, on either the read or the write path. `group_roundtrip.rs` skips every DATA member, because writing one needs its length field placed immediately in front — which is open item 9, seen from inside a group | Any counterparty that puts `RawData` in a `Parties` entry |
| 9 | **The encoder has no DATA invariant.** Writing a dynamic DATA field must regenerate its length field, place it immediately before, and count bytes including embedded `0x01`. Only the read path is specified | Any counterparty that sends `RawData`/`XmlData` |
| 10 | **Can `ktls-core` be driven from a plain non-blocking socket with no async runtime?** Its documented usage is `tokio-rustls`-shaped. If not, ADR-0005's central claim collapses to "userspace rustls only" and the hot-path guarantee goes with it. **Cannot be checked here — needs the Linux box of open item 6** | The TLS plan; ADR-0005 acceptance |
