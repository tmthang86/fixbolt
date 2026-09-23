# ADR-0104 — The published dictionary is QuickFIX's XML, shipped with a NOTICE

- **Status**: **Accepted — 2026-09-23** (by the manager, following the owner's choice of outcome C in conversation: QuickFIX + NOTICE). Proposed 2026-09-23. Accepted or refused by the manager under the owner's
  standing mandate. The owner chose the route (outcome C of ADR-0101, 2026-09-23, in
  conversation); this ADR records what that choice ships and what it costs.
- **Date**: 2026-09-23
- **Deciders**: Tran Manh Thang (chose QuickFIX + `NOTICE`). Written by the architect (Opus).
- **Supersedes**: [ADR-0001](ADR-0001-relationship-to-quickfix.md) decision 1's clause
  *"not redistributed inside this repository"*, **for the three files named in decision 1 below
  only**. ADR-0001 decision 5 is not superseded — it is the rule this ADR carries out.
  ADR-0001 decisions 2–4 stand unchanged: the `.def` corpus, `Session.cpp` and QuickFIX's
  generated C++ remain a test oracle in gitignored `vendor/`.
- **Related**: [ADR-0101](ADR-0101-the-shipped-dictionary-is-chosen-by-a-rule-written-before-the-orchestra-diff-is-measured.md)
  *Result* (the measurement that produced outcome C);
  [ADR-0097](ADR-0097-phase-3-makes-the-engine-dependable-by-a-stranger-and-fixp-waits-on-a-running-oracle.md)
  decisions 4 and 7 (exit criteria 1–3); `CLAUDE.md` §2 non-negotiables 6 and 9, §8 (`vendor/`);
  [reference/orchestra-fix44-vs-quickfix-fix44.md](../reference/orchestra-fix44-vs-quickfix-fix44.md);
  plan [2026-09-23-p3-dictionary-source](../plans/2026-09-23-p3-dictionary-source.md).

## Context

A crate downloaded from crates.io cannot build `fixbolt-dict`, because `crates/dict/build.rs`
reads QuickFIX's XML from gitignored `vendor/` (ADR-0101 *Context*). ADR-0101 measured the
Apache-2.0 alternative, FIX Orchestra's FIX 4.4 file, against QuickFIX's: **687 divergences, 97
of them visible to the acceptance gate**, including three repeating groups whose member sequence
differs — outcome C under a rule written before the measurement. The owner chose option (b) of
ADR-0097 decision 4: ship QuickFIX-derived data, with a `NOTICE`.

ADR-0001 foresaw exactly this in decision 5: *"If any QuickFIX-derived text or data ever ships
inside a fixbolt artifact, the attribution clause above is honoured in `NOTICE`, and the name
'QuickFIX' is never used in fixbolt's own name or marketing."*

## Research

Read 2026-09-23.

| Source | What it says |
|---|---|
| <https://raw.githubusercontent.com/quickfix/quickfix/master/LICENSE> | *The QuickFIX Software License, Version 1.0*, "Copyright (c) 2001-2020 Oren Miller". Five conditions and a disclaimer, quoted below. |
| <https://github.com/quickfix-j/quickfixj/blob/master/LICENSE>; `quickfixj-messages/quickfixj-messages-fix44/src/main/resources/` | QuickFIX/J ships `FIX44.xml` (and a `FIX44.modified.xml`) inside its jars, under the same licence text ("Copyright (c) 2001-2005 quickfixengine.org"). |
| <https://github.com/quickfixgo/quickfix> (`LICENSE`, `spec/`) | QuickFIX/Go commits `spec/FIX40.xml` … `FIX50SP2.xml`, `FIXT11.xml` in its repository under the QuickFIX Software License. |
| crates.io API, `quickfix-msg44` 0.2.1; <https://github.com/arthurlm/quickfix-rs> `quickfix-msg44/build.rs` | A Rust crate on crates.io that does what this ADR does: ships `src/FIX44.xml`, generates from it in `build.rs`, crate size **41 470 bytes**, 120 458 downloads. Declares `license = "MIT OR Apache-1.1"` — which lets a user pick MIT and drop the QuickFIX conditions; not a pattern to copy. |
| crates.io download of `fefix` 0.7.0 | Ships `src/fefix_core/resources/quickfix/FIX-4.0.xml` … `FIXT-1.1.xml` under `license = "MIT OR Apache-2.0"` with **no** QuickFIX licence or notice in the crate. Prior art that under-attributes. |
| <https://github.com/spdx/license-list-data> (`licenses.json`, list 3.29.0) | **No SPDX identifier exists for the QuickFIX licence.** Its shape is Apache-1.1's, but the text is not Apache-1.1's. |
| <https://doc.rust-lang.org/cargo/reference/manifest.html> | `license` is an SPDX 2.3 expression, "must be a known license from the SPDX license list"; `license-file` may be used "in lieu of" `license` for a nonstandard licence. |
| <https://github.com/rust-lang/crates.io> `src/licenses.rs`; <https://github.com/EmbarkStudios/spdx> `src/lexer.rs` | crates.io parses `license` with the `spdx` crate, `allow_unknown: false`; that crate accepts `LicenseRef-…` ids even in its strict mode ("Only license identifiers in the SPDX license list, or Document/LicenseRef, are allowed"). Read from source, **not** proven by a publish. |
| <https://doc.rust-lang.org/cargo/reference/publishing.html> | crates.io limits a `.crate` to 10 MB compressed. |
| `vendor/quickfix/spec/` at pin `386ce46e` (`scripts/fetch-quickfix-assets.sh`) | `FIX44.xml` 315 399 B (gzip -9: 36 721 B), sha256 `a82655b5…3358425`; `FIXT11.xml` 11 927 B (2 094 B), sha256 `baf0ef6d…090bf20`; `FIX50SP2.xml` 1 471 310 B (156 771 B), sha256 `7d34e565…19d3c3f3`. None carries a licence header of its own. |

**Found nothing:** an SPDX `LicenseRef` for QuickFIX used by any crate on crates.io; any statement
from quickfixengine.org on whether a generated table is "a product derived from this software";
the `ferrumfix` repository at the URL crates.io names (404).

The licence's conditions, verbatim:

> 1. Redistributions of source code must retain the above copyright notice, this list of
>    conditions and the following disclaimer.
> 2. Redistributions in binary form must reproduce the above copyright notice, this list of
>    conditions and the following disclaimer in the documentation and/or other materials
>    provided with the distribution.
> 3. The end-user documentation included with the redistribution, if any, must include the
>    following acknowledgment: "This product includes software developed by quickfixengine.org
>    (http://www.quickfixengine.org/)." Alternately, this acknowledgment may appear in the
>    software itself, if and wherever such third-party acknowledgments normally appear.
> 4. The names "QuickFIX" and "quickfixengine.org" must not be used to endorse or promote
>    products derived from this software without prior written permission. […]
> 5. Products derived from this software may not be called "QuickFIX", nor may "QuickFIX"
>    appear in their name, without prior written permission of quickfixengine.org

## Options considered

- **Ship `FIX44.xml` only; `fix50sp2` stays bring-your-own.** Saves 1.48 MB raw / 159 KB
  compressed and nothing else — the NOTICE is paid either way. Rejected: it leaves a published
  feature that does not build from crates.io, for no licence saving.
- **Ship the three files** (chosen).
- **Ship pre-generated `.rs` tables instead of XML.** The tables are equally QuickFIX-derived, so
  the NOTICE is the same; a reader can no longer diff the input against upstream; and the
  generator in `build.rs` would stop being exercised by every build. Rejected.
- **Ship a trimmed or reformatted XML.** A modification with no licence benefit, and the
  byte-identical pin check below would no longer be possible. Rejected.

## Decision

1. **What ships.** Exactly three files, **byte-identical** to QuickFIX at the pin
   `386ce46e917ae494ab6e90b1be90fd421cdbe3f9` that `scripts/fetch-quickfix-assets.sh` already
   uses: `FIX44.xml`, `FIXT11.xml`, `FIX50SP2.xml`. **1 798 636 bytes raw, ~196 KB compressed
   — about 2 % of crates.io's 10 MB limit.** Nothing else from QuickFIX enters the tree: the `.def`
   corpus, the generated C++, `FixValues.h` and all source stay in `vendor/` as the oracle.
2. **Where.** `crates/dict/spec/FIX44.xml`, `crates/dict/spec/FIXT11.xml`,
   `crates/dict/spec/FIX50SP2.xml`. `.gitattributes` marks them `-text` so no checkout rewrites a
   byte. `scripts/check-dict-spec-pin.sh` (new) asserts each file's sha256 against the value
   recorded in the script, asserts the script's pin equals `fetch-quickfix-assets.sh`'s
   `PINNED_SHA`, and, when `vendor/` is present, `cmp`s each against `vendor/quickfix/spec/`.
   Bumping the QuickFIX pin is then one deliberate commit that moves all three.
3. **`build.rs`** reads `spec/FIX44.xml` (and, under `fix50sp2`, the pair) relative to the
   package root: no `vendor/`, no network, no external toolchain (non-negotiable 6). The
   `NANOFIX_FIX44_XML`, `NANOFIX_FIXT11_XML`, `NANOFIX_FIX50SP2_XML` overrides stay, for a user
   who brings a customised dictionary. The generator itself does not change, so the generated
   tables are the same bytes as today's — that equality is the switch's gate.
4. **The NOTICE.** One text, in two places that must stay byte-identical (the pin script checks
   it): `NOTICE` at the repository root, for the source distribution on GitHub, and
   `crates/dict/NOTICE`, because only files under a package's root reach its `.crate`. Text:

   ```text
   fixbolt-dict — third-party notice

   The files spec/FIX44.xml, spec/FIXT11.xml and spec/FIX50SP2.xml in this crate
   (crates/dict/spec/ in the fixbolt repository) are copied unmodified from QuickFIX,
   https://github.com/quickfix/quickfix, commit 386ce46e917ae494ab6e90b1be90fd421cdbe3f9.
   The tables fixbolt-dict generates from them at build time are derived from them.
   Those files and those tables are distributed under the QuickFIX Software License,
   Version 1.0, reproduced in full below. The rest of fixbolt is licensed MIT OR
   Apache-2.0 and contains no QuickFIX source code.

   This product includes software developed by quickfixengine.org
   (http://www.quickfixengine.org/).

   fixbolt is not QuickFIX and is not endorsed by quickfixengine.org.

   ----------------------------------------------------------------------------
   <the LICENSE file of quickfix/quickfix at commit 386ce46e, verbatim, in full>
   ```

   The licence is reproduced from the **pinned commit**, not from `master` — the copyright line
   has changed upstream before (QuickFIX/J's copy reads "2001-2005 quickfixengine.org";
   the pin reads "2001-2020 Oren Miller", identical to `master` by `diff` on 2026-09-23).
5. **The acknowledgment reaches binaries through the code.** `fixbolt_dict::NOTICE: &str =
   include_str!("../NOTICE")`, re-exported as `fixbolt::NOTICE`, so an application can satisfy
   condition 3's *"in the software itself"* by printing it wherever it lists third-party
   notices. `docs/GUIDE.md` states the obligation: **a binary that links fixbolt carries
   QuickFIX-derived tables, and whoever distributes that binary owes conditions 2 and 3.**
6. **Cargo metadata.** `fixbolt-dict`: `license = "(MIT OR Apache-2.0) AND
   LicenseRef-QuickFIX-1.0"`. The `AND` is the honest reading — a user may choose MIT or Apache
   for fixbolt's code but must also honour QuickFIX's licence for the data; `quickfix-msg44`'s
   `OR` is the mistake this avoids. `LicenseRef-QuickFIX-1.0` is read by crates.io's parser as
   valid (source, above), **unproven until the first publish**; if crates.io refuses it, the
   fallback is `license-file = "NOTICE"`, which crates.io documents for exactly this case, at
   the cost that tools parsing SPDX then see nothing. Every other published crate keeps
   `MIT OR Apache-2.0`: none of them contains QuickFIX data, and a dependency-licence tool
   (`cargo-deny`, `cargo about`) sees `fixbolt-dict`'s expression through the graph.
7. **The name.** Condition 4 and ADR-0001 decision 5: "QuickFIX" appears in fixbolt's
   documentation as a fact — the acceptance corpus it is tested against, the engine it is
   interop-tested with, the source of its dictionary — and never as an endorsement, a
   compatibility badge or a comparison in marketing copy. This is a hand check on `README.md`
   and crate descriptions before publish.
8. **The referee after the switch.** With the shipped XML identical to the vendored XML, "compare
   against QuickFIX's XML" becomes comparing a file with itself. What still referees the tables:
   (i) the pin script — the shipped bytes are QuickFIX's bytes; (ii) at the switch commit only,
   the generated `fix44.rs` and `fixt11_fix50sp2.rs` hashes equal to those of the parent commit;
   (iii) the tests that compare the tables with **QuickFIX's generated C++** —
   `interop_quickfix_fields.rs`, `interop_quickfix_messages.rs`, `interop_quickfix_order.rs`,
   `fixt_order.rs`, and `enums.rs`'s `FixValues.h` half — which read `vendor/quickfix/src/C++`
   and are a different program from `build.rs`; their XML reads move to `crates/dict/spec/` so
   they check what ships. A missing oracle stays a failure, never a skip
   (`crates/dict/tests/common/mod.rs`), and CI proves the oracle tests ran with
   `scripts/check-feature-gated-tests-ran.sh fixbolt-dict - LOG`. `scripts/dict-diff.py` stays in
   the tree as the tool that produced ADR-0101's result, not as a gate.

## Consequences

**Good**

- `cargo add fixbolt` builds with nothing but crates.io — every feature, `fix50sp2` included.
- The tables do not change at all: 912 / 12 524 / 1 708 / 730, 59 / 59 and FIXT's score are the
  same bytes' results. None of ADR-0101's 687 divergences reaches a user.
- The acceptance gate and the product read **one** dictionary, which Orchestra could not offer.
- The obligation is written down in four places a user meets it (the `.crate`, the repository,
  the `license` field, the API), where `fefix` and `quickfix-msg44` shipped the same files with
  less.

**Bad — and accepted**

- **The attribution clause reaches every binary built with fixbolt.** Condition 2 and 3 are the
  distributor's duty; for a trading firm shipping an internal binary that is usually nothing,
  for a vendor shipping a product it is a line in their notices. MIT-only was never on offer
  for the tables once option (b) was chosen.
- **The repository now contains QuickFIX data**, which `CLAUDE.md` §2 item 9 and §8 forbade
  in words. Those rules change (the plan gives the sentences); the risk they guarded — a
  licence clause entering the tree by accident — is now a licence clause entering it on
  purpose, held by a pin script rather than by `.gitignore`.
- **`LicenseRef-QuickFIX-1.0` is unproven on crates.io.** The Cargo book says "known license";
  crates.io's code says `LicenseRef` passes. The first publish settles it; the fallback loses
  machine-readability.
- **QuickFIX's dictionary errors ship as fixbolt's behaviour.** QFJ-757's too-narrow
  `StipulationValue(234)`, the missing `NoSides` members, `HaltReasonChar` for `HaltReason` —
  ADR-0101's table is now a list of places where fixbolt follows QuickFIX rather than FIX. A user
  who needs the specification's answer supplies a dictionary through the override.
- **A generated table's legal status is our reading, not a ruling.** We treat the tables as
  derived from the licensed files, which is the cautious reading; nobody upstream has said so.
- **Upgrading the QuickFIX pin is now a product change**, not only a test-oracle change: it moves
  the tables users get, so it needs the same gates as a codec change.
- **Condition 4 constrains how the project talks about itself.** "Passes QuickFIX's acceptance
  suite" is a fact; anything that reads as endorsement is not allowed without permission.

## Sources

The *Research* table; all read 2026-09-23. In-repository: `crates/dict/build.rs` lines 36–47;
`crates/dict/Cargo.toml`; `crates/dict/tests/common/mod.rs` header;
`scripts/fetch-quickfix-assets.sh` lines 1–25 (`PINNED_SHA`); `Cargo.toml`
`license = "MIT OR Apache-2.0"`; ADR-0001 *Decision*; ADR-0101 *Result*.
