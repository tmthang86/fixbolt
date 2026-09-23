# ADR-0101 — The shipped dictionary is chosen by a rule written before the Orchestra diff is measured

- **Status**: **Accepted — 2026-09-23** (the decision rule, by the manager under the owner's standing mandate; the Result section is written after the spike). Proposed 2026-09-23. Approved or refused by the manager under the owner's
  standing mandate (the owner answered ADR-0097 Q2: *Orchestra, with QuickFIX's XML as referee,
  decided only after the difference is measured*). The spike's result is written into
  *Result* below when it has run; the outcome is then read off the rule in decision 3, not
  argued afresh.
- **Date**: 2026-09-23
- **Deciders**: Tran Manh Thang (Q2 of ADR-0097). Written by the architect (Opus).
- **Related**: [ADR-0097](ADR-0097-phase-3-makes-the-engine-dependable-by-a-stranger-and-fixp-waits-on-a-running-oracle.md)
  decisions 4 and 7 (exit criteria 1 and 2);
  [ADR-0001](ADR-0001-relationship-to-quickfix.md) decisions 1 and 5 (QuickFIX is data and an
  oracle; any QuickFIX-derived data that ships triggers `NOTICE`);
  [ADR-0083](ADR-0083-ten-field-types-one-field-spelled-two-ways-one-empty-component-and-where-the-sp2-oracle-comes-from.md)
  (the SP2 pair build and its named-exception pattern); `DESIGN.md` §4 D3 and D5;
  `CLAUDE.md` §2 non-negotiables 3, 5, 6, 9 and §6 *Dependencies*;
  plan [2026-09-23-p3-dictionary-source](../plans/2026-09-23-p3-dictionary-source.md).

## Context

`crates/dict/build.rs` (lines 36–47, `DEFAULT` / `FIXT_DEFAULT` / `SP2_DEFAULT`) reads
`../../vendor/quickfix/spec/FIX44.xml`, and `vendor/` is gitignored (ADR-0001). A crate
downloaded from crates.io has no `vendor/`, so `fixbolt-dict` — and everything above it — does
not build for a stranger. Cargo also skips gitignored files when it packages
(<https://doc.rust-lang.org/cargo/reference/manifest.html>, *include/exclude*), so the file cannot
ride along from `vendor/` by accident either.

ADR-0097 decision 4 listed three ways out and recommended the first without deciding it:

- **(a)** ship FIX Orchestra's `OrchestraFIX44.xml` (Apache-2.0) and keep QuickFIX's XML as the
  oracle the generated tables must agree with;
- **(b)** ship QuickFIX-derived tables and a `NOTICE`;
- **(c)** make the user supply the XML.

What the tables feed is a gate this project already paid for: `[measured 2026-08-28]` against
QuickFIX's generated C++, **912 / 912** tag numbers, **12 524 / 12 524** (message, tag) pairs,
**1 708 / 1 708** enum values, **93** message types, group order on **730 / 730** groups (731
with `NoHops(627)`) — `docs/CONFORMANCE.md` §2–§3. The 59 acceptance definitions were written
against QuickFIX's XML, and the acceptance comparator is positional (non-negotiable 5,
`DESIGN.md` §4 D3). Every place Orchestra disagrees with QuickFIX is a place where the gate and
the product would read different dictionaries.

## Research

Searched 2026-09-23. `[scouted]` marks what the architect read off the file itself; the
agreement with QuickFIX was **not** measured, on purpose — the rule below is written blind to it.

| Source | What it says |
|---|---|
| <https://github.com/FIXTradingCommunity/orchestrations> (GitHub API, `license`) | Repository licence **Apache-2.0**. Root holds `LICENSE`, `README.md`, `pom.xml`, `Examples/`, `FIX Standard/` — **no `NOTICE` file**. |
| same, `FIX Standard/` listing | `OrchestraFIX44.xml` **1 517 010 bytes**; also `OrchestraFIX42.xml`, `OrchestraFIXLatest.xml` (8.6 MB), `FIX44Session.xml`, `FIXTSession.xml`. **There is no FIX 5.0 SP2 Orchestra file.** |
| same, `FIX Standard/Readme.md` | FIX 4.2 and 4.4 files *"were translated from FIX Unified Repository 2010 Edition"*, are *"frozen; no substantive updates will be made"*, and carry message structure only. |
| same, `FIX Standard/Known Issues.md` | Known FIX 4.4 errors are **kept** in the 4.4 file for consistency with the legacy repository (example: `MassCancelRejectReason(532)` typed `char`). |
| same, commit history of `OrchestraFIX44.xml` | Last changed `cd24169a2abd` on 2026-09-08 (*"Update FIX 4.2, 4.4 and Latest"*); ten commits since 2023 — "frozen" is not "unchanging". |
| `[scouted]` the file at `cd24169a2abd8daba7c360987c7a46ca11873a12`, sha256 `a36262895e90bbcad2948e0c98072173a571a253ba63107a89617441c67a9f86` | Root `fixr:repository version="FIX.4.4_EP311"`; 912 `field`, 93 `message`, 15 `component` (incl. `StandardHeader` / `StandardTrailer`), 92 `group`, 247 `codeSet`, 1 728 `code`. Groups are separate elements reached by `groupRef`, not inline as in QuickFIX. Enumerated fields take a `…CodeSet` as their `type`. DATA fields carry an explicit `lengthId`. 68 elements carry `updated="FIX.Latest"`; one says `deprecated="FIIX.4.4"`. `dcterms:rights` reads *"Copyright (c) FIX Protocol Ltd. All Rights Reserved."* gzip -9 of the file is **150 283 bytes**. |
| <https://github.com/FIXTradingCommunity/fix-orchestra-spec> `v1-0-STANDARD/orchestra_spec.md` | **"the order of fields in an Orchestra file is not guaranteed to match the order on the wire"** — order is the presentation protocol's business. `presence` is `required` / `optional` / `forbidden` / `ignored` / `constant`. |
| <https://github.com/quickfix-j/quickfixj/blob/master/quickfixj-messages/readme.md> | QuickFIX/J generates FIX Latest and FIXT 1.1 from Orchestra, after an XSLT that *"remove[s] elements … that cause issues"*; the FIX 4.2 / 4.4 Orchestra files **"are not used by the QuickFIX/J build"** — its FIX 4.4 stays its own `FIX44.xml`. |
| <https://github.com/FIXTradingCommunity/fix-orchestra-quickfix> | `repository-quickfix` generates a QuickFIX data dictionary from an Orchestra file (Apache-2.0, last pushed 2022). No published comparison of its output with QuickFIX's shipped `FIX44.xml` was found. |
| <https://pypi.org/project/fixorchestra/> (Gary Hughes, MIT, `fixaudit.py`) | Compared an **older** FIX 4.4 orchestration with the 2010 Repository (20200402): 912 = 912 fields, 93 = 93 messages, **one discrepancy — Logon (35=A) lacked `NoMsgTypes` / `RefMsgType` / `MsgDirection`**. Orchestra vs Repository, not vs QuickFIX; whether it persists in EP311 is the spike's to say. |
| <https://www.quickfixj.org/jira/si/jira.issueviews:issue-html/QFJ-757/QFJ-757.html> | QuickFIX's `FIX44.xml` restricts `StipulationValue(234)` more than the FIX 4.4 text does — QuickFIX's XML is **not** the specification either. |
| <https://www.fixtrading.org/packages/fix-4-4-20030618-specification-documentation/> | *FIX 4.4 with Errata 20030618* is the normative FIX 4.4 document. `fiximate`'s FIX 4.4 page now redirects to `orchimate.org`, which renders Orchestra — so it is **not** an independent tie-breaker. |
| <https://fixtrading.org/fix-repository/> | *"The Apache License, Version 2.0 applies to the current format including Orchestra"*; the Unified Repository was first published under a proprietary licence. |
| <https://www.apache.org/licenses/LICENSE-2.0> §4 | Redistribution requires (a) a copy of the licence, (b) prominent notices on **modified** files, (c) retained copyright notices in Source form, (d) a `NOTICE` **only if the Work includes one**. |
| <https://doc.rust-lang.org/cargo/reference/publishing.html>; <https://github.com/rust-lang/crates.io/issues/195> | crates.io limits a `.crate` to **10 MB** compressed. |
| <https://doc.rust-lang.org/cargo/reference/manifest.html> | `license` is an SPDX 2.3 expression; `AND` means the user must comply with all. |

**Found nothing:** a published table of differences between QuickFIX's `FIX44.xml` and either the
2010 Repository or the Orchestra FIX 4.4 file; any engine that ships FIX 4.4 generated from
Orchestra (QuickFIX/J explicitly does not); an Orchestra FIX 5.0 SP2 file; a statement from the
FIX Trading Community reconciling the file's *"All Rights Reserved"* line with the repository's
Apache-2.0 licence.

## Options considered

The three of ADR-0097 decision 4, plus two variants of (a):

- **(a1) Orchestra, unmodified.** No NOTICE (the upstream has none), a licence copy, the file
  byte-identical so Apache §4(b) never applies. Every divergence from QuickFIX is named in the
  referee test's exemption list.
- **(a2) Orchestra plus a committed overlay.** A short table in `build.rs` that corrects the
  Orchestra data where the FIX 4.4 **specification document** sides against it. An overlay row
  justified only by *"QuickFIX says so"* is QuickFIX-derived data and would trigger ADR-0001
  decision 5 — so no such row is allowed.
- **(a3) Orchestra transformed offline** (QuickFIX/J's XSLT route) into a QuickFIX-shaped file
  that is shipped. Rejected: the shipped file is then a modified Apache file (§4(b) notices), a
  second format to maintain, and it hides the corrections inside a transform instead of listing
  them.
- **(b) QuickFIX-derived tables with `NOTICE`.** Always available; the attribution clause then
  reaches every binary a user ships.
- **(c) User supplies the XML.** Breaks `cargo add` for everyone, to protect nobody.

## Decision

1. **The spike measures before anything is built** (plan row 1). One program — Python 3,
   standard library only, `scripts/dict-diff.py`, **no change to `build.rs` or any crate** —
   reads QuickFIX's `vendor/quickfix/spec/FIX44.xml` and Orchestra's
   `vendor/orchestra/OrchestraFIX44.xml`, flattens both into the same model (components
   expanded, groups nested, header / trailer separated — the flattening `build.rs` does), and
   writes a divergence table over nine dimensions, one row per divergence, keyed by content:

   | Dim | Compared | QuickFIX-side count it must reproduce |
   |---|---|---|
   | T | tag number ↔ name | 912 |
   | Y | field type, after both spellings map to one `FieldType` variant | 912 |
   | M | message types | 93 |
   | P | (message, tag) body pairs | 12 524 |
   | R | (message, tag) required pairs, where the pair exists on both sides | — (reported) |
   | E | (tag, value) enum pairs | 1 708 |
   | H | header and trailer membership | 30 header |
   | L | DATA → length tag | 16 DATA fields |
   | G | group key (message, counter): exists, delimiter, **member sequence exactly equal** | 731 |

   G is the dimension non-negotiable 5 lives in, and the Orchestra specification says outright
   that file order need not be wire order — so G is compared as exact sequence equality, not as
   a set.

   The program proves itself before it is believed: **QuickFIX against itself gives zero rows and
   reproduces every count in the right-hand column**; and a mutated copy of the Orchestra file
   (two adjacent members of one group swapped, one enum value removed) gives **exactly those two
   rows**, in G and E. A spike that cannot reproduce the numbers the Rust tests already assert
   is measuring its own flattening.

2. **The Orchestra file at spike time is fetched, never committed.**
   `scripts/fetch-orchestra-assets.sh` fetches `FIX Standard/OrchestraFIX44.xml` at commit
   `cd24169a2abd8daba7c360987c7a46ca11873a12` into gitignored `vendor/orchestra/`, and refuses
   a file whose sha256 is not `a36262895e90…67a9f86` (full value above). A moving upstream is not
   an oracle (the same reason `fetch-quickfix-assets.sh` pins its SHA).

3. **The decision rule, fixed now.** Each divergence row is classed:

   - **gate-visible** — any row in M, H, L or G; or any row in T, Y, P, R or E whose tag, or
     whose message type, appears in the 59 FIX 4.4 acceptance definitions
     (`vendor/quickfix/test/definitions/server/fix44/*.def`, read by the spike), or whose
     message type is one of the seven session messages `0 1 2 3 4 5 A`;
   - **quiet** — every other row.

   Then, read in order, the first that holds wins:

   | Outcome | When | What ships |
   |---|---|---|
   | **A** | gate-visible = 0 **and** quiet ≤ 25 | Orchestra unmodified (a1). Each quiet row becomes a named exemption in the referee test. |
   | **B** | total rows ≤ 150, **and** every gate-visible row is settled by the *FIX 4.4 with Errata 20030618* document (volume and page cited in the row), **and** the rows where the document sides against Orchestra number ≤ 25 | Orchestra plus an overlay (a2) of exactly those ≤ 25 rows, each carrying its citation. Rows where the document sides with Orchestra are not overlaid; they are exemptions, and row 2 must then show 59 / 59 still green. |
   | **C** | anything else — including any gate-visible row the document does not settle, or 59 / 59 going red in row 2 under outcome B | **Fall back to (b)**, QuickFIX-derived tables with `NOTICE`, and the manager returns to the owner before building it (the scope plan's risk row already says so). (c) is not chosen by this rule; the owner may choose it over (b). |

   Why these numbers: 25 is the size of named-exemption list this project already reviews in one
   sitting (`TYPE_EXEMPTIONS` in `crates/dict/tests/interop_quickfix_fields.rs` has 14); 150 is
   about 1.2 % of the 12 524 pairs — past it, the overlay plus the exemptions are a second
   dictionary maintained by hand. Both are judgement. They are fixed **before** the measurement
   so that they cannot be tuned to it.

4. **If A or B: where the file lives and how it is built** (plan row 2).
   - `crates/dict/spec/OrchestraFIX44.xml`, **byte-identical** to the pinned upstream (no
     §4(b) notice needed, §4(c) satisfied by leaving the `dcterms:rights` line in place);
     `crates/dict/spec/LICENSE-orchestrations` (the upstream `LICENSE`, verbatim);
     `crates/dict/spec/README.md` (source URL, commit, sha256, "unmodified"). 1.5 MB raw,
     ~150 KB compressed: 1.5 % of crates.io's 10 MB limit. A CI step checks the sha256.
   - `fixbolt-dict`'s `license` becomes `"(MIT OR Apache-2.0) AND Apache-2.0"`: the crate's own
     code stays dual-licensed, the shipped data is Apache-2.0 whichever the user picks.
   - `build.rs` reads that file by default, relative to `CARGO_MANIFEST_DIR`, with no network and
     no external toolchain (non-negotiable 6), through the existing `roxmltree = "=0.20.0"`
     build-dependency — **no new dependency** (§6). `NANOFIX_FIX44_XML` still overrides, and the
     root element (`fixr:repository` vs `fix`) selects the reader, so a user may still bring
     QuickFIX-format XML (option c survives as an escape hatch, not the default).
   - Both readers produce **one neutral model** (messages → members: field / component / group,
     with presence), and the existing `emit` consumes only that model. The refactor lands first,
     on the QuickFIX source, and is proven by the generated `fix44.rs` and
     `fixt11_fix50sp2.rs` being **byte-identical** before and after.
   - Under B, the overlay is a `const` table in `build.rs`, checked in both directions like
     `SP2_LENGTH_EXCEPTIONS`: a row whose *before* value is not what the Orchestra file says, or
     whose correction is already true, fails the build.

5. **QuickFIX stays the referee, and a missing referee is red, not skipped.** The tests that
   already compare the tables with QuickFIX's generated C++ and XML
   (`interop_quickfix_fields.rs`, `interop_quickfix_messages.rs`, `interop_quickfix_order.rs`,
   `enums.rs`) keep reading `vendor/quickfix/`; each gains an exemption list for the quiet rows,
   checked both ways (an exemption that no longer diverges fails). One new test,
   `crates/dict/tests/referee_quickfix_xml.rs`, covers what they do not — required pairs, header
   and trailer, DATA → length — so all nine dimensions have a referee. This set is *the
   dictionary agreement test* ADR-0097 exit criterion 2 names.
   `crates/dict/tests/common/mod.rs` already says *"A missing file is a failure, never a skip"*,
   and that stands: the no-`vendor/` CI job builds and packages (exit criteria 1 and 3) and never
   runs these tests, while a crates.io user never runs a dependency's tests at all. That the
   referee **ran** is proven in CI by `scripts/check-feature-gated-tests-ran.sh`, extended to
   take `-` for "default features": every test the `fixbolt-dict` build lists appears in the
   run's log with a status, none ignored.

6. **`fix50sp2` stays bring-your-own on crates.io.** No Orchestra FIX 5.0 SP2 file exists, and
   `OrchestraFIXLatest.xml` is not SP2. The feature keeps reading QuickFIX-format XML through
   `NANOFIX_FIXT11_XML` / `NANOFIX_FIX50SP2_XML`; without them the build fails loudly naming both
   variables (today's behaviour, kept). The library crate `fixbolt` does not expose the feature,
   so `cargo add fixbolt` is unaffected. This narrows the phase-3 scope plan's trap row
   *"dry-run chạy … từng feature công khai"*: `fix50sp2` is built in CI with `vendor/`, and in
   the no-`vendor/` job its failure message is asserted instead. Deriving SP2 from
   `OrchestraFIXLatest.xml` filtered by `added` is a separate spike if ever wanted.

## Consequences

**Good**

- The choice is made by a table and a rule, not by a mood after seeing the numbers.
- Under A or B the published crate needs nothing but itself: no `vendor/`, no network, no new
  dependency, no NOTICE, 150 KB of data.
- The referee grows from four dimensions checked to nine, so the first time any source is
  swapped the whole surface is compared — this ADR buys that even if outcome C wins.
- DATA → length gets an explicit source (`lengthId`) instead of the name rule plus exceptions.
- The neutral model separates *reading a dictionary* from *emitting tables*; a future source (an
  SP2 derivation, a venue's Orchestra file) is one reader, not a second generator.

**Bad — and accepted**

- **The product and the gate may read different dictionaries.** Under A or B every exemption
  is a place where fixbolt answers differently from QuickFIX, and the 59 definitions were written
  against QuickFIX. They stay green only if no exempted row is one they send — which is exactly
  what the "gate-visible" class is meant to catch, and exactly what it can miss if a row touches
  behaviour the corpus reaches indirectly.
- **The thresholds are judgement.** 25 and 150 are defended above, not derived. A result of 26
  quiet rows lands in B for a reason that is arithmetic, not engineering.
- **Orchestra is not frozen in practice** (ten commits since 2023). Pinning protects the build;
  it also means upstream errata reach fixbolt only by a deliberate re-pin, re-spike and re-run.
- **The licence reading rests on the repository's `LICENSE`.** The file's own metadata says *"All
  Rights Reserved"*. We read that as the copyright notice Apache §4(c) tells us to retain, under
  a licence granted by the repository — the reading every Apache-licensed project with a
  copyright line relies on. If the FIX Trading Community says otherwise, the outcome is C.
- **`build.rs` grows a second reader** and the model refactor touches the generator the whole
  session layer depends on — the change is invisible when right and silently wrong when not,
  which is why byte-identical output is its gate.
- **`fix50sp2` is not a one-line `cargo add`** for a stranger. It is opt-in and documented, but
  it is option (c) for that feature.
- **The spike costs a Python program** that duplicates the flattening logic of `build.rs`. That
  duplication is the point (a second program, as `fixt.rs` did with `xml.etree`), and also a
  second place to be wrong — hence its self-check against the counts the Rust tests assert.
- **Outcome C returns the question to the owner** after a spike's worth of work, with the
  NOTICE option ADR-0097 recommended against.

## Result

**Outcome C — written 2026-09-23.** `[measured 2026-09-23]` `python3 scripts/dict-diff.py`
at commit `4eaeb53` (branch `plan/p3-dictionary-source`), on the owner's desktop (a count, not a
timing, so no §9 settings apply). Inputs: QuickFIX `vendor/quickfix/spec/FIX44.xml` at pin
`386ce46e`, sha256 `a82655b5…3358425`; Orchestra `vendor/orchestra/OrchestraFIX44.xml` at
`cd24169a`, sha256 `a36262895e90…67a9f86`. The spike's own proofs passed first:
`--self-check` reproduced 912 / 93 / 12 524 / 1 708 / 731 / 30 / 16 with 0 rows (red first,
with group flattening stubbed); `--mutation-check` found exactly the 2 planted rows.

| Dim | QuickFIX count | Orchestra count | Rows | Gate-visible | Quiet |
|---|---|---|---|---|---|
| T tag ↔ name | 912 | 912 | 1 | 0 | 1 |
| Y field type | 912 | 912 | 2 | 0 | 2 |
| M message types | 93 | 93 | 0 | 0 | 0 |
| P body pairs | 12 524 | 12 527 | 3 | 0 | 3 |
| R required pairs | — | — | 15 | 0 | 15 |
| E enum (tag, value) | 1 708 | 2 371 | 663 | 94 | 569 |
| H header / trailer | 30 | 30 | 0 | 0 | 0 |
| L DATA → length | 16 | 16 | 0 | 0 | 0 |
| G groups (exact sequence) | 731 | 731 | 3 | 3 | 0 |
| **total** | | | **687** | **97** | **590** |

Read off decision 3: **not A** (97 gate-visible rows, A needs 0); **not B** (687 rows, B needs
≤ 150). **Outcome C.**

**The unit, stated rather than re-argued.** Decision 1 counts E per (tag, value) pair, and that
is how the rule was applied. 660 of the 663 E rows are **29 fields that QuickFIX gives no enum
list at all** while Orchestra points them at a code set — 93 of the gate-visible rows are one
field, `RefMsgType(372)`, which the corpus sends. Counted per field, E would be 32 rows and
the whole table **56 rows, 5 gate-visible** (`RefMsgType(372)`, `OrdStatus(39)`, the three G
rows). That count was noted and **not** applied: the rule was fixed before the measurement, and
re-counting after seeing the result is the tuning decision 3 exists to prevent. Even per field,
A fails (5 gate-visible rows) and B would hinge on the FIX 4.4 document settling all 5 — the
unit changes the margin, not the fact that the gate-visible rows exist.

What the rows are (full table and the three traps:
[`docs/reference/orchestra-fix44-vs-quickfix-fix44.md`](../reference/orchestra-fix44-vs-quickfix-fix44.md)):
every E row is a value Orchestra has and QuickFIX lacks — Orchestra is a strict superset;
G is three members QuickFIX lacks (`ClearingFeeIndicator(635)` in `35=AE` `NoSides`,
`OrigOrdModTime(586)` in `35=s` and `35=t` `NoSides`), the same three as P; T is tag 327
named `HaltReasonChar` by QuickFIX and `HaltReason` by Orchestra; Y is 532 and 674, `STRING`
in QuickFIX and `int` in Orchestra; 14 of the 15 R rows come from QuickFIX wrapping a group in a
component while Orchestra puts `presence` on the `groupRef` itself.

**The owner's choice (2026-09-23, in conversation, per the C row):** ship **QuickFIX's**
`FIX44.xml`-derived dictionary in the published crate, **with a `NOTICE`** — not Orchestra,
not user-supplied. That decision, what it ships and what it costs, is
[ADR-0104](ADR-0104-the-published-dictionary-is-quickfixs-xml-shipped-with-a-notice.md). This
ADR's decisions 4–6 (the Orchestra build, the Orchestra referee, `fix50sp2` as
bring-your-own) are therefore **not taken**; decisions 1–3 stand as the record of how the choice
was made.

## Sources

The *Research* table; all read 2026-09-23. In-repository: `crates/dict/build.rs` lines 1–70 and
the function list (`generate`, `emit`, `collect_groups`, `collect_required`, `collect_header`);
`crates/dict/Cargo.toml`; `crates/dict/tests/common/mod.rs` header;
`crates/dict/tests/interop_quickfix_{fields,messages,order}.rs` and `enums.rs` headers;
`scripts/check-feature-gated-tests-ran.sh` lines 1–110; `scripts/fetch-quickfix-assets.sh`
lines 1–40; `.gitignore` line 28; `Cargo.toml` `license = "MIT OR Apache-2.0"`;
`docs/CONFORMANCE.md` §1–§3 and §9; `DESIGN.md` §4 D3 and D5.
