# `codec` — internals

Layer L1 in [DESIGN.md §3](../DESIGN.md#3-crates): parse and serialise FIX in place at the
I/O buffer, the hot path, zero runtime dependencies. `no_std` is a goal, not yet a rule
(CLAUDE.md §6).

## Files, and what each keeps

| File | Keeps |
|---|---|
| `lib.rs` | Crate root; the `no_std` intent and the pointer to `benches/alloc.rs` as the actual proof of non-negotiable 1 |
| `index.rs` | `MessageView`, `FieldIndex<const N>` — the 24-byte, `Copy` index kept apart from the bytes it points into ([ADR-0003](../decisions/ADR-0003-message-representation.md)) |
| `dict.rs` | The `Dictionary` trait: what parsing needs from a FIX dictionary, as a trait rather than a dependency on `dict` |
| `parse.rs` | `parse_into` and the frame reader; the syntax/semantics boundary — this crate rejects only what it cannot read, everything else is passed up to `session` |
| `checksum.rs` | The FIX checksum, a plain byte loop |
| `timestamp.rs` | `SendingTime` formatted once a minute and patched per message, not reformatted from scratch |
| `group.rs` | `GroupIter`, `GroupEntry` — reading repeating groups off the flat index, nested groups included |
| `template.rs` | `Template` — outbound messages as a pre-sorted parts list, patched rather than rebuilt per send |
| `encoding.rs` | `Encoding` — one trait over the wire encodings (ADR-0079), statically dispatched; `TagValue` is its tag=value impl, forwarding unchanged to `parse_into`/`MessageView::get`/`FieldIndex::view`/`Template::encode_with` |

## Read in this order

1. `lib.rs` — what the crate promises and how that promise is checked
2. `index.rs` — the shape everything else operates on
3. `dict.rs` — the trait the parser needs, before reading what uses it
4. `parse.rs` — inbound: bytes to index
5. `checksum.rs`, `timestamp.rs` — the two per-field helpers `parse.rs` and `template.rs` share
6. `group.rs` — reading repeating groups off the index
7. `template.rs` — outbound: index to bytes
8. `encoding.rs` — the trait `TagValue` names over 2–7, read last since it composes them

## Tests that guard it

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
