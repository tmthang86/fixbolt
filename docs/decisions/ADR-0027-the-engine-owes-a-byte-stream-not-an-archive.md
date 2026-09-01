# ADR-0027 — The engine owes a faithful byte stream at a boundary, and never an archive

> **Status:** **Accepted — 2026-09-01.** Answers `PRD.md` open decision 9 and settles the scope
> of `STATUS.md` open item 30's parts (d) and (e).
>
> **Decided by the owner, explicitly, on 2026-09-01**, together with ADR-0026 and ADR-0028.

- **Date**: 2026-09-01
- **Deciders**: Tran Manh Thang
- **Related**: `DESIGN.md` D7, D8, D10 and D10b,
  [ADR-0008](ADR-0008-journal-is-a-trait.md),
  [ADR-0011](ADR-0011-a-full-ring-disconnects.md),
  [ADR-0017](ADR-0017-the-inbound-count-is-persisted-after-delivery.md),
  `PRD.md` §5 and §6, `STATUS.md` open item 30

## Context

A FIX acceptor in production is usually required to answer *"what exactly did we send that
counterparty, and when"* — years later. The question is whether this engine's journal is that
answer.

**It is not, and the sizes say so.** `DESIGN.md` D7's journal exists to serve a `ResendRequest`:
`journal::Store` is `MemJournal<SLOTS, SLOT_LEN>` with **`SLOTS = 8`** and `SLOT_LEN = 512`
`[verified 2026-09-01]`. It is a small bounded ring holding what a resend needs. An archive is
unbounded, long-lived, and has integrity requirements the journal has never claimed.

### What the obligation actually is — `[documented 2026-09-01]`, from the regulations and vendors, not measured here

- **MiFID II** requires records of electronic communications relating to the reception,
  transmission and execution of client orders to be kept **at least five years, up to seven if a
  regulator asks**.
- Records must sit in a **tamper-evident archive** that cannot be altered or deleted, with
  timestamping that proves the data has not changed, and must be **searchable and audit-ready**.
- **Drop copy** — the industry's real-time audit mechanism — is *a separate FIX session* carrying
  a copy of activity to a compliance destination. It is a deployment topology, not an engine
  internal.

**Every one of those properties is a storage-system property.** Retention windows, immutability,
tamper evidence, search, and a legal retention clock are what an archive is for, and a
latency-first engine that grew them would be a worse engine and a worse archive.

## Decision

**1. The engine owes exactly one thing: a faithful, ordered, timestamped copy of both directions,
delivered at a boundary, off the hot path.** Bytes as they went on the wire — not a
re-serialisation, not a summary — each with the direction, the connection, the identity and the
engine's own timestamp, in the order the engine processed them.

**2. It is delivered through the existing ring, never written by the engine thread.** D8 forbids
the engine thread from doing I/O and D10b already disconnects a connection whose consumer cannot
keep up. An audit consumer that falls behind is subject to **the same policy as any other slow
consumer** — [ADR-0011](ADR-0011-a-full-ring-disconnects.md) — and that is deliberate: an
acceptor that silently drops audit records because a disk was slow is worse than one that stops.

**3. The engine owns no retention, no rotation, no integrity, no search, and no clock for any of
them.** Five years, immutability, tamper evidence and searchability belong to whatever consumes
the stream. **This is a permanent non-goal**, not a phase-3 item, and `PRD.md` §5 gains it.

**4. The audit tap is a different feature from the journal, and they do not share a store.** The
journal answers a `ResendRequest` under D7 and is bounded at eight slots. The tap answers a
regulator. Merging them would either make the journal unbounded — putting archive growth behind a
`ResendRequest` on the message path — or bound the archive, which makes it not an archive.
**Conflating them is how an audit requirement lands on the hot path**, and naming that is most of
what this ADR is for.

**5. Drop copy is not an engine feature.** It is a second FIX session to a compliance
destination, which is the application's arrangement — and once ADR-0026's registry exists, an
ordinary entry in it.

**6. What ships is the boundary, and one reference consumer that is not on the hot path.** The
offline journal reader `STATUS.md` open item 30(e) asks for stays in scope, because *"we never
received order X"* must be answerable from what this engine wrote; it is a separate binary
reading the mmap, and it is not an archive either.

## Consequences

**Good**

- **The hot path is protected by a decision rather than by vigilance.** The next person asked for
  "audit logging" has a document saying where it goes.
- **`GUIDE.md` gains something a user cannot infer**: the engine is not their compliance system,
  and their retention obligation is not met by pointing at the journal. That is exactly the class
  of constraint §4 says `GUIDE.md` exists for.
- **The journal keeps its size and its purpose.** Eight slots is right for a resend and absurd
  for an archive; this ADR is why it does not have to grow.
- **A slow audit consumer behaves like every other slow consumer**, under a policy that already
  exists and is already tested, instead of getting a special case.

**Bad, and these are the price**

- **The user has to build or buy the other half**, and a reader comparing feature lists with
  QuickFIX — which ships file and SQL stores — will count this as a gap. It is one; it is a
  deliberate one, and §5 will say so.
- **"Faithful" is a claim that needs a gate.** A tap that silently reorders, or that drops under
  load without saying so, is worse than none — and nothing tests it yet.
- **The engine's timestamp is not an execution timestamp.** It is when the engine processed the
  bytes. Anybody reconciling against a venue clock needs to know that, and the difference is
  unmeasured.
- **Two stores means two things to keep consistent** when a message is both resendable and
  auditable, and nothing enforces that they agree.

## Open questions

1. **Does the tap carry rejected and malformed input?** A message the framer threw away is
   exactly what a dispute is about, but it is also what an attacker controls — an unbounded
   channel for garbage. Probably yes, with the pre-session refusals counted rather than copied.
2. **What is "faithful" under backpressure?** ADR-0011 disconnects rather than drops, which is
   consistent; whether an audit consumer should be able to opt out of that and lose records is a
   deployment question with a compliance answer, and this ADR does not make it.
3. **Does the tap need its own thread and core?** It is one more consumer under
   [ADR-0015](ADR-0015-explicit-cores-pinned-from-inside-and-read-back.md)'s rules, and
   `ShardPlan` already validates non-engine threads' cores.
