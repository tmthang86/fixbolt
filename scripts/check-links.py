#!/usr/bin/env python3
"""Fail if any markdown file links to a repository file that does not exist.

CLAUDE.md §4: a stale document is worse than no document. This repository has
already shipped dead links twice — 14 references to CLAUDE.md while it was
untracked (fixed in 6de1319), and 4 references to CHANGELOG.md before it existed
(fixed in a6ff5dd). Both were found by hand. This finds them by machine.

Only internal links are checked. External URLs are not fetched: a network call
would make this gate flaky, and a flaky gate gets switched off.

Anchors (#section) are stripped and not verified — verifying them means parsing
every heading, and the failure mode of a wrong anchor is mild compared with a
link to a file that is not there.

Run: scripts/check-links.py
"""

import os
import re
import sys

LINK = re.compile(r"\[[^\]]*\]\(([^)]+)\)")
SKIP_DIRS = {".git", "vendor", "target", "node_modules"}
EXTERNAL = ("http://", "https://", "mailto:", "#")


def markdown_files(root):
    for dirpath, dirnames, filenames in os.walk(root):
        dirnames[:] = [d for d in dirnames if d not in SKIP_DIRS]
        for name in sorted(filenames):
            if name.endswith(".md"):
                yield os.path.relpath(os.path.join(dirpath, name), root)


def main():
    root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    dead, checked, scanned = [], 0, 0

    for rel in markdown_files(root):
        scanned += 1
        base = os.path.dirname(rel)
        with open(os.path.join(root, rel), encoding="utf-8") as fh:
            text = fh.read()
        for match in LINK.finditer(text):
            target = match.group(1).split("#")[0].strip()
            if not target or target.startswith(EXTERNAL):
                continue
            checked += 1
            # Resolve relative to the file holding the link, not to the repo
            # root. An earlier version of this check got that wrong and
            # reported 13 false positives.
            resolved = os.path.normpath(os.path.join(root, base, target))
            if not os.path.exists(resolved):
                line = text[: match.start()].count("\n") + 1
                dead.append((rel, line, target))

    print(f"{scanned} markdown files, {checked} internal links checked")

    if dead:
        print(f"\nFAIL: {len(dead)} dead internal link(s)\n", file=sys.stderr)
        for rel, line, target in dead:
            print(f"  {rel}:{line}  →  {target}", file=sys.stderr)
        return 1

    print("no dead internal links")
    return 0


if __name__ == "__main__":
    sys.exit(main())
