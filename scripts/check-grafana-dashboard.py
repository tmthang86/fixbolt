#!/usr/bin/env python3
"""Check tools/grafana/fixbolt.json for the shape ADR-0171 decision 5 requires
so the file survives BOTH ways people load a dashboard: the Grafana import
dialog and file provisioning.

Background (ADR-0171 *Context* item 3, grafana/grafana#10786): a dashboard
exported "for sharing" carries `__inputs` and `${DS_PROMETHEUS}`-style
placeholders. The import dialog fills those in; file provisioning does not,
and the dashboard fails at load with "Datasource `${DS_PROMETHEUS}` was not
found". The fix decided here is a `datasource`-type template variable
(`${datasource}`) that every panel and every target refers to instead — no
`__inputs`, nothing for provisioning to leave unfilled.

This script checks STRUCTURE. It does not run Grafana and does not know
whether a panel draws — that is the plan's step 7, on a machine with a
container runtime. It also does not know whether a queried series is one the
exporter actually emits — that is
`crates/metrics/tests/dashboard.rs::every_series_the_dashboard_queries_is_scraped`,
against a real scrape. The three checks divide the work the way ADR-0171
divides it: this script is the dashboard-shape oracle, `promtool` (via
scripts/check-metrics-format.sh) is the wire-format oracle, and the Rust test
is the cross-check between the two files' vocabularies.

Checks, each printing FAIL and the panel/field at fault:

  1. The file parses as JSON.
  2. No `__inputs` key anywhere in the document (recursively).
  3. No `${DS_` substring anywhere in the raw text — the broken placeholder
     shape, whatever variable name it would have used.
  4. A template variable of type `datasource` exists, and its name is what
     panels below refer to as `${<name>}`.
  5. Every panel that is not a `row` (a row is a divider, not a query) names
     its `datasource.uid` as exactly `${<the datasource variable>}`.
  6. Every target under such a panel does the same, and every target has a
     non-empty `expr`.
  7. Panel ids are unique.

Usage: scripts/check-grafana-dashboard.py [path-to-dashboard.json ...]
Defaults to tools/grafana/fixbolt.json. Exit 0 and a summary line on success;
otherwise every FAIL line is printed and the exit code is 1.
"""
from __future__ import annotations

import json
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
DEFAULT_TARGET = REPO_ROOT / "tools" / "grafana" / "fixbolt.json"


def find_key(obj, key: str, path: str = "$") -> list[str]:
    """Every JSON path at which `key` appears, however deep."""
    hits: list[str] = []
    if isinstance(obj, dict):
        for k, v in obj.items():
            p = f"{path}.{k}"
            if k == key:
                hits.append(p)
            hits.extend(find_key(v, key, p))
    elif isinstance(obj, list):
        for i, v in enumerate(obj):
            hits.extend(find_key(v, key, f"{path}[{i}]"))
    return hits


def check_dashboard(path: Path) -> list[str]:
    """Every FAIL line for one dashboard file. Empty means it passed."""
    fails: list[str] = []
    raw = path.read_text(encoding="utf-8")

    try:
        doc = json.loads(raw)
    except json.JSONDecodeError as e:
        return [f"FAIL {path}: not valid JSON: {e}"]

    inputs = find_key(doc, "__inputs")
    for p in inputs:
        fails.append(
            f"FAIL {path}: `__inputs` at {p} — this is the \"export for "
            "sharing\" shape that file provisioning cannot fill in "
            "(ADR-0171 Context item 3)"
        )

    if "${DS_" in raw:
        fails.append(
            f"FAIL {path}: contains `${{DS_` — a placeholder the import "
            "dialog fills in and provisioning does not; use a "
            "`datasource`-type template variable instead"
        )

    variables = doc.get("templating", {}).get("list", [])
    ds_vars = [v for v in variables if v.get("type") == "datasource"]
    if not ds_vars:
        fails.append(
            f"FAIL {path}: no template variable of type `datasource` — "
            "every panel needs one to refer to instead of naming a uid"
        )
        # Nothing below can be checked meaningfully without it.
        return fails
    if len(ds_vars) > 1:
        fails.append(
            f"FAIL {path}: {len(ds_vars)} `datasource`-type variables "
            f"({[v.get('name') for v in ds_vars]}) — one is enough and more "
            "is ambiguous about which panels should use which"
        )
    ds_name = ds_vars[0].get("name")
    if not ds_name:
        fails.append(f"FAIL {path}: the datasource variable has no `name`")
        return fails
    want_ref = f"${{{ds_name}}}"

    def panel_datasource_ok(p) -> bool:
        ds = p.get("datasource")
        if isinstance(ds, dict):
            return ds.get("uid") == want_ref
        return ds == want_ref

    seen_ids: dict[int, str] = {}

    def walk_panels(panels, path_prefix: str):
        for i, p in enumerate(panels):
            ppath = f"{path_prefix}[{i}]"
            pid = p.get("id")
            title = p.get("title", "<untitled>")
            if pid is not None:
                if pid in seen_ids:
                    fails.append(
                        f"FAIL {path}: panel id {pid} ({title!r} at {ppath}) "
                        f"reuses the id of {seen_ids[pid]!r}"
                    )
                else:
                    seen_ids[pid] = title

            if p.get("type") == "row":
                # A row is a divider; it may carry nested panels (collapsed
                # rows do) and has no query of its own to check.
                walk_panels(p.get("panels", []), f"{ppath}.panels")
                continue

            if not panel_datasource_ok(p):
                fails.append(
                    f"FAIL {path}: panel {title!r} at {ppath} does not use "
                    f"{want_ref} as its datasource"
                )

            targets = p.get("targets", [])
            if not targets:
                fails.append(
                    f"FAIL {path}: panel {title!r} at {ppath} has no targets"
                )
            for j, t in enumerate(targets):
                tpath = f"{ppath}.targets[{j}]"
                expr = t.get("expr")
                if not expr or not str(expr).strip():
                    fails.append(
                        f"FAIL {path}: target at {tpath} (panel {title!r}) "
                        "has no `expr`"
                    )
                if not panel_datasource_ok(t):
                    fails.append(
                        f"FAIL {path}: target at {tpath} (panel {title!r}) "
                        f"does not use {want_ref} as its datasource"
                    )

    walk_panels(doc.get("panels", []), "$.panels")

    return fails


def main(argv: list[str]) -> int:
    targets = [Path(a) for a in argv] or [DEFAULT_TARGET]
    all_fails: list[str] = []
    for t in targets:
        if not t.is_file():
            all_fails.append(f"FAIL {t}: no such file")
            continue
        all_fails.extend(check_dashboard(t))

    if all_fails:
        for line in all_fails:
            print(line, file=sys.stderr)
        print(
            f"check-grafana-dashboard: FAIL — {len(all_fails)} problem(s) "
            f"across {len(targets)} file(s)",
            file=sys.stderr,
        )
        return 1

    print(
        f"check-grafana-dashboard: OK — {len(targets)} file(s), "
        "no __inputs, no ${DS_, every panel and target on the datasource "
        "variable, every target has an expr, panel ids unique"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
