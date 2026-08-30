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

`[to testing-skills]` — *a target written as a comment is not a gate.* One measured instance,
7× off, unnoticed. Nothing FIX-specific in it.

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

`[to testing-skills]` — *the optimiser deleted the reversal.* `false-greens.md` §5 already has
"a reversal can itself be a no-op" from a search-and-replace that missed; this is the same
shape produced by the compiler instead, which no amount of grepping the diff would catch.

## `nm -u` on an rlib proves nothing about generic code

`[cost 2026-08-30]` Non-negotiable 4 — *the engine thread never sleeps in the
kernel* — has no machine check, and this is the attempt that failed.

`dtruss` is the right tool and macOS System Integrity Protection refuses it
without disabling SIP. The substitute tried was to read the compiled rlib's
undefined symbols: a blocking primitive the linker never has to resolve cannot
be called.

It reported clean. It also reported clean with `std::thread::sleep` added to the
middle of the loop, which is the reversal that should have failed it.

**The reason is monomorphisation.** `Engine` is generic in six parameters and
`serve` is generic in its application type, so neither is code-generated into
the rlib at all — there is nothing for `nm` to see. Adding one concrete type
alias did not help: the *function bodies* are still generic.

Two things follow, and the second is the general one:

* **Non-negotiable 4 is a hand-check** until `tools/w2w` runs on Linux, where a
  syscall trace can be taken. `CLAUDE.md` §2's table of what is machine-checked
  says so; it is not claimed anywhere else.
* **A check over compiled artefacts has to be told which instantiation it is
  checking.** For a generic-heavy crate that is a binary that actually uses it,
  not the library.

`[to testing-skills]` — *two instruments that cannot see what they were pointed at*: a syscall
tracer the OS refuses to run, and a symbol check that passes with the violation present. Both
were deleted rather than shipped, which is the part worth contributing.

## A benchmark that replays one message measures a dropped connection

`[cost 2026-08-30]` The engine's allocation bench had a case called `busy`: a
Logon sent into `Loopback` a thousand times, one `Engine::turn` per send. It
reported **1 allocation per 1000 iterations** — close enough to zero to look
like a rounding artefact, and stable across consecutive runs, which made it look
real.

It was measuring nothing. The session refuses the second Logon as a sequence
number already used and drops the link, so from iteration three on the engine
held **no connections** and the loop was `send into a queue nobody reads`. The
one allocation was that `VecDeque` reaching a doubling boundary. Two hours went
into looking for it in `Engine::turn`, which never ran.

The fix is not a bigger warm-up. It is an **assertion that the path is still
alive at the end of the count**:

```rust
assert_eq!(engine.connections(), 1, "not dropped at message two");
```

and traffic with increasing sequence numbers, rendered before the count starts
so the harness's own `format!` is not charged to the engine.

`[to testing-skills]` — *the benchmark measured a torn-down system.* Sibling of "the vacuous
wait": an assertion whose expected value is *nothing* passed because the thing under test was
no longer there. Contribute the fix — every case asserts its own path is live.

**Generalised:** a zero from a counting allocator means *did not allocate* only
if something separately proves *did run*. Every case in these benches now
asserts its own path — `the send path sends`, `the framing path must actually
cut a message`, `must still hold a live session`. Injection proves the counter
sees the path; the assertion proves the path was taken.

## `received_with` judges `SendingTime` against the last `tick`, not the wall

`[cost 2026-08-30]` Found while fixing the above. `Session::received_with` takes
no clock — by design, D1: the session layer has none, and time arrives only as
`Input::Tick`. So the 120-second `SendingTime` skew check compares against the
last instant a `tick` supplied, and **a session that has never ticked holds
zero**.

A Logon is then 2026 years of skew and is refused silently. The first engine in
the bench accepted the identical Logon only because an earlier case had already
ticked it ten thousand times.

It bit three times — an allocation bench, a dispatch test and a backpressure
test — before it was fixed rather than worked around. **The fix is the order
inside `Connection::turn`: tick first, then read.** A session then always has a
time before it judges anything, and the hole closes for every caller at once
rather than for whichever one last tripped over it.

`[measured 2026-08-30]` moving the tick left the wire gate at 59 / 59, so the
corpus does not care which side of the read it falls on. The three workarounds
were deleted in the same commit — **a workaround left in place after the cause
is fixed is a comment that will be believed later.**

## What a thread hop costs when the ring may not use `unsafe`

`[measured 2026-08-30]` Apple M5, macOS 25.6, unpinned, best-of-7 × 200 000 iterations,
`crates/engine/benches/dispatch.rs`. A 163-byte `NewOrderSingle` — the same message every
other benchmark here is measured on.

| Path | ns/op |
|---|---|
| `InlineDispatch::deliver` + reply | **2.7** |
| Ring, one way (engine → application) | **128.0** |
| Ring, round trip (→ handler → back) | **242.5** |

**The hop is ~50× the inline call**, and the one-way figure is ~0.8 ns per byte, which is
almost exactly the cost of copying with `AtomicU8` loads and stores instead of `memcpy`. The
reason it is built that way, and what would reverse it, is
[ADR-0007](../decisions/ADR-0007-spsc-ring-without-unsafe.md).

Two things this number is *not*:

- **Not the cost of the option.** An application that chose the ring did so because it may
  stall for milliseconds. 240 ns against 40 ms is not the trade it is making.
- **Not a Linux number.** Nothing here was measured on the machine `DESIGN.md` §9 describes.

The inline figure moved between 2.5 and 4.9 ns across runs of the same binary. At that size
the loop is a handful of instructions and the harness's own overhead is the same order, so the
ceiling is set at 15 ns rather than at 2×. **A ceiling tighter than the measurement's own
spread is a gate that goes red at random**, and DESIGN §6 already says what happens to those.

## A settle criterion that counts spins measures the machine, not the protocol

`[measured 2026-08-30]` Linux 6.18 x86_64, 4 vCPU container, `cargo 1.94.1` — **not** the
M5 laptop every other number on this page came from.

`crates/engine/tests/wire.rs` is the gate that matters: the same 59 acceptance definitions
that `crates/session/tests/score.rs` runs in process, run again through kernel TCP. It was
recorded here and in `README.md` as **59 / 59**. On the Linux box it scores **39 / 59**,
first run, with the working tree unchanged.

The engine is not what is wrong. Three runs, changing one constant and nothing else — the
`quiet` bound in `Wire::pump`, the number of consecutive `Engine::turn` calls that moved
nothing before the harness declares the exchange settled:

| `quiet` bound | Score | Wall time |
|---|---|---|
| 200 — as committed | 39 / 59 | 0.7 s |
| 2 000 | 43 / 59 | 4.3 s |
| 20 000 | **59 / 59** | 41.3 s |

**A score that climbs monotonically with a timeout is a timing artefact**, and the reported
diffs say the same thing in the other direction: `FieldCount { expected: 9, actual: 8 }` at
one line and `expected: 8, actual: 9` four lines later is one reply arriving after the step
that was supposed to read it, shifting every comparison behind it. `--test score` scores
59 / 59 on this same machine, so the session's answers are right and only their arrival time
is being measured.

**The defect is that a spin count is not a settle criterion.** `pump` races loopback
delivery: it asks "have I turned 200 times with nothing to do", which is a question about how
fast this CPU spins, when the question it means to ask is "has the kernel finished handing
over what the engine wrote". Those coincide only on the machine the constant was tuned on.
The doc comment on `pump` is explicit that idling once is not settled and that the bound
exists to avoid hanging — the bound was right to exist and wrong to be a count.

Two consequences worth keeping separate:

- **The 59 / 59 over TCP is a single-machine result.** It reproduces on the M5 and does not
  reproduce here. Until the criterion is deterministic the wire gate cannot go in CI, and
  `tools/w2w` — which exists to produce Linux numbers — would be built on top of a gate that
  Linux is currently failing. Open item 17.
- **The in-process gate is unaffected**, because it never touches a socket. That is the
  reason it is worth keeping both, and it is what makes the diagnosis above cheap: two gates
  over the same corpus disagreeing localises the fault to the thing that differs.

**Generalised, and the reason this is written down rather than fixed in passing:** a check
whose green depends on a duration is not a check, it is a coin whose weighting is the
hardware. It reads as a gate, it goes in a status page as a number, and it is discovered by
running it somewhere else — never by reading it. The same shape has a name and a collection
of siblings in the sibling repository's `false-greens.md`; `CLAUDE.md` §11 says what goes
back there.

`[to testing-skills]` — as a false-green case: *a gate whose green is a duration*. Contribute
the three-row table, the monotonic-score diagnostic, and the rule that a settle criterion must
name the event it waits for. It needs no FIX to be understood.

## CI had been red for both of these before either was noticed

`[measured 2026-08-30]` The section above was written as though running the suite on Linux
was what found the wire gate. It was not the first thing to find it. **GitHub Actions had
already been failing on the same assertion, on `main`, since the engine merged** — run
`33291318638`, commit `9986890`, job *Builds with nothing optional installed*:

```
failures:
    the_fifty_nine_definitions_pass_through_a_real_socket
test result: FAILED. 0 passed; 1 failed
```

The merge commit's own message reports the gates green on an Apple M5 with `cargo 1.95.0`,
and that report is true. CI disagreed with it within a minute and **nothing read the
disagreement**. `CLAUDE.md` §10 already names this in its own words — *a check proves nothing
until something reads it* — and here it cost the repository a status page, a `README` blurb,
a `DESIGN.md` §6 row and a `PRD.md` exit criterion that all said 59 / 59 while the machine
that runs on every push said otherwise. **The gate was not missing and was not wrong. It was
unread.**

The same run was red for a second, independent reason, and that one has a different cause:

```
error: can be more succinctly written as a byte str
   --> crates/dict/tests/interop_quickfix_fields.rs:133:16
133 |         .chain([b'*', b'?', b'!'])
    |                ^^^^^^^^^^^^^^^^^^ help: try: `*b"*?!"`
    = note: `-D clippy::byte-char-slices` implied by `-D warnings`
```

`clippy::byte_char_slices` does not exist in the toolchains this repository is developed on —
`clippy 0.1.94` here and `1.95.0` on the M5 both pass that file. The runner's help URL says
`rust-1.98.0`. **CI installs whatever stable is on the day it runs, and there is no
`rust-toolchain.toml`**, so `-D warnings` means *deny every lint any future clippy invents*.
A repository can go red with no commit, and the person who reads it first will be looking for
what they broke.

Two rules, and neither is about FIX:

- **A gate on a machine you do not sit at has to report to somewhere you look.** A red run
  that only exists in a tab is the same as no run. The cheapest fix is that a plan cannot
  close on a laptop's word: the closing evidence names the CI run.
- **`-D warnings` with an unpinned toolchain is a scheduled outage.** Either pin the
  toolchain and upgrade deliberately, or deny a named list rather than the category.

`[to testing-skills]` — two cases. *A red check nobody read*, which is the mirror of
`false-greens.md` §7 "the report that only speaks when it fails": here it spoke and there was
no one on the channel. And *`-D warnings` against a rolling toolchain*, where the failing
build is caused by a release rather than by a change. The second is not a false green at all
and may belong in a section of its own upstream.
