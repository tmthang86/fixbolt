# ADR-0078 — SBE enters as an encoding without a session, and FIXP is its own phase

- **Status**: **Accepted — 2026-09-18**, by the plan [closing-the-open-items](../plans/2026-09-18-closing-the-open-items.md), approved by the manager under the owner's 2026-09-18 mandate. Proposed the same day.
- **Date**: 2026-09-18
- **Deciders**: Tran Manh Thang
- **Related**: `PRD.md` §2 *Phase 2*, *Phase 2 starts with an architectural decision*, §6
  row 3; [ADR-0003](ADR-0003-message-representation.md);
  [ADR-0079](ADR-0079-one-view-per-encoding-and-one-trait-over-them.md); `DESIGN.md` D1, D3
- **Answers**: PRD §6 open decision 3 — *does SBE ride a tag=value session, or does FIXP enter
  scope? Decides the size of phase 2 by roughly 5×*

## Context

SBE is an encoding: the FIX Trading Community's own introduction says it *"is complimentary
to other FIX standards for session protocol and application level behavior"* — it binds the
FIX type system to native binary layouts and says nothing about sessions. Where SBE is used
for order entry it rides **FIXP** or a venue's own binary session: CME iLink 3 is *"FIX Simple
Binary Encoding over the FIXP session layer"*, and FIXP has no `MsgSeqNum(34)`, no
`ResendRequest(35=2)` and no `SequenceReset(35=4)` — recovery is *Retransmit Request* /
*NotApplied*, negotiation is *Negotiate/Establish*, and the framing is SOFH. Market data in
SBE (B3 UMDF, MOEX SIMBA) is multicast with no session at all. The search of 2026-09-18 found
**no production venue that carries SBE inside a FIXT tag=value session** — the combination the
PRD row named as the cheap option exists in the standard (SBE can be a `FIXT` payload with
`ApplVerID`) and in no venue this project could interoperate with.

Prior art on the engine side: Artio supports FIX tag=value and CME iLink 3 as **separate
connection types** with separately generated codecs (`artio-ilink3-codecs`, SBE-generated), and
a 2020 request for a venue-independent FIXP+SOFH+SBE API (artio issue #400) was closed without
one. QuickFIX and QuickFIX/J ship no SBE, no FIXP.

So the honest shape of the question is: **phase 2 without FIXP is an encoding and a
dictionary; phase 2 with FIXP is a second session state machine with its own conformance
oracle (there is no `.def`-style corpus for FIXP; CME's certification harness is the oracle,
and it is not public).**

## Decision

1. **Phase 2 delivers SBE as an encoding, with no session of its own.** Concretely: an
   `Encoding` trait (ADR-0079), a generated SBE codec from an SBE XML schema (the same D3
   rule — layouts from generated tables, never from a call site), a `View` for SBE, and the
   dictionary/`ApplVerID` work that FIX 5.0 / FIXT 1.1 drag in. The SBE codec is gated by its
   own tests and by round-tripping the FIX Trading Community's reference schema examples.
2. **FIXP is phase 3, its own plan and its own ADR**, opened only when there is a target venue
   and an oracle for it. The session layer's purity (D1) already means a second session
   machine is a second `impl` beside `Session`, not a change to it; that is the whole
   architectural provision phase 2 makes for FIXP, and it is free.
3. **SBE inside a FIXT tag=value session is not built.** It has no venue; building it would be
   building a test harness for a combination nobody runs. If a `.def`-style oracle for it ever
   appears, that is a phase-3 line.
4. **Phase 2's size is therefore the small one** (PRD row 3's "5×" is the FIXP branch, which is
   deferred). `PRD.md` §2 *Phase 2* is edited to say: encoding trait, SBE codec, FIX 5.0 /
   FIXT 1.1 dictionaries, `docs/internals`; not FIXP, not FAST, not FIXML.

## Consequences

**Good**

- Phase 2 is briefable: an encoding trait and a generated codec are codec-crate work with a
  test oracle (the SBE reference schemas) and no new session semantics.
- The FIXP door is kept open at zero cost by a decision that already exists (D1).
- No venue-specific session code enters a repository that is meant to be generic.

**Bad — and accepted**

- **An SBE codec with no session cannot log on anywhere.** A user who wants iLink 3 gets
  encoders and decoders and has to bring the session. That is exactly what Artio's issue #400
  asked for and did not get; this project will be in the same place until phase 3.
- **"Phase 2 supports SBE" will be read as more than it is.** `PRD.md` §2 must say *encoding
  only* in those words.
- **FIXP's oracle problem does not go away by deferring it**; phase 3 will face a session
  layer with no public conformance corpus, and the only independent opinion is a venue's
  certification environment.

## Sources

- FIX Trading Community, SBE v1.0 RC3 *Introduction* — *"complimentary to other FIX
  standards for session protocol"* —
  <https://github.com/FIXTradingCommunity/fix-simple-binary-encoding/blob/master/v1-0-RC3/doc/01Introduction.md>.
- CME Group, *iLink Simple Binary Encoding* — SBE over FIXP —
  <https://cmegroupclientsite.atlassian.net/wiki/spaces/EPICSANDBOX/pages/714113056/iLink+Simple+Binary+Encoding>;
  FIXP's recovery model (no 34/35=2/35=4) as summarised at
  <https://www.submillisecond.com/glossary/protocols/binary-fix>.
- Artio, issue #400 *FIXP + SOFH + SBE API support* — <https://github.com/artiofix/artio/issues/400>;
  `quickfix-j/quickfixj-ilink3` (a converter between FIX and iLink3 built on Artio) —
  <https://github.com/quickfix-j/quickfixj-ilink3>. All read 2026-09-18.
- Search 2026-09-18 for a venue carrying SBE inside a FIXT tag=value session: nothing found.
