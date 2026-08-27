# Measured costs

Numbers this project produced by running something and reading the output, and what they
cost the designs that got them wrong. Anything not measured here is marked as somebody
else's claim in [prior-art.md](prior-art.md) and must stay there.

**Machine and date for every measurement below:** Apple MacBook Pro, M5, 32 GB, macOS,
`cargo 1.95.0`, **2026-08-27**. macOS gives no thread pinning and schedules across
super/performance/efficiency cores, so these are *comparative* numbers, good for ranking two
designs against each other on one machine. **They are not a latency SLA and must not be
quoted as one.**

---

## 1. A 512-entry inline field array costs 6× the parse time

### What was measured

`matthart1983/nanofix` at `HEAD` (2026-07-05), release build, `--no-default-features`,
`build.rs` neutralised so it would link without Aeron. A hand-written harness — not the
project's Criterion suite — calling `parser.parse(&msg)` 20,000,000 times per case after a
100,000-iteration warm-up, with `black_box` on input and result.

Two messages, built to be realistic rather than minimal:

- heartbeat, **73 bytes**, 6 fields
- `NewOrderSingle`, **149 bytes**, 15 fields

### The design under test

```rust
pub const MAX_FIELDS: usize = 512;

#[repr(C, align(16))]                       // 12 bytes of data, padded to 16
pub struct FieldEntry { tag: u32, offset: u32, length: u16, _pad: u16 }

pub struct MessageView<'a> {
    buffer: &'a [u8],
    field_count: u16,
    fields: [FieldEntry; MAX_FIELDS],       // 8,192 bytes, inline
    ...
}
```

`size_of::<MessageView>()` = **8,224 bytes**, confirmed by running it. `MessageView::new()`
is called inside `parse()` — `src/parser.rs:136` — so one is constructed per message and
returned by value.

### Result

| `MAX_FIELDS` | `size_of::<MessageView>` | heartbeat | `NewOrderSingle` | throughput |
|---|---|---|---|---|
| **512** (as shipped) | 8,224 B | 565.0 ns | 605.1 ns | 1.77 M msg/s |
| **64** (one constant changed) | 1,056 B | **95.4 ns** | **138.8 ns** | **10.49 M msg/s** |

**5.9× on the heartbeat, 4.4× on the `NewOrderSingle`, from editing a single integer.**

### The diagnostic that identifies this class of bug

Disabling validation changed almost nothing: 565.0 ns validated versus 552.5 ns unchecked on
the heartbeat, a 2% difference. When switching off the work does not change the time, **the
work is not where the time goes.** A fixed per-call cost is dominating, and the only fixed
per-call cost in that function is the struct.

### Why it is so expensive

A real `NewOrderSingle` uses about 20 fields — 320 bytes of the 8,192 available. **96% of the
structure is never read.** It is constructed, zero-initialised and moved on every message,
and at 8 KB it evicts a large fraction of L1 on every parse.

`align(16)` on a 12-byte struct compounds it: 25% of every entry is padding, for no benefit.
Natural alignment 4 packs a third more entries into each cache line.

### What this project does instead

[ADR-0003](../decisions/ADR-0003-message-representation.md): split the reusable index from
the borrowed view, so the parser never constructs a large object.

```rust
pub struct FieldIndex { count: u16, fields: [FieldEntry; MAX_FIELDS] }  // owned once, reused
pub struct MessageView<'a> { buf: &'a [u8], idx: &'a FieldIndex }       // 24 bytes
pub fn parse_into(buf: &[u8], idx: &mut FieldIndex) -> Result<usize, ParseError>;
```

`[measured]` **24 bytes, not 16** — verified with `rustc -O` on 2026-08-27. `&[u8]` is a fat
pointer (16 bytes) plus 8 for the index reference. Over 16 bytes means passed **indirectly**
on x86-64 SysV and AArch64, so hot-path functions taking it by value carry `#[inline]`.

**Guard:** `benches/parse.rs` asserts a regression ceiling (not the published 150 ns — see
[DESIGN.md §6](../DESIGN.md#6-gates) for why the two numbers differ), and
`const _: () = assert!(size_of::<MessageView<64>>() == 24);` pins the size. Without both, this
page is prose, and prose does not hold a constraint.

---

## 2. A benchmark can be 7× off its own stated target and nobody notices

`benches/parse_benchmark.rs` in the same project carries the comment
`// Target: ≤ 80 ns (industry: ~200 ns)` above the heartbeat case. The measured value in
the harness above was **565 ns**.

The target was written as a comment. Nothing read it, nothing failed when it was missed.

**Consequence for this project:** every performance target in
[DESIGN.md §6](../DESIGN.md#6-gates) names a committed benchmark, and the benchmark asserts
the bound. A target that only a human can check is not a gate.

---

## 3. A feature flag that does not gate its module makes a crate unbuildable

`cargo test --no-default-features --lib` on that project fails at link:
`ld: symbol(s) not found for architecture arm64`, on `_aeron_init`, `_aeron_start`,
`_aeron_publication_offer` and others.

Cause: `Cargo.toml` declares `aeron` as a feature, but `src/lib.rs:1` is `mod aeron_c;` with
no `#[cfg]`. The module is always compiled, so libaeron is always required at link time.

`build.rs` compounds it — it panics even under `--no-default-features`, and its fallback
search path list contains `/Users/matt/projects/active/aeron`.

Net effect: **nobody can run that project's test suite without first building Aeron from
source.** Its "238 tests" cannot be executed as shipped. `cargo check` does pass, so the
Rust itself is real code; it is the packaging that is broken.

**Consequence for this project:** [DESIGN.md §4 D5](../DESIGN.md#d5--transport-is-a-trait-tcp-is-the-only-implementation-that-ships-by-default).
A CI job runs `cargo test --no-default-features` on a machine with no optional toolchain
installed, every commit.

---

## 4. Reference points from other engines — not measured here

Repeated from [prior-art.md](prior-art.md) because the comparison is what justifies the
architecture. **Every row is the vendor's or project's own claim.**

| Engine | Claim | Note |
|---|---|---|
| fix8 (C++) | `NewOrderSingle` encode **2.1 µs**, `ExecutionReport` decode **3.2 µs** | Production hardware. 68% faster than QuickFIX |
| fix8 (C++) | **1.4 µs** encode *without framework overhead* | Their own figure. **33% of their latency is framework** |
| QuickFIX (C++) | 6,000–8,000 msg/s per session | Commodity hardware, minimal application |
| QuickFIX `FileStore` | `Sync()` per write, across 3 files | The dominant latency source in the default configuration |

Set against the 138.8 ns measured in §1: the distance between a mature C++ engine and a
flyweight parser is roughly **an order of magnitude**, and fix8's own numbers say where it
goes. That is the entire argument for [DESIGN.md §1](../DESIGN.md#1-the-finding-this-architecture-is-built-around).
