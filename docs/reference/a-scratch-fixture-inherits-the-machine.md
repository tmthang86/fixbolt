# A scratch fixture inherits the machine, not the project

> **What this page is:** a gate that was green in CI and red on the developer
> machine, for a reason that had nothing to do with what it tested. Found
> `[2026-08-31]` while walking `CLAUDE.md` §9's checklist before closing a plan,
> which is the only reason it was found at all.

## What happened

`scripts/check-lint-config.sh` proves by reversal that the workspace's clippy
configuration really denies `unwrap()`, `expect()` and `panic!()` — non-negotiable
7. It does that by writing a throwaway crate, copying the workspace's own
`[lints.*]` blocks into it verbatim, and running clippy over it twice: once with
the three constructs present and once without.

The throwaway crate is written to `mktemp -d`. **`rust-toolchain.toml` does not
reach there**, and on a machine with no `rustup default` configured — which the
owner's desktop is, because every project it builds pins its own — `cargo clippy`
never ran at all:

```
FAIL: the workspace lints do not deny: unwrap_used expect_used panic
--- clippy output ---
error: rustup could not choose a version of cargo to run, because one wasn't
specified explicitly, and no default is configured.

FAIL: the workspace lints reject code that follows the rule.
--- clippy output ---
error: rustup could not choose a version of cargo to run, ...
```

Both halves failed, exit 1, and the transcript's headline said the workspace lints
were broken. **They were not.** The same script is green in CI, where the runner
installs a default toolchain as a matter of course.

The fix is one line — copy `rust-toolchain.toml` into the scratch crate — and it
closes a second, quieter hole at the same time. `rust-toolchain.toml`'s own
comment in this repository says the pinned toolchain is load-bearing, because
`-D warnings` denies every lint the *installed* clippy knows, including ones
released after the code was written. On any machine that did have a default, the
gate had been checking the workspace's lint config against **a different compiler
from the one the workspace uses**. Nobody would have noticed until the two
disagreed.

Proven by reversal after the fix: commenting out `unwrap_used = "deny"` in
`Cargo.toml` makes it exit 1 with `FAIL: the workspace lints do not deny:
unwrap_used` — naming the one lint removed, not all three — and restoring it makes
it exit 0.

## The generalisation

`[to testing-skills]` — **a fixture built outside the project directory inherits
the machine's defaults, not the project's.** Version pinning is almost always
scoped to a directory: a toolchain file, a virtualenv, a lockfile, a `.nvmrc`, a
container image, a `.env`. A test that constructs a scratch project in a temp
directory to exercise the real one steps outside every one of those, silently, and
what it then measures is whatever the host happens to have.

Two failure directions, and the second is the dangerous one.

- **The host has nothing.** The tool does not run, and if the harness reads exit
  status it reports the *system under test* as broken. That is what happened here:
  a false red, loud, wrong about the cause, and cheap only because somebody read
  the output instead of the exit code.
- **The host has something different.** The tool runs, the gate goes green, and
  it green-lights a configuration against a version the project never uses. Silent,
  indefinite, and it only surfaces the day the two versions disagree — at which
  point the gate is the last place anyone looks, because it has been passing for
  months.

**The cheap defence is to copy the pinning artefact into the fixture and to say in
a comment which one it is.** The cheaper defence, where it fits, is not to leave
the project at all: build the fixture inside the tree, under an ignored directory,
so every scoped setting still applies.

**The audit that follows a finding like this is small and worth doing.** Three
other scripts in this repository use `mktemp -d`; all three use it only for output
files and run a binary or a build from inside the tree, where rustup still walks
up to the pinning file. One script was affected, and now the class is written down
rather than the instance.

**And the reason this was found is worth as much as the finding.** Nothing in CI
would ever have shown it, because CI is precisely the environment where it passes.
It surfaced because a plan's closing checklist requires every gate to be run *on
the machine doing the work* and its output read — not its exit status, and not its
last run somewhere else.
