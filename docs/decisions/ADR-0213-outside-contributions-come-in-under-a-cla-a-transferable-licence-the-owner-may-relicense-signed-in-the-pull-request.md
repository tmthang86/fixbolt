# ADR-0213 — Outside contributions come in under a CLA: a transferable licence the owner may relicense, signed in the pull request

- **Status**: **Accepted — 2026-09-28 (owner's decision)** for decision 1, *a CLA, not a DCO*.
  Decisions 2–7 are the architect's, made to carry decision 1 out, and are accepted with it; the
  CLA texts they produce are **drafts until a lawyer has reviewed them** (decision 7), and no
  outside contribution is merged before that.
- **Date**: 2026-09-28
- **Deciders**: Tran Manh Thang (owner), decision 1. Written by the architect (Opus).
- **Related**: [ADR-0206](ADR-0206-the-documentation-is-an-mdbook-over-docs-in-place-split-by-diataxis-and-no-cited-file-moves.md)
  decision 4 (outside contributions wait for this ADR); plan
  [docs-for-embedders](../plans/2026-09-26-docs-for-embedders.md), owner's answer to Q1;
  [prior-art-for-embedders](../reference/prior-art-for-embedders.md) §3 (licence models; CLA
  versus DCO); `STATUS.md` open items 123 (closed by this ADR) and 124 (what the owner still does).
- **Out of scope**: **which licence fixbolt is under in future.** Staying MIT OR Apache-2.0, a
  copyleft-plus-commercial dual licence, a source-available licence — none is decided here, and
  every decision below is chosen so that none is foreclosed.

## Context

fixbolt is MIT OR Apache-2.0, and `v0.1.0` is tagged under those terms; that release stays
available under them to anyone who received it, whatever happens later — relicensing permissively
released code is not retroactive, and changing the licence of contributed code needs each
copyright holder's consent ([prior-art-for-embedders](../reference/prior-art-for-embedders.md)
§3). Today every line is the owner's, so the owner alone can still choose any licence for future
versions. The first outside contribution merged without an agreement ends that: under a DCO, or
under no agreement at all, a contribution arrives on inbound = outbound terms — the project's
licence of the day — and a later relicence needs that contributor again.

The owner decided on 2026-09-28 that outside contributions will be accepted under a **CLA**, not
a DCO, so the owner keeps the ability to relicense and to sell a licence on other terms later.
That leaves four questions this ADR answers: what the contributor grants (a licence or the
copyright itself), on which template, with a separate form for an employer or not, and how a
signature is given and recorded.

**What was found** (searched 2026-09-28):

- **Dual-licensing projects use a licence grant, not an assignment.** The Qt Contribution
  Agreement gives The Qt Company a copyright and patent licence to distribute contributions "under
  license terms of The Qt Company's choosing", and the contributor keeps ownership
  ([qt.io](https://www.qt.io/community/legal-contribution-agreement-qt),
  [v1.2 PDF](https://www.qt.io/hubfs/_website/PDF/Corporate_Qt-ContributionLicenseAgreement_v1_2_FINAL.pdf)).
  Grafana Labs moved Grafana, Loki and Tempo from Apache-2.0 to AGPLv3 in 2021 and adopted a CLA
  based on Apache's for the purpose, granting "a perpetual, worldwide, non-exclusive … copyright
  license to reproduce, prepare derivative works of, … sublicense, and distribute"
  ([Grafana blog](https://grafana.com/blog/grafana-loki-tempo-relicensing-to-agplv3/),
  [Grafana CLA](https://grafana.com/docs/grafana-cloud/developer-resources/contribute/cla/)).
  MongoDB's 2018 move to the SSPL rested on its contributor agreement
  ([Wikipedia, CLA](https://en.wikipedia.org/wiki/Contributor_license_agreement),
  [MongoDB SSPL FAQ](https://www.mongodb.com/legal/licensing/server-side-public-license/faq)).
  Canonical, which started Project Harmony, moved in 2011 from copyright assignment to a Harmony
  licence agreement, the contributor keeping the copyright
  ([Wikipedia, Project Harmony](https://en.wikipedia.org/wiki/Project_Harmony_(licensing)),
  [canonical.com/legal/contributors](https://canonical.com/legal/contributors)). For Sentry
  (FSL, [Sentry blog](https://blog.sentry.io/relicensing-sentry/)) **the search did not find the
  terms contributions to `getsentry/sentry` arrive under**; it is not relied on here.
- **An assignment has formalities a licence does not.** In the United States a transfer of
  copyright ownership "is not valid unless an instrument of conveyance … is in writing and signed
  by the owner" ([17 U.S.C. § 204](https://www.law.cornell.edu/uscode/text/17/204)), while a
  non-exclusive licence is excluded from "transfer of copyright ownership" (§ 101) and needs no
  signed writing ([DMLP](https://www.dmlp.org/legal-guide/creating-written-contract-transfer-or-license-rights-under-copyright)).
  In Germany an author's copyright cannot be transferred at all, only rights of use granted
  ([UrhG, English](https://www.gesetze-im-internet.de/englisch_urhg/englisch_urhg.html);
  [Wikipedia](https://en.wikipedia.org/wiki/Copyright_law_of_Germany)).
- **Project Harmony publishes licence and assignment templates, individual and entity, with five
  outbound options.** Option Five: "We may license the Contribution under any license, including
  copyleft, permissive, commercial, or proprietary licenses. As a condition on the exercise of this
  right, We agree to also license the Contribution under the terms of the license or licenses which
  We are using for the Material on the Submission Date." The licence is "transferable", and §6.3
  binds any assignee to the agreement. The templates are under CC BY 3.0
  ([harmonyagreements.org](https://www.harmonyagreements.org/),
  [guide](https://www.harmonyagreements.org/guide),
  [HA-CLA-I](https://www.harmonyagreements.org/docs/ha-cla-i-v1.pdf),
  [HA-CLA-E](https://www.harmonyagreements.org/docs/ha-cla-e-v1.pdf)). The criticism of record is
  that Option Five lets a company take contributions proprietary
  ([Kuhn, 2011](https://ebb.org/bkuhn/blog/2011/07/07/harmony-harmful.html)).
- **Apache's ICLA and CCLA** grant a sublicensable copyright licence and a patent licence, and the
  CCLA names the employees authorised to contribute in a *Schedule A*
  ([ICLA](https://www.apache.org/licenses/icla.pdf),
  [CCLA](https://www.apache.org/licenses/cla-corporate.pdf)). The ICLA is written for a
  non-profit foundation: its preamble promises that the Foundation "shall not use Your Contributions
  in a way that is contrary to the public benefit or inconsistent with its nonprofit status". **The
  search found no licence stated on the ICLA text itself.**
- **Signing tools.** `contributor-assistant/github-action` (CLA Assistant Lite) records each
  signature — a pull-request comment reading `I have read the CLA Document and I hereby sign the
  CLA` — in a JSON file in a branch of the same or another repository; it was **archived on
  2026-03-23** and runs on Node 20 (`gh api repos/contributor-assistant/github-action`: `archived:
  true`; [README](https://github.com/contributor-assistant/github-action/blob/master/README.md);
  risks listed in [webloomlabs/uptime-cairn#30](https://github.com/webloomlabs/uptime-cairn/issues/30)).
  StepSecurity maintains a drop-in fork, `step-security/contributor-assistant-github-action`, on
  Node 24, last release 2026-09-22, free for public repositories and behind a subscription check
  for private ones ([repository](https://github.com/step-security/contributor-assistant-github-action),
  [StepSecurity docs](https://docs.stepsecurity.io/github-actions/actions/stepsecurity-maintained-actions.md)).
  Its README says the signatures branch "should not be protected". The hosted CLA Assistant
  (`cla-assistant.io`, SAP) is up, reads the CLA from a GitHub Gist, stores signatures in its own
  database on Azure, and its code was last committed in October 2023
  ([cla-assistant/cla-assistant](https://github.com/cla-assistant/cla-assistant)).
- **The cost of any CLA** is that some contributors and employers refuse to sign one
  ([opensource.com](https://opensource.com/article/19/2/cla-problems)); already recorded in
  [prior-art-for-embedders](../reference/prior-art-for-embedders.md) §3.

## Decision

1. **A CLA, not a DCO** — the owner's decision, 2026-09-28. Every outside contribution is covered
   by a signed CLA before it is merged. A DCO sign-off is not asked for in addition.

2. **The contributor grants a licence, not the copyright.** The grant is perpetual, worldwide,
   non-exclusive, royalty-free, irrevocable, **transferable**, and sublicensable through any number
   of tiers, for copyright and for the contributor's patent claims the contribution reads on. The
   outbound term is Harmony's **Option Five**: the Project Owner may license a contribution under
   any licence, copyleft, permissive, commercial or proprietary, on the one condition that it is
   also licensed under the licence fixbolt used on the day it was submitted. An assignment would
   give the owner sole standing over the whole tree, but it needs a signed writing in the United
   States, cannot be made at all under German law, and is the form Canonical and Qt moved away from
   or never used; a licence gives every right relicensing needs with none of those costs. Option
   Five's condition costs nothing today: a contribution merged into the public repository is
   published under MIT OR Apache-2.0 at that moment anyway.

3. **Template: Harmony HA-CLA-I and HA-CLA-E, version 1.0, Option Five**, adapted and attributed
   as CC BY 3.0 requires — not Apache's ICLA. Harmony's Option Five says in the text what the owner
   needs (any licence, including commercial and proprietary) where an adapted ICLA would say it
   only through "sublicense"; Harmony's licence is stated to be transferable and its §6.3 binds a
   successor; Harmony's templates carry an open licence that permits adaptation, where no licence
   was found on the ICLA's text; and the ICLA's non-profit promise would have to be cut out. The
   adaptation fills in the parties and the instructions, drops the *Media* clauses fixbolt does not
   need, and adds a plain-words summary marked as not part of the Agreement.

4. **Two forms.** [CLA.md](../../CLA.md), the individual agreement, is signed by **every** outside
   contributor. [CCLA.md](../../CCLA.md), the entity agreement, is signed **in addition** by an
   employer or other entity that owns or may own rights in its people's work (the individual
   form's §3(c) requires it), once, by someone authorised to bind the entity, with a Schedule A of
   the GitHub accounts that contribute on its behalf — Harmony's entity form plus Apache's CCLA
   Schedule A. An entity signs out of band: a GitHub issue opens it, the signed copy is exchanged
   privately and never committed, and the entity's name, date and accounts are recorded in the
   signatures branch.

5. **The counterparty is "the Project Owner", Tran Manh Thang, and "We" includes any successor
   under §6.3.** No company exists today. If the owner later forms one to hold fixbolt, a written
   assignment of the agreements to it carries every licence received (they are transferable), and
   §6.3 binds the company to the same terms, including Option Five's condition.

6. **Signing tool: CLA Assistant Lite, in StepSecurity's maintained fork, pinned by commit SHA,
   with signatures on a dedicated branch.** A workflow `.github/workflows/cla.yml` on
   `pull_request_target` and `issue_comment` runs
   `step-security/contributor-assistant-github-action` pinned to a full commit SHA, with
   `path-to-document` pointing at `CLA.md` on `main` (one authored copy — no Gist),
   `path-to-signatures: signatures/v1/cla.json`, `branch: cla-signatures` (an unprotected branch
   holding nothing else, so `main` stays protected and a signature commit starts no CI run), and
   the owner and bots in `allowlist`. The workflow never checks out pull-request code. Its status
   check is made **required** on `main`. A new version of `CLA.md` changes the path to
   `signatures/v2/`, so everyone signs again. The hosted CLA Assistant was not chosen: it would keep
   the record of who signed on a third party's database, read the text from a Gist (a second copy),
   and its code has not moved since 2023. The archived original was not chosen: no security fixes,
   and a Node runtime GitHub is retiring.

7. **Two things gate the first outside merge, and both are the owner's**:
   (a) **a lawyer's review** of `CLA.md` and `CCLA.md` — including enforceability of signing by
   pull-request comment where the owner and likely contributors live, and the governing law, which
   is left as a marked placeholder in §6.1 of both texts; the *Draft* notice at the top of each is
   removed only when the review is done, in the same commit as any change it asks for;
   (b) **the signing tool installed and proven by reversal**: a pull request from an account not
   in the allowlist shows the check red and cannot merge, turns green after the signing comment,
   and the signature appears in `signatures/v1/cla.json` on `cla-signatures`. The steps are
   `STATUS.md` open item 124.

## Consequences

**Good**

- Every licence model stays open for future versions — staying MIT OR Apache-2.0, a copyleft
  licence with a commercial one beside it, a source-available licence — for contributed code as
  well as the owner's own, without asking any contributor again.
- A contributor keeps their copyright and may reuse their own work however they like; the
  Option Five condition guarantees their contribution stays available under the licence of the day
  they submitted it. That is a smaller ask than an assignment, and the one Qt, Grafana and
  Canonical make.
- The agreement survives the owner forming a company: the licence is transferable and the terms
  bind the successor.
- A patent licence comes with every contribution, which neither a DCO nor MIT gives.
- The record of who signed lives in this repository, in a branch, as a file the owner controls
  and can read with `git`; the text is signed by the same file readers see.

**Bad — and accepted**

- **Fewer contributors.** Some people and employers will not sign a CLA, and Option Five in
  particular is the clause critics single out: it lets the project take a contribution
  proprietary. An embedder whose employer forbids CLAs cannot contribute at all.
- **The owner is not the sole copyright holder.** With a non-exclusive licence, the owner cannot
  by that licence alone sue someone who infringes a contributor's part; an assignment would have
  given that standing. Accepted: relicensing, not enforcement, is what the owner asked to keep.
- **Harmony 1.0 dates from 2011 and has not been revised.** A lawyer may want changes; each is
  made in the text and recorded in its attribution note.
- **A signature by pull-request comment is a click-through.** Whether it binds as intended in every
  contributor's jurisdiction is exactly what decision 7(a) asks a lawyer; an entity signs a real
  document for that reason.
- **A third party's code runs with write permission on the repository** at every pull request:
  StepSecurity's fork, on `pull_request_target`. Pinning by SHA means a change reaches the
  repository only when the owner moves the pin; the workflow never checks out pull-request code.
  If StepSecurity stops maintaining it, the pin keeps working until the runtime is retired, and
  replacing it is a change of workflow, not of agreement — the signatures file stays.
- **The owner carries paperwork**: entity agreements by hand, allowlist upkeep, a re-signing round
  whenever the text changes.
- **Nothing here reaches back.** `v0.1.0` and every commit already public stay MIT OR Apache-2.0
  for whoever has them; the CLA governs only contributions made under it.
