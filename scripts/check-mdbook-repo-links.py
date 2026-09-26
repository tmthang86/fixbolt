#!/usr/bin/env python3
r"""Exercises the pure `rewrite_target` function inside
scripts/mdbook-repo-links.py without going through mdBook's own stdin/stdout
protocol -- the fastest way to prove the two required cases from
docs/plans/2026-09-26-docs-for-embedders.md, step 3: a link that leaves
docs/ becomes a GitHub blob URL at the commit being built, and a link that
stays inside the book is left exactly as written.

There is no `script-logic`-style test for a Python script in this repository
yet (`grep -rn "\.py" .github/workflows/ci.yml` shows scripts/check-links.py
and scripts/check-grafana-dashboard.py run directly as gates, with no
separate unit test): the `script-logic` job that exists tests only shell
scripts' pure functions. This script follows that job's shape (import the
pure function, assert, print `ok`/`FAIL` per case, exit non-zero on any
failure) translated to Python, placed beside the script it tests. Wiring it
into CI is not part of this file -- see the delivery report for step 3.

Run: python3 scripts/check-mdbook-repo-links.py
"""

import importlib.util
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
REPO_ROOT = os.path.dirname(HERE)


def _load():
    spec = importlib.util.spec_from_file_location(
        "mdbook_repo_links", os.path.join(HERE, "mdbook-repo-links.py")
    )
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


mrl = _load()

pass_count = 0
fail_count = 0


def check(name, got, want):
    global pass_count, fail_count
    if got == want:
        pass_count += 1
        print(f"ok    {name}")
    else:
        fail_count += 1
        print(f"FAIL  {name}\n      want {want!r}\n      got  {got!r}")


SHA = "abc1234def5678900000000000000000000aaaa"
KNOWN = {
    "GUIDE.md",
    "index.md",
    "internals/README.md",
    "internals/codec.md",
    "decisions/ADR-0001-relationship-to-quickfix.md",
}


def rewrite(target, chapter_dir):
    return mrl.rewrite_target(target, chapter_dir, "docs", KNOWN, REPO_ROOT, SHA)


# The plan's own example: a link that leaves docs/ entirely, from a
# top-level chapter (docs/GUIDE.md, chapter_dir "").
check(
    "a link out of docs/ becomes a GitHub blob URL at the built commit",
    rewrite("../crates/codec/src/dict.rs", ""),
    f"https://github.com/{mrl.OWN_REPO}/blob/{SHA}/crates/codec/src/dict.rs",
)

# The plan's other half: an in-book link is untouched.
check(
    "an in-book link to another top-level page is untouched",
    rewrite("GUIDE.md", ""),
    None,
)
check(
    "an in-book link between two nested chapters is untouched",
    rewrite("codec.md", "internals"),
    None,
)

# README.md -> index.md: an in-book link, still rewritten, never turned into
# a GitHub URL, because docs/internals/README.md is served at
# internals/index.html by mdBook's own built-in "index" preprocessor.
check(
    "a link to an in-book README.md is rewritten to index.md, not GitHub",
    rewrite("internals/README.md", ""),
    "internals/index.md",
)
check(
    "the README.md -> index.md rewrite keeps a relative prefix as written",
    rewrite("../internals/README.md", "decisions"),
    "../internals/index.md",
)
check(
    "the README.md -> index.md rewrite keeps an anchor",
    rewrite("internals/README.md#codec", ""),
    "internals/index.md#codec",
)

# A bare directory under docs/ names no chapter file -- not a page
# SUMMARY.md lists, so it is treated the same as "under docs/ but unknown"
# and becomes a GitHub tree URL rather than a dead in-book link.
check(
    "a link to a bare directory under docs/ becomes a GitHub tree URL",
    rewrite("decisions/", ""),
    f"https://github.com/{mrl.OWN_REPO}/tree/{SHA}/docs/decisions",
)
check(
    "the same directory link without a trailing slash lands the same way",
    rewrite("decisions", ""),
    f"https://github.com/{mrl.OWN_REPO}/tree/{SHA}/docs/decisions",
)

# docs/plans/ is explicitly ADR-0206 decision 5's example of "a file under
# docs/ that SUMMARY.md does not list" -- unknown to the book although it
# lives inside docs/, so it gets a GitHub URL like an out-of-docs link would.
check(
    "docs/plans/ is inside docs/ but not in the book, and gets a GitHub URL",
    rewrite("plans/", ""),
    f"https://github.com/{mrl.OWN_REPO}/tree/{SHA}/docs/plans",
)

# A known in-book file reached by climbing out and back in still resolves to
# the same normalized in-book path and is left untouched.
check(
    "a known page reached via ../ back into docs/ is still in-book",
    rewrite("../GUIDE.md", "decisions"),
    None,
)

# External URLs and pure in-page anchors are never touched.
check("an external https:// URL is untouched", rewrite("https://example.com/x", ""), None)
check("a mailto: link is untouched", rewrite("mailto:a@example.com", ""), None)
check("a bare in-page anchor is untouched", rewrite("#section", ""), None)

# split_protected: a fenced code block's literal `[x](y)` is never rewritten.
content = (
    "See [GUIDE.md](../crates/codec/src/dict.rs) for real.\n\n"
    "```markdown\n"
    "An example: `[x](y)` is Markdown link syntax.\n"
    "```\n"
)
rewritten = mrl.rewrite_content(content, "", "docs", KNOWN, REPO_ROOT, SHA)
check(
    "a real link inside prose is rewritten",
    f"https://github.com/{mrl.OWN_REPO}/blob/{SHA}/crates/codec/src/dict.rs" in rewritten,
    True,
)
check(
    "a fenced code block's illustrative link text survives untouched",
    "`[x](y)` is Markdown link syntax." in rewritten,
    True,
)

# The reference-link form `[label]: target` is read the same way.
ref_content = "See [dict-src] for the parser.\n\n[dict-src]: ../crates/codec/src/dict.rs\n"
ref_rewritten = mrl.rewrite_content(ref_content, "", "docs", KNOWN, REPO_ROOT, SHA)
check(
    "a reference-style link is rewritten the same way an inline one is",
    f"[dict-src]: https://github.com/{mrl.OWN_REPO}/blob/{SHA}/crates/codec/src/dict.rs"
    in ref_rewritten,
    True,
)

# Regression: docs/decisions/ADR-0088-...md's real shape,
# `` [`Recovery`]: ../../crates/engine/src/recovery.rs `` -- an inline code
# span sitting INSIDE a reference-link label. A first version of this script
# split inline code out of the text before matching either link regex, which
# tore this exact line into three pieces and silently stopped rewriting it
# (REFLINK's `^...$` anchors never saw a whole line again). Chapter is
# docs/decisions/ADR-0088-...md itself, two levels below docs/, so
# `../../crates/...` really does escape docs/ from there.
backtick_label_content = (
    "See [`Recovery`] below.\n\n[`Recovery`]: ../../crates/engine/src/recovery.rs\n"
)
backtick_label_rewritten = mrl.rewrite_content(
    backtick_label_content, "decisions", "docs", KNOWN, REPO_ROOT, SHA
)
check(
    "a reflink whose label is backtick-wrapped is still rewritten",
    f"[`Recovery`]: https://github.com/{mrl.OWN_REPO}/blob/{SHA}/crates/engine/src/recovery.rs"
    in backtick_label_rewritten,
    True,
)

print(f"\n{pass_count} ok, {fail_count} FAIL")
sys.exit(1 if fail_count else 0)
