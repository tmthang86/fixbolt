# ADR-0082 — The session is generic over tag=value encodings, and the boundary to SBE is the session, not the trait

- **Status**: Accepted — 2026-09-19
- **Date**: 2026-09-19
- **Deciders**: Tran Manh Thang, who delegated this decision
  ("uỷ quyền toàn bộ cho bạn", 2026-09-19) to the PR B manager session rather than reading the
  ADR himself. Recorded because a reader asking *who weighed this* deserves the honest answer:
  PR A's architect proposed it, the PR B manager verified its two load-bearing code claims —
  the five equality bounds at `crates/session/src/lib.rs:1487-1493` and the seven skeletons
  `out.rs` builds (`0 1 2 3 4 5 A`) — and accepted it on that basis.
- **Amends** [ADR-0079](ADR-0079-one-view-per-encoding-and-one-trait-over-them.md) decision 3
  (its text is not changed) and narrows the "behind the same trait surface" of its decision 4.
  Consistent with [ADR-0078](ADR-0078-sbe-enters-as-an-encoding-without-a-session-and-fixp-is-its-own-phase.md)
  decisions 1–3 and [ADR-0080](ADR-0080-the-dictionary-rides-the-encoding-and-a-fixt-session-is-one-table-built-from-two-xml-files.md)
  decision 1.
- **Related**: `DESIGN.md` D1, D3, D9, D16 (written by row A4 of the plan
  [phase-2-fixt-and-sbe](../plans/2026-09-19-phase-2-fixt-and-sbe.md)); `crates/codec/src/encoding.rs`;
  `crates/session/src/lib.rs` (the `impl Session` bounds) and `crates/session/src/out.rs`
  (`Outbound::new`).
- **Raised by**: step A2 of the plan, as a design finding — the developer could not write
  `Session<E> where E::Dict: Tables` alone and reported why.

## Context

ADR-0079 decision 3 says the session layer *"needs from a view only the session fields … the
trait exposes those as methods on `View`, so `Session` reads them through the trait and never
touches encoding-specific bytes"*, and decision 4 says validation is *"per encoding, behind the
same trait surface"*. Step A2 found that the session needs three things the four shared
operations do not give, and none of them is an oversight in the trait:

1. **It walks the message in wire order.** The dictionary pass (item 39) answers
   `14e_TagSpecifiedOutOfRequiredOrder`, `14h_RepeatedTag`, `14b_RequiredFieldMissing` and the
   group-count definitions by iterating `MessageView::field_at` / `len` / `find_from`
   (`crates/session/src/lib.rs:3775-3863`). A repeated tag or a tag out of order is a
   *tag=value* defect: an SBE message cannot have either, because its layout is the schema's.
2. **It lays out seven FIX session messages** — Logon, Logout, Reject, Heartbeat, TestRequest,
   ResendRequest, SequenceReset — with `codec::TemplateBuilder` (`crates/session/src/out.rs`,
   `Outbound::new`). These messages *are* the FIX session layer; an encoding that carries no
   FIX session header (SBE, `session_fields → None`) has nothing to put in them.
3. **It matches `ParseError::BadTag`** to answer `14a_BadField` differently from a garbled
   message (`2d_GarbledMessage`).

So A2 wrote what the session actually needs, as bounds on the `impl`, not on the trait:

```rust
E: for<'a> Encoding<
        View<'a> = MessageView<'a, N>,
        Scratch = FieldIndex<N>,
        Template<24, 320> = Template<24, 320>,
        Field = u32,
        ParseError = ParseError,
    >,
E::Dict: Tables,
```

**The specification agrees with the code, not with ADR-0079's sentence.** The FIX Session
Layer specification defines its own subject in terms of one encoding: §3.1.4 *valid FIX
message* — *"A session or application message that is a tagvalue encoded string of octets that
is properly formed according to the FIX tagvalue encoding specification"* — and §4.5 —
*"The initiator and acceptor shall validate incoming FIX messages to ensure proper tagvalue
encoding. The integrity of message data content shall be verified for message length and the
tagvalue encoding checksum according to the FIX tagvalue encoding specification."* A FIXT
session is a tag=value session by definition. The only standard way for SBE to ride one is as
an application payload under `ApplVerID`, and ADR-0078 decision 3 declined to build that for
lack of any venue. The session that is encoding-independent is **FIXP** — its introduction lists
*"Encoding independent supporting binary protocols"* as a design goal — and ADR-0078 decision 2
already makes FIXP *a second `impl` beside `Session`*, not `Session<E>` with a different `E`.

**Prior art draws the same line.** Artio keeps the FIX tag=value `Session` (`FixLibrary.sessions()`)
and the FIXP connections (`FixLibrary.fixPConnections()`, iLink 3 and B3 Binary Entrypoint) as
separate types with separately generated codecs; its `iLink3Sessions()` was deprecated *into*
`fixPConnections()`, not merged into `Session`. QuickFIX, QuickFIX/J and quickfix-go run their
session over a tag=value `Message`/`FieldMap` with no encoding abstraction at all, and ship no
SBE (ADR-0078 sources). Real Logic's SBE is a codec generator and disclaims the session. The
search of 2026-09-19 for an engine whose *one* session state machine is generic over both
tag=value and SBE found nothing; FerrumFIX lists a session layer (5) beside its tagvalue/JSON/
FAST codecs (6) but its session documentation shows no encoding parameter — inconclusive, not a
counter-example.

## Decision

1. **`Session<E>` is generic over tag=value encodings, and says so.** Its contract is the six
   bounds above: `E: Encoding` with five associated-type equalities plus `E::Dict: Tables`.
   What varies across `E` is the dictionary and the index capacity `N` — exactly what FIXT 1.1 /
   FIX 5.0 SP2 (PR B) needs, and all it needs. `DESIGN.md` D16 states the five bindings and what
   each one means, in the words of the rustdoc on the `impl` (`lib.rs` *What the session needs of
   an `Encoding`, beyond the trait*), so that "generic" is not read as "encoding-agnostic".
2. **ADR-0079 decision 3 is amended.** The sentence *"needs from a view only the session fields …
   never touches encoding-specific bytes"* is retired: the session reads the view in wire order
   and lays out tag=value skeletons, because the FIX session layer is tag=value by
   specification. Everything else in decision 3 stands — the session is generic over `E`, stays
   pure (D1), and the 59 definitions against `Session<Fix44TagValue>` prove the generalisation
   moved nothing. Decision 4's *"behind the same trait surface"* is narrowed to what is true:
   tag=value validation lives in the session over `MessageView`; SBE's validation is structural
   and lives in `sbe`; no trait method carries either.
3. **`Encoding` does not grow.** It stays ADR-0079 decision 2's four operations plus
   `session_fields` (decision 3's one hook). No ordered walk, no builder, no error probe is added
   to it. The `Option` on `session_fields` remains the one question a *future* session may ask an
   encoding — "do you carry a FIX session header at all" — and is the whole provision this trait
   makes for phase 3.
4. **The boundary to SBE is the session's `where` clause, not the trait.** `Sbe<S>: Encoding`
   compiles; `Session<Sbe<S>>` does not, on five equalities before `Tables` is ever asked.
   Therefore **`SbeTables<S>` implements `codec::Dictionary` only** — `dict::Tables` is a
   *session* bound, nothing reaches it for SBE, and an impl of it would exist to satisfy a
   constraint that can never be reached. `sbe` does not depend on `dict`. Row C4 of the plan is
   amended accordingly, and the fact that `Session<Sbe<S>>` is rejected is recorded where
   `library` can see both crates (row C7), as a `compile_fail` doctest.

### The alternative, not chosen: widen the trait so the session has no equality bounds

Add to `Encoding` an ordered walk (`field_at`, `len`, `find_from`, a group cursor), a
`type Builder` with `field` / `slot` / `build`, and a probe on `ParseError` for "bad tag", so
that `Session<E>` could be written with `E: Encoding` and `E::Dict: Tables` alone. Rejected:

- **The added methods are tag=value shapes with tag=value names.** A repeated tag, a tag out of
  required order, a `35=A` skeleton with a `98=` slot — an SBE implementation would answer
  `None` or `Err` to each. The session would be generic in its signature and tag=value in every
  branch. That is nominal genericity, and a reader would trust it.
- **It is the trait harness for "SBE inside a FIXT session"**, the combination ADR-0078
  decision 3 declined because no venue runs it. The bounds are that decision's type-level form.
- **Every added GAT method is another inlining boundary on the parse hot path** that ADR-0079
  decision 5's band (A-desk, ADR-0031) must re-prove — a cost with no consumer.
- **The session that could want SBE would not want this trait.** FIXP's own messages
  (Negotiate, Establish, Sequence, RetransmitRequest) are SBE-encoded in every venue that runs it
  (CME iLink 3, B3). A phase-3 FIXP `impl` needs SBE-shaped skeletons and a walk over `SbeView`,
  not a tag=value builder reached through a trait. ADR-0078 decision 2 already puts it beside
  `Session`, where it can have both.

## Consequences

**Good**

- PR B is untouched: `TagValue<Fixt11Fix50Sp2, N>` satisfies all five equalities, and the
  session's genericity buys exactly the dictionary swap FIXT 1.1 needs.
- The contract is a **type error**, not a runtime surprise: `Session<Sbe<S>>` fails to compile
  with the equality that failed named in the message.
- No hot-path cost is added and none has to be re-measured beyond what ADR-0079 decision 5
  already requires of PR A.
- `sbe` stays zero-dependency (no `dict`), and C4 loses a dependency on A2.

**Bad — and accepted**

- **"`Session<E>` is generic" will be read as more than it is.** D16 must say *tag=value* in
  those words, and `GUIDE.md`'s alias section must not imply an SBE session exists.
- **The `where` clause is six lines that every `impl Session` block repeats**, and `N` is pinned
  by an equality rather than by a parameter of `Session` — a subtlety a reader meets in the
  rustdoc, not in the signature. D16 names it.
- **Phase 3 reuses nothing of `out.rs` or the validation pass for a FIXP session.** It shares
  `Role`, `Input`/`Output`, `Config` and the pure-machine discipline (D1), and brings its own
  skeletons and its own walk. That was already the shape ADR-0078 decision 2 chose; this ADR
  makes the cost explicit rather than adding to it.
- **If a `.def`-style oracle for SBE-in-FIXT ever appears**, this ADR is superseded, the trait
  grows the ordered walk and the builder then, and the A-desk band is re-run on that commit.
  Nothing in decision 3 or 4 makes that harder than it is today.

## Sources

- FIX Trading Community, *FIX Session Layer* (June 2020), §3.1.4 *valid FIX message* and §4.5
  *Message exchange during a FIX connection* — the tagvalue sentences quoted above —
  <https://www.fixtrading.org/wp-content/uploads/download-manager-files/FIX_Session_Layer_June_2020.pdf>
  (read 2026-09-19, `pdftotext`).
- FIX Trading Community, *FIXP* v1.1 *Introduction* — *"Encoding independent supporting binary
  protocols"*; FIXP *"interoperates with the other FIX standards at the application and
  presentation layers, but it is not dependent on them"* —
  <https://github.com/FIXTradingCommunity/fixp-specification/blob/master/v1-1/doc/01Introduction.md>
  (read 2026-09-19).
- Artio, `FixLibrary` javadoc — `sessions()` against `fixPConnections()`, `iLink3Sessions()`
  deprecated in favour of `fixPConnections()` —
  <https://javadoc.io/static/uk.co.real-logic/artio-core/0.121/uk/co/real_logic/artio/library/FixLibrary.html>
  (read 2026-09-19); artio issue #400 and the QuickFIX family's absence of SBE — ADR-0078 sources.
- FerrumFIX layer table (session at layer 5, codecs at layer 6) — <https://docs.rs/fefix/latest/fefix/>
  and <https://docs.rs/fefix/latest/fefix/session/index.html> (read 2026-09-19; no encoding
  parameter visible on the session types — inconclusive).
- Search 2026-09-19 for one session state machine generic over both tag=value and SBE: nothing
  found.
- `crates/session/src/lib.rs` (`impl Session` bounds and their rustdoc; the ordered walk at
  3775–3863), `crates/session/src/out.rs` (`Outbound::new`), `crates/codec/src/encoding.rs` —
  the working tree of branch `plan/phase-2-a`, read 2026-09-19.
