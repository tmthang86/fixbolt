# A dependency gate that was green about a graph one crate short

`[measured 2026-09-08]` · `cargo-deny` 0.20.2 · **`[to testing-skills]`**

## What happened

A licence and advisory gate was added to a workspace that is about to be
published: `deny.toml` with a permissive allow-list, plus a CI job running
`cargo deny check`. It came back

```
advisories ok, bans ok, licenses ok, sources ok
```

The gate was then proven by reversal, as the house rule requires: add a real
crate under a licence the allow-list does not contain — `MPL-2.0` — and watch it
go red.

**As a normal dependency it went red immediately**, with the right message:

```
error[rejected]: failed to satisfy license requirements
21 │ license = "MPL-2.0"
   │            rejected: license is not explicitly allowed
licenses FAILED
```

**As a `[dev-dependencies]` entry it stayed green.** Same crate, same licence,
same workspace, one line moved from one table to another:

| | `cargo metadata` | `cargo tree` | `cargo deny list` | verdict |
|---|---|---|---|---|
| normal dep | 12 packages | shows it | shows it, `MPL-2.0 (1)` | **red**, correctly |
| dev dep | 12 packages | shows it | 11 crates, no `MPL-2.0` | **green**, wrongly |

Tried on two different workspace members, a binary crate and a library crate;
identical result. Setting `exclude-dev = false` explicitly changed nothing, and
neither did stripping the config to two lines. The checker is fine — the graph it
was handed was short.

## Why the reversal nearly missed it

The first reversal attempt used a dev-dependency, because that is where a
throwaway test crate naturally goes. It came back green, and the obvious reading
was **"the reversal is broken"** — a mistyped licence, a stale lock file, a config
that was not loaded. Three of those were checked and ruled out before the actual
hypothesis — *the tool never looked* — was even considered.

That ordering is the lesson. **A reversal that fails to go red is evidence about
the gate, not only about the reversal**, and the instinct is to debug the
reversal first. Had the reversal been written with a normal dependency — the more
natural choice, and the one that works — it would have gone red on the first try,
proven the gate, and the hole would have shipped.

## The fix, which is not a fix

The tool's blind spot could not be configured away, so it was made **loud**
instead. The CI job now compares the set of crates the gate judged against the
set the build system resolves, and fails when they differ:

```
cargo deny list  | ... | sort -u  > deny-saw.txt
cargo tree       | ... | sort -u  > cargo-saw.txt
diff -u deny-saw.txt cargo-saw.txt || exit 1
```

Equal today, 11 and 11. With the MPL-2.0 dev-dependency present, `cargo deny
check` still prints all four `ok` lines — and the diff prints `+option-ext` and
exits 1.

An unfixable hole is worth converting into a noisy one. The gate still cannot
judge those crates; it can no longer be quiet about not having tried.

## The generalisation

> **A green gate answers "did what I looked at pass", never "did I look at
> everything".** The second question is a separate check, and it is usually
> cheap: compare the population the gate reports against the population some
> other tool independently derives, and fail on disagreement.
>
> And when a reversal does not go red, spend the first hypothesis on the gate
> rather than on the reversal. Reaching for "my test is wrong" first is what
> keeps a hole open — the tool has just told you something, in the only way it
> can.
