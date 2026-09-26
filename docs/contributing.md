# Contributing

**Contributions from outside the project team are not accepted yet.** Accepting them waits for
a licensing decision between a contributor licence agreement and a Developer Certificate of
Origin, which will be recorded as an ADR before the first outside contribution is taken
([ADR-0206](decisions/ADR-0206-the-documentation-is-an-mdbook-over-docs-in-place-split-by-diataxis-and-no-cited-file-moves.md),
decision 4).

## Coming next

Three files at the root of the repository are being written as the next part of
[the documentation plan](plans/2026-09-26-docs-for-embedders.md) (an internal plan, in
Vietnamese). This page will link to them once they exist:

- `ARCHITECTURE.md`: a short map of the code by names to search for, and the invariants the
  code keeps, including what it deliberately does not have.
- `CONTRIBUTING.md`: building the workspace, running the gates, and how a change is proposed.
- `SECURITY.md`: how to report a vulnerability.

## What exists today

- **Building and testing the workspace**: [README.md](../README.md), *As a contributor: build the
  workspace*. The test suite needs `scripts/fetch-quickfix-assets.sh` run first; the crates an
  application depends on do not.
- **The engineering rules**: [CLAUDE.md](../CLAUDE.md). §1 is *plan first, then build*, §2 is the
  list of non-negotiables and the machine check behind each, and §7 is which tests a change must
  run.
- **How the system is built**: [DESIGN.md](DESIGN.md), by section; §4 holds the decisions.
- **Which file holds what**: [docs/internals/](internals/README.md), one page per crate.
- **Why each decision was made, and at what cost**: [docs/decisions/](decisions/).
- **Traps already paid for, and measured costs**: [docs/reference/](reference/).
- **Where the work stands**: [STATUS.md](../STATUS.md).
