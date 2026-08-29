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
each section, order is `sort_by_tag`.

**The blind spot, stated plainly: the 59 definitions prove nothing about repeating groups.**
`[measured]` FIX 4.4 declares 93 of them and the acceptance set populates exactly **one** —
`386=3` in `14i_RepeatingGroupCountNotEqual.def`, a file whose whole purpose is a *wrong*
count. `454` appears twice, both `=0`. So a session that is 59/59 has had its group handling
tested by two entries of one group in one negative test.

Inside a group the tag-ascending rule above **does not hold**: order is declaration order,
delimiter first. That sentence used to sit here as an aside marked "out of scope"; it is now
`[measured]` and gated, but by two tests that have nothing to do with this corpus —
`crates/codec/tests/group_roundtrip.rs` and `crates/dict/tests/interop_quickfix_order.rs`,
the latter checking the order against QuickFIX's own generated C++ on 730/730 groups. See
[fix44-dictionary-traps.md](fix44-dictionary-traps.md) traps 5 to 7.

**Do not reach for this corpus to test a group.** It has no group to test with.

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

## The trap that cost the most: never `cat *.def`

`[measured 2026-08-28]` **35 of the 59 files do not end in a newline**, and most files begin
with a `#` comment. So concatenating them glues the last line of one file onto the first line
of the next:

```
$ tail -1 10_MsgSeqNumEqual.def      # no trailing newline
eDISCONNECT
$ head -1 10_MsgSeqNumGreater.def
# @testcase 10 - Message sequence number greater than expected
$ cat 10_MsgSeqNumEqual.def 10_MsgSeqNumGreater.def | grep '^e'
eDISCONNECT# @testcase 10 - Message sequence number greater than expected
```

The corpus then appears to carry comments on the same line as a directive. It does not:
**0 lines in the corpus have a `#` after a directive.** Counting exact `eDISCONNECT` over the
concatenated blob gives **28**; the real number is **64**.

This is not a hypothetical. The claim "a `#` comment can sit on the same line as an `i`/`e`
directive" reached a plan's *"what is known for certain"* section as a measured fact, and the
loader was about to be written to strip something that is never there — code that guards
nothing, justified by a comment that is false.

**What survives concatenation and what does not.** Counting lines by their *first* character
is safe, because gluing only damages the end of a line: `I` 289, `E` 250, `i` 66, `e` 64 come
out right either way. Anything matching a whole line, or reading the last field of a line, is
wrong. The `58=`, `373=` and `35=` tallies on this page were re-measured per file on
2026-08-28 and were unaffected.

**Guarded by** `crates/conformance/tests/script.rs::concatenating_the_files_corrupts_the_corpus`,
which asserts both numbers — the 28 the naive reading produces and the 64 the loader sees.

## `<TIME>` is not one width, and the corpus's `9=` values say which

`[measured 2026-08-28]` Solving each line's declared `BodyLength` for the length of the
`<TIME>` it contains, over every line that carries its own `9=`:

| Line | `<TIME>` width | Evidence |
|---|---|---|
| `I` | **17** — `YYYYMMDD-HH:MM:SS` | `2d_GarbledMessage` and `3c_GarbledMessage`, two lines each, all four agree |
| `E` | **21** — with `.mmm` | `SessionReset.def` lines 18 and 27 |

An `E` line is the engine's own output and FIX 4.4 `SendingTime` carries milliseconds; an `I`
line is what the reflector sends, and it does not. Substituting one width everywhere costs
four bytes per timestamp, and nothing notices until something compares a `9=`.

### And three `E` lines write a placeholder their own `9=` does not match

`14e_IncorrectEnumValue.def:26`, `8_OnlyApplicationMessages.def:29` and
`RejectResentMessage.def:6` each carry a **literal** `52=` of 17 bytes while their `9=` is
computed for 21. They are not stale. They are the same phenomenon as `10=0`: **tag 52 is
matched by regex, so its written value never had to be right, and the author computed `9=`
for what the engine actually emits.**

The consequence is the one that matters: an engine passing this suite must write `SendingTime`
**with milliseconds**, because three `9=` values depend on it and tag `9` is compared exactly.

### The other side of the same rule: `60=` is echoed, not regenerated

`15_HeaderAndBodyFieldsOrderedDifferently.def` expects `9=101`, and that number only comes out
when `52` is 21 bytes and `60` is **17** — the value copied verbatim off the inbound message.
An engine that regenerates `TransactTime` in its own format moves the body by four bytes and
fails a test whose name says nothing about time.

**Guarded by** `crates/conformance/tests/script.rs::
a_time_placeholder_is_seventeen_bytes_inbound_and_twenty_one_outbound` (343 inbound at 17, 0 at
21; 247 outbound at 21) and `crates/conformance/tests/echo.rs::body_length_is_one_hundred_and_one`.

## What the acceptance server actually is: an echo server that re-orders

`[measured 2026-08-28]` 42 of the 250 `E` lines carry `35=D`, and there are **22 `(I, E)`
application pairs** across the 59 files. The server under test sends application messages
straight back. **A session state machine alone cannot pass this suite** — fifteen files need an
application behind it.

It re-orders, and that is the point of
`15_HeaderAndBodyFieldsOrderedDifferently.def`: the same `NewOrderSingle` arrives twice, once
in order and once shuffled, and the **same bytes** are expected back both times.

Which header fields come back is not "all" or "none", and the corpus is emphatic:

| Tag | | Echoed? | The file that says so |
|---|---|---|---|
| `97` | PossResend | **yes** | `19b_PossResendMessageThatHasNotBeenSent.def` |
| `122` | OrigSendingTime | **no** | `2m_BodyLengthValueNotCorrect.def` |

The line runs between resend metadata, which belongs to the transmission, and a flag the
counterparty set about the order itself. Guessing "all header fields" fails the second;
guessing "no header fields" fails the first.

**Guarded by** `crates/conformance/tests/echo.rs` — all 22 pairs reproduced, plus
`poss_resend_is_echoed_and_orig_sending_time_is_not`.

## `00000000-00:00:00` is a placeholder, not a time — and substituting it breaks the clock

`[measured 2026-08-28]` Two different things in this corpus look identical and are not:

| Where | What it is | What a loader must do |
|---|---|---|
| `52=<TIME>`, 288 occurrences, almost all on `I` lines | **Input.** QuickFIX's reflector substitutes the real clock before sending | Substitute a **real instant** |
| `52=00000000-00:00:00.000`, 244 occurrences, all on `E` lines | **A placeholder for expected output**, for a tag the comparator matches by shape and never by value | Leave it alone |

This loader substituted the placeholder for `<TIME>` as well, for a year, and nothing noticed —
because until a session existed, nothing parsed a `52=` value. It is **not a date**: month 00,
day 00. A `SendingTime` check that accepts it accepts anything, and one that rejects it rejects
every message in the corpus.

The corpus writes the same placeholder into every one of the five `fields.fmt` tags on an `E`
line — `10=0`, `52=`, `60=`, and where they appear `42=` and `122=`. **A test that compares an
`E` line byte for byte is comparing placeholders**, and it passes only for as long as the
loader happens to produce the same ones. One did, in `crates/conformance/tests/echo.rs`, and it
went red the moment `<TIME>` became a real instant. The fix is to compare by the corpus's own
rule and assert the specific invariant separately.

**Guarded by** `crates/session/src/clock.rs::the_corpus_placeholder_is_not_a_date` and
`crates/session/tests/score.rs::the_harness_clock_and_the_corpus_agree`, which proves the
number the runner ticks with is the instant the loader writes.

### `<TIME±N>` ran backwards, and only one file would ever have shown it

Four lines carry an offset: `<TIME-1>`, `<TIME+10>`, `<TIME-121>`, `<TIME+121>`. With the base
at midnight of year zero there is nowhere to go backwards to, so the substitution wrapped with
`rem_euclid` — turning `<TIME-121>` into **86 279 seconds forward**, in
`2o_SendingTimeValueOutOfRange.def`, the one file in the corpus that exists to test
`SendingTime` accuracy. Both halves of that test would have measured the same sign.

A base with room on either side — midday of an ordinary day — makes the arithmetic ordinary.
Years away from the four hard-coded `52=` values in the corpus (2001, 2002, 2004), so none of
them becomes accidentally fresh.

## A file's name is not its test: `1e_NotLogonMessage.def`

`[measured 2026-08-28]` Deleting the "the first message must be a Logon" rule from the session
**leaves the score unchanged at 6 / 59.** The file named for that rule sends:

```
I8=FIX.4.4^A35=0^A34=1^A49=TW44^A52=<TIME>^A56=DLSI^A
```

`56=DLSI` is the wrong TargetCompID. Whichever check runs first ends the connection, and both
produce the same `eDISCONNECT`, so the corpus cannot distinguish them. **Two rules, one
observation.**

This is not the only such pair — it is the first one a reversal caught. The general shape:
a `.def` file proves *some* rule fired, never *which*. Anywhere two rules share an outcome, the
score is satisfied by either, and only a test written outside the corpus separates them.

**Guarded by** `crates/session/tests/logon.rs`, which takes that same corpus line, corrects
`56=` to `ISLD`, and asserts the drop still happens — and, as its other half, that flipping
`35=0` to `35=A` is then accepted.

## A message with no `10=` parses as `Incomplete`, and `Incomplete` means "wait"

`[measured 2026-08-28]` A test helper rebuilt a message's `9=` and forgot to re-append the
trailer. `parse_into` returned `Ok(Parsed::Incomplete)` — correct: without `10=` the frame is
not finished. The session treats `Incomplete` as *wait for more bytes*, which is also correct,
and returns `Link::Up`.

So **both halves of a two-sided test passed on a message that was never judged**: the "must be
accepted" case, and — vacuously — nothing at all. It surfaced only because the case that was
*supposed* to fail also passed.

Any hand-built FIX message in a test needs its `10=` present before `with_real_checksum` can
replace it, and any test that asserts `Link::Up` should be paired with one asserting
`Link::Dropped` on the same wire. A green from `Incomplete` looks exactly like a green from
`Ok`.

## An `E` line with no `I` in front of it is the corpus's only way to say "wait"

`[measured 2026-08-29]` **33 of the 250 `E` lines** do not follow an `I` line.
There is no `WAIT` directive in the grammar, and none is needed: an expected
message with no input in front of it can only be the engine speaking on its own,
and the only thing that makes an engine do that is time passing.

So the runner's rule is: **before matching an `E` line, if the session has said
nothing, push the clock forward by one `HeartBtInt` and try again, at most three
times.** The same rule applies to `eDISCONNECT` — `6_SendTestRequest.def` ends
by running out of patience, with no message in between. `WAITS` in
`crates/conformance/src/runner.rs`.

`HeartBtInt` comes from **the file's own Logon**, not from configuration:
`[measured]` `108=30` in most files, `108=6` in `4a_NoDataSentDuringHeartBtInt`
and `6_SendTestRequest` — the only two whose output depends on it at all.

### And that granularity hides every threshold the session has

Because the harness can only tick a whole interval at a time, the corpus cannot
see where any of QuickFIX's three timers actually sit. `[measured 2026-08-29]`
against `6_SendTestRequest.def`:

| Timer | QuickFIX's value | What the corpus would also accept |
|---|---|---|
| heartbeat due | 1.0 × `HeartBtInt` since we last sent | anything in (0×, 1×] |
| test request due | 1.2 × (n+1) × `HeartBtInt` since they last sent | anything in (1×, 2×] |
| give up | 2.4 × `HeartBtInt` since they last sent | anything in (2×, 3×] |

`crates/session/tests/heartbeat.rs` ticks by the millisecond and asserts the
boundary on both sides. Every one of the three has a reversal that is red only
there.

## `TestReqID` is compared byte for byte, so `112=TEST` is a constant

Tag `112` is **not** in `test/definitions/fields.fmt`, so rule 5 applies and the
expected value is matched exactly. `[measured]` the distribution across `E`
lines: `HELLO` ×23 — the ID the counterparty sent, thrown back — **`TEST` ×2**,
`HELLO1`/`HELLO2` ×1, `1` ×2.

The two `TEST`s are the ones the engine invented, in `6_SendTestRequest.def`.
Nothing in FIX 4.4 requires any particular string there; a counter or a
timestamp is equally correct on the wire and fails this gate. **It is QuickFIX's
default leaking into the oracle**, and `OWN_TEST_REQ_ID` in
`crates/session/src/lib.rs` is the one place that depends on it.

## Whether a message's sequence number is checked depends on its `MsgType`

This is not a session-wide rule with exceptions; it is a per-handler argument.
QuickFIX's `Session::verify(msg, checkTooHigh, checkTooLow)` is called with
different arguments from `nextLogon`, `nextLogout`, `nextSequenceReset` and the
rest, and **`nextSequenceReset` is the only handler that never advances the
inbound count at all**.

| `35=` | too high | too low | advances `34=` in | The file that proves it |
|---|---|---|---|---|
| `A` Logon | **after the reply** | yes | only if not too high | `1a_ValidLogonMsgSeqNumTooHigh` |
| `5` Logout | no | **no** | yes | `10_MsgSeqNumEqual` — gap-fills to 20, then logs out with `34=3` |
| `2` ResendRequest | no | **no** | yes | `8_OnlyAdminMessages` — sends `34=5` twice, the second time behind the count |
| `4` SequenceReset, `123=Y` | yes | yes | **no** | `10_MsgSeqNumLess` |
| `4` SequenceReset, no gap fill | **no** | **no** | **no** | `11a`, `11b`, `11c` — all three send `34=0` |
| everything else | yes | yes | yes | `2c`, `14a`, `2b` |

A session that applies one rule to everything scores **36 / 59** instead of 37,
and the file it loses depends on which rule it picked. `[measured 2026-08-29]`
by reversal, four ways.

`11c_NewSeqNoLess.def` is the sharpest of these: the `SequenceReset` it rejects
carries `34=0`, the Reject that answers it carries `45=0`, and the *next*
message is `34=2` — so the rejected message did not consume a number, unlike
every other Reject in the corpus.

## `MsgType` must be the third field, and the codec does not say so

`ParseError::BadFrameStart` covers `8=` at byte 0 and `9=` immediately after it,
and its documentation says MsgType's position is deliberately not the parser's
business. Something still has to enforce it: QuickFIX's own parser throws
`InvalidMessage` when `35=` is not third.

`[measured 2026-08-29]` **`2t_FirstThreeFieldsOutOfOrder.def` is the only file
in the corpus that sends one** — two of them, `35=0|8=…` and `8=…|34=3|35=0` —
and both expect to be **ignored**, with the connection carrying on and the
inbound sequence number untouched.

## A frame that cannot be read is fatal only if it says it is a Logon

QuickFIX catches `InvalidMessage`, then digs the `35=` out of the raw bytes with
`identifyType` and disconnects **only** when it reads `A`. Everything else is
logged and dropped on the floor.

The corpus states this once from each side and the two halves are in different
files: `1d_InvalidLogonLengthInvalid.def` is a Logon with a wrong `9=` and
expects the link to go; `2d`, `3c` and `2t` are garbled non-Logons and expect
the next message to be read as though nothing happened.

**One file cannot separate the rule from a weaker one.** "Any unreadable frame
before a Logon is fatal" passes `1d` just as well and is wrong.
`crates/session/tests/heartbeat.rs::a_garbled_frame_is_fatal_only_when_it_claims_to_be_a_logon`
holds both halves against the same bytes.


## A gap is asked for **once**, and the Logon is answered before it is asked

A message running ahead of the count is not refused: it is held, and the session
asks for what it missed with `ResendRequest (35=2)` carrying `7=` the number it
expects and `16=0` — "and everything after", which is how FIX 4.2 and later
write infinity.

Two things about that are stated exactly once each in the corpus.

**Once per gap.** `10_MsgSeqNumGreater.def` sends `34=10` and then `34=20` while
the gap is open, and expects **one** `ResendRequest`. QuickFIX's
`doTargetTooHigh` returns early when a resend is already outstanding and the new
number is at or past the one it asked from. A session that asks twice fails the
file on the second message, as output no line asked for.

**The Logon first.** `1a_ValidLogonMsgSeqNumTooHigh.def` opens with `34=5` on an
empty session and expects `35=A` and *then* `35=2`, in that order. QuickFIX's
`nextLogon` answers the Logon, and only afterwards asks whether the number ran
ahead — so a Logon with a gap in front of it still logs the session on, and the
inbound count does **not** move past it.

### What the corpus cannot see about a gap

`[measured 2026-08-29]` every file that opens a gap ends before opening a second
one, and the deepest any of them holds is **two** messages. So three behaviours
score the same either way and have their own tests in
`crates/session/tests/resend.rs`:

- **A filled gap is closed.** A session that never clears the outstanding range
  scores 42 / 59 and then never asks again — it has already asked. The next gap
  in a real session goes unrequested, in silence.
- **Held messages are replayed in sequence order.** `RejectResentMessage.def`
  holds exactly one, so it proves only that a held message precedes a fresh one.
- **What happens when there is no room.** The held message must be dropped
  rather than truncated, and dropping it must leave the count where it was, so
  the counterparty's next message running ahead asks for it again.

## An inbound `ResendRequest` over administrative messages is answered with one gap fill

QuickFIX never replays an administrative message — a Logon, a Heartbeat, a
Reject are all meaningless out of their moment. It emits a `SequenceReset` with
`123=Y` covering them instead, and `8_OnlyAdminMessages.def` is the file whose
name says exactly that.

The shape is specific, and every part of it is compared:

```
35=4 | 34=<BeginSeqNo> | 43=Y | 49 | 52 | 56 | 122=<a timestamp> | 36=<EndSeqNo + 1> | 123=Y
```

- **`34=` is the first number of the range being filled**, not the next outbound
  number, and sending it does **not** spend one. The file fills `34=1` while its
  next real message is `34=5`, and both are in it.
- **`36=` is one past the last number filled.** `16=4` gives `36=5`; `16=0`
  means "everything sent so far", so the end becomes the last number this end
  used and `36=` is the next one.
- `43=Y` and `122=` are there because a gap fill stands in for a resend, and a
  resent message carries both. `122` is one of the five tags matched by shape,
  so only its width is pinned — 21 bytes, with milliseconds, and the file's own
  `9=93` is what says so.
