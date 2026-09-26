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

# The README of each crate published to crates.io (ADR-0160). Each is shipped in
# its `.crate` and rendered on crates.io and docs.rs, where a relative link into
# the repository's `docs/` breaks — so these, like `crates/library/README.md`
# before them, may (and must) name repository files by absolute URL. The
# exemption waives rule (a)'s "must be relative" only, never "must exist".
PUBLISHED_READMES = frozenset(
    f"crates/{c}/README.md" for c in ("codec", "dict", "session", "engine", "sbe", "library")
)

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
    must_be_absolute = []
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
                    if rel in PUBLISHED_READMES:
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
                    if rel in PUBLISHED_READMES:
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
                continue
            # Senior review of PR #104: the exemption above (`absolute.append`
            # skipped for `rel in PUBLISHED_READMES`) only ever WAIVED the
            # "must be relative" message for a link that was already
            # absolute — it never asked whether a *relative* link in one of
            # these six files can survive being published. `crates.io` and
            # `docs.rs` render a README as the crate's own front page, with no
            # access to anything above the crate's own directory (only
            # `include`'s allowlist ships in the `.crate` — ADR-0160 decision
            # 4) — so a relative link that climbs out of the crate directory
            # (any leading `..` segment) is dead the moment it is published,
            # even though it resolves perfectly well inside this checkout.
            # `crates/library/README.md:87` citing `../../docs/decisions/...`
            # was exactly this: real inside the repo, 404 on docs.rs.
            if rel in PUBLISHED_READMES and target.split("#")[0].startswith(".."):
                line = text[: match.start()].count("\n") + 1
                must_be_absolute.append((rel, line, target))

    floor_broken = own_checked < OWN_REPO_FLOOR

    print(
        f"{scanned} markdown and rust files, {checked} internal links checked, "
        f"{len(absolute)} absolute URLs naming a file in this repository, "
        f"{ignored_bare} foreign URLs sharing only a filename with this repository (not judged), "
        f"{foreign_named} foreign GitHub URLs naming another repository (not judged), "
        f"{own_checked} absolute URLs into this repository checked against its tree, "
        f"{own_not_file_link} own-repository URLs that are not file links (not judged)"
    )
    sys.stdout.flush()  # the summary goes out before any FAIL on stderr, even through a pipe

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

    if must_be_absolute:
        print(
            f"\nFAIL: {len(must_be_absolute)} relative link(s) in a published README "
            "climb out of the crate directory — dead on crates.io/docs.rs",
            file=sys.stderr,
        )
        for rel, line, target in must_be_absolute:
            print(f"  {rel}:{line}  →  {target}", file=sys.stderr)
            print(
                "      published READMEs render outside this repository tree; "
                "use an absolute GitHub URL instead",
                file=sys.stderr,
            )

    if floor_broken:
        print(
            f"\nFAIL: the own-repository URL rule checked {own_checked} URLs, below its floor "
            f"of {OWN_REPO_FLOOR} — crates/library/README.md alone carries {OWN_REPO_FLOOR}, "
            "so the rule has stopped matching",
            file=sys.stderr,
        )

    if dead or absolute or missing or floor_broken or must_be_absolute:
        return 1

    print("no dead internal links")
    return 0


# --- --rendered mode -------------------------------------------------------
#
# Everything above reads Markdown/Rust source and never fetches a URL. This
# mode reads mdBook's own rendered HTML output instead (docs/plans/2026-09-26-
# docs-for-embedders.md step 4) and checks something the source-only mode
# structurally cannot: that an in-book link's `#anchor` names a heading (or
# any other `id=`) that actually exists on the page mdBook wrote -- not just
# that the page itself exists. It is a second, independent mode selected by
# `--rendered <dir>`; nothing above this comment changes behaviour, and this
# mode never runs unless `--rendered` is given.
#
# scripts/mdbook-repo-links.py (ADR-0206 decision 5) rewrites a link that
# leaves the book into a `github.com/.../blob/<sha>/...#anchor` URL, written
# by the *author*, who only ever sees mdBook's own rendering -- so the
# fragment they wrote is mdBook's heading id, not GitHub's. GitHub renders a
# `.md` file it serves as a "blob" with its own heading-anchor slugger, which
# does not always agree with mdBook's (pulldown-cmark's, via mdbook-html):
# this mode cannot fetch github.com to find out (the same "no network calls"
# reasoning as the mode above), so instead it measures every heading actually
# rendered on this tree, computes what GitHub's own slug rule would produce
# from the same heading text, and prints every place the two disagree. This
# is printed, not asserted red: it says which existing `#anchor` fragments
# would 404 if that same page were ever read as a GitHub blob instead of a
# book page (including, but not limited to, the ones this preprocessor
# itself rewrites), so a human can judge whether that specific instance
# matters, rather than the gate assuming every disagreement does.
#
# What this mode cannot see: the `#anchor` on a link the preprocessor
# rewrote to a GitHub URL. That href is external here (skipped with every
# other `https://` href), its target is a file the book does not render
# (docs/plans/, crates/..., a path SUMMARY.md does not list), so no rendered
# page holds its ids, and the source mode above strips anchors before it
# checks anything. A wrong fragment on such a link -- `#L120` past the end of
# a .rs file, a heading renamed in a plan -- is checked by neither mode.
from html.parser import HTMLParser  # noqa: E402  (kept beside its one user)

HEADING_TAGS = {"h1", "h2", "h3", "h4", "h5", "h6"}
RENDERED_EXTERNAL = ("http://", "https://", "mailto:", "javascript:", "tel:")


class _PageParser(HTMLParser):
    """Collects, from one rendered HTML page: every element `id=`, every
    `<a href="...">` with the source line it appeared on, and the (id, text)
    of every heading that carries an id -- mdBook always gives a heading an
    id when it has one, so a heading without one (`<h1 class="menu-title">`,
    the sidebar's book title) is simply not a heading anyone can link to.
    """

    def __init__(self):
        super().__init__(convert_charrefs=True)
        self.ids = set()
        self.links = []  # [(href, lineno)]
        self.headings = []  # [(id, text)]
        self._heading_stack = []  # [[id_or_None, [text_parts]]]

    def handle_starttag(self, tag, attrs):
        attrs = dict(attrs)
        hid = attrs.get("id")
        if hid:
            self.ids.add(hid)
        if tag == "a" and attrs.get("href"):
            self.links.append((attrs["href"], self.getpos()[0]))
        if tag in HEADING_TAGS:
            self._heading_stack.append([hid, []])

    def handle_endtag(self, tag):
        if tag in HEADING_TAGS and self._heading_stack:
            hid, parts = self._heading_stack.pop()
            if hid:
                self.headings.append((hid, "".join(parts).strip()))

    def handle_data(self, data):
        if self._heading_stack:
            self._heading_stack[-1][1].append(data)


def _parse_page(path):
    parser = _PageParser()
    with open(path, encoding="utf-8") as fh:
        parser.feed(fh.read())
    return parser


def _html_files(root):
    for dirpath, _dirnames, filenames in os.walk(root):
        for name in sorted(filenames):
            if name.endswith(".html"):
                yield os.path.join(dirpath, name)


def github_heading_slug(text):
    """GitHub's own Markdown heading-anchor slug (the `github-slugger`
    algorithm GitHub itself uses): lowercase, drop anything that is not a
    letter, digit, space, hyphen or underscore, then turn EACH remaining
    whitespace character into its own hyphen -- a run of whitespace is NOT
    collapsed to one hyphen, which is why `D1 — The session layer` (a space,
    an em dash, a space) slugs to `d1--the-session-layer`, two hyphens, not
    one: the em dash is dropped by the first step, leaving two adjacent
    spaces, and each becomes its own hyphen.

    `[measured 2026-09-26]` the first version of this collapsed whitespace
    with `\\s+` -> a single hyphen, matching nothing else's actual behaviour:
    it turned 1710 of this tree's own headings red as "differs from mdBook"
    on this repository's very first `--rendered` run, essentially every
    heading built from an em dash or any other punctuation-only run between
    two words -- mdBook's own id for `DESIGN.md`'s "fixbolt — Design" is
    `fixbolt--design` (double hyphen), read directly off the rendered page,
    not off a written description of the algorithm.

    This function returns the *base* slug only; a heading text repeated on
    the same page needs GitHub's "-1", "-2", ... de-duplication applied on
    top, in heading order -- done by the caller (main_rendered), which is
    the one that knows a page's full heading order. `print.html`, mdBook's
    single concatenated everything-page, repeats headings like "Related" and
    "What is not proven" dozens of times and needed exactly this: without
    per-page dedup, every occurrence after the first on that page reported a
    false difference (938 of them, `[measured 2026-09-26]`, almost the whole
    finding).
    """
    text = text.strip().lower()
    text = re.sub(r"[^\w\s-]", "", text, flags=re.UNICODE)
    return re.sub(r"\s", "-", text)


def main_rendered(root):
    if not os.path.isdir(root):
        print(f"FAIL: {root} is not a directory -- run `mdbook build` first", file=sys.stderr)
        return 1

    pages = {}
    for path in _html_files(root):
        rel = os.path.relpath(path, root)
        pages[rel] = _parse_page(path)

    dead_files = []
    dead_anchors = []
    checked = 0
    skipped_external = 0

    for rel, page in sorted(pages.items()):
        base_dir = os.path.dirname(rel)
        for href, lineno in page.links:
            target, _, anchor = href.partition("#")
            if not target and not anchor:
                continue  # a bare `#` with nothing after it links nowhere in particular
            if target.startswith(RENDERED_EXTERNAL):
                skipped_external += 1
                continue
            if target.startswith("/"):
                # Site-root-relative (mdBook's own 404.html "go home" link).
                # Verifying it needs the deployment's base path (a GitHub
                # Pages project site is not served at "/"), which is a
                # deploy-configuration fact this mode does not have -- job
                # `book` (step 6) owns that, not this file.
                skipped_external += 1
                continue
            target_rel = rel if not target else os.path.normpath(os.path.join(base_dir, target))
            checked += 1
            if target_rel not in pages:
                dead_files.append((rel, lineno, href))
                continue
            if anchor and anchor not in pages[target_rel].ids:
                dead_anchors.append((rel, lineno, href, target_rel))

    slug_diffs = []
    for rel, page in sorted(pages.items()):
        # A repeated heading's *first* occurrence keeps the bare slug; each
        # later one is disambiguated by appending "-1", "-2", ... in the
        # order headings appear on the page -- both GitHub's slugger and
        # mdBook's own do this, so it has to be replicated per page, not
        # just per heading, or a page that repeats a heading (`print.html`
        # concatenates the whole book, so this is common there) reports a
        # false difference on every occurrence after the first.
        occurrences = {}
        seen_ids = set()
        for hid, text in page.headings:
            if hid in seen_ids:
                continue  # the same id can appear twice in one page's markup (e.g. a duplicated anchor link); judge it once
            seen_ids.add(hid)
            base_slug = github_heading_slug(text)
            occurrences[base_slug] = occurrences.get(base_slug, 0) + 1
            count = occurrences[base_slug]
            slug = base_slug if count == 1 else f"{base_slug}-{count - 1}"
            if slug != hid:
                slug_diffs.append((rel, hid, slug, text))

    print(
        f"{len(pages)} rendered pages, {checked} internal hrefs checked, "
        f"{skipped_external} external/site-absolute hrefs skipped, "
        f"{len(slug_diffs)} heading(s) where GitHub's slug would differ from mdBook's id"
    )
    sys.stdout.flush()

    if slug_diffs:
        print(
            "\nanchors where GitHub's slug differs from mdBook's id (measured on this tree, "
            "not a failure by itself -- see the note above main_rendered):"
        )
        for rel, hid, slug, text in slug_diffs:
            print(f'  {rel}#{hid}  ->  GitHub would slug "{text}" as #{slug}')

    if dead_files:
        print(
            f"\nFAIL: {len(dead_files)} internal href(s) point at a page this build did not render",
            file=sys.stderr,
        )
        for rel, lineno, href in dead_files:
            print(f"  {rel}:{lineno}  →  {href}", file=sys.stderr)

    if dead_anchors:
        print(
            f"\nFAIL: {len(dead_anchors)} internal href(s) name an anchor missing from the target page",
            file=sys.stderr,
        )
        for rel, lineno, href, target_rel in dead_anchors:
            print(f"  {rel}:{lineno}  →  {href}  ({target_rel} has no matching id)", file=sys.stderr)

    if dead_files or dead_anchors:
        return 1

    print("no dead internal hrefs or anchors in the rendered book")
    return 0


if __name__ == "__main__":
    if len(sys.argv) >= 3 and sys.argv[1] == "--rendered":
        sys.exit(main_rendered(sys.argv[2]))
    sys.exit(main())
