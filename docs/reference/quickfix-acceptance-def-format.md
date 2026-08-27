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

All **247** `E` lines in the 59 files were checked by script. Every one follows:

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

## Cost estimate for the runner

Revised down after reading the files. The runner needs: a line parser for 7 directives,
`<TIME>` substitution, a TCP client that can hold more than one session, and the ~40-line
comparator above.

**1–2 days**, not the 3–5 originally guessed in ADR-0001 before the format had been read.

## Licence note

These files are QuickFIX-licensed. They are used here as a **test oracle**, fetched at build
time into a gitignored directory. They are never redistributed inside nanofixengine. See ADR-0001.
