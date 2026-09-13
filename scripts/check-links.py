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
each cited an ADR as `https://github.com/fixbolt/docs/decisions/...` — an
organisation that does not exist. Two things hid them: this walked markdown only,
so it never opened a `.rs` file; and the link was an external URL, which it skips
by design. Extending the walk alone would still have missed them.

So there is a second rule, and it needs no network: **a file that lives in this
repository must be linked by relative path, never by absolute URL.** An absolute
URL naming a path this repository has is reported, whatever host it claims —
which catches a wrong organisation, a renamed repository and a moved file alike,
and would have caught these two the moment they were written.

**That rule over-matched, because it compared a URL's tail the same way
regardless of whose tail it was.** `[measured 2026-09-13]` it read a citation of
`EmbarkStudios/cargo-deny`'s `CHANGELOG.md` as a link to this repository's own
`CHANGELOG.md` — a bare filename is not evidence of which repository it
belongs to. Two rules replace the one above: **(a)** a URL into this
repository's own path (`OWN_REPO`, below — a repository rename changes it only
there) is judged on any matching tail, even one segment, because this
repository's own file found under the wrong path is exactly what should be
caught. **(b)** any other path is judged only when the matching tail carries
at least two segments — a bare filename is a convention, not an address. The
limit that leaves, stated rather than hidden: an absolute URL to this
repository's own root file under the **wrong organisation**
(`https://github.com/fixbolt/CHANGELOG.md`) now passes through unreported; it
is counted and printed as a foreign URL sharing only a filename, never
silently dropped.

**Rule (b)'s tail search over-matched again, this time on a well-formed
GitHub URL rather than a bare filename.** `[measured 2026-09-13]` a citation of
`tokio-rs/tokio`'s own `.github/workflows/ci.yml` — a real, well-formed
`owner/repo/blob/<ref>/<path>` URL naming a file this repository also happens
to have at that same tail — was read as this repository's own file linked the
wrong way. **(c)**, checked before (b): a `github.com` URL whose segments
parse as `owner/repo/(blob|tree|raw|commit|blame)/<ref>/<path...>`, whose
owner segment is **not** this repository's own owner (`OWN_REPO`, below,
compared case-insensitively), and whose repo segment is neither this
repository's current name nor a former one (`FORMER_NAMES`, below) names
another repository explicitly and is not judged at all — counted in its own
summary line, never silently dropped. `[measured 2026-09-13]` the owner check
was added after a senior review found the first version of (c) judged only
the repo segment: `https://github.com/tmthang86/fixbolt-engine/...` and
`https://github.com/tmthang86/fixbot/...` — this repository's own owner, an
unfamiliar repo name — were both waved through as foreign and never checked.
This is narrower than it sounds: a *malformed* GitHub URL (no `blob/<ref>`,
the wrong segment count — the `fixbolt/docs/decisions/...` case rule (b)
already catches) and every non-GitHub host still fall through to (b)
unchanged, and a well-formed URL whose repo segment **is** `fixbolt` or
`nanofixengine` under the wrong owner is still judged by (b), because that is
a wrong-organisation citation of this repository, not another one. The limit
this still leaves: a well-formed URL under **another owner's** repository
whose tail happens to match a file in this repository (a fork, or an
unrelated repository that merely shares a relative path) now passes
unjudged, counted.

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

# This repository's own GitHub address, as host+path segments. A rename or a
# move to a different organisation changes only this tuple.
OWN_REPO = ("github.com", "tmthang86", "fixbolt")

# Names this repository has answered to besides its current one. A URL whose
# repo segment is one of these is still a citation of *this* repository (under
# the wrong owner, or from before the rename) and must stay judged by rule (b)
# below — rule (c) must not wave it through as "another repository".
FORMER_NAMES = ("nanofixengine",)

# The path segment that marks a github.com URL as a file link rather than a
# link to the repo, an issue, a PR, or anything else GitHub serves at a path.
GITHUB_FILE_VERBS = ("blob", "tree", "raw", "commit", "blame")


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
    """The repo-relative path this URL names, if this repository has that file,
    and whether it is evidence of that at all.

    Compared from the right: a URL carries an owner and a repository name in
    front of the path, and the point is to catch the case where those are
    wrong. But the tail alone does not say whose file it is — so which tail
    length counts as evidence depends on whether the URL's owner+repo segments
    are this repository's own (`OWN_REPO`):

    (a) this repository's own path → any matching tail is evidence, even one
        segment, by design: its own file under the wrong sub-path is exactly
        the case this exists to catch.
    (b) any other path (including a different organisation on the very same
        host — see `OWN_REPO`'s comparison below, which is by segment, not by
        string prefix) → only a matching tail of two or more segments is
        evidence. A bare filename such as `CHANGELOG.md` or `README.md` is a
        convention thousands of repositories share, not an address.

    Returns `(tail, ignored_tail, foreign)`: `tail` is set when rule (a) or
    (b) judges the match as evidence; `ignored_tail` is set instead when a
    bare filename matched but rule (b) declined to call that evidence;
    `foreign` is set instead when rule (c) recognised the URL as naming
    another repository outright and declined to judge it at all. The caller
    counts each of the latter two classes rather than dropping them silently.
    """
    rest = url
    for prefix in URL:
        if rest.startswith(prefix):
            rest = rest[len(prefix) :]
            break
    parts = rest.split("#")[0].split("?")[0].strip("/").split("/")
    # GitHub reads a host, an owner and a repository name without regard to
    # case, and `www.github.com` is the same host, so all three spell this
    # repository. `[measured 2026-09-13]` compared as written, the senior review
    # of PR #68 linked `https://github.com/TmThang86/Fixbolt/blob/main/CHANGELOG.md`
    # and `https://www.github.com/tmthang86/fixbolt/blob/main/CHANGELOG.md` —
    # both this repository's own root file — and both were counted as foreign
    # and not judged. The file path after them stays case-sensitive.
    head = [p.lower() for p in parts[: len(OWN_REPO)]]
    if head and head[0].startswith("www."):
        head[0] = head[0][len("www.") :]
    own = head == list(OWN_REPO)

    # (c) a well-formed github.com file URL naming a repository that is
    # neither this one nor a former name of this one, **and whose owner
    # segment is not this repository's own owner**, is a citation of another
    # repository, full stop — never judged, whatever tail it happens to
    # share with this repository. Checked before (b)'s tail search, which
    # cannot otherwise tell "this repo's file, wrong path" from "another
    # repo's file that happens to sit at the same relative path". The owner
    # check keeps a wrong or misspelled repository name **under this
    # repository's own owner** from being waved through as "foreign": that
    # is far more likely a mistaken citation of this repository than a real
    # other repository, so it falls through to (b) and is judged there.
    if (
        not own
        and head
        and head[0] == "github.com"
        and len(parts) >= 6
        and parts[3] in GITHUB_FILE_VERBS
        and parts[4]
        and parts[2].lower() not in (OWN_REPO[2],) + FORMER_NAMES
        and parts[1].lower() != OWN_REPO[1].lower()
    ):
        return None, None, True

    ignored_tail = None
    for i in range(len(parts)):
        tail = "/".join(parts[i:])
        if not tail or "." not in parts[-1]:
            continue
        if not os.path.isfile(os.path.join(root, tail)):
            continue
        if own or "/" in tail:
            return tail, None, False
        ignored_tail = tail  # rule (b): a bare filename, not judged
    return None, ignored_tail, False


def main():
    root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    dead, checked, scanned = [], 0, 0

    absolute = []
    ignored_bare = 0
    foreign_named = 0

    for rel in source_files(root):
        scanned += 1
        base = os.path.dirname(rel)
        with open(os.path.join(root, rel), encoding="utf-8") as fh:
            text = fh.read()
        for match in list(LINK.finditer(text)) + list(REFLINK.finditer(text)):
            target = match.group(1).split("#")[0].strip()
            if target.startswith(URL):
                tail, ignored_tail, foreign = names_a_repo_file(root, target)
                if foreign:
                    foreign_named += 1
                if ignored_tail:
                    ignored_bare += 1
                if tail:
                    # crates/library/README.md is included into rustdoc via include_str!
                    # and published to crates.io and docs.rs, where relative paths into docs/
                    # break. It must use absolute GitHub URLs.
                    if rel == "crates/library/README.md":
                        checked += 1
                        continue
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
        f"{len(absolute)} absolute URLs naming a file in this repository, "
        f"{ignored_bare} foreign URLs sharing only a filename with this repository (not judged), "
        f"{foreign_named} foreign GitHub URLs naming another repository (not judged)"
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
