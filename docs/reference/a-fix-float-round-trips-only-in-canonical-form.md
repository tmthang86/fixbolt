# A FIX float round-trips only in canonical form

`[measured 2026-09-23]` ADR-0120, plan `docs/plans/2026-09-23-p3-decimal.md` step 5.
`fixbolt_codec::Decimal` and `as_decimal`/`Decimal::format` read and write a FIX float, and three
things a caller can get wrong are not obvious from the types alone.

## The corpus fact, stated precisely

`crates/codec/tests/decimal.rs::every_float_value_in_the_corpus_parses` walks every field the
dictionary types as a float (`FieldType::Float`/`Qty`/`Price`/`PriceOffset`/`Amt`/`Percentage`)
in QuickFIX's own FIX 4.4 `.def` corpus (`vendor/`, fetched by
`scripts/fetch-quickfix-assets.sh`). `[measured]` the corpus holds **25** such values, and
`as_decimal` refuses exactly **one**: `38=+200.00` in `14f_IncorrectDataFormat.def`, which the
`.def` sends expecting a reject (decision 3's leading-`+` refusal).

**`002000.00` is not one of those 25 values.** It is a `PRICE` in the FIXT 1.1 / FIX 5.0 SP2
bench message `crates/codec/benches/alloc.rs` builds under the `fix50sp2` feature (tag `38`,
the `fixt_parse_allocs` case) — a sample chosen for that bench, unrelated to the FIX 4.4 corpus
test above. `tests/decimal.rs` uses the same string directly, twice, as a unit-test input:
`leading_zeros_are_accepted_and_do_not_overflow` (`002000.00` → `(200_000, -2)`) and
`non_canonical_inputs_format_canonically` (`002000.00` → formats as `2000.00`). Three different
places use the same bytes for three different reasons; none of them is "the corpus contains
`002000.00`".

## Three ways the round trip is not what it looks like

1. **A negative zero loses its sign.** `-0` and `-0.00` both parse to a zero mantissa —
   `Decimal`'s `mantissa: i64` has no signed zero, so `negative { -magnitude }` with
   `magnitude == 0` is `0`, not `-0`. `Decimal::format` then writes `0` / `0.00`, never `-0` /
   `-0.00`. Guarded by `negative_zero_parses_to_zero` and, on the write side,
   `non_canonical_inputs_format_canonically`'s `(b"-0.00", b"0.00")` row.

2. **A positive exponent formats to an integer that reads back as a different `Decimal`.**
   `Decimal::new(5, 3)` (only reachable through `new`, e.g. from an SBE mantissa/exponent pair —
   a FIX-parsed value's exponent is never positive) formats as `5000`. Reading `5000` back gives
   `(5000, 0)`, not `(5, 3)`. `format(as_decimal(s)) == s` (ADR-0120 decision 5a) still holds —
   `5000` formats to itself — but `as_decimal(format(d)) == Ok(d)` (decision 5b) is stated only
   for `exponent <= 0`, which is exactly why. Guarded by
   `a_positive_exponent_reads_back_as_its_integer`, which names the reason in its own text.

3. **A zero mantissa with a positive exponent has two rules that conflict, and one was chosen
   without a test.** Decision 4 says both "no leading zero other than a single `0`" and, for
   `exponent > 0`, "that many `0`s and no point" — for `Decimal::new(0, e)` with `e > 0` those
   disagree (`0` vs `000…0`). `Decimal::format`'s rustdoc resolves it explicitly: a zero mantissa
   is always written `0`, whatever the exponent, since the all-zeros form would break the
   leading-zero rule. **No table test in `tests/decimal.rs` exercises `Decimal::new(0, e)` for
   `e > 0`** — the resolution is currently proven only by reading the source and the rustdoc
   beside it, not by a red-then-green reversal.

## Guarded by

- `crates/codec/tests/decimal.rs`: `every_float_value_in_the_corpus_parses`,
  `negative_zero_parses_to_zero`, `non_canonical_inputs_format_canonically`,
  `a_positive_exponent_reads_back_as_its_integer`, `canonical_strings_round_trip_byte_identical`,
  `every_non_positive_exponent_round_trips`.
- `fuzz/fuzz_targets/decimal.rs`: decision 5b over arbitrary bytes, on every value `as_decimal`
  accepts (`exponent <= 0` always holds for those, so it cannot see gap 3 above).
- **Not guarded**: `Decimal::new(0, e)` for `e > 0` — gap 3 is open; a future table-test row is
  the fix, not a code change (the rustdoc already states the intended behaviour).

## Sources

- ADR-0120 decisions 4 and 5, and its *Consequences* — the FIX float grammar and the two-halves
  round-trip statement this page is about.
- ADR-0028 decision 4 (superseded in signature by ADR-0120, substance carried forward) — exact
  scale preserved on the wire is the reason `1.5 != 1.50` and the reason canonical form is not
  always the input form.
