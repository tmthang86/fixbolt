# A bare filename is not evidence of a repository

`[measured 2026-09-13]`

A doc-link gate that compared a URL's tail against this repository's own files, from the
right, read a citation of another project's `CHANGELOG.md` as a link to this repository's
own, and failed the build about a link that was correct. A false red, not a silence — and
the fix had to avoid trading it for a silence.

## What the gate was checking

`scripts/check-links.py` exists to catch an absolute URL that names a path this repository
actually has, where a relative link was meant instead — the two real bad links named in
the script's own docstring, in `crates/engine/src/dispatch.rs` and
`crates/session/src/journal.rs`, were both `https://github.com/fixbolt/docs/decisions/...`,
a wrong organisation that a
right-to-left comparison of the URL's tail against every file in the repository catches
regardless of which organisation, host, or branch name the wrong URL used.

## Where it over-matched

`names_a_repo_file` walked the URL's path segments from the left, building shorter and
shorter tails, and returned the first one that happened to exist as a file in this
repository — **any** tail, as long as `parts[-1]` carried a dot. The shortest possible tail
is the bare filename alone. `CHANGELOG.md`, `README.md` and `LICENSE` are files this
repository has at its root, and they are also files that thousands of other repositories
have at theirs.

A citation of `https://github.com/EmbarkStudios/cargo-deny/blob/main/CHANGELOG.md` —
`cargo-deny` is a real dependency-auditing tool, and its changelog is a reasonable thing to
cite in passing — matched this repository's own `CHANGELOG.md` on that last, bare segment.
The gate reported it exactly as it reports a genuinely wrong link — `FAIL: 1 link(s) name a
repository file by absolute URL`, then `this repository has CHANGELOG.md; link it by relative
path` — a sentence correct about the file and wrong about the URL. `[measured 2026-09-13]` the
plan that fixed it went red on the old script for exactly that URL, in its own draft, when it
wrote the URL as a markdown link to cite it as an example; on the new script the same link
reads `1 foreign URLs sharing only a filename with this repository (not judged)`.

## The two rules that replaced the one

A single right-to-left tail match cannot tell "this repository's file, at the wrong path"
apart from "some other repository's file, which happens to share a name." The fix is not a
longer comparison — it is asking a different question depending on whose path the URL
names:

- **(a)** a URL into this repository's own GitHub path — `OWN_REPO`, compared by segment
  (`["github.com", "tmthang86", "fixbolt"]`), never by string prefix, so a sibling
  repository like `fixbolt-other` does not match on a shared prefix — is judged on **any**
  matching tail, even one segment. Its own file under the wrong sub-path is exactly the
  case this gate exists to catch. **Those three segments are compared without case, and a
  `www.` in front of the host is dropped**, because GitHub reads them that way:
  `[measured 2026-09-13]` compared as written, a senior review's
  `https://github.com/TmThang86/Fixbolt/blob/main/CHANGELOG.md` and
  `https://www.github.com/tmthang86/fixbolt/blob/main/CHANGELOG.md` — both this repository's
  own root file — were counted as foreign and not judged.
- **(b)** any other path is judged only when the matching tail carries **two or more**
  segments. A single segment is a filename, and a filename names no owner.

## The limit, stated rather than hidden

Rule (b) has a deliberate gap, and it is the same shape as the bug it replaces, just
narrower: an absolute URL to this repository's own root file, under the **wrong**
organisation — `https://github.com/fixbolt/CHANGELOG.md`, where `fixbolt` is not this
repository's owner — now passes through unreported. Closing that gap would require knowing
which organisations are *not* this repository, which is not a finite list. Rather than
leave that class invisible again, the gate counts it: a bare-filename match outside
`OWN_REPO` is printed in the summary line as a foreign URL "sharing only a filename with
this repository (not judged)" — seen, not silently dropped, the same choice `doc_table`
makes for a prose cell it cannot probe.

## The generalised lesson

**A matcher that compares from the right needs enough segments to name an owner; one
segment names a convention.** A filename by itself is metadata thousands of projects agree
on for free — `CHANGELOG.md`, `README.md`, `LICENSE`, `Cargo.toml` — and agreeing on a
convention is not the same fact as belonging to a particular repository. Any check that
walks a path backward to decide ownership inherits this: the more segments two things
share, the stronger the claim that they are the same thing, and a claim resting on one
shared segment is barely a claim at all. The fix here is not a smarter heuristic on the
filename; it is asking whether the claim is strong enough to act on, and saying out loud
when it is not.

## Related

- [a-matcher-excluded-the-separator-every-real-name-uses](a-matcher-excluded-the-separator-every-real-name-uses.md)
  — the same family from the opposite direction: a comparison built to reject a substring
  match ended up rejecting every real match too, because the character it excluded was the
  one every real instance actually used.
- [a-known-limitations-list-rots-in-one-direction](a-known-limitations-list-rots-in-one-direction.md)
  — why a limit a gate cannot close is written down and counted rather than left unsaid:
  the alternative is a document that keeps claiming less is proven than actually is, or, as
  here, a gate that claims more than it actually checked.

[to testing-skills]
