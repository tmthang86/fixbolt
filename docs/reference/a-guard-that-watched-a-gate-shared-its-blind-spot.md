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
