#!/usr/bin/env python3
"""Fill the three generated regions of docs/SUMMARY.md from what is actually
on disk under docs/reference/, docs/internals/ and docs/decisions/, so the
book's table of contents cannot go stale by omission the way a hand-maintained
list can (ADR-0206 decision 6).

Three regions, each bounded by a `<!-- BEGIN GENERATED: <name>. ... -->` /
`<!-- END GENERATED: <name> -->` marker pair already in docs/SUMMARY.md:

  reference  -> one entry per docs/reference/*.md, sorted by filename.
  internals  -> one entry per docs/internals/*.md EXCEPT README.md (that one
                is listed by hand as the parent page), sorted by filename,
                nested one level under the hand-written "Crate internals" line.
  decisions  -> one entry per docs/decisions/ADR-*.md, sorted NUMERICALLY by
                the ADR number in the filename (not lexicographically, so
                ADR-2 sorts before ADR-10).

A file's title is its first Markdown H1 (`# ...`), read verbatim including
any inline code spans (`codec`) or em dashes the heading already carries —
this script does not reformat the title, only lifts it.

Two modes:

  (default)  rewrite docs/SUMMARY.md in place, replacing each region's body.
  --check    do not write. Exit 1 and name, on stderr, every file the current
             SUMMARY.md is missing from a region and every file a region
             lists that no longer exists on disk (or whose region content
             otherwise disagrees with what would be generated). Exit 0 and
             print nothing to stdout beyond a summary line when it already
             matches.

Run: scripts/gen-book-summary.py [--check]
"""

import argparse
import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
DOCS = os.path.join(ROOT, "docs")
SUMMARY = os.path.join(DOCS, "SUMMARY.md")

H1 = re.compile(r"^#\s+(.+?)\s*$", re.M)
ADR_NUMBER = re.compile(r"^ADR-(\d+)-")

# Each region: (marker name, directory relative to docs/, link prefix,
# exclude set, sort key, indent for its generated lines).
REGIONS = ("reference", "internals", "decisions")


def title_of(path):
    """The file's first H1 heading text, or its filename if it has none —
    a missing H1 is a defect in the source file, not something this script
    should hide by silently skipping the entry.
    """
    with open(path, encoding="utf-8") as fh:
        text = fh.read()
    match = H1.search(text)
    if match:
        return match.group(1)
    return os.path.basename(path)


def adr_sort_key(filename):
    match = ADR_NUMBER.match(filename)
    # A file under docs/decisions/ that does not match ADR-<digits>- would
    # sort first (key 0) rather than crash the whole generator — CLAUDE.md
    # non-negotiable 7 (no panic in a library crate) is a Rust rule, but the
    # same spirit applies here: a script gate should say what is wrong, not
    # traceback. `check-adr-numbers.sh` is the machine that actually enforces
    # ADR filename shape.
    return int(match.group(1)) if match else 0


def listed_files(dirname, exclude=()):
    try:
        names = os.listdir(os.path.join(DOCS, dirname))
    except FileNotFoundError:
        return []
    return [
        n
        for n in names
        if n.endswith(".md") and n not in exclude and not n.startswith(".")
    ]


def region_files(name):
    if name == "reference":
        return sorted(listed_files("reference"))
    if name == "internals":
        return sorted(listed_files("internals", exclude={"README.md"}))
    if name == "decisions":
        files = [f for f in listed_files("decisions") if f.startswith("ADR-")]
        return sorted(files, key=adr_sort_key)
    raise ValueError(name)  # pragma: no cover - REGIONS is the only caller


def region_lines(name):
    prefix = {"reference": "reference/", "internals": "internals/", "decisions": "decisions/"}[name]
    indent = "  " if name == "internals" else ""
    bullet = "- " if name != "internals" else "- "
    lines = []
    for filename in region_files(name):
        path = os.path.join(DOCS, prefix + filename)
        title = title_of(path)
        lines.append(f"{indent}{bullet}[{title}]({prefix}{filename})")
    return lines


def region_body(name):
    lines = region_lines(name)
    return "".join(line + "\n" for line in lines)


def marker_pattern(name):
    return re.compile(
        r"(?P<prefix>^[ \t]*<!-- BEGIN GENERATED: "
        + re.escape(name)
        + r"\.[^\n]*-->\n)"
        r"(?P<body>.*?)"
        r"(?P<suffix>^[ \t]*<!-- END GENERATED: "
        + re.escape(name)
        + r" -->)",
        re.M | re.S,
    )


LINK_TARGET = re.compile(r"\]\(([^)]+)\)")


def paths_in(body):
    return LINK_TARGET.findall(body)


def expected_paths(name):
    prefix = {"reference": "reference/", "internals": "internals/", "decisions": "decisions/"}[name]
    return [prefix + f for f in region_files(name)]


def rewrite(text):
    for name in REGIONS:
        pattern = marker_pattern(name)
        match = pattern.search(text)
        if not match:
            print(f"docs/SUMMARY.md has no marker pair for region '{name}'", file=sys.stderr)
            return None
        text = pattern.sub(
            lambda m, body=region_body(name): m.group("prefix") + body + m.group("suffix"),
            text,
            count=1,
        )
    return text


def check(text):
    problems = []
    for name in REGIONS:
        pattern = marker_pattern(name)
        match = pattern.search(text)
        if not match:
            problems.append(f"docs/SUMMARY.md has no marker pair for region '{name}'")
            continue
        actual_body = match.group("body")
        actual = paths_in(actual_body)
        expected = expected_paths(name)
        missing = [p for p in expected if p not in actual]
        extra = [p for p in actual if p not in expected]
        for p in missing:
            problems.append(f"docs/SUMMARY.md is missing an entry for docs/{p} (region '{name}')")
        for p in extra:
            problems.append(
                f"docs/SUMMARY.md lists docs/{p} in region '{name}', but that file does not exist "
                "(or no longer belongs there)"
            )
        if not missing and not extra:
            expected_body = region_body(name)
            if actual_body.strip("\n") != expected_body.strip("\n"):
                problems.append(
                    f"docs/SUMMARY.md region '{name}' lists the right files but not with the "
                    "generated titles or order — run scripts/gen-book-summary.py to regenerate"
                )
    return problems


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--check",
        action="store_true",
        help="report drift and exit non-zero instead of rewriting docs/SUMMARY.md",
    )
    args = parser.parse_args()

    with open(SUMMARY, encoding="utf-8") as fh:
        text = fh.read()

    if args.check:
        problems = check(text)
        counts = {name: len(region_files(name)) for name in REGIONS}
        print(
            "docs/SUMMARY.md generated regions: "
            + ", ".join(f"{name} {counts[name]}" for name in REGIONS)
        )
        if problems:
            print(f"\nFAIL: {len(problems)} problem(s) with the generated regions", file=sys.stderr)
            for problem in problems:
                print(f"  {problem}", file=sys.stderr)
            return 1
        print("docs/SUMMARY.md's generated regions match docs/reference, docs/internals and docs/decisions")
        return 0

    new_text = rewrite(text)
    if new_text is None:
        return 1
    if new_text != text:
        with open(SUMMARY, "w", encoding="utf-8") as fh:
            fh.write(new_text)
        print("docs/SUMMARY.md rewritten")
    else:
        print("docs/SUMMARY.md already up to date")
    return 0


if __name__ == "__main__":
    sys.exit(main())
