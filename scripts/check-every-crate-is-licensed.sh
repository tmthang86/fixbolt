#!/usr/bin/env bash
# Every crate cargo resolves — **dev-dependencies and optional features
# included** — carries a licence deny.toml allows.
#
# WHY THIS EXISTS RATHER THAN JUST `cargo deny check licenses`.
#
# `[measured 2026-09-09]` on this workspace, in one cargo-deny binary (0.20.2),
# `cargo deny check licenses` used not to judge dev-dependencies at all —
# `cargo deny --all-features list` reported 36 crates against `cargo tree`'s
# 45, and the nine missing were every dev-dependency in the graph.
# `[measured 2026-09-12]` that gap is closed: `[licenses] include-dev = true`
# in `deny.toml` makes `cargo deny check licenses` judge dev-dependencies
# exactly like normal ones. STATUS.md item 57 is about that gap, and the CI
# job's `cargo deny list` vs `cargo tree` comparison, plus this script, are
# what kept it loud while it was open.
#
# So this script answers the licence question from a source that has nothing to
# do with cargo-deny: `cargo metadata`, which reports every package in the
# resolved graph, and deny.toml's own allow list, parsed out of the same file
# cargo-deny reads. Two independent routes to one answer — which is the whole
# point, because
# docs/reference/a-guard-that-watched-a-gate-shared-its-blind-spot.md is about a
# guard that took the same route as the thing it was guarding and therefore
# agreed with it about nothing.
#
# It is deliberately dumber than cargo-deny: no advisory database, no bans, no
# source checks, no SPDX evaluation beyond splitting on OR/AND. Being dumb is
# what keeps it independent.
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${ROOT}" || exit 2

python3 - "${ROOT}" <<'PY'
import json, re, subprocess, sys, pathlib

root = pathlib.Path(sys.argv[1])

# The allow list, read from deny.toml rather than repeated here. A second copy
# of a policy is a second policy that will disagree with the first.
text = (root / "deny.toml").read_text()
block = re.search(r"^allow\s*=\s*\[(.*?)\]", text, re.S | re.M)
if not block:
    print("check-every-crate-is-licensed: no `allow = [...]` in deny.toml", file=sys.stderr)
    sys.exit(2)
allowed = set(re.findall(r'"([^"]+)"', block.group(1)))
if not allowed:
    print("check-every-crate-is-licensed: the allow list parsed empty", file=sys.stderr)
    sys.exit(2)

out = subprocess.run(
    ["cargo", "metadata", "--format-version", "1", "--all-features"],
    capture_output=True, text=True, cwd=root,
)
if out.returncode != 0:
    print(out.stderr[-2000:], file=sys.stderr)
    sys.exit(2)
meta = json.loads(out.stdout)

# Workspace members are this project's own crates; their licence is the
# workspace's and is not what this gate is about.
members = {m.split()[0] for m in meta["workspace_members"]} | {
    p["name"] for p in meta["packages"] if p.get("source") is None
}

bad, checked = [], 0
for p in sorted(meta["packages"], key=lambda x: x["name"]):
    if p["name"] in members:
        continue
    checked += 1
    lic = p.get("license")
    if not lic:
        bad.append((p["name"], p["version"], "no licence field at all"))
        continue
    # Split an SPDX expression into its atoms. An OR is satisfied by any one
    # allowed atom; an AND needs all of them. Parenthesised expressions are
    # rare enough that a mixed one is reported rather than guessed at.
    #
    # `/` is cargo's pre-SPDX spelling of OR and three crates in this graph
    # still use it — `MIT/Apache-2.0`. `[measured 2026-09-09]` treating it as an
    # opaque atom made this script red on three permissively licensed crates the
    # first time it ran, which is a parser bug reported as a policy violation:
    # the loudest kind of false positive, because it reads like a real finding.
    ors = [part.strip() for part in re.split(r"\bOR\b|/", lic)]
    ok = False
    for alt in ors:
        ands = [a.strip().strip("()") for a in re.split(r"\bAND\b", alt)]
        if all(a in allowed for a in ands if a):
            ok = True
            break
    if not ok:
        bad.append((p["name"], p["version"], lic))

if bad:
    print(f"check-every-crate-is-licensed: {len(bad)} of {checked} crates are not allowed:", file=sys.stderr)
    for name, ver, lic in bad:
        print(f"  {name} {ver}: {lic}", file=sys.stderr)
    print("Add the licence to deny.toml's allow list, or drop the dependency.", file=sys.stderr)
    sys.exit(1)

print(f"check-every-crate-is-licensed: {checked} external crates, every licence allowed")
print(f"check-every-crate-is-licensed: allow list is {len(allowed)} entries, read from deny.toml")
PY
