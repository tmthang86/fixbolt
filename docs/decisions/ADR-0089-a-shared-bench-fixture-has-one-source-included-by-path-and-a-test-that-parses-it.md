# ADR-0089 — A shared bench fixture has one source, included by path, and a test that parses it

- **Status**: Accepted — 2026-09-20. The owner delegated the technical decisions of
  [the-desk-free-residue](../plans/2026-09-20-the-desk-free-residue.md) to the architect on
  2026-09-20. **Nothing here is built**: the module and the two tests named below are *to be
  written* in step 1 of that plan.
- **Date**: 2026-09-20
- **Deciders**: Tran Manh Thang (mandate); written by the architect.
- **Related**: `CLAUDE.md` §2 non-negotiable 1 (the gate that reads the fixture) and 10,
  [ADR-0016](ADR-0016-per-machine-baselines-replace-absolute-targets.md) (why a bench asserts),
  [a-benchmark-parsed-a-message-the-parser-rejects](../reference/a-benchmark-parsed-a-message-the-parser-rejects.md)
  (2026-09-05, the mechanism),
  [a-bench-message-that-fails-its-own-checksum](../reference/a-bench-message-that-fails-its-own-checksum.md)
  (2026-09-19, the correction that reached one file of four), `crates/codec/benches/harness.rs`
  (the by-path precedent), `STATUS.md` item 94.
- **Closes**: `STATUS.md` item 94.

## Context

One hand-written `NewOrderSingle` — `…167=BOO␁10=098␁`, checksum actually 097 — was pasted into
four bench files. Its history is the whole argument:

| Date | What happened | Files reached |
|---|---|---|
| 2026-09-05 | Found in `codec/benches/parse.rs`; corrected to `10=097`; the page said the engine bench "carries the same literal and is unaffected" | 1 of 4 |
| 2026-09-19 | Found again in `codec/benches/alloc.rs` while adding a case; the liveness assert was written to *state* the flaw (`Err(BadCheckSum)`) rather than fix it, because the owner was away and the fixture feeds a §2 gate | 0 of 3 remaining |
| 2026-09-20 | Filed as item 94, the fourth appearance of the family | — |

`codec/benches/alloc.rs` is the machine check for non-negotiable 1. Its `parse` and
`parse via Encoding` cases walk the whole message before failing at `10=`, so the zero they
report is real — and the assertion that guards them currently asserts a **rejection**. A gate
whose liveness assert says "the parser refused this" is a gate that will be read wrong by the
next person, which is what happened twice already.

Two pages of `docs/reference/` say the cheap guard is one line per fixture — *assert the input is
valid before timing anything on it* — and both were right. Neither stopped the recurrence,
because the guard was applied to the file the bug was reported in and the fixture lived in three
more. **Discipline reached one file of four twice.**

## Decision

### 1. One source

The message lives once, in `crates/codec/benches/fixture.rs`, as `pub const NEW_ORDER_SINGLE:
&[u8]` with `10=097`, beside a `pub fn assert_valid()` that parses it with `Validation::ALL` and
requires `Ok(Parsed::Complete { consumed })` with `consumed == NEW_ORDER_SINGLE.len()`.

`codec` has `autobenches = false` and already keeps a non-target module in `benches/`
(`harness.rs`, included by `#[path]` from three benches and from other crates). The fixture
follows that precedent exactly: `#[path = "fixture.rs"] mod fixture;` inside `codec`, and
`#[path = "../../codec/benches/fixture.rs"] mod fixture;` from `engine`. Not a `pub mod` of the
library — a bench fixture is not API, and `codec` has zero runtime dependencies to protect.

### 2. Every bench that uses it asserts it at startup

`codec/benches/parse.rs`, `codec/benches/alloc.rs`, `engine/benches/dispatch.rs` and
`engine/benches/ring_full.rs` drop their literals, take `fixture::NEW_ORDER_SINGLE`, and call
`fixture::assert_valid()` before anything is counted or timed — including the two engine benches
that never parse it, because "this bench does not parse the message" is the sentence that kept
the wrong bytes alive for fifteen days. `alloc.rs`'s liveness assert flips from
`Err(ParseError::BadCheckSum)` to `Ok(Parsed::Complete { .. })`, and its second assert
(`get(55) == Some(b"INTC")`) stays.

### 3. A test guards the fixture, and a test guards the singleness

`crates/codec/tests/bench_fixture.rs`, run by `cargo test --all` on every commit:

- `the_shared_bench_message_parses_clean_under_full_validation` — includes the module by path
  and calls `assert_valid()`. `cargo test` never builds a `harness = false` bench, so without this
  test the fixture would be checked only by the `bench` CI job.
- `no_bench_carries_its_own_copy_of_the_shared_message` — walks `crates/*/benches/*.rs` from
  `CARGO_MANIFEST_DIR/../..` and fails, naming the file, if the byte sequence `167=BOO\x0110=`
  appears anywhere but `codec/benches/fixture.rs`. `[measured 2026-09-20]` that sequence occurs
  in exactly the four files above and nowhere else in `crates/*/benches/`.

### 4. Timing figures are not re-published by this change

`parse.rs` already carried `10=097` since 2026-09-05, so no codec timing case changes its input.
`dispatch.rs` and `ring_full.rs` hand the bytes to `deliver` and never read the checksum digit;
one byte of payload differs and no baseline is touched. `alloc.rs` counts allocations, and a
message that now completes instead of failing at its last field allocates exactly as it did:
zero, or the gate is red. Non-negotiable 10 has nothing to say here and this ADR claims no
number.

## Consequences

**Good**

- The fixture cannot be corrected in one file of four again, because there is one file.
- A fifth appearance is refused by a test that runs on every commit, not by a page in
  `docs/reference/` that asks the next author to grep.
- The non-negotiable 1 gate's liveness assert says what a reader expects it to say: the parse
  completed.

**Bad — and accepted**

- **A `#[path]` include across crates is a build-time coupling cargo does not see.** Moving or
  renaming `codec/benches/fixture.rs` breaks `engine`'s benches with a file-not-found error at
  their next build, not at the rename. `harness.rs` already carries this cost and its doc
  comment names both include spellings; `fixture.rs` does the same.
- **The singleness test is a grep.** It looks for one marker (`167=BOO\x0110=`), so a copy that
  changes the `167=` value or the field order slips past it. It stops the paste, not the
  re-typing; the reference pages still carry the rule for the re-typing.
- **Only this fixture is covered.** `session/benches/validate.rs` and `engine/benches/density.rs`
  carry their own messages with their own input asserts, and nothing generalises "every bench
  fixture is validated at startup" into a machine check. That would need every bench to declare
  its fixtures, which is a larger convention than four files justify today.
- **The fixture is codec-valid, not dictionary-valid.** `52=00000000-00:00:00.000` is not a
  timestamp any dictionary accepts (the 2026-09-05 page). `assert_valid` parses with `NoDict`
  because that is what the four benches do; a bench that one day validates it against `Fix44`
  will need a second fixture or a real timestamp, and the assert will say so on its first run.

## Alternatives rejected

| Alternative | Why not |
|---|---|
| Fix the three literals in place and add the one-line assert to each | The 2026-09-05 fix, done three more times. Four copies stay four copies |
| A `#[doc(hidden)] pub mod fixtures` in `fixbolt_codec` | Bench data in a library's public surface, for a crate that guards its zero-dependency, `no_std`-aspiring boundary |
| A script that greps every bench for `10=` and recomputes checksums | Parsing Rust string escapes in bash for a check a Rust test does in ten lines |
| A `fixbolt-bench-fixtures` crate | A workspace member for one constant; `DESIGN.md` §7 adds crates one at a time behind a plan, and this is not that |
