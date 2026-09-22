# A `vendor/` tree fetched before SP2 was added fails the `fix50sp2` gates, and a symlinked worktree inherits it

`[2026-09-22]` `vendor/` is gitignored and fetched by `scripts/fetch-quickfix-assets.sh`. The
script grew `/src/C++/fix50sp2/` when phase 2 landed (2026-09-19), but a tree fetched **before**
that keeps `src/C++/fix44` and the four `.h` files only. On such a tree
`cargo test -p fixbolt-dict --tests --features fix50sp2` fails at build time:

```
cannot read .../vendor/quickfix/src/C++/fix50sp2: No such file or directory
```

The main working tree on the desk was in that state on 2026-09-22 — every `fix50sp2` gate run
here since 2026-09-19 that read green ran in CI, not on this tree. Worktrees that symlink the
main tree's `vendor/` (`../fb-ab-parse` did) inherit the hole; the P3 worktree of the
2026-09-22 closing plan hit it, fetched its own `vendor/`, and reported it.

## What to do

Re-run `scripts/fetch-quickfix-assets.sh` in the tree; its last lines print
`generated SP2 headers: 160`. Check `ls vendor/quickfix/src/C++/` shows `fix50sp2` before any
`--features fix50sp2` command is trusted from a local tree. A fetch is idempotent.

## Guarded by

Nothing yet. `scripts/check-feature-gated-tests-ran.sh` proves the named SP2 tests ran in CI,
which is why CI never saw this; no script checks that a local `vendor/` matches the fetch script
it was made by. A `vendor/quickfix/FETCHED-BY` stamp compared against the script's hash would
close it; open by omission, not decision.
