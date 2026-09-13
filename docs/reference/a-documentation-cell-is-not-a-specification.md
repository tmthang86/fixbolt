# A documentation cell is not a specification

`[researched 2026-09-13]`

A review found a value the parser accepts and a documentation cell forbids, and read the
disagreement as the engine being loose. The opposite was true: the protocol defines the
value, the engine had implemented and documented that definition two levels down, and the
one wrong half was the cell. Nothing in `crates/*/src` changed. The lesson is about which
of two disagreeing texts gets to decide, and it is not "whichever you read second".

## What was found

`docs/CONFIGURATION.md` §1 listed `HeartBtInt` as *positive integer, seconds*. The senior
review of PR [#68](https://github.com/tmthang86/fixbolt/pull/68) wrote a throwaway probe,
fed `HeartBtInt=0` to `Settings::parse`, and watched it come back `Ok`. Probe 6 in
`mod doc_table` checks that an integer is *written* as digits; probe 3 checks
enumerations; neither reads a range, so nothing in the test suite had an opinion. The
review recorded it as *"refusing zero, or changing the cell, is a behaviour decision for
the owner"* — STATUS.md item 76, as first written.

## What was true

FIX 4.4 defines `108=0`: no heartbeats are generated at all, and a `TestRequest` still
forces one ([B2BITS, tag 108](https://www.b2bits.com/fixopaedia/fixdic44/tag_108_HeartBtInt_.html);
[OnixS, `msgtype 0`](https://www.onixs.biz/fix-dictionary/4.4/msgtype_0_0.html)). Zero is a
value with a meaning, not a value outside the range.

And this repository already knew. `crates/session/src/lib.rs:1322` documents the field:
*"zero means no heartbeats at all — which is what FIX 4.4 says `108=0` means"*; `:2262`
repeats it as a comment on the rule, and `:2263` is the rule — `if self.beat_ms == 0 {
return Link::Up; }`. `crates/engine/src/settings.rs:1522` takes any `u32` for the key with
no range check, and the absence of a check is the design, not an omission. Three places
in the code agreed with the specification. One cell in a lookup table disagreed with all
four.

## Why the review read it backwards

The cell was the thing under test. `doc_table` exists to make `docs/CONFIGURATION.md` §1
say what the parser does, so when the two disagreed, the reflex was that the document was
the oracle and the parser had drifted from it. That reflex is right for most of §1 — a
cell listing `Y` and `N` is a claim about the parser, and the parser is what it describes.
It is wrong for a cell that restates a *protocol* fact, because there the cell and the
parser are both claims about a third thing, and the third thing is written down somewhere
neither of them controls.

A documentation cell is a restatement. It is downstream of the specification exactly as
the code is, and it can be wrong in exactly the same ways. When it disagrees with the
code, the disagreement says one of them is wrong; it does not say which.

## The rule

**When a cell and the code disagree about a value's meaning, the protocol decides — read
it before changing either.** Concretely, in this repository, before a `[DEFAULT]`-section
value is refused or a *Values* cell is rewritten: find the FIX 4.4 definition of the tag
the key maps to, quote it, and only then touch whichever half disagrees with it. Item 76
closed by changing the cell, in the style line 279 already used for `logout_timeout_ms`
(*"zero is off"*), and by touching no line of source.

## Where the gate was blind, and what closes it

The real gap was not a wrong cell; a cell can be wrong and be found. The gap was that **no
probe read a range**. Probe 6 proves an integer cell is read as written; probe 3 proves an
enumeration lists exactly the parser's literals. A cell that says *positive* while the
parser accepts `0` passed both, and would have kept passing. The regression test this
entry owes under `CLAUDE.md` §4 is a probe that reads a *Values* cell's stated bound and
feeds the parser the first value outside it: a cell saying *positive* must see `0`
refused, a cell saying *non-negative* or naming `0` must see it accepted, and a cell of the
shape `` `a`–`b` `` must see `b+1` refused. Reverting the `HeartBtInt` cell to *positive
integer* must turn that probe red on the sentence that names the key. Plan:
[what-the-residue-review-found](../plans/2026-09-13-what-the-residue-review-found.md).

## The generalised lesson

**A description of a system is not the system's specification, even when it sits in the
repository's own documentation and a test compares the two.** Two artefacts that both
restate a third source can disagree with each other while one of them is right; a gate
comparing them can only report the disagreement, and a reader who picks the artefact the
gate was *built to protect* as the oracle is choosing by habit. The tell is a claim about
*meaning* — what a value denotes, what a range is for — rather than about *form*: form is
the parser's to define, meaning is the protocol's. For a claim about meaning, the fix
begins by opening the third source, and a probe that reads the *stated* bound — rather than
the shape of the digits — is the one that would have caught the wrong half first.

## Related

- [a-doc-gate-never-opened-the-file-it-was-guarding](a-doc-gate-never-opened-the-file-it-was-guarding.md)
  — the same document set, the other failure: a gate green about a file it had never read.
- [a-known-limitations-list-rots-in-one-direction](a-known-limitations-list-rots-in-one-direction.md)
  — why a review's first sentence about an item is corrected in place rather than left to
  stand: item 76's first text claimed a behaviour decision existed where none did.
