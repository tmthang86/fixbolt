#!/usr/bin/env python3
"""Fail if a link points at a repository file that does not exist, or names one
by absolute URL instead of by relative path.

CLAUDE.md §4: a stale document is worse than no document. This repository has
already shipped dead links twice — 14 references to CLAUDE.md while it was
untracked (fixed in 6de1319), and 4 references to CHANGELOG.md before it existed
(fixed in a6ff5dd). Both were found by hand. This finds them by machine.

Only internal links are checked. External URLs are not fetched: a network call
would make this gate flaky, and a flaky gate gets switched off.

**That exemption had a hole, and it cost two dead links that lived in the code
for a day.** `crates/engine/src/dispatch.rs` and `crates/session/src/journal.rs`
each cited an ADR as `https://github.com/nanofixengine/docs/decisions/...` — an
organisation that does not exist. Two things hid them: this walked markdown only,
so it never opened a `.rs` file; and the link was an external URL, which it skips
by design. Extending the walk alone would still have missed them.

So there is a second rule, and it needs no network: **a file that lives in this
repository must be linked by relative path, never by absolute URL.** An absolute
URL naming a path this repository has is reported, whatever host it claims —
which catches a wrong organisation, a renamed repository and a moved file alike,
and would have caught these two the moment they were written.

Anchors (#section) are stripped and not verified — verifying them means parsing
every heading, and the failure mode of a wrong anchor is mild compared with a
link to a file that is not there.

Run: scripts/check-links.py
"""

import os
import re
import sys

LINK = re.compile(r"\[[^\]]*\]\(([^)]+)\)")
# `[label]: url` — rustdoc's reference form, which is how one of the two dead
# links was written and why matching only the inline form was not enough.
REFLINK = re.compile(r"^\s*(?://[/!]\s*)?\[[^\]]+\]:\s*(\S+)\s*$", re.M)
SKIP_DIRS = {".git", "vendor", "target", "node_modules"}
EXTERNAL = ("http://", "https://", "mailto:", "#")
URL = ("http://", "https://")
SUFFIXES = (".md", ".rs")
# Extensions a link has to carry before a `.rs` file's link is treated as a path.
#
# **This exists because the first version reported 13 false positives**, all of
# them rustdoc intra-doc links: `[crate::FieldIndex]`, `[Self::render]`,
# `[GroupIter::declared]`. Those name Rust items, not files, and rustdoc resolves
# them itself — checking them here would be a second, worse implementation of
# something the compiler already does. In a `.rs` file a link is a path only if
# it says so: an extension, or a relative prefix.
PATHISH = (".md", ".rs", ".toml", ".sh", ".py", ".yml", ".yaml", ".def", ".xml")


def looks_like_a_path(rel, target):
    if not rel.endswith(".rs"):
        return True
    if "::" in target:
        return False
    return target.startswith(("./", "../")) or target.endswith(PATHISH)


def source_files(root):
    for dirpath, dirnames, filenames in os.walk(root):
        dirnames[:] = [d for d in dirnames if d not in SKIP_DIRS]
        for name in sorted(filenames):
            if name.endswith(SUFFIXES):
                yield os.path.relpath(os.path.join(dirpath, name), root)


def names_a_repo_file(root, url):
    """The repo-relative path this URL names, if this repository has that file.

    Compared from the right: a URL carries an owner and a repository name in
    front of the path, and the point is to catch the case where those are wrong.
    """
    parts = url.split("#")[0].split("?")[0].strip("/").split("/")
    for i in range(len(parts)):
        tail = "/".join(parts[i:])
        if not tail or "." not in parts[-1]:
            continue
        if os.path.isfile(os.path.join(root, tail)):
            return tail
    return None


def main():
    root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    dead, checked, scanned = [], 0, 0

    absolute = []

    for rel in source_files(root):
        scanned += 1
        base = os.path.dirname(rel)
        with open(os.path.join(root, rel), encoding="utf-8") as fh:
            text = fh.read()
        for match in list(LINK.finditer(text)) + list(REFLINK.finditer(text)):
            target = match.group(1).split("#")[0].strip()
            if target.startswith(URL):
                tail = names_a_repo_file(root, target)
                if tail:
                    line = text[: match.start()].count("\n") + 1
                    absolute.append((rel, line, target, tail))
                continue
            if not target or target.startswith(EXTERNAL):
                continue
            if not looks_like_a_path(rel, target):
                continue
            checked += 1
            # Resolve relative to the file holding the link, not to the repo
            # root. An earlier version of this check got that wrong and
            # reported 13 false positives.
            resolved = os.path.normpath(os.path.join(root, base, target))
            if not os.path.exists(resolved):
                line = text[: match.start()].count("\n") + 1
                dead.append((rel, line, target))

    print(
        f"{scanned} markdown and rust files, {checked} internal links checked, "
        f"{len(absolute)} absolute URLs naming a file in this repository"
    )

    if absolute:
        print(
            f"\nFAIL: {len(absolute)} link(s) name a repository file by absolute URL",
            file=sys.stderr,
        )
        for rel, line, target, tail in absolute:
            print(f"  {rel}:{line}  →  {target}", file=sys.stderr)
            print(f"      this repository has {tail}; link it by relative path", file=sys.stderr)

    if dead:
        print(f"\nFAIL: {len(dead)} dead internal link(s)\n", file=sys.stderr)
        for rel, line, target in dead:
            print(f"  {rel}:{line}  →  {target}", file=sys.stderr)

    if dead or absolute:
        return 1

    print("no dead internal links")
    return 0


if __name__ == "__main__":
    sys.exit(main())
