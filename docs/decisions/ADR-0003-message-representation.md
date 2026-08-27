# ADR-0003 — Separate the field index from the message view

- **Status**: Proposed
- **Date**: 2026-08-27
- **Deciders**: Tran Manh Thang
- **Related**: [reference/measured-costs.md §1](../reference/measured-costs.md)

## Context

The flyweight approach to FIX parsing is settled and correct: scan the wire buffer once,
record `(tag, offset, length)` per field, never copy a value, hand the caller byte ranges
into the original buffer. `hffix` does it, `matthart1983/nanofix` does it, and it is the
reason both are fast.

The question this ADR answers is narrower and easier to get wrong: **where does the field
index live?**

`matthart1983/nanofix` puts it inline in the view it returns:

```rust
pub const MAX_FIELDS: usize = 512;

#[repr(C, align(16))]
pub struct FieldEntry { tag: u32, offset: u32, length: u16, _pad: u16 }

pub struct MessageView<'a> { buffer: &'a [u8], field_count: u16, fields: [FieldEntry; 512], ... }
```

`size_of::<MessageView>()` is **8,224 bytes**, and one is constructed inside `parse()` and
returned by value for every message.

This was measured rather than reasoned about. Full method in
[measured-costs.md §1](../reference/measured-costs.md); the result:

| `MAX_FIELDS` | `size_of` | heartbeat | `NewOrderSingle` | throughput |
|---|---|---|---|---|
| 512 | 8,224 B | 565.0 ns | 605.1 ns | 1.77 M msg/s |
| 64 | 1,056 B | **95.4 ns** | **138.8 ns** | **10.49 M msg/s** |

Changing one integer produced a 4–6× improvement. A real `NewOrderSingle` touches ~20 of the
512 slots, so 96% of the structure is constructed, zeroed and moved without ever being read.

Two separate mistakes are stacked here. The array is too large, **and** it is in the wrong
place — a per-message object rather than a per-connection one.

## Decision

**Split the reusable index from the borrowed view. The parser writes into an index the
caller owns and never constructs a large object.**

```rust
#[repr(C)]                        // 12 bytes, natural alignment 4 — NOT align(16)
pub struct FieldEntry { tag: u32, offset: u32, length: u16, _pad: u16 }

/// Owned once per connection and reused for every message. No lifetime parameter.
pub struct FieldIndex { count: u16, fields: [FieldEntry; MAX_FIELDS] }

/// Two words. Free to copy, free to pass by value.
#[derive(Clone, Copy)]
pub struct MessageView<'a> { buf: &'a [u8], idx: &'a FieldIndex }

pub fn parse_into(buf: &[u8], idx: &mut FieldIndex) -> Result<usize, ParseError>;
```

1. **`MAX_FIELDS = 64`** to begin with. Overflow is `ParseError::TooManyFields` — surfaced,
   never silently truncated. The number is revisited when a real message population is
   available, not by preference.
2. **`#[repr(C)]` with natural alignment, not `align(16)`.** Padding a 12-byte struct to 16
   wastes 25% of every cache line for nothing.
3. **`MessageView` is `Copy` and two words wide**, so passing it across the layers in
   [DESIGN.md §2](../DESIGN.md#2-layers) costs nothing.
4. Field lookup stays a **linear scan** over `count` entries. For 15–30 fields a scan beats a
   map, and it needs no allocation and no hashing.

## Consequences

**Good**

- The measured 4–6× is available by construction rather than by remembering to be careful.
- `FieldIndex` has **no lifetime parameter**, so it can be stored in a connection struct,
  pooled, or reused across reads without fighting the borrow checker.
- `MessageView` being 16 bytes and `Copy` removes any temptation to pass it by reference,
  box it, or clone defensively.
- Overflow becomes an error the caller sees, instead of a message that silently loses its
  tail — which is how a truncating parser produces a wrong `ExecutionReport` and no warning.

**Bad — and these are real**

- **The API is less obvious.** `parse(buf) -> MessageView` reads better than
  `parse_into(buf, &mut idx) -> usize`. Every user has to learn that they own an index. This
  is a genuine ergonomic cost paid for a measured performance gain, and it should be
  documented at the top of the crate rather than buried.
- **`MAX_FIELDS = 64` will reject real messages.** Large repeating groups — a market-data
  snapshot, a mass-quote — exceed it easily. FIX 4.4 session messages and single-order flow
  do not, which is why 64 is right *for the acceptor path this project targets first*. It is
  wrong for a market-data decoder, and that limitation must be stated in the crate docs, not
  discovered.
- **Two lifetimes now interact**: the buffer's and the index's. `MessageView<'a>` requires
  both to outlive it. Users who read into a fresh buffer per message will meet borrow-checker
  errors that the naive design would not have produced.
- **A reused index is a hazard.** A `MessageView` built from an index that has since been
  re-parsed points at the wrong fields. The lifetime system catches this only if the borrow
  of `FieldIndex` is held for the view's life — which the signature above does enforce, but
  it means `parse_into` cannot be called while any view is alive, and some call patterns will
  find that restrictive.

## Alternatives considered

| Alternative | Why rejected |
|---|---|
| Keep the index inline, just shrink `MAX_FIELDS` to 64 | Recovers most of the measured win (1,056 B) but still constructs and moves a 1 KB object per message, and still has no way to reuse it. Half a fix |
| Heap-allocate the index (`Vec<FieldEntry>`) | One allocation per message, or a pool to avoid it — and a pool is exactly what "the caller owns a `FieldIndex`" already is, with more machinery |
| Parse lazily — scan for a tag on each lookup | No index at all, so nothing to size. Attractive for messages read once, but FIX message handling reads many fields, and re-scanning per lookup is O(n·m) |
| `MaybeUninit` for the array, skipping initialisation | Removes the zeroing but not the size or the move, and buys `unsafe` in the most-executed function in the codebase. Rejected under the workspace rule that every `unsafe` block must name what proves it sound |

## Guards

Prose does not hold a constraint. These must exist before this ADR is called enforced:

1. `benches/parse.rs` — asserts the ≤ 150 ns gate for `NewOrderSingle`.
2. A compile-time assertion pinning `size_of::<MessageView>()` to two words.
3. `benches/alloc.rs` — a counting allocator, asserting **zero** allocations across a parse.
4. A test that a message with more than `MAX_FIELDS` fields returns `TooManyFields` and
   **not** a truncated success.
