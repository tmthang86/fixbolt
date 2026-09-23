# ADR-0120 — A decimal is a mantissa and a signed exponent, read by a free function, and it round-trips byte for byte only in canonical form

- **Status**: **Accepted — 2026-09-23** (by the manager under the owner's standing mandate, with the phase 3 row 4 plan). Proposed 2026-09-23. Written by the architect (Opus) for phase 3 row 4
  ([plan](../plans/2026-09-23-p3-decimal.md)). On acceptance it **supersedes
  [ADR-0028](ADR-0028-a-decimal-is-a-copy-value-parsed-on-demand.md) decision 1** (the
  representation) and **the signature in decision 2** (`view.decimal(tag)`), and **narrows the
  round-trip claim in ADR-0028's *Consequences*** (*"byte-identical across the corpus"*).
  ADR-0028 decisions 2 (on demand, nothing stored per message), 3 (no binary floating point in
  the public API), 4 (the scale that came off the wire is kept) and 5 (nothing enters the session
  layer) stand unchanged and are used below. ADR-0028's text is not edited beyond a status line
  naming this ADR, following the precedent ADR-0100 set for ADR-0045.
- **Date**: 2026-09-23
- **Deciders**: proposed by the architect; accepted by the manager under the owner's standing
  mandate, or by the owner.
- **Related**: [ADR-0028](ADR-0028-a-decimal-is-a-copy-value-parsed-on-demand.md),
  [ADR-0079](ADR-0079-one-view-per-encoding-and-one-trait-over-them.md),
  [ADR-0097](ADR-0097-phase-3-makes-the-engine-dependable-by-a-stranger-and-fixp-waits-on-a-running-oracle.md)
  exit criterion 5, `DESIGN.md` D2 and D16, `crates/dict/src/field_type.rs` (`signed_number`).

## Context

ADR-0028 was accepted on 2026-09-01 and never built. Four things it could not see then are on
disk now, and one of them contradicts it.

### 1. The second encoding's decimal has a signed exponent — `[documented]`, `[read in repo]`

Phase 2 added SBE (`crates/sbe`, ADR-0078, ADR-0081). The SBE 1.0 standard encodes a price as a
composite of a signed integer **mantissa** and a signed `int8` **exponent**, value = mantissa ×
10^exponent; the exponent spans −128..127 and may be positive; `decimal64` / `decimal32` carry
the exponent as a schema constant; an optional decimal is null when its mantissa is the
mantissa type's minimum (`i64::MIN` for `int64`)
([SBE 1.0 §Decimal encoding](https://github.com/FIXTradingCommunity/fix-simple-binary-encoding/blob/master/v1-0-STANDARD/doc/02FieldEncoding.md)).
This workspace's own SBE fixtures already carry exactly that shape: `OPT_DECIMAL` is an `int64`
mantissa with null `i64::MIN` and a constant exponent −3; `QTY` is an `int32` mantissa with
constant exponent 0 (`crates/sbe/src/tests.rs:86-120`, `crates/sbe/tests/encoding.rs:44-75`).

ADR-0028 decision 1 fixed `scale: u8`, and its open question 3 asked *"`u8` scale or `i8`?"*.
A `u8` scale cannot hold an SBE decimal whose exponent is positive without multiplying the
mantissa out — which can overflow, and which throws away the exponent that decision 4 says is
kept. Artio, the precedent ADR-0028 cites, stores its scale in an `int` bounded at 127 and uses
−128 as its NaN sentinel
([DecimalFloat.java](https://github.com/artiofix/artio/blob/master/artio-codecs/src/main/java/uk/co/real_logic/artio/fields/DecimalFloat.java)),
which is the `int8` range by another name.

### 2. The session already has a FIX float grammar, and the corpus pins it — `[read in repo]`

`crates/dict/src/field_type.rs` `signed_number` is what `FieldType::{Float, Qty, Price,
PriceOffset, Amt, Percentage}::accepts` runs, and the session answers `373=6` when it says no.
Its grammar: an optional leading `-`, digits, at most one `.`, at least one digit somewhere;
`.5` and `5.` are both accepted. `14f_IncorrectDataFormat.def` sends `38=+200.00` and expects
`373=6`, so a leading `+` is refused **by the oracle**, not by taste. This matches the
specification — *"Sequence of digits with optional decimal point and sign character (ASCII
characters "-", "0" - "9" and ".")"*
([FIX 5.0 SP2 data types](https://b2bits.com/fixopaedia/fixdic50-sp2/data_types_.html)) — and
both QuickFIX engines: QuickFIX C++'s `DoubleConvertor` accepts one leading `-`, digits and one
`.`, needs at least one digit, and fails on any byte left over, `e` included
([FieldConvertors.h](https://github.com/quickfix/quickfix/blob/master/src/C%2B%2B/FieldConvertors.h));
QuickFIX/J's `DoubleConverter` throws on `+` explicitly and on any non-digit other than one `-`
and one `.`
([DoubleConverter.java](https://github.com/quickfix-j/quickfixj/blob/master/quickfixj-base/src/main/java/quickfix/field/converter/DoubleConverter.java)).
QuickFIX/J's `DecimalConverter`, by contrast, hands the string to `new BigDecimal(value)`
unchecked, so it accepts `+` and `1E3`
([DecimalConverter.java](https://github.com/quickfix-j/quickfixj/blob/master/quickfixj-base/src/main/java/quickfix/field/converter/DecimalConverter.java))
— the two converters of one engine disagree, which is the failure a single grammar prevents.

A decimal reader that accepts a value the session refuses, or refuses as malformed a value the
session accepts, is two rules for one thing (`CLAUDE.md`: one rule, one place).

### 3. The corpus itself is not canonical — `[measured 2026-09-23]`

`grep` over the FIX 4.4 server `.def` files for the float-typed tags 6, 14, 31, 32, 38, 44, 99,
151, 152 finds `38=100` ×15, **`38=002000.00` ×4**, `38=200.00` ×2, `6=0.0`, `151=100`, `14=0`
and the refused `38=+200.00`. The specification allows leading zeros and a trailing point —
*"00023.23 = 23.23"*, *"23.0" = "23.0000" = "23" = "23."*
([FIX float](https://fix.dev/kb/data-types/float)). **No formatter that keeps scale and drops
leading zeros can reproduce `002000.00`**, so ADR-0028's *"parse → serialise must be
byte-identical across the corpus"* is false on the corpus it names. It is true on the canonical
form, and that is what can be tested.

### 4. The house pattern for typed reads is a free function — `[read in repo]`

`codec` already reads typed values as free functions over the value bytes: `as_u32`, `as_i64`,
`as_char` (`crates/codec/src/index.rs:210-276`), all returning the fieldless `ConvertError`
(`NotANumber`, `Overflow`, `NotOneByte`). `MessageView` has no typed getter at all; its public
surface is `get`, `find_from`, `field_at`, `len`, `is_empty`. ADR-0028's `view.decimal(tag)`
would be the only typed method on the view and would have to return `Option<Result<…>>` to keep
*absent* apart from *malformed*.

### Prior art on range and refusal — `[documented]`

- FIX: *"All float fields must accommodate up to fifteen significant digits"*
  ([FIX float](https://fix.dev/kb/data-types/float)). An `i64` holds every 18-digit integer.
- Artio `DecimalFloat`: `long` value bounded at ±999 999 999 999 999 999 (18 nines), scale ≤ 127
  (DecimalFloat.java, above).
- `rust_decimal`: 96-bit mantissa, scale 0..=28; trailing zeros are kept (`1.50` stays scale 2);
  `from_str` rounds digits it cannot hold, `from_str_exact` errors instead
  ([docs.rs](https://docs.rs/rust_decimal/latest/rust_decimal/struct.Decimal.html)). A
  dependency, so out of `codec` regardless (`CLAUDE.md` §6).
- `fixed` (crates.io) is **binary** fixed point and states that 0.001 cannot be represented
  exactly ([docs.rs](https://docs.rs/fixed/latest/fixed/)) — not a candidate for prices.
- QuickFIX/Go `FIXDecimal` embeds `shopspring/decimal` (arbitrary precision, heap-backed) and a
  `Scale` used when writing
  ([fix_decimal.go](https://github.com/quickfixgo/quickfix/blob/main/fix_decimal.go)).
- Chronicle FIX: the search found nothing public on how it represents a price.
- Parse cost: no primary benchmark of a FIX-float-to-integer-pair parse was found. A search
  snippet attributes ~100 ns per string to `rust_decimal`'s original parser; the page
  (`cantortrading.fi`) did not resolve when fetched, so that figure is **someone else's claim**,
  unverified, and is not used as a bound.

## Decision

**1. `Decimal { mantissa: i64, exponent: i8 }`, value = mantissa × 10^exponent.** Private fields,
`const fn new(mantissa, exponent)`, `const fn mantissa()`, `const fn exponent()`. `Copy`, 16 bytes
(asserted at compile time beside `MessageView`'s 24), `#![no_std]`, no `alloc`. The SBE sign
convention, so one number means one thing in both encodings of this workspace. A FIX-parsed value
always has `exponent <= 0`; a positive exponent reaches a `Decimal` only through `new`. This
answers ADR-0028 open question 3: **signed**.

**2. Read by a free function, `as_decimal(value: &[u8]) -> Result<Decimal, ConvertError>`,**
beside `as_i64`, re-exported from the crate root. No method is added to `MessageView`; the call
is `as_decimal(view.get(44)?)`, exactly as `as_i64` is used today. ADR-0028 decision 2's
substance — parsed only when asked, nothing stored per message, the index unchanged — is
untouched.

**3. The grammar is the session's, to the byte.** Accepted: `-? digit* ('.' digit*)?` with at
least one digit; leading zeros; a leading or trailing point. Refused as `NotANumber`: empty, `+`,
`e`/`E`, whitespace, a second point, a lone `-` or `.`, any other byte. Refused as `Overflow`,
never rounded: a mantissa outside ±(2^63 − 1) once leading zeros are dropped (trailing zeros after
the point count — they are the scale), or more than 128 digits after the point. **A syntax
fault wins over an overflow**: `99…9x` is `NotANumber`, never `Overflow`, so the whole value is
classified before its size is. `i64::MIN` is refused on parse so the range is symmetric and never collides with SBE's null. The rule that
binds this to `crates/dict`'s `signed_number` is a test, not shared code:
**`FieldType::Price.accepts(v)` is true exactly when `as_decimal(v)` is `Ok` or `Err(Overflow)`**,
exhaustively over short strings and by generation over long ones. Collapsing the two into one
function is deferred: `dict` is being rebuilt by phase 3 row 2 in parallel, and the session's
validator is not this row's to move.

**4. Written by `Decimal::format(self, out: &mut [u8; Decimal::MAX_LEN]) -> &[u8]`,** infallible,
in canonical form: `-` only for a negative mantissa; the integer part with no leading zero other
than a single `0`; when `exponent < 0`, a `.` and exactly `-exponent` fraction digits; when
`exponent > 0`, that many `0`s and no point. Never `+`, never exponent notation. `MAX_LEN = 147`
(`-`, 19 digits, 127 zeros). The caller passes the returned slice as a `Template` slot value;
`Template` does not change.

**5. The round trip is stated as what holds, and each half is a test.**
(a) For every canonical string `s`, `format(as_decimal(s)) == s`, byte for byte.
(b) For every `Decimal` with `exponent <= 0` and `mantissa != i64::MIN`,
`as_decimal(format(d)) == Ok(d)`.
(c) A non-canonical input — `002000.00`, `23.`, `.5`, `-0.00` — parses to the right mantissa
and exponent and formats to the canonical form (`2000.00`, `23`, `0.5`, `0.00`). **The sign of
a negative zero is not kept.**
An application that must echo a counterparty's bytes verbatim echoes `view.get(tag)`, not a
reformatted `Decimal`; that is a `GUIDE.md` constraint.

**6. Equality is structural.** `PartialEq`, `Eq`, `Hash` are derived, so `1.5 != 1.50`, because
they are different bytes on the wire (ADR-0028 decision 4). No `Ord`, no arithmetic, no numeric
comparison: across exponents each is a rounding decision (ADR-0028 *Bad*, third bullet).

**7. SBE and the `Encoding` trait.** The same type serves both encodings: every SBE decimal
(`int64` or `int32` mantissa, `int8` exponent) is `Decimal::new(m, e)` without loss, and SBE's null
stays SBE's `Option` (`Block::value` already returns `Ok(None)` for it). Wiring an SBE accessor is
**not** part of this decision's build; `crates/sbe` is untouched by phase 3 row 4. **Nothing is
added to `Encoding`**: `Encoding::field` returns bytes whose meaning is per encoding, and a typed
read on the trait would be a fifth shared operation ADR-0079 did not decide — it needs its own ADR
if it is ever wanted.

## Consequences

**Good**

- **One number, one meaning, two encodings.** An SBE price and a FIX price land in the same
  16-byte `Copy` value with nothing lost from either, which a `u8` scale could not promise.
- **The decimal reader and the session cannot drift apart silently.** A value the session
  answers `373=6` for is `NotANumber` here, and the corpus's own `38=+200.00` is the named case.
- **The house pattern holds**: a fourth `as_*` beside three, the same error type, no new
  `MessageView` surface, so `MessageView` stays exactly 24 bytes and its API unchanged.
- **The round-trip claim is now one that can pass.** ADR-0028 promised byte identity on a corpus
  that contains `002000.00`; this states the property that is true and tests each part.

**Bad, and these are the price**

- **A `-0` loses its sign**, and a non-canonical value is not reproduced byte for byte. An
  application that relays prices must relay the bytes, and only `GUIDE.md` can tell it so.
- **Two copies of one grammar**, `dict::signed_number` and `codec::as_decimal`, held together by
  a test rather than by being one function. A test is weaker than a single source; the collapse
  is deferred, and the deferral is recorded here so it is not forgotten.
- **`exponent: i8` caps the fraction at 128 digits.** `0.` followed by 200 zeros is a
  well-formed FIX float the session accepts and this reader refuses as `Overflow` — the same
  deliberate refusal ADR-0028 accepted for more than 18 significant digits, now with a second
  axis.
- **Structural equality surprises.** `as_decimal(b"1.5") != as_decimal(b"1.50")` will be filed as
  a bug by somebody; the rustdoc and `GUIDE.md` say why before they do.
- **ADR-0028 open question 2 stays open**: `as_decimal` does not know whether the tag it is given
  is float-typed. Calling it on `35=` is the caller's mistake and returns `NotANumber`, not a
  compile error.
- **Two new timing cases have no baseline on any machine** until the next `DESIGN.md` §9 boot
  records them, and `scripts/bench.sh --strict` (ADR-0095 decision 4) treats `NO BASELINE` as
  fatal — that boot must record them first.
