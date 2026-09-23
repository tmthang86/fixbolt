# `codec` — internals

Layer L1 in [DESIGN.md §3](../DESIGN.md#3-crates): parse and serialise FIX in place at the
I/O buffer, the hot path, zero runtime dependencies. `no_std` is a goal, not yet a rule
(CLAUDE.md §6).

## Files, and what each keeps

| File | Keeps |
|---|---|
| `lib.rs` | Crate root; the `no_std` intent and the pointer to `benches/alloc.rs` as the actual proof of non-negotiable 1 |
| `index.rs` | `MessageView`, `FieldIndex<const N>` — the 24-byte, `Copy` index kept apart from the bytes it points into ([ADR-0003](../decisions/ADR-0003-message-representation.md)) |
| `decimal.rs` | `Decimal { mantissa: i64, exponent: i8 }` (16 bytes, `Copy`) and the free function `as_decimal`, reading a FIX float from a field's bytes on demand, beside `as_i64`; `Decimal::format` writes the canonical form back ([ADR-0120](../decisions/ADR-0120-a-decimal-is-a-mantissa-and-a-signed-exponent-read-by-a-free-function-and-round-trips-only-in-canonical-form.md)) |
| `dict.rs` | The `Dictionary` trait: what parsing needs from a FIX dictionary, as a trait rather than a dependency on `dict` |
| `parse.rs` | `parse_into` and the frame reader; the syntax/semantics boundary — this crate rejects only what it cannot read, everything else is passed up to `session` |
| `checksum.rs` | The FIX checksum, a plain byte loop |
| `timestamp.rs` | `SendingTime` formatted once a minute and patched per message, not reformatted from scratch |
| `group.rs` | `GroupIter`, `GroupEntry` — reading repeating groups off the flat index, nested groups included |
| `template.rs` | `Template` — outbound messages as a pre-sorted parts list, patched rather than rebuilt per send |
| `encoding.rs` | `Encoding` — one trait over the wire encodings (ADR-0079), statically dispatched; `TagValue` is its tag=value impl, forwarding unchanged to `parse_into`/`MessageView::get`/`FieldIndex::view`/`Template::encode_with` |
| `benches/fixture.rs` | Not a bench target (`Cargo.toml`'s `autobenches = false`) — the one source of the shared `NewOrderSingle` bench message (`NEW_ORDER_SINGLE`, `10=097`) and `assert_valid()`, included by `#[path]` into `benches/alloc.rs`, `benches/parse.rs`, `engine/benches/dispatch.rs` and `engine/benches/ring_full.rs`, the same precedent `harness.rs` set ([ADR-0089](../decisions/ADR-0089-a-shared-bench-fixture-has-one-source-included-by-path-and-a-test-that-parses-it.md)) |

## Read in this order

1. `lib.rs` — what the crate promises and how that promise is checked
2. `index.rs` — the shape everything else operates on
3. `decimal.rs` — a typed value read from the index the same way `as_i64` is, before the parser
   that fills the index
4. `dict.rs` — the trait the parser needs, before reading what uses it
5. `parse.rs` — inbound: bytes to index
6. `checksum.rs`, `timestamp.rs` — the two per-field helpers `parse.rs` and `template.rs` share
7. `group.rs` — reading repeating groups off the index
8. `template.rs` — outbound: index to bytes
9. `encoding.rs` — the trait `TagValue` names over 2–8, read last since it composes them

## Tests that guard it

- `tests/decimal.rs` (30 tests) — `as_decimal` grammar and overflow, `Decimal::format`'s
  canonical form, and the two round-trip halves of ADR-0120 decision 5; `benches/decimal.rs` —
  two timing cases, `NO BASELINE` until plan step 7 records one on the §9 machine; the alloc
  case `decimal` in `benches/alloc.rs` (below) is the non-negotiable-1 proof for this path
- `tests/parse_basics.rs`, `tests/defs.rs`, `tests/stream.rs` — inbound parsing, including
  incomplete frames
- `tests/groups.rs`, `tests/group_roundtrip.rs` — repeating groups, nested included
- `tests/data_fields.rs`, `tests/data_encode.rs` — `DATA`-typed fields and their lengths
- `tests/slot_order.rs`, `tests/roundtrip.rs`, `tests/timestamp.rs` — template field order and
  timestamp patching
- `tests/encoding.rs` — `Encoding`/`TagValue`; `benches/alloc.rs` also counts a `parse via
  Encoding` case, which must read 0 same as the direct path
- `tests/bench_baselines.rs`, `tests/bench_verdict.rs` — the Criterion suite's own sanity
- `benches/alloc.rs` — the counting allocator proving non-negotiable 1 (CLAUDE.md §2)
- `tests/bench_fixture.rs` — guards `benches/fixture.rs`, run on every commit (not only in the
  `bench` CI job): `the_shared_bench_message_parses_clean_under_full_validation` parses the
  fixture under `Validation::ALL`; `no_bench_carries_its_own_copy_of_the_shared_message` walks
  every `.rs` file recursively under `crates/`, `tools/`, and `benches/` for the `167=BOO\x0110=` marker and names any bench that kept its
  own copy — the fourth recurrence of
  [a-bench-message-that-fails-its-own-checksum](../reference/a-bench-message-that-fails-its-own-checksum.md)
  is what this closes
