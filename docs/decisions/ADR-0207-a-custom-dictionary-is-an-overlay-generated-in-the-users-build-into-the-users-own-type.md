# ADR-0207 — A custom dictionary is an overlay, generated in the user's build into the user's own type

- **Status**: Proposed — 2026-09-26. *Revised in place 2026-09-26, while Proposed*, with the owner's
  answers: decision 8 (FIX 4.4 only) and the first *Bad* consequence (rebuild accepted).
- **Date**: 2026-09-26
- **Deciders**: Tran Manh Thang (owner). Written by the architect (Opus).
- **Related**: [plans/2026-09-26-docs-for-embedders.md](../plans/2026-09-26-docs-for-embedders.md);
  [ADR-0080](ADR-0080-the-dictionary-rides-the-encoding-and-a-fixt-session-is-one-table-built-from-two-xml-files.md)
  decision 1 (the dictionary rides the encoding; the registry picks among compiled encodings,
  never a dictionary at run time); [ADR-0081](ADR-0081-sbe-tables-come-from-this-repositorys-generator-and-the-oracle-is-the-specs-bytes-plus-sbe-tool-behind-a-script.md)
  (`sbe-gen`: a generator a user's `build.rs` calls, loaded twice); [ADR-0104](ADR-0104-the-published-dictionary-is-quickfixs-xml-shipped-with-a-notice.md)
  (three shipped XML files, byte-identical, under `NOTICE`); [ADR-0160](ADR-0160-six-crates-release-in-lockstep-and-the-packaged-sources-are-the-stranger-before-crates-io-is.md)
  (six crates in lockstep); [ADR-0161](ADR-0161-0-1-0-is-a-git-tag-not-a-crates-io-upload-and-the-stranger-and-the-semver-gate-read-the-tag.md);
  `DESIGN.md` §4 D3, D5; `CLAUDE.md` §2 non-negotiables 1, 5, 6, 7, 9;
  [fix44-dictionary-traps](../reference/fix44-dictionary-traps.md) *There is no user-defined tag range*;
  [a-bitset-keyed-by-tag-scales-with-the-highest-tag-not-the-field-count](../reference/a-bitset-keyed-by-tag-scales-with-the-highest-tag-not-the-field-count.md);
  [prior-art-for-embedders](../reference/prior-art-for-embedders.md) §2.

## Context

A firm connecting to a venue meets the venue's *rules of engagement*: fields the FIX standard does
not define (usually tags 5000–9999, sometimes above 10 000, occasionally below 5000), new enum values
on standard fields, venue-only repeating groups and message types, and fields the venue makes
required. QuickFIX users handle this by editing a copy of the XML and pointing `DataDictionary=`
at it, at run time, per session (QuickFIX configuration reference, quoted in the prior-art page).

What fixbolt offers today, `[measured 2026-09-26]` from the code:

- `ValidateUserDefinedFields=N` / `DictionaryChecks::skipping_user_defined_fields` lets a tag
  ≥ 5000 through unasked, and `allowing_unknown_msg_fields` relaxes *tag not defined for this
  message type*. Reading such a tag in a handler needs no dictionary. **That covers a plain custom
  tag and nothing else.**
- A custom repeating group cannot be parsed (the parser learns a group's delimiter and members from
  `codec::Dictionary`), a new enum value on a standard field is `373=5`, a new message type is
  `373=11`, a venue-required field is not required, and a custom tag below 5000 is `373=0`.
- The only other path is `NANOFIX_FIX44_XML`, which replaces the **whole** FIX 4.4 file for the
  **whole** build graph: the type is still called `Fix44`, every crate in the graph gets the
  dialect, and one binary can speak one dialect.
- `Session` and `Engine` are already generic over the encoding, whose `Dict` is any
  `codec::Dictionary + dict::Tables` (ADR-0080). **But the `fixbolt` facade is not**:
  `crates/library/src/app.rs:231` parses with `Fix44` and `crates/library/src/reply.rs:379-380`
  orders and encodes with `Fix44`; and every `serve*` / `connect_and_serve*` door builds its engine
  with the default encoding `TagValue<Fix44, N>`.
- The generator is `crates/dict/build.rs`, 1 595 lines, file-level
  `#![allow(clippy::indexing_slicing)]`, failing through `die()` = `process::exit(1)`. It emits
  code naming `crate::FieldType`, so its output cannot be `include!`d into another crate. It
  already merges two XML files with agreement checks (`merge_fields`, `merge_components`, for the
  FIXT 1.1 + FIX 5.0 SP2 pair, ADR-0080 decision 2).
- `ALLOWED` and `DEFINED_TAGS` are bitsets over `0..=max_tag`. FIX 4.4's highest tag is 956
  (15 words per message type, 93 message types). A single custom tag 20000 makes it 313 words:
  about 233 KB for `ALLOWED` instead of about 11 KB — static data, no run-time allocation, but
  cache footprint and binary size.

Prior art (prior-art page §2): run-time dictionaries (QuickFIX, OnixS, B2BITS, IronFix) and
build-time generation (Fix8's `f8c`, Chronicle, QuickFIX/J's typed classes, `fixer-gen`, `fefix`'s
`codegen`). QuickFIX-format XML is the common input. Fix8 and Chronicle state that several
dialects live in one application.

## Options

**A. A run-time dictionary** (load XML at start-up, per session, as QuickFIX does). Rejected:
ADR-0080 decision 1 says the registry picks among compiled encodings, never a dictionary at run
time; `Dictionary`/`Tables` are associated functions with no receiver, so a run-time table would
need a receiver or a global, and a lookup through a structure built at start-up is what D3 and the
static-dispatch design exist to avoid. Reversing that is a larger decision than this feature.

**B. An overlay through an environment variable into `fixbolt-dict`'s own build.** Smallest change:
`FIXBOLT_DICT_OVERLAY=venue.xml` merged onto `spec/FIX44.xml`, so `Fix44` itself becomes the
dialect and the facade works unchanged. Rejected: one dialect per build graph (cargo builds
`fixbolt-dict` once), a type named `Fix44` that is not FIX 4.4, the 59-definition gate's meaning
("59 / 59 on `Fix44TagValue`") silently depends on an environment variable, and a library crate
depending on fixbolt cannot carry its own dialect. (Cargo's old bug where `[env]` changes did not
rerun a build script, rust-lang/cargo#10358 and #14350, is fixed — both closed — so that is not a
reason.)

**C. User-crate build-time generation from an overlay, into the user's own type.** Chosen.

## Decision

1. **One generator, three callers.** The generator moves out of `crates/dict/build.rs` into
   `crates/dict/src/gen/`, lint-clean (no `panic!`/`unwrap`/`expect`, no panicking index —
   non-negotiable 7 and the indexing-debt ratchet apply, because it is now under `src/`), returning
   `Result<String, GenError>` where it used to `die`. It is loaded twice, `sbe-gen`'s pattern
   (ADR-0081): `build.rs` includes it by `#[path]` to generate `Fix44` (and, under `fix50sp2`, the
   pair) exactly as today, and the crate exposes it as `pub mod gen` behind a new **off-by-default**
   feature `gen` for a user's `build.rs`. The feature gates the `mod` declaration
   (non-negotiable 6); `roxmltree` (already the pinned build-dependency, `=0.20.0`) becomes an
   optional normal dependency under `gen` only. **No new crate**: a separate generator crate would
   have to join the lockstep release family (ADR-0160) because `fixbolt-dict`'s own `build.rs`
   would build-depend on it, or would need the three XML files moved out of `crates/dict/spec/`
   (ADR-0104 decision 2, non-negotiable 9). Both are larger than the feature.
2. **The refactor is proven to change nothing first.** The emitted `fix44.rs` and
   `fixt11_fix50sp2.rs` hash identical to the parent commit (ADR-0104 decision 8 (ii)'s gate,
   reused), before any overlay code exists.
3. **Input: QuickFIX-format XML, two shapes.**
   - **Overlay** onto the shipped FIX 4.4: a `<fix>` document whose `<header>`, `<messages>`,
     `<components>` and `<fields>` sections are each optional, merged by the same agreement rules the
     FIXT pair already uses. A field may be **added**; an existing field may gain `<value>`
     entries; a field that repeats an existing number must agree on name and type, and one that
     repeats a name must agree on number, **or the build fails naming both**. A message or component
     may be **added**; an existing message or component may gain fields, groups and components, and
     a gained field may be `required="Y"`. Nothing is removed or retyped by an overlay — a user who
     needs that supplies the whole file.
   - **Whole file**: a user's own complete QuickFIX FIX 4.4 XML (their existing customised copy),
     generated as-is.
   The shipped `spec/` files are read, never written (non-negotiable 9); the base text reaches the
   generator as `include_str!` from inside the `fixbolt-dict` package, so it works from the git tag
   and from a `.crate`.
4. **Output: the user's own zero-sized type.** `gen` writes one Rust file for the user to
   `include!` in a module of their choosing: the tables, a unit struct named by the caller, and its
   `impl codec::Dictionary` and `impl dict::Tables`. Paths in the emitted code are absolute through
   the facade (`::fixbolt::dict::…`), so a user depends on `fixbolt` at run time and on
   `fixbolt-dict` with `features = ["gen"]` only as a build-dependency. The emitted file carries a
   compile-time check that the generator's format version equals the runtime crate's, so a
   build-dependency at a different tag fails to compile with a sentence, not silently.
5. **The facade becomes generic over the dictionary, additively.** `fixbolt::dict` re-exports
   `Dictionary`, `Tables`, `FieldType`, `Fix44` and `TagValue`. `App`, `Handler`, `Reply` and
   `Incoming` gain a trailing type parameter `D` defaulting to `Fix44`, used wherever `Fix44` is
   named today. Three new doors, generic over the encoding `E`, are added beside the existing ones —
   `serve_over` (standard acceptor, with recovery and log), `serve_hft_over` (hft acceptor, with
   recovery and log) and `connect_and_serve_over` (initiator) — and every existing door keeps its
   signature. Other doors (sharded, TLS) stay FIX 4.4 until a user asks; a user who needs one today
   drives `fixbolt_engine::Engine` directly, which is already generic. Whether the additions pass
   `cargo semver-checks` as minor is **measured by the existing gate**, not assumed.
6. **Several dialects in one process are several engines**, one per dictionary type — ADR-0080
   decision 1 unchanged. A registry does not choose a dictionary per counterparty.
7. **Settings.** `UseDataDictionary`, `DataDictionary`, `TransportDataDictionary` and
   `AppDataDictionary` in a configuration file become a named refusal, `Problem::DictionaryIsBuildTime`
   (`Problem` is `#[non_exhaustive]`), whose sentence points at the how-to — where today they are
   *unknown key*. `ValidateUserDefinedFields` keeps its meaning: with a dialect that defines a tag,
   that tag is defined, and the knob governs only tags the dialect does not define.
8. **Scope of the first version: FIX 4.4 only** (owner, 2026-09-26) — an overlay onto
   `FIX44.xml`, or a whole FIX 4.4 file. Overlays onto the FIXT 1.1 + FIX 5.0 SP2 pair are out of
   scope (plan, *Ngoài phạm vi*); the generator's input type is shaped so a later plan can add
   them.

## Consequences

**Good**

- The dialect is code: validation, group parsing and field order for a venue's own fields come
  from generated tables exactly as FIX 4.4's do — D3 and non-negotiable 5 hold for the user's type
  with no new rule, and nothing is looked up through a start-up structure.
- A QuickFIX user brings the XML they already have, whole or as the diff.
- Several venues' conflicting uses of the same tag number live in one binary, each in its own type
  and its own engine, which neither option B nor QuickFIX's global field numbering gives.
- The 59 definitions keep running against `Fix44`, whose generated bytes are proven unchanged;
  the overlay path is tested against the same corpus with an empty overlay.
- `Fix44` and every existing door keep their exact signatures.

**Bad — and accepted**

- **A dialect change is a rebuild — accepted by the owner on 2026-09-26.** QuickFIX's operator
  can swap a file and restart; a fixbolt user rebuilds. For a firm that treats venue onboarding as
  a deploy this is the norm (Fix8 and Chronicle work the same way); for one that expects an
  operator to edit XML, it is a real loss, and the how-to and the "why fixbolt" page say so.
- **The largest cost is the refactor**: 1 595 lines of generator rewritten to the library lints
  (errors as values, no panicking index), under a byte-identical-output gate. It touches no hot
  path and no behaviour, and it is still the riskiest step in the plan.
- **API surface grows**: a type parameter on four facade types, three new doors, a re-export
  module, a feature, a `GenError`. Each is public API that the semver gate then holds.
- **A high custom tag inflates the static tables** by `(max_tag + 1) / 64` words per message type
  — ~233 KB for one tag at 20 000. Static, allocation-free, but paid in binary and cache; the
  generator prints the size so a user sees it.
- **The QuickFIX licence reaches the user's generated tables** (derived from the shipped
  `FIX44.xml`, ADR-0104), and, for a whole-file dictionary, whatever licence the user's own copy is
  under. The how-to states it; the obligation is the user's.
- **Two copies of `fixbolt-dict` compile** in a user's build (host, for `build.rs`; target, through
  `fixbolt`), and must be the same tag; the version check turns a mismatch into a compile error,
  which is the best that can be done.
- **No acceptance corpus exists for a dialect.** The corpus proves an empty overlay changes nothing;
  what an overlay *adds* is tested only by this repository's own invented fixture, never by a
  venue's rules of engagement (which may not be committed here).

## Sources

- QuickFIX C++ [configuration](https://quickfixengine.org/c/documentation/getting-started/configuration.html)
  (`UseDataDictionary`, `DataDictionary`, `TransportDataDictionary`, `AppDataDictionary`,
  `ValidateUserDefinedFields`), read 2026-09-26.
- QuickFIX/J [customising-quickfixj.md](https://github.com/quickfix-j/quickfixj/blob/master/customising-quickfixj.md)
  and [quickfixj-codegenerator](https://github.com/quickfix-j/quickfixj-codegenerator).
- Fix8 [README](https://github.com/fix8/fix8) and [fix8.org](https://fix8.org/);
  Chronicle [FIX engine](https://chronicle.software/fix-engine/);
  OnixS [dialect description](https://ref.onixs.biz/net-fix-engine-guide/dialect-description.html);
  B2BITS [dictionaries format](https://b2bits.atlassian.net/wiki/spaces/B2BITS/pages/6065406/FIX+Antenna+C+.NET+dictionaries+format);
  `fefix` 0.7.0 [`Dictionary`](https://docs.rs/fefix/latest/fefix/struct.Dictionary.html);
  [`fixer`](https://lib.rs/crates/fixer); [IronFix](https://github.com/joaquinbejar/IronFix).
- rust-lang/cargo [#10358](https://github.com/rust-lang/cargo/issues/10358) (closed 2025-01-01) and
  [#14350](https://github.com/rust-lang/cargo/issues/14350) (closed 2024-08-04), state read with
  `gh api` on 2026-09-26.
- Code facts in *Context*: the working tree at `6c2192d`.
