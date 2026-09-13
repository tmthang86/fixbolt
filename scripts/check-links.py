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

**Rule (c) fixed a false red. It left a true silence next to it, found while
proving (c) in the review of PR #69.** `[measured 2026-09-13]` a link reading
`https://github.com/tmthang86/fixbolt/blob/main/DESIGN.md` — this repository's
own owner and name, `blob/main`, and a path this repository does not have at
that spot (the real file is `docs/DESIGN.md`) — printed nothing. Rule (b)'s
tail search only ever matches a suffix that **is** a real file; a wrong path
under this repository's own name has no such suffix, so nothing in (a)–(c)
ever looks at it. And the hole was wider than that one case:
`crates/library/README.md` is exempted from rule (a)'s "use a relative path"
message because it is included into rustdoc and published to crates.io, where
a relative link into `docs/` breaks — but that exemption was granted the
moment *any* suffix of the URL matched a real file, which is not the same
thing as the URL's own path existing. A README link reading
`.../fixbolt/blob/main/crates/docs/GUIDE.md` (wrong: the real file is
`docs/GUIDE.md`, not `crates/docs/GUIDE.md`) was counted "checked" solely
because the tail `docs/GUIDE.md` exists somewhere in the tree.

**(d)**, checked before (c) and before the tail search, closes both: a
`github.com` URL whose owner is this repository's own owner, whose repository
name is the current one or a former one (`FORMER_NAMES`), and whose fourth
segment is a path-carrying verb (`OWN_REPO_PATH_VERBS` — `blob`, `tree`,
`raw`, `blame`; not `commit`, whose sha carries no path to check) is judged by
reading the exact path after the ref and testing it against this repository's
tree with `os.path.exists` — `exists`, not `isfile`, because a `tree/` URL
names a directory. The ref itself is read as exactly one path segment, the
way `remark-validate-links` reads it (`value.split(slash).slice(1)`): a ref
containing `/` is not recognised as a ref at all, so everything after its
first segment is read as part of the path, and a multi-segment ref reads as a
wrong path — noisy, never silently green, and the limit is stated rather than
hidden. Applies to **every** file, `crates/library/README.md` included: the
README's exemption waives rule (a)'s "must be relative" message, never rule
(d)'s "must exist" one. See
[a-bare-filename-is-not-evidence-of-a-repository](reference/a-bare-filename-is-not-evidence-of-a-repository.md)
for the shape of the hole this closes and the one still open.

An own-repository URL whose fourth segment is not one of `OWN_REPO_PATH_VERBS`
— `actions/…`, `pull/…`, `commit/<sha>`, or the bare repository root — names
nothing rule (d) can check against a path, and is counted rather than
silently ignored: printed in the summary as "not a file link". Below
`OWN_REPO_FLOOR`, rule (d) is treated as having stopped matching rather than
this repository having started citing itself less — `crates/library/README.md`
alone cites three files this way, so a correctly matching rule (d) never
checks fewer than three.

**A senior review of PR #69 found four more holes in the same gate.**
`[measured 2026-09-13]` **(1)** an own-repository URL with no recognised verb
at all — `https://github.com/tmthang86/fixbolt/docs/GUIDE.md`, no
`blob`/`tree`/`raw`/`blame` — was judged only by (d) and, finding no verb,
counted as "not a file link" without ever trying rule (a)'s tail search; a
wrong path here read as silence instead of a report. Fixed: a verb-less
own-repository URL now falls through to that tail search, and is counted "not
a file link" only when the search finds nothing either. **(2)** `<https://...>`
autolinks — the form rustdoc's `-D warnings` (ADR-0066) requires for a bare
URL in a doc comment — and
`raw.githubusercontent.com/<owner>/<repo>/<ref>/<path>` URLs were never read
at all: `LINK`/`REFLINK` match only `[x](url)` and `[label]: url`. `AUTOLINK`
below reads the first; `RAW_HOST` extends rule (d) to the second, one segment
shorter since the host already means "raw". **(3)** three narrower nits: a
bare ref (`tree/main`) no longer counts toward `OWN_REPO_FLOOR`, since it
names no particular file; the existence test compares each path segment
case-exactly against `os.listdir` (`resolve_case_exact`), because macOS's
default filesystem resolves `docs/design.md` to the real `docs/DESIGN.md`
while Linux, where CI gates this repository, does not; a `..` segment is
refused outright rather than followed out of the tree; and a Markdown link
title (`[x](url "title")`) is stripped (`strip_link_title`) before the target
is read, which a title left in the path had turned into a new, loud false
"this repository has no ...". **(4) left as stated limits, not fixed**: a
`%2F`-encoded `/` inside a ref can still hide a multi-segment ref from the "a
ref containing `/` is not recognised" check above; a `fixbolt.git/...` URL
does not match `OWN_REPO` (the trailing `.git` is not stripped) and falls
through unjudged; and GitHub's `edit/<ref>/<path>` and `commits/<ref>/<path>`
(plural — commit history scoped to a path, unlike singular `commit/<sha>`)
verbs are not in `OWN_REPO_PATH_VERBS` and are counted as "not a file link"
rather than checked.

Run: scripts/check-links.py
"""

import os
import re
import sys
from urllib.parse import unquote

LINK = re.compile(r"\[[^\]]*\]\(([^)]+)\)")
# `[label]: url` — rustdoc's reference form, which is how one of the two dead
# links was written and why matching only the inline form was not enough.
REFLINK = re.compile(r"^\s*(?://[/!]\s*)?\[[^\]]+\]:\s*(\S+)\s*$", re.M)
# `<https://...>` — the autolink form. Rustdoc's `-D warnings` (ADR-0066) makes
# this the *required* spelling of a bare URL in a doc comment (a bare
# `https://...` with no brackets is `error: this URL is not a hyperlink`), so
# `crates/engine/src/settings.rs`'s doc comments and every reference list in
# `docs/reference/` are written this way — and neither LINK nor REFLINK reads
# it: `<...>` is not `[x](...)`  and not `[x]: ...`. Matches inside a `//!` or
# `///` comment the same way LINK/REFLINK already do — the regex is
# comment-blind, so a `.rs` file's doc-comment autolinks read the same as a
# `.md` file's.
AUTOLINK = re.compile(r"<(https?://[^\s<>]+)>")
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

# The subset of GITHUB_FILE_VERBS that carries a checkable path after the ref,
# for rule (d) below. `commit/<sha>` is excluded on purpose: the sha is the
# whole address, and there is no path segment after it to test against this
# repository's tree. Derived from GITHUB_FILE_VERBS rather than listed again,
# so the two rules that read a verb list never drift apart.
OWN_REPO_PATH_VERBS = tuple(v for v in GITHUB_FILE_VERBS if v != "commit")

# Rule (d)'s floor. `crates/library/README.md` alone cites three of this
# repository's own files by absolute URL — it must, see the note on that file
# in `names_a_repo_file` — so a correctly matching rule (d) never checks fewer
# than three URLs. Below this, the rule has very likely stopped matching (a
# renamed verb, a shifted segment index) rather than the repository actually
# citing itself less. A bare ref (`tree/main`, an own-repository URL with an
# empty path) does not count toward this floor: it names no particular file,
# so it is confirmed to exist and left uncounted rather than inflated into
# "checked".
OWN_REPO_FLOOR = 3

# `raw.githubusercontent.com/<owner>/<repo>/<ref>/<path>` is the same kind of
# citation as a `github.com/<owner>/<repo>/raw/<ref>/<path>` URL, one segment
# shorter because the host already means "raw" — there is no verb segment to
# read. Judged the same way as rule (d): read the path after the ref, test it
# against this repository's tree.
RAW_HOST = "raw.githubusercontent.com"


def strip_link_title(raw):
    """A Markdown inline link may carry a title after the URL, quoted:
    `[x](url "title")` or `[x](url 'title')`. Read whole, the title becomes
    part of the "path" tested below and produces a loud false red the moment
    somebody adds one to an existing link — so it is stripped before anything
    else looks at the target.
    """
    match = re.match(r"""^(\S+)\s+(?:"[^"]*"|'[^']*')\s*$""", raw)
    return match.group(1) if match else raw


def resolve_case_exact(root, path):
    """Resolve `path` under `root` by comparing each segment case-exactly
    against `os.listdir` of its parent, rather than asking the OS whether the
    path exists.

    macOS's default filesystem is case-insensitive: `os.path.exists` alone
    would pass `blob/main/docs/design.md` against a real `docs/DESIGN.md` on
    the machine this is written on, and fail the same link on the Linux CI
    box that gates this repository. A `..` segment is refused outright rather
    than resolved — it is reported as a path this repository does not have,
    not followed out of the tree. Returns the resolved absolute path, or
    `None` if any segment does not match.
    """
    current = root
    for segment in path.split("/"):
        if segment in ("", ".", ".."):
            return None
        try:
            entries = os.listdir(current)
        except (FileNotFoundError, NotADirectoryError):
            return None
        if segment not in entries:
            return None
        current = os.path.join(current, segment)
    return current


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

    Returns `(tail, ignored_tail, foreign, own_repo)`. When `own_repo` is not
    `None`, rule (d) judged this URL by reading its path directly: `own_repo`
    is `("file", path)` for `raw.githubusercontent.com/<owner>/<repo>/<ref>/<path>`
    or a `github.com` URL whose fourth segment is a path-carrying verb with a
    ref after it (`path` is `""` for the bare ref, e.g. `tree/main`); it is
    `("not_file_link", None)` when the URL is this repository's own (or a
    former name's) but names no checkable path — `actions/…`, `pull/…`,
    `commit/<sha>`, the bare repository root, **or** a verb-less own-repository
    URL whose tail search below still found nothing. That last case is why
    `own_repo` is not always decided immediately: an own-repository URL with
    no recognised verb (`.../fixbolt/docs/GUIDE.md`) falls through to rule
    (a)'s tail search first, exactly as a non-own-repository URL would, and
    only becomes `("not_file_link", None)` if that search comes up empty —
    see the comment at the `own_or_former` check below. Whenever `own_repo`
    is set, the other three are `(None, None, False)`. Otherwise `own_repo`
    is `None` and the other three keep their rule (a)–(c) meaning: `tail` is
    set when rule (a) or (b) judges the match as evidence; `ignored_tail` is
    set instead when a bare filename matched but rule (b) declined to call
    that evidence; `foreign` is set instead when rule (c) recognised the URL
    as naming another repository outright and declined to judge it at all.
    The caller counts every one of these classes rather than dropping any of
    them silently.
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

    # (d) this repository's own path, current name or a former one
    # (`FORMER_NAMES`) — unlike `own` above, which only matches the current
    # name, because a URL under a former name is still a citation of *this*
    # repository and rule (d) exists to check its path, not merely to flag it
    # as wrong-organisation the way (b) does. A bare filename is not evidence
    # here — the owner and repository segments already are — so any tail
    # length counts, same reasoning as (a).
    own_or_former = (
        len(head) == len(OWN_REPO)
        and head[0] == OWN_REPO[0]
        and head[1] == OWN_REPO[1]
        and head[2] in (OWN_REPO[2],) + FORMER_NAMES
    )
    # (d, raw host) `raw.githubusercontent.com/<owner>/<repo>/<ref>/<path>` —
    # checked ahead of (d)'s `github.com` form because it is a different host
    # entirely and carries no verb segment: `parts[3]` is already the ref.
    raw_own_or_former = (
        len(parts) > 3
        and parts[0].lower() == RAW_HOST
        and parts[1].lower() == OWN_REPO[1]
        and parts[2].lower() in (OWN_REPO[2],) + FORMER_NAMES
    )
    if raw_own_or_former:
        path = unquote("/".join(parts[4:]))
        return None, None, False, ("file", path)

    if own_or_former:
        verb = parts[3] if len(parts) > 3 else None
        if verb in OWN_REPO_PATH_VERBS and len(parts) > 4:
            # parts[4] is the ref, read as exactly one segment (see the
            # module docstring); everything after it is the path, `/`-joined
            # and percent-decoded.
            path = unquote("/".join(parts[5:]))
            return None, None, False, ("file", path)
        # Not a checkable blob|tree|raw|blame link — `.../fixbolt/docs/GUIDE.md`
        # carries no verb at all, and rule (a) below would still judge it
        # (own-repo path, any matching tail is evidence) if this returned
        # early. Fall through to that tail search instead of rule (c), which
        # cannot apply here anyway (own_or_former and rule (c)'s "not this
        # repository" test are mutually exclusive); only when the tail search
        # below finds nothing is this counted as not a file link.
    elif (
        # (c) a well-formed github.com file URL naming a repository that is
        # neither this one nor a former name of this one, **and whose owner
        # segment is not this repository's own owner**, is a citation of
        # another repository, full stop — never judged, whatever tail it
        # happens to share with this repository. Checked before (b)'s tail
        # search, which cannot otherwise tell "this repo's file, wrong path"
        # from "another repo's file that happens to sit at the same relative
        # path". The owner check keeps a wrong or misspelled repository name
        # **under this repository's own owner** from being waved through as
        # "foreign": that is far more likely a mistaken citation of this
        # repository than a real other repository, so it falls through to
        # (b) and is judged there. Reached only when `own_or_former` above is
        # false — the two conditions are mutually exclusive by construction
        # (this branch requires the repo segment to be neither the current
        # name nor a former one; `own_or_former` requires the opposite).
        not own
        and head
        and head[0] == "github.com"
        and len(parts) >= 6
        and parts[3] in GITHUB_FILE_VERBS
        and parts[4]
        and parts[2].lower() not in (OWN_REPO[2],) + FORMER_NAMES
        and parts[1].lower() != OWN_REPO[1].lower()
    ):
        return None, None, True, None

    ignored_tail = None
    for i in range(len(parts)):
        tail = "/".join(parts[i:])
        if not tail or "." not in parts[-1]:
            continue
        if not os.path.isfile(os.path.join(root, tail)):
            continue
        if own or "/" in tail:
            return tail, None, False, None
        ignored_tail = tail  # rule (b): a bare filename, not judged
    if own_or_former:
        # The tail search above found nothing for an own-repository URL with
        # no checkable verb (no verb at all, or a verb outside
        # OWN_REPO_PATH_VERBS such as `actions/…`, `pull/…`, `commit/<sha>`,
        # or the bare repository root) — counted as not a file link rather
        # than silently returned as if it were an ordinary non-repository URL.
        return None, None, False, ("not_file_link", None)
    return None, ignored_tail, False, None


def main():
    root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    dead, checked, scanned = [], 0, 0

    absolute = []
    missing = []
    ignored_bare = 0
    foreign_named = 0
    own_checked = 0
    own_not_file_link = 0

    for rel in source_files(root):
        scanned += 1
        base = os.path.dirname(rel)
        with open(os.path.join(root, rel), encoding="utf-8") as fh:
            text = fh.read()
        for match in list(LINK.finditer(text)) + list(REFLINK.finditer(text)) + list(
            AUTOLINK.finditer(text)
        ):
            # A Markdown inline link's optional title (`[x](url "title")`)
            # is stripped before the anchor, since the anchor split assumes
            # the target ends at `#...` and a title would otherwise ride
            # along as part of the "path" tested below.
            target = strip_link_title(match.group(1).strip()).split("#")[0].strip()
            if target.startswith(URL):
                tail, ignored_tail, foreign, own_repo = names_a_repo_file(root, target)
                if own_repo:
                    kind, path = own_repo
                    line = text[: match.start()].count("\n") + 1
                    if kind == "not_file_link":
                        own_not_file_link += 1
                        continue
                    if not path:
                        # A bare ref (`tree/main`, `blob/main`) names no
                        # particular file — always confirmed to exist (it is
                        # this repository's own root), never reported, and
                        # not counted toward OWN_REPO_FLOOR: counting it would
                        # let the floor stay green while the verb match that
                        # actually matters (a real path after the ref) had
                        # silently stopped firing.
                        continue
                    own_checked += 1
                    resolved = resolve_case_exact(root, path)
                    if resolved is None:
                        # Covers a wrong path, a wrong case on any segment —
                        # macOS resolves `docs/design.md` to the real
                        # `docs/DESIGN.md` and this walk does not — and a
                        # `..` segment, which is refused outright rather than
                        # followed out of the tree.
                        missing.append((rel, line, target, path))
                        continue
                    if not os.path.isfile(resolved):
                        # A real directory — rule (d) has confirmed it
                        # exists; there is no "use a relative path instead"
                        # precedent for a directory link, so this is not
                        # judged any further.
                        continue
                    # A real file: same "use a relative path" treatment as
                    # rule (a)/(b), README's exemption included — that
                    # exemption waives "must be relative", never "must
                    # exist", which was already tested above.
                    if rel == "crates/library/README.md":
                        checked += 1
                        continue
                    absolute.append((rel, line, target, path))
                    continue
                if foreign:
                    foreign_named += 1
                    continue
                if ignored_tail:
                    ignored_bare += 1
                    continue
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

    floor_broken = own_checked < OWN_REPO_FLOOR

    print(
        f"{scanned} markdown and rust files, {checked} internal links checked, "
        f"{len(absolute)} absolute URLs naming a file in this repository, "
        f"{ignored_bare} foreign URLs sharing only a filename with this repository (not judged), "
        f"{foreign_named} foreign GitHub URLs naming another repository (not judged), "
        f"{own_checked} absolute URLs into this repository checked against its tree, "
        f"{own_not_file_link} own-repository URLs that are not file links (not judged)"
    )

    if absolute:
        print(
            f"\nFAIL: {len(absolute)} link(s) name a repository file by absolute URL",
            file=sys.stderr,
        )
        for rel, line, target, tail in absolute:
            print(f"  {rel}:{line}  →  {target}", file=sys.stderr)
            print(f"      this repository has {tail}; link it by relative path", file=sys.stderr)

    if missing:
        print(
            f"\nFAIL: {len(missing)} absolute URL(s) into this repository name a path it does not have",
            file=sys.stderr,
        )
        for rel, line, target, path in missing:
            print(f"  {rel}:{line}  →  {target}", file=sys.stderr)
            print(f"      this repository has no {path}", file=sys.stderr)

    if dead:
        print(f"\nFAIL: {len(dead)} dead internal link(s)\n", file=sys.stderr)
        for rel, line, target in dead:
            print(f"  {rel}:{line}  →  {target}", file=sys.stderr)

    if floor_broken:
        print(
            f"\nFAIL: the own-repository URL rule checked {own_checked} URLs, below its floor "
            f"of {OWN_REPO_FLOOR} — crates/library/README.md alone carries {OWN_REPO_FLOOR}, "
            "so the rule has stopped matching",
            file=sys.stderr,
        )

    if dead or absolute or missing or floor_broken:
        return 1

    print("no dead internal links")
    return 0


if __name__ == "__main__":
    sys.exit(main())
