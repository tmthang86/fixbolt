# Why Rust, and what it costs

fixbolt is written in Rust. This page explains what that choice gives this particular engine, with
the file or gate in this repository that shows each benefit in use, and what the choice costs,
with sources. It does not claim that Rust makes a program fast. Layout, allocation and how the
engine waits on the kernel decide this engine's latency ([Why fixbolt](why-fixbolt.md)); the
language decides how cheaply those decisions can be kept true as the code changes.

## Why microseconds matter at all

Aquilina, Budish and O'Neill, *Quantifying the High-Frequency Trading "Arms Race"* (Quarterly
Journal of Economics 137(1), 2022), measured latency races directly from exchange message data.
They report a modal race duration of 5–10 µs, races accounting for about 20 % of trading volume,
and stakes "on the order of $5 billion per year in global equity markets"
([OUP](https://academic.oup.com/qje/article/137/1/493/6368348),
[author's page](https://ericbudish.org/publication/quantifying-the-high-frequency-trading-arms-race/)).

That is why a few microseconds inside a FIX engine can matter to the firm running it. **It is not
an argument for Rust, or for any language.** The paper measures markets; it says nothing about how
the software in them is written.

## What this engine needs from its language

Four things, each tied to a rule in [DESIGN.md §4](../DESIGN.md#4-the-decisions-that-shape-it):

1. No pause the engine did not schedule (D4, D8).
2. A message handed to your code as a view into the receive buffer, without that view being able to
   outlive the buffer or see it overwritten (D2).
3. A way to prove, not assert, that the hot path does not allocate (non-negotiable 1).
4. Rules that hold because a tool refuses to build code that breaks them, not because a reviewer
   noticed (D5, D6).

## What Rust gives this engine

### No garbage collector

There is no collector, so there is no collection pause. That changed one design decision directly:
on a garbage-collected runtime, a common way to protect a protocol engine from pauses in
application code is to put the two in separate threads or processes. fixbolt makes that separation
an option (`RingDispatch`) and runs your handler inline on the engine thread by default
([DESIGN.md §4 D4](../DESIGN.md#d4--dispatch-is-a-trait-inline-is-the-default-the-ring-buffer-is-the-option),
[ADR-0002](../decisions/ADR-0002-engine-library-split.md)).

What remains is the allocator, which can also pause. That is why allocation is kept off the hot
path and counted (below).

### The borrow checker makes a borrowed view safe

`MessageView<'a, N>` borrows two things: the receive buffer and the field index the parser wrote
into ([ADR-0003](../decisions/ADR-0003-message-representation.md)). The lifetime `'a` means the
compiler rejects a handler that keeps the view after the call returns, and rejects code that parses
the next message into the index while a view of the previous one is alive. In a language without
borrow checking, the same design is a pointer and a length into a buffer that will be reused; it is
correct only for as long as every caller remembers the rule.

This is the mechanism that makes "parse in place, copy nothing" something fixbolt can offer as its
public API rather than an internal trick. [GUIDE.md §3](../GUIDE.md#3-messageview-borrows-the-engines-buffer)
is what it means for your handler.

### Generics that compile away

Several choices that would otherwise cost a branch or an indirect call on every message are
resolved by the compiler:

- The dictionary is a trait whose functions take no `self`, so the tables a session uses are known
  at compile time, with no `dyn` and no pointer to a run-time structure
  (`crates/codec/src/dict.rs`).
- The field index size is a const generic, `FieldIndex<N>`, so a large index for market data costs
  an order-flow path nothing.
- The acceptor and initiator are a sealed trait with a compile-time constant, so the role branch is
  resolved at compile time ([DESIGN.md §4 D1](../DESIGN.md#d1--the-session-layer-is-a-pure-state-machine-with-no-io)).
- `Dispatch::OUT_OF_BAND` is a constant, so the engine code that collects replies from another
  thread disappears when dispatch is inline.

C++ templates can do the same; what Rust adds is that the trait bounds are checked where the
generic is written, not where it is used.

### Allocation you can count

Rust lets a binary replace its global allocator with one declaration. Each hot-path crate's
`benches/alloc.rs` installs a counting allocator that forwards to the system allocator and
increments a counter, runs the hot path, and asserts the count is zero
([CONFORMANCE.md §4](../CONFORMANCE.md#4-zero-allocation-on-the-hot-path)). The guard is proven by
breaking it: one added allocation in the counted loop makes the count non-zero and the bench fail.

This is not unique to Rust: a C or C++ program can interpose `malloc` or replace `operator new`.
What Rust makes easy is that the standard library's collections all allocate through that one
global allocator, so the count sees them. Installing it is itself `unsafe` (`GlobalAlloc` is an
unsafe trait), and the bench file says what makes it sound (`crates/codec/benches/alloc.rs`).

### Panics and unsafe are policy the compiler enforces

- **Panics.** The workspace `Cargo.toml` denies clippy's `unwrap_used`, `expect_used`, `panic`
  and `indexing_slicing` lints, so a library crate that could panic on those paths does not pass
  CI. `scripts/check-lint-config.sh` proves the lints bite by reversal
  ([DESIGN.md §4 D6](../DESIGN.md#d6--no-panic-unwrap-or-expect-in-any-library-crate)).
- **`unsafe` is opt-in and can be found by search.** `fixbolt-session` and `fixbolt-sbe` declare
  `#![forbid(unsafe_code)]`; `fixbolt-codec`, `fixbolt-dict` and the `fixbolt` facade contain no
  `unsafe` block. The `unsafe` in `fixbolt-engine` is calls into the C library (`poll`, `pipe`,
  `fcntl`, CPU affinity, `io_uring`), each module behind the feature that needs it
  ([ADR-0019](../decisions/ADR-0019-two-unsafe-blocks-and-an-error-the-enum-can-hold.md)). The ring
  that hands messages to an application thread is safe Rust by decision, at a published price
  ([ADR-0007](../decisions/ADR-0007-spsc-ring-without-unsafe.md)).
- **Errors are values.** The session's errors are enums with no fields, so an error path cannot
  build a string (non-negotiable 2).

### Optional code is really optional

A Cargo feature can remove a module from the build entirely: `#[cfg(feature = "…")]` on the `mod`
declaration. TLS, `io_uring`, CPU pinning and FIX 5.0 SP2 are all gated that way, and CI builds
`--no-default-features` on a machine with nothing optional installed
([DESIGN.md §4 D5](../DESIGN.md#d5--transport-is-a-trait-tcp-is-the-only-implementation-that-ships-by-default)).

## What it costs

The list below takes its general points from two sources the project checked: matklad,
*Why Not Rust* ([2020](https://matklad.github.io/2020/09/20/why-not-rust.html)), and Databento,
*Rust vs C++ for trading systems* ([2025](https://databento.com/blog/rust-vs-cpp)). Each point says
how it shows up here.

- **The language is big, and complexity costs programmer time** (matklad). In fixbolt this is
  concrete: `MessageView` carries two lifetimes and a const generic, the index cannot be reused while
  a view lives, and a handler written for one index size cannot take another without conversion.
  [ADR-0003](../decisions/ADR-0003-message-representation.md) lists these as real costs.
- **Compile times.** matklad: "Rust intentionally picked slow compilers in the generics dilemma."
  This engine leans on exactly that trade (generated tables, const generics, monomorphised
  sessions), so it pays it. No build-time figure has been measured here.
- **`unsafe` has no formal memory model.** matklad: "there's no definition of Rust memory model, so
  it is impossible to formally check if a given unsafe block is valid". fixbolt's answer is not a
  proof. It is confinement: keep `unsafe` out of the protocol crates, gate what remains behind
  features, and require each `unsafe` to name the test, fuzz target or Miri run that argues it sound
  (`CLAUDE.md` §2, non-negotiable 8). That is evidence, not verification.
- **FFI is a bridge, not a seam.** matklad: "integration between the worlds needs explicit bridges.
  These are not seamless." Every system call the standard library does not wrap is `unsafe` code
  here. Databento: "Most exchanges provide native C++ APIs, and critical infrastructure components
  like FPGA interfaces and kernel bypass networking libraries target C++ first." A venue's native
  API, or a kernel-bypass stack, reaches a Rust engine through a C interface someone has to write
  and keep sound. Kernel bypass is not part of fixbolt today
  ([PRD.md §5](../PRD.md#5-permanent-non-goals)).
- **Moves are copies at the machine level.** matklad: "Rust's move semantics is based on values
  (`memcpy` at the machine code level)." A related cost shows up in the calling convention:
  `MessageView` is 24 bytes, and on x86-64 and AArch64 a struct over 16 bytes is passed through
  memory, so hot-path functions that take it by value are marked `#[inline]`, and a compile-time
  assertion in `crates/codec/src/lib.rs` fails the build if the view grows
  ([DESIGN.md §4 D2](../DESIGN.md#d2--the-field-index-is-separate-from-the-message-view)).
- **One compiler, one production backend.** matklad (2020): "There's only one complete
  implementation of Rust — the `rustc` compiler", and it relies on LLVM. fixbolt pins its toolchain,
  so a compiler change arrives as a deliberate upgrade.
- **Ecosystem and hiring.** Databento: "Networking, serialization, and venue SDKs are widely
  available in C++ or via C bindings", and "C++ has been a staple in universities and industry for
  decades, making hiring and onboarding easier." A firm embedding fixbolt adopts a Rust dependency
  and needs people who can read it. As of 2026-08-27 the Rust ecosystem had no production-proven
  FIX acceptor ([reference/prior-art.md](../reference/prior-art.md)), and fixbolt's own production
  track record is zero ([PRD.md §3](../PRD.md#3-where-this-stands-against-quickfix)).
- **Rules the lints cannot see.** The panic lints do not see every panic: a `debug_assert!` and
  `split_at` panic without naming a denied lint
  ([a debug assertion](../reference/a-debug-assertion-is-a-panic-the-lint-cannot-see.md),
  [`split_at`](../reference/split-at-panics-where-no-lint-looks.md)), older code that indexes
  slices is recorded as debt with a ratchet (`scripts/check-indexing-debt.sh`), and Cargo unifies
  features across a workspace, so a feature combination can build together and fail alone
  ([the trap](../reference/feature-flags-unify-across-a-workspace.md)).

## Where the sources are

The research behind this page, including the claims about Rust in trading that circulate without a
source and are not repeated here, is
[reference/prior-art-for-embedders.md §5](../reference/prior-art-for-embedders.md).
