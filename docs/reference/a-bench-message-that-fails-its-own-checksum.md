# A bench message that fails its own checksum

> `[measured 2026-09-19]` — found while adding the `parse via Encoding` case to
> `crates/codec/benches/alloc.rs`, step A1 of
> [plans/2026-09-19-phase-2-fixt-and-sbe.md](../plans/2026-09-19-phase-2-fixt-and-sbe.md).
>
> **The mechanism is not new and is not restated here.**
> [a-benchmark-parsed-a-message-the-parser-rejects](a-benchmark-parsed-a-message-the-parser-rejects.md)
> recorded it on 2026-09-05: `10=098` against an actual 097, `parse_into` returning
> `Err(BadCheckSum)` from the arm that runs at `tag == 10`, after every field is already in the
> index. What is new is that **the correction reached one file and the literal lived in four.**

## What was still wrong on 2026-09-19

The 2026-09-05 fix corrected `crates/codec/benches/parse.rs` to `10=097` and gave it an
input-validity assert. The same byte literal had been pasted into three other benches; that page
checked one of them by name and did not inventory the rest:

| File | Literal | Does it parse? | Affected |
|---|---|---|---|
| `crates/codec/benches/parse.rs` | `10=097` | yes | fixed 2026-09-05 |
| `crates/codec/benches/alloc.rs` | `10=098` | **yes** | **this page** |
| `crates/engine/benches/dispatch.rs` | `10=098` | no — `deliver` only | no |
| `crates/engine/benches/ring_full.rs` | `10=098` | no — `deliver` only | no (checked 2026-09-19) |

## Why the alloc case still measures what its name says

`benches/alloc.rs` counts allocations, not nanoseconds, and both frame checks live at the end of
`parse_into`: every field is pushed into the index before `10=` is read. The counted loop walks
the whole message and the zero it reports is the zero it claims. What was wrong is that
**nothing said so** — the case discarded the result with `let _ =`, while `CLAUDE.md` §2's
machine-check table already claimed "each case asserting its own path is live".

## What it cost, and the guard that is there now

Writing the new case's liveness assert as `assert_eq!(warm, Parsed::Complete { .. })` — the
obvious guard, and the one the 2026-09-05 page recommends — went **red on the first run** with
`BadCheckSum`. Ten minutes went into deciding whether the fixture was broken or the trait was.

`Complete` is the wrong guard here precisely because it guards the *fixture* rather than the
*path*. The right one asserts the outcome the fixture actually produces, so the flaw is stated
in the source instead of hidden by a `let _ =`, plus one read-back proving the index was filled:

- `crates/codec/benches/alloc.rs:114-124` — `warm_parse == Err(ParseError::BadCheckSum)` and
  `idx.view(msg).get(55) == Some(b"INTC")`, guarding the `parse` case.
- `crates/codec/benches/alloc.rs:143-153` — the `parse via Encoding` case asserts the trait
  returns *the same outcome* as `parse_into` on the same bytes, which is stronger than either.

Both proven by reversal on 2026-09-19: `Validation::NONE` in the warm turns the first red
(`left: Ok(Complete { consumed: 149 })`), an `idx.clear()` before the view turns the second red
(`left: None`).

## The general shape

**A fixture correction is only finished when every copy of the fixture has been visited, and
"copy" means the byte literal, not the file the bug was reported in.** A grep costs seconds; the
2026-09-05 page checked one sibling by name and the other two were never looked at. When a trap
page says a fixture is shared by copy, the fix owes an inventory.

## `[2026-09-20]` The fourth recurrence, closed by a gate rather than a byte

Item 94 came back a fourth time: `alloc.rs` still carried `10=098` when this page's own fix
landed elsewhere, because the 2026-09-05 and 2026-09-19 corrections each reached one file by
hand and the family has four. Three rounds of correcting the literal by hand each closed one
file of four and left the other three exactly as wrong as before.

The fix this time is not a fifth correction. `crates/codec/benches/fixture.rs` is now the one
source — `pub const NEW_ORDER_SINGLE` with `10=097` and `pub fn assert_valid()` — included by
`#[path]` into all four benches (the `harness.rs` precedent; `codec/Cargo.toml` already sets
`autobenches = false`), each calling `assert_valid()` before it times or counts anything, even
the two that never parse the message. **The transferable rule: a fixture shared by copy across
files is closed by removing the copies, not by correcting one.**

Guarded by `crates/codec/tests/bench_fixture.rs`, run on every commit, per CLAUDE.md §4 — every
recorded trap gets a regression test:

- `the_shared_bench_message_parses_clean_under_full_validation` — parses the fixture under
  `Validation::ALL`.
- `no_bench_carries_its_own_copy_of_the_shared_message` — walks every `.rs` file recursively under
  `crates/`, `tools/`, and `benches/` for the marker `167=BOO\x0110=` and fails, naming the file, if it appears anywhere but
  `fixture.rs`.

Both carry an anti-vacuous half the ADR did not ask for: red if the walk found no files under `crates/` or `tools/`, or fewer than 100 in total, red if `fixture.rs` itself lost the marker — so the singleness test cannot pass by
looking at nothing. [ADR-0089](../decisions/ADR-0089-a-shared-bench-fixture-has-one-source-included-by-path-and-a-test-that-parses-it.md).

## Related

- [a-benchmark-parsed-a-message-the-parser-rejects](a-benchmark-parsed-a-message-the-parser-rejects.md)
  — the mechanism, the timing cost, and the 2026-09-05 correction this one completes.
