# A guard was built to prove a gate had looked, and it asked the same short question

`[measured 2026-09-09]` Found while adding an optional dependency tree in wave B plan 4.
`STATUS.md` item 57.

## The setup

A licence gate runs `cargo deny check` over the dependency graph and fails on anything not on an
allow-list. `[measured 2026-09-08]` that gate was found to be **silently incomplete** — it did not
judge the workspace's dev-dependencies, and said `ok` rather than saying so. An `MPL-2.0` crate in
`[dev-dependencies]` was resolved by `cargo metadata`, printed by `cargo tree`, and never seen.

The fix was not to argue with the checker. It was to add a **guard**: take the list of crates the
checker says it judged, take the list cargo itself resolves, and fail when they differ. An
invisible hole becomes a loud one. That guard has been green ever since, reporting `11 and 11`.

## What went wrong

The next change put two dependencies behind an optional feature. Measured on that workspace:

```
cargo deny list                    11 crates — `ring` absent
cargo deny --all-features list     36 crates
cargo tree --workspace             11 crates
cargo tree --workspace --all-features   36 crates
```

The guard's two inputs are the first and the third. **Both omit the same 25 crates**, so they
agree, so the guard passes, so it reports that the gate looked at everything cargo resolves — and
25 crates had not been compared by anybody.

And the punchline is that the gate itself was **fine**. Removing one licence from the allow-list
made `cargo deny check` go red on a crate in the optional tree, with or without the flag. The
checker was judging 36. Its own `list` subcommand reports 11. **The guard's premise — that `list`
reports what `check` looked at — was never true**, and the guard could not discover that, because
it never asked anything `check` had answered.

## The shape

A guard was written to answer *"did the gate look at everything?"* and it answered it by asking
**the same question, of the same tool, in the same short form**. It is not that the guard was
weak; it is that it and the thing it watched shared a blind spot by construction, so its agreement
carried no information at all. It would have gone on printing `11 and 11` however many optional
trees were added.

Both sides now ask `--all-features`, and read `36 and 36`.

## `[measured 2026-09-09, the same day]` And then one dev-dependency proved all of it at once

The change that exposed the guard also added a **dev-dependency** — a certificate generator for a
test. Within one CI run it produced three separate findings, and they are only legible together:

1. **The advisory check found a real vulnerability in it.** `RUSTSEC-2026-0009`, stack exhaustion
   via a deprecated parsing feature, two levels down. Fixed by pinning the patched version.
2. **The repaired guard went red, correctly**, at 36 against 45. The licence check does not judge
   dev-dependencies, so nine crates were genuinely unjudged — the exact hole the guard was built
   for, firing the first time a dev-dependency arrived.
3. **The same binary was using three different graphs.** `list` reported 36. `check licenses`
   judged 36. `check advisories` judged all 45 — it is what found the advisory in #1. One tool,
   one workspace, one invocation style, three answers to "what is in this project?"

**Point 3 is the one to carry.** It was not documented, it is not intuitive, and it means *"the
tool checked it"* is not a single fact — it depends on which sub-check, and the sub-checks
disagree.

The resolution is worth stating too, because the tempting one is wrong. Widening the cargo side of
the guard would have made 45 match 45 and turned the light off — hiding the hole rather than
closing it, which is this page's whole subject. Instead the guard was **narrowed** to its honest
question (*did the tool judge everything it claims to judge?*), and the gap was **covered by a
second check that takes a different route entirely**: read the package graph from `cargo metadata`,
read the policy from the config file, compare them, and never call the tool at all.

That second check went red on its own first run — on three crates using the pre-SPDX `MIT/Apache-2.0`
spelling, which its parser did not split. A parser bug reported as a policy violation is the
loudest possible false positive, because it reads exactly like a real finding.

## What to take from it, part two

- **A gate is not one gate.** Before trusting "X checks this", find out which part of X, over which
  graph. Sub-checks of one tool can disagree about the input, silently.
- **When a guard goes red because it finally works, do not widen it until it is quiet.** Narrow it
  to what it can honestly assert, and cover the remainder with something that does not share its
  machinery.
- **A second opinion has to come from a different direction to be worth anything.** The covering
  check reads metadata and config; it can be wrong in its own ways, and that is the point — its
  failures are uncorrelated with the tool's.

## What to take from it

- **A guard over a gate must not be derived from the gate.** If the guard's input comes from the
  same tool, the same query, or the same configuration as the thing it checks, then whatever the
  gate is blind to, the guard is blind to as well — and it will report agreement, which reads
  exactly like a pass.
- **Two lists agreeing is only evidence if they were produced independently.** `11 and 11` looked
  like a strong result. It was two copies of one omission.
- **A tool's own "what did I look at?" output is a claim, not a measurement.** `cargo deny list`
  and `cargo deny check` disagreed about their own graph by 25 crates, in the same binary, on the
  same workspace.
- **The way this surfaced is worth copying**: nobody audited the guard. It was found by *adding
  something the guard should have noticed* and observing that nothing moved. A guard that does not
  react when you hand it the case it exists for has not been tested.

`[to testing-skills]`
