# Prior art for an embeddable engine — other FIX engines, custom dictionaries, licences, and documentation

`[researched 2026-09-26]` Gathered for
[plans/2026-09-26-docs-for-embedders.md](../plans/2026-09-26-docs-for-embedders.md),
[ADR-0206](../decisions/ADR-0206-the-documentation-is-an-mdbook-over-docs-in-place-split-by-diataxis-and-no-cited-file-moves.md)
and
[ADR-0207](../decisions/ADR-0207-a-custom-dictionary-is-an-overlay-generated-in-the-users-build-into-the-users-own-type.md).
The older survey, [prior-art.md](prior-art.md), covers how engines admit a counterparty, represent
a price and hand events to an operator; this page does not repeat it.

**How to read the numbers on this page.** Every latency figure below is **the vendor's own
claim**, quoted with the conditions the vendor stated and nothing more. None was reproduced here.
None may be quoted in this repository beside a fixbolt figure as though the two were comparable:
a fixbolt figure obeys `CLAUDE.md` §2 non-negotiable 10 (committed benchmark, machine, §9
settings), and none of these does. Where a search found nothing, the page says *not found*
rather than guessing.

## 1. Engines

| Engine | Licence / commercial model | Published latency (their claim, their conditions) | Documentation shape |
|---|---|---|---|
| **QuickFIX (C++)** | QuickFIX Software License: BSD-style with an acknowledgment clause and a restriction on the name "QuickFIX"; not OSI-approved because of the acknowledgment clause. Text: [Oracle's mirror](https://docs.oracle.com/en/middleware/fusion-middleware/osa/19.1/osalg/quickfix-j-license.html); [repository](https://github.com/quickfix/quickfix) | Not found on the official site | Per-language landing pages at quickfixengine.org; a configuration reference ([configuration](https://quickfixengine.org/c/documentation/getting-started/configuration.html)) |
| **QuickFIX/J** | Same licence lineage ([quickfixengine.org/j](https://quickfixengine.org/j/)) | Not found | Separate site, a wiki, and repository markdown ([customising-quickfixj.md](https://github.com/quickfix-j/quickfixj/blob/master/customising-quickfixj.md)) |
| **QuickFIX/n, QuickFIX/Go** | Same licence lineage; commercial support offered by Connamara Systems ([quickfixgo](https://github.com/quickfixgo/quickfix)) | Not found | README plus an examples repository ([quickfixgo/examples](https://github.com/quickfixgo/examples)) |
| **Fix8 (C++)** | LGPL v3 ([README](https://github.com/fix8/fix8)); a commercial product, Fix8Pro, from Fix8 Market Tech ([fix8.org](https://fix8.org/)) | *Their claim*: NewOrderSingle encode **1.38 µs**, ExecutionReport decode **3.75 µs**, "on typical hardware", method on their Performance page, not reproduced here ([fix8.org](https://fix8.org/)). *Their older claim*, still in the README: "on average 68% faster encoding/decoding the same message than Quickfix"; "On production level hardware, client NewOrderSingle encode latency is now 2.1us, and ExecutionReport decode 3.2us"; "Without the framework overhead, NewOrderSingle encode latency is 1.4us" — hardware not named, not reproduced here ([README.md](https://github.com/fix8/fix8/blob/master/README.md)) | Landing page, wiki, FAQ, API docs |
| **Chronicle FIX (Java)** | Commercial, licensed with consulting, no public price list ([chronicle.software](https://chronicle.software/choosing-chronicle-fix-engine/)) | *Their claim*: round trip **< 4 µs** for NewOrderSingle "verified at the 99th percentile"; hardware not stated on the page ([chronicle.software/fix-engine](https://chronicle.software/fix-engine/)) | Product pages and use-case articles |
| **OnixS (C++/.NET/Java)** | Commercial subscription, 30-day evaluation; venue dictionaries maintained under a service commitment ([OnixS guide](https://www.onixs.biz/insights/the-complete-guide-to-fix-engine-selection-in-2026)) | **Declines to publish comparative figures**, arguing vendor numbers describe isolated, optimised hardware ([same guide](https://www.onixs.biz/insights/the-complete-guide-to-fix-engine-selection-in-2026)) | Per-language engine guides, each with a dictionary and a dialect article; a public FIX dictionary reference |
| **B2BITS FIX Antenna** | Commercial contract; per-server, per-site and custom tiers; non-production at 50 % ([pricing FAQ](https://www.b2bits.com/product_support/faqs/fix_engine_price)) | Not found | Confluence knowledge base split by language edition |
| **Rapid Addition** | Commercial, sales-led, no public pricing ([rapidaddition.com](https://rapidaddition.com/platform/ra-fix-engine/)) | Not found | Marketing site; no public technical docs found |
| **FerrumFIX / `fefix` (Rust)** | MIT OR Apache-2.0; bundled FIX specification content CC BY-ND 4.0 ([ferrumfix.org](https://ferrumfix.org/)) | Not found. The project says not to use it in production before 1.0 ([ferrumfix.org](https://ferrumfix.org/)) | Overview site and docs.rs |
| **`fixer` / `fixer-gen` (Rust)** | QuickFIX Software License, unusually for a Rust crate ([lib.rs](https://lib.rs/crates/fixer)) | Not found | crates.io and docs.rs only |
| **IronFix (Rust)** | MIT ([repository](https://github.com/joaquinbejar/IronFix)) | **None, stated explicitly**: every target is "a design goal, unmeasured" — the disclosure norm this repository already holds itself to | README with a *What is not implemented yet* list; no acceptor |

**Not found in this pass:** any latency figure for QuickFIX (any language), B2BITS, Rapid
Addition or Esprow; an Esprow product named "EFIX" (Esprow's public products are FIX test and
simulation tools, [esprow.com](https://www.esprow.com/products/fix-application-servers.php)).

## 2. How engines let a user add fields, messages and venue dialects

The feature QuickFIX users expect, and the mechanisms in use:

| Engine | When the dictionary is read | What a user edits | Source |
|---|---|---|---|
| **QuickFIX (C++)** | **Run time**, per session, from a path in the configuration | Their own copy of the XML | `UseDataDictionary` (default `Y`: "You should always use a DataDictionary if you are using repeating groups"); `DataDictionary` ("XML definition file for validating incoming FIX messages. If no DataDictionary is supplied, only basic message validation will be done"); `TransportDataDictionary` / `AppDataDictionary` for FIXT 1.1; `ValidateUserDefinedFields` (default `Y`; "If set to N, user defined fields will not be rejected if they are not defined in the data dictionary, or are present in messages they do not belong to"). `AllowUnknownMsgFields` is **not** on the C++ page — [configuration](https://quickfixengine.org/c/documentation/getting-started/configuration.html) |
| **QuickFIX/J** | Run time for validation, **plus build-time code generation** of typed message classes | A copy of the XML (or an Orchestra repository), built into a separate artifact | "It is not necessary to maintain a fork of the entire QuickFIX/J project to provide customised QuickFIX Dictionaries"; the example adds a field at tag 5000 ([customising-quickfixj.md](https://github.com/quickfix-j/quickfixj/blob/master/customising-quickfixj.md)); Maven plugin `quickfixj-codegenerator` with `dictFile`, `packaging`, `fieldPackage` ([repository](https://github.com/quickfix-j/quickfixj-codegenerator)) |
| **Fix8** | **Build time**: the `f8c` compiler generates C++ encoders, decoders and tables | The schema, in its own or QuickFIX XML format | "If you need to add customised messages or fields, simply update the schema and recompile"; "Multiple FIX variants can be used in the same application" ([README](https://github.com/fix8/fix8), [fix8.org](https://fix8.org/)) |
| **Chronicle FIX** | Build time: generated typed interfaces per schema | The schema | "supports multiple FIX schemas within a single engine" ([chronicle.software/fix-engine](https://chronicle.software/fix-engine/)) |
| **OnixS** | Run time | A dialect description in XML that adds messages, fields and groups, or turns a field into a group; per-session or engine-wide | [dictionary](https://ref.onixs.biz/net-core-fix-engine-guide/articles/fix-dictionary.html), [dialect description](https://ref.onixs.biz/net-fix-engine-guide/dialect-description.html) |
| **B2BITS FIX Antenna** | Run time, per session by dictionary id | Its own format, and **QuickFIX-format XML accepted natively** as a migration path | [dictionaries format](https://b2bits.atlassian.net/wiki/spaces/B2BITS/pages/6065406/FIX+Antenna+C+.NET+dictionaries+format) |
| **FerrumFIX (`fefix` 0.7.0)** | Either: generated code (`codegen` feature) or a runtime `Dictionary` | Any QuickFIX-style XML | `Dictionary::from_quickfix_spec(input) -> Result<Self, ParseDictionaryError>` ([docs.rs](https://docs.rs/fefix/latest/fefix/struct.Dictionary.html)). No page found on adding a venue dialect specifically |
| **`fixer-gen`** | Build time only | A FIX XML | "custom FIX applications may generate source specific to the FIX spec of that application using the `fixer-gen` tool" ([lib.rs](https://lib.rs/crates/fixer)) |
| **IronFix** | Run time | QuickFIX XML via `Dictionary::from_quickfix_xml`; only FIX 4.4 is embedded ([repository](https://github.com/joaquinbejar/IronFix)) | |

**What the table settles.** Two families exist. Run-time dictionaries (QuickFIX, OnixS, B2BITS,
IronFix, `fefix`'s dynamic mode) let an operator change a dialect without a rebuild and pay a
table lookup through a data structure built at start-up. Build-time generation (Fix8, Chronicle,
QuickFIX/J's typed classes, `fixer-gen`, `fefix`'s `codegen`) turns the dialect into code and
needs a rebuild per change. **The QuickFIX XML format is the lingua franca** — accepted by six of
the nine above — so whatever fixbolt does, a user's existing QuickFIX dictionary should be an
input it accepts. Two of the build-time engines (Fix8, Chronicle) state that several dialects
live in one application.

`[measured 2026-09-26]` in this repository: `ValidateUserDefinedFields=N` already exists
(`DictionaryChecks::skipping_user_defined_fields`, `crates/session/src/lib.rs`), so a plain
custom tag at or above 5000 can be let through today. What cannot be expressed without replacing
the whole XML through `NANOFIX_FIX44_XML` is a custom repeating group, a new enum value on a
standard field, a new message type, a venue-required field, or a custom tag below 5000.

## 3. Licence models for a commercial open-source library

| Model | Mechanism | Source |
|---|---|---|
| Dual licence (copyleft + commercial) | Free under (A)GPL/LGPL; a commercial licence for closed use. Qt; MongoDB (AGPL, later SSPL) | [architecture-weekly](https://www.architecture-weekly.com/p/why-open-source-isnt-always-fair); [MongoDB SSPL FAQ](https://www.mongodb.com/legal/licensing/server-side-public-license/faq); [LWN](https://lwn.net/Articles/790379/) |
| Business Source License 1.1 | Source visible; competing hosted/embedded offerings barred; each version converts to MPL 2.0 after four years (HashiCorp) | [hashicorp.com/bsl](https://www.hashicorp.com/en/bsl) |
| Functional Source License | Anything except undermining the producer; converts to Apache-2.0 or MIT after two years (Sentry) | [fsl.software](https://fsl.software/); [Sentry blog](https://blog.sentry.io/introducing-the-functional-source-license-freedom-without-free-riding/) |
| Permissive core, paid support or editions | Implicit in the vendor models of §1; not separately researched | — |

**Relicensing code already released permissively is not unilateral.** Changing the licence of
contributed code needs each copyright holder's consent; projects doing it collect consent one
contributor at a time ([BurntSushi/chan#4](https://github.com/BurntSushi/chan/issues/4),
[frankmcsherry/columnar#2](https://github.com/frankmcsherry/columnar/issues/2)). Code already
published under MIT/Apache-2.0 stays available under those terms to whoever received it.

**CLA versus DCO decides whether the option survives.** A DCO (`Signed-off-by:`) is
inbound = outbound: each contribution arrives under the project's current licence only, so a
later relicence needs every contributor again. A CLA grants the project a licence broad enough to
relicense and to ship the contribution in a commercial edition, and is the usual tool of a
dual-licence or open-core business
([tenthirtyam.org](https://tenthirtyam.org/dispatches/2026/04/08/dco-vs-cla-managing-contribution-agreements-in-open-source/),
[opensource.com](https://opensource.com/article/18/3/cla-vs-dco-whats-difference),
[FINOS](https://osr.finos.org/docs/bok/Artifacts/CLAs-And-DCOs)). The cost of a CLA is real: some
contributors and employers refuse to sign one on principle
([opensource.com](https://opensource.com/article/19/2/cla-problems)). **The choice has to be made
before the first outside contribution is merged**; after it, the DCO outcome is the default for
that contribution.

## 4. Documentation structure

- **Diátaxis** ([diataxis.fr](https://diataxis.fr/), [start here](https://diataxis.fr/start-here/)):
  tutorials (learning by doing), how-to guides (a task, for a competent reader), reference
  (accurate, complete, dry), explanation (the why). Each needs a different writing style.
- **`ARCHITECTURE.md`** ([matklad, 2021](https://matklad.github.io/2021/02/06/ARCHITECTURE.md.html)):
  for a 10k–200k-line project, a short file next to `README.md` and `CONTRIBUTING.md`; finding
  *where* to change something is the expensive part for an occasional contributor; document only
  what rarely changes; a codemap of names to search for rather than links that rot.
- **rust-analyzer** ([architecture.md](https://github.com/rust-lang/rust-analyzer/blob/master/docs/dev/architecture.md)):
  *Architecture Invariant* call-outs, including for things deliberately **absent** from the code.
- **mdBook facts that decide the layout** (mdBook v0.5.4, released 2026-07-06):
  - Only chapters listed in `SUMMARY.md` are rendered; a markdown file not listed is not parsed
    ([rust-lang/mdBook#702](https://github.com/rust-lang/mdBook/issues/702), open on 2026-09-26).
  - "Relative links that end with `.md` will be converted to the `.html` extension"; links to
    `README.md` become `index.html` ([markdown format](https://rust-lang.github.io/mdBook/format/markdown.html)).
    Links to `README.md` inside non-root folders have had bugs
    ([#984](https://github.com/rust-lang/mdBook/issues/984), [#1920](https://github.com/rust-lang/mdBook/issues/1920)).
  - Heading ids are generated by lower-casing and replacing spaces with dashes
    ([markdown format](https://rust-lang.github.io/mdBook/format/markdown.html)); whether every
    id matches GitHub's for the headings this repository uses (em dashes, backticks, section
    numbers) is **not established** — a check has to measure it.
  - `[output.html.redirect]` maps an old path to a new URI, for moved pages
    ([renderers](https://rust-lang.github.io/mdBook/format/configuration/renderers.html)).
  - `mdbook test` compiles Rust samples with `rustdoc`; external crates need `-L` library paths
    ([test](https://rust-lang.github.io/mdBook/cli/test.html)).
  - The mdBook CI page recommends a pinned install and mentions `mdbook-linkcheck2` for broken
    links ([continuous integration](https://rust-lang.github.io/mdBook/continuous-integration.html),
    [mdbook-linkcheck2](https://github.com/marxin/mdbook-linkcheck2)); GitHub's own starter
    workflow deploys with `actions/upload-pages-artifact` and `actions/deploy-pages`
    ([starter workflow](https://github.com/actions/starter-workflows/blob/main/pages/mdbook.yml)).
- **Security policy.** GitHub's private vulnerability reporting is a repository setting,
  separate from `SECURITY.md`
  ([configuring](https://docs.github.com/code-security/security-advisories/working-with-repository-security-advisories/configuring-private-vulnerability-reporting-for-a-repository)).
- **Not found:** the internal page structure of the tokio and rustls documentation sites.

## 5. "Why Rust" — what may be cited, and what may not

**May be cited:**

- Aquilina, Budish & O'Neill, *Quantifying the High-Frequency Trading "Arms Race"*, QJE 137(1),
  2022: modal race 5–10 µs, races ~20 % of volume, stakes "on the order of $5 billion per year
  in global equity markets" ([OUP](https://academic.oup.com/qje/article/137/1/493/6368348),
  [author's page](https://ericbudish.org/publication/quantifying-the-high-frequency-trading-arms-race/)).
  For *why microseconds matter*, never for Rust versus C++.
- matklad, *Why Not Rust* ([2020](https://matklad.github.io/2020/09/20/why-not-rust.html)):
  complexity costs programmer time; compile times; one implementation and LLVM (at the time);
  tooling gaps; FFI friction; `memcpy` moves; no formal memory model for `unsafe`.
- Databento, *Rust vs C++ for trading systems* ([blog](https://databento.com/blog/rust-vs-cpp)):
  no numeric claim; C++'s edge is ecosystem, native venue APIs, FPGA and bypass toolchains, hiring.

**Must not be cited** — uncited, unverifiable, and in one case contradicted by primary sources:
"Jane Street 50 ms → 8 µs" and "Jane Street adopted Rust" (Jane Street documents itself as an
OCaml shop: [janestreet.com](https://www.janestreet.com/technology/),
[ocaml.org](https://ocaml.org/success-stories/large-scale-trading-system)); "Jump Trading 30–50 %
fewer incidents"; "Tower Research adopted Rust"; "98.7 % of C++ latency". They circulate in
mechanicalsnail.com and hftadvisory.substack.com posts. **Any of these phrases in a pull request
is a finding.**

## Gaps, stated as gaps

- No reproduced competitor latency figure exists here, and this page makes none comparable.
- Rapid Addition's dictionary tooling: no public description found.
- Open-core as a separately sourced model: not researched.
- mdBook versus GitHub heading-id agreement on this repository's headings: unmeasured.
