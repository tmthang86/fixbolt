# ADR-0028 — A decimal is a `Copy` value parsed on demand, and the gap was real

> **Status:** **Accepted — 2026-09-01.** Answers `PRD.md` open decision 10.
>
> **Decided by the owner, explicitly, on 2026-09-01**, together with ADR-0026 and ADR-0027.
>
> **This ADR overturns the guess that raised the question.** Open decision 10 was written
> suspecting *decimal / price types* was **mislabelled** — that ADR-0003 had already answered it
> and a typed decimal would be the owned per-message object D2 forbids. Prior art says otherwise,
> and the record of being wrong is kept rather than tidied.

- **Date**: 2026-09-01
- **Deciders**: Tran Manh Thang
- **Related**: [ADR-0003](ADR-0003-message-representation.md), `DESIGN.md` D2 and D9,
  `PRD.md` §3 and §6, [prior-art.md](../reference/prior-art.md),
  [measured-costs.md](../reference/measured-costs.md)

## Context

`PRD.md` §3 has listed *decimal / price types* as a phase-1 gap — *"bytes and integers only"* —
since it was written, and `PRD.md` open decision 10 questioned whether it is a gap at all:
ADR-0003 hands the application a borrowed `MessageView` on purpose, and a decimal type sounded
like the owned, per-message object [D2](../DESIGN.md) exists to avoid.

### What the guess got wrong — `[documented 2026-09-01]`

**Artio has one.** `DecimalFloat` represents the significant digits in a **value** field and the
position of the point in a **scale** field. That is two integers — a small `Copy` POD, no heap, no
per-message construction. Artio is the closest architectural cousin this project has: low latency,
no allocation on the path, the same refusal to build objects per message.

So the two things the guess conflated come apart:

| | |
|---|---|
| What D2 and ADR-0003 forbid | **an owned, large, per-message structure** — the 8 224-byte `MessageView` that cost 5.9× and was split for it |
| What a decimal is | **16 bytes, `Copy`, produced only when the caller asks for a field** |

A `{ value: i64, scale: i32 }` is smaller than `MessageView` itself, which ADR-0003 already
blesses at 24 bytes and `Copy`. **Nothing about it is the thing D2 forbids.** The gap in `PRD.md`
§3 is real; the objection was to a design nobody proposed.

`[documented]` The rest of the field agrees on the shape and disagrees on the type: QuickFIX/Go
carries a `FIXDecimal` arbitrary-precision fixed-point value; QuickFIX/J uses `double` and has
been argued about for years for exactly the reason below.

## Decision

**1. `codec` gains a `Decimal { value: i64, scale: u8 }` — `Copy`, no allocation, no `std`.** Two
integers, the same representation Artio arrived at. It stays `no_std`-compatible, which
`DESIGN.md` §6 names as `codec`'s goal.

**2. It is parsed on demand, never eagerly.** `view.decimal(tag) -> Result<Decimal, _>` reads the
bytes the borrowed view already points at. **No field is decoded because a message arrived**;
nothing is stored per message; the field index keeps holding offsets and lengths. This is
ADR-0003 applied rather than contradicted.

**3. Binary floating point is refused, and this is the substantive half.** No `f64` in the public
API, on either path. A price that survives a round trip through `f64` is a price that can print
back differently from the bytes the counterparty sent, and this engine's serialise path is
byte-oriented and template-patched (D9) precisely so that what goes out is what was decided.
QuickFIX/J's `double` is the known counter-example and the long-running argument about it is the
evidence.

**4. Scale is preserved exactly as it appeared on the wire.** `1.50` and `1.5` are the same
number and **not the same bytes**, and a counterparty that sent trailing zeros gets them back.
`Decimal` therefore carries the scale it was parsed with, and serialisation reproduces it — the
field-order rule of D3 applied to a value instead of a position.

**5. Nothing is added to the session layer.** Decimals are application-message content; the
session machine reads `SeqNum` and `int` fields and has no business with prices. D1's purity is
untouched.

## Consequences

**Good**

- **A phase-1 gap closes with about a day of work in one crate**, and it is the gap most visible
  to somebody writing their first order handler against this engine.
- **It is provably free**: a `Copy` two-integer value parsed on demand cannot allocate, and
  `benches/alloc.rs` is where that is asserted rather than argued.
- **Exact-scale round-tripping is a correctness property**, not an aesthetic one, and it is the
  half a `double` cannot have at any price.
- **The PRD stops carrying a gap it suspected was fictional.** Either state was fine; carrying it
  *undecided* was not.

**Bad, and these are the price**

- **`i64` with a `u8` scale has a range, and it is not FIX's.** FIX `float` is a decimal string of
  arbitrary length; a counterparty may legitimately send more significant digits than `i64` holds.
  The parse must **refuse** rather than round, and refusing a value some other engine accepts is a
  real interoperability cost that this ADR takes deliberately.
- **`codec`'s public API grows**, which is a `CHANGELOG.md` entry and a `DESIGN.md` §3 row.
- **Arithmetic is not offered and users will want it.** Adding two `Decimal`s of different scales
  is a decision about rounding, and rounding money is the application's business, not a codec's.
- **One more thing that must be proven, not asserted**: the round trip is a property test —
  parse → serialise must be byte-identical across the corpus and a generated set — and it does not
  exist yet.

## Open questions

1. **What does a `Decimal` parse cost against the 122.6 ns parse baseline?** Unmeasured, and it
   goes into `benches/parse.rs` with its own case rather than being folded into an existing one.
2. **Does `dict` know which tags are `float`-typed?** It tabulates 23 field types already, so a
   typed accessor could refuse `view.decimal(35)` at runtime — or the type table could make it a
   compile-time error for generated constants. Not decided.
3. **`u8` scale or `i8`?** FIX prices with a negative scale (`1E3`-style) are not FIX 4.4 `float`
   syntax, but nothing has surveyed a real message population — the same gap `MAX_FIELDS = 64`
   already carries.
