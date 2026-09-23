# ADR-0077 — The positioning stays acceptor-first, and "fastest" is said only beside a reproduced pair

- **Status**: **Accepted — 2026-09-18**, by the plan [closing-the-open-items](../plans/2026-09-18-closing-the-open-items.md), approved by the manager under the owner's 2026-09-18 mandate. Proposed the same day.
- **Superseded in part**: **Decision 2 superseded by [ADR-0099](ADR-0099-kernel-tcp-stays-the-default-and-the-headline-and-a-bypass-figure-is-a-second-labelled-row.md)** (2026-09-23). Decisions 1, 3 and 4 stand.
- **Date**: 2026-09-18
- **Deciders**: Tran Manh Thang
- **Related**: `PRD.md` §6 row 1, §1; `DESIGN.md` §1 *Positioning*; `README.md` line 3;
  [ADR-0004](ADR-0004-bidirectional-engine.md); [ADR-0068](ADR-0068-a-published-figure-is-two-procedures-shown-side-by-side.md);
  `docs/reference/measured-costs.md` *B8*; `CLAUDE.md` §2 non-negotiable 10
- **Answers**: PRD §6 open decision 1 — *does the headline positioning stay "the fastest
  acceptor on kernel TCP" now that the engine is bidirectional?*

## Context

The headline was written on 2026-08-27 when the acceptor was the gap in the Rust ecosystem
(two initiators, no production-proven acceptor — `reference/prior-art.md`). Since then the
engine became bidirectional (ADR-0004), the initiator is interop-green against `libquickfix`,
and the first head-to-head exists: `[measured 2026-09-15]` B8, `standard`, loopback, against
`matthart1983/nanofix` — fixbolt's admin p50 was +416 / −175 ns against nanofix across the two
procedures (sign changes: no difference claimed), app p50 +1 719 / +1 113 ns *slower*, and
fixbolt's own arms **did not reproduce** (5.2–5.6% at p50) while nanofix's did. `hft` was not
compared, because nanofix is thread-per-connection blocking and pairing it with `hft` is the
mode mixing non-negotiable 4 refuses.

So "the fastest" is, on the one measurement that exists, unsupported for `standard` and
unmeasured for `hft`. "Acceptor" is still where the ecosystem gap is and where every gate
points (59/59 through a socket, `tools/w2w` is an acceptor under load). What the search found
(2026-09-18): the sibling engines position on a property, not a superlative — nanofix on
*"ultra-low-latency"*, Artio on *"resilient, high-performance"*, QuickFIX on being the
reference. None claims *fastest*.

## Decision

1. **Acceptor-first stays.** `DESIGN.md` §1 and `README.md` keep the acceptor as the headline
   and say the initiator ships in the same engine; the sentence that the initiator is a
   second role sharing one session core (ADR-0004) is kept.
2. **"The fastest" is retired from every headline** and replaced with the property the
   design guarantees and the gates check: *a FIX 4.4 acceptor on kernel TCP whose latency is
   a published, reproduced number* — `README.md` line 3, `DESIGN.md` §1 *Positioning*,
   `PRD.md` §1 and `CLAUDE.md`'s first paragraph (the rules file quotes the positioning).
3. **A superlative may appear only beside the pair that supports it**: a `DESIGN.md` §8 or
   `measured-costs.md` sentence of the form *"faster than X by N% at p50/p99/p99.9, two
   procedures, reproduced, same machine, same mode"* — and only in the mode measured. B8 as it
   stands supports no such sentence and `prior-art.md`'s row keeps pointing at it as a claim.
4. **`hft` against a spinning peer is the comparison that would earn the word back**, and it
   is not scheduled: it needs a second engine that busy-polls one session per core, which
   none of the surveyed Rust engines does. Until one exists, `hft` figures are published
   against the engine's own `standard` and against the kernel-TCP floor, not against a peer.

## Consequences

**Good**

- The headline says something the repository can prove on any given day, per non-negotiable
  10, instead of something one B8 table already contradicts for `standard`.
- PRD §6 row 1 closes; `DESIGN.md` §1 and `README.md` stop being blocked on it.

**Bad — and accepted**

- **A weaker headline.** "Fastest" is what a reader scanning GitHub reacts to; "published,
  reproduced number" is what a reader evaluating a deployment wants. This project is built
  for the second reader.
- **The B8 result is not flattering and this ADR points at it.** fixbolt's `standard`
  application path is ~1.1–1.7 µs slower than nanofix's at p50 on loopback; that is now a
  sentence in the positioning's own history rather than a table nobody links to. Item 49
  (the unattributed application-path cost) is the work that answers it.
- **Retiring a word from four documents and a rules file is a docs-only change that touches
  `CLAUDE.md`**, which the owner edits; the plan lists the lines.

## Sources

- `docs/reference/measured-costs.md` *B8 — against `matthart1983/nanofix`* `[measured
  2026-09-15]`; `docs/reference/prior-art.md`.
- Search 2026-09-18, positioning lines of `matthart1983/nanofix`, `artiofix/artio`, QuickFIX
  (README first paragraphs): property-based, no superlative.
