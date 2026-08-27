# Prior art — FIX engines, surveyed 2026-08-27

What already exists, what it proves, and what it costs to ignore. Every claim here was read
off the project's own README or licence file on the date above, not inferred.

## Name collision — read this first

**`matthart1983/nanofix` already exists**: *"Ultra-low-latency FIX protocol engine in Rust —
28 ns serialize, 2.25M msg/s"*, MIT, 18 stars. Same name, same language, same stated purpose.

This is not a legal problem — MIT, and no trademark is claimed — but it is a real one:
`cargo add nanofixengine`, a web search, or a GitHub search will surface both. The name is
already committed to at `github.com/tmthang86/nanofixengine`; changing it later costs more the
longer it waits. Decide deliberately rather than by default.

## Rust

| Project | Licence | Acceptor | Maturity, in its own words |
|---|---|---|---|
| **`matthart1983/nanofix`** | MIT | Yes — `FixServer`, thread-per-connection, CompID whitelisting | 18★. FIXT 1.1 / FIX 5.0 SP2 session state machine, zero-alloc pools, SIMD SOH scan, 238 tests. Claims 28 ns heartbeat serialise, 59 ns `NewOrderSingle`, 2.25M msg/s, TCP RTT p50 15.6 µs / p99.9 86.5 µs — **all measured on Apple Silicon with Criterion, no production deployment shown** |
| **`ferrumfix` (`fefix`)** | MIT/Apache | Session layer present | 450★. README: *"currently under heavy development and wildly unstable, so all interested parties should refrain from using it in production prior to its 1.0 release"* |
| **`fixer-rs`** | — | Yes — echo acceptor example | 10★, 236 commits. README: *"still under heavy development"* |
| **`IronFix`** | MIT | **No** — *"ironfix-engine has Initiator only"* | 11★. Resend store non-functional, derive macros expand to `todo!()`. README: *"treat any figure as unmeasured until you have produced it yourself"* |
| **`quickfix` crate** | — | Yes — `Acceptor` | Unofficial FFI binding to C++ libquickfix. Needs CMake + a C++17 compiler. *"API MAY CHANGE IN FUTURE VERSION"* |
| **`hotfix`** | — | **No** — initiator / buy-side only | — |

**Reading:** as of this date there is no production-proven, pure-Rust FIX acceptor. That is
the gap nanofixengine exists to fill, and also the reason nobody has filled it cheaply.

## C++

| Project | Licence | What to take from it |
|---|---|---|
| **`quickfix/quickfix`** | QuickFIX Software License (BSD-3 shape + attribution + naming restriction) | The `spec/*.xml` dictionaries and the **59 FIX 4.4 acceptance definitions** in `test/definitions/server/fix44/`. See [ADR-0001](../decisions/ADR-0001-relationship-to-quickfix.md) |
| **`hffix`** | FreeBSD (some parts Boost) | **The codec design model.** *"Fast, efficient encoding and decoding of FIX in place, at the location of the I/O buffer"*; *"does no memory allocation on the free store"*; fields exposed as `char const* begin(), char const* end()` into the buffer. Deliberately omits session management, threading, sockets — which is exactly the right split |
| `fix8` | — | Not surveyed |

## Measured numbers worth keeping

Collected from the sources above and from the shadow-exchange performance research on the
same date. **None of these were reproduced locally.** Treat every one as a claim until a
nanofixengine benchmark produces its own.

| Claim | Source | Note |
|---|---|---|
| QuickFIX 6,000–8,000 msg/s per session | QuickFIX developers mailing list | Commodity hardware, minimal application |
| QuickFIX `FileStore` calls `Sync()` per write, 3 files per message | `quickfix/quickfix` issue #38 | The dominant latency source in the default configuration |
| `matthart1983/nanofix`: 28 ns serialise, 2.25M msg/s | its README | Apple Silicon, Criterion, 1M iterations |
| exchange-core (Java, LMAX Disruptor): ~5M ops/s, single order book | project site | Decade-old hardware. Matching is not the bottleneck at any realistic FIX rate |
| Go channel 4.9–9.4M ops/s; lock-free ring buffer 12–15M ops/s | Go ring-buffer benchmarks | Relevant only as an order-of-magnitude reference for queue handoff cost |

## Sources

- <https://github.com/matthart1983/nanofix>
- <https://github.com/ferrumfix/ferrumfix>
- <https://github.com/fixer-rs/fixer>
- <https://github.com/joaquinbejar/IronFix>
- <https://docs.rs/quickfix>
- <https://github.com/quickfix/quickfix>
- <https://jamesdbrock.github.io/hffix/>
- <https://github.com/quickfix/quickfix/issues/38>
