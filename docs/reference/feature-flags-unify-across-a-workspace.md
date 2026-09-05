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

---

# Second case: a `cfg(feature = …)` naming a feature the crate does not have

`[measured 2026-08-30]` **`tools/w2w --mode standard` was accepted, printed `mode: standard`,
and ran nothing.** The binary worked. The mode did not exist.

`src/main.rs` selected the blocking strategy like this:

```rust
#[cfg(all(feature = "standard", unix))]
Mode::Standard => pump(acceptor, &stop, Block::new(16)),
#[cfg(not(all(feature = "standard", unix)))]
Mode::Standard => { eprintln!("w2w: this build has no standard mode"); }
```

`w2w` depends on `fixbolt-engine`, which *does* have a `standard` feature, on by default. But
**features are per-crate, and a `cfg` never reaches into a dependency's.** `w2w`'s own manifest
declared no features at all, so `feature = "standard"` was simply false, every such branch took
its `else`, and the timed loop never ran.

## Why the symptom was so quiet

The banner still printed, because it is printed before the branch. The process exited 0, because
nothing failed. The only tell was that the latency block was **absent** — and a reader skimming
for "did it run" sees the mode line and stops.

## Two things that could have caught it, and what each was worth

- **`cargo build` warned.** `unexpected_cfg_condition_value`, on by default, pointing at the
  exact line: *"no expected values for `feature`… consider adding `standard` as a feature in
  `Cargo.toml`"*. It was in the output the whole time. `cargo clippy -- -D warnings` would have
  turned it into an error and CI would have gone red on the next push.
- **Running the thing and reading what came back** caught it first, and immediately.

Both work. Note which one needed no infrastructure at all.

## The rule

> **A `cfg(feature = "x")` is a question about the crate it is written in.** Enabling `x` on a
> dependency does not make it true. A binary that wants to switch on a dependency's feature must
> declare and forward its own:
>
> ```toml
> [features]
> default = ["standard"]
> standard = ["fixbolt-engine/standard"]
> ```

`CLAUDE.md` §2 rule 6 already guards the mirror image — *a feature in the manifest whose `mod` is
not behind `#[cfg]`*, which makes the crate unbuildable for everyone but its author. **This is
the same mistake from the other side, and it fails in the opposite direction: everything builds,
and a code path silently disappears.**

## And the gate now refuses to assume it ran what it asked for

`scripts/check-no-kernel-sleep.sh` invokes this binary twice, once per mode, and its whole
meaning rests on the second run behaving differently from the first. Had that script existed in
this shape a day earlier it would have been **green about two runs of the same mode**.

So `w2w` now prints `mode: <name>` on its own line, and the script **reads it back and fails if
it is not the mode it asked for**. Proven by reversal: asking for `hft` while passing
`--mode yield` gives `w2w ran mode 'yield' when 'hft' was asked for`, exit 1.

`[to testing-skills]` The generalised case: **a harness that selects a variant by flag, and
verifies the result without verifying the selection.** Any A/B gate — two modes, two
configurations, two algorithms — is only as good as its evidence that arm B was actually the one
that ran. The cheap fix is for the thing under test to *state* which arm it took, and for the
harness to check that statement rather than its own intent.

---

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

---

## The other direction: a target the local command never compiles

`[measured 2026-09-05]` The cases above are a feature that is **on** when the command said off.
This is a feature that is **off** on the developer's machine and on in CI, and it cost a red
build in wave B.

`crates/engine/tests/shard_wire.rs` destructures `presession::Progress` **field by field, with
no `..`**, and its comment says exactly why: *"a new way for this stage to dispose of a socket
breaks the build here rather than disappearing"*. That guard is a repair, not a precaution — a
counter added in 2026-09-01 was read by four fields out of five and two connections vanished
with every assertion still green (CI run 33509748294).

Wave B added a sixth field, `Progress::unframeable`. **The guard fired exactly as designed.** It
fired in CI and nowhere else:

```text
error[E0027]: pattern does not mention field `unframeable`
   --> crates/engine/tests/shard_wire.rs:259:13
```

`cargo clippy --all-targets -- -D warnings` was clean on the laptop, three times, on three
commits. That command compiles every target of the **default feature set**, and `shard_wire.rs`
is behind `--features affinity`. **`--all-targets` is not `--all-configurations`, and the two
read as the same promise.** CI's own line is
`cargo clippy --all-targets --features affinity -- -D warnings`, one flag longer, and that flag
is the whole difference between a guard that ran and a guard that existed.

The change was also *semantic* and not only a pattern: that file asserted `gone == 1` for
`1d_InvalidLogonLengthInvalid.def`, whose `9=40` is a lie the framer takes at its word — which
is precisely the connection that became `unframeable`. Left to `..`, the count would have moved
columns silently, which is the third occurrence of the very failure the destructure was written
to stop.

`[to testing-skills]` **A guard only guards the configurations something compiles it in.** The
strongest structural check in a codebase — an exhaustive pattern, a `#[non_exhaustive]` match, a
compile-time assertion — is inert in every build that does not include the file it lives in, and
nothing about running the usual local command reveals that. Two things follow:

- **Know which of your checks are behind a feature**, and run that configuration before pushing
  rather than after. Here: `cargo clippy --all-targets --features affinity` and
  `cargo test -p fixbolt-engine --features affinity`, the two lines CI runs and a laptop does
  not.
- **Prefer to state the missing configuration rather than remember it.** The failure mode is not
  that the guard is weak; it is that *"I ran the checks"* and *"CI runs the checks"* are
  different sentences, and only one of them has the flag in it.
