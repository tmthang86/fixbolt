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

## `nanofix` prepared for a head-to-head — no figure yet — 2026-09-14

[the-second-linux-desk](../plans/2026-09-04-the-second-linux-desk.md) step A7. This section only
makes the boot B, row B8 comparison possible; **nothing below is a latency number.**

### Pin

`matthart1983/nanofix` at commit `0f79bae0ad141653c80cf01c60c807649a6adf7b`, cloned 2026-09-14 —
the same commit §1 above measured parse cost on (dated there 2026-07-05, its author date).
Licence: **MIT**, *"Copyright (c) 2026 Matt Hart"* (`vendor/nanofix/LICENSE`, gitignored, not
committed here — CLAUDE.md §2 rule 9).

### Two patches, neither committed anywhere, both local to the gitignored checkout

1. **`build.rs` neutralised to a no-op** (`println!("cargo:rerun-if-changed=build.rs");` and
   nothing else). The same defect §3 above describes for the parse-cost measurement is still
   there unchanged: `build.rs::main` never reads `CARGO_FEATURE_AERON`, so even
   `--no-default-features` alone still tries to locate or build Aeron. Neutralising it is the one
   patch the A7 brief names explicitly.
2. **An empty `[workspace]` table added to `vendor/nanofix/Cargo.toml`**, so Cargo stops walking
   up into fixbolt's own `[workspace]` at the repo root — this checkout is nested under fixbolt's
   gitignored `vendor/`, and without this line `cargo build` there errors with "current package
   believes it's in a workspace when it's not".

With both in place, `cd vendor/nanofix && cargo build --release --lib` compiles clean from a
`rm -rf target` in 3.74s — no `cmake`, no Aeron. `cargo build --release` (no `--lib`) still fails
at link: `src/bin/aeron_demo.rs` and `aeron_media_driver.rs` call real Aeron driver functions and
are pulled in by default as `src/bin/*.rs` binaries — sidestepped by building only `--lib` and the
example below, never the crate's own bins.

### The example acceptor, and a defect it had to route around

`vendor/nanofix/examples/fixbolt_w2w_acceptor.rs` (gitignored, not this repository — CLAUDE.md §2
rule 9). CompIDs and `BeginString` match exactly what `tools/w2w/src/main.rs`'s `--connect` half
sends as its Logon (`tools/w2w/src/main.rs`, fn `logon`: `49=W2W`, `56=ISLD`,
`8=FIX.4.4`) — so the acceptor's own SenderCompID is `ISLD`, and it whitelists `W2W` as the only
TargetCompID.

**It does not call `nanofix::server::FixServer::start`.** That convenience wrapper's own
`handle_connection` (`src/server.rs:132-235` in the pinned commit) reads the inbound Logon by
hand, purely to learn the peer's CompID before a `Session` exists for it, and never runs that
Logon's `MsgSeqNum` through `Session::validate_inbound_seq`. `Session::new` starts
`inbound_seq_num` at 1 regardless (`src/session.rs:125-131`) and `validate_inbound_seq` never
advances a counter that is behind the received sequence (`src/session.rs:271-286`) — it sends a
`ResendRequest` instead. So with `FixServer::start`, `inbound_seq_num` stays parked at 1 forever,
and **every single message after the Logon reads as a sequence gap.** Confirmed on this loopback
acceptor with a raw-socket probe (`python3` script, not committed): each reply to a `TestRequest`
arrived as a `ResendRequest` (`35=2`, `7=1`, `16=<n>`) immediately followed by the real
`Heartbeat` (`35=0`) — and on the first `tools/w2w --connect --path admin` run, the strict `35=`
assertion in `tools/w2w/src/main.rs`'s `measure` read the `ResendRequest` and failed with `expected 35=0, got 35=2`.

**No nanofix source was changed to fix this.** The example instead builds its own accept loop
from nanofix's lower-level public pieces — `Acceptor`, `Session`, `FixEngine`,
`StdTcpTransport` — the same ones `FixServer::start` itself is built from, and adds one call
`FixServer` omits: after reading the Logon by hand, `session.on_gap_filled(logon_seq + 1)` primes
the inbound counter past it, using `Session::on_gap_filled` exactly as its own doc comment says
("complete a resend"), before the session is handed to `FixEngine::new_acceptor`. That is a
different call sequence through the same `pub` surface, written in this file, not a patch to
`nanofix`'s code.

`TestRequest -> Heartbeat` (`--path admin`) needed nothing else: nanofix's own engine dispatches
`MsgType=1` itself, session-level, before `FixApp` is ever consulted (`src/engine.rs:601-611`).
`NewOrderSingle -> ExecutionReport` (`--path app`) goes through `FixApp::on_message`, the one
message type nanofix's dispatch loop hands to application code (`src/engine.rs:669-688`); the
reply is built with nanofix's own `serializer::build_execution_report` helper, echoing the
order's ClOrdID (tag 11) and setting ExecType (tag 150) to `F`, which is exactly what
`tools/w2w/src/main.rs`'s `measure` checks for `--path app` (the `Path::App` branch after the
`35=` assertion).

**Both paths are comparable, not admin-only.** The A7 brief allowed for an admin-only comparison
if the API gave no application reply; here it does.

### The gate run — not a figure

```
target/release/w2w --connect 127.0.0.1:<port> --path admin --messages 100 --warmup 10
target/release/w2w --connect 127.0.0.1:<port> --path app   --messages 100 --warmup 10
```

against `vendor/nanofix/target/release/examples/fixbolt_w2w_acceptor <port>` on loopback: Logon
accepted both times, 100 of 100 replies correct (`35=0` for admin, `35=8` with a matching ClOrdID
and `150=F` for app), `allocs 0` on the generator thread's timed window, both processes exit 0.
**The ns figures `w2w` printed are not quoted here and must not be read as a measurement** — this
step's own brief says it measures nothing, this was a laptop-adjacent desk with no §9 mitigations
and no fixed mode declared, and CLAUDE.md §2 rule 10 requires the machine and the §9 settings
beside any number that is meant to be one. **No figure yet — boot B, row B8** measures both
engines with `check-machine.sh` and reports it there, in `standard` mode (nanofix is
thread-per-connection with a blocking `recv`, `src/server.rs:117` / `src/transport_tcp.rs:23-37`
in the pinned commit — pairing it with `hft` would be the mode-mixing non-negotiable 4 forbids).

### Traps this step was watching for (plan's *Bẫy đã lường trước*)

- **A patched nanofix is not nanofix.** Both patches above are named, by file and effect, beside
  the pinned commit hash — and the `on_gap_filled` workaround is emphasised as *not* a source
  patch for the same reason: a number from any of this must carry that description next to it,
  not just the commit hash.
- **`nanofix` refuses the Logon, or has no app reply.** Neither happened — recorded above so the
  next reader does not re-discover it.
- **`hft` compared against a blocking engine.** B8 is scoped to `standard` only; see above.

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

`[measured 2026-09-05]` `size_of::<Connection<Loopback, Acceptor, MemJournal<64,512>, 64, 4096,
8192>>()` = **21 456 bytes, 20.95 KiB**, of which `Session<Acceptor,64>` is **9 064 B**. The
shape `tools/w2w` actually runs — `Store`, `N = 256`, `RX = 4096`, `TX = 8192` — is
**23 760 bytes, 23.2 KiB**. **`L1d` on this machine is 32 KiB, so one connection does fit in
L1**, with room left over.

> **This paragraph said something else for a day, and the something else was measured.**
> `[measured 2026-08-30]` the same expression read **54 600 bytes, 53.3 KiB**, with
> `MemJournal<64,512>` supplying 33 288 B of it, and this section was built on the sentence
> *"one connection does not fit in L1"*. Both were true when they were written.
> [ADR-0046](../decisions/ADR-0046-the-ring-is-the-resend-store-and-a-replay-goes-in-batches.md),
> commit `6d02f3a` on **2026-09-04**, made the ring `Box<[Slot<LEN>]>` and raised its default to
> 4 096 slots. `size_of::<MemJournal<64,512>>()` went from **33 288 to 32**, because the slots
> left the struct — and nothing pointed at this paragraph to say so. A correct measurement
> invalidated by a refactor a week later looks exactly like a correct measurement.
>
> **The memory did not go away, it moved.** `Store` is `MemJournal<4096, 512>` = **2 MiB per
> connection, on the heap**, addressed by `seq % 4096` so consecutive messages walk it end to
> end. What that costs is measured below rather than assumed.

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

### How much a message touches — measured, and it is a ramp rather than a wall

`[measured 2026-09-05]` `crates/engine/benches/density.rs`, §9 desktop, median of 20 clean runs
out of 22, `pass 12 fail 0 unknown 1`. N logged-on sessions on one engine, each handing over one
`NewOrderSingle` per turn, the journal ring pinned small so it does not join in. Per message is
per turn divided by N.

| N | ns per turn | **ns per message** | vs N=1 | working set |
|---|---|---|---|---|
| 1 | 1 659.8 | 1 659.8 | 1.000 | 26 KiB |
| 2 | 3 325.4 | 1 662.7 | 1.002 | 51 KiB |
| 4 | 6 686.0 | 1 671.5 | 1.007 | 102 KiB |
| 8 | 13 514.5 | 1 689.3 | 1.018 | 204 KiB |
| 16 | 27 369.5 | 1 710.6 | 1.031 | 408 KiB |
| 32 | 56 684.4 | 1 771.4 | 1.067 | 816 KiB, past the L2 edge |
| 64 | 121 009.1 | 1 890.8 | **1.139** | 1.63 MiB |

**There is no wall. There is a ramp, and the shape is the answer.** If a message touched most of
its connection, the 512 KiB L2 edge would arrive at N ≈ 20 and put a **step** there. At N=16 the
cost is up 3.1% and at N=32 up 6.7%, smoothly, with nothing at the edge. A smooth climb is what
a **small** touched set inside a **large** allocation looks like: only the touched lines compete,
so the miss rate rises gradually instead of falling off a cliff.

Turning the excess into bytes, with the latency table above: at N=64 a message costs **231.0 ns**
more than at N=1; about 69% of a 1.63 MiB working set cannot sit in a 512 KiB L2, and the L2-to-L3
step on this machine is ~8.6 ns. That is ~39 lines, **~2.5 KiB**. The same arithmetic at N=32 gives
~2.2 KiB. The model is rough — at N=16 it predicts no excess and 50.8 ns is measured, because L2
is shared with code and stacks and is not fully associative — so the honest figure is **on the
order of 2 to 4 KiB, and certainly not the 21 KiB of the structure**.

**So the old lower bound is the one that survived.** This section used to offer two, from the
withdrawn 53.3 KiB: the L2 edge at **N ≈ 9** if a message touched all of it, or **N ≈ 128** if it
touched about 4 KiB. The first is refuted by the absence of a step. Recomputed on what is
measured — ~2.5 KiB touched against a 512 KiB L2 — the edge is nearer **N ≈ 200**, and by N=64,
which is already a dense engine, cache alone has added **13.9%** to every message.

That 13.9% is the smallest of the three terms on this page. The linear sweep and the head-of-line
blocking below are both larger, and neither is helped by knowing this.

### And the ring, which is 2 MiB, costs nothing measurable

`[measured 2026-09-05]` one session, identical work, the journal ring swept from 8 slots to
4 096 — 4 KiB of heap to 2 MiB:

| Ring | ns per turn |
|---|---|
| 8 slots (4 KiB) | 1 659.8 |
| 64 slots | 1 635.5 |
| 512 slots | 1 654.8 |
| 4 096 slots (2 MiB) — `Store`, the default | 1 657.7 |

A spread of 1.5% **and not monotone**, which is the signature of no effect at all rather than a
small one. `crates/engine/benches/journal.rs` says the same from the other direction: a `put` of
a 191-byte `ExecutionReport` walking the whole ring costs **8.9 ns**, and pinned to one slot
**6.3 ns**.

The reason is that a `put` writes 191 bytes into three cache lines at a perfectly predictable
512-byte stride, and both the prefetcher and the store buffer are good at exactly that. **A large
allocation is not a cache cost; a large *touched set* is.**

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

## The slot cursor: predicted −36 ns, measured +5, and the rate that did not extrapolate

`[measured 2026-08-31]` Same container as the section above, `pass 2 fail 6 unknown 3`.
Same-machine A/B, **30 runs per arm**, medians.

The section above put a tag comparison at **~0.4 ns** and the 105 of them at **~42 ns**, and
concluded that a forward cursor — try the position after the previous hit, fall back to the
full scan — would remove ~91 of those comparisons and save **~36 ns**.

It was written, it passed every correctness gate, and it is **slower**:

| arm | median | low mode (n) | high mode (n) |
|---|---:|---:|---:|
| `find` (baseline) | **154.6** | 151.3 (16) | 183.4 (14) |
| forward cursor | **159.8** | 155.8 (17) | 187.8 (13) |
| **delta** | **+5.2 ns (+3.4%)** | **+4.5** | **+4.4** |

This box is bimodal — the same shape as the 324 ns mode elsewhere in this file — so the
comparison is made **within each mode as well as on the pooled median**. The two within-mode
deltas agree to **0.1 ns** and both agree in sign with the pooled one. It is a reproducible
regression, not noise.

### Why, and the correction it forces

**The 0.4 ns per comparison was measured on scans of 78 elements and extrapolated down to 14.
That extrapolation does not hold.** The padding arms ran 217 to 1001 comparisons; the rate at
which a long scan burns time says nothing about a 14-element scan over data that is hot,
16 bytes per element, and perfectly predicted. A short scan is superscalar; the cursor replaces
it with a data-dependent branch and a loop-carried `usize`, and the optimiser has less to work
with, not more.

**So the "~42 ns is the scan" figure in the section above is an over-estimate, and this is the
measurement that says so.** The honest reading of both together: the scan's true share at 14
slots is **smaller than 42 ns and is not separable by the instruments used here** — what *is*
established is that removing it by this method costs 3.4% rather than saving 24%.

The section above's conclusion survives and is strengthened: **the scan is not where the 60 ns
target is lost.** The fixed cost (~31 ns before a single variable field is written) and `put`
(~7 ns per field) are still the terms that matter, and neither was touched.

### What was kept

`crates/codec/tests/slot_order.rs` — six cases holding that the caller's ordering never reaches
the wire, which the body path had **no** guard for before this. `tests/group_roundtrip.rs` had
it for fields inside a group and nothing had it for the body. Proven by reversal twice: taking
the caller's slots in supplied order turns four of six red on wrong bytes; deleting the
cursor's fallback scan turned the same four red on dropped fields. The guard outlives the
change that prompted it.

`[to testing-skills]` — *a rate measured in one regime and extrapolated into another.* The
padding experiment was sound and its number was right **for the regime it measured**; the error
was carrying 0.4 ns/comparison down to a scan an order of magnitude shorter, where the hardware
behaves differently. The cheap defence is to state the range your rate was measured over next
to the rate itself, and to treat any use outside that range as a prediction to be tested rather
than a figure to be quoted. Here the prediction was −36 ns and the measurement was +5, and
**only building the thing found it** — no amount of re-reading the estimate would have.

## The instrument was 80% of its own smallest reading — 2026-08-31

> **`[corrected 2026-09-01]` THIS SECTION'S CONCLUSION IS WRONG.** The 1.3 ns was the
> optimiser deleting a 163-byte copy, not a faster instrument, and the doubling test below
> passes either way because doubling the calls doubles whatever fraction survives. The old
> harness was **preventing** the elision, not adding overhead. Kept unedited because the
> reasoning is the point —
> [a-benchmark-can-delete-its-own-work.md](a-benchmark-can-delete-its-own-work.md) has the
> refutation and the corrected numbers.

`DESIGN.md` §6's timing gates were moved from absolute targets to per-machine baselines
([ADR-0016](../decisions/ADR-0016-per-machine-baselines-replace-absolute-targets.md)). The
change to `crates/codec/benches/harness.rs` was mechanical: `Suite::bench` lost its
`ceiling_ns: f64` parameter, because the limit now comes from `benches/baselines.tsv` instead
of from a constant at the call site. **The timed loop was not touched — not one byte.**

`inline deliver + reply` then read **1.3 ns** where the same case had read **6.3 ns** minutes
earlier, reproducibly, three runs each side.

### The check that mattered

A benchmark that gets 5× faster when a function signature changes has, on the face of it,
stopped measuring. The obvious suspicion is that the optimiser deleted the work. That was
tested rather than argued about: the closure was made to call `deliver` **twice** per
iteration, changing nothing else.

| | ns/op |
|---|---|
| one `deliver` per iteration | 1.3 · 1.3 · 1.3 |
| two `deliver` per iteration | 2.6 · 2.6 · 2.6 |

An exact factor of two, three times running. The work is real and the 1.3 ns is real. What had
been wrong was the **6.3 ns**.

### What the old number was made of

So roughly **5 of the old 6.3 ns were the harness**, not the engine — the instrument was about
80% of its own smallest reading. The most likely mechanism is inlining: `Suite::bench` is
generic over the closure and monomorphised per call site, and the three-argument shape left the
closure behind an indirect call in a loop whose body is a handful of instructions.

The evidence for that reading is not the mechanism, which was not confirmed, but a side effect
that was. Across 21 runs the old harness put this case in **three discrete clusters** —
6.3–6.4 (13 runs), 7.4 (4 runs), 8.1–8.3 (4 runs), with **nothing in between**. Across 30 runs
of the new harness it reads **1.3 ns on every single run**, spread 1.000. The trimodality
belonged to the measuring apparatus and vanished with it.

**The two ring cases did not change.** `ring, one way` still draws its unexplained second mode
at +24% — 2 runs in 21 with the old harness, 2 in 30 with the new. So the harness was hiding a
small case entirely while leaving a large one alone, which is what an additive overhead does.

### It was written down before it was measured

The constant that was deleted carried this doc comment, added 2026-08-30:

> Baseline 2.5–4.9 ns across runs. The spread is the measurement's, not the code's: at this
> size the loop is a handful of instructions and **the harness's own overhead is the same
> order.**

That is the finding, stated correctly, a day early, by somebody who did not then measure it.
`CLAUDE.md` §4 already says prose does not hold a constraint and a comment asserting runtime
behaviour must name the thing that proves it. This is what happens when it does not: the
sentence sat beside the number it invalidated, and the number went into `DESIGN.md` §6 anyway.

`[to testing-skills]` — *the instrument was most of the measurement, and a comment beside it
said so and was never checked.* Two transferable parts. **First**, when a figure moves sharply
after a change that should not have touched it, the question is not "which change caused this"
but "does this case still measure its own work" — and that is answered by **scaling the work
and checking the response is proportional**, which costs one edit and settles it in one run.
**Second**, a benchmark whose readings fall into discrete clusters with nothing between them is
reporting a property of the *harness or the environment*, not a distribution of the code's
cost; continuous noise is noise, quantised noise is a mode, and a mode has a cause worth
finding. Here the cause was the instrument, and finding it removed 80% of the reading.

## Two ways a benchmark loop poisons its own machine check — 2026-08-31

Both found while recording the ADR-0016 baselines, and both produced numbers that looked fine.

**A baseline must be taken through the path that will judge it.** The first characterisation
ran the four timing bench targets directly, 30 times, and `encode ExecutionReport (template)`
never came within 3% of its 1.10 limit. The very next `scripts/bench.sh` run put it **over**.
`bench.sh` runs eight targets rather than four, so the case is not measured in the same machine
state that will grade it. A baseline recorded through a shorter path is a baseline for a
different experiment.

**Back-to-back runs make the machine dirty for the next run's own quietness check.** With no
gap between iterations, `scripts/check-machine.sh` — which samples CPU over one second at the
start of `bench.sh` — read **25%, 36%, 19%, 31%** busy, against 0–2% for the same box measured
by hand. The previous suite was still inside the sampling window. Every one of those runs
correctly disqualified itself under non-negotiable 10, so nothing false was published; but a
loop that reports its own load rather than the machine's will disqualify **all** its runs, and
if the gate had merely warned instead of failing, all of them would have been kept.

`[to testing-skills]` — *the measurement loop was part of the system under measurement.* The
general shape: any harness that samples the environment to decide whether a run is valid must
leave the environment enough time to return to the state it is sampling for, and any baseline
must be recorded through the **whole** invocation path that will later judge it, not a
convenient subset of it. Both failures are invisible in the numbers themselves — the figures
look ordinary — and are only visible in the *validity* column beside them, which is an argument
for having one.

## A rate extrapolated across a cache boundary, and the second time this shape appeared — 2026-08-31

ADR-0011 raised the ring's default capacity from 64 KiB to 4 MiB and stated the resulting slack
as **"roughly 3.6 ms"**. Nothing had measured 3.6 ms. It was **47.7 µs, measured at 64 KiB,
multiplied by 64.**

The ADR was honest about it — a revision note put the true value somewhere in **1.6–3.6 ms** and
said it should be read as an order of magnitude rather than a measurement. What then happened is
the ordinary way a caveat dies: four other documents picked the number up, and one of them
(this author, the same day) attached a **`[measured]`** tag to it. A figure that had been
carefully labelled as derived became, three files later, a measurement.

### Measuring it

`crates/engine/benches/ring_full.rs` now fills **both** capacities and prints both, so the
scaling is visible rather than assumed. Four runs, §9 desktop, `check-machine.sh` `pass 10
fail 0 unknown 1`:

| Capacity | Messages held | Time to fill | Per message |
|---|---|---|---|
| 64 KiB | 352 | 47.7–48.2 µs | 135–136 ns |
| 4 MiB | 22 550 | **5.05–5.36 ms** | **223–237 ns** |

The real figure is **above the entire 1.6–3.6 ms range** the ADR allowed for — about **48% more
slack** than the headline extrapolation, and the ratio is **106–112×** for 64× the capacity.

### Why, and it is the interesting half

**The per-message cost nearly doubles: 135 → ~230 ns.** 64 KiB fits in L2 on this machine and
4 MiB does not, so every write to the larger ring goes further out in the hierarchy. A ring that
fills *more slowly* takes *longer* to overflow, so the application gets *more* time, not less.

The extrapolation assumed the rate was a property of the code. It is a property of the code
**and the working set**, and the capacity change moved the working set across a cache boundary —
which is precisely the thing the multiplication could not see.

**Nothing in ADR-0011's decision changes**, and its margin is larger than it claimed. That is
worth stating plainly: this correction found the design in better shape than the document said,
which is the direction people do not check.

### The same shape, twice in one day

This is the second instance in this file of **a rate measured in one regime and carried into
another**. The first was the serialise slot scan: 0.4 ns per tag comparison, measured over a
78-element scan and extrapolated down to a 14-element one, where short scans on hot data behave
differently — it predicted −36 ns and measured **+5.2**. Here the extrapolation went the other
way across a cache boundary instead of a branch-prediction one, and was wrong by 48%.

`[to testing-skills]` — this reinforces the existing case rather than adding one: *state the
range a rate was measured over, next to the rate*, and treat any use outside that range as a
prediction to be tested. The addition worth making is the **tell**: both failures crossed a
boundary in the machine rather than in the code — a scan short enough for the predictor, a
buffer small enough for the cache. **When the parameter you are scaling is also the thing that
decides which hardware regime you are in, the scaling factor is not a factor.** And both were
settled the same cheap way: measure the second point instead of computing it.

---

## `std::sync::mpsc::try_recv` makes no syscall at all — 2026-08-31

Step 4 of [threads-and-affinity](../plans/2026-08-30-threads-and-affinity.md) has to move an
accepted socket from an acceptor thread to the engine thread that will own it. The obvious
carrier is `std::sync::mpsc`, and the obvious worry is that `Receiver::try_recv` takes a lock —
which on the engine thread would be a `futex`, and `CLAUDE.md` §2 non-negotiable 4 makes that a
bug rather than a slow path.

**Measured rather than reasoned about**, because the answer decides a design and the source is
not the evidence the rule asks for. AMD Ryzen 7 3700X, Linux 7.0.0-30-generic, rustc 1.98.0,
`--release`. One thread spins on `try_recv` two million times while another sends five values;
`strace -f`, syscalls attributed by tid to the spinning thread, between two markers it writes
itself:

```
syscalls between the markers:   (none)

the same thread over the WHOLE run, setup and teardown included:
  3 sigaltstack   2 write   2 rt_sigprocmask   2 munmap   2 mprotect
  2 mmap          1 set_robust_list  1 sched_getaffinity  1 rseq
  1 madvise       1 gettid  1 exit
```

Two million calls on the empty path, five on the non-empty path, **zero syscalls**. Every entry
in the second list is thread start-up or exit and every one of them falls outside the marked
region.

**Why the second list is in this note.** An empty result is exactly what a broken measurement
also produces: a marker that never matched, a tid read wrongly, a trace that captured the wrong
process. The whole-run count is what separates *the thread made no syscalls here* from *the awk
matched nothing* — and it was checked, not assumed. `CLAUDE.md` §10: a green result that was
inferred rather than observed is not a result.

**What it settles:** the shard runtime can hand sockets to engine threads over
`std::sync::mpsc` and drain them with `try_recv` without a blocking call. **What it does not
settle:** the *sending* side, which runs on the acceptor thread and is allowed to block anyway;
and whether this holds on another libstd version, since none of it is a documented guarantee.

`[to testing-skills]` — **"no events were recorded" and "the recorder was not running" look
identical.** Any measurement whose success case is an *absence* — no syscalls, no allocations,
no queries, no log lines — needs a second count from the same instrument that is expected to be
non-zero. Here the same `strace` output, the same tid filter, over a wider window: 19 syscalls,
so the filter demonstrably works and the empty window is a fact about the code. Without it the
transcript reads the same whether the code is clean or the trace is empty.

---

## The isolated core is 36% slower at the one thing the engine does most — 2026-08-31

`DESIGN.md` §9 tells you to give the engine an isolated core: `isolcpus`, `nohz_full`,
`rcu_nocbs`. D8's model is one non-blocking `read` per connection per turn, and the section
above measured that syscall at **703 ns**, calling it the largest single term in the design.

**Nobody had measured the two against each other.**

`[measured 2026-08-31]` AMD Ryzen 7 3700X, Linux 7.0.0-30-generic, `check-machine.sh` reading
`pass 10 fail 0`, `crates/engine/benches/turn.rs`, `Engine::turn` over real TCP sockets:

| Core | `isolcpus`? | L3 domain | turn, 1 session | turn, 16 sessions | per session |
|---|---|---|---|---|---|
| `cpu0` | no | 0 | **498.5 ns** | 8143.2 ns | 509.0 ns |
| `cpu5` | no | 1 | **497.4 ns** | 8083.4 ns | 505.2 ns |
| `cpu6` | **yes** | 1 | **680.1 ns** | 10875.6 ns | 679.7 ns |
| `cpu7` | **yes** | 1 | **671.6 ns** | 10752.2 ns | 672.0 ns |

**+36% on the isolated cores.**

### It is not the L3 domain, and that is the point of the table

This CPU has two L3 domains, `cpu0-3` and `cpu4-7`. `cpu5` and `cpu6` are **in the same one**
and differ by 36.7%; `cpu0` and `cpu5` are in **different** ones and differ by 0.2%. The
placement variable is isolation, not cache. `CLAUDE.md` §10: *a cause accepted because a knob
moved with it* is the failure this table is built to avoid, so the arms were chosen to move one
thing.

**Which of the three isolation options it is was not separated**, and this page does not guess.
They were applied to the same set of CPUs by one kernel command line, and separating them needs
a reboot with a different one. The documented mechanism is `nohz_full`: full dynticks turns on
kernel context tracking, which runs on **every** kernel entry and exit — and this workload is
nothing but kernel entries and exits. That is a hypothesis with a named mechanism, not a
measurement, and it is labelled as one.

### What it does to §8's number, and to §9's advice

The **703 ns** in the section above was measured *pinned to an isolated core* — its own text
says so. Matched for placement, the two readings agree: 672–680 ns here for a whole
`Engine::turn`, against 703 ns there for a bare C `read`. The remaining gap is 4%, between two
different programs on two different days, and its sign is the wrong way round by 31 ns, which
is unexplained and is smaller than anything this page can currently resolve.

So the honest statement is not *"the engine got faster"*. It is:

> **On an ordinary core this engine turns a session in ~500 ns. On the core `DESIGN.md` §9
> recommends, it takes ~675 ns.** §9 buys jitter isolation and pays 36% of the dominant term for
> it, and until 2026-08-31 the trade was neither measured nor stated.

Whether that trade is worth it is a question about **jitter, which this table does not
measure**. A tail that isolation removes could easily be worth 175 ns of median. Nothing here
argues either way; what it removes is the assumption that isolation is free.

### What a turn costs beyond the syscall it is made of

Measured in the same binary and the same run, so the subtraction is not across programs:

```
recv on a quiet socket                474.6 ns        469.4 ns      (two runs)
engine turn, 1 idle session           505.2 ns        497.9 ns
engine turn, 4 idle sessions         2012.0 ns       1994.2 ns      (503.0 / 498.6 per session)
engine turn, 16 idle sessions        8162.3 ns       8064.2 ns      (510.1 / 504.0 per session)
```

**`Engine::turn` adds about 30 ns per session over the `read` it is made of**, and the syscall
is **94%** of it. Flat from 1 to 16 sessions to within 2%, which is the property 703 ns was
published with and which now holds for the engine rather than for a floor.

Set against this project's own numbers: `parse NewOrderSingle (validated)` is 122.6 ns on this
machine. **The sweep's own work is a quarter of a parse; the syscall that discovers there is
nothing to parse is four of them.**

### The generalisation

`[to testing-skills]` — **a setting recommended for performance, never measured against the
operation it changes.** The recommendation and the hot operation were both written down here,
in the same document, for a day: §9 says isolate the core, §8 says the dominant cost is a
syscall, and nothing had put a stopwatch on the two together. The measurement took four runs of
an existing benchmark under `taskset` and reversed a piece of advice.

The shape is not specific to CPUs. It appears wherever a configuration is adopted for a *class*
of benefit — isolation, a bigger cache, a stricter isolation level, a safer allocator, a
retry-heavy client — and the cost lands on a *specific* operation nobody re-times afterwards.
Two properties make it survivable: the setting is applied once and lives in an environment
rather than in code, so no diff shows it; and the benefit it buys is real, so questioning it
feels like questioning the goal.

The cheap defence is the one used here: **when you adopt a setting for performance, measure the
single operation your design says dominates, with and without it, changing nothing else.** One
variable, two arms, and a third arm that differs in the *other* plausible way — here `cpu0`
against `cpu5`, which is what makes "it is not the cache" a measurement rather than a claim.

---

## The answer: it is `nohz_full`, and it loses until p99.99 — 2026-08-31

The section above could not say which of `isolcpus`, `nohz_full` and `rcu_nocbs` cost 36%,
because one kernel command line applied all three to the same CPUs. This is the reboot that
separated them, and the second half — what the setting *buys* — which had never been measured
either.

### The design: three flags, three CPUs, one boot

Not one flag per boot. **Different CPUs get different flags in the same boot**, so the arms
share the kernel, the temperature, the load and the session, and the subtraction between them
is not a subtraction across two days:

```
isolcpus=4,6,12,14  rcu_nocbs=7,15  nohz_full=4,12  processor.max_cstate=1
```

`cpu4` also takes `isolcpus`, deliberately. `nohz_full` only stops the tick on a CPU with **at
most one runnable task**; without `isolcpus` the scheduler keeps putting work there, adaptive
tick flickers, and the arm can read *free* because the mechanism never engaged rather than
because it costs nothing. That is a false green with a plausible story attached, and
`isolcpus` is what holds the precondition still.

`nohz_full` cannot be tested alone at all — the kernel adds `rcu_nocbs` to any CPU that has it —
so it is reached by subtracting the two arms that turn out to be free. Four arms, not two, for
that reason.

### What it costs

`[measured 2026-08-31]` AMD Ryzen 7 3700X, Linux 7.0.0-30-generic, SMT off, `performance`,
turbo off, `check-machine.sh` `pass 10 fail 0`, machine quiet at 0%.
`scripts/measure-isolation-cost.sh`, and `crates/engine/benches/turn.rs` under `taskset`:

| Core | Flags | user-space loop | bare `getpid` | `Engine::turn`, 1 session |
|---|---|---|---|---|
| `cpu5` | none | 1.0578 ns/iter | 198.87 ns | 501.8 ns |
| `cpu6` | `isolcpus` | 1.0574 | 198.94 ns | **494.8 ns** |
| `cpu7` | `rcu_nocbs` | 1.0578 | 198.86 ns | 498.2 ns |
| `cpu4` | `isolcpus` + `nohz_full` | 1.0546 | **354.76 ns** | **670.7 ns** |

**`nohz_full` is the whole 36%.** `isolcpus` and `rcu_nocbs` are free, and the `isolcpus`-only
core is the fastest of the four. Four interleaved repetitions: `cpu4` held 352.96–354.76 and
the other three 198.86–199.04.

Two things it is **not**, and each is ruled out by a reading in the same run:

- **Not the clock.** The user-space loop — a dependent multiply-add chain that never enters the
  kernel — agrees across all four cores to 0.3%, and the `nohz_full` core is the *faster* one.
- **Not interrupts.** The `nohz_full` core takes **3743 fewer** local timer interrupts per
  second than the others and is still 78% slower per kernel entry. The sign is the argument.

What remains is the kernel entry and exit path, which is where full dynticks runs its context
tracking — `CONFIG_CONTEXT_TRACKING_USER=y`, `CONFIG_VIRT_CPU_ACCOUNTING_GEN=y` in this kernel.
The mechanism named as a hypothesis in the section above is the one that survived.

### What it buys

`nohz_full` is bought for jitter, so stopping at the median would repeat the error this page
was written to record. Every call timed individually, 5 000 000 per core,
`measure-isolation-cost.sh --jitter`:

| Core | Flags | p50 | p99 | p99.9 | p99.99 | calls > 1 µs | timer ticks |
|---|---|---|---|---|---|---|---|
| `cpu5` | none | 216 | 224 | 240 | 3720 | 1130 | 1283 |
| `cpu6` | `isolcpus` | 216 | 224 | **224** | 2848 | 1078 | 1281 |
| `cpu7` | `rcu_nocbs` | 216 | 224 | 240 | 3440 | 1120 | 1281 |
| `cpu4` | `isolcpus` + `nohz_full` | 376 | 376 | 384 | **504** | **2** | **2** |

**The count of calls over 1 µs tracks the timer interrupt count call for call** — 1130/1283,
1078/1281, 1120/1281, and 2/2. The tail is not merely *correlated with* a knob that moved
alongside it; it is the tick, and the two counters say so independently.

**And then the row reads left to right.** `nohz_full` is worse at p50, worse at p99, and
**worse at p99.9** — 384 ns against 224. It wins from **p99.99 outward**, where it is genuinely
good: 504 ns against 2848. `max` does not follow it — over four runs `cpu4`'s worst call was
852, 2966, 11582 and 14107 ns — so the rare large excursion is not what it removes.

### The arithmetic that decided it

> A turn is ~500 ns, so a busy `hft` engine performs ~2 000 000 kernel entries per second per
> core. `nohz_full` taxes **every one** of them 160 ns and removes **~1100 excursions of 3 µs**.
> 0.32 s of tax per second against 0.0033 s of tail: **a hundred to one against.**

[ADR-0021](../decisions/ADR-0021-nohz-full-leaves-section-9.md) takes `nohz_full` out of §9's
recommendation and prices it instead. `isolcpus` and `rcu_nocbs` stay — free, and kept for a
mechanism about *other tenants* that a quiet machine cannot exercise. That last point is
labelled rather than implied: on this box `isolcpus` removed 1078 excursions against 1130, which
is nothing, because there was nothing there to remove.

### Two near-misses, both worth more than the result

**`scaling_cur_freq` lies on a `nohz_full` core.** It read **2 240 000 kHz** for `cpu6` — exactly
that core's `scaling_min_freq` — while `cpu6` was at 100% load and `cpu5` read 3 792 929. The
ratio, 1.69, is a tidy and completely wrong explanation for a 1.36 slowdown, and it was the first
thing found. `amd-pstate-epp` updates that value from a path tied to the tick, and `nohz_full`
had stopped the tick. **The user-space loop is what refuted it**: the same core, the same run,
the same speed as everyone else.

`[to testing-skills]` — **an instrument that reports a plausible cause can be downstream of the
thing under test.** The frequency counter was not merely inaccurate; it was inaccurate *because
of the setting being investigated*, and it failed toward an explanation that fit. The defence is
a second reading through a different mechanism — here, wall-clock time for a fixed amount of
work, which needs no cooperation from the kernel at all.

**The guard against a false green was itself a false green.** The tick counter was added
specifically so a `nohz_full` arm that failed to engage could not read as *isolation is free*.
Its first version used `awk '/^LOC:/'`, and `/proc/interrupts` right-aligns its first column, so
the line begins with a space and the pattern matched nothing: it printed a delta of **0 for every
core**, on a boot where two cores were ticking three million times a run. A constant-zero column
reads as *everything is tickless* — the reassuring answer — and means *nothing was read*.

`[to testing-skills]` — **a guard reporting the same value everywhere is reporting nothing, and
"all clear" is the disguise it wears.** It was caught only because the value was *expected to
differ between arms*: a guard whose readings must vary is falsifiable, one that only ever prints
"fine" is not. The fix was `$1 == "LOC:"`, and the corrected column reads 1281 against 2.

---

## `nohz_full` taxes the cores that do NOT have it — 2026-08-31

The section above measured `nohz_full` at +155 ns per kernel entry **on the core carrying it**,
and [ADR-0021](../decisions/ADR-0021-nohz-full-leaves-section-9.md) took it out of §9 on that
basis. Rebooting into the new §9 line found the cost is bigger than that, and lands somewhere
nobody was looking.

`[measured 2026-08-31]` `cpu5` carried **no isolation flag in any of the three boots**:

| Boot | `nohz_full` | `cpu5` bare `getpid` | `cpu5` `Engine::turn`, 1 session |
|---|---|---|---|
| §9 as it was | `6,7,14,15` | 198.36–199.04 ns | ~501 ns |
| the four-arm experiment | `4,12` | 198.87 ns | 501.8 ns |
| §9 after ADR-0021 | **none** | **154.62 ns** | **455.7 ns** |

**Naming any CPU in `nohz_full` costs ~44 ns per kernel entry on *every* CPU**, on top of the
~155 ns paid by the CPUs that are actually in the list. Three independent readings agree on the
size of it: bare `getpid` +44 ns, `recv on a quiet socket` +47 ns, `Engine::turn` +46 ns. That
is the shape of **one fixed cost per kernel entry**, not the shape of a workload. And
`user_loop` did not move — 1.0577 against 1.0578 — so once again it is not the clock.

The documented mechanism is that `NO_HZ_FULL` switches the kernel to context tracking and
`VIRT_CPU_ACCOUNTING_GEN` **system-wide** as soon as any CPU uses it, changing the user↔kernel
transition path for all of them. **That mechanism is not verified here.** It is the explanation;
the numbers are the measurement.

### The prediction that made it falsifiable

Re-recording the baselines was the chance to test the explanation rather than illustrate it, so
the prediction went into the plan **before** the runs: only the four syscall-bound cases should
move; the twelve pure user-space cases should not. **If `parse NewOrderSingle` had moved 10% as
well, the explanation was wrong** and something other than `nohz_full` had changed between the
boots.

24 qualifying runs of `scripts/bench.sh --strict`, one machine, one boot:

| Case | old baseline | new median | change |
|---|---|---|---|
| `walk 1 group, 2 entries, 2 members` | 58.7 | 59.2 | +0.8% |
| `walk 4 levels, 61-tag member list` | 352.9 | 350.1 | −0.8% |
| `group_members contains, 61 tags` | 9.7 | 9.6 | −1.0% |
| `encode 1 group, 2 entries` | 108.4 | 113.6 | +4.8% |
| `parse NewOrderSingle (validated)` | 122.6 | 121.8 | −0.7% |
| `parse NewOrderSingle (no checks)` | 117.0 | 115.3 | −1.5% |
| `parse Heartbeat (validated)` | 57.3 | 57.9 | +1.0% |
| `encode ExecutionReport (template)` | 239.1 | 237.6 | −0.6% |
| `SendingTime from the cache` | 4.9 | 4.9 | 0.0% |
| `inline deliver + reply` | 1.3 | 1.3 | 0.0% |
| `ring, one way` | 267.4 | 262.8 | −1.7% |
| `ring, round trip` | 515.7 | 513.0 | −0.5% |
| **`recv on a quiet socket`** | 470.9 | **420.5** | **−10.7%** |
| **`engine turn, 1 idle sessions`** | 500.3 | **448.9** | **−10.3%** |
| **`engine turn, 4 idle sessions`** | 2002.9 | **1807.1** | **−9.8%** |
| **`engine turn, 16 idle sessions`** | 8139.4 | **7333.5** | **−9.9%** |

**Twelve cases inside their own noise with no direction; four cases down together by a tenth.**
The largest non-syscall move, `encode 1 group` at +4.8%, is the wrong sign for anything
systematic and sits barely outside that case's own 3.8% spread.

`benches/baselines.tsv` takes the four new figures at n=24, margin 1.10 (max/median 1.007–1.013),
verdict `pass 11 fail 0 unknown 1`. The twelve keep theirs, and the file says why. Proven by
reversal: setting `engine turn, 1 idle sessions` to 400.0 turns exactly **one of four** cases
red, naming that case and its own limit.

### What it changes

`DESIGN.md` §8's dominant row was **505 ns per session** and is now **449 ns**, of which
**~420 ns is the syscall**. The 2026-08-30 figure of **703 ns** — a C program's bare `read`, on
an isolated core, on a machine with `nohz_full` — is now explicable as 449 + ~45 (the global
tax) + ~155 (the per-core tax) + the difference between two programs. **§9's tuning was
responsible for roughly a third of the number this design was budgeted against.**

Everything in the section above was measured in a boot where `nohz_full` existed on *some*
core, so its four-arm figures (501.8 / 494.8 / 498.2 / 670.7) each carry the ~45 ns global tax.
The comparison between them is unaffected — same boot, one variable — and the conclusion is
strengthened, not weakened: the full price of `nohz_full` on the core that has it is **~200 ns
per kernel entry**, not 155.

### The generalisation

`[to testing-skills]` — **a setting scoped to a subset can charge the whole system, and the
control group is inside the blast radius.** The four-arm experiment was carefully built to move
one variable per CPU, and every arm of it — including the untouched control — was sitting in the
tax. Nothing in that design could have seen it, because the thing being varied per-CPU had an
effect that was not per-CPU. It was found only by measuring the *control* again after the
setting was removed everywhere.

The cheap defence is not a better experiment; it is one extra reading. **After you remove the
setting for good, re-measure the arm you were treating as the baseline.** If the baseline moved,
the setting was never as scoped as its interface suggested — and everything measured before it
was removed carries the difference.

---

## What the pre-session stage costs a connection — 2026-09-01

`[measured 2026-08-31]` sharding took the acceptance corpus from **59 to 57**, and
`[measured 2026-09-01]` a stage that holds each socket until its `Logon` arrives puts it back
to **59 through two shards** ([ADR-0020](../decisions/ADR-0020-a-pre-session-stage-owns-the-socket-until-logon.md)).
This is what that stage costs. **A fix nobody has priced is a fix that gets quietly reverted
the first time somebody measures the connection path.**

`[measured 2026-09-01]` AMD Ryzen 7 3700X, Linux 7.0.0-30-generic, `check-machine.sh` reading
**`pass 11 fail 0 unknown 1`** on the [ADR-0021](../decisions/ADR-0021-nohz-full-leaves-section-9.md)
§9 kernel command line, `crates/engine/benches/presession.rs`, medians of **20 qualifying
`scripts/bench.sh` runs**, max/median 1.006–1.011:

| Case | ns/op | per socket |
|---|---|---|
| `recv on a quiet socket` | 420.5 | — |
| **`presession sweep, 1 quiet sockets`** | **435.9** | 435.9 |
| **`presession sweep, 16 quiet sockets`** | **6819.5** | **426.2** |
| `engine turn, 1 idle sessions` | 448.9 | 448.9 |
| `engine turn, 16 idle sessions` | 7333.5 | 458.3 |
| **`presession, read and route an identity`** | **84.0** | once per connection |

### Reading it

**The sweep is the syscall, again.** The stage's own work over the bare `recv` it is made of is
**~15 ns per socket**, against the engine's **~28 ns** for a session it actually owns — which is
the right shape, because the stage has no session machine, no journal and no dispatch. Waiting
for a `Logon` is measurably *cheaper* per socket than serving one.

**The decision costs 84 ns, once.** Reading `49=` and `56=` off the bytes and hashing them to a
shard is **a fifth of one `recv`**, and it happens once in the life of a connection. Set against
this project's own numbers, it is two thirds of a `parse NewOrderSingle` (121.8 ns) — for
something a session does not do at all.

### What this does NOT measure, and it is the number somebody will want

**The wall-clock latency a `Logon` gains by going through the stage.** These are the costs of the
*work*; the connection path also gains a channel hop and a **cross-thread handoff** — the socket
is read on the acceptor thread and served on a shard thread — and neither is in this table.
Nothing here says whether that is 2 µs or 20.

It is not measurable by a bench of this shape: per-iteration setup would need a fresh socket pair
inside the timed window, which would measure `TcpStream::connect`. **The thing that measures it is
`tools/w2w`** — a load generator on a separate machine, timestamping the wire — which is
`STATUS.md` open item 6 and has never been run.

Stated plainly rather than estimated: **the stage's own work is priced; the handoff it introduces
is not.** What is known is where it lands — on the connection path, once per counterparty, and
**not** on `DESIGN.md` §8's message budget, which is why §8 does not move.

---

## CPU mitigations cost 61% of every syscall, and it is not the one that was named — 2026-09-01

`STATUS.md` open item 22 has carried this since 2026-08-30, marked **`[unproven]`**:

> *"`mitigations=off` — full mitigations are on, `vmscape` alone does an IBPB on every syscall
> return, needs a reboot and is a security decision."*

One sentence, one named mechanism, entirely plausible. **The mechanism was wrong**, and the
number is larger than anything else this page records.

`[measured 2026-09-01]` AMD Ryzen 7 3700X, Linux 7.0.0-30-generic, the
[ADR-0021](../decisions/ADR-0021-nohz-full-leaves-section-9.md) §9 line, SMT off, performance
governor, turbo off, machine quiet. Three boots, one variable each.

| Arm | Kernel command line adds | bare `getpid` | `engine turn, 1` |
|---|---|---|---|
| — | *nothing* (§9 as shipped) | 154.5 ns | 448.9 ns |
| **A** | `mitigations=off` | **59.45** | **175.2** |
| **B** | `vmscape=off` | 154.5 — **nothing** | 443.8 |
| **C** | `retbleed=off spec_rstack_overflow=off` | **59.46** | **176.0** |

Every syscall-bound case, arm A against the shipped configuration:

| Case | mitigated | `mitigations=off` | change |
|---|---|---|---|
| `recv on a quiet socket` | 420.5 | 156.9 | **−62.7%** |
| `engine turn, 1 idle sessions` | 448.9 | 175.2 | **−61.0%** |
| `engine turn, 4 idle sessions` | 1807.1 | 712.3 | −60.6% |
| `engine turn, 16 idle sessions` | 7333.5 | 2985.6 | −59.3% |
| `presession sweep, 1 quiet sockets` | 435.9 | 165.4 | −62.0% |
| `presession sweep, 16 quiet sockets` | 6819.5 | 2610.3 | −61.7% |

The tail follows: 5 000 000 timed calls, **p50 216 → 80 ns** and **p99.99 2848 → 88** — flat
all the way out, because what used to dominate the tail is now smaller than a timer tick.

### The control group is what makes it a measurement

Thirteen pure user-space cases, over the same runs: **−4.1% to +4.1%, with no direction.**
`parse NewOrderSingle` +2.0%, `ring, one way` +0.3%, `encode ExecutionReport` −0.3%,
`SendingTime` and `inline deliver + reply` exactly 0.0%. `presession, read and route an
identity` — pure bytes, no syscall — sat still at −0.2%.

And the anchor held: `user_loop`, a dependent multiply-add chain that never enters the kernel,
read 1.0563–1.0585 ns/iter across **all three boots** against the baseline's 1.0577–1.0581.
This is a cross-boot A/B, the shape this page has been burned by before; that agreement to
0.1% is what makes the three boots comparable at all.

### Which mitigation, and the one that was named is not it

**Arm B turned off exactly `vmscape` and nothing moved.** `getpid` read 154.5 ns — unchanged to
the digit — while `/sys` confirmed `vmscape: Vulnerable` and everything else still mitigated.
Across the benches it gave back 0.5–1.1%, which is noise.

**Arm C turned off `retbleed`'s untrained return thunk and `spec_rstack_overflow`'s Safe RET,
put `vmscape`'s IBPB back on, and left the retpolines on — and recovered arm A to within
0.5% on every row.**

So the whole of it is the **AMD return-thunk family**. `vmscape`'s IBPB on every kernel exit
and `spectre_v2`'s retpolines, STIBP and RSB filling cost, together, **under 1%**.

### It also explains what arm A could not

Arm A's saving was **not equal across syscalls**: `getpid` gave back 95 ns, `recv` gave back
264. A fixed IBPB per kernel exit would give back the same in both, and that mismatch was
recorded as unexplained before arm C ran.

A return thunk and a Safe RET are not a fixed per-syscall cost. They add work to **every
return inside the kernel**, so the bill scales with how much kernel code the syscall runs.
`getpid` is a leaf. `recv` walks a dispatch chain. **The unequal saving was evidence for the
mechanism that turned out to be right, and against the one that had been named** — and nobody
read it that way until the arm that could settle it was run.

### What was not separated, and what was turned off beyond its name

- **`retbleed` and `spec_rstack_overflow` were not separated from each other.** They are the
  same class of mechanism — both rewrite the kernel's return path — so splitting them yields
  two smaller numbers and no different decision. Two more reboots buy nothing.
- **Arm C turned off slightly more than its two names suggest**: `retbleed=off` also drops
  `STIBP: always-on` from `spectre_v2`'s line. STIBP protects between two threads of one core
  and SMT is off under §9, so it almost certainly costs nothing here. *Almost certainly* is not
  a measurement, which is why this sentence exists.

### The finding that changes §9, and it is not the 61%

**`scripts/check-machine.sh` read `pass 11 fail 0 unknown 1` in all three arms**, including the
one with every CPU mitigation disabled. §9 had no row for them.

So two machines could both satisfy §9, both be called publishable under non-negotiable 10, and
**differ by 61% on the operation §8 says dominates the design**. That is the same hole
[ADR-0021](../decisions/ADR-0021-nohz-full-leaves-section-9.md) closed for `nohz_full`, in the
same checklist, found the same way — by measuring a setting the checklist never mentioned.
[ADR-0023](../decisions/ADR-0023-section-9-records-the-cpu-mitigations.md) adds the row.

### The generalisation

`[to testing-skills]` — **turning the exact knob you named and watching nothing move is the
cheapest way to learn that the mechanism you named is wrong.**

This page already records the other face of that coin: *a cause accepted because a knob moved
alongside it*. This is the cheaper face, and it is the one people skip. A named mechanism that
explains the data feels finished — it has a name, a story, and a number that fits — so the
experiment that would refute it looks like a formality and gets dropped for the one that would
confirm it.

The refutation cost one boot and one reading, and it killed a sentence that had stood for two
days and would otherwise have shipped in a public document. **The confirming arm ran later and
was worth less**: by then the answer was already narrowed to what was left.

The rule that follows: **when a write-up names a mechanism, the next experiment is the one that
turns off exactly that mechanism** — not the one that turns off everything and shows a big
number. A big number tells you there is something to find. Only the named knob tells you
whether you have found it.

---

## What LTO and `codegen-units` are worth here, and who actually gets it — 2026-09-01

`STATUS.md` open item 13 since the project began: *"Release profile is default. No `lto`, no
`codegen-units = 1`, no PGO. Cheap, but each is a number to be measured before and after, not
a setting to be assumed."*

`[measured 2026-09-01]` AMD Ryzen 7 3700X, the ADR-0021 §9 line with mitigations in force,
`check-machine.sh` reading `pass 12 fail 0 unknown 1` for every run counted. Four arms plus the
default, **ten `scripts/bench.sh` runs each**, medians. No reboots — this is the only lever in
item 13's family that needs none.

| Case | default | `lto="thin"` | `lto="fat"` | `cgu=1` | both |
|---|---|---|---|---|---|
| `walk 1 group, 2 entries` | 58.6 | −3.4% | +5.3% | **−16.6%** | −11.0% |
| `walk 4 levels, 61-tag` | 348.8 | −7.1% | −0.6% | +1.1% | −1.9% |
| `group_members contains` | 9.7 | −3.1% | −7.2% | −10.3% | −3.1% |
| `encode 1 group` | 109.5 | −2.8% | −13.1% | −11.4% | −14.2% |
| `parse NewOrderSingle (validated)` | 125.4 | −3.7% | −1.6% | −0.8% | −2.0% |
| `parse NewOrderSingle (no checks)` | 116.7 | −2.0% | −0.2% | +1.8% | −0.7% |
| `parse Heartbeat` | 59.2 | −3.5% | −6.3% | −1.4% | −4.6% |
| `encode ExecutionReport` | 237.2 | +1.3% | −0.3% | **−8.8%** | −6.3% |
| `SendingTime from the cache` | 4.9 | 0.0% | **+12.2%** | 0.0% | **+12.2%** |
| `ring, one way` | 267.7 | −0.1% | −5.9% | −4.0% | −6.3% |
| `ring, round trip` | 513.4 | +0.9% | −7.9% | −3.8% | −5.2% |
| `presession, read and route` | 83.4 | −8.4% | **−30.8%** | −0.4% | **−30.1%** |
| `presession sweep, 1` | 434.4 | −2.7% | −5.3% | −0.2% | −5.5% |
| `presession sweep, 16` | 6795.9 | −2.7% | −5.4% | −0.1% | −5.6% |
| `recv on a quiet socket` | 418.7 | −3.0% | −2.9% | +0.1% | −2.9% |
| `engine turn, 1` | 444.8 | −2.8% | −3.8% | −0.8% | −3.9% |
| `engine turn, 4` | 1782.0 | −2.7% | −4.7% | −2.1% | −5.0% |
| `engine turn, 16` | 7241.6 | −2.2% | −4.8% | −2.2% | **−4.8%** |
| **clean build** | **5.2 s** | 17.1 s | 15.9 s | 5.2 s | 16.3 s |

`inline deliver + reply` is **absent from this table on purpose**: it read 1.3 ns in three arms
and 7.4–8.6 in the two with `codegen-units = 1`, and that turned out to be the benchmark
deleting its own work rather than a profile effect —
[a-benchmark-can-delete-its-own-work.md](a-benchmark-can-delete-its-own-work.md). With the fix
it reads **8.5 ns in every arm**, and `codegen-units = 1` is exonerated.

The prediction, written before the runs: user-space improves, syscall-bound barely moves. It
was **directionally right and understated the syscall side** — 3–6% is more than "barely".

### And then the decision went the other way

The naive read is *"adopt `lto = "fat"`, it is 3–6% on the dominant path and 30% on one hot
function"*. Two things stop it, and neither is visible in the table.

**Cargo honours `[profile.*]` only from the top-level package being built.** A profile in a
*dependency* is ignored. So `[profile.release]` here would apply to this workspace's own
benchmarks and `tools/w2w`, and **not to anybody who depends on these crates**. It would make
the published numbers better and no consumer's program faster. That is documented cargo
behaviour and is labelled as such — it was not tested here.

**And part of the gain is an artifact of measuring.** A benchmark is a separate crate calling
into the library, so LTO inlines library internals into the benchmark loop.
`presession, read and route an identity` fell **83.4 → 57.7 ns**, but in production
`Shards::hand` calls `identity_of` from inside the same crate, where it is already inlinable.
`recv on a quiet socket` fell 2.9% on a case that is ~94% kernel time — 12 ns off roughly 25 ns
of user-space work, which is an inlining effect at the bench boundary and not a kernel one.

[ADR-0024](../decisions/ADR-0024-the-workspace-keeps-the-default-release-profile.md) keeps the
default profile and puts the range in `GUIDE.md`, where a consumer — whose profile *does* apply
to their own binary — can decide with the caveat attached.

### The generalisation

`[to testing-skills]` — **a benchmark measures the library inlined into the benchmark, and a
whole-program optimisation flatters that arrangement specifically.** The bigger the measured
win from LTO on a micro-benchmark, the more of it is likely to be the call boundary between the
harness and the code, which production may not have in the same place.

The cheap check is not a better benchmark. It is a question asked before adopting: **does this
setting reach the thing that ships, and is the boundary it optimises the boundary production
has?** Here the answer to the first was no, on documented behaviour, and it settled the
decision without needing the second.

## The wire, at last: 16 µs a round trip, and pinning buys only the tail — 2026-09-02

**Phase 1 exit criterion 6, and the first end-to-end number this project has ever been
entitled to publish.** Every figure in `DESIGN.md` §8 until today was either a micro-benchmark
of one stage or somebody else's literature; the ratio that decides which of them matter — *how
much of a round trip is this design's to lose* — had no denominator, because `tools/w2w` had
never run on a machine matching §9.

### The machine, and the command

AMD Ryzen 7 3700X (Zen 2, 8 cores, 2 CCDs), Linux 7.0.0-30-generic, rustc 1.98.0, bare metal.
`scripts/check-machine.sh` reads **`pass 12  fail 0  unknown 1`** — the unknown is NIC IRQ
affinity, and this measurement is over loopback, where there is no NIC to steer. Kernel command
line `isolcpus=6,7,14,15 rcu_nocbs=6,7,14,15 processor.max_cstate=1`, **no `nohz_full`**
([ADR-0021](../decisions/ADR-0021-nohz-full-leaves-section-9.md)), **CPU speculation
mitigations in force** ([ADR-0023](../decisions/ADR-0023-section-9-records-the-cpu-mitigations.md)).

```
cargo build --release -p fixbolt-w2w --features affinity
scripts/w2w-baseline.sh                    # RUNS=20, 20 000 messages each, engine cpu6 / client cpu7
```

Every figure below is the **median over 20 whole runs** of 20 000 timed round trips after
2 000 warmup, with the quiet row re-read before each run and any run over 3% busy discarded.
The client is the only clock: `Instant::now()` before `write_all`, read after the whole reply
is in. **Allocations inside the timed window: 0**, counted by the binary itself on both threads
and asserted — a reversal that puts one `to_vec()` in the loop reads `allocs 2000` over 2 000
messages.

### The figures

| Mode | Path | min | **p50** | **p99** | **p99.9** | qualifying runs | spread |
|---|---|---|---|---|---|---|---|
| `hft` | `TestRequest` → `Heartbeat` | 15 810 | **16 010** | **20 589** | **22 127** | 20 / 20 | 1.006 |
| `hft` | `NewOrderSingle` → `ExecutionReport` | 17 288 | **19 908** | **24 657** | **26 150** | 20 / 20 | 1.005 |
| `standard` | `TestRequest` → `Heartbeat` | 16 020 | **19 447** | **24 106** | **25 609** | 16 / 20 | 1.003 |
| `standard` | `NewOrderSingle` → `ExecutionReport` | 17 624 | **20 920** | **25 618** | **27 092** | 19 / 20 | 1.005 |

All in nanoseconds. `spread` is the largest per-run p50 over the median of them, the same
quantity `benches/baselines.tsv`'s margin column is built from — and at 1.003–1.006 this is the
tightest measurement this repository has ever taken, against 1.10–1.30 for the micro-benchmarks
on the same box. **A round trip through two kernel sockets is a more reproducible thing to
measure than 250 ns of user-space work**, which is not the direction anybody expects.

### 1. The design owns 2.9% of the round trip, measured on both ends of the division

`DESIGN.md` §8's user-space rows total **~0.46 µs** at N = 1 — parse 0.123 + session ~0.1 +
inline dispatch 0.0085 + serialise 0.239. Against the measured `hft` admin round trip of
**16.0 µs** that is **2.9%**.

That number was §8's second reading — *"on kernel TCP, this engine's user-space path is under 5%
of the total"* — and it was arithmetic over a **borrowed** denominator. It is now arithmetic
over a measured one, and it came out where the literature said it would. **This is the only
figure in this file that confirms rather than corrects a literature number.**

### 2. `hft` is worth 3.44 µs against `standard`, and D8's whole case is that number

`hft` p50 **16 010** against `standard` p50 **19 447** on the identical path: **3 437 ns, 17.7%**.

`standard` blocks in `poll` and is woken; `hft` sweeps its sockets at
`[measured 2026-08-31]` ~449 ns a turn. So the wakeup this trade buys out is
**~3.9 µs** — the delta plus the sweep it replaces — and §8 has carried
*"2–5 µs, `epoll`-class"* from the literature since it was written.
**`[measured 2026-09-02]` it is 3.9 µs on this box, inside that band, and that row is no longer
borrowed.** [ADR-0013](../decisions/ADR-0013-two-modes-standard-and-hft.md) decision 4 said a
`standard` figure and an `hft` figure are not comparable and must not be quoted as one; they are
comparable *as a difference*, which is the one thing that difference is for.

**And it is 17.7%, not 10×.** An `hft` engine burns a whole core, permanently, per polling
thread to buy it. `GUIDE.md` §0's *"do not choose `hft` because it sounds better"* now has the
exchange rate printed next to it.

### 3. Pinning to an isolated core buys **nothing** at p50 and **11×** at p99.9

Three arms, `hft` / app path, 9 qualifying runs each, one variable between A and B — whether
`isolcpus` names the core. cpu4–cpu7 are one CCD and share one L3, so cache placement is held
constant across A and B.

| Arm | p50 | p99 | **p99.9** |
|---|---|---|---|
| A — pinned to isolated `cpu6` / `cpu7` | 19 968 | 24 807 | **26 300** |
| B — pinned to `cpu4` / `cpu5`, which `isolcpus` does not name | 19 407 | 25 478 | **266 887** |
| C — not pinned at all | 19 607 | 27 241 | **293 749** |

**p50 moves 2.9% across all three and B is the *fastest* of them.** p99.9 moves **10.1× between
A and B and 11.2× between A and C** — from 26 µs to 267 and 294 µs, which is the shape of a
scheduler putting something else on the core for a quarter of a millisecond.

This closes a gap `DESIGN.md` §9 had marked open in its own words. The `isolcpus` row said the
option was **free** — `[measured 2026-08-31]` `Engine::turn` reads 494.8 ns on an `isolcpus`
core against 501.8 on an untouched one — and then said the *benefit* was **unmeasured**, kept on
a mechanism about other tenants "that a quiet machine cannot exercise". **A quiet machine
exercises it fine**; what could not see it was the instrument. A 20 000-sample micro-benchmark
of a 500 ns operation has no p99.9 worth reading, and the excursion this finds is 250 µs long —
five hundred times the thing being measured. It took an end-to-end measurement to make the
excursion visible at all.

**It is also the exact opposite shape to `nohz_full`.** That option costs 160 ns on every kernel
entry and is worse at p50, p99 **and p99.9**, winning only from p99.99
([ADR-0021](../decisions/ADR-0021-nohz-full-leaves-section-9.md)). `isolcpus` costs nothing
measurable and wins by an order of magnitude at p99.9. **Two isolation knobs, opposite verdicts,
and neither is derivable from the other's name.**

### 4. The application path costs 3.9 µs and the micro-benchmarks account for 320 ns of it

`hft` app p50 **19 908** against admin **16 010**: **+3 898 ns**. At the minimum, where
queueing is least, still **+1 478 ns**.

What the committed benchmarks predict for the extra work, all on this box:

| Extra work in the app path | ns |
|---|---|
| session parses a `NewOrderSingle` (122.6) rather than a `Heartbeat` (57.3) | +65 |
| the desk parses it a second time, unvalidated (117.0) | +117 |
| the desk encodes a 14-slot `ExecutionReport` from a prebuilt template (239.1) | +239 |
| the `Heartbeat` the session would have built instead | ~−100 |
| **predicted** | **~+320** |

**Measured is 4.6× the prediction at the minimum and 12× at p50, and this is not explained.**
Recorded as a gap rather than reasoned into one, because `[measured 2026-08-30]` this repository
has already published a cause that turned out to be wrong for a whole day on exactly this kind
of arithmetic — [the-score-followed-the-timeout](measured-costs.md).

**The largest named candidate, and it is not in any benchmark**: the session validates every
inbound message against the FIX 4.4 dictionary in a pass of its own, and **no benchmark measures
that pass.** `crates/session/src/lib.rs` walks every field asking `is_header`, `is_defined_tag`,
`field_type`, `allows(msg_type, tag)`, `enum_allows` and `field_type().accepts()`, then walks
`required_header()` and `required(msg_type)` calling `view.get(tag)` — a linear scan of the field
index — once per required tag. A `NewOrderSingle` carries 14 fields and ~13 required tags where a
`Heartbeat` carries 6 and ~8; the work is per-field and per-required-tag, and none of it is in
`benches/parse.rs`, which parses with **`NoDict`**.

So **`DESIGN.md` §8's `Parse (D2) 0.12 µs` row does not describe what the engine does to an
inbound message.** It describes framing, field indexing, `9=` and `10=` — with the dictionary
parameter set to a type whose every answer is a no-op. That is a documentation defect found by
reading the code the wire figure pointed at, and it is now `STATUS.md` open item **39**. The
figure is not withdrawn: it is correctly labelled for what it measures and §8 now says what it
excludes.

### 5. Four runs of twenty were discarded, by the guard, for the reason the guard exists

The `standard` arms ran while Chrome woke up on the housekeeping cores — three renderers at
~10% of a core each. `scripts/w2w-baseline.sh` re-reads CPU busy **before every run** and
discarded four at 3–4% against the 3% ceiling.

**And the sixteen that qualified read a spread of 1.003** — tighter than the `hft` arms, which
ran on an idle box. That is `isolcpus` again, from the other direction: the load could not be
scheduled onto cpu6 or cpu7 and therefore could not reach the measurement. The guard discarded
runs it did not have to. **That is the correct trade and the guard stays**: the alternative is a
guard that has to be right about *which* load matters, and `[measured 2026-08-31]` the last time
this project trusted a start-of-sample machine check, a model loaded mid-sample and ruined
twelve runs of twenty.

### The generalisation

`[to testing-skills]` — **a percentile is a property of the instrument as much as of the system,
and a micro-benchmark has no far tail to report.** The isolation setting in §3 above was
measured as *free and unproven* for two days by a benchmark timing a 500 ns operation. The
excursion it prevents is 250 µs long. No amount of resampling that benchmark would have found
it, because a 250 µs stall inside a 500 ns measurement does not appear as a slow p99.9 — it
appears as one absurd outlier in a distribution nobody reads to the end, or gets thrown out as
an artifact.

The rule that follows is cheap to apply: **when a setting is claimed to help the tail, the
instrument must be at least as long-lived as the stall it is supposed to catch.** Ask what the
mechanism's timescale is before choosing the measurement, not after. A 20 µs end-to-end round
trip can see a 250 µs stall as a 12× percentile; a 500 ns loop cannot see it at all.

## Two cases over their band, and only one of them is about the code — 2026-09-02

`scripts/bench.sh --strict` went red the first time it ran on the §9 desktop after phase 1
closed. Two cases were over their band and **the two have nothing in common**: one is a
documented decision whose cost was never re-recorded, and the other is the benchmark harness
changing the number it reports.

Neither was caused by the branch that found them: `git diff origin/main -- crates/` was empty.

### 1. `presession, read and route an identity`: 84 → 202 ns, and it is ADR-0026's price

`[measured 2026-09-02]` six consecutive runs on a box reading 0–1% busy: **201.3 · 197.9 ·
201.6 · 202.2 · 209.3 · 205.7 ns** against a baseline of 84.0 × 1.10 = 92.4. **2.4×.**

**Found by reading, not by measuring.** The baseline was recorded at `f15c82d` when
`identity_of` was two scans:

```rust
sender: field_value(msg, b"49=")?,
target: field_value(msg, b"56=")?,
```

[ADR-0026](../decisions/ADR-0026-a-counterparty-registry-in-the-pre-session-stage.md) decision 2
widened `Identity` with the sub-IDs, so it is now **four**:

```rust
sender_sub: field_value(msg, b"50="),
target_sub: field_value(msg, b"57="),
```

`field_value` is a linear scan from byte 0, and **a tag that is absent scans the whole message
before returning `None`**. `50=` and `57=` do not appear in the acceptance corpus's `Logon` —
checked, zero occurrences — so the two new calls are each a full traversal. Two short scans
became two short scans plus two full ones.

**Proven by reversal, one variable.** Replacing the two new calls with `None` and changing
nothing else:

```
presession, read and route an identity     83.2 ns/op   baseline 84.0 x1.10 = [76.4, 92.4]
presession, read and route an identity     83.1 ns/op
presession, read and route an identity     83.1 ns/op
```

Three consecutive runs, in band, within 1 ns of a baseline recorded on a different day. There is
no ambiguity left in this one.

**And the leading hypothesis was wrong.** Before reading the code, the suspicion was *suite
composition*: two `presession, registry lookup of N` cases were added by `61e5cd7`, **after**
this baseline was recorded, and a case that runs before another changes the cache state it
starts from. That was a reasonable guess and it was not the answer. **Reading the diff of the
function cost five minutes and was decisive; the measurement that would have tested the wrong
hypothesis would have cost twenty and produced a number nobody could interpret.**

**What it is not: a regression to revert.** ADR-0026 widened the identity on purpose, and
`identity_of` runs **once per connection**, not once per message — beside a 426 ns pre-session
sweep, a TCP handshake and a 16 µs `Logon` round trip, 119 ns is 0.7% of connecting. The item is
a **baseline that was never re-recorded when a decision changed the code under it**, which is a
different defect from a slow function and needs a different fix.

### 2. `encode ExecutionReport (template)`: 242 → 277 ns, and the code under test never changed

`[measured 2026-09-02]` six runs: **274.2 · 279.6 · 275.5 · 283.3 · 275.0 · 279.4 ns** against
239.1 × 1.10 = 263.0. **+16%.**

`Template::encode` has not been touched since the baseline. `crates/codec/benches/serialize.rs`
is **byte-identical** to its state at the commit that recorded the baseline. The only change to
`crates/codec/src/` in the whole range is **4 insertions and 4 deletions** in `template.rs` —
[ADR-0044](../decisions/ADR-0044-a-builder-that-is-not-moved-per-field.md) making `field`,
`slot`, `group` and `build` take `&mut self`. And **`struct Template` is byte-identical across
it**, which was the one mechanism by which that commit could have reached `encode` at all.

**So the first thing measured was the machine, by rebuilding the old binary.** A worktree at
`bf798ea` — the commit that recorded 239.1 — built and run **today, on this box, on this
toolchain**:

```
encode ExecutionReport (template)   239.5 · 240.2 · 241.9 · 242.3 · 244.7 ns   median ~242
```

Right on its baseline. **The machine and the toolchain are innocent**, and that was worth two
minutes to establish before blaming anything else — the box has been rebooted, retuned and had
its `§9` checklist extended twice since that number was written down.

**Then the bisect, and it lands on two commits rather than one:**

| Commit | `encode`, five runs | median |
|---|---|---|
| `bf798ea` — where 239.1 was recorded | 239.5 · 240.2 · 241.9 · 242.3 · 244.7 | **242** |
| `f15c82d` | 231.1 · 231.2 · 232.2 · 237.7 · 238.0 | 232 |
| `54eebe9` | 234.2 · 235.2 · 241.8 · 241.9 · 243.4 | 241 |
| **`4396d6d`** — *"a baseline is a band"* | 260.1 · 260.3 · 263.1 · 263.7 · 267.1 | **263** |
| `576f924` — ADR-0044 | 273.5 · 273.9 · 276.5 · 276.9 · 280.4 | **277** |

**ADR-0044 is +11 ns of the +35. The other +22 belongs to `4396d6d`, which is the commit that
built the gate to catch a benchmark measuring nothing.**

### And `4396d6d` did not change the measurement. It changed what happens after it.

`Suite::bench` is byte-identical across that commit:

```rust
pub fn bench<F: FnMut()>(&mut self, name: &str, mut f: F) {
    for _ in 0..10_000 { f(); }              // warmup
    let mut best = f64::INFINITY;
    for _ in 0..7 {
        let iters = 200_000u32;
        let t = Instant::now();
        for _ in 0..iters { f(); }           // the timed loop
        best = best.min(t.elapsed().as_nanos() as f64 / f64::from(iters));
    }
    ...                                       // <- everything 4396d6d changed is here
```

Everything the commit touched — `verdict()`, the `[floor, ceiling]` format string, a
`Vec<String>` field, the `Verdict::Under` arm — runs **strictly after `best` is computed**.
**Code that executes after a measurement moved that measurement by 9%.**

`bench` is generic and monomorphised per closure, so each case gets its **own copy** of it, and
growing the body grows the copy the timed loop sits inside. That is the mechanism named, and it
is a hypothesis rather than a demonstration: the plausible route is the inliner's budget for
that copy changing whether `f` is inlined into the timed loop, or the loop's alignment inside
the enlarged function.

**One hypothesis was tested and refuted.** If the cause were where the case's two 512-byte stack
objects land — `Template<32, 512>`'s scratch and the 512-byte `out` — then padding the frame
would move the figure back. It does not:

```
PAD=0    285.8 · 271.7 · 285.8      PAD=32   266.5 · 277.2 · 263.2
PAD=8    274.1 · 266.8 · 272.5      PAD=64   265.2 · 274.3 · 276.7
PAD=16   272.8 · 274.3 · 269.1      PAD=512  269.4 · 270.8 · 273.0
```

Every arm sits in the same ~270 band and none recovers 242. **It is code layout, not data
placement.**

**And it is case-specific, which rules out a blanket effect.** In the **same binary**, across
the same commit:

| | `54eebe9` | `4396d6d` |
|---|---|---|
| `encode ExecutionReport (template)` | 245.2 · 242.2 · 242.9 | 265.0 · 270.4 · 262.8 |
| `SendingTime from the cache` | 4.9 · 4.9 · 4.9 | 4.9 · 4.9 · 4.9 |

One case moved 9%; the other did not move at all, to the printed precision.

### What this means for the gate, and it is worse than one red case

**239.1 was never a property of `Template::encode`.** It was a property of `encode` *plus the
size of the harness function it was measured inside*, and nothing in `benches/baselines.tsv`
records the second term. Any future edit to `harness.rs` — including one that only prints
differently — can move this case again, and the margin ladder in that file's header has no
column for it.

`4396d6d`'s own commit message is where this was already written down, by the person who wrote
it, on the day:

> **NOT PROVEN, and not claimed: the `--strict` half of this decision has not run.**
> `bench.sh --strict` exits 1 at the §9 machine check before reaching the under-baseline branch,
> and CI does not run `--strict` at all. **It needs a §9 Linux box.**

It got one 32 days of commits later, and the first thing it said was that the commit which
wrote that sentence had cost 9%.

### The generalisation

`[to testing-skills]` — **the harness is part of the measurement, and "my change only affects
reporting" is not a reason to skip re-measuring.** A generic timing function is monomorphised
per case, so the code you add to print a result lands in the same function body as the loop you
are timing. The edit that moved this number by 9% adds a struct field and a `match` and runs
after the clock is read.

Two cheap things follow, and neither is "write a better benchmark":

1. **When the harness changes, re-run the baselines** — treat an edit to the measuring code the
   way you treat an edit to the measured code. Here nothing did, for a month, because the gate
   that would have said so could not run on the machine that had the baselines.
2. **Ask what a figure is a property of.** This project's file records the CPU, the case, the
   margin, the run count, the date and the machine verdict — six columns, and the seventh term
   turned out to be the size of the function doing the timing. A baseline that moves when the
   printing changes is measuring two things and naming one.

And a third that is about the order of work rather than about benchmarks: **for case 1 above,
reading the diff of the function was decisive in five minutes and the measurement everyone
reaches for first would have tested the wrong hypothesis.** Locate before you measure, when
locating is a `git log -S` away.

## The dictionary pass costs seven parses, and it explains a sixth of the gap it was named for

`[measured 2026-09-05]` §9 desktop, AMD Ryzen 7 3700X, `check-machine.sh` `pass 12 fail 0
unknown 1`, bench build with function alignment pinned (ADR-0049), medians of **21** qualifying
`scripts/bench.sh` runs. `crates/session/benches/validate.rs`, through the public
`fixbolt_session::validate` that [ADR-0050](../decisions/ADR-0050-the-dictionary-pass-is-public-so-it-can-be-timed.md)
added for exactly this reason.

| Case | ns/op | margin |
|---|---|---|
| `validate NewOrderSingle` (the `parse.rs` shape, 15 fields) | **882.1** | 1.10 |
| `validate Heartbeat` (the `parse.rs` shape, 8 fields) | **169.5** | 1.10 |
| `validate NewOrderSingle, w2w bytes` (16 fields) | **897.3** | 1.10 |
| `validate TestRequest, w2w bytes` (9 fields) | **218.4** | 1.10 |

For scale, the parse that precedes it on the same `NewOrderSingle` is **120.0 ns**. **The
dictionary pass costs about seven times the parse**, and until this file existed nothing timed
it: `benches/parse.rs` parses with `NoDict`, whose every answer is a no-op, so §8's parse row
was never about this work.

Proven by reversal, which is the only thing that says the case measures the pass rather than
the harness: `validate` returning `None` immediately reads **1.1 ns** for every case.

### And it is not the answer to the question it was opened for

`STATUS.md` item 39 existed because the application round trip is **3 898 ns** above the
administrative one and the committed benchmarks accounted for ~320 ns. The pass was written
down as the largest untimed candidate. Subtracting the two figures that are `w2w`'s exact
bytes — the only subtraction the numbers license — gives **678.9 ns, 17.4% of the gap**. With
everything else `--path app` adds (inbound parse ~+60, dispatch +9, the application's own
`Validation::NONE` re-parse +114, its template encode +233) the measured subtotal is **~1 094
ns, 28%**, and **~2 804 ns is still unattributed**. `DESIGN.md` §8 carries the arithmetic.

**The largest named candidate was a sixth of the answer.** That is the whole value of adding it
up: the pass is genuinely the biggest user-space row in the budget, it is genuinely seven times
the parse, and *both of those can be true while it explains almost none of the gap it was
opened for*. A number large enough to feel like an explanation is the exact shape this file
already records under [the score followed the timeout](#the-score-followed-the-timeout-and-the-timeout-was-not-the-cause).

### The generalisation

`[to testing-skills]` — **"the largest candidate" is a ranking, not a quantity, and the two get
confused the moment one of them is measured.** An unexplained gap attracts a shortlist; the
biggest item on it gets measured first and comes back large in its own right; and the natural
next sentence — *so that is where the time went* — does not follow from anything. The
discipline that costs nothing is to **finish the subtraction out loud**: put the measured
pieces in a table, subtract, and write the remainder down as a number with its own name. Here
the remainder is 72% and now has an open item; without the table it would have been a feeling
that the gap was mostly explained.

## The application round trip: what the extra 3 898 ns is, and mostly is not

`[measured 2026-09-05]` `tools/w2w --path app` costs **3 898 ns** more per round trip at p50
than `--path admin` on the §9 desktop. `STATUS.md` open item 49 named four candidates for the
part no benchmark accounted for. Two of them are now priced and **both are noise**.

### The bytes, measured rather than assumed

`strace -f -e trace=sendto` on `./target/release/w2w` at its default flags, over the last 2 000
sends of each direction — all 2 000 identical, so exact:

| Path | in | out |
|---|---|---|
| `--path admin`, `35=1` then `35=0` | **83** | **87** |
| `--path app`, `35=D` then `35=8` | **149** | **191** |

Item 49 recorded these as "149 in and ~200 out against 79 and ~70" and **one of the four was
right**. The sizes drift with the digit count of `34=` and `11=`/`112=`, so they mean nothing
without a message count beside them: at `--messages 20` the same paths read 77/81 and 143/179.

### Candidate 1 and 4 — the kernel copying more bytes: ~25 ns

`crates/engine/benches/payload.rs`. **The two real sizes cannot be subtracted from each other**
— they differ by 170 bytes, worth tens of nanoseconds inside a round trip of ~12 600, and across
three repetitions their difference read −4, +13 and +46 ns, which is scatter with a sign change
in it. The term is read off the outer pair instead, where the lever is 8 184 bytes each way:

| Case | ns |
|---|---|
| `TCP loopback, 8 in 8 out` | 12 528.8 |
| `TCP loopback, 83 in 87 out` | 12 608.5 |
| `TCP loopback, 149 in 191 out` | 12 648.4 |
| `TCP loopback, 8192 in 8192 out` | 14 890.9 |

`(14 890.9 − 12 528.8) / 16 368` = **0.1443 ns per byte** written and read. The application path
moves 66 more bytes in and 104 more out, so the term is `170 × 0.1443` = **24.5 ns** — **0.9%**
of the 2 804 ns it was a candidate for.

### Candidate 3 — journalling the reply: 8.9 ns

`crates/engine/benches/journal.rs`. The administrative path never does this at all, so the whole
figure counts: `MemJournal<4096,512>::put` of the 191-byte `ExecutionReport`, walking the ring as
the engine does, is **8.9 ns**. The ring sweep above confirms it in situ.

### The subtraction, out loud

| Term | ns | Where from |
|---|---|---|
| inbound parse, larger message | ~60 | `parse NewOrderSingle` − `parse Heartbeat` |
| **the dictionary pass** | **679** | `validate NewOrderSingle, w2w bytes` − `validate TestRequest, w2w bytes` |
| dispatch | 9 | `inline deliver + reply` |
| the application's own `Validation::NONE` re-parse | 114 | `library, parse only` |
| its template encode | 233 | `encode ExecutionReport (template)` |
| **kernel copies of the bigger payload** | **24.5** | `TCP loopback` slope, above |
| **`Journal::put` of the reply** | **8.9** | `journal put, 191 bytes, walking` |
| **accounted** | **1 128** | **28.9%** |
| **unattributed** | **2 770** | **71.1%** |

**Two of the four candidates are dead and the remainder barely moved**, from ~2 804 to ~2 770 ns.
What is left is the engine's framing and read-buffer management, which no benchmark isolates, and
the session's own `Heartbeat` serialise on the admin side, which has no committed case either.

This is the second time on this page that the largest *named* candidate turned out not to be the
answer. The first was the dictionary pass: 679 ns is real and is the biggest single row here, and
it is still only **17.4%** of the thing it was nominated to explain.

### One number on this page is about the machine and not about this engine

`[measured 2026-09-05]` those `TCP loopback` figures are ~12.5 µs for **four syscalls**. A bare
`getppid` on the same box in the same run is 170.5 ns, a pipe round trip 778.9 ns and a UNIX
socketpair 1 924.9 ns. A loopback write of 8 bytes alone is 5 450 ns — thirty-two bare syscalls.

The **differences** on this page are unaffected: both `tools/w2w` paths make 44 002 `sendto` calls
each, so a per-syscall constant cancels. The **absolutes** are environment-bound and are not a
round-trip claim about anything. See
[a-loopback-write-costs-thirty-two-syscalls.md](a-loopback-write-costs-thirty-two-syscalls.md).

## TLS on the wire: kTLS was the slower of the two, and one arm did not reproduce at p50 — 2026-09-14

`[measured 2026-09-14]` **The first TLS latency figures this repository publishes**, step 6-M of
[plans/2026-09-04-tls.md](../plans/2026-09-04-tls.md), and the condition
[ADR-0005](../decisions/ADR-0005-tls.md) decision 5 set for `DESIGN.md` §8's TLS row: the same
load with TLS off, with kTLS and with userspace `rustls`, on one Linux box. Two things came back
that nobody predicted, and **neither has a cause**: kTLS was slower than userspace `rustls` in
both `hft` paths, and one arm's p50 moved 15.9% between two identical procedures — with two kTLS
arms' p99 moving 7–19% as well.

### The machine, and the command

AMD Ryzen 7 3700X (Zen 2, 8 cores), **Linux 7.0.0-31-generic**, rustc 1.98.0 (88d9e12ae
2026-08-18), bare metal. Kernel command line `isolcpus=6,7,14,15 rcu_nocbs=6,7,14,15
processor.max_cstate=1`, **no `nohz_full`**
([ADR-0021](../decisions/ADR-0021-nohz-full-leaves-section-9.md)), **CPU speculation
mitigations in force** ([ADR-0023](../decisions/ADR-0023-section-9-records-the-cpu-mitigations.md)).
`fixbolt-machine on`: governor `performance`, boost 0, SMT off, THP `never`, `net.core.busy_poll`
50; the `tls` module loaded. `scripts/check-machine.sh` reads **`pass 12  fail 0  unknown 1`** in
the header of both procedures (07:25:28, 07:56:27) and straight after each (07:51:32, 08:22:07);
the unknown is NIC IRQ affinity, and this is loopback. A read taken at 07:24:03 was
`pass 11 fail 1 unknown 1`, its failing row verbatim:

```
FAIL   machine is quiet       7% CPU busy over 1s — code 30% of a core  gnome-shell 8% of a core  code 2% of a core
```

It is not the verdict of either procedure, whose own header read passed at 07:25:28, and the
baseline re-reads the quiet row before every run.

```
cargo build --release -p fixbolt-w2w --features affinity,tls          # at 1178f4d
RUNS=20 ARMS="hft:admin:off hft:admin:ktls hft:admin:userspace hft:app:off hft:app:ktls hft:app:userspace standard:admin:off standard:admin:ktls" scripts/w2w-baseline.sh
```

`1178f4d` is `main`, CI run [`34767259852`](https://github.com/tmthang86/fixbolt/actions/runs/34767259852)
(push to `main`, success). **The commit and the tree were read by the manager's session, not by
the procedure**: `git status -sb` clean on `main` at 07:22, and straight after the build at 07:24
`git rev-parse HEAD` read `1178f4d3d9905291eb3c5b2ec27302ff138349c3` with an empty
`git status --short`; the binary's modification time is 07:24:19. Neither procedure's output
records HEAD or the tree state itself. Both procedures print
`runs 20   messages 20000   warmup 2000   gap 8s` and `engine cpu6   client cpu7   (allow-unisolated 0)`.
One `w2w` run of 1 000 messages was taken at 07:25:18 and thrown away first, as the plan's §6.7
item 5 asks after a reboot.

**What `ktls` and `userspace` mean in this binary: TLS at both ends of the socket.**
`tools/w2w/src/main.rs`'s `serve` (its `EngineSide::Tls` arm) wraps the engine's accepted socket
in a `TlsTransport`, `tls_arm::connect` wraps the client's dialled one and, at its end, refuses a
`ktls` run whose *client* handover fell back. Both ends are built from the engine's own
`tls::server_config`/`tls::client_config` (`tools/w2w/src/main.rs`, `tls_arm::Pki::new`), which offer
`TLS13_AES_128_GCM_SHA256` and nothing else (`crates/engine/src/tls.rs:821-827`). Every TLS figure below is a round trip with record
processing at both ends; nothing here separates one end from the other.
`scripts/w2w-baseline.sh` refuses a run whose `tls:` read-back is not the arm's (`kernel`
for `ktls`, the `grep -qx "tls: $want_tls"` check) and refuses `allocs` ≠ 0 for every arm but
`userspace` (the `allocs +0` check after it); both procedures end
`baseline exit=0`, so every `ktls` run read back `kernel` and counted 0 allocations.

**Two procedures, not one.** Procedure 1 ran 07:25:28–07:50:59, about seven minutes after boot
(procedure 2's own `uptime` line reads `up 38 min` at 07:56:27). Procedure 2 is the identical
command, 07:56:27–08:21:57, and **it was run because the single runs taken between the two
disagreed with procedure 1**: one `userspace` admin run read p50 17 553 against procedure 1's
20 774 for that arm; seven more single admin runs followed, whose four `userspace` runs all read
that faster level while the kTLS and `off` runs sat only 2–3% under procedure 1; and the whole
procedure was run again rather than publish either figure (§2 below, with those runs verbatim).

### Procedure 1, the summary blocks, verbatim

```
  == hft / admin / off: median of 20 qualifying runs (0 disqualified) ==
     min    15835 ns
     p50    16065 ns      (across runs: 16001 .. 16231)
     p99    20804 ns
     p99.9  22232 ns
     spread max/median 1.010
     machine pass 12   fail 0   unknown 1
     pinned  engine cpu6, client cpu7

  == hft / admin / ktls: median of 20 qualifying runs (0 disqualified) ==
     min    24256 ns
     p50    25298 ns      (across runs: 25148 .. 25449)
     p99    37345 ns
     p99.9  38924 ns
     spread max/median 1.006
     machine pass 12   fail 0   unknown 1
     pinned  engine cpu6, client cpu7

  == hft / admin / userspace: median of 20 qualifying runs (0 disqualified) ==
     min    17644 ns
     p50    20774 ns      (across runs: 20639 .. 20930)
     p99    25303 ns
     p99.9  27617 ns
     spread max/median 1.008
     machine pass 12   fail 0   unknown 1
     allocs  NOT asserted zero — userspace leaves ADR-0005 decision 3's guarantee
     pinned  engine cpu6, client cpu7

  == hft / app / off: median of 18 qualifying runs (2 disqualified) ==
     min    17453 ns
     p50    20289 ns      (across runs: 20219 .. 20349)
     p99    25107 ns
     p99.9  26936 ns
     spread max/median 1.003
     machine pass 12   fail 0   unknown 1
     pinned  engine cpu6, client cpu7

  == hft / app / ktls: median of 20 qualifying runs (0 disqualified) ==
     min    27677 ns
     p50    29591 ns      (across runs: 29436 .. 29767)
     p99    36409 ns
     p99.9  42571 ns
     spread max/median 1.006
     machine pass 12   fail 0   unknown 1
     pinned  engine cpu6, client cpu7

  == hft / app / userspace: median of 20 qualifying runs (0 disqualified) ==
     min    19352 ns
     p50    21641 ns      (across runs: 21511 .. 21942)
     p99    26385 ns
     p99.9  28198 ns
     spread max/median 1.014
     machine pass 12   fail 0   unknown 1
     allocs  NOT asserted zero — userspace leaves ADR-0005 decision 3's guarantee
     pinned  engine cpu6, client cpu7

  == standard / admin / off: median of 20 qualifying runs (0 disqualified) ==
     min    16000 ns
     p50    19522 ns      (across runs: 19417 .. 19587)
     p99    24286 ns
     p99.9  25879 ns
     spread max/median 1.003
     machine pass 12   fail 0   unknown 1
     pinned  engine cpu6, client cpu7

  == standard / admin / ktls: median of 20 qualifying runs (0 disqualified) ==
     min    27918 ns
     p50    29135 ns      (across runs: 28704 .. 29506)
     p99    34475 ns
     p99.9  40547 ns
     spread max/median 1.013
     machine pass 12   fail 0   unknown 1
     pinned  engine cpu6, client cpu7
```

The two disqualified runs of `hft / app / off` read `11% busy` and `20% busy` before they started.

### Procedure 2, the summary blocks, verbatim

```
  == hft / admin / off: median of 20 qualifying runs (0 disqualified) ==
     min    15354 ns
     p50    15670 ns      (across runs: 15600 .. 15749)
     p99    20439 ns
     p99.9  22037 ns
     spread max/median 1.005
     machine pass 12   fail 0   unknown 1
     pinned  engine cpu6, client cpu7

  == hft / admin / ktls: median of 20 qualifying runs (0 disqualified) ==
     min    23625 ns
     p50    24657 ns      (across runs: 24496 .. 24827)
     p99    30447 ns
     p99.9  37872 ns
     spread max/median 1.007
     machine pass 12   fail 0   unknown 1
     pinned  engine cpu6, client cpu7

  == hft / admin / userspace: median of 20 qualifying runs (0 disqualified) ==
     min    17147 ns
     p50    17473 ns      (across runs: 17333 .. 17813)
     p99    22307 ns
     p99.9  25538 ns
     spread max/median 1.019
     machine pass 12   fail 0   unknown 1
     allocs  NOT asserted zero — userspace leaves ADR-0005 decision 3's guarantee
     pinned  engine cpu6, client cpu7

  == hft / app / off: median of 20 qualifying runs (0 disqualified) ==
     min    16942 ns
     p50    19998 ns      (across runs: 17323 .. 20158)
     p99    24055 ns
     p99.9  26841 ns
     spread max/median 1.008
     machine pass 12   fail 0   unknown 1
     pinned  engine cpu6, client cpu7

  == hft / app / ktls: median of 20 qualifying runs (0 disqualified) ==
     min    27021 ns
     p50    29501 ns      (across runs: 29286 .. 29676)
     p99    39029 ns
     p99.9  41794 ns
     spread max/median 1.006
     machine pass 12   fail 0   unknown 1
     pinned  engine cpu6, client cpu7

  == hft / app / userspace: median of 20 qualifying runs (0 disqualified) ==
     min    18851 ns
     p50    21596 ns      (across runs: 21331 .. 21851)
     p99    26500 ns
     p99.9  28238 ns
     spread max/median 1.012
     machine pass 12   fail 0   unknown 1
     allocs  NOT asserted zero — userspace leaves ADR-0005 decision 3's guarantee
     pinned  engine cpu6, client cpu7

  == standard / admin / off: median of 20 qualifying runs (0 disqualified) ==
     min    15509 ns
     p50    19151 ns      (across runs: 19056 .. 19256)
     p99    23895 ns
     p99.9  25388 ns
     spread max/median 1.005
     machine pass 12   fail 0   unknown 1
     pinned  engine cpu6, client cpu7

  == standard / admin / ktls: median of 20 qualifying runs (0 disqualified) ==
     min    27663 ns
     p50    28840 ns      (across runs: 28634 .. 28985)
     p99    34300 ns
     p99.9  40031 ns
     spread max/median 1.005
     machine pass 12   fail 0   unknown 1
     pinned  engine cpu6, client cpu7
```

### What userspace allocates

The baseline does not assert `allocs` for `userspace` and does not print it in the summary, so
the count comes from two single runs taken between the procedures (file time 07:53), **with no
per-run quiet check** — an allocation count does not depend on it, a latency figure does — pinned
`engine-core: cpu6`, `client-core: cpu7`, `tls: userspace`, 20 000 samples after 2 000 warmup:

```
tls: userspace
w2w: TestRequest -> Heartbeat, over kernel TCP on loopback
     20000 samples after 2000 warmup
     allocs     80000   (both threads, the timed window only)

tls: userspace
w2w: NewOrderSingle -> ExecutionReport, over kernel TCP on loopback
     20000 samples after 2000 warmup
     allocs     80000   (both threads, the timed window only)
```

**4 allocations per round trip, over both threads and both TLS ends together.** The `off` and
`ktls` arms count **0** in every run of both procedures, asserted.

### The mode scripts' TLS arms, verbatim

Run straight after procedure 1 on the same boot, the same `pass 12 fail 0 unknown 1`.
`scripts/check-no-kernel-sleep.sh`, 07:50:59, TLS arms (the trace counts are the engine thread's,
attributed by tid, over the script's own 300-message run — not the timed procedure):

```
== TLS arm: hft mode with kTLS must also pass, and read back 'kernel' ==
   8110 recvfrom
   8108 accept4
    355 sendto
      9 getrandom
      4 setsockopt
      4 munmap
      4 mmap
      3 sigaltstack
GREEN ok — --tls ktls: no blocking call, socket calls present, tls: kernel

== TLS arm: --tls userspace must read back 'userspace' ==
GREEN ok — --tls userspace reads back tls: userspace (the read-back line distinguishes arms)
no-kernel-sleep exit=0
```

`scripts/check-standard-gives-the-core-back.sh`, 07:51:01, TLS arm:

```
== TLS arm: standard mode with kTLS must also give the core back ==
  mode reported   standard
  engine CPU      .31%   (ceiling 5%)
  found sleeping  20 of 20 samples
  round trip p50  29466 ns   (ceiling 1000000 ns)
  tls reported    kernel   (wanted kernel)
GREEN ok — standard + ktls blocks, stays alive, is woken by the data, tls: kernel
standard exit=0
```

Both scripts' plain halves passed in the same runs, the red halves included:
`RED   ok — --mode standard trips it:  6 poll`, `RED   ok — hft trips it on the policy, as it must`
and `RED   ok — yield trips it on the policy, as it must`.

### The table, and what TLS adds

Medians of the per-run p50 / p99 / p99.9, ns, both procedures side by side and **not averaged**.

| Mode, path | TLS | procedure 1 | procedure 2 |
|---|---|---|---|
| `hft`, `TestRequest` → `Heartbeat` | off | 16 065 · 20 804 · 22 232 | 15 670 · 20 439 · 22 037 |
| | kTLS | 25 298 · 37 345 · 38 924 | 24 657 · 30 447 · 37 872 |
| | userspace | 20 774 · 25 303 · 27 617 | **17 473** · 22 307 · 25 538 |
| `hft`, `NewOrderSingle` → `ExecutionReport` | off | 20 289 · 25 107 · 26 936 (18 of 20) | 19 998 · 24 055 · 26 841 |
| | kTLS | 29 591 · 36 409 · 42 571 | 29 501 · 39 029 · 41 794 |
| | userspace | 21 641 · 26 385 · 28 198 | 21 596 · 26 500 · 28 238 |
| `standard`, `TestRequest` → `Heartbeat` | off | 19 522 · 24 286 · 25 879 | 19 151 · 23 895 · 25 388 |
| | kTLS | 29 135 · 34 475 · 40 547 | 28 840 · 34 300 · 40 031 |

p50 added over `off` **in the same procedure**, ns:

| Mode, path | kTLS, proc. 1 / 2 | userspace, proc. 1 / 2 | kTLS − userspace, proc. 1 / 2 |
|---|---|---|---|
| `hft`, admin | +9 233 / +8 987 | +4 709 / +1 803 | 4 524 / 7 184 |
| `hft`, app | +9 302 / +9 503 | +1 352 / +1 598 | 7 950 / 7 905 |
| `standard`, admin | +9 613 / +9 689 | not run | — |

**The `off` arms agreed with the 2026-09-02 table** (*The wire, at last*, above): they differ from
it by at most **2.1% at p50, 2.4% at p99 and 3.0% at p99.9** (rounded), both procedures, all three
arms — against a kernel one
patch level older (`7.0.0-30`) and code twelve days older, so this is agreement, not a controlled
repeat. Re-measuring that table is step B2 of
[plans/2026-09-04-the-second-linux-desk.md](../plans/2026-09-04-the-second-linux-desk.md).

### 1. kTLS was the slower of the two, in both `hft` paths, in both procedures, at every percentile

Over plain TCP, kTLS added **9.0–9.5 µs** at p50 in the two `hft` arms and **9.6–9.7 µs** in the
`standard` arm; userspace `rustls`, run in `hft` only, added **1.8–4.7 µs** on the administrative
path and **1.4–1.6 µs** on the application path. kTLS
minus userspace is **4.5 / 7.2 µs** administrative and **8.0 / 7.9 µs** application at p50, and
positive at p99 and p99.9 in both paths and both procedures too (8 140 to 14 373 ns).

What else was read on the same boot: the engine thread's eight most frequent syscalls under
`hft --tls ktls` were the plain arm's socket calls — `recvfrom`, `accept4`, `sendto` — plus
`getrandom` ×9 and `setsockopt` ×4, and no sleeper (the script block above). Nothing here times
those calls or the kernel's record path.

**What this does NOT show, said plainly:**

- **Anything about a NIC.** This is loopback. Neither arm has hardware offload available —
  `[read 2026-09-14]` by the manager on this desk, `ethtool -k lo` reads
  `tls-hw-tx-offload: off [fixed]`, `tls-hw-rx-offload: off [fixed]` and
  `tls-hw-record: off [fixed]` — so the case ADR-0005 decision 2 names, *"NIC offload where the
  hardware supports it"*, is not in this measurement at all. On the software side, the
  highest-priority `gcm(aes)` in `/proc/crypto` on this kernel is `generic-gcm-aesni-avx`
  (priority 500; `generic-gcm-aesni` is 400); which implementation the kernel's TLS bound at
  runtime was not read.
- **Anything about another kernel**, another CPU or another suite. One boot, one kernel, one suite.
- **Any cause.** No variable was isolated. `CLAUDE.md` §10: a cause is not accepted because a knob
  moved with it, and no knob was moved.
- **One end's cost.** Both ends run TLS in the same arm; the delta covers the request and the
  reply at both ends together, and is not a per-message decrypt row.

**What it does not change by itself.** ADR-0005 decision 2 prefers kTLS for reasons that are not
latency on loopback: D8 preserved, parse-in-place preserved, AES-NI with NIC offload where the
hardware supports it (the NIC half is out of reach here, above), and the hot-path guarantee — which
this measurement also read, **`allocs 0` for `ktls` against 80 000 per 20 000 round trips for
`userspace`**. Whether the design should still prefer kTLS for `hft` is a design question and is
`STATUS.md` open item **84**, for the architect.

### 2. One arm's p50 moved 15.9% between two identical procedures, and two arms' p99 moved too

`hft / admin / userspace` read p50 **20 774** in procedure 1 (runs 20 639 .. 20 930, spread 1.008)
and **17 473** in procedure 2 (runs 17 333 .. 17 813, spread 1.019). The two ranges do not
overlap. **At p50**, every other arm moved **0.2–2.5%**, all eight in the same direction, faster.

**At p99 the picture is not one arm.** `hft / admin / ktls` p99 moved 37 345 → 30 447 (**−18.5%**):
procedure 1's per-run p99 fell in two clusters, 8 runs at 30 928–32 141 and 12 at 37 220–37 841,
and procedure 2's 20 runs all read 29 887–30 699. `hft / app / ktls` p99 moved 36 409 → 39 029
(**+7.2%**; runs 35 908–37 071, then 36 529–40 116). `hft / admin / userspace` p99 moved −11.8% and
p99.9 −7.5%; `hft / app / off` p99 −4.2%. Every other p99 moved at most 1.8%, and every other
p99.9 at most 2.7%. **And one level shift sat inside a single procedure**: procedure 1's
`standard / admin / ktls` read p50 29 305–29 506 over runs 1–10 and 28 704–28 965 over runs 11–20,
a spread of 1.013; procedure 2's runs read 28 634–28 985.

**What prompted procedure 2.** A single `userspace` admin run between the procedures (file time
07:53, the allocation block above, **no per-run quiet check**) read p50 **17 553**. Seven more
single `hft` admin runs followed on the same binary and boot, `fixbolt-machine on`, engine `cpu6`,
client `cpu7`, 20 000 messages after 2 000 warmup. **They had no per-run quiet check either** —
the manager's editor session was active — **so none of these single runs is a §9 figure**; they
are what the manager saw before deciding to re-run the whole procedure. The terminal output,
verbatim:

```
07:54:28 userspace min 17142  p50 17433  p99 22553
07:54:36 userspace min 17323  p50 17613  p99 22523
07:54:45 userspace min 17133  p50 17453  p99 22873
07:54:53 ktls min 23675  p50 24747  p99 30768
07:55:02 ktls min 23444  p50 24506  p99 30428
07:55:10 off min 15309  p50 15670  p99 20639
07:55:19 userspace min 17133  p50 17423  p99 22432
```

Four `userspace` runs at p50 17 423–17 613, against procedure 1's 20 774 for that arm; two kTLS
runs 2.2% and 3.1% under procedure 1's 25 298; one `off` run 2.5% under its 16 065. Procedure 2
then published 17 473, 24 657 and 15 670 for the same three arms.

**The spread column cannot register how far below the median a run falls.** Inside procedure 2,
`hft / app / off` printed spread **1.008** while five of its twenty runs read p50 17 323, 17 523,
17 864, 18 976 and 19 136 against a median of 19 998. The spread is maximum over median
(`scripts/w2w-baseline.sh`, the summary's `spread max/median` line). A fast run does move it, a little, by lowering the median:
with those five runs replaced by 20 050 the same column reads 1.005. But a run 13.4% under the
median and a run 0.3% under it move it by exactly the same amount.

**The numbers, without an interpretation.** The moved arm's median per-run *minimum* went
17 644 → 17 147 (−497 ns), in line with the other seven arms' minima (−255 to −656 ns), while its
p50 went −3 301 ns. Procedure 1's per-run minima for that arm were 17 523–17 774, all above
procedure 2's p50.

**What differed between the procedures — candidates, none claimed as a cause.** Recorded before
procedure 1: the quiet-row FAIL at 07:24:03, the release build (binary modified 07:24:19), the
thrown-away run at 07:25:18, and about seven minutes since boot. Recorded between procedure 1 and
procedure 2: both non-negotiable-4 scripts under `strace` (started 07:50:59 and 07:51:01), a
`check-machine.sh` read at 07:51:32, the two single `userspace` runs at 07:53, the seven single
admin runs at 07:54:28–07:55:19 with the manager's editor session active, and 38 minutes since
boot by 07:56:27. Nothing was varied to separate any of them. The transferable half is
[a-tight-spread-inside-one-procedure-did-not-reproduce-across-two.md](a-tight-spread-inside-one-procedure-did-not-reproduce-across-two.md):
**a per-procedure spread bounds dispersion inside one procedure, not between procedures**, and one
twenty-run median is one observation. `STATUS.md` open item **85**.

### What is not proven

- **A cause for either finding.** Named above; nothing was varied.
- **Reproducibility beyond two procedures in one boot.** Two is enough to show a figure moving,
  not enough to say where it settles; `DESIGN.md` §8 publishes both rather than choosing.
- **The commit under test, from the procedure's own output.** HEAD and a clean tree were read by
  the manager's session around the build, not printed by `scripts/w2w-baseline.sh` or `w2w`.
- **A NIC, another kernel, another suite.** Loopback; `7.0.0-31-generic`; `TLS13_AES_128_GCM_SHA256`
  only. The plan's optional step 8 — other suites against the kernel — **did not run**.
- **`userspace` allocations in the procedures themselves.** The 80 000 is from two single runs;
  the baseline does not count it per run.
- **`standard` under userspace TLS, and `standard` on the application path under TLS.** Not in the
  arm list.
- **An initiator under TLS in `hft`.** `tools/w2w` is an acceptor and a client; `connect_and_serve_tls`
  is `standard` only, and nothing here measured it.
- **What a rekey costs.** Nothing here sent a `KeyUpdate` inside a timed window.

## Boot B, 2026-09-15: two procedures, the first hardware stamps, and what conntrack and EEE cost

`[measured 2026-09-15]` boot B of
[plans/2026-09-04-the-second-linux-desk.md](../plans/2026-09-04-the-second-linux-desk.md): the
journal and log's added cost (B5), the first head-to-head against `matthart1983/nanofix` (B8),
one netfilter suspect tested (B7), the cable to the Mac (B6), a spin-loop profile (B9, diagnostic)
and a third procedure of B2. Raw logs and per-run output:
`target/w2w-baseline/boot-b-*` (gitignored) and the scratchpad directories the plan names.

### Settings in force for every figure below

- Machine: the §9 desktop, AMD Ryzen 7 3700X, kernel `7.0.0-31-generic`, boot line
  `isolcpus=6,7,14,15 rcu_nocbs=6,7,14,15 processor.max_cstate=1` (no `nohz_full`, mitigations
  on), `fixbolt-machine on` (governor performance, boost 0, SMT off, THP never, busy_poll/busy_read
  50), `enp9s0` IRQs 85–89 on cpu4, `rx-usecs 0`, EEE off at the desk.
- `FIXBOLT_NIC=enp9s0 scripts/check-machine.sh` → `pass 15 fail 0 unknown 0`; the busy row is
  re-read before every run (3% ceiling); **0 runs disqualified** in any procedure below. **Except
  B6's procedure 1 1 s wire arms**, whose own summaries read `pass 14 fail 1 unknown 0` (below).
- Commit `5ca3889`, clean tree, for every w2w figure. Desk `w2w` sha256 `350d3c17320f` (release,
  `--features affinity`, `cap_net_raw,cap_net_admin+ep`); Mac `w2w` sha256 `7b2b52cb9be7` at the
  same commit.
- Engine pinned to cpu6, local client to cpu7; `/tmp` (where `--journal file-async` and `--log
  file` write) is **tmpfs**.
- Procedure
  ([ADR-0068](../decisions/ADR-0068-a-published-figure-is-two-procedures-shown-side-by-side.md)):
  `scripts/w2w-baseline.sh`, 20 runs × 20 000 timed round trips, 2 000 warmup, `GAP` 8 s, unless
  noted. Procedure 1: 2026-09-15 00:58–02:54; procedure 2: 02:54–04:47. Reproduced = both medians
  within 5% of the smaller at that percentile.

### B5 — journal and log

p50 ns; "none" is B2's app arm of the same procedure; added = arm − none.

| arm | proc 1 p50 (added) | proc 2 p50 (added) | p99 proc 1 / 2 | p99.9 proc 1 / 2 | verdict |
|---|---|---|---|---|---|
| hft app, none | 20 219 | 18 951 | 24 857 / 23 354 | 26 700 / 25 082 | not reproduced |
| hft app, `--journal file-async` | 20 714 (+495) | 19 537 (+586) | 24 887 / 23 745 | 26 866 / 25 473 | not reproduced (p50 6.0, p99.9 5.5) |
| hft app, `--log file` | 21 120 (+901) | 19 983 (+1 032) | 25 393 / 24 151 | 27 236 / 25 879 | not reproduced (5.7 / 5.1 / 5.2) |
| standard app, none | 21 005 | 19 918 | 25 844 / 24 732 | 27 372 / 26 175 | not reproduced (p50) |
| standard app, `--journal file-async` | 21 455 (+450) | 20 488 (+570) | 25 814 / 24 807 | 27 257 / 26 385 | reproduced (4.7 / 4.1 / 3.3) |
| standard app, `--log file` | 21 901 (+896) | 20 840 (+922) | 26 285 / 25 062 | 27 813 / 26 685 | not reproduced (p50 5.1) |

Reading: the *added* term has the same sign in both procedures and both modes, but only one of the
four arms reproduced — `standard` app `--journal file-async` (+450 → +570 ns, 4.7%); `hft` app's
journal and log, and `standard` app's log, did not (`hft` log's added term moved **+901 →
+1 032 ns, 14.5%**). File-async journal added **+450 to +586 ns** per application round trip,
`FileLog` **+896 to +1 032 ns** per round trip — one inbound record and one outbound record, so
half of that is per direction, **not separately measured** (against §8's `~340 ns [unmeasured]`).
The journal is `FileJournal<4096, 512>`, `Durability::Async` (item 88
fixed, step S1), on tmpfs — **not a disk figure**.

### B8 — against `matthart1983/nanofix` (loopback split, `standard`)

fixbolt: `w2w-baseline.sh` with `LISTEN=127.0.0.1:0 ARMS="standard:admin standard:app"` (engine
half on cpu6, `--connect` generator on cpu7). nanofix: the example acceptor
`vendor/nanofix/target/release/examples/fixbolt_w2w_acceptor` (nanofix `0f79bae`, sha256
`a27c5c94439d`), a fresh process per run under `taskset -c 6`, the same `w2w --connect` on cpu7,
the same busy check, GAP and counts. Driver script `scratchpad/b8-nanofix.sh`, quoted verbatim at
the end of this section — **it lives nowhere else in the repository**. ns p50 / p99 / p99.9:

Admin path:

| arm | procedure 1 | procedure 2 | diff | verdict |
|---|---|---|---|---|
| fixbolt admin | 19 522 / 24 321 / 25 839 | 18 480 / 23 204 / 24 717 | 5.6 / 4.8 / 4.5 % | not reproduced (p50) |
| nanofix admin | 19 106 / 22 172 / 22 899 | 18 655 / 21 495 / 22 342 | 2.4 / 3.1 / 2.5 % | reproduced |

Application path:

| arm | procedure 1 | procedure 2 | diff | verdict |
|---|---|---|---|---|
| fixbolt app | 21 005 / 25 949 / 27 307 | 19 963 / 24 692 / 26 310 | 5.2 / 5.1 / 3.8 % | not reproduced (p50, p99) |
| nanofix app | 19 286 / 22 318 / 23 099 | 18 850 / 21 851 / 22 688 | 2.3 / 2.1 / 1.8 % | reproduced |

Same-procedure differences, fixbolt − nanofix, ns (proc 1 / proc 2):

| | p50 | p99 | p99.9 |
|---|---|---|---|
| admin | +416 / −175 (sign changes: no difference claimed) | +2 149 / +1 709 | +2 940 / +2 375 |
| app | +1 719 / +1 113 | +3 631 / +2 841 | +4 208 / +3 622 |

nanofix's own app path costs +180 / +195 ns over its own admin path (proc 1 / proc 2); fixbolt's
app path costs +1 483 / +1 483 over its own admin path. **Both `standard`, both blocking on
kernel TCP, loopback, one session.** `prior-art.md`'s claim rows are not changed by this.

The driver script, verbatim — it lives nowhere else in the repository:

```bash
#!/usr/bin/env bash
# B8: `tools/w2w --connect` against vendor/nanofix's example acceptor, shaped like
# scripts/w2w-baseline.sh's loopback split run: a fresh acceptor per run pinned to
# ENGINE_CORE, the generator pinned to CLIENT_CORE, the busy check per run (3%
# ceiling, same function), GAP seconds between runs, every run's output kept, and
# median + two-sided dispersion from the script's own functions.
set -uo pipefail
cd /home/tmt/Projects/nanofixengine
RUNS=${RUNS:-20}; MESSAGES=${MESSAGES:-20000}; WARMUP=${WARMUP:-2000}; GAP=${GAP:-8}
ENGINE_CORE=${ENGINE_CORE:-6}; CLIENT_CORE=${CLIENT_CORE:-7}; PORT=${PORT:-19876}
PATHS=${PATHS:-"admin app"}
OUT_DIR=${OUT_DIR:?}
ACC=vendor/nanofix/target/release/examples/fixbolt_w2w_acceptor
BIN=target/release/w2w
mkdir -p "$OUT_DIR"
BASELINE_SOURCE_ONLY=1 . scripts/w2w-baseline.sh   # median, dispersion (it cd-s and sets -e)
set +e -u -o pipefail; cd /home/tmt/Projects/nanofixengine
busy_pct() {
  read -r _ a b c idle rest < /proc/stat
  local t0=$((a+b+c+idle)) i0=$idle
  sleep 1
  read -r _ a b c idle rest < /proc/stat
  local t1=$((a+b+c+idle)) i1=$idle
  local dt=$((t1-t0)) di=$((i1-i0))
  [ "$dt" -le 0 ] && { echo 100; return; }
  echo $(( (100*(dt-di)) / dt ))
}
scripts/check-machine.sh
echo "runs $RUNS   messages $MESSAGES   warmup $WARMUP   gap ${GAP}s"
echo "commit $(git rev-parse --short HEAD)   tree $( [ -z "$(git status --porcelain)" ] && echo clean || echo "$(git status --porcelain | wc -l) paths")"
echo "uptime $(awk '{printf "%d:%02d", $1/3600, ($1%3600)/60}' /proc/uptime)"
echo "binary $(sha256sum "$BIN" | cut -c1-12) $(date -r "$BIN" -Iseconds)"
echo "acceptor nanofix $(git -C vendor/nanofix rev-parse --short HEAD) $(sha256sum "$ACC" | cut -c1-12) $(date -r "$ACC" -Iseconds)"
echo "output $OUT_DIR"
echo "engine cpu$ENGINE_CORE (taskset, whole acceptor process)   client cpu$CLIENT_CORE"
for path in $PATHS; do
  mins=(); p50s=(); p99s=(); p999s=(); skipped=0
  for i in $(seq 1 "$RUNS"); do
    b=$(busy_pct)
    if [ "$b" -gt 3 ]; then
      printf '  nanofix  %-5s run %2d  DISQUALIFIED, %s%% busy\n' "$path" "$i" "$b"
      skipped=$((skipped+1)); sleep "$GAP"; continue
    fi
    alog="$OUT_DIR/nanofix-$path-run-$i-acceptor.txt"
    taskset -c "$ENGINE_CORE" "$ACC" "$PORT" >"$alog" 2>&1 &
    apid=$!
    for _ in $(seq 1 50); do grep -q 'listening on' "$alog" && break; sleep 0.1; done
    grep -q 'listening on' "$alog" || { cat "$alog"; kill "$apid"; echo "FAIL: acceptor never listened"; exit 1; }
    rc=0
    out=$("$BIN" --connect "127.0.0.1:$PORT" --path "$path" --client-core "$CLIENT_CORE" \
            --messages "$MESSAGES" --warmup "$WARMUP" 2>&1) || rc=$?
    kill "$apid" 2>/dev/null; wait "$apid" 2>/dev/null
    echo "$out" > "$OUT_DIR/nanofix-$path-run-$i.txt"
    echo "$out" | grep -qx "path: $path" || { echo "$out"; echo "FAIL: generator ran a path other than $path"; exit 1; }
    echo "$out" | grep -qE '^ *allocs +0 ' || { echo "$out"; echo "FAIL: allocs != 0 (generator)"; exit 1; }
    [ "$rc" -eq 0 ] || { echo "$out"; echo "FAIL: connect exit $rc"; exit 1; }
    g() { echo "$out" | awk -v k="$1" '$1==k {print $2}'; }
    mins+=("$(g min)"); p50s+=("$(g p50)"); p99s+=("$(g p99)"); p999s+=("$(g p99.9)")
    printf '  nanofix  %-5s run %2d  %s%% busy   min %8s  p50 %8s  p99 %8s  p99.9 %8s\n' \
      "$path" "$i" "$b" "$(g min)" "$(g p50)" "$(g p99)" "$(g p99.9)"
    sleep "$GAP"
  done
  n=${#p50s[@]}
  {
    echo
    echo "  == nanofix / $path: median of $n qualifying runs ($skipped disqualified) =="
    [ "$n" -gt 0 ] && {
      echo "     min    $(printf '%s\n' "${mins[@]}" | median) ns"
      echo "     dispersion $(dispersion p50 "${p50s[@]}")"
      echo "     dispersion $(dispersion p99 "${p99s[@]}")"
      echo "     dispersion $(dispersion p99.9 "${p999s[@]}")"
    }
    echo "     machine $(scripts/check-machine.sh 2>/dev/null | grep -E '^pass [0-9]+')"
  } | tee -a "$OUT_DIR/summary.txt"
done
```

### B7 — item 51, suspect 1: conntrack on loopback (A–B–A, A/B only, not a §8 figure)

`[measured 2026-09-15]` before: `nft list tables` → six tables (`ip`/`ip6` × `filter`, `nat`,
`mangle`, Tailscale and iptables-nft); `warp-cli settings` → `Mode: DnsOverHttps`;
`nf_conntrack_count` 53. B: `nft add table ip fixbolt` with chains `pre` (prerouting, priority
raw) `iif "lo" notrack` and `out` (output, priority raw) `oif "lo" notrack`. After removal the
ruleset is identical to before except packet counters (diff: counter lines only). The flush arm
was **skipped** — the owner was not at the desk (Q2). 2026-09-15 05:03–05:29, commit `5ca3889`.

`crates/engine/benches/payload.rs` binary run directly (sha256 `e9633d31e344`, 5 runs per phase,
the harness's per-run best), ns/op:

| case | A1 before | B notrack | A2 after |
|---|---|---|---|
| TCP loopback, 8 in 8 out | 12 596.9 12 583.2 12 600.9 12 594.4 12 588.9 | 12 173.8 12 175.0 12 158.1 12 175.2 12 212.7 | 12 575.7 12 570.9 12 603.0 12 597.8 12 604.6 |
| TCP loopback, 83 in 87 out | 12 652.3 12 686.9 12 657.9 12 674.0 12 642.6 | 12 258.6 12 272.4 12 261.8 12 238.8 12 250.6 | 12 662.7 12 678.9 12 672.2 12 629.1 12 677.0 |
| TCP loopback, 149 in 191 out | 12 676.6 12 721.0 12 745.7 12 690.3 12 725.3 | 12 298.9 12 288.9 12 287.3 12 314.0 12 291.2 | 12 713.6 12 696.3 12 670.4 12 727.9 12 739.4 |
| TCP loopback, 8192 in 8192 out | 14 674.1 14 717.7 14 844.5 14 809.6 14 910.8 | 14 292.4 14 294.6 14 542.8 14 894.5 14 331.7 | 14 681.7 14 701.8 14 693.9 14 629.6 14 810.0 |

conntrack on `lo` costs **~420 ns** — **3.3%** of the 12.6 µs 8-byte TCP loopback round trip and
about **4%** of item 51's ~10.2 µs gap, A1 ≈ A2. Item 51's gap is not explained by it.

`w2w-baseline.sh RUNS=10 ARMS=hft:admin` (loopback, combined), per-run p50 ns:

| phase | per-run p50 | median p50 / p99 / p99.9 |
|---|---|---|
| A1 before | 18 285 18 255 18 184 15 720 18 305 18 314 16 832 15 669 18 304 18 244 | 18 249 / 22 001 / 24 696 |
| B notrack | 15 199 15 259 15 279 15 350 15 499 15 379 15 710 15 539 15 409 15 239 | 15 364 / 20 313 / 23 955 |
| A2 after | 18 185 16 612 18 205 18 164 18 245 18 174 18 124 18 155 18 225 18 204 | 18 179 / 21 936 / 24 521 |

Bimodal in both A phases — most runs ~18.2 µs, some ~15.7 µs — and never slow under notrack.
Procedure 3 (15 133 median, every run 15 048–15 229) ran 04:48–05:01 **without** payload bench
runs before it; A1 ran right after five payload runs. Candidate, not a cause: conntrack state left
by the payload bench's connections makes some later connections take a ~3 µs slower path.
`nf_conntrack_count`: 53 before, 30 under notrack, 51 after.

### B6 — over the cable: A/B, and the interval-0 failure record

`[measured 2026-09-15]` topology, per-run counters and the `eee` gate are recorded at
[DESIGN.md §9](../DESIGN.md) and [hft-playbook.md §4](../hft-playbook.md) (the +14.6 µs A/B is
committed there). Two things from B6 are recorded only here.

**The two 1 s procedures ran close together.** Procedure 1's 1 s arms ran 05:32–06:10, procedure
2's ran 06:20–06:58: starts 48 minutes apart, but only 10 minutes between the end of one and the
start of the other — short of ADR-0068's rule 4 (at least 30 minutes, one pass through other
steps). The pair (45 146 ‖ 42 918) did not reproduce anyway. **The two procedures also carry a
different machine verdict**: procedure 1's 1 s wire arms' own summaries (`b6-1/wire-1s.log`) both
read `machine pass 14 fail 1 unknown 0` — the printed header block read `pass 15`, because the
script took its verdict from a second `check-machine.sh` run (fixed in code now), and which row
failed there is not recorded; procedure 2's carry `pass 15 fail 0 unknown 0`.

**Interval 0 wire: not measured.** Every attempt FAILed on a missing TX stamp before completing
an arm: procedure 1 run 1 (`hw-tx-missing 1`), procedure 2 run 2 (1), and in the A/B busy0 run 1
(1), irq6 run 3 (10), eee-on run 3 (1), eee-off run 4 (1). `tx_hwtstamp_skipped` rose 0 → 1 → 5 →
29 → 50 → 52 across them: igb holds one TX timestamp at a time and skips the next request when
one is pending. The script's rule (Sửa 2) fails any run with a missing stamp. **A failing run's
own wire p50 is not a figure** — each carries `hw-tx-missing ≥ 1` — and is dropped below. The runs
that did complete clean (20 000 round trips, `hw-rx-missing 0 hw-tx-missing 0`), all diagnostic and
not reproduced, wire p50 ns: EEE off, busy_poll 50, IRQs on cpu4 — **26 178–26 218 ns, n = 4**
(26 186 procedure 2 run 1; 26 178, 26 210, 26 218 the interval-0 EEE-off attempt's runs 1–3); IRQs
on cpu6 — **29 082, 29 138 ns, n = 2**, about +2.9 µs over the EEE-off runs; EEE on — **39 522,
39 546 ns, n = 2**, about +13.3 µs. `busy_poll`/`busy_read` 0 has **no clean run** — it failed at
run 1 — so the plan's busy_read A/B has **no result**. The 2 000-round-trip smoke (24 562, 26 042)
and the discard run (26 306) are not comparable and are excluded from every range above. **Only
the admin arm ever ran at interval 0** — the script stops at the first FAIL, so the application
arm never started.

**A/B, one procedure each, RUNS=10, 07:08–07:49, differences only
([ADR-0068](../decisions/ADR-0068-a-published-figure-is-two-procedures-shown-side-by-side.md)
rule 4).** The plan's B6 gate asked for two A/B arms — `busy_read` and IRQs on the engine core —
plus `tx_hwtstamp_skipped` read per run; both A/B arms in fact ran at interval 0, after both 1 s
procedures had already failed there, and `tx_hwtstamp_skipped` was read once per procedure rather
than per run:

- EEE on at the desk (`ethtool --set-eee enp9s0 eee on`, link bounced, `enabled - active`, Mac
  media gained `energy-efficient-ethernet`) vs EEE off again in the same hour, hft admin, 1 s:
  wire p50 **54 310** (51 730..57 874) vs **39 714** (37 978..41 122) → **+14.6 µs**, ≈ the
  16.5 µs 1000BASE-T wake time; counterparty p50 388 354 vs 374 937. *A/B only, never a
  figure — the published 1 s pair is 45 146 ‖ 42 918.* Over Q15's 5% threshold →
  §9 row + a `check-machine.sh` `eee` row (step Q15, this PR).
- IRQs on cpu6: interval 0 only, failed at run 3 on a missing TX stamp; the two clean runs before
  it read wire p50 **29 138, 29 082** against the EEE-off runs' 26 178–26 218 — about +2.9 µs
  each, **diagnostic, n = 2**.
- `busy_poll`/`busy_read` 0: interval 0 only, failed at run 1 on a missing TX stamp — **no clean
  run, no result**.
- The 1 s EEE-off arm read 39 714 while procedures 1 and 2 read 45 146 and 42 918 an hour
  earlier: the 1 s wire figure moves ~14% across the morning.

### B9 — perf on the engine thread, diagnostic

`[measured 2026-09-15]` `hft`, loopback, 400 000 round trips, 4 s at 4 999 Hz, `-g`:
`sudo perf record -F 4999 -g -t <engine-tid> -- sleep 4`; 19 945 samples (admin), 19 920 (app).
Share of engine-thread samples, children (cumulative):

| symbol | admin | app |
|---|---|---|
| `entry_SYSCALL_64_after_hwframe` | 89.30 % | 75.34 % |
| `fixbolt_engine::Acceptor::accept` | **49.93 %** | **36.70 %** |
| `TcpTransport::send` → `__x64_sys_sendto` | 30.65 % → 29.57 % | 32.18 % → 31.19 % |
| `__x64_sys_accept4` → `do_accept` | 27.65 % → 25.39 % | 18.27 % → 16.74 % |
| `TcpTransport::recv` → `__x64_sys_recvfrom` | 12.74 % → 10.70 % | 10.37 % → 8.08 % |
| `sock_alloc_file` (under `do_accept`) | 10.94 % | 7.48 % |
| `nft_do_chain` (self) | 1.27 % | 0.98 % |

Top self symbols, app: `fixbolt_session::scan_fields::<256>` 5.40 %, `Session::judge` 1.75 %,
`Template<32,512>::encode_with` 1.74 %, `MessageView::get` 1.15 %, `parse_into::<Fix44,256>`
1.03 %. Top self, admin: `__memcg_slab_post_alloc_hook` 3.15 %, `srso_safe_ret` 3.04 %,
`srso_return_thunk` 2.81 %, `native_queued_spin_lock_slowpath` 2.45 %, `evict` 1.19 %, `__fput`
1.18 %, `inode_init_always_gfp` 1.11 %, `do_accept` 1.07 %.

Reading, **diagnostic — not a cause**: the `hft` engine thread spins, so these are shares of the
spin loop, not per-message latency. `Acceptor::accept` (cumulative, including its own code and the
syscall it makes) is **49.9%** of admin samples and **36.7%** of app samples; the `accept4`
syscall itself is **27.7%** and **18.3%**. A non-blocking `accept4` on an empty listener allocates
a socket file and inode (`sock_alloc_file`, `inode_init_always_gfp`) and frees it (`__fput`,
`evict`) before returning `EAGAIN`, as the callchain shows. What that does to the round trip — how
long a turn is when a request lands — is not measured.

### B2, procedure 3

`[measured 2026-09-15]` **Procedure 3, diagnostic only (not a published column), B2 again
04:48–05:01, same commit:** p50 hft admin 15 133, hft app 18 961, standard admin 18 856, standard
app 20 319. `hft` matches procedure 2 within 0.1%; `standard` sits 2.0–2.2% above procedure 2 and
below procedure 1. So procedure 1 is the outlier, not a monotone drift. Candidate recorded, not a
cause: procedure 1 started ~7 minutes after the build slot (cargo builds, rustdoc, the Mac
rebuild) ended.

### What is not proven

- **An interval-0 wire figure on this NIC with this procedure.** Every attempt FAILed on a
  missing TX stamp (B6); the closest thing is diagnostic single runs.
- **The 1 s wire figure at a single setting.** It moved ~14% across one morning (B6); only one
  A/B was run per variable, so nothing separates that drift from EEE, busy_poll/IRQ placement or
  the hour itself.
- **A cause for either loopback bimodality.** B7's "conntrack state left by the payload bench"
  and B2/B8's "procedure 1 started ~7 minutes after the build slot" are both candidates recorded,
  not causes; nothing was varied to isolate either.
- **Any latency effect from B9's profile.** It is a share of the spin loop, not a per-message
  timing; what a non-blocking `accept4`'s allocate-and-free costs a round trip is not measured.
- **Item 51's second suspect.** Suspect 1 (conntrack) is 3.3% of the 12.6 µs 8-byte TCP loopback
  round trip, about 4% of item 51's ~10.2 µs gap; the CPU speculation mitigations (suspect 2) were
  not tested this boot, and the flush arm (Tailscale + full ruleset) was skipped — the owner was
  not at the desk.
- **Whether fixbolt's admin path actually costs more than nanofix's.** The sign of the
  same-procedure difference flipped between procedures (+416 / −175 ns); no difference is
  claimed at admin p50.
- **Item 89's effect on the per-message path and on accept latency.** The cadence is built
  (`Limits::listener_every`, `pump`'s `ListenerCadence`, ADR-0069, Proposed) with default 1, so
  nothing on the hot path changed by shipping it. What N saves at N > 1, and what it costs a
  fresh connect, is unmeasured: boot C runs the A/B, N ∈ {1, 16, 256}, per ADR-0068.
  **`[measured 2026-09-18]` answered by boot C, next section: −12.7 ‖ −13.0% on the application
  path at N = 16, +1.9 ‖ +1.7% on the administrative one, both reproduced.**

## Boot C, 2026-09-18: the listener cadence, N ∈ {1, 16, 256}, two procedures

`[measured 2026-09-18]` step 5 of
[plans/2026-09-18-polling-the-listener-less-often-than-the-sessions.md](../plans/2026-09-18-polling-the-listener-less-often-than-the-sessions.md),
the A/B [ADR-0069](../decisions/ADR-0069-the-listener-is-polled-on-a-cadence-in-hft.md)
decision 4 asked for before any default could move. Raw output:
`target/w2w-baseline/boot-c-p{1,2}-n{1,16,256}/summary.txt` and the per-run `.txt` files beside
them; driver `target/boot-c-evidence/c89.sh`; gate logs
`target/boot-c-evidence/c89-nks-{default,256}-tls.log` (all gitignored — `target/` is where boot
evidence lives, because `/tmp` is tmpfs on this desk).

### Settings in force for every figure below

- Machine: the §9 desktop, AMD Ryzen 7 3700X, kernel `7.0.0-31-generic`, the boot B line
  (`isolcpus=6,7,14,15 rcu_nocbs=6,7,14,15 processor.max_cstate=1`, no `nohz_full`, mitigations
  on, `fixbolt-machine on`). `FIXBOLT_NIC=enp9s0 scripts/check-machine.sh` → **`pass 16 fail 0
  unknown 0`** — one more row than boot B's 15 (the EEE row, added 2026-09-15) — read before every
  run; runs that failed the busy row are the non-qualifying ones in the counts below.
- Code: commit **`55a1549`** (branch `plan/the-second-linux-desk-c`, the merge of PR #76), tree
  clean except untracked docs; `w2w` release with `--features affinity,tls`; the same binary for
  every arm and both procedures.
- Loopback, `hft`, engine on `cpu6`, client on `cpu7`, `scripts/w2w-baseline.sh` with
  `LISTENER_EVERY=N`, **10 runs × 20 000 timed round trips**, 2 000 warmup, `GAP` 8 s.
- Two procedures per ADR-0068, each running all three N on both paths; **procedure 1 in the
  order N = 1, 16, 256, procedure 2 in the order 256, 16, 1**, so an order or warm-up effect would
  appear as a cross-procedure disagreement rather than hide inside both.

### The table

ns, p50 / p99 / p99.9; qualifying runs of 10 in parentheses; reproduced = both medians within 5%
of the smaller at every published percentile.

| N | path | procedure 1 | procedure 2 | largest diff | reproduced? |
|---|---|---|---|---|---|
| 1 | admin | 16 070 / 21 050 / 22 753 (9) | 16 080 / 20 719 / 22 798 (10) | p99 1.6% | yes |
| 16 | admin | 16 381 / 21 591 / 23 695 (9) | 16 361 / 20 970 / 22 923 (9) | p99.9 3.4% | yes |
| 256 | admin | 16 376 / 21 410 / 24 060 (10) | 16 361 / 21 531 / 23 866 (9) | p99.9 0.8% | yes |
| 1 | app | 20 209 / 25 188 / 27 202 (9) | 20 228 / 25 198 / 26 956 (8) | p99.9 0.9% | yes |
| 16 | app | 17 644 / 22 763 / 26 535 (8) | 17 603 / 22 628 / 26 765 (10) | p99.9 0.9% | yes |
| 256 | app | 17 608 / 22 603 / 27 242 (10) | 17 633 / 22 773 / 26 285 (10) | p99.9 3.6% | yes |

Six arms, six reproduced — the first boot on this desk where every zero-interval loopback arm
did (boot B: nine of ten did not, all faster the second time). Nothing was changed to earn
that; it is recorded, not explained, and it makes item 85's *build slot* candidate neither
stronger nor weaker (no build preceded either procedure here).

### The differences, same procedure, N against N = 1

| | admin p50 | admin p99 | app p50 | app p99 | app p99.9 |
|---|---|---|---|---|---|
| N = 16, procedure 1 | **+1.9%** (+311 ns) | +2.6% | **−12.7%** (−2 565 ns) | −9.6% | −2.5% |
| N = 16, procedure 2 | **+1.7%** (+281 ns) | +1.2% | **−13.0%** (−2 625 ns) | −10.2% | −0.7% |
| N = 256, procedure 1 | +1.9% | +1.7% | −12.9% | −10.3% | +0.1% |
| N = 256, procedure 2 | +1.7% | +3.9% | −12.8% | −9.6% | −2.5% |
| N = 256 against N = 16 | −0.0 / 0.0% | −0.8 / +2.7% | −0.2 / +0.2% | −0.7 / +0.6% | +2.7 / −1.8% |

- **The application path's 2.6 µs is one `accept4`.** B9 put `Acceptor::accept` at 36.7% of the
  application path's engine-thread samples; at N = 1 a request that lands while the thread is
  inside `accept4` — allocating a socket file and an inode and freeing both before `EAGAIN` —
  waits for it. Thinning the ask 16× removes almost all of that wait, and 256× removes nothing
  more that this procedure can see (≤ 0.3% at p50).
- **The administrative path's +0.3 µs is real and unexplained.** It reproduced (+1.9 ‖ +1.7%),
  it is the same sign at N = 256, and the only difference from the application arm is the
  message. One candidate, recorded as such: without an `accept4` between polls the engine's
  `recvfrom` re-takes the socket's user lock more often, so more loopback segments arrive while
  the socket is owned and take the backlog path (`tcp_v4_rcv` → `sk_add_backlog`, processed at
  `release_sock`) instead of being processed in the sender's softirq — a few hundred nanoseconds
  each. The application path would pay the same and hide it inside its 2.6 µs. **Not tested**:
  `perf stat` on the backlog path, or `busy_read` 0 versus 50, at N = 1 and 16 would decide it.
  Open, in `STATUS.md` item 89's closing row.
- **A trap, found by C-49's syscall counts and worth one sentence here**: `tools/w2w`'s
  `--listener-every` defaulted to **1** in its own argument parser and did not read `Limits`'
  default, so the moment the library's default moved to 16 (this step), every later `w2w` run
  without the flag — C-84, C-85, C-49 in this boot — ran at N = 1 while the library shipped 16.
  The three are labelled N = 1 where they are published; as of the next commit `w2w` takes
  `Limits`' default. Two copies of a default are two defaults (ADR-0069 *Bad*, the second copy).
- **Connect and logon did not move with N**: per-run `connect-rtt` 68–85 µs and `logon-rtt`
  1.07–1.12 ms in every run of every arm. One connect per run, on the client's clock, through
  the pre-session stage; it bounds what a fresh connection experiences and is not the accept
  delay (ADR-0069 consequence 2), which no instrument here reads.

### Rule 4, with and without the cadence

`scripts/check-no-kernel-sleep.sh` green in both configurations, `w2w` built with
`--features affinity,tls`:

| | `accept4` | `recvfrom` | `sendto` | log |
|---|---|---|---|---|
| default | 7 901 | 7 900 | 351 | `target/boot-c-evidence/c89-nks-default-tls.log` |
| `W2W_EXTRA="--listener-every 256"` | 7 970 | 8 320 | 351 | `target/boot-c-evidence/c89-nks-256-tls.log` |

`strace -c` on the engine tid over the whole `hft` half of the script, which includes the idle
hold after the 300 messages. In the hold `Spin::idle` resets the countdown every iteration, so
the listener is asked as often as at N = 1 — which is why the two `accept4` totals are within 1%.
**The `accept4` count inside the loaded window alone is derived** — the gap between `recvfrom`
and `sendto` grows from 7 549 to 7 969 with the cadence, consistent with 256 polls of the socket
per ask of the listener — **not observed separately**; the script does not split the trace by
window. No name from `SLEEPERS` appeared in either run.

### C-PRD7 — the wakeup, measured: `epoll_wait` 4 960 ns, `poll` 4 819 ns at p50

`[measured 2026-09-18]` `crates/engine/benches/wakeup.rs` (step 4.2 of
[closing-the-open-items](../plans/2026-09-18-closing-the-open-items.md)), `WAKEUP_CORES=6,7`,
commit `85460c1`'s code, `pass 16 fail 0 unknown 0`; one thread on cpu6 writes one byte and
records the instant, the other on cpu7 returns from `epoll_wait` (arm 1) or `poll` (arm 2) and
reads the same clock; 20 000 wakes per run, 20 runs, the median of the 20 per-run p50s:

| wait | p50 (median of 20) | per-run p50 range | max/median | p99 | p99.9 | max |
|---|---|---|---|---|---|---|
| `epoll_wait` | **4 960 ns** | 4 949 .. 4 980 | 1.004 | ≈ 7.8–7.9 µs | ≈ 8.9 µs | ≈ 10.3 µs |
| `poll` | **4 819 ns** | 4 809 .. 4 829 | 1.004 | ≈ 7.8–7.9 µs | ≈ 8.9 µs | ≈ 10.3 µs |

This is the number `DESIGN.md` §8's *Cost of a wakeup* row carried from the literature as
2–5 µs since it was written, and ADR-0014 open question 1 / ADR-0025 open question 2 asked
for. It reads the top of the range. `poll` is 141 ns cheaper than `epoll_wait` at p50 here, one
fd each. Baseline: `benches/baselines.tsv`, `wakeup epoll_wait` 4 960, margin **1.10** (the
ladder's floor; the measured max/median of 1.004 is far under it), n = 20. ADR-0025 accepts on
this number and keeps its ceiling of 4 (the arithmetic crossover with a 448.9 ns idle turn is
N ≈ 11; the busy turn at N > 1 is still unmeasured, so the ceiling is not raised).

### C-85 — the build-slot candidate, tested with one variable, and it is dead

`[measured 2026-09-18]` item 85's only recorded candidate for the 2026-09-14/15 drift was
*"procedure 1 began minutes after a build slot"*. One variable: **procedure 1 started under two
minutes after `cargo build --release` finished; procedure 2 after ten minutes of an idle
machine**; `hft` admin, `off`, listener cadence N = 1 (`w2w`'s own default, see C-89's trap), 10 runs ×
20 000 each, same commit
`85460c1`, same binary, `pass 16 fail 0 unknown 0`.

| | procedure 1 (< 2 min after the build) | procedure 2 (10 min idle) | diff |
|---|---|---|---|
| p50 | 16 005 | 15 990 | 0.09% |
| p99 | 21 000 | 20 794 | 0.99% |
| p99.9 | 22 978 | 22 312 | 2.99% |

Reproduced at every percentile; **the build slot / thermal candidate is refuted** — a build
immediately before a procedure moved nothing this procedure can see. The header's new
thermal/frequency line read `thermal iwlwifi_1 65000 cpu6-freq n/a` on this desk: **there is no
CPU thermal zone and no `cpuinfo_cur_freq`** exposed (the governor is `performance`, boost 0), so
those two columns cannot help here and say so rather than printing a number from the wrong
sensor. What moved ten arms by 4.7–6.7% on 2026-09-15 remains unexplained; item 85's guard
(ADR-0068 pairs, the comparator script) is what stands between it and a published figure.

### C-84 — TLS with one mode per end: where the 9 µs lives

`[measured 2026-09-18]` four arms, `hft` admin, loopback, `tools/w2w --tls <engine> --client-tls
<client>`, 10 runs × 20 000, two procedures in opposite arm order, commit `85460c1`'s code,
`pass 16 fail 0 unknown 0`; `w2w` at listener cadence **N = 1** (its `--listener-every`
default, which did not follow `Limits`' 16 — C-89's trap, below). Every arm reproduced (largest
difference 2.9%, at p99.9). p50 ns, procedure 1 ‖ procedure 2; *added* is against the same
procedure's `off` arm at the same cadence, boot C N = 1 (16 070 ‖ 16 080):

| engine | client | p50 | added over `off` | against userspace/userspace |
|---|---|---|---|---|
| kTLS | kTLS | 25 503 ‖ 25 473 | +9 433 ‖ +9 393 | +4 679 ‖ +4 754 |
| userspace | userspace | 20 824 ‖ 20 719 | +4 754 ‖ +4 639 | — |
| kTLS | userspace | 21 175 ‖ 21 250 | +5 105 ‖ +5 170 | **+351 ‖ +531** (the engine's kTLS) |
| userspace | kTLS | 22 177 ‖ 22 147 | +6 107 ‖ +6 067 | +1 353 ‖ +1 428 (the client's kTLS) |

- **The engine's kTLS costs the engine 0.35–0.53 µs over its own userspace `rustls`**, with
  the same client — about 2% of the round trip. That is the whole engine-side price of ADR-0005
  decision 2.
- **The client's kTLS costs the client ~1.4 µs** — `w2w`'s client is a plain blocking thread
  reading one record at a time.
- **Both kTLS is superadditive.** Additive prediction from the one-sided costs: 20 824 + 351 +
  1 353 = 22 528 (‖ 22 678); measured 25 503 (‖ 25 473): **+2 975 ‖ +2 795 ns beyond the sum**.
  Candidate, not isolated: each kernel record layer decrypts in `recvmsg` only once the whole
  record is queued, so two of them on one loopback pair serialise where two userspace layers
  overlap their work with the socket.
- The mixed arms **print** their allocation counts and do not assert them: 4 001 on the client
  thread in the userspace-client arms, the client's own `rustls`, outside the engine; the
  symmetric arms keep their assertions (kTLS 0, userspace 4 per round trip on the engine).

The 2026-09-14 both-ends table (*TLS on the wire*, above) is unchanged as the figure it was;
this section says which end owns how much of it. `ADR-0070` decision 5 records it.

### C-91 — the 3–8% slowdown is code, not the machine

`[measured 2026-09-18]` one A/B, same boot, `pass 16 fail 0 unknown 0`: a worktree `../fb-0905`
at **`0149b26`** — the commit that recorded the 2026-09-05 baseline lines — against today's tree
(commit `85460c1`'s code), **20 rounds alternating tree order**, the three suites `serialize`,
`density`, `validate` each round, medians of 20. Logs `target/boot-c-evidence/c91-runs.txt`,
`c91-timeline.txt`. ns/op, old → new:

| case | `0149b26` | today | diff |
|---|---|---|---|
| engine turn, 1 busy sessions | 1 660.2 | 1 817.2 | **+9.5%** |
| engine turn, ring 4096 / 512 / 64 | 1 680.8 / 1 675.5 / 1 676.1 | 1 801.0 / 1 787.9 / 1 800.1 | +7.2 / +6.7 / +7.4% |
| engine turn, 2 / 4 / 8 busy | 3 333.8 / 6 725.0 / 13 519.2 | 3 659.9 / 7 326.6 / 14 788.1 | +9.8 / +8.9 / +9.4% |
| engine turn, 16 / 32 / 64 busy | 27 447.5 / 56 907.7 / 121 587.5 | 30 026.8 / 61 886.7 / 130 743.0 | +9.4 / +8.7 / +7.5% |
| validate Heartbeat | 167.9 | 173.4 | +3.3% |
| validate NewOrderSingle | 909.7 | 956.7 | +5.2% |
| validate NewOrderSingle, w2w bytes | 927.8 | 968.5 | +4.4% |
| validate TestRequest, w2w bytes | 216.3 | 229.4 | +6.1% |
| encode ExecutionReport (template) | 245.8 | 237.4 | **−3.4%** |
| SendingTime from the cache | 4.9 | 5.8 | +18.4% |

**Verdict: code.** The old commit, built and run in today's boot on today's kernel
(`7.0.0-31`), reads its own 2026-09-05 numbers (`engine turn, 1 busy` 1 660 against the 1 657.7
line), so the kernel `-30 → -31` candidate is dead and the slowdown is in commits merged since
`0149b26`: **~7–10% on every engine-turn case** — larger than boot B's +3.0% reading of the same
cases suggested — and 3–6% on the validate cases, with `encode ExecutionReport (template)` 3.4%
*faster*, as boot B also saw. Two facts narrow the bisect (the plan's C-91b): the toolchain is
pinned at `1.98.0` throughout, and the validate cases are pure user space, so their 3–6% points
at `588b350` (`52=` read at every precision, 2026-09-09) or its neighbours, while the engine-turn
cases add a `recvfrom` per turn and point at the TLS branch's changes to `pump` and the transport
(2026-09-09 → 13). Note that this run's *new* tree carried `ListenerEveryTurns=16` in the library
— `density.rs` drives `Engine::turn` directly, so the cadence does not enter it — and the cadence
would make a turn cheaper, not dearer, in any case.

### C-91b — the bisect: one commit crosses the line, and the slope is five steps

`[measured 2026-09-18]` same boot, `pass 16 fail 0 unknown 0`; `git bisect start 85460c1
0149b26`, judge = `density`'s `engine turn, 1 busy sessions`, median of 3 runs per commit (5 on
the first), **bad above 1 743 ns** (1 660.2 × 1.05); log `target/boot-c-evidence/c91b-log.txt`.
The commits bisect visited, in history order, oldest first (each figure is the state of the tree
*at* that commit, so a docs commit's figure is the code merged before it):

| commit | date | what it is | median ns | vs `0149b26` |
|---|---|---|---|---|
| `0149b26` | 2026-09-05 | the baseline commit (C-91) | 1 660.2 | — |
| `792c2e7` | 2026-09-06 | docs (plan) — code merged 09-05 → 09-06 under it | 1 700.0 | +40 |
| `28465e8` | 2026-09-09 | merge PR #55 | 1 719.6 | +59 |
| **`588b350`** | 2026-09-09 | **`feat(session)!: read 52= at every precision`** | **1 752.2** | **+92 — first over 1 743** |
| `043a4a2` | 2026-09-09 | docs (status) | 1 740.2 | +80 |
| `627637e` | 2026-09-08 | docs (plan) | 1 742.2 | +82 |
| `906c256` | 2026-09-09 | merge PR #56 (`utc-timestamp-widths`) | 1 752.9 | +93 |
| `1c36406` | 2026-09-09 | `TlsTransport` hands its keys to the kernel | 1 753.8 | +94 |
| `f085f43` | 2026-09-12 | docs (ADR) — the TLS and settings code of 09-09 → 09-12 under it | 1 792.2 | +132 |
| `85460c1` | 2026-09-18 | today (C-91) | 1 817.2 | +157 |

`git bisect` names **`588b350`** as the first bad commit — it is where the 5% line is crossed,
and it is worth about **+33 ns** on the turn (1 719.6 → 1 752.2; `043a4a2` and `627637e` read
~10 ns under it, which is the noise band of a 3-run median). It touches
`crates/session/src/clock.rs` (`parse_utc`, 17 and 21 bytes → seven widths) and
`crates/dict/src/field_type.rs`; the candidate mechanism is the width dispatch now on the hot
path of every inbound `SendingTime` — which is also the lead the validate cases pointed at in
C-91 (+3–6% on pure user-space cases), so one commit explains both families' first step.

**But the profile is cumulative, not one step.** ~+40 ns had already arrived by `792c2e7`
(code of 09-05 → 09-06), ~+20 more by `28465e8`, ~+33 at `588b350`, ~+40 between `1c36406`
and `f085f43` (the TLS transport and settings work of 09-09 → 09-12), ~+25 from `f085f43` to
HEAD (09-12 → 09-18, which includes `ListenerEveryTurns` and the pump changes). Five steps of
2–3% each, on a case whose band is 10%: none would have tripped `bench.sh` alone, and together
they are 9.5%. **The machine question is closed; the code question is five perf tasks**, one per
segment, each with its two bisect endpoints named — STATUS item 93.

### C-49 — in situ: the kernel does not charge the application path for its payload

`[measured 2026-09-18]` `perf trace -s` over whole `hft` runs, engine thread only, both paths,
commit `85460c1`'s code, `w2w` at listener cadence N = 1 (its own default — the C-89 trap above),
`pass 16 fail 0 unknown 0`; logs `target/boot-c-evidence/c49-*.perf` and `.w2w`. **The tracer
inflates every syscall about 2×** — p50 under trace read 35.2–35.8 µs on *both* paths — so only
the *difference* between the paths is a reading, never a level.

| engine thread, per run | admin | app |
|---|---|---|
| `sendto`, 22 001 calls, total time | 186.608 ms / 186.508 ms (two runs) | 187.663 ms / 183.876 ms |
| `sendto`, average | 8.48 µs | 8.53 / 8.36 µs |
| `recvfrom`, average | 2.47 µs | 2.45 µs |

**The application path's `sendto` differs from the administrative one by ≤ 50 ns and in both
signs; `recvfrom` by 20 ns.** The kernel's own share of the larger payload is ~0 at this
instrument's resolution, which is what the +24.5 ns slope row of `DESIGN.md` §8 predicted from
the 8 → 8 192 byte lever. So the ~3 130 ns of the application round trip that lies outside one
engine turn (D_in 765.5 ns) is **not payload-proportional kernel work**.

Side reading, and the trap it found: `accept4` was called 40.6–45.7 k times per run against
40.6–45.6 k `recvfrom` — **one `accept4` per spin turn**, although the library's default is 16
since C-89. `w2w`'s `--listener-every` defaulted to 1 in its own parser and did not read
`Limits`; every flagless `w2w` run in this boot after C-89 ran at N = 1. Answered, not open; the
fix is in the next commit.

**Item 49 closes here, by decision.** Three probes have retired the named candidates with
numbers — the dictionary pass (17.4%, item 39), the payload copy (0.9%, and now in situ ~0),
`Journal::put` (8.9 ns) — and the client's timed loop is byte-for-byte the same on both paths.
The remainder is published in §8 as *unattributed, outside the engine turn and not the kernel's
payload work*. The one instrument that would split it is named and not scheduled: software
`SO_TIMESTAMPING` (TX/RX software stamps, which loopback supports) on both sockets, giving four
segments per round trip — client stack out, engine-side dwell socket-in → socket-out, engine
stack out, client stack in; `w2w --wire-timestamps` already parses the cmsg (`ts[0]`), so it is
a `--stamp software` arm of some later boot if the number is ever needed.

### What is not proven

- **A mechanism for the administrative path's +0.3 µs.** One candidate above, untested.
- **Anything on a NIC, in `standard`, or with more than one session.** Loopback, `hft`, N = 1
  session only. `standard` is outside the cadence by ADR-0069 decision 3.
- **The accept delay itself at N = 16 under load.** Consequence 2's *N × one iteration* is
  arithmetic; `connect-rtt` does not measure it.
- **The in-window `accept4` count**, derived as stated above.
- **A mechanism for both-kTLS being superadditive** (C-84): one candidate, not isolated.
- **What moved boot B's ten arms** (C-85 refuted the only candidate; nothing else was varied).
- **The mechanism behind each of the five steps** (C-91b named the segments; `588b350`'s
  width dispatch is a candidate, not a measured cause; the other four segments have none yet).
- **What the ~3 130 ns outside the engine turn is** (item 49, closed by decision; the software
  stamp probe is named above and not scheduled).
- **The busy turn at N > 1** (ADR-0025 open question 1), so the `hft` ceiling stays 4 although
  the measured wakeup puts the arithmetic crossover at N ≈ 11.

