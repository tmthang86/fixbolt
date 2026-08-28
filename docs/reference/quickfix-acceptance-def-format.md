# The QuickFIX `.def` acceptance format

The FIX 4.4 session acceptance definitions are the gate for nanofixengine's session layer
([ADR-0001](../decisions/ADR-0001-relationship-to-quickfix.md)). This page records what the
format actually is, so nobody has to re-derive it.

Everything below was read off the files themselves and off `test/Comparator.rb` on
**2026-08-27**, not inferred. Fetch them with `scripts/fetch-quickfix-assets.sh`; they land
in `vendor/`, which is **gitignored and never committed**.

## Size — smaller than it looks

| | |
|---|---|
| Files in `test/definitions/server/fix44/` | **59** |
| Total lines across all 59 | **1,319** |
| Distinct directives in the whole grammar | **7** |
| Distinct placeholders | **1** (`<TIME>`) |

## Grammar — the complete set

```
# ...                  comment. Metadata lives here: @testcase, @condition, @expected
I<fix-message>          send this message TO the system under test
E<fix-message>          expect this message FROM the system under test
iCONNECT                connect the default session
eDISCONNECT             expect the default session to disconnect
i<N>,CONNECT            connect session N          (multi-session tests)
i<N>,DISCONNECT         disconnect session N
e<N>,DISCONNECT         expect session N to disconnect
```

Counted across the 59 files: 289 `I`, 250 `E`, 61 `iCONNECT`, 61 `eDISCONNECT`, and 8
numbered session directives. There is nothing else.

**The field separator inside `I`/`E` lines is a literal SOH (`0x01`) byte in the file**, not
an escape sequence, not a pipe. Confirmed by hexdump:

```
4938 3d46 4958 2e34 2e34 01 3335 3d41 01 ...
I 8 = F I X . 4 . 4 ␁ 3 5 = A ␁ ...
```

`test/definitions/SOH` is a one-byte file containing `0x01` — it exists so shell tooling can
reference the separator.

`<TIME>` appears 352 times and is the only placeholder; the runner substitutes the current
UTC timestamp when sending.

## Comparison rules — the part that constrains the serialiser

From `Comparator.rb`. An expected `E` line and the received message are compared like this:

1. Split both on SOH.
2. **The field count must be equal.** Any extra or missing field fails.
3. **The field order must be identical, positionally.** Tag *i* of expected is compared with
   tag *i* of received. A correct message with fields in a different order **fails**.
4. For a tag listed in `test/definitions/fields.fmt`, the *received* value is matched against
   a regex and the expected value is ignored:

   | Tag | Field | Pattern |
   |---|---|---|
   | `10` | CheckSum | `\d{3}` |
   | `42` | OrigTime | `\d{8}-\d{2}:\d{2}:\d{2}` |
   | `52` | SendingTime | `\d{8}-\d{2}:\d{2}:\d{2}` or the same with `.\d{3}` |
   | `60` | TransactTime | `\d{8}-\d{2}:\d{2}:\d{2}` |
   | `122` | OrigSendingTime | `\d{8}-\d{2}:\d{2}:\d{2}` |

5. Every other tag is compared by **exact string equality**.

### The trap

Rule 3 is the one that will cost time if it is not known in advance.

**nanofixengine's serialiser must emit fields in exactly the order QuickFIX emits them**, or these
tests fail on messages that are perfectly valid FIX. The acceptance suite is not only a test
of session *behaviour*; it silently pins the *field ordering* of every message the session
layer generates.

Note also what rule 4 implies: `9` (BodyLength) is **not** in `fields.fmt`, so it is compared
exactly. The expected `BodyLength` in each `.def` is therefore a hard assertion that the
message body is byte-for-byte the expected length.

**Guard:** this trap gets a regression test the moment the runner exists — a test that emits
a `Reject` with correct fields in a deliberately wrong order and asserts the runner rejects
it. Without that test, this page is just prose, and prose does not hold a constraint.

## The ordering rule — answered from the data, 2026-08-27

Rule 3 pins field order, so the question was: *what* order? XML declaration order was the
guess. It is wrong.

All **250** `E` lines in the 59 files were checked by script. **247** of them carry `9=` and
**244** carry `10=` — the frame fields below exist only on that subset, and a runner that
assumes every `E` line is a complete frame will trip over the rest. Field ordering holds on
every one:

```
8, 9, 35            fixed, in that order
header fields       ascending by tag number   (34, 49, 52, 56, …)
body fields         ascending by tag number   (45, 58, 371, 372, 373 for a Reject)
10                  last
```

Zero violations. Evidence that it is *not* XML order: `spec/FIX44.xml` declares
`SenderCompID(49)` and `TargetCompID(56)` before `MsgSeqNum(34)`, yet every expected header
reads `35, 34, 49, 52, 56`. This is QuickFIX's `FieldMap` — a tag-sorted map with the three
leading header tags forced first.

**Consequence for the serialiser:** no per-message ordering table is needed. The generated
data the serialiser needs is the **set of header tags**, to split header from body. Within
each section, order is `sort_by_tag`. Repeating groups are the known exception (QuickFIX keeps
declaration order inside a group, delimiter first); the FIX 4.4 acceptance set contains no
populated group — one `454=0` — so groups are out of scope until something needs them.

**Also observed:** 3 `E` lines carry neither `9=` nor `10=`, and 3 more carry `9=` without
`10=`. `Reflector.rb`'s `fixify!` inserts a computed `9=` when absent and a computed `10=`
when absent. The conformance runner must apply the same normalisation to expected lines
before comparing, or those six tests can never pass.

## The `10=` values are placeholders, and the `9=` values are mostly not

`[measured]` 2026-08-28, by parsing all 539 lines through `crates/codec`:

| | Count |
|---|---|
| `E` lines carrying `10=` | 244 |
| …whose value is the real checksum of their own bytes | **0** |
| …that are literally `10=0` | 238 |
| `E` lines carrying `9=` | 247 |
| …that agree with their own body | 244 |

This follows from rule 3 above — the comparator matches tag 10 by regular
expression, so its value never had to be real — but the consequence is worth stating
outright: **a conformance runner that checksum-validates expected output fails all 244
lines and learns nothing.** Frame validation belongs on the `I` side, where the
reflector computes real values, and on the engine's own output.

The three `E` lines whose `9=` is wrong are each **stale by exactly 4 bytes**:

| Line | Declared | Actual |
|---|---|---|
| `14e_IncorrectEnumValue.def:26` | 121 | 117 |
| `8_OnlyApplicationMessages.def:29` | 93 | 89 |
| `RejectResentMessage.def:6` | 63 | 59 |

All three carry a 17-character `SendingTime` where the length was computed for a
21-character one — the missing `.000`. QuickFIX's own fixtures, not a loader bug.

> **Guarded by** `crates/codec/tests/defs.rs` — `the_corpus_checksums_are_placeholders_not_checksums`
> asserts the 0, and `body_length_is_checked_where_the_corpus_declares_one` pins the
> exact six lines whose `9=` disagrees.

## What the deliberately malformed lines actually expect

`[measured]` Six lines across the 539 cannot be parsed. Five are dropped in silence:

| Line | Why | What QuickFIX does |
|---|---|---|
| `2t_FirstThreeFieldsOutOfOrder.def:8` | `35=0` before `8=` | ignored, sequence number **not** consumed |
| `2d`/`3c_GarbledMessage.def:8` | `4garbled9=` | ignored |
| `2d`/`3c_GarbledMessage.def:13` | `49garbled=` | ignored |
| `14a_BadField.def:25` | `-1=HI` | **Reject sent, sequence number consumed** |

The last one is the exception that shapes the codec's API. `2m_BodyLengthValueNotCorrect`
says it in its own comment — *"Invalid message was ignored, and valid one was processed.
Therefore we should expect a resend request"* — so a parser that returns `Err` is right for
the first five. For `14a` it is not enough: `@expected` says *"Send Reject … Increment
inbound MsgSeqNum"*, so the session must read `34=` out of a message the parser could not
finish, and must put the text `-1` into `371=`.

That is why `ParseError::BadTag` carries a **byte offset** rather than a tag value, and why
the index keeps every field read before the failure. Same reasoning as the decision to have
no `EmptyValue` error: refuse only what cannot be read, and never in a way that makes a
definition unpassable.

Note the same file sends `999=`, `0=` and `5000=` and expects a Reject for each. Those are
readable numbers; the parser passes them up and the session rejects them against the
dictionary. Only the unreadable one stops in the codec.

## Cost estimate for the runner

Revised down after reading the files. The runner needs: a line parser for 7 directives,
`<TIME>` substitution, the 5-step normalisation above, and the ~40-line comparator.

**It needs no TCP client.** An earlier version of this page said it did. Because the session
layer is a pure state machine ([DESIGN.md D1](../DESIGN.md#4-the-decisions-that-shape-it)),
the runner drives it **in-process**: feed `I` lines in, compare the `Action::Send` bytes
against `E` lines. No socket, no listener, no timing window, no flake. The `iCONNECT` /
`eDISCONNECT` / `i<N>,CONNECT` directives become calls on the machine, not real connections.
That is the whole reason D1 exists.

**1–2 days**, not the 3–5 originally guessed in ADR-0001 before the format had been read.

## Licence note

These files are QuickFIX-licensed. They are used here as a **test oracle**, fetched at build
time into a gitignored directory. They are never redistributed inside nanofixengine. See ADR-0001.
