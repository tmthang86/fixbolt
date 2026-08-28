# Changelog

Notable changes to the published crates. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versioning will follow
[Semantic Versioning](https://semver.org/spec/v2.0.0.html) from the first release.

**Scope, stated so this file does not drift into being a second `STATUS.md`:** it records
changes to the **public API and observable behaviour of released crates**, and nothing else.
Decisions live in [docs/decisions/](docs/decisions/); where the work stands lives in
[STATUS.md](STATUS.md); what is about to be built lives in [docs/plans/](docs/plans/). A thing
that has not shipped does not belong here — `CLAUDE.md` §4: one rule, one place.

## [Unreleased]

**Nothing has been released, and no crate exists.** `Cargo.toml` declares `members = []`, and
that is accurate rather than a placeholder: `CLAUDE.md` §1 adds crates one at a time, each
behind an approved plan.

The first entries under `Added` will be `codec` and `dict`, per
[the approved plan](docs/plans/2026-08-27-codec-dict.md).

The API those crates will publish — `parse_into`, `Parsed`, `ParseError`, `FieldIndex<N>`,
`MessageView`, `Template`, and the `Dictionary` trait — is already decided and open to review
in [DESIGN.md §4](docs/DESIGN.md#4-the-decisions-that-shape-it) D2, D3 and D9. It sits there
and not here because it is a design, not yet a change.

**Before the first publish, the crate names change.** `nanofixengine` is a placeholder taken
to clear a collision with `matthart1983/nanofix`; the shortlist is in [STATUS.md](STATUS.md).
Renaming after a crates.io publish is not possible, so it happens before.
