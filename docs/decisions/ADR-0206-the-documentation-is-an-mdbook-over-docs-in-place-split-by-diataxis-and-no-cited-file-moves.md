# ADR-0206 — The documentation is an mdBook over `docs/` in place, organised by Diátaxis, and no cited file moves

- **Status**: Proposed — 2026-09-26. *Revised in place 2026-09-26, while Proposed*, with the owner's
  answers to the plan's six questions: decision 8's last two bullets (Q2), decision 9 (Q3), and the
  contributor audience in decision 4 (Q1).
- **Date**: 2026-09-26
- **Deciders**: Tran Manh Thang (owner). Written by the architect (Opus).
- **Related**: [plans/2026-09-26-docs-for-embedders.md](../plans/2026-09-26-docs-for-embedders.md);
  [ADR-0207](ADR-0207-a-custom-dictionary-is-an-overlay-generated-in-the-users-build-into-the-users-own-type.md);
  [ADR-0104](ADR-0104-the-published-dictionary-is-quickfixs-xml-shipped-with-a-notice.md) decision 7
  (how "QuickFIX" may be named); [ADR-0099](ADR-0099-kernel-tcp-stays-the-default-and-the-headline-and-a-bypass-figure-is-a-second-labelled-row.md)
  (the headline sentence); [ADR-0161](ADR-0161-0-1-0-is-a-git-tag-not-a-crates-io-upload-and-the-stranger-and-the-semver-gate-read-the-tag.md)
  (no docs.rs); `CLAUDE.md` §2 non-negotiable 10, §4;
  [prior-art-for-embedders](../reference/prior-art-for-embedders.md) §4.

## Context

The owner wants fixbolt to read as a FIX engine framework that another company's Rust developers
embed, and later perhaps license commercially. The documents today are written for the people who
built it: `docs/` holds about 8 400 lines in twelve top-level files (`DESIGN.md` 2 094,
`GUIDE.md` 2 045, `CONFORMANCE.md` 1 131, …), plus 131 ADRs, 131 reference pages, 92 plans and
11 crate internals pages; `STATUS.md` alone is 6 518 lines. An outsider has no front page that
says what to read first, no statement of the invariants a contributor must not break, no
`CONTRIBUTING.md` or `SECURITY.md`, and no rendered site — ADR-0161 means there is no docs.rs
either.

What constrains any restructuring, `[measured 2026-09-26]` with `grep -rl` over `docs/`,
`crates/`, `scripts/`, `tools/`, `README.md`, `STATUS.md`, `CLAUDE.md`:

- `DESIGN.md` is named by **288** files; its section numbers are cited in prose ("`DESIGN.md` §8",
  "D3") about **776** times. `GUIDE.md` is named by **143** files, `CONFORMANCE.md` by **55**,
  including `.github/workflows/ci.yml` and `scripts/stranger-check.sh`, which reads
  `GETTING-STARTED.md`'s marked code blocks by path.
- Only **24** markdown links carry an `#anchor` into a top-level doc; the citations are prose.
  A prose citation cannot be redirected. **A moved section breaks it silently**, and
  `scripts/check-links.py` does not verify anchors at all (its header says so).
- **95** links from `docs/` point into `../crates/…`; rendered on a website, each is a 404.
- mdBook renders only chapters listed in `SUMMARY.md`; an unlisted markdown file is not parsed
  (rust-lang/mdBook#702, open). Relative `.md` links are rewritten to `.html`.

## Decision

1. **Diátaxis is the table of contents, not the file layout.** `docs/SUMMARY.md` groups every
   page under *Tutorials*, *How-to guides*, *Reference*, *Explanation*, and a fifth part,
   *Contributing and project records*. The two audiences, in order: the embedder (an outside Rust
   developer building an acceptor or initiator), then the contributor.
2. **mdBook, source in place.** `book.toml` at the repository root with `[book] src = "docs"`
   and the build directory under `target/` (already gitignored). The markdown in the repository
   stays the one authored copy (`CLAUDE.md` §8 of the global rules: one authored copy); the site
   is generated and never edited.
3. **No file that is cited moves, and no cited section is cut out of its file.** Link survival is
   achieved by not breaking links, not by repairing them: every existing path and every section
   number stays where it is. A large document is not split; a new page (a how-to, an explanation)
   is **added** and links *into* the existing section, and the existing section is never shortened
   into a pointer while anything cites it. Redirect stubs and `[output.html.redirect]` are
   therefore unused today and reserved for a future move, which needs its own ADR.
4. **New pages are new files in new directories**: `docs/how-to/`, `docs/explanation/`,
   `docs/index.md` (the book's landing page). Contributor files live at the repository root where
   GitHub looks for them — written for the **project team's developers**, not the public (owner,
   2026-09-26): `ARCHITECTURE.md` (matklad's shape: short, a codemap of names to search
   for, *Architecture Invariant* call-outs that include what is deliberately absent, each mapped to
   a `CLAUDE.md` §2 item), `CONTRIBUTING.md`, `SECURITY.md`. The book links to them.
   `CONTRIBUTING.md` states the rule that **contributions from outside the team are not accepted**
   until a future licensing ADR decides between a CLA and a DCO; that ADR is due before the first
   outside contribution is accepted, and does not block this one. `SECURITY.md` points only at
   GitHub's private vulnerability reporting (enabled by the owner); it names no email address.
5. **Links that leave `docs/` are rewritten at build time, not in the source.** A preprocessor
   in this repository, `scripts/mdbook-repo-links.py` (Python standard library only, the shape of
   `scripts/check-links.py`), rewrites any relative link that resolves outside `docs/`, or to a
   file under `docs/` that `SUMMARY.md` does not list (the plans), into a GitHub blob URL at the
   commit being built. The source keeps relative links, so `check-links.py`'s rule "a file in this
   repository is linked by relative path" is unchanged.
6. **What is in the book, and what is not.** In: every top-level `docs/*.md`, `docs/how-to/`,
   `docs/explanation/`, `docs/internals/`, every ADR, every reference page. Out: `docs/plans/`
   (Vietnamese, addressed to the owner, internal by `CLAUDE.md` §6), `STATUS.md`, `CLAUDE.md` —
   reachable from the site as links to GitHub. The ADR and reference parts of `SUMMARY.md` are
   **generated** by `scripts/gen-book-summary.py` from the directories; `--check` mode fails CI
   when a file is missing from `SUMMARY.md` or listed but absent. The hand-written part of
   `SUMMARY.md` is the Diátaxis part.
7. **Gates.** A CI job `book` on every pull request: installs a pinned mdBook binary by checksum,
   runs `scripts/gen-book-summary.py --check`, `mdbook build` (exit code read, and the build log
   searched for mdBook's own warnings, which do not fail the build by themselves), then an
   anchor-aware link check over the rendered HTML. Deployment to GitHub Pages is a separate job,
   on push to `main` only, with `actions/upload-pages-artifact` and `actions/deploy-pages`.
   Every Rust code block in a tutorial or how-to is either a verbatim copy of a file that CI
   compiles (checked by `<!-- sample: <path> -->` markers, the mechanism
   `scripts/stranger-check.sh` already uses for `GETTING-STARTED.md`) or is fenced `text` with the
   reason written beside it. `mdbook test` is not used: it needs library paths to every dependency
   and would be a second, weaker compile of code CI already builds.
8. **House rules for the new explanation pages**, which a reviewer checks by hand:
   - **Licence-neutral.** A page states the licence the code is under today (MIT OR Apache-2.0,
     `fixbolt-dict` additionally under the QuickFIX Software License for its data, ADR-0104) and
     nothing about future licensing, commercial editions or pricing.
   - **Every fixbolt number obeys non-negotiable 10**; a page that repeats one links to where it is
     recorded with its machine and settings, never restates it bare.
   - **Another engine's latency figure appears only on the prior-art reference page**, labelled
     as that vendor's claim with the conditions the vendor stated, never beside a fixbolt number.
     The phrases listed as not citable in
     [prior-art-for-embedders](../reference/prior-art-for-embedders.md) §5 are a review finding
     wherever they appear.
   - **The "why fixbolt" page is a technical explanation, not a comparison** (owner, 2026-09-26): it
     explains fixbolt's own mechanisms — in-place parse into a borrowed view, no hot-path
     allocation, build-time tables with static dispatch, patched outbound templates, a pure
     session, inline dispatch, the mode split — and the costs each avoids, linking to where each
     figure is recorded. Another engine appears only as a sourced design fact where the explanation
     needs it ("QuickFIX reads its XML dictionary at run time, per its configuration page"); there
     is no ranking, no "faster than X", and no other engine's number on the page.
   - **This keeps the page inside ADR-0104 decision 7**: "QuickFIX" appears as a sourced fact, which
     decision 7 allows, and never as an endorsement, a compatibility badge or a comparison. Held by
     machine — `scripts/check-doc-claims.sh` fails a line that names another engine together with a
     comparative (`faster`, `slower`, `better`, `outperform`, `than`), the prior-art reference page
     exempted because it quotes vendors — and by hand, in the senior review of that pull request.
9. **This work is phase 5** (owner, 2026-09-26): a new phase in `PRD.md` §2 after phase 4 and before
   the kernel-bypass candidate. No accepted ADR needs superseding. ADR-0204 gave the bypass item
   "no phase number until the owner scopes a phase that includes it", which stays true — the
   candidate stays under *Later phases*, now noted as coming after phase 5. ADR-0204's *Context*
   sentence "Phase 5 is not scoped" was a fact on its date, and ADR-0141's "FIXP not before phase 5"
   still holds, because phase 5 contains no FIXP work. The placement is recorded in `PRD.md` §2
   (plan step 1a) and here.

## Consequences

**Good**

- Nothing that cites anything breaks: 288 + 143 + 55 files and ~776 prose citations keep
  resolving, because no path and no section number moves.
- An outsider gets one entry point, a reading order, and pages written for the job they came to
  do, without the project losing the dense documents its own work cites.
- The site cannot drift from the repository: it is built from the same files on every pull
  request, and the generated parts of `SUMMARY.md` are checked, not remembered.
- Code in tutorials and how-tos is compiled, so a sample that stops building turns CI red.

**Bad — and accepted**

- **The big documents stay big.** `GUIDE.md` and `DESIGN.md` remain 2 000-line chapters in the
  book; the new how-to and explanation pages are an easier way in, not a replacement. A future
  split needs a redirect plan and its own ADR.
- **Some duplication of intent.** A how-to and a `GUIDE.md` section cover the same ground from
  two directions. The rule that bounds it — a how-to lists steps and links to the constraint,
  never restates it — is a hand check, not a machine check.
- **Two link checkers.** `check-links.py` judges the source; the rendered-site check judges
  anchors and the rewrite. Either can be green while the other is red, and both must run.
- **A pinned mdBook and a link-check binary are new CI inputs**, updated by hand. mdBook's
  heading-id rule is not guaranteed to equal GitHub's for this repository's headings (em dashes,
  backticks, numbers); until measured, an anchor that works on GitHub may not work on the site.
- **Links to code go to GitHub, not to rendered API docs.** There is no docs.rs (ADR-0161); API
  reference in the book is "run `cargo doc --open`" plus the curated pointers, until rustdoc
  output is published beside the book (not decided here).
- **The Pages site is public from the first deploy**, and it is a second place outsiders read.
  Anything wrong on it is wrong in public; the same review applies as to the repository.
- **The plans are not in the book**, so a reader following a design decision back to its plan
  leaves the site for GitHub.

## Sources

- [Diátaxis](https://diataxis.fr/); matklad, [ARCHITECTURE.md](https://matklad.github.io/2021/02/06/ARCHITECTURE.md.html);
  [rust-analyzer architecture](https://github.com/rust-lang/rust-analyzer/blob/master/docs/dev/architecture.md).
- mdBook: [markdown and links](https://rust-lang.github.io/mdBook/format/markdown.html),
  [renderers and redirects](https://rust-lang.github.io/mdBook/format/configuration/renderers.html),
  [test](https://rust-lang.github.io/mdBook/cli/test.html),
  [continuous integration](https://rust-lang.github.io/mdBook/continuous-integration.html),
  [#702](https://github.com/rust-lang/mdBook/issues/702); latest release v0.5.4 (2026-07-06, GitHub API).
- GitHub [mdBook starter workflow](https://github.com/actions/starter-workflows/blob/main/pages/mdbook.yml);
  [private vulnerability reporting](https://docs.github.com/code-security/security-advisories/working-with-repository-security-advisories/configuring-private-vulnerability-reporting-for-a-repository).
- Counts in *Context*: `grep -rl` and `wc -l` on the working tree at `6c2192d`, 2026-09-26.
