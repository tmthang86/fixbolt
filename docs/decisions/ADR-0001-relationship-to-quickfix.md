# ADR-0001 — Relationship to QuickFIX C++: take the data, not the code

- **Status**: Accepted — 2026-08-27
- **Date**: 2026-08-27
- **Deciders**: Tran Manh Thang

## Context

nanofixengine is a FIX 4.4 engine in Rust targeting HFT-grade latency. QuickFIX C++ is the
reference open-source FIX engine: 20+ years old, deployed widely, with complete spec
coverage. The question put to this ADR is whether nanofixengine should be a **port** of it.

Three things were established before deciding.

**1. The licence permits a port.** The QuickFIX Software License is BSD-3-clause in
shape, with two conditions that survive into any derivative work:

- Redistribution — source or binary — must retain the copyright notice, the condition
  list and the disclaimer.
- End-user documentation must carry the acknowledgment: *"This product includes software
  developed by quickfixengine.org (http://www.quickfixengine.org/)"*.
- The names **"QuickFIX"** and **"quickfixengine.org"** may not be used to endorse or
  name a derived product without prior written permission from `ask@quickfixengine.org`.

So a port is legal. Legality was never the constraint.

**2. QuickFIX's architecture is the thing that caps its throughput.** Published QuickFIX
figures sit around 6,000–8,000 msg/s per session on commodity hardware. That ceiling is
not an implementation accident; it follows from the design: heap-allocated field objects,
an ordered map per message, virtual dispatch on the application callback path, and a
`FileStore` that calls `Sync()` on every write across three files (seqnum, header, body).
A faithful port reproduces every one of those decisions in a language that makes them
*harder* to express, not easier. The borrow checker fights a 2000s-era C++ object graph.

**3. The genuinely valuable parts of the QuickFIX repository are data, not code.**

| Asset | Path in `quickfix/quickfix` | Why it is worth taking |
|---|---|---|
| FIX data dictionaries | `spec/FIX44.xml`, `spec/FIXT11.xml`, … | Machine-readable field, message and component definitions. Weeks of transcription avoided, and the source of the code generator's input |
| **Acceptance test suite** | `test/definitions/server/fix44/` — **59 `.def` scripts** | The FIX Protocol organisation's session-layer conformance tests, as executable scripts. This is the single most valuable asset in the repository |
| Session semantics | `src/C++/Session.cpp` | Reference *behaviour* for the awkward cases: sequence number too low/too high, `ResendRequest` and gap fill, logon negotiation, `PossDupFlag` handling |

The 59 acceptance definitions turn "is our session layer correct?" from a judgement call
into a gate that either passes or fails.

## Decision

**Do not port the QuickFIX C++ source. Clean-room the engine; adopt three QuickFIX assets
as data and as a test oracle.**

1. **Vendor `spec/*.xml`** as the input to nanofixengine's code generator. Fetched by script into
   `vendor/`, which is gitignored; not redistributed inside this repository.
2. **Vendor `test/definitions/server/fix44/`** and build a runner for the `.def` format.
   A session-layer change is not done until these pass.
3. **Read `Session.cpp` as a specification of behaviour**, and write Rust that satisfies the
   same acceptance tests. No line-by-line transliteration.
4. **Codec design follows `hffix`** (FreeBSD licence), not QuickFIX: parse and serialise in
   place at the I/O buffer, expose fields as byte ranges into that buffer, no heap
   allocation on the hot path. `hffix` deliberately omits the session layer; nanofixengine
   supplies it.
5. If any QuickFIX-derived text or data ever ships inside a nanofixengine artifact, the
   attribution clause above is honoured in `NOTICE`, and the name "QuickFIX" is never used
   in nanofixengine's own name or marketing.

## Consequences

**Good**

- Latency is bounded by nanofixengine's own design, not inherited from a port.
- The acceptance suite exists on day one, so the session layer — the part most likely to
  produce subtle, expensive bugs — has a real gate rather than a reviewer's opinion.
- No attribution or naming obligations attach to nanofixengine's own source, because none of it
  is derived from QuickFIX source.
- The data dictionaries remove the largest source of transcription error.

**Bad — and these are real**

- **All session-layer behaviour must be written from nothing.** The 59 acceptance tests say
  whether it is right; they do not write it. Estimated 3–6 weeks for a complete FIX 4.4
  session layer that passes them.
- **Spec coverage will lag QuickFIX for a long time.** QuickFIX handles FIX 4.0–5.0 SP2 and
  every edge case accumulated over two decades. nanofixengine will handle FIX 4.4 and whatever
  the tests force it to handle. Anything outside that is unknown territory.
- **The `.def` runner is itself work**, and it is a prerequisite, not a nice-to-have.
  The format has since been read and written up in
  [reference/quickfix-acceptance-def-format.md](../reference/quickfix-acceptance-def-format.md):
  7 directives, one placeholder, 1,319 lines across all 59 files. **Revised estimate: 1–2
  days.** That page also records the trap it carries — the comparator comes with a hidden
  constraint on serialiser field ordering.
- **No production track record.** QuickFIX's real value is that thousands of counterparties
  have already found its bugs. nanofixengine starts with that value at zero, and no amount of
  test coverage substitutes for it.
- Reading `Session.cpp` while writing Rust carries a genuine risk of accidental
  transliteration, which would drag the attribution obligation back in. Mitigation: write
  the Rust against the *acceptance tests* first, consult `Session.cpp` only when a test
  fails and the intended behaviour is unclear.

## Alternatives considered

| Alternative | Why rejected |
|---|---|
| Port QuickFIX C++ faithfully to Rust | Imports the architecture responsible for the 6–8k msg/s ceiling. Defeats the entire purpose |
| Wrap QuickFIX C++ via FFI (`quickfix` crate) | Proven session layer, but keeps the C++ toolchain, the `Sync()`-per-write store and the throughput ceiling. Correct choice for an *application* that needs FIX; wrong for a project whose purpose *is* the engine |
| Build on `ferrumfix` / `fixer-rs` | `ferrumfix`: *"wildly unstable, refrain from using it in production prior to its 1.0 release"*. `fixer-rs`: 10 stars, *"under heavy development"*. Neither is a foundation |
| Fork `matthart1983/nanofix` (MIT, 18★) | Closest prior art and worth reading. But its claims are lab-measured with no production evidence, and adopting an unproven codebase wholesale replaces one unknown with another. See `docs/reference/prior-art.md` |

## Open questions

1. Does `test/definitions/client/` also need to run, or do the 59 server definitions cover
   the acceptor side completely? — answerable by reading the client files.
2. Is the FIX 4.4 dictionary in `spec/FIX44.xml` sufficient for custom-tag extension, or
   does the generator need its own schema? — answerable by reading the XSD.
3. The comparator pins field ordering (see the reference page). Does QuickFIX's ordering
   follow `spec/FIX44.xml` declaration order in every case, or are there exceptions? — this
   decides whether the serialiser can be generated from the XML alone.
