# `--no-default-features` does not mean what it reads as in a workspace

`[measured 2026-08-30]` **A CI job named "Builds with nothing optional installed" was green
about a build in which the optional thing was installed.** Not intermittently — always, from the
moment a second workspace member depended on the crate under test.

This is the trap `CLAUDE.md` §10 calls *"a check proves nothing until something reads it"*, one
step further along: the check ran, something read it, and it was measuring a different build.

## What happened

`crates/engine` took its first optional dependency:

```toml
[features]
default = ["standard"]
standard = ["dep:libc"]

[dependencies]
libc = { version = "0.2", optional = true }
```

CI's guard for non-negotiable 6 was, and had been for as long as the job existed:

```
cargo test --all --no-default-features
```

The expectation was an engine with no external dependency. What actually got built:

```
$ cargo tree --workspace --no-default-features -e normal -i libc
libc v0.2.189
└── fixbolt-engine v0.0.0 (/home/user/fixbolt/crates/engine)
    └── fixbolt-w2w v0.0.0 (/home/user/fixbolt/tools/w2w)
```

`tools/w2w` is a workspace member. It depends on `fixbolt-engine` **with default features**.
`--no-default-features` applies to the *packages cargo was asked to build*, and cargo then
**unifies features across everything in one invocation** — so `w2w`'s ordinary dependency edge
switched `standard` back on for the whole build. The flag under test was turned back on by a
sibling crate, and nothing said so.

## How it was noticed, which is the uncomfortable part

Not by the gate. By a **test count**.

`crates/engine/tests/standard.rs` carries `#![cfg(all(feature = "standard", unix))]`, so with the
feature off it should compile to an empty binary. The run with `--no-default-features` was
expected to report **210** passing tests against the default run's 214. It reported **214**.

Four tests are what stood between this and shipping. Had the new module carried no tests of its
own — the ordinary case — the counts would have matched, the job would have stayed green, and
nothing would have pointed at it.

## The rule

> **`--no-default-features` is only meaningful at the scope of one package.** In a workspace it
> is a statement about the invocation, not about any crate in it, and any other member that
> depends on the crate with defaults on silently repeals it.

`-p <crate> --no-default-features` is the scope where the flag means what it reads as.

## The fix, and what makes it a gate rather than a hope

`scripts/check-no-optional-deps.sh`, run by CI beside the workspace-wide command rather than in
place of it. Two assertions per crate, and the second exists because the first alone is empty:

1. `cargo tree -p <crate> --no-default-features -i <dep>` must report the dependency **absent**.
2. `cargo test -p <crate> --no-default-features` must **build and pass** — a dependency missing
   from a crate that does not compile proves nothing.

**Proven by reversal.** Removing `optional = true` from `libc` turns assertion 1 red with the
graph printed, and restoring it turns it green. That reversal was run, and its output read,
before the gate was believed.

Note the shape of the reversal that would *not* have worked: deleting `#[cfg(feature = ...)]`
from the `mod` declaration. That is the failure the job was written for, and this trap is a
different one — the manifest and the `cfg` were both correct the whole time.

## What it costs elsewhere

Every future optional dependency in this workspace inherits this. `scripts/check-no-optional-deps.sh`
carries a list of `crate:dependency` pairs, and **the list is the thing that goes stale**: a
crate that takes an optional dependency and is not added to it is back to being guarded by a
command that cannot see it.

`[to testing-skills]` The generalised case: **a build-configuration flag whose scope is wider
than the thing it names.** A test suite run under "feature off" that a neighbouring target turns
back on is a false green with no symptom — the suite passes, the binary is correct, and the
configuration under test was never built. The tell here was a test count that did not drop when
a `cfg`-gated file should have vanished; the durable fix was to assert the *configuration*
directly (is this dependency in the graph?) rather than to infer it from the fact that the tests
passed.
