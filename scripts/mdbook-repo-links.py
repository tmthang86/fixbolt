#!/usr/bin/env python3
r"""An mdBook preprocessor: a link that leaves docs/, or points at a file under
docs/ that docs/SUMMARY.md does not list, is rewritten to a GitHub blob (file)
or tree (directory) URL at the commit being built; a link that stays inside
the book is left alone (ADR-0206 decision 5).

Two things it fixes on top of that, both found rendering the book in step 1:

  - `docs/index.md` is served at `internals/index.html`
    A link to a `README.md` (any directory, not just docs/internals/) is
    served by mdBook's own built-in "index" preprocessor at `index.html`,
    never `README.html`. A link naming `README.md` explicitly is rewritten to
    name `index.md` instead, so mdBook's ordinary `.md` -> `.html` link
    resolution lands on the file mdBook actually writes. This is a plain
    in-book link both before and after the rewrite -- never a GitHub URL.
  - A link to a bare directory (`decisions/`, `plans/`) names nothing mdBook
    can serve -- there is no single chapter file for a directory -- so it
    falls out of "stays inside the book" the same way a file SUMMARY.md
    doesn't list does, and becomes a GitHub tree URL.

Protocol: mdBook's preprocessor protocol (https://rust-lang.github.io/mdBook/
for_developers/preprocessors.html), read from mdbook-preprocessor 0.5.4's own
source (`~/.cargo/registry/.../mdbook-preprocessor-0.5.4/src/lib.rs` and
`mdbook-driver-0.5.4/src/builtin_preprocessors/cmd.rs`, since the rendered
guide for 0.5.x was not trusted blind): mdBook calls this script twice.

  1. `mdbook-repo-links.py supports <renderer>` with stdin closed -- exit 0
     means "yes", anything else means "no". This preprocessor supports every
     renderer, so it always exits 0 without reading stdin.
  2. With no arguments: a JSON array `[context, book]` on stdin (`context` is
     a `PreprocessorContext` -- `root` is the directory holding `book.toml`,
     i.e. the repository root, not `docs/`; `config.book.src` is `"docs"`,
     read from the context rather than hard-coded so a future `src` rename
     does not silently break this script). It must print the (possibly
     modified) `book` object alone -- not the pair -- as JSON on stdout, and
     exit 0. mdBook parses that stdout as the `Book` it renders next.

Only two Markdown link shapes are rewritten: `[text](target)` and the
reference form `[label]: target` -- the same two `scripts/check-links.py`
reads, and the same regexes, so the two scripts agree on what counts as a
link without maintaining two definitions of it. A fenced code block is left
untouched even when it contains something that looks like a link:
`docs/decisions/ADR-0091-...md` quotes literal `[x](y)`-shaped text inside a
```markdown fence as an illustration, and rewriting that would corrupt the
quotation, not fix a link. An inline (single-backtick) code span is NOT
given the same protection, on purpose: this repository's reference-style
links routinely backtick just the label (`` [`Recovery`]: ../../crates/... ``,
docs/decisions/ADR-0088-...md), and a first version of this script split
inline code out of the text before matching, which tore that exact line in
two and silently stopped rewriting it. `grep -rn '`.*`\]\(' docs/ docs/**/*.md`
(then repeated for the reference-link form) found zero cases where an inline
code span sits inside an *illustrative* link in this docs tree, so there is
nothing today that protection would have earned; the fenced-block case is
the one that is real, and is the only one implemented.

An external URL (`http(s)://`, `mailto:`) and a bare `#anchor` (in-page) are
never touched. Everything else is resolved relative to the *chapter's own*
directory inside `docs/` -- exactly how a browser would resolve it -- and
classified by where it lands:

  - Outside `docs/` entirely (`../crates/codec/src/dict.rs`) -> a GitHub blob
    URL naming the resolved path from the repository root.
  - Inside `docs/`, and that path is one `docs/SUMMARY.md` links to -> left
    as a relative link (README.md -> index.md rewrite aside).
  - Inside `docs/`, but not a path `docs/SUMMARY.md` links to (`docs/plans/`,
    a bare directory) -> a GitHub blob or tree URL naming `docs/<path>`.

"Is it a file or a directory" is answered by looking at the real checkout,
not guessed from the spelling of the link -- a trailing slash is not required
(`[decisions/](decisions/)` and `[decisions/](decisions)` land the same way).

The commit is `git rev-parse HEAD` by default, overridable by the
FIXBOLT_BOOK_SHA environment variable so CI can pin it to the exact commit
under test rather than whatever HEAD happens to be at build time (a shallow
CI checkout can otherwise leave HEAD detached at a merge commit that never
lands on the default branch).

Run (by mdBook, via book.toml's [preprocessor.repo-links] table): no
arguments, JSON on stdin. Run by hand for a smoke test: pipe the same JSON
mdBook would send. scripts/check-mdbook-repo-links.py exercises the pure
`rewrite_target` function directly instead.
"""

import json
import os
import posixpath
import re
import subprocess
import sys

OWN_REPO = "tmthang86/fixbolt"

# The same two link shapes, and the same regex for the inline one, as
# scripts/check-links.py's LINK -- kept in sync by eye, not by import,
# because that script deliberately stays read-only (never rewrites) and this
# one deliberately rewrites; sharing an import would tie their futures
# together for no shared benefit.
LINK = re.compile(r"\[[^\]]*\]\(([^)]+)\)")
REFLINK = re.compile(r"^([ \t]*\[[^\]]+\]:[ \t]*)(\S+)([ \t]*)$", re.M)

FENCE = re.compile(r"^```.*?^```", re.M | re.S)

EXTERNAL_PREFIXES = ("http://", "https://", "mailto:")


def resolve_sha(env=None):
    env = os.environ if env is None else env
    override = env.get("FIXBOLT_BOOK_SHA")
    if override:
        return override
    out = subprocess.run(
        ["git", "rev-parse", "HEAD"], capture_output=True, text=True, check=True
    )
    return out.stdout.strip()


def load_summary_paths(docs_root_abs):
    """The docs/-relative paths docs/SUMMARY.md actually links to -- the
    book's own, already-authoritative definition of "in the book" (ADR-0206
    decisions 5 and 6). Read with the same two regexes as everything else, so
    a hand-written SUMMARY.md entry and one scripts/gen-book-summary.py
    generated are read identically.
    """
    path = os.path.join(docs_root_abs, "SUMMARY.md")
    try:
        with open(path, encoding="utf-8") as fh:
            text = fh.read()
    except FileNotFoundError:
        return set()
    targets = LINK.findall(text) + [m[1] for m in REFLINK.findall(text)]
    paths = set()
    for target in targets:
        if target.startswith(EXTERNAL_PREFIXES) or target.startswith("#"):
            continue
        clean = target.split("#", 1)[0].strip()
        if clean:
            paths.add(posixpath.normpath(clean))
    return paths


def _fence_spans(text):
    """Character ranges of every fenced code block in `text`, computed fresh
    against whatever the current text is (never cached across a rewrite,
    since a substitution changes every later offset).
    """
    return [(m.start(), m.end()) for m in FENCE.finditer(text)]


def _inside(pos, spans):
    return any(start <= pos < end for start, end in spans)


def github_url(repo_rel, repo_root_abs, sha, anchor):
    is_dir = os.path.isdir(os.path.join(repo_root_abs, repo_rel))
    verb = "tree" if is_dir else "blob"
    url = f"https://github.com/{OWN_REPO}/{verb}/{sha}/{repo_rel}"
    if anchor:
        url += "#" + anchor
    return url


def rewrite_target(target, chapter_dir, docs_root_name, known_paths, repo_root_abs, sha):
    """The new link text for `target`, written inside a chapter whose own
    file sits at `chapter_dir` (posix, relative to docs/ root; "" for a
    top-level chapter) -- or `None` when `target` should be left exactly as
    written.
    """
    path_part, _, anchor = target.partition("#")
    if not path_part or path_part.startswith(EXTERNAL_PREFIXES):
        return None

    docs_rel = posixpath.normpath(posixpath.join(chapter_dir, path_part))

    if docs_rel == ".." or docs_rel.startswith("../"):
        # Leaves docs/ altogether -- resolve against the repository root.
        repo_rel = posixpath.normpath(
            posixpath.join(docs_root_name, chapter_dir, path_part)
        )
        return github_url(repo_rel, repo_root_abs, sha, anchor)

    if docs_rel in known_paths:
        # An in-book page. README.md -> index.md is the one rewrite an
        # in-book link still needs: mdBook's built-in "index" preprocessor
        # serves every README.md at index.html, never README.html, and that
        # rename is invisible to the generic .md -> .html link resolution
        # this preprocessor otherwise leaves alone.
        if path_part.endswith("README.md"):
            new_path_part = path_part[: -len("README.md")] + "index.md"
            return new_path_part + (("#" + anchor) if anchor else "")
        return None

    # Under docs/, but not a page docs/SUMMARY.md links to: docs/plans/ (by
    # name, ADR-0206 decision 5), a bare directory such as docs/decisions/,
    # or any other file the book simply does not carry.
    repo_rel = posixpath.join(docs_root_name, docs_rel)
    return github_url(repo_rel, repo_root_abs, sha, anchor)


def _sub_group(match, group_index, new_value):
    if new_value is None:
        return match.group(0)
    whole = match.group(0)
    start = match.start(group_index) - match.start(0)
    end = match.end(group_index) - match.start(0)
    return whole[:start] + new_value + whole[end:]


def rewrite_content(content, chapter_dir, docs_root_name, known_paths, repo_root_abs, sha):
    """Rewrite every link in `content`, except one sitting inside a fenced
    code block (an illustration, not a link -- see the module docstring's
    ADR-0091 example).

    Fence spans are recomputed immediately before each regex pass rather than
    once up front: `LINK.sub` below can change the text's length, which
    would silently invalidate any span computed against the text before that
    substitution ran.
    """

    def rewrite_link(m):
        if _inside(m.start(), fence_spans):
            return m.group(0)
        return _sub_group(
            m,
            1,
            rewrite_target(m.group(1), chapter_dir, docs_root_name, known_paths, repo_root_abs, sha),
        )

    fence_spans = _fence_spans(content)
    content = LINK.sub(rewrite_link, content)

    def rewrite_reflink(m):
        if _inside(m.start(), fence_spans):
            return m.group(0)
        return _sub_group(
            m,
            2,
            rewrite_target(m.group(2), chapter_dir, docs_root_name, known_paths, repo_root_abs, sha),
        )

    fence_spans = _fence_spans(content)
    content = REFLINK.sub(rewrite_reflink, content)
    return content


def process_book(book, docs_root_name, known_paths, repo_root_abs, sha):
    def process_items(items):
        for item in items:
            if isinstance(item, dict) and "Chapter" in item:
                chapter = item["Chapter"]
                path = chapter.get("path")
                if path:
                    chapter_dir = posixpath.dirname(path.replace(os.sep, "/"))
                    chapter["content"] = rewrite_content(
                        chapter["content"],
                        chapter_dir,
                        docs_root_name,
                        known_paths,
                        repo_root_abs,
                        sha,
                    )
                process_items(chapter.get("sub_items", []))

    process_items(book.get("items", []))
    return book


def main(argv):
    if argv and argv[0] == "supports":
        # This preprocessor only rewrites Markdown link text; nothing it
        # does depends on which renderer consumes the result.
        return 0

    context, book = json.load(sys.stdin)
    sha = resolve_sha()
    docs_root_name = context.get("config", {}).get("book", {}).get("src", "src")
    repo_root_abs = context.get("root") or os.getcwd()
    docs_root_abs = os.path.join(repo_root_abs, docs_root_name)
    known_paths = load_summary_paths(docs_root_abs)

    book = process_book(book, docs_root_name, known_paths, repo_root_abs, sha)

    json.dump(book, sys.stdout)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
