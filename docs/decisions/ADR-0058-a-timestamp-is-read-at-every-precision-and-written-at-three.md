# ADR-0058: A timestamp is read at every precision the wire can carry, and written at three

- **Status:** Accepted — 2026-09-09, **approved by the owner on the day it was written**, after the survey it asked for. **Revised in place once, before approval — see *Revision 1*
  at the foot of this page.** §5 permits a `Proposed` ADR to be revised with the revision
  recorded in the text, and this one was: the owner asked for a survey of other engines before
  deciding, the survey moved decision 1's upper bound, and it found a width nothing here had
  counted
- **Date:** 2026-09-09
- **Plan:** [utc-timestamp-widths](../plans/2026-09-09-utc-timestamp-widths.md) — this page is
  that plan's gate; **no line of it is written before this is `Accepted`**
- **Constrained by** [ADR-0057](ADR-0057-sub-millisecond-time-arrives-beside-the-tick.md) (the
  send side writes 21, 24 or 27 bytes and the configured width is a ceiling),
  [D13](../DESIGN.md) (`Tick` counts **milliseconds** from year zero, so anything finer is
  dropped on the way in), and non-negotiable 2 (the session is pure — no allocation, no
  `format!`, fieldless errors)
- **Supersedes nothing.** ADR-0057 decided where sub-millisecond time *enters*; this decides
  what widths are *understood*, which that ADR recorded as `STATUS.md` open item 59 and
  explicitly did not settle

## Context

`STATUS.md` open item 59. This engine reads a `UTCTimestamp` at four widths and refuses the
other **seven** — the item counted six, and *Revision 1* below explains where the seventh came
from — and **before a `Logon` that refusal is a hang-up with no byte sent** — the exact
defect half A of [timestamp-micros](../plans/2026-09-04-timestamp-micros.md) was written to
close, still reachable through the interop oracle's own configuration file.

`[measured 2026-09-09]` both readers in this codebase, probed directly:

| `TimestampPrecision` | bytes | example | `session::clock::parse_utc` | `dict::FieldType::UtcTimestamp` |
|---|---|---|---|---|
| 0 | 17 | `20260909-10:00:00` | 63956167200000 | accepts |
| **1** | **19** | `20260909-10:00:00.1` | **`None`** | **refuses** |
| **2** | **20** | `20260909-10:00:00.11` | **`None`** | **refuses** |
| 3 | 21 | `20260909-10:00:00.111` | 63956167200111 | accepts |
| **4** | **22** | `20260909-10:00:00.1111` | **`None`** | **refuses** |
| **5** | **23** | `20260909-10:00:00.11111` | **`None`** | **refuses** |
| 6 | 24 | `20260909-10:00:00.111111` | 63956167200111 | accepts |
| **7** | **25** | `20260909-10:00:00.1111111` | **`None`** | **refuses** |
| **8** | **26** | `20260909-10:00:00.11111111` | **`None`** | **refuses** |
| 9 | 27 | `20260909-10:00:00.111111111` | 63956167200111 | accepts |
| **12** | **30** | `20260909-10:00:00.111111111111` | **`None`** | **refuses** |

The two readers now agree, which is what half B's interop scenario bought
([one-field-two-readers](../reference/one-field-two-readers.md)); they agree on a set that is
too small.

**The oracle, read at the pin rather than cited.** `[researched 2026-09-09]` QuickFIX C++ at
`386ce46e` — the SHA `scripts/fetch-quickfix-assets.sh` pins — `src/C++/FieldConvertors.h`,
`UtcTimeStampConvertor`. The blob is in `vendor/quickfix`'s object store but **not in the sparse
checkout**, so it was read with `git cat-file -p 386ce46e:src/C++/FieldConvertors.h` and nothing
was fetched, widened or committed:

- **Serialising** clamps `precision` to `0..=9` and writes `17 + 1 + precision` bytes, or 17 when
  the precision is 0.
- **Parsing** refuses `length < 17 || length > 27` and then, past the seconds, requires a `.` and
  runs `for (; i < length; ++i)` over the rest demanding digits. So **every length from 17 to 27
  is accepted, 18 included** — a stamp ending in a bare `.` parses, with `fraction = 0`.
- The fraction is handed on as `(fraction, length - 17 - 1)`, and `FieldTypes.h`'s
  `convertToNanos` multiplies it by `PRECISION_FACTOR[precision]`, the table
  `{1000000000, 100000000, 10000000, 1000000, …}`.

**That last line is the one that decides the shape of the fix.** `PRECISION_FACTOR[1]` is
100 000 000 ns, so `.1` is **one tenth of a second — 100 ms, not 1 ms**. A fractional field is a
decimal fraction, so the digits are read left-aligned and padded on the right, never parsed as an
integer.

## Decision

**1. Reading: every width the wire can carry, which is 17 and 19 through 30.** `parse_utc` and
`dict`'s `time` both take a `UTCTimestamp` with a seconds field and, optionally, a `.` followed by
**one to twelve** digits. Both readers change together, in one commit, and a test asserts they
agree across the whole range — the half-A defect was one reader moving alone.

`[revised 2026-09-09]` **The upper bound is 12 digits, not 9, and the survey is why.** QuickFIX/J
accepts **30 bytes** — picoseconds — and the FIX Technical Addendum on time precision extends the
timestamp type to them. A 30-byte stamp is refused here exactly as silently as a 19-byte one, so
leaving it out would close six instances of the defect and leave a seventh standing for no reason
beyond not having counted it. Nothing above 30 is accepted: no surveyed engine sends it and the
type has no wider form.

**2. Eighteen bytes stays refused, and this is a deliberate divergence from the oracle.** A `.`
with no digit after it is not a fraction. QuickFIX accepts it as `fraction = 0`; nothing can make
it *send* one, because precision 0 writes no `.` at all, so the divergence is unreachable through
the oracle and is a laxity rather than a requirement. `docs/SESSION-BEHAVIOUR.md` records it as a
known, chosen difference rather than leaving it to be discovered.

**3. The fraction is left-aligned and truncated to milliseconds, never parsed as an integer.**
`.1` is 100 ms, `.12` is 120 ms, `.1239` is 123 ms. D13 makes the return value milliseconds and
[ADR-0057](ADR-0057-sub-millisecond-time-arrives-beside-the-tick.md) already fixed truncation over
rounding; this decision only says the digits are **positional**. The alternative — reading the
digits as a number — is wrong by up to 99 ms and **no gate in this repository could see it**,
because every skew is judged against a 120 000 ms bound.

**4. Strict out, liberal in, and the asymmetry is the decision.** ADR-0057 decision 1 keeps the
send side at 3, 6 or 9 digits: those are the widths the FIX 5.0 SP2 EP wording sanctions, and a
venue that measures clocks reads a published width as a claim about resolution. Nothing about
being generous on the way *in* licenses being loose on the way *out*, and this ADR does not touch
`TimestampPrecision`'s accepted values.

**5. The gate is somebody else's engine, configured for a width this one does not send.** A new
`scripts/interop.sh` scenario runs the C++ end at `TimestampPrecision=2` — a width **fixbolt
cannot produce**, so the assertion cannot be satisfied by this repository talking to itself. Its
reversal is the code of today: the session hangs up on the `Logon` and the scenario fails on
*never logged on*.

## Consequences

**Good.**

- The silence goes away for **seven** widths a QuickFIX counterparty can be configured to send —
  the six between 19 and 26, and picoseconds at 30 — and the one remaining refusal (18 bytes) is
  unreachable from any of them.
- One rule replaces a four-entry table in both readers, so a third precision cannot be added to
  one and forgotten in the other. The shape that caused
  [one-field-two-readers](../reference/one-field-two-readers.md) stops existing.
- `dict`'s `time` is shared with `UTCTIMEONLY`, which gains the same widths for free and by the
  same argument.

**Bad, and named rather than discovered.**

- **This engine becomes more permissive than the specification it cites.** The EP wording
  *enumerates* 3, 6 and 9 (and the Technical Addendum adds 12) rather than giving a range; 1, 2,
  4, 5, 7 and 8 are accepted here because a real engine sends them, not because a document
  blesses them. A counterparty whose stamps are malformed in exactly this
  way will now be tolerated rather than told.
- **The corpus still cannot see any of it.** 0 of the 59 definitions carry a stamp wider than 21
  bytes. `59 / 59` after this change means *nothing regressed* and cannot mean *this works* — the
  same honest limit half A recorded, and the reason decision 5 puts the gate outside.
- **A branch on the fraction length replaces a `match` on four constants** on the receive path.
  It is not the send path ADR-0057 measured at +0.7 ns, but it is not free either, and the plan
  owes a `benches/` reading rather than an assurance.
- 18 bytes is now the only width in 17..=27 that this engine refuses and QuickFIX accepts. That is
  one asymmetry to remember instead of six, but it is not zero.

## Alternatives considered

**(a) Refuse the six widths, but loudly.** Keep the strict set and answer a `Logon` carrying an
unreadable stamp with a `Logout` naming the fault rather than a hang-up. Rejected: the corpus
requires silence before a `Logon` (`1c`, `1d`) for exactly this class, so "loudly" would mean
diverging from the acceptance gate to preserve a strictness that no venue asked for. It also
leaves a working counterparty unable to connect, which is the defect, not a milder form of it.

**(b) Accept 17..=27 exactly as QuickFIX does, 18 included.** Rejected by decision 2: the
divergence is unreachable through the oracle, so accepting it buys no interoperability and gives
up the ability to say a bare `.` is malformed.

**(c) Widen `parse_utc` only, and leave `dict`.** That is precisely the bug of 2026-09-08, whose
write-up is [one-field-two-readers](../reference/one-field-two-readers.md). Named here so it
cannot be arrived at again by accident.

**(d) `[added 2026-09-09]` Keep a strict enumeration and simply add 30 — 17, 21, 24, 27, 30 —
which is what QuickFIX/J does and very nearly what quickfix-go does.** This is the serious
alternative, and it is not a straw man: **half the QuickFIX family is strict**, and a strict set
is what the specification's own wording enumerates. Rejected on one asymmetry, stated in
[prior-art.md](../reference/prior-art.md#researched-2026-09-09-five-engines-read-the-same-field-five-different-ways):
accepting a width can never break interoperability with a strict engine, because a strict engine
never *sends* one; refusing a width does break it, and here it breaks it as silence before a
`Logon`. The strict set buys conformance to a sentence and costs a connection.

## Revision 1 — 2026-09-09, before approval

The owner asked, in place of approving the first draft, for a survey of how other engines read
this field. It was done by reading five parsers rather than five feature lists, and the table is
in [prior-art.md](../reference/prior-art.md#researched-2026-09-09-five-engines-read-the-same-field-five-different-ways).
Three things came back, and the second changed this page:

1. **The QuickFIX family splits two against two.** C++ and .NET take a range; J and Go take an
   enumeration. "Match the oracle" was never a single instruction, and the first draft of this ADR
   had quietly assumed it was.
2. **Picoseconds exist and this engine refuses them silently.** QuickFIX/J accepts 30 bytes.
   Decision 1's bound moved from 9 fractional digits to 12, and `STATUS.md` item 59's "six of the
   ten precisions" is really **seven of eleven**.
3. **The positional reading of the fraction was confirmed by a second, independent
   implementation.** QuickFIX/n's `ParseFraction` starts at `decimalBase = 0.1` and multiplies
   down, which is the same arithmetic as QuickFIX C++'s `PRECISION_FACTOR` table reached by a
   different route. `.1` is 100 ms in both. Decision 3 was right and is now doubly sourced.

**What did not change.** Decisions 2 (18 bytes stays refused), 3 (positional truncation) and 4
(strict out, liberal in) stand as first written. Decision 2 is if anything better supported: three
of the four QuickFIX ports reject a bare `.`, and C++ is alone in accepting it.

## Sources

- QuickFIX C++ at `386ce46e`: `src/C++/FieldConvertors.h` (`UtcTimeStampConvertor`),
  `src/C++/FieldTypes.h` (`convertToNanos`, `PRECISION_FACTOR`). Read from the object store, not
  fetched into the working tree.
- [ADR-0057](ADR-0057-sub-millisecond-time-arrives-beside-the-tick.md), decision 1 and its
  ceiling rule.
- [one-field-two-readers](../reference/one-field-two-readers.md) — the two-reader failure this
  decision is shaped to make unrepeatable.
- `STATUS.md` open item 59.
