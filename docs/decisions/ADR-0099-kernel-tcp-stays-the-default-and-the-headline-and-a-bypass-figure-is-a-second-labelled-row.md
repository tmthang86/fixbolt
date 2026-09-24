# ADR-0099 — Kernel TCP stays the default and the headline; a bypass figure is a second, labelled row

- **Status**: **Accepted — 2026-09-23, by the owner**, with
  [ADR-0098](ADR-0098-phase-4-is-the-owners-five-items-each-entering-behind-a-measurement-that-can-kill-it.md)
  (its Q2: *yes*, and the positioning sentence is rewritten to match — `CLAUDE.md`,
  `README.md`, `DESIGN.md` §1, `PRD.md`, `INTRODUCTION.md` §5; Q3: bypass is Onload over AF_XDP
  only, no TCP stack of this project's own; Q5: no `ef_vi` card in phase 4). It **supersedes
  [ADR-0077](ADR-0077-acceptor-first-stays-and-fastest-is-said-only-beside-a-reproduced-pair.md)
  decision 2** and **[ADR-0074](ADR-0074-kernel-bypass-io-uring-and-the-logon-hop-stay-unmeasured-by-decision.md)
  decision 1**; their other decisions stand, and their text is not edited beyond a status line
  naming this ADR.
- **Bypass dropped**: the bypass item this ADR positions (Onload over AF_XDP) was dropped at
  probe gate G1 on 2026-09-24, before any stack came up — see
  [ADR-0201](ADR-0201-onload-on-the-i211-runs-without-hardware-flow-filters-one-channel-count-holds-for-the-boot-and-the-control-path-leaves-the-cable.md)
  *Result*. This ADR's positioning decision (kernel TCP is the headline; a bypass figure, when
  one exists, is a second labelled row) is unaffected — there is no bypass figure to place.
- **Date**: 2026-09-23
- **Deciders**: Tran Manh Thang (chose kernel bypass for phase 4, 2026-09-23). Written by the
  architect (Opus).
- **Related**: `PRD.md` §1, §5 *Kernel bypass*; `DESIGN.md` §1 *Positioning*, §8, §9, D5, D11;
  `README.md` line 3; `CLAUDE.md` first paragraph (quotes the positioning — the owner edits it);
  [ADR-0068](ADR-0068-a-published-figure-is-two-procedures-shown-side-by-side.md)

## Context

ADR-0077 decision 2 made the positioning *"a FIX 4.4 acceptor on kernel TCP whose latency is a
published, reproduced number"*. ADR-0074 decision 1 kept bypass out of `PRD.md` §5 and allowed
exactly one Onload run — a copy-mode smoke test — *"not a latency figure and not published as
one"*. The owner has put kernel bypass into phase 4 (ADR-0098 item 2). A bypass figure that
cannot be published is not worth measuring; a bypass figure published as the headline breaks the
one promise the headline makes — that anyone can reproduce it on an ordinary Linux box.

The search of 2026-09-23 (sources in ADR-0098): Onload runs on non-Solarflare NICs through
AF_XDP, community-supported; one public report shows it *slower* than the kernel on AWS; the
desk's `igb` driver now carries an AF_XDP zero-copy path (kernel 7.0.0-31, symbols only). No
published Onload-on-`igb` figure was found.

## Decision

1. **The headline keeps kernel TCP.** The positioning becomes: *a FIX acceptor whose latency is a
   published, reproduced number — on kernel TCP by default, and on a named kernel-bypass stack
   only beside a kernel figure from the same boot.* `README.md` line 3 keeps *on kernel TCP*;
   the bypass clause lives in `DESIGN.md` §1 and §8, not in the one-line headline.
2. **A bypass figure is a second row, never a replacement.** It appears only in a table that
   also shows the kernel-TCP figure taken on the same boot, same mode, same procedure pair
   (ADR-0068), and its row names the stack (Onload version, AF_XDP mode *zero-copy* or *copy*
   as read back from the kernel, NIC and driver). A bypass figure with no kernel twin is not
   published.
3. **ADR-0074 decision 1 is replaced**: an Onload run on the desk may be published as a figure
   when it meets ADR-0098 item 2's kill line and decision 2 above. The copy-mode-only smoke test
   is no longer the ceiling. **The order stands**: Onload first, `ef_vi` second (needs hardware
   this project does not own), DPDK never.
4. **Bypass is `hft` only** unless a `standard` run under the bypass stack passes
   `scripts/check-standard-gives-the-core-back.sh`; **plaintext only** (D11).
5. **ADR-0077 decisions 1, 3 and 4 stand**: acceptor-first; a superlative only beside a
   reproduced pair and only in the mode measured; `hft` against a spinning peer is the
   comparison that would earn "fastest" — a bypass row does not earn it.

## Consequences

**Good**

- Kernel bypass can produce a publishable number without the headline losing what makes it
  reproducible.
- A bypass result that loses to the kernel is still publishable, as the same two-row table — the
  negative result has a home.

**Bad — and accepted**

- **The positioning is longer and easier to misquote.** "fixbolt does X µs" will be quoted from
  the bypass row without its kernel twin; decision 2 forbids it inside this repository and
  cannot forbid it outside.
- **`README.md`, `DESIGN.md` §1, `PRD.md` §1 and `CLAUDE.md`'s first paragraph all quote the
  positioning**; `CLAUDE.md` is the owner's to edit, and until it is edited the rules file
  quotes a superseded sentence.
- **A same-boot kernel twin doubles every bypass measurement**, and a bypass boot needs its own
  §9 rows before its first number.

## Sources

ADR-0098 *Research*; ADR-0077; ADR-0074; `STATUS.md` item 14 (struck, text kept).
