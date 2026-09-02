# A checksum is blind to a swap, and so is a length

`[measured 2026-09-02]` — found while building `crates/library`, by a reversal that was
expected to be a formality.

## The shape

A FIX message carries two self-describing fields: `9=BodyLength` and `10=CheckSum`. Between
them they are supposed to catch a message that has been damaged. They are the two fields a
test reaches for when it wants to assert *"the message is well formed"* without writing out
every byte.

`fixbolt`'s library layer writes `49=SenderCompID` and `56=TargetCompID` into a reply by
**reversing** them out of the incoming message: this side's sender is the name the
counterparty addressed the message to. Getting that backwards addresses the reply to
yourself, and the counterparty either ignores it or drops the session.

The reversal was broken on purpose, to see the test that guards it go red. It did — on the
field assertion. **Both self-describing fields were unchanged:**

```
correct  8=FIX.4.4|9=111|35=8|34=7|49=US   |52=…|56=ALPHA|…|10=137|
swapped  8=FIX.4.4|9=111|35=8|34=7|49=ALPHA|52=…|56=US   |…|10=137|
                     ^^^                                        ^^^
                     same                                       same
```

`9=` is the same because a swap moves the same bytes: `US` and `ALPHA` are still five and two
characters, on either side of the swap. `10=` is the same because a checksum is a **sum**, and
addition does not care where a byte sits.

## Why it matters beyond FIX

**A test that asserts a message's length and its checksum has asserted nothing about where
anything is.** Both are functions that discard order — one discards it by construction, the
other by being a sum. Two of the commonest "is this well formed" assertions in any wire
protocol are exactly blind to the class of bug that reorders, swaps or transposes.

The class is not exotic. Swapping two values of the same type is what you get from:

- a reversal written the wrong way round — the case here;
- two positional arguments in the same order in the caller and the opposite order in the
  callee;
- a `from`/`to` pair, a `src`/`dst` pair, a `debit`/`credit` pair, a `lat`/`lon` pair;
- any `.zip()` over two collections that came from the same place.

A length check and a hash **do** catch truncation, corruption and insertion. They are not
useless. They are just orthogonal to placement, and it is easy to read "the checksum passed"
as "the message is right".

## What made the difference

The assertion that caught it compares **the whole message, byte for byte, against a literal**
— not field by field, and not by any digest of it:

```rust
const EXPECTED: &[u8] = b"8=FIX.4.4\x019=111\x0135=8\x0134=7\x0149=US\x01…";
assert_eq!(show(&out[range]), show(EXPECTED));
```

Two things about that are load-bearing:

1. **The expectation is a literal, written once by hand.** Building it with the same builder
   the code under test uses would make the two agree by construction, and a swap inside the
   builder would appear on both sides.
2. **It compares position, not content.** `assert_eq!` on the whole byte string is an ordered
   comparison. Any assertion that sums, sorts, counts or set-compares would have passed.

The `show()` helper renders `0x01` as `|` so the failure prints two readable lines side by
side rather than two escaped blobs. That is not cosmetic: the diff is what tells you *which*
two fields swapped, and an unreadable diff is a failing test that still costs an hour.

## The rule

**Whenever a value can be swapped with another of the same type, assert on position.** A
length, a checksum, a hash, a count, a sum and a set comparison are all order-blind, and
reaching for one of them is how a transposition ships.

And when a reversal is expected to be a formality, run it anyway. This one was.

`[to testing-skills]`
