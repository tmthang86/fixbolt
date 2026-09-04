# Prior art — FIX engines, surveyed 2026-08-27

What already exists, what it proves, and what it costs to ignore. Every claim here was read
off the project's own README or licence file on the date above, not inferred.

## Name collision — read this first

**`matthart1983/nanofix` already exists**: *"Ultra-low-latency FIX protocol engine in Rust —
28 ns serialize, 2.25M msg/s"*, MIT, 18 stars. Same name, same language, same stated purpose.

This is not a legal problem — MIT, and no trademark is claimed — but it is a real one:
`cargo add fixbolt`, a web search, or a GitHub search will surface both. The name is
already committed to at `github.com/tmthang86/fixbolt`; changing it later costs more the
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
the gap fixbolt exists to fill, and also the reason nobody has filled it cheaply.

## C++

| Project | Licence | What to take from it |
|---|---|---|
| **`quickfix/quickfix`** | QuickFIX Software License (BSD-3 shape + attribution + naming restriction) | The `spec/*.xml` dictionaries and the **59 FIX 4.4 acceptance definitions** in `test/definitions/server/fix44/`. See [ADR-0001](../decisions/ADR-0001-relationship-to-quickfix.md) |
| **`hffix`** | FreeBSD (some parts Boost) | **The codec design model.** *"Fast, efficient encoding and decoding of FIX in place, at the location of the I/O buffer"*; *"does no memory allocation on the free store"*; fields exposed as `char const* begin(), char const* end()` into the buffer. Deliberately omits session management, threading, sockets — which is exactly the right split |
| `fix8` | — | Not surveyed |

## Measured numbers worth keeping

Collected from the sources above and from the shadow-exchange performance research on the
same date. **None of these were reproduced locally.** Treat every one as a claim until a
fixbolt benchmark produces its own.

| Claim | Source | Note |
|---|---|---|
| QuickFIX 6,000–8,000 msg/s per session | QuickFIX developers mailing list | Commodity hardware, minimal application |
| QuickFIX `FileStore` calls `Sync()` per write, 3 files per message | `quickfix/quickfix` issue #38 | The dominant latency source in the default configuration |
| `matthart1983/nanofix`: 28 ns serialise, 2.25M msg/s | its README | Apple Silicon, Criterion, 1M iterations |
| exchange-core (Java, LMAX Disruptor): ~5M ops/s, single order book | project site | Decade-old hardware. Matching is not the bottleneck at any realistic FIX rate |
| Go channel 4.9–9.4M ops/s; lock-free ring buffer 12–15M ops/s | Go ring-buffer benchmarks | Relevant only as an order-of-magnitude reference for queue handoff cost |

## How other engines admit a counterparty

`[documented 2026-09-01]` Read for [ADR-0026](../decisions/ADR-0026-a-counterparty-registry-in-the-pre-session-stage.md).
**Every figure and API name here is someone else's claim** — nothing in this section was run.

| Engine | How a `Logon` reaches its configuration |
|---|---|
| **QuickFIX** | Session identity is `(BeginString, SenderCompID, TargetCompID)`, plus an optional **`SessionQualifier`** to separate otherwise-identical sessions. `SessionSettings` holds one block per session; the incoming triple is matched against them |
| **QuickFIX/J dynamic acceptors** | `DynamicAcceptorSessionProvider` — a **provider**, not a table. `AcceptorTemplate=Y` marks a block as a template rather than a registered session; `TemplateMapping` maps a sessionID *pattern* (`*` wildcards, `ANY_SESSION`) to a template and the session is materialised on demand |
| **Artio** | Identity comes from a pluggable **`SessionIdStrategy`** which *may* include **SubID and LocationID**, not only the comp-ID pair. On a `Logon` an **`AuthenticationStrategy`** runs: `authenticateAsync`, then `AuthenticationProxy.accept(…)` — **choosing the FIX dictionary at that moment** — under `authenticationTimeoutInMs`. The accepting process (`FixEngine`) then raises `SessionExistsHandler`, and a `FixLibrary` takes ownership via `requestSession(surrogateSessionId)`; until it does, the engine itself processes heartbeats, gap fills and resend requests |

**What transferred, and it is three things.**

1. **All three decide at the `Logon`, in the accepting stage** — none routes at accept time.
   [ADR-0020](../decisions/ADR-0020-a-pre-session-stage-owns-the-socket-until-logon.md) reached
   that here from a *conformance* failure (`1b_DuplicateIdentity.def`, 57/59), and the industry is
   already there for an operational reason. **Two independent roads to one shape is the strongest
   evidence this document holds about anything.**
2. **All three are a callback or a provider, not a fixed table.** A static map is the degenerate
   case. This is what makes authentication, per-counterparty policy and eventual hot reload
   possible at all, and it is why ADR-0026 chose a trait.
3. **Identity is not always `(49, 56)`** — Artio's `SessionIdStrategy` may take SubID and
   LocationID; QuickFIX has `SessionQualifier` for the same need. `presession::Identity` reads the
   comp-ID pair only, so **a counterparty disambiguating by `50=`/`57=` cannot be served today**.
   That was found by reading other engines, not by reading FIX.

**Where this design deliberately parts from Artio:** `authenticateAsync` lets an application
consult a remote service during logon. ADR-0026 decision 4 refuses that — `lookup` is synchronous,
because the acceptor thread must not block in `hft` and an accept path that awaits the network is
a denial-of-service surface no logon deadline closes.

## How other engines represent a price

`[documented 2026-09-01]` Read for [ADR-0028](../decisions/ADR-0028-a-decimal-is-a-copy-value-parsed-on-demand.md).

- **Artio** — `DecimalFloat`: the significant digits in a **value** field, the position of the
  point in a **scale** field. Two integers, `Copy`, no heap, produced on demand.
- **QuickFIX/Go** — `FIXDecimal`, an arbitrary-precision fixed-point value.
- **QuickFIX/J** — `double`, and it has been argued about for years.

**This refuted a guess made here.** `PRD.md` open decision 10 suspected that *decimal / price
types* was mislabelled as a gap — that a typed decimal would be the owned per-message object D2
forbids. Artio, whose constraints are the same as this project's, has one anyway, **because 16
bytes of `Copy` is not what D2 forbids**: what D2 forbids is the 8 224-byte `MessageView` that cost
5.9×. The gap is real and the objection was to a design nobody proposed.

## Sources

- <https://github.com/matthart1983/nanofix>
- <https://github.com/ferrumfix/ferrumfix>
- <https://github.com/fixer-rs/fixer>
- <https://github.com/joaquinbejar/IronFix>
- <https://docs.rs/quickfix>
- <https://github.com/quickfix/quickfix>
- <https://jamesdbrock.github.io/hffix/>
- <https://github.com/quickfix/quickfix/issues/38>
- <https://github.com/artiofix/artio/wiki/Session-Management>
- <https://javadoc.io/static/uk.co.real-logic/artio-core/0.121/uk/co/real_logic/artio/engine/EngineConfiguration.html>
- <https://github.com/artiofix/artio/blob/master/artio-codecs/src/main/java/uk/co/real_logic/artio/fields/DecimalFloat.java>
- <https://javadoc.io/static/org.quickfixj/quickfixj-core/2.3.0/quickfix/mina/acceptor/DynamicAcceptorSessionProvider.html>
- <https://www.quickfixj.org/usermanual/2.3.0/usage/acceptor_dynamic.html>
- <https://quickfixengine.org/c/documentation/getting-started/configuration.html>
- <https://github.com/quickfixgo/quickfix/blob/main/fix_decimal.go>
- <https://www.onixs.biz/insights/understanding-fix-drop-copy.html> — drop copy as an audit topology (ADR-0027)

## Re-read 2026-09-03 — the field, seven days later

`[documented 2026-09-03]` **Every line here is somebody else's claim, read off a README, a
docs page or a vendor page by a research pass; nothing was run.** Kept because two of the
findings changed what this repository plans next (`STATUS.md` item 45).

**Rust.** No pure-Rust acceptor with a production track record has appeared. `easyfix`
(`ldanko/easyfix`, v0.14.10, pushed 2026-08-26) is the most recently active project and ships an
`examples/acceptor`, self-labelled work in progress. `ForgeFIX` is initiator-only, FIX 4.2, Tokio.
`hotfix` v0.12.1 (2026-05) is initiator-only, sync, rustls TLS, no latency claim beyond "on par
with QuickFIX". `quickfix-rs` (`arthurlm`, v0.2.1, 2026-02) remains the only acceptor-capable
crate, by being the C++ engine behind an FFI. `matthart1983/nanofix` was created 2026-03-21, 21
stars, 16 commits, all figures self-reported on Apple Silicon.

**Numbers the field publishes, so this repository's own can be read against them.** Chronicle
FIX: under 4 µs p99.9 for a `NewOrderSingle` round trip, JVM, no bypass named. OnixS C++: 0.99–1.29 µs
send on 144-byte messages. B2BITS FIX Antenna C++: p50 5.7 µs tick-to-trade. CoralFIX: ~4.8 µs
one-way over loopback. Rapid Addition: under 13 µs p99.9 wire-to-wire. None of these headline
numbers is attributed to Onload or `ef_vi` on the vendor's own page, and none states its
mitigations, isolation or IRQ layout the way `DESIGN.md` §9 requires — which is why they are
listed here and quoted nowhere else in this repository.

**What a QuickFIX-family engine carries that this one does not, as of this date** — read off
QuickFIX/J's configuration reference: `NextExpectedMsgSeqNum(789)` and
`LastMsgSeqNumProcessed(369)`; `ResetOnLogon`/`ResetOnLogout`/`ResetOnDisconnect`;
`LogonTimeout`/`LogoutTimeout`; `TimestampPrecision` beyond milliseconds;
`ValidateFieldsOutOfOrder`, `ValidateUserDefinedFields`;
initiator connection settings in the file; a message log. Each is placed in `STATUS.md` item 45's
waves, or named there as deliberately declined (`SendRedundantResendRequests`, `RefreshOnLogon`,
a database store).

**`[corrected 2026-09-04]` `MaxMessageSize` was on that list and did not belong on it.** It is
not a configuration key in any engine surveyed. `SessionSettings.h` carries **113 config keys**
and this is not one; `MaxMessageSize` is `FixFieldNumbers.h:61` — **tag 383, an optional field
in the Logon message** (`spec/FIX44.xml:284`), by which the two ends *tell each other* their
limit. QuickFIX/J's configuration reference has no such setting either. The error came from
reading a tag name as a settings name, and it survived because nothing cross-checks a prior-art
claim against the source sitting in `vendor/`.

**How the four engines bound an inbound message, since the question is real even if the key is not:**

| Engine | Read buffer | Ceiling | Set where | Over-long |
|---|---|---|---|---|
| QuickFIX C++ | `std::string`, appended to, grows on the heap | **none** | — | waits forever; the buffer grows on every read. `Parser::readFixMessage` takes an `int length` and checks only `< 0`, so `9=2000000000` is a denial-of-service surface |
| QuickFIX/J | no message-size setting | **none** | — | — |
| **Artio** | fixed `ByteBuffer`, **16 KiB** default | 16 KiB | `receiverBufferSize(int)` at engine construction, or `fix.core.receiver_buffer_size` | records the message and **disconnects**, naming the cause: *"Unable to frame message, receiver buffer too small"* |
| **fixbolt** | `[u8; RX]`, **4 KiB** | 4 KiB | a type parameter, compile time | `Cut::Garbage`; the session decides, except pre-session, which closes **silently** |

**Two things to take from it.** Artio is the closest engine in philosophy and it decides only
once the buffer is genuinely full (`offset == 0 && byteBuffer.remaining() == 0`); fixbolt decides
from `9=` alone and so is both earlier and more honest. But Artio *names the reason* and fixbolt
does not — `presession.rs` answers `Step::Gone`, closing the socket with no reason and no event,
which is the failure `conn.rs:348` already argues against for `DuplicateIdentity`.

**And Artio's default is 16 KiB against fixbolt's 4 KiB.** That is a data point, not evidence:
Artio allocates on the heap, so its ceiling is cheaper. `RX = 4096` here rests on no measurement
— the acceptance corpus never exceeds 200 bytes, so it cannot speak to the question.

**Testing oracles.** No public FIX fuzzing corpus and no public FIX capture set was found;
QuickFIX/J's acceptance suite spans more versions and both roles by directory structure, its
exact file count unconfirmed. The 59-file gate stays the only free oracle for FIX 4.4 acceptors.

**Kernel TCP knobs this engine has not tried**, from the same pass and unmeasured here:
`SO_BUSY_POLL` / `SO_PREFER_BUSY_POLL` / `SO_INCOMING_NAPI_ID` (only meaningful with a real NIC —
loopback has no NAPI), and `io_uring` with `IORING_REGISTER_NAPI` as a `standard`-mode poller.
Both are wave C of item 45, after the NIC-to-NIC number exists to compare against.
