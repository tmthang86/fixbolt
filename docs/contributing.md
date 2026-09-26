# Contributing

Changes to fixbolt come from the project team. **Contributions from outside the team are not
accepted yet**: that waits for a licensing ADR deciding between a contributor licence agreement
and a Developer Certificate of Origin, due before the first outside contribution is taken
([ADR-0206](decisions/ADR-0206-the-documentation-is-an-mdbook-over-docs-in-place-split-by-diataxis-and-no-cited-file-moves.md),
decision 4).

Three files at the root of the repository are the starting point:

- **[ARCHITECTURE.md](../ARCHITECTURE.md)**: a short map of the code by names to search for, and
  the invariants the code keeps — including what it deliberately does not have — each mapped to
  the rule and the check that holds it.
- **[CONTRIBUTING.md](../CONTRIBUTING.md)**: the toolchain, fetching the test oracles, the gates
  to run before a pull request, and how a change gets in — plan, approval, branch, draft pull
  request, green CI, review.
- **[SECURITY.md](../SECURITY.md)**: how to report a vulnerability (GitHub's private
  vulnerability reporting only), which versions are supported, and what is in scope.

Beyond those:

- **The engineering rules**: [CLAUDE.md](../CLAUDE.md) — §1 plan first, §2 the non-negotiables and
  the machine check behind each, §4 which document a change must update, §7 which tests a change
  must run.
- **How the system is built**: [DESIGN.md](DESIGN.md), by section; §4 holds the decisions.
- **Which file holds what**: [internals/](internals/README.md), one page per crate.
- **Why each decision was made, and at what cost**: [decisions/](decisions/).
- **Traps already paid for, and measured costs**: [reference/](reference/).
- **Where the work stands**: [STATUS.md](../STATUS.md).
