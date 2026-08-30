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

---

## 5. This engine's own numbers

`[measured]` 2026-08-28. **Everything above this section was measured on somebody else's
code**; this section is the first row of the table that is ours.

**Machine and method, because a number without them is not a number** (`CLAUDE.md` §2 rule 10):

| | |
|---|---|
| Machine | Apple M5, macOS |
| Core pinning | **none** — `DESIGN.md` §9 settings are Linux-only and none is in force |
| Build | `cargo bench`, release profile, `nanofix-codec` at `886daa8`'s successor |
| Estimator | **best of 7 runs × 200,000 iterations**, not the mean |
| Harness | `crates/codec/benches/harness.rs`, 24 lines, no dependencies |

**The estimator is optimistic and that matters.** Taking the minimum reports the
least-disturbed run. It suppresses scheduler noise, which is what makes a laptop number
comparable at all, but it is not what a mean would say and it is not what a p99 would say.
Consecutive runs of the same binary moved by ~6% (72.8 → 77.0 ns), which is the honest
precision of this setup.

| Operation | ns/op | Published target | Verdict |
|---|---|---|---|
| Parse `NewOrderSingle`, `Validation::ALL` | **77.0** | ≤ 150 | inside, by 2× |
| Parse `NewOrderSingle`, no frame checks | 74.5 | — | body-length and checksum cost ~2.5 ns |
| Parse `Heartbeat`, `Validation::ALL` | 35.0 | — | |
| Encode `ExecutionReport`, 3 fixed + 14 slots | **93.8** | ≤ 60 | **missed, by 56%** |
| `SendingTime` from `TimestampCache` | 1.8 | — | against 50-100 ns formatted naively (§1) |
| Allocations, parse / encode / lookup / group walk | **0 / 0 / 0 / 0** | 0 | `benches/alloc.rs`, counting allocator |

### Repeating groups, same machine and method

`[measured]` 2026-08-28, `crates/codec/benches/groups.rs`.

| Operation | ns/op | |
|---|---|---|
| Walk one group, 2 entries, 2-tag member list | **29.4** | `386` in a `NewOrderSingle` — the corpus's only populated group |
| Walk 4 nesting levels, 61-tag outer member list | **145.2** | `552 → 78 → 756 → 806` in a `TradeCaptureReport`, FIX 4.4's deepest chain |
| `group_members().contains()`, 61 tags | **5.6** | |
| Encode one group, 2 entries | **35.6** | |

**The 5.6 ns settles an open question and closes it against optimising.** `group_members`
returns the dictionary's *declaration* order, so membership is a linear scan and cannot be a
binary search without a second, sorted table. The repeating-groups plan flagged that as a
risk and refused to act without a number. The longest member list FIX 4.4 has — `(AE, 552)`,
61 tags — costs 5.6 ns to scan. A second table is not bought.

**Walking the deepest message in the dictionary costs about what parsing it costs** (145 ns
against 77 ns for a `NewOrderSingle` parse), and it is only paid when something asks for the
group. A message with no groups pays none of it: `MessageView` does not know groups exist
until `group()` is called.

**The one that misses.** `Template::encode` finds each slot by scanning the caller's list, so
the cost is slots × parts. Fourteen slots is a realistic `ExecutionReport` and it is where the
93.8 ns goes. Not optimised, deliberately: this whole page exists because the reference project
optimised a codec that was 1% of its budget. The number that decides is the Linux one at the
`engine` step, and `DESIGN.md` §8 puts the codec at ~1% of wire-to-wire either way.

**Do not quote any of these as the engine's numbers.** They are a relative reference on one
unpinned laptop. `DESIGN.md` §6's wire-to-wire row is the only one that measures what a
counterparty experiences, and it has not been run.

### Robustness, same day

`[measured]` `cargo +nightly fuzz run parse -- -max_total_time=600`:

```
Done 304230294 runs in 601 second(s)
stat::number_of_executed_units: 304230294
stat::average_exec_per_sec:     506206
stat::new_units_added:          1370
stat::peak_rss_mb:              542
```

Zero crashes, zero timeouts, `fuzz/artifacts/` empty. The target asserts three properties, not
just the absence of a panic: `consumed` never exceeds the input, and every field the index
reports lies inside the consumed prefix. The second is what makes `LengthOutOfBounds`
load-bearing — a DATA length is supplied by the counterparty.


## An injected allocation the optimiser can delete proves nothing

`[cost 2026-08-30]` The counting-allocator benches are proven by *reversal*: put
an allocation on the path, see the number move. The first injection into
`crates/engine/benches/alloc.rs` was

```rust
let _leak = std::vec![0u8; 4];
```

and it reported **0**, exactly as if the guard were working. It was not — the
`Vec` is never read, the bench builds in release, and LLVM deleted the
allocation before it happened.

```rust
let leak = std::vec![0u8; 4];
core::hint::black_box(&leak);   // now it reports 10000
```

Earlier injections in `codec` and `session` survived by luck: they used the
allocated value (`msg.to_vec()` then passed on), so nothing could remove them.

**The rule: an injection must be observed, not assumed, and a reversal that
reports "still zero" is a reversal that did not run.** `CLAUDE.md` §7 already
says a guard is proven by reversal and that the reversal must be confirmed to
have changed something. This is what that sentence costs when it is skipped.
