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

`[to testing-skills → [PR #2](https://github.com/tmthang86/testing-skills/pull/2), open]` — *a target written as a comment is not a gate.* One measured instance,
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
| Build | `cargo bench`, release profile, `fixbolt-codec` at `886daa8`'s successor |
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

`[to testing-skills → [PR #2](https://github.com/tmthang86/testing-skills/pull/2), open]` — *the optimiser deleted the reversal.* `false-greens.md` §5 already has
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

`[to testing-skills → [PR #2](https://github.com/tmthang86/testing-skills/pull/2), open]` — *two instruments that cannot see what they were pointed at*: a syscall
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

`[to testing-skills → [PR #2](https://github.com/tmthang86/testing-skills/pull/2), open]` — *the benchmark measured a torn-down system.* Sibling of "the vacuous
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

## The score followed the timeout, and the timeout was not the cause

`[measured 2026-08-30]` Linux 6.18 x86_64, 4 vCPU container, `cargo 1.98.0`.

`crates/engine/tests/wire.rs` runs the 59 acceptance definitions through kernel TCP. It was
recorded as **59 / 59**. On this machine it scored **39 / 59**, first run, working tree
unchanged. Changing one constant and nothing else — the `quiet` bound in `Wire::pump`, the
number of consecutive `Engine::turn` calls that moved nothing before the harness declared the
exchange settled — walked the score:

| `quiet` bound | Score |
|---|---|
| 200 — as committed | 39 / 59 |
| 2 000 | 43 / 59 |
| 20 000 | 59 / 59 |

**That table is real and its obvious reading is wrong.** A score that climbs with a timeout
does say the harness is waiting for something. It does *not* say the timeout is the defect,
and the first write-up here concluded that it did: *"a spin count is not a settle criterion"*.
That went into `STATUS.md`, `README.md`, `DESIGN.md` §6, `PRD.md` and a pull request before it
was checked.

### What it actually was

**Nagle's algorithm, on the test harness's own client socket.**

`2m_BodyLengthValueNotCorrect.def` is the one file that fails, and its own comment says why it
is unusual: *"Send a message with a length that is too long, it will combine with the next
message and be ignored."* An over-long `9=` produces **no reply** — an incomplete frame has
nothing to answer. No outbound segment therefore carries a piggybacked ACK, the peer's delayed
ACK holds for tens of milliseconds, and Nagle keeps every subsequent small write queued behind
the unacknowledged one. Four `I` lines then arrive as **one 477-byte read**, and the framer
discards all four — the correct answer to a question the corpus never asked.

Traced, rather than reasoned:

```
DBG recv 120        DBG cut=Need len=120     <- the over-long frame, correctly held
   write ok                                  <- the next I line, into the kernel
DBG recv idle       DBG cut=Need len=120     <- and again, and again, for milliseconds
   write ok            write ok
DBG cut=Garbage(477)                         <- all four, at once
```

The engine already sets `TCP_NODELAY` on the sockets it accepts (`transport.rs:68`). The
harness did not set it on the client, which made the test rig the only Nagle-enabled peer in
the exchange. **One line fixes it**, and the longer timeouts were merely outwaiting the
delayed ACK.

### The 2 × 2 that settles it

| | Nagle on (as committed) | `set_nodelay(true)` |
|---|---|---|
| Spin count, 200 | **39 / 59** | **59 / 59** |
| Wall-clock bound | **39 / 59** | **59 / 59** |

The spin count moves nothing in either direction. `set_nodelay` moves everything in both.
Removing that one line from the finished fix returns the score to exactly **39 / 59** — the
original number, which is what makes this a reversal and not a story.

### What was kept anyway, and labelled

`Wire::pump` now bounds itself in wall time rather than in turns, and **the gate scores
59 / 59 at both 1 ms and 20 ms** — only the run time moves, 0.8 s against 14.5 s. That
flatness is the whole justification: a bound in turns is a bound on a machine. But nothing
measured here shows it mattering, and the code comment says so rather than implying otherwise.
An earlier draft of this fix also added a `settle` hook to `fixbolt_conformance`'s public
trait; **the reversal that was supposed to prove it showed the gate stayed at 59 / 59 with it
disabled**, so it was deleted rather than shipped. Machinery that cannot be shown to matter is
machinery that will be believed later.

### What this cost, and the rule that comes out of it

Two things, and the second is the expensive one.

- **A number that moves with a knob invites the conclusion that the knob is the cause.** It is
  evidence that something is being waited on, and nothing more. The next question is *what*,
  and the way to answer it is a trace, not a third value of the knob.
- **A wrong diagnosis published confidently is worse than an open question.** This one reached
  five documents and a pull request in the same hour it was formed, each restating it as
  settled. What it lacked was a single-variable experiment — the 2 × 2 above took one run per
  cell and would have refuted it before any of that was written.

`[to testing-skills → [PR #2](https://github.com/tmthang86/testing-skills/pull/2), open]` — two cases, both legible without FIX. *The knob that correlates with
the fix and is not the cause* — a monotonic response to a timeout, a plausible mechanism, and
a completely different real cause. And *the test rig as the only misconfigured peer*: the
system under test had `TCP_NODELAY` set and the harness did not, so the harness measured a
network condition the product never has.

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

`[to testing-skills → [PR #2](https://github.com/tmthang86/testing-skills/pull/2), open]` — two cases. *A red check nobody read*, which is the mirror of
`false-greens.md` §7 "the report that only speaks when it fails": here it spoke and there was
no one on the channel. And *`-D warnings` against a rolling toolchain*, where the failing
build is caused by a release rather than by a change. The second is not a false green at all
and may belong in a section of its own upstream.

## A ceiling tuned on one machine, red on another, and not in CI so nobody saw it

`[measured 2026-08-30]` `crates/engine/benches/dispatch.rs` **fails on Linux**:

```
inline deliver + reply                  5.4 ns/op   ceiling 15
ring, one way                         332.5 ns/op   ceiling 260
panicked: ring, one way: 332.5 ns/op exceeds the 260 ns regression ceiling
```

The ceilings were set from the M5 figures published above — inline 2.7 ns, ring one way
128.0 ns — with roughly 2× of headroom. On a shared 4 vCPU container the inline case is
comfortably inside its ceiling and **the ring hop is 2.6× the M5's**, which puts it outside.

**Nothing is regressing.** A cross-thread hand-off over two atomics is dominated by
inter-core latency, and inter-core latency on a virtualised shared host is not the M5's. The
gate is measuring the machine, which `DESIGN.md` §6 already says these numbers do — *they rank
designs against each other on one machine, they are not an SLA*. What it does not say is what
happens when the gate itself is then asserted on a different one.

**Three things went wrong at once, and the third is the one that matters.**

1. `CLAUDE.md` §7 says a hot-path change runs the Criterion suite **and** `benches/alloc.rs`.
   Working on the ring, `alloc` was run and `dispatch` was not — the two are named together in
   the table and only one was read.
2. The commit that closed that work stated its gates and did not include this one, because it
   had not been run. That is the `§9` box's failure mode inside a single commit rather than
   across a merge.
3. **The bench is not in CI.** `cargo test --all` does not run a `harness = false` bench, and no
   job runs `cargo bench`. So a ceiling that has been red on every Linux machine since it was
   written has never once been reported by anything. It was found by running it by hand while
   doing something else.

The third is the general shape and it is not about benchmarks: **an assertion that no automated
thing executes is a comment.** This repository already has that written down — *a check proves
nothing until something reads it* — and had already paid for it once, when CI was red on `main`
for a day. This is the same defect one layer down: not a check nobody read, but a check nobody
ran.

**Not fixed here, deliberately.** Raising the ceiling makes it stop catching a real regression
on the machine it was tuned for; deleting it throws away a real guard; making it relative to a
per-machine baseline is a design change to how a `DESIGN.md` §6 gate is measured, which the §4
sync table says needs its own plan. `STATUS.md` open item 20.

`[to testing-skills → [PR #2](https://github.com/tmthang86/testing-skills/pull/2), open]` — two cases. *A threshold calibrated on one machine and asserted on
another*: the number is honest, the comparison is not, and the failure looks exactly like a
regression. And *the assertion nothing runs* — a guard outside the command CI actually invokes,
which is a false green that never even had to lie, because nobody asked it.

## One run is not a measurement: the ceiling that was "red on Linux" flips both ways

The section above recorded, on one run, that `benches/dispatch.rs` was **red on Linux — ring
one way 332.5 ns against a 260 ns ceiling**. That went into `STATUS.md` as open item 20 and
into a pull-request description as a property of the machine.

`[measured 2026-08-30]` Five runs of every timing case, same container — Linux 6.18.44
x86_64, Intel Xeon 2.10GHz, 4 vCPU shared, `rustc 1.98.0`, `cargo bench` release profile,
each figure already a best-of-7 over 200 000 iterations:

| Case | Ceiling | min | max | Spread | Over the ceiling |
|---|---|---|---|---|---|
| `parse NewOrderSingle (validated)` | 150 | 102.0 | 107.5 | 5% | 0 / 5 |
| `parse NewOrderSingle (no checks)` | 145 | 97.3 | 102.0 | 5% | 0 / 5 |
| `parse Heartbeat (validated)` | 70 | 52.4 | 54.4 | 4% | 0 / 5 |
| `encode ExecutionReport (template)` | 190 | 177.6 | 199.4 | 12% | **2 / 5** |
| `SendingTime from the cache` | 5 | 3.4 | 3.7 | 9% | 0 / 5 |
| `walk 1 group, 2 entries` | 60 | 50.8 | 56.8 | 12% | 0 / 5 |
| `walk 4 levels, 61-tag member list` | 300 | 285.0 | 314.8 | 10% | **3 / 5** |
| `group_members contains, 61 tags` | 12 | 8.9 | 10.1 | 13% | 0 / 5 |
| `encode 1 group, 2 entries` | 75 | 72.8 | 88.5 | 22% | **4 / 5** |
| `inline deliver + reply` | 15 | 3.4 | 11.3 | **232%** | 0 / 5 |
| `ring, one way` | 260 | 188.5 | 233.2 | 24% | 0 / 5 |
| `ring, round trip` | 500 | 339.4 | 447.3 | 32% | 0 / 5 |

**On this container `ring, one way` never exceeded its ceiling in five runs**, and `parse`
moved between 102.0 and 136.3 ns across sessions — the whole machine gets slower and faster
over minutes, and every case moves together. Three cases flip colour between runs; not one is
over in all five.

### Then the same benchmarks ran on a second shared machine and disagreed

`[measured 2026-08-30]` CI run 33304774414, GitHub Actions `ubuntu-latest`: **AMD EPYC 7763,
2 cores**, Linux 6.17.0-1022-azure, same commit, same `rustc 1.98.0`.

| Case | Ceiling | 4 vCPU Xeon, 5 runs | 2-core EPYC, CI |
|---|---|---|---|
| `parse NewOrderSingle (validated)` | 150 | 102.0–107.5 | 127.6 |
| `walk 1 group, 2 entries` | 60 | 50.8–56.8 | **62.9** |
| `walk 4 levels` | 300 | 285.0–314.8 | **319.7** |
| `encode 1 group, 2 entries` | 75 | 72.8–88.5 | **101.4** |
| `encode ExecutionReport (template)` | 190 | 177.6–199.4 | **261.0** |
| `inline deliver + reply` | 15 | 3.4–11.3 | 6.4 |
| `ring, one way` | 260 | 188.5–233.2 | **328.3** |
| `ring, round trip` | 500 | 339.4–447.3 | **622.9** |

**Six of twelve cases are over the ceiling on the CI runner and zero were on the container.**
And `ring, one way` came in at 328.3 ns — within 1.3% of the 332.5 ns that originally named
open item 20. So the first correction written here was itself half wrong: the 332.5 ns was not
only a noisy moment, it is close to what that *class* of machine actually does.

Both statements are needed and neither alone is true:

* **Run to run on one machine**, the spread is 5–232% and three cases change colour.
* **Machine to machine**, the same case differs by up to **1.7×** (`ring, one way`: 188.5 to
  328.3), and the ceiling sits between the two.

The `ring` figures have a visible cause rather than a mysterious one: the benchmark moves a
message between two threads, and the runner has exactly two cores — the worst case for a
cross-thread hop, with nothing left over for anything else on the box.

### And the noise itself is a property of the machine

A second CI run on the next commit (`bf7fe48`, run 33304926978) repeated the figures on the
same runner class:

| Case | Run 1 | Run 2 | Difference |
|---|---|---|---|
| `ring, one way` | 328.3 | 331.1 | **0.9%** |
| `ring, round trip` | 622.9 | 623.5 | **0.1%** |
| `ring_full`, ns per message | 194 | 195 | 0.5% |

A third run (`db5d8b1`, run 33304998832) held: `ring, one way` 327.2, `ring, round trip` 622.2.
Across three runs those are **1.2%** and **0.2%** apart.

**But the same three runs disagree by 83% about something else, and that is the real lesson.**
`ring_full` is the one measurement in this repository that does *not* go through the bench
harness — it fills the ring once and times the fill:

| Run | messages accepted | time to fill | ns per message |
|---|---|---|---|
| 33304774414 | 352 | 68.518 µs | 194 |
| 33304926978 | 352 | 68.889 µs | 195 |
| 33304998832 | 352 | **125.595 µs** | **356** |

The **count is identical to the message** in all three. The **duration nearly doubles.**

So the correction written above — *the runner is a stable instrument that happens to be
calibrated wrong* — is itself too strong, and this is the third time this one finding has had
to be narrowed. **The stability belongs to the measurement method, not to the machine.** The
harness takes the best of 7 runs of 200 000 iterations, which is what discards the scheduler's
interference; the one figure gathered as a single shot of 352 operations swings 83% on the same
box in the same ten minutes.

Two consequences, one for each half:

* **A per-machine baseline is a live option for the ceilings**, because the harness-mediated
  figures reproduce to ~1% on this runner — there is room for a real regression to show.
* **Any single-shot duration is not a measurement**, wherever it appears. What survives from
  `ring_full` is the count (352 messages, exact three times); the microseconds are one sample
  of a distribution nobody has characterised.

### It was two CPUs all along

A fifth run resolved it, and the thing that resolved it was a line added to
`scripts/check-machine.sh` one commit earlier: **the CPU model, printed with every set of
figures.**

| Run | CPU | `ring, one way` | `ring, round trip` | `ring_full` ns/msg | `parse` |
|---|---|---|---|---|---|
| 33304774414 | **EPYC 7763** | 328.3 | 622.9 | 194 | 127.6 |
| 33304926978 | (7763) | 331.1 | 623.5 | 195 | — |
| 33304998832 | (7763) | 327.2 | 622.2 | 356 | — |
| 33307245558 | (9V74) | 270.7 | 514.7 | 139 | 124.0 |
| 33307366947 | **EPYC 9V74** | 272.9 | 517.7 | 139 | 123.3 |

The GitHub runner pool is **not one machine**. It has at least two CPU generations, and the
five samples are not one noisy distribution — they are **two tight ones**:

| | within EPYC 7763 | within EPYC 9V74 | between them |
|---|---|---|---|
| `ring, one way` | 327.2–331.1, **1.2%** | 270.7–272.9, **0.8%** | **21%** |
| `ring, round trip` | 622.2–623.5, **0.2%** | 514.7–517.7, **0.6%** | **20%** |
| `parse NewOrderSingle` | 127.6 | 123.3–124.0 | **3%** |

And the mechanism is visible rather than assumed: the gap is **21% on the cross-thread cases
and 3% on the single-threaded ones**. `ring` moves a message between two cores; Zen 3 and the
later generation differ in inter-core latency far more than in single-core throughput. The
figure that moved is exactly the figure that should move.

So the run of corrections resolves like this, and the last one is an explanation rather than a
description:

| # | Claim | n | Refuted by |
|---|---|---|---|
| 1 | red on Linux | 1 run | 5 runs |
| 2 | noise on Linux, 3 of 12 flap | 5 runs, 1 machine | a second machine |
| 3 | 1.7× between machines; the runner is stable | 2 machines | a single-shot timing |
| 4 | the stability is the harness's | 3 CI runs | a fourth CI run |
| 5 | cross-thread is unstable | 4 CI runs | **a fifth, once the CPU was labelled** |
| 6 | **the pool is two CPUs; each is stable to ~1%; they differ 21% cross-thread and 3% single-threaded** | 5 CI runs, 2 labelled | *nothing yet* |

**Every one of the first five treated a pooled sample from an unlabelled fleet as a
measurement of one thing.** More samples never fixed that and could not: averaging over a
mixture converges on a number that describes neither component. What fixed it was one line of
metadata per sample.

The rule, and it is the whole lesson of this entry: **before calling a spread "noise", label
each sample with the machine that produced it.** A bimodal result from a heterogeneous fleet is
indistinguishable from a noisy result on one box until you write down which box.

### And with the CPU controlled, the earlier correction turns out to have been right

`ring_full` was left open above: 194, 195, then **356** on the same 7763. A sixth run put the
same shape on the *other* CPU — 139, 139, then **263** on the 9V74 — so it is not one bad
sample on one box:

| CPU | `ring, one way` (harness) | `ring_full` ns/msg (single shot) |
|---|---|---|
| EPYC 7763 | 328.3, 331.1, 327.2 — **1.2%** | 194, 195, **356** — **83%** |
| EPYC 9V74 | 270.7, 272.9, 271.4 — **0.8%** | 139, 139, **263** — **89%** |

**Two measurements of the same cross-thread hop, on the same machine, in the same run** — and
one holds to ~1% while the other roughly doubles. The only difference between them is that one
takes the best of 7 runs of 200 000 iterations and the other times 352 operations once.

So correction 4 — *the stability is the harness's, not the machine's* — was **true**, and the
fourth sample never refuted it. That sample changed the CPU, and with CPU uncontrolled the two
effects were indistinguishable. Labelling the machine separated them:

| Effect | Size | Visible in |
|---|---|---|
| CPU generation | **21%** cross-thread, 3% single-threaded | between clusters |
| single shot vs best-of-7 | **~2×**, on both CPUs | within a cluster |

The confound was never in the data; it was in not recording which machine each number came
from. **Two real effects of similar magnitude, one uncontrolled variable, and a sequence of
corrections that each explained the whole of the spread by one of them.** That is what makes a
finding oscillate rather than converge.

ADR-0011 leans on the count, which is exactly right and now for a measured reason: 352 messages
on every run of both CPUs, and a fill duration that spans 139–356 ns per message.

The rules that generalise: **a threshold whose margin is smaller than the spread of the
machines it will run on reports the infrastructure, not the code** — and **before crediting a
machine with being quiet, check whether the quiet came from the machine or from the averaging
in your harness.** Two figures from the same box in the same minutes, one repeating to 0.2%
and one swinging 83%, differ only in how they were gathered. Measure the spread on more
than one machine before believing any verdict from such a gate — and note that the first
correction is as likely to be wrong as the first reading was. Here the sequence was: *red on
Linux* (one run), then *noise on Linux* (five runs, one machine), then *1.7× between machines*
(two machines). Only the third survived contact with more data.

`inline deliver + reply` is the extreme and it was predicted in writing. `harness.rs` said in
its own doc comment: *"Baseline 2.5–4.9 ns across runs. The spread is the measurement's, not
the code's."* The ceiling was set at 15 ns anyway. Observed: 3.4–11.3 ns over five runs, and
17.8 ns on a sixth — a 232% spread against a ceiling 3× the baseline.

### Two things found only because the benchmarks were finally run

**A bench target that measured nothing and passed.** Cargo auto-discovers `benches/*.rs`.
`benches/harness.rs` is a module included by `#[path]`, not a benchmark, and Cargo made it a
target of its own: `cargo bench --bench harness` printed `running 0 tests … 0 measured` and
exited 0. Fixed with `autobenches = false`, and it is now the injection that proves
`scripts/bench.sh`'s liveness check can fail.

**A failing case nobody had ever seen.** The harness asserted inside each case, so the first
case over its ceiling ended the process. `groups` has four cases; it died at the second, and
the fourth — `encode 1 group, 2 entries`, over its ceiling on 4 of 5 runs — had never once
been executed. A benchmark exists to produce numbers, and one that stops at the first bad
number hides exactly the ones worth having. The harness now measures and prints every case,
then asserts at the end; the assertion is reachable only through `suite()`, so a bench cannot
report figures without being checked.

`[to testing-skills]` — two cases. *One run is not a measurement, and its direction is not
evidence of its sign*: a gate whose spread exceeds its margin was read once, in each
direction, and both readings reached documents. The fix is not a better threshold but
measuring the spread before believing any verdict. And *the fail-fast assertion that hides its
own evidence*: a check that aborts at the first failure suppresses the results after it, so
the run that most needed reading produces the least. Report every case, then fail.

### And on a machine whose settings we control, tuning moves the median by 0.5%

`[measured 2026-08-30]` The owner's Linux desktop — AMD Ryzen 7 3700X (Zen 2, 8 cores,
2 × 4-core L3 domains), Linux 7.0.0-30-generic, rustc 1.98.0 — is the first machine in
this thread whose `DESIGN.md` §9 state could be **changed and changed back at will**, so
the question "how much of the ceiling problem is the machine" finally has a same-machine
A/B rather than a comparison between two rented CPUs.

`scripts/check-machine.sh` goes `pass 1 fail 7` → `pass 6 fail 2` with five settings —
governor `performance`, boost off, SMT off, THP `never`, `busy_poll=50`. The two that
remain need a kernel command line and a reboot.

**15 full `scripts/bench.sh` runs in each state.** Medians, and how often each case cleared
its own ceiling:

| case | ceiling | tuned med | over | untuned med | over |
|---|---|---|---|---|---|
| `walk 4 levels, 61-tag member list` | 300 | 347.6 | **15/15** | 354.2 | **15/15** |
| `encode 1 group, 2 entries` | 75 | 104.7 | **15/15** | 103.5 | **15/15** |
| `encode ExecutionReport (template)` | 190 | 241.4 | **15/15** | 236.7 | **15/15** |
| `ring, one way` | 260 | 259.6 | 5/15 | 260.4 | 9/15 |
| `ring, round trip` | 500 | 499.1 | 7/15 | 500.4 | 8/15 |
| `parse NewOrderSingle (validated)` | 150 | 122.2 | 0/15 | 122.9 | 0/15 |
| `parse Heartbeat (validated)` | 70 | 55.3 | 0/15 | 56.2 | 0/15 |

**Every median moves less than 2%.** Tuning a machine to §9 is not what makes these numbers
what they are.

The table separates the cases into two kinds, and the distinction is the useful part:

- **Three cases are over on every single run in both states**, by 16%, 40% and 27%. No
  amount of machine state explains those. They are real gaps between the code and the
  ceiling somebody wrote, and they are the only rows here that a §6 gate can honestly fail
  on today.
- **The two ring cases are coin flips.** 5/15 and 9/15, 7/15 and 8/15 — the ceiling sits
  *at the median*, so the verdict is decided by which side of its own noise a run lands on.
  A gate that reports red 33% of the time on unchanged code is not measuring the code.

### The 0.5% that decides a ceiling

Run the `dispatch` bench **alone** — not through `bench.sh` — 15 times in each state, and
the ring case is well behaved:

```
TUNED    n=15  min=257.2  med=259.6  max=260.4  spread=1.2%  stdev=0.89  over 260:  1/15
UNTUNED  n=15  min=259.2  med=260.9  max=264.7  spread=2.1%  stdev=1.32  over 260: 14/15
```

**That paragraph was written here, and it was wrong.** It said the median moves 0.5%, the
ceiling is inside that 0.5%, and unchanged code therefore "goes from failing 14 of 15 runs
to passing 14 of 15". The first half survives. **The verdict does not.**

`[measured 2026-08-30]` The owner asked whether the machine had been idling with its screen
off during that sample, so the identical command was run again — and the second sample
disagrees with the first:

```
TUNED    sample 1  n=15  med=259.6  min=257.2  max=260.4   over 260:  1/15
TUNED    sample 2  n=15  med=260.3  min=256.7  max=325.1   over 260:  9/15
UNTUNED  sample 1  n=15  med=260.9  min=259.2  max=264.7   over 260: 14/15
UNTUNED  sample 2  n=15  med=262.3  min=258.8  max=265.5   over 260: 14/15
```

**The medians reproduce to within 1.4 ns. The pass rate does not reproduce at all** — 1/15
became 9/15 on the same machine, same command, same binary. Pooled over 30 runs per state:

| | median | over the 260 ceiling | second mode ≥300 |
|---|---|---|---|
| tuned | 259.7 | **10 / 30 — 33%** | 2 / 30 |
| untuned | 261.8 | **28 / 30 — 93%** | 0 / 30 |

So the honest statement is the weaker one: the governor moves the median **0.8%**, the
ceiling sits between the two medians, and the case fails **93% of the time untuned and 33%
tuned**. Tuning helps and does not rescue it. **Neither state produces a stable verdict, and
that is the finding** — a gate that answers differently a third of the time on unchanged
code is not a gate, whichever way the machine is set.

`[to testing-skills]` — *the median reproduced and the verdict did not.* Fifteen runs looked
like plenty: the spread was 1.2%, the distribution looked tight, and one sample was written
up as a result. What made it wrong was not noise in the numbers, it was that **the statistic
being reported was a threshold crossing** — and a threshold sitting near the median converts
a small, well-behaved shift into a coin flip, so the pass rate needs far more samples than
the median does. The check that would have caught it costs one command: **run the sample
twice before writing down a rate.** A second reason this one slipped: it was found by a
question from outside — *"was the machine idling?"* — not by the person holding the data.

The screen-off hypothesis itself was refuted while testing it. CPU frequency was sampled
throughout both re-runs and held **3793–3814 MHz** in each state, `sleep-inactive-ac-timeout`
is `0` so the box never suspends on AC, and `power-profiles-daemon` at `balanced` was watched
for 40 s and **did not** revert the `performance` governor. None of the three could have
produced the difference, and the difference turned out not to need producing: it was sampling.

That the same case is stable at 1.2% in one sample and swings 26% in the next, and 26–38%
inside `bench.sh`, is the same lesson from a third angle: what the harness measures depends
on what ran before it and on nothing you can see in the number.

### A second mode near 324 ns, five sightings, all in one machine state

`ring, one way` occasionally returns **323.7, 323.9, 323.7, 324.9, 325.1 ns** instead of
~259 — five values inside 1.4 ns of each other, which is not jitter but a second mode. It
is **not explained**, and it is recorded here rather than in a commit message because the
next person to see 324 should know it is not new.

The paragraph that stood here said **"all five sightings are in the §9-tuned state; none in
roughly 45 untuned runs"**, and named SMT-off as the suspect. That was the third hypothesis
about this mode, and like the two before it, it was wrong — and wrong the same way: an
association read off a sample too small for a 5% event.

## Three hypotheses about one 324 ns mode, all refuted by measurement

### 1. Zen-2 L3 placement — refuted

The 3700X has two 4-core L3 domains, so the guess was that a run straddling them pays
Infinity Fabric latency. The first check was a **bad experiment**: `taskset -c 0-3` against
`-c 0,4` changes the CPU *count* as well as the L3 relationship, and can attribute nothing.
Redone with the count held at two:

```
2 CPUs, same L3 (0,1):   323.7  259.7  259.8  259.9  259.8
2 CPUs, cross L3 (0,4):  259.6  260.0  256.6  257.4  259.8
8 CPUs, free    (0-7):   256.2  259.9  259.9  257.3  259.6
```

**No L3 effect** — ~259 in all three arms — and the outlier landed in the arm the hypothesis
favoured least.

### 2. SMT off — refuted by a 2 × 2

A verb was added to the machine helper so SMT could be varied **independently** of the other
four settings, which is the only way to attribute anything. 50 runs in each of four states:

| | governor / boost | SMT | mode ≥300 |
|---|---|---|---|
| A | powersave / on | on | 2 / 50 |
| B | performance / off | **off** | 3 / 50 |
| C | performance / off | on | 3 / 50 |
| D | powersave / on | **off** | 5 / 50 |

**All four.** Not SMT, not the governor, not boost. The earlier "all five sightings were
tuned" was the sample, not the machine — the second time in one afternoon that a rate
computed from a handful of runs pointed at the wrong cause.

### 3. Thermal throttling — refuted

Asked whether a hot CPU was stepping down mid-run. It gets genuinely hot: **91 °C** under
load, against the 3700X's 95 °C Tctl limit. But throttling means **frequency falls**, and it
did not:

```
quiet      1/30 over 300   med 262.4 ns   65-77 °C   min freq across all cores 3789 MHz
+8 spinners  30/30         med 449.5 ns   76-91 °C   min freq across all cores 3786 MHz
```

At 91 °C every core was still at ~3790 MHz. No step-down, so no throttle. (The first attempt
at this measurement sampled only the **maximum** frequency across cores — which cannot see
one core dropping — and its sampler's wait loop was a **busy spin**, so the "quiet" arm was
never quiet. Both fixed before the numbers above were taken.)

## What it actually is: the row that was not on the checklist

```
quiet machine, 60 runs           mode ≥300:  0-3 / 60   (~5%)
+ 8 spinners,  60 runs           mode ≥300:     55 / 60   (92%)
median under load                262 ns -> 449 ns        (+71%)
```

**Competing CPU load.** Against **0.8%** for every `DESIGN.md` §9 tuning row combined — the
checklist was reading governor, SMT, THP and C-states, and had **no row at all** for whether
anything else was running. The box reported `pass 6` while an LLM, an editor and two Electron
apps shared it.

`scripts/check-machine.sh` now carries **`machine is quiet`**: CPU busy over a one-second
window from `/proc/stat`, FAIL above 3%, with the top processes attributed by their own delta
over the same window. Proven by reversal — eight spinners take it from `PASS 1%` to
`FAIL 26%`, naming each spinner at exactly 100% of a core.

`[to testing-skills]` — *three causes proposed, three refuted, and the real one was not on
the checklist.* The generalisable part is not "check CPU load"; it is that **a tuning
checklist enumerates what somebody thought of, and the largest term can simply be absent from
it** — here by a factor of ninety, 71% against 0.8%. Two supporting rules, both paid for:
**an association from a handful of samples of a rare event will point at whatever varied
most recently** (L3, then SMT, then thermal — each fitted the data available when proposed),
and **the instrument must be checked before the hypothesis**: one sampler here spun a whole
core while measuring quietness, and another read `ps %CPU` — a **lifetime average**, which
reported an idle process at 19% on a machine `/proc/stat` measured as 1% busy, and that
number reached the owner as fact before the two were compared.

**What is still unexplained, and a fifth hypothesis refuted.** The mode does not vanish on a
quiet machine, so the obvious next step was to stop measuring load *before* a batch and
measure it *per run* — `/proc/stat` either side of each individual execution, so every
outlier carries the busy figure for its own 1.1 s. Sixty runs, then sixty more with the
desktop's LLM shut down:

```
                 mode >=300   median (normal runs)   busy% on the outliers
LLM resident        6 / 60          259.5 ns         13 13 13 17 13 13
LLM shut down       6 / 60          259.3 ns         14 13 13 14 14 13
```

`13%` is the benchmark itself — one core of eight. **The outliers carry the same background
load as every other run, and closing the LLM changed nothing: 6/60 either way.** So load is
**sufficient** to produce the mode — eight spinners take it to 92% — and is **not what
produces the naturally occurring ones**. Sufficient is not necessary, and the intervention
that proved the first had been quietly answering the second.

Five hypotheses have now been proposed and measured away: L3 placement, SMT, governor/boost,
thermal, and background load. Three more followed, and so did a proper characterisation.

### Eight hypotheses, and what the thing actually looks like

`[measured 2026-08-30]` on the desk box with `DESIGN.md` §9 **satisfied** — `check-machine.sh`
`pass 10 fail 0 unknown 1` — after `isolcpus=6,7,14,15 nohz_full=6,7,14,15 rcu_nocbs=6,7,14,15
processor.max_cstate=1` and the five runtime rows.

| # | Hypothesis | Test | Result |
|---|---|---|---|
| 1 | Zen-2 L3 placement | `taskset`, CPU count held at 2 | **Refuted** — ~259 ns in all three arms |
| 2 | SMT off | 2 × 2 over (governor·boost) × SMT, 50 runs a cell | **Refuted** — present in all four |
| 3 | governor / boost | same 2 × 2 | **Refuted** |
| 4 | Thermal throttling | temp + per-core frequency under load | **Refuted** — 91 °C, no step-down |
| 5 | Background CPU load | `/proc/stat` per run; LLM shut down | **Refuted** — 6/60 either way, outliers carry the same busy % |
| 6 | The scheduler | pinned to isolated cores, tick off, RCU elsewhere | **Refuted** — 5/60 vs 4/60 unpinned |
| 7 | Interrupts on the core | `/proc/interrupts` per run on the pinned pair | **Refuted** — 2 outliers with ~1300, **3 with exactly 0**, a normal run with 1085 |
| 8 | Memory layout / ASLR | 250 runs `setarch --addr-no-randomize`, 250 with | **Refuted** — **14/250 vs 14/250**, z = 0.00 |

Hypothesis 8 refutes itself twice over, and the second way is stronger than the statistics:
**with ASLR off the layout is fixed across runs, so a layout-dependent effect would have to be
0% or 100% — not 5.6%.**

### What it is, precisely, even though the cause is unknown

Pooling those 500 runs — one process each, `ring, one way`:

```
250-254 ns  #######                                                        7
255-259 ns  ############################################################ 455
260-264 ns  #########                                                      9
290-294 ns  #                                                              1
320-324 ns  ########################                                      24
325-329 ns  ####                                                           4

  main mode    n=472   median 258.4   stdev 1.98
  second mode  n= 28   median 323.7   stdev 1.25     5.6% of runs
  ratio of medians                    1.2527
```

**The gap is empty**: one value out of 500 lies between the two clusters. Both clusters are
equally tight, which rules out "a slow run" — a run perturbed by something external would
smear, and these do not. **A process picks one of two states at startup and stays in it for
its whole life**, and the two states differ by a factor of **1.2527**, near enough 5/4 to be
worth saying out loud.

That is worth more than another guess at the cause. "Sometimes slow" cannot be designed
against; "5.6% of processes run in a second state 25% slower, decided at startup, invariant
to machine tuning, isolation, load, interrupts and address layout" is a specific thing to go
looking for — and it says plainly that **any single benchmark run of this case has a 5.6%
chance of being 25% wrong**, which is the practical consequence for every ceiling in
`DESIGN.md` §6.

`[to testing-skills]` — *characterise before you attribute.* Eight hypotheses were proposed
and eight refuted, three of them from associations in samples of 5 to 60 runs that dissolved
when the sample grew — the ASLR difference read 8.3% against 3.3% at n=60 and 5.6% against
5.6% at n=250. What finally produced something usable was not a ninth hypothesis but 500 runs
and a histogram: an empty gap between two tight clusters says "two states", and that is a
fact about the system that survives every wrong guess about why. **The rate needs a large
sample; the shape needs only an honest plot, and the shape is what was actionable.**

### Clean baselines, quiet machine, `dispatch` run directly

Not through `cargo`, which is itself a competing process — via `cargo bench` the same box
gives ~5% where the bare binary gives 0/60.

```
untuned + quiet   n=60  min 257.4  med 260.6  max 326.1   over 260: 43/60   mode: 3/60
tuned   + quiet   n=60  min 256.6  med 259.7  max 323.9   over 260: 13/60   mode: 1/60
```

The medians are 0.3% apart and the pass rates are 72% and 22%, which is the same lesson this
file records twice above: **the ceiling sits at the median, so the rate is not a measurement
of the code in any machine state.**

**A hypothesis was tested and refuted.** Zen 2 puts 4 cores per L3 domain, so the obvious
guess was that a run straddling the two domains pays Infinity Fabric latency. The first
attempt to check it was a bad experiment — `taskset -c 0-3` against `-c 0,4` changes the
CPU **count** as well as the L3 relationship, so it could not attribute anything. Repeated
with the count held at two:

```
2 CPUs, same L3 (0,1):   323.7  259.7  259.8  259.9  259.8
2 CPUs, cross L3 (0,4):  259.6  260.0  256.6  257.4  259.8
8 CPUs, free    (0-7):   256.2  259.9  259.9  257.3  259.6
```

**No L3 effect** — all three arms sit at ~259 — and the outlier appeared in the *same*-L3
arm, which is where the hypothesis predicted it least. Inter-CCX distance is not the
mechanism. What is, is unknown.

### What this says about the ceilings

`STATUS.md` open item 20 asked whether a per-machine baseline is viable. On the evidence
here, keyed on the CPU model it is — the single-threaded cases hold to 3% or better across
all 30 runs — but **the two ring ceilings cannot be rescued by tuning a box**. They sit
inside the harness's own run-to-run variation, and closing that needs pinning (`isolcpus`,
still unset here) or a different measurement, not a better governor.

## `scaling_cur_freq` is frozen on a `nohz_full` core, and reads 41% low

`[measured 2026-08-30]` After the desk box was booted with
`isolcpus=6,7,14,15 nohz_full=6,7,14,15`, a thermal sample taken while the benchmark ran
pinned to those cores reported:

```
xung lõi 6: min 2240 med 2240 max 2240 MHz      (governor: performance)
```

**2240 MHz is this CPU's `scaling_min_freq`.** Taken at face value it says every measurement
pinned to an isolated core ran at 59% clock — while producing **the same 259 ns** as an
unpinned run at 3790 MHz. Both cannot be true, so one of the two instruments was lying.

The one that lies is the sysfs file. Counting work actually executed, one second each:

```
cpu6 (isolated, nohz_full):  7,895,418 loops/s
cpu0 (ordinary):             7,958,092 loops/s      0.8% apart
```

The isolated core runs at full speed. The driver is **`amd-pstate-epp`**, which refreshes
`scaling_cur_freq` from a periodic tick — and `nohz_full` **stops that tick on exactly the
cores being measured**. The hardware is fine; the file is frozen at whatever it held when the
tick stopped.

The trap is sharper than a stale number: **the isolation that makes a core worth measuring on
is what breaks the instrument pointed at it**, so the reading is wrong precisely where
somebody would think to take it, and it is wrong in the direction that invites a wrong story
— a benchmark "running at minimum clock" is a tidy explanation for almost anything.

**Use it only on cores without `nohz_full`.** On an isolated core, measure work done per unit
time — or `aperf`/`mperf` via `turbostat`, which reads the counters rather than the governor's
opinion. `scripts/check-machine.sh` is unaffected: it reads the governor and the boost flag,
never a per-core current frequency.

`[to testing-skills]` — *the fourth instrument in one day that could not see what it was
pointed at*, after `ps %CPU` (a lifetime average), a per-process sampler slow enough to
distort its own window, and a `quietness` sampler whose wait loop was a busy spin. The pattern
across all four is one rule: **check the instrument against a known state before believing a
surprising reading.** Here the known state was free — an ordinary core, measured the same way,
one second of work.

## The SFF case, the fan profile, and GNOME's power mode: three non-effects, measured

The desk box is a Mini-ITX / small-form-factor build, its BIOS fan profile is `silent`, and
GNOME's Power Mode is `Balanced`. Each was raised as a possible source of instability. All
three were measured rather than reasoned about, and none of them moves anything.

### Thermal drift over a 7-minute soak: none

`[measured 2026-08-30]` §9 satisfied, `ring, one way` run continuously pinned to the isolated
cores, 379 runs in 420 s, temperature sampled alongside every run:

| window (s) | n | median ns | Tctl |
|---|---|---|---|
| 1–84 | 75 | 259.0 | 64 °C |
| 85–167 | 75 | 258.8 | 64 °C |
| 168–250 | 75 | 258.5 | 64 °C |
| 251–332 | 75 | 258.8 | 64 °C |
| 333–416 | 75 | 258.9 | 64 °C |

**0.5 ns — 0.2% — between the first window and the last.** Temperature rises 59 → 64 °C in the
first minute and then sits there for the remaining six. Correlation of temperature against
per-run time, main mode only: **r = +0.060**, which is nothing.

The 3700X throttles at 95 °C Tctl. The workload that matters reaches **64 °C**, leaving 31 °C
of headroom, so the `silent` fan profile does not constrain it. The 91 °C recorded earlier came
from an artificial eight-spinner stress test, not from anything this project measures — and
even there the frequency never stepped down.

**This is what makes the box publishable, more than the tuning is:** a machine whose numbers
drift as it warms cannot carry a latency figure however well configured, and this one does not
drift.

### GNOME Power Mode `Balanced`: no effect, for a reason worth knowing

The driver is **`amd-pstate-epp`**, where `power-profiles-daemon` sets an EPP hint separately
from the governor. An earlier check here only watched the governor, which would have missed it.
Reading both:

```
tuning off:  PPD=balanced  governor=powersave    epp=performance  max=4426 MHz  boost=1
tuning on:   PPD=balanced  governor=performance  epp=performance  max=3600 MHz  boost=0
```

**EPP is already `performance` under `Balanced`** — on this machine PPD's balanced profile does
not ask for a conservative preference. And setting `governor=performance` for a measurement
collapses `energy_performance_available_preferences` to `performance` alone, so the desktop
setting cannot reach the measurement even in principle.

**The A/B intended to prove this failed, and is recorded as failed.** `powerprofilesctl set
performance` returned `Failed to activate CPU driver 'amd_pstate': ... policy11 ... Device or
resource busy`, so both arms ran under `balanced` — two samples of one condition, not a
comparison. They agreed (258.5 and 258.8, 4/100 and 7/100 in the second mode), which measures
reproducibility and answers nothing about PPD. The state readings above are what answer it.

### And a side effect of the tuning, found by that failure

`[measured 2026-08-30]` **`SMT off` breaks `power-profiles-daemon`'s mode switching.** Proven
by reversal:

```
SMT on  (CPU 0-15):  performance -> balanced      succeeds
SMT off (CPU 0-7 ):  policy11: Device or resource busy    fails
```

`policy11` belongs to a CPU that is offline while SMT is off, and PPD writes every policy. It
is harmless and reverts when SMT comes back, but anyone measuring on this box will see the
desktop's Power Mode selector throw an error and should know why. It also cost a working A/B,
which is the more expensive half: **a failed intervention that still produces two clean-looking
arms is exactly the shape of a false green.**

## The engine is syscall-bound, and the §8 budget is spent before FIX begins

`[measured 2026-08-30]` desk box, §9 satisfied, pinned to an isolated core. **Not a measurement
of `Engine::turn`** — a C program issuing the syscall that `turn` issues, on connected loopback
sockets, which makes it a **floor** for the engine rather than a reading of it.

D8 defines the model: *"`Engine::turn` is one non-blocking pass over every connection … read
once … a counterparty that writes faster than this end processes must not be able to starve the
other connections on the thread."* So an idle turn is **one `read` per connection**, and each
returns `EAGAIN`.

```
N=1      703.2 ns/read      703.2 ns/turn
N=2      705.1 ns/read     1410.1 ns/turn
N=4      704.1 ns/read     2816.5 ns/turn
N=16     702.3 ns/read    11237.5 ns/turn
N=64     703.6 ns/read    45033.0 ns/turn
N=256    707.0 ns/read   180988.1 ns/turn
```

**Flat to N=256** — 703 ns is a fixed per-socket cost and the sweep is exactly linear. A message
that arrives just after its socket was polled waits up to one whole turn to be seen, so this
table is *added latency per session*, not throughput.

### Where the 703 ns goes

```
clock_gettime (vDSO, no kernel entry)    22.9 ns
syscall(getpid) — enters and leaves, does nothing   353.8 ns
read(/dev/null)                         452.2 ns
read(socket) -> EAGAIN                  703.0 ns
```

**354 ns of every socket poll is kernel entry and exit doing nothing at all.** Set against this
project's own numbers: `parse NewOrderSingle (validated)` is **125.5 ns**. *The syscall that
discovers there is nothing to parse costs 5.6× the parse.*

`DESIGN.md` §8 budgets *"an entire user-space path under 1 µs"*. The user-space path is not the
problem — the vDSO line shows user space doing work in tens of nanoseconds. **The budget is
spent crossing into the kernel, before any FIX work starts.**

### What this says about "many sessions on one core"

`PRD.md` names the target as *"an acceptor that holds **many sessions on one core** and does not
stall"*. Against the table above, "many" has a cost that can be stated exactly:

| Sessions on one polling thread | Idle sweep | Against §8's 1 µs |
|---|---|---|
| 1 | 703 ns | 70% of it, spent finding nothing |
| **2** | **1.41 µs** | **the whole budget, exceeded, before parsing anything** |
| 16 | 11.2 µs | 11× |
| 128 | 90 µs | 90× |

**Two sessions on one core exhausts the design's entire user-space latency budget in polling
alone.** That is not a tuning problem; it is the arithmetic of one syscall per socket per turn.
It is also why HFT practice dedicates a core to a latency-critical gateway rather than sharing
one — a point the outside literature makes in general terms and this table makes in nanoseconds
for this codebase.

The PRD's target is not thereby wrong: a broker gateway carrying many client sessions is a real
product, and 90 µs is unremarkable for one. **But it is a different product from "the fastest
acceptor that can run on kernel TCP", and the two cannot share a polling thread.** That is a
decision for `PRD.md` and it has not been made.

### It also reprioritises the open items, by an order of magnitude

Open item 12 defers SIMD on the grounds that it would win *"20–40 ns per message on a 10–20 µs
floor — under 0.5%"*. That reasoning stands and this measurement sharpens it: **the syscall is
703 ns per socket per turn**, so anything that removes syscalls is worth roughly **20× what
SIMD is worth**, and the ordering follows from measurement rather than taste:

1. **Fewer sessions per polling thread.** Free, and the largest single factor.
2. **`mitigations=off`.** Full mitigations are in force — `retbleed` untrained return thunk,
   `spec_rstack_overflow` Safe RET, `spectre_v2` retpolines with STIBP always-on, and
   `vmscape: IBPB before exit to userspace`, an IBPB on **every** syscall return. Zen 2 pays
   heavily for these. **`[unproven]` — this has NOT been measured here**, it needs a reboot,
   and it is a security decision on a machine somebody also uses as a desktop.
3. **Batch the syscall** — `recvmmsg`, or `io_uring` with `SQPOLL`, which removes the per-socket
   entry entirely.
4. **Kernel bypass**, open item 14, which removes the kernel from the path. Its own entry
   already says the first measurement is `tools/w2w` twice on one box, kernel versus Onload.

`[to testing-skills]` — *the budget was being checked against the wrong half of the system.*
Every measurement in this file until now timed **user-space work**: parse, encode, ring hop,
allocation counts, all in the 5–500 ns range and all carefully guarded. The path they sit on
crosses into the kernel once per socket per turn at 703 ns, and **nothing measured that until
somebody asked a question about deployment shape**. A latency budget stated for "the user-space
path" invites exactly this: the measured part is optimised to a fraction of the unmeasured part.
The cheap defence is to measure one whole turn end to end, including the syscalls, before
tuning anything inside it.

## Kernel bypass removes the largest term and leaves two behind

Asked whether kernel bypass — no syscall — makes many sessions on one core a non-issue. It
removes the term that dominates today and **does not remove the shape of the problem**. Three
terms, measured where they can be.

### Term 1 — the polling sweep stays linear in N

Bypass replaces `read()` with a userspace descriptor poll. The syscall's **703 ns** goes; what
replaces it is a memory read of a descriptor the NIC wrote by DMA, so it costs what a memory
access costs — see the curve below, **1 to 80 ns** depending on where it lands. That is a
9–70× improvement and it is still `N ×`. At N=128 and 50 ns a sweep is **6.4 µs**, six times
`DESIGN.md` §8's whole user-space budget.

### Term 2 — cache, which bypass does not touch at all

`[measured 2026-08-30]` `size_of::<Connection<Loopback, Acceptor, MemJournal<64,512>, 64, 4096,
8192>>()` = **54 600 bytes, 53.3 KiB**. `Session<Acceptor,64>` is 8 960 B and
`MemJournal<64,512>` is 33 288 B of it. **`L1d` on this machine is 32 KiB — one connection does
not fit in L1.**

Random-access latency by working-set size, pointer-chase, §9 machine, isolated core:

| Working set | ns per access | Tier |
|---|---|---|
| 16–32 KiB | **1.05** | L1d |
| 64 KiB | 2.58 | |
| 256 KiB | 3.11 | L2 |
| 512 KiB | 5.53 | L2 edge |
| 1 MiB | 9.65 | |
| 4–8 MiB | 11.5–12.0 | L3 |
| 16 MiB | 31.0 | L3 edge |
| 32–64 MiB | 68–79 | RAM |

**L1 to RAM is 75×**, and it applies to *every* memory access the engine makes — parse, session
step, template patch — not to the polling alone. Adding sessions walks the whole engine down
that table.

**What is not known, and it decides where the wall is:** how much of the 53.3 KiB a connection
touches per message. The structure size is measured; the *touched* set is not. The two bounds
are far apart and both are stated rather than one being picked:

- If a message touched all of it, the L2 edge arrives at **N ≈ 9**.
- If it touched 4 KiB — a framer head, the live part of the field index, the hot session
  fields, a template — the L2 edge arrives at **N ≈ 128**.

Measuring that fraction is worth more than any further guess about the 324 ns mode.

### Term 3 — head-of-line blocking, which nothing removes

One thread and N sessions serialise by construction. The per-message work is measured on this
machine: `parse NewOrderSingle` 125.5 ns, `encode ExecutionReport` 240.0 ns, plus the session
step — call it ~465 ns of work per message. With `k` sessions holding a message at the same
instant, the last one served waits `(k-1) × 465 ns` before its own processing begins.

Bypass does not touch this. Neither does a faster codec. **It is the cost of sharing a thread**,
and the only fix is fewer sessions on it — which is
[ADR-0012](../decisions/ADR-0012-latency-first-and-one-session-per-polling-thread.md)'s
decision, arrived at from a different direction.

### So: does bypass make density free?

**No.** It removes 703 ns per socket, which is the largest single term today and worth doing
for that reason alone — open item 14 already ranks Onload first. What is left afterwards is a
sweep still linear in N, a cache hierarchy that punishes N by up to 75×, and serialisation that
is linear in *active* sessions. **A latency-first engine wants few sessions per thread whether
or not the kernel is in the path**, which is the conclusion ADR-0012 reached before this
measurement existed and which this measurement did not overturn.

`[to testing-skills]` — *removing the dominant term promotes the next one, and it is rarely the
one that was being discussed.* The instinct "no syscall, therefore no problem" is right about
the term it names and silent about the two behind it. The cheap defence is to write down every
term you can name **before** removing any of them, so that the second-largest is already
measured when it becomes the largest.

## The serialise target is missed by the fixed cost, not by the scan it was blamed on

`[measured 2026-08-31]` Intel Xeon @ 2.10GHz, 4 cores, **`check-machine.sh` = `pass 2 fail 6
unknown 3`, guest under docker**. Every absolute figure below is therefore a same-machine A/B
and **not publishable** (non-negotiable 10); the *ratios* are what transfer.

`DESIGN.md` §6 publishes **≤ 60 ns** for template serialisation and records **93.8 ns** as not
meeting it. STATUS open item 11 named the cause: `Template::encode` finds each slot by a linear
scan of the caller's list, so cost is slots × parts. That cause is **real and is about a
quarter of the total**, and the open item's framing — fix the scan and the gate is met — does
not survive measurement.

### First, an experiment that measured nothing, because it varied nothing

To show the scan was expensive, the 14 slots the caller supplies were **reversed** and the
encode re-measured: **145.0 → 153.5 ns**, 6%, which reads as *the scan does not matter*.

The conclusion is wrong because the experiment is. Forward, part *i* matches at position *i*:
1+2+…+14 = **105** comparisons. Reversed, it matches at 14−*i*: 14+13+…+1 = **105**. Reversing
the caller's order does not change the comparison count at all. The 6% is branch prediction.

**The variable that actually moves the scan is padding, not order.** Put *k* slots the template
never declares in **front** of the real ones and every `find()` walks past them: comparisons go
105 → 105 + 14*k*, while the output stays byte-identical — verified, 169 bytes and the same
byte sum in all four arms.

| pad | comparisons | ns (median of 5) | ns per added comparison |
|---:|---:|---:|---:|
| 0 | 105 | 178.5 | — |
| 8 | 217 | 209.3 | 0.28 |
| 32 | 553 | 407.9 | 0.51 |
| 64 | 1001 | 575.7 | 0.44 |

**A tag comparison costs ~0.4 ns**, so the 105 of them are worth **~42 ns** — about **24%** of
the whole operation, not the bulk of it.

### Where the rest goes: a sweep over the number of slots

Same template shape, *N* slots declared and *N* supplied, five runs each, medians:

| slots | message | ns | Δ per slot |
|---:|---:|---:|---:|
| 0 | 43 B | 30.8 | — |
| 1 | | 46.9 | |
| 2 | | 55.2 | |
| 4 | | 77.5 | |
| 8 | | 111.3 | |
| 14 | 169 B | 152.5 | **8.7** |

The curve is **linear in the number of slots**, not quadratic in the comparison count: a
straight line through *N* predicts every point within noise, while a line through comparisons
under-predicts the middle badly. That alone refutes "the scan dominates".

**30.8 ns is spent before a single variable field is written** — prefix assembly, three static
fields, the body-length render, the trailer. That is **51% of the entire 60 ns target**, on a
message carrying nothing.

### The checksum was suspected and cleared

`checksum()` runs over the whole message, so the 0-slot figure is not a constant: it covers 43
bytes at *N*=0 and 169 at *N*=14. Measured on its own at both lengths: **2.3 ns** and **3.2
ns**. It is vectorised and nearly flat — **~0.9 ns of the 122 ns difference**. Not the cause,
and worth recording as cleared rather than left as an open suspicion.

### What this means for the gate

Removing the scan **completely** — a perfect O(1) slot lookup — leaves
`152.5 − 42 + ~6 ≈ **116 ns**` on this box, against a published target of **60**.

So the three levers in order: **the ~31 ns fixed cost** (largest single term, and it is paid by
every message however small), **~7 ns per field in `put`**, and **~0.4 ns per tag comparison**.
Item 11 named the third.

`[to testing-skills]` — *an experiment that varies the label instead of the variable.* Reversing
an ordered search looks like the obvious way to make it expensive and is exactly symmetric:
the total work is identical, so the measurement can only report noise, and the noise reads as
a negative result about the hypothesis. The cheap defence is to **write down the quantity you
believe you are changing, and its value in each arm, before running it** — 105 and 105 on the
same line would have stopped this one before the compiler did. The experiment that does work
adds unmatched elements in front, where the count provably moves and the output provably does
not.
