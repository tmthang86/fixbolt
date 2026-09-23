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

**Guarded by**: nothing machine-checks that this specific call stays `split_at_checked` — it is
read by eye per `CLAUDE.md` §2 rule 7's "hand check" clause. `crates/codec/tests/decimal.rs`'s
`a_fraction_of_128_digits_parses` and `the_longest_output_is_max_len` exercise the boundary
`mid` values (`whole == 0`, `whole == len`) that would have panicked under `split_at`.
