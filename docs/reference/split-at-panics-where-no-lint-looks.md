# `split_at` panics where no lint looks

`[found 2026-09-23]` `Decimal::format`, `crates/codec/src/decimal.rs`, plan
`docs/plans/2026-09-23-p3-decimal.md` step 2. **`[to testing-skills]`**

`[u8]::split_at(mid)` panics when `mid > self.len()`. The first draft of the fraction-digit split
computed `mid` from a subtraction whose result was not yet proven in range, and reached for
`split_at` because it reads as plain, safe-looking slice code — no `unwrap`, no `expect`, no
`panic!`, no `a[i]`.

None of the three lints this workspace denies for non-negotiable 7 sees it:

- `clippy::unwrap_used`, `clippy::expect_used`, `clippy::panic` match the **named call**, and
  `split_at` is none of those names.
- `clippy::indexing_slicing` (added after exactly this shape of panic —
  [an-allow-at-the-top-of-a-file-silenced-the-whole-crate](an-allow-at-the-top-of-a-file-silenced-the-whole-crate.md))
  matches the **`a[i]` / `a[i..j]` syntax**, and a method call is neither.

`scripts/check-indexing-debt.sh` is a ratchet over the same lint, so it is blind to `split_at` in
exactly the same way. The general shape this repeats — a deny-list stops the spelling it was
written against and nothing that means the same thing —
[is written up here](a-debug-assertion-is-a-panic-the-lint-cannot-see.md).

**The fix.** `[u8]::split_at_checked(mid)` (stable since Rust 1.80) returns `Option<(&[u8],
&[u8])>` instead of panicking, and `Decimal::format` treats `None` as "the digits do not reach
past the point" — a real case (`.5` has more fraction digits than total digits), not an error
path. No lint enforces choosing the checked form over the panicking one; only the workspace's
`clippy::indexing_slicing = "deny"` label at the crate root and this page make the reason
explicit for the next `split_at`-shaped call.

**Guarded by**: no machine check keeps this call as `split_at_checked`. It is read by eye, as
`CLAUDE.md` §2 rule 7's hand-check clause says. What the tests prove is that every split
`format` can make gives the right bytes. In `crates/codec/tests/decimal.rs`,
`format_places_the_point_at_every_position` formats `±i64::MAX` at every fraction width from 1
to 128 and compares each result with a string built by hand. That covers every integer-digit
count `whole` from 18 down to 1, then `whole == 0` (`len == frac`), then more fraction digits
than there are digits (`frac > len`, out to 128). `whole == len` cannot happen: a negative
exponent always leaves at least one digit after the point, so the largest `whole` is `len - 1`.
Proven by reversal: with the filter changed to `whole > 1`, the test fails on
`(9223372036854775807, -18)`, with `left: "0.9223372036854775807"`.
