# cargo-semver-checks stops on a crate the baseline does not have

> `[measured 2026-09-27]` — `cargo-semver-checks` 0.50.0 on the desk, the commit that moved
> `fixbolt-metrics` and `fixbolt-store-sqlite` into the published set. Neither crate is in the
> baseline tag `v0.1.0`.

## The shape

`scripts/check-semver-against-tag.sh` runs `cargo semver-checks --workspace --baseline-rev
<tag>`. `--workspace` means every publishable member. When a member is publishable at `HEAD` but
absent from the tag, the tool does not skip it. It errors, and the whole run exits 101:

```text
error: failed to retrieve local crate data from git revision
    2: package `fixbolt-metrics` not found in …/target/semver-checks/git-v0_1_0/a0056e15…
check-semver-against-tag: FAIL — cargo-semver-checks itself found a real semver break against v0.1.0 (exit 101); read the output above
```

That red is not a semver break. It is a crate with nothing to compare against. The six crates
that do have a baseline still printed `196 checks` each, and the run's exit status was 101 all
the same. So the job fails for a reason its own message misnames. The first new crate that joins
the published set, before any tag carries it, turns the blocking `semver` job red.

## The fix

The script now reads the workspace members and their `publish` field. Every publishable member
whose `Cargo.toml` is absent from the tag (`git cat-file -e <tag>:<dir>/Cargo.toml`) is passed as
`--exclude <name>` and named on one line:

```text
check-semver-against-tag: not in v0.1.0, excluded (nothing to compare against): fixbolt-metrics fixbolt-store-sqlite
check-semver-against-tag: OK — 6 crates checked against v0.1.0, cargo-semver-checks exit 0
```

A crate in the script's own `PUBLISHED` list that is missing from the tag is a FAIL, never an
exclusion. So the exclusion cannot hide one of the crates the check exists to compare. The two
new crates join `PUBLISHED` in the commit after the first tag that carries them.

## The guard

This check has no fixture of its own. The script is the guard, and the CI `semver` job runs it
on every pull request. Red: the exit 101 above, on the commit before the fix. Green after the
fix: the two lines above.

**What it cannot see:** a crate renamed between the tag and `HEAD`. Under its new name it is
absent from the tag, and it would be excluded rather than compared.
