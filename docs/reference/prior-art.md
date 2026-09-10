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

## `[researched 2026-09-10]` Five engines, and the two questions a local refusal asks

Read for the fix to `STATUS.md` item 63, found while building the `tls` plan's step 4b: a
connection this engine refused for a **local policy** reason (`TlsRequireKernel=Y`) reported
`Ended(SendingTimeOutOfRange)` — a protocol accusation, with `last_skew_ms` reading about
**2026 years**, sending an operator to check NTP on the counterparty's host for a fault that was
entirely local.

**Every engine's own source was read** — QuickFIX C++ from `vendor/quickfix-src` at the SHA
`scripts/fetch-quickfix-assets.sh` pins, the rest from their repositories. Nothing here was run.

### Question 1 — is `SendingTime` judged against a live clock, or against a stored "now"?

| Engine | What the check reads |
|---|---|
| **QuickFIX C++** | `labs(m_timestamper() - sendingTime) <= m_maxLatency` — `Session.h:242-247`. **Live**, called inside the check |
| **QuickFIX/J** | `Math.abs(SystemTime.currentTimeMillis() - sendingTime…) / 1000 <= maxLatency`. **Live** |
| **quickfix-go** | `if delta := time.Since(sendingTime); delta <= -1*s.MaxLatency \|\| delta >= s.MaxLatency`. **Live** — `time.Since` reads `time.Now()` at the call |
| **Artio** | Has `INVALID_SENDING_TIME` as a disconnect reason; the check is in the session logic, not driven by a stored tick |
| **nanofix** | **Does not check `SendingTime` at all** |
| **fixbolt** | `self.now_ms`, written **only** by `tick_inner` (`crates/session/src/lib.rs:2092`), initialised to `0` (`:1414`). `received_with` takes **no time argument** |

**Four of four that check it read a live clock. fixbolt is the only one that depends on a prior
tick having run** — and that is not a style difference, it is the whole defect: a session whose
tick was skipped compares a real timestamp against zero and produces a skew of two thousand years.

**This is a consequence of D1 rather than an oversight**, and that is worth saying plainly. The
session layer is pure and time arrives as `Input::Tick`, so it *cannot* call a clock inside the
check the way all four of these do. The other engines are not more careful here; they are
impure, and impurity happens to make this class of bug unreachable. **The purity is still worth
its price** — it is what makes the 59 acceptance definitions runnable with no socket and no
clock — but it moves the obligation: **every path that judges a message must be sure a tick
preceded it**, and nothing in this repository checks that today.

### Question 2 — does a locally-refused connection still judge the counterparty's bytes?

| Engine | What happens to the first message when the acceptor refuses |
|---|---|
| **QuickFIX C++** | `if (!m_pSession) { server.getMonitor().drop(m_socket); return false; }` — `SocketConnection.cpp:174-177`. The socket is dropped and **no session ever judges the bytes**. `m_pSession->next(message, …)` runs only when a session was found |
| **quickfix-go** | `session, ok := a.sessions[sessID]; if !ok { … return }`, and the deferred `netConn.Close()` runs. **Closed before `session.connect()`** — the bytes are logged, never judged |
| **QuickFIX/J** | `disconnect(String reason, boolean logError)` — the refusal is a disconnect with a free-text reason |
| **Artio** | Authentication runs at the `Logon`; a refusal is `FAILED_AUTHENTICATION` or `AUTHENTICATION_TIMEOUT` |
| **fixbolt today** | **Judges them anyway.** `Connection::turn`'s `if !self.closing` block covers only the tick/send half; the socket read and the "judged in order" loop that calls `received_with` sit **after** it (`crates/engine/src/conn.rs:363-433`) and run whatever `closing` says |

**And QuickFIX C++ contradicts itself on this, in the same function.** The unknown-session path
drops without judging — but the `AllowedRemoteAddresses` check, a pure local policy, runs
**after** `m_pSession->next(message, …)` has already processed the `Logon`
(`SocketConnection.cpp:178-186`). So that one deny-list refusal *does* let the session act first.
One file, two orders, no comment about the difference. **Do not read the family as having a
settled rule here** — read it as: the case everybody got right is the one where no session
exists yet.

### Question 3 — is there a type that names *why*, and does it separate local from protocol?

| Engine | The type |
|---|---|
| **QuickFIX C++** | **None.** `void Session::disconnect()` takes no argument at all (`Session.cpp:621`); reasons live in `onEvent` log strings |
| **QuickFIX/J** | **A string.** `disconnect(String reason, boolean logError)` |
| **quickfix-go** | `MessageRejectError` for *protocol* rejects, split only by `IsBusinessReject()`. **No disconnect-reason type** |
| **nanofix** | **None.** `SessionAction::SendLogout { text: Option<String> }` and nothing else |
| **Artio** | **`DisconnectReason`, 26 variants**, in the SBE schema |

**Artio is the only one with a structured enum, and it is the direct precedent — because it
mixes both kinds in one flat type.** Local-side: `APPLICATION_DISCONNECT`, `LIBRARY_DISCONNECT`,
`ENGINE_SHUTDOWN`, `SLOW_CONSUMER`, `DUPLICATE_SESSION`, `FAILED_AUTHENTICATION`,
`AUTHENTICATION_TIMEOUT`, `ADMIN_API_DISCONNECT`, `REPLAY_BACK_PRESSURE_DISCONNECT`,
`INVALID_CONFIGURATION_NOT_LOGGING_MESSAGES`. Counterparty-side: `INCORRECT_BEGIN_STRING`,
`FIRST_MESSAGE_NOT_LOGON`, `MSG_SEQ_NO_TOO_LOW`, `INVALID_SENDING_TIME`,
`NEGATIVE_HEARTBEAT_INTERVAL`, `MISSING_LOGON_COMP_ID`, `INVALID_FIX_MESSAGE`.

**The separation is in the prose, not in the type.** Every local variant's description begins
*"We disconnected…"*; `REMOTE_DISCONNECT` reads *"The TCP connection was disconnected
remotely"*. Agency is carried by a sentence a reader has to read.

**`INVALID_CONFIGURATION_NOT_LOGGING_MESSAGES` is the closest analogue to `TlsRequireKernel`**
that exists anywhere in the family: a connection ended because *this deployment is configured in
a way that cannot deliver what it promised*. Artio put it in the same enum as the protocol
faults and did not flinch.

### What this settles, and what it does not

**Settled:** adding a local-policy variant to `DropReason` is not a divergence — it is what the
only engine with such a type already does, ten times over. fixbolt's own `disconnect_with`
already established the same pattern on 2026-09-02 for the single-logon case, before any of this
was read.

**Settled:** a refused connection should not judge the counterparty's bytes. Two engines drop
the socket without ever handing them to a session, and neither has anything like fixbolt's
problem as a result.

**Not settled, and deliberately left open:** whether local and protocol reasons should be *two
types* rather than one enum. Nobody in the family has tried it, so there is no evidence either
way — only Artio's choice to keep one flat enum and carry the distinction in documentation, which
is exactly the kind of prose-held constraint `CLAUDE.md` §4 says does not hold.

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
`LastMsgSeqNumProcessed(369)`; `TimestampPrecision` beyond milliseconds. Each is placed in
`STATUS.md` item 45's waves, or named there as deliberately declined
(`SendRedundantResendRequests`, `RefreshOnLogon`, a database store).

**`[corrected 2026-09-06]` "the QuickFIX family" is not one engine, and this line was quoting the
wrong member of it.** The two fields above were taken from QuickFIX/**J**'s reference, but the
engine `scripts/interop.sh` links against is the **C++** one, and a survey of five engines found
they disagree about both the key names and the features:

| | `789` read | key to send `789` | `369` sent | key for `369` |
|---|---|---|---|---|
| QuickFIX **C++** | yes | **`SendNextExpectedMsgSeqNum`** | **never** | **none exists** |
| QuickFIX/**J** | yes | `EnableNextExpectedMsgSeqNum` | every message | `EnableLastMsgSeqNumProcessed` |
| QuickFIX/**n** | **no** | — | every message | `EnableLastMsgSeqNumProcessed` |
| quickfix**go** | yes | `EnableNextExpectedMsgSeqNum` | every message | `EnableLastMsgSeqNumProcessed` |
| `nanofix` | no | — | no | — |

So `369` is **not** a gap against QuickFIX C++ — that engine does not have the feature either —
and writing QuickFIX/J's key name into a C++ configuration file would be ignored in silence.

**`[built 2026-09-06]` Both are implemented here now**, and the list above is the shape they
were built to: `789` read unconditionally and sent behind `SendNextExpectedMsgSeqNum`, `369`
behind `EnableLastMsgSeqNumProcessed`. `789` is confirmed in both directions against a real
`libquickfix`; `369` cannot be, for the reason the table gives. What is left of the original
line — *what a QuickFIX-family engine carries that this one does not* — is
`TimestampPrecision` beyond milliseconds, which is wave B's third plan.
The survey, and what it means for an engine whose application writes its own bytes, is
[who-owns-the-outbound-header](who-owns-the-outbound-header.md). **A family name on a feature
list is a claim about whichever member you happened to read.**

**`[researched 2026-09-09]` `TimestampPrecision` in QuickFIX C++, read from source at the pinned
SHA, because the sentence above had already been paid for once and was about to be repaid.**
`quickfix/quickfix` at `386ce46e` — the commit `scripts/fetch-quickfix-assets.sh` pins — read
from raw source, nothing fetched into `vendor/` and nothing committed:

| | QuickFIX **C++** at `386ce46e` |
|---|---|
| Key | `TimestampPrecision`, read with `getInt` — **an integer 0–9**, not a named value; out of range throws (`SessionSettings.h:133`, `Session.h:167–174`, `SessionFactory.cpp:219–221`) |
| Default | **3** (`Session.cpp:68`) |
| Legacy spelling | `MillisecondsInTimeStamp`, a bool mapping to 3 or 0 (`Session.h:159–166`) |
| On FIX **4.4** | sub-second **supported** — `beginString >= FIX.4.2` (`Session.h:175–184`) |
| Parser accepts | **any length 17–27**: `.` at index 17, then 0–9 digits (`FieldConvertors.h:492–596`) |
| Serialiser writes | `17 + 1 + precision` bytes (`FieldConvertors.h:465–489`) |

Two consequences, recorded where the next reader will look for them.
**`scripts/interop.sh` can be an oracle for this feature in both directions** — unlike `369`,
the C++ end can be told `TimestampPrecision=6` and will both send and accept a 24-byte `52=` on
a FIX.4.4 session. And **the value is an int, so `SECONDS|MILLIS|MICROS|NANOS` is QuickFIX/J's
spelling, not this oracle's**: a `.cfg` shared between the two ends has to carry a number.
`[2026-09-09]` that enum had already been written into
[timestamp-micros](../plans/2026-09-04-timestamp-micros.md)'s *what is known for certain* table
**citing this page**, which has never contained it — and the row survived a re-verification two
days after the correction above was written.
[ADR-0057](../decisions/ADR-0057-sub-millisecond-time-arrives-beside-the-tick.md) carries the
decision; the receive-side asymmetry it also found — this engine accepts 17/21/24/27 where the
oracle accepts 17–27 — is `STATUS.md` open item 59.

### `[researched 2026-09-09]` Five engines read the same field five different ways

Item 59 asked what the strict set should be, and the answer was researched by reading each
engine's parser rather than its documentation. **The QuickFIX family splits two against two on
this exact question**, which is the sharpest instance yet of the rule two paragraphs up.

| Engine | Widths its parser accepts | A bare `.` with no digits | Read at |
|---|---|---|---|
| QuickFIX **C++** | **any length 17–27** | **accepted**, as `fraction = 0` | `386ce46e:src/C++/FieldConvertors.h` |
| QuickFIX/**J** | **exactly 17, 21, 24, 27, 30** | rejected | `quickfix-j/quickfixj`, `UtcTimestampConverter.java:42–46, 182` |
| **quickfix-go** | **exactly 17, 21, 24, 27** | rejected | `quickfixgo/quickfix`, `fix_utc_timestamp.go`, `switch len(bytes)` |
| QuickFIX/**n** (.NET) | **≥ 17, any number of fractional digits**, and a trailing timezone offset | rejected (a zero-length fraction throws) | `connamara/quickfixn`, `DateTimeConverter.cs:40–120` |
| **nanofix** (the reference project) | **none — it never reads one** | — | `matthart1983/nanofix`: `SENDING_TIME: u32 = 52` is declared in `src/tags.rs:10` and used in no session, parser or message code; there is no skew check |

Three things follow, and none of them was guessable from a feature list.

**1. "Match the oracle" has no single answer here.** Two members enumerate the sanctioned widths
and two accept a range. An engine that copies whichever one it read first will disagree with the
other half of the same family.

**2. But the asymmetry is not symmetric, and that is what decides it.** *Accepting* a width can
never break interoperability with a strict engine, because a strict engine never **sends** one.
*Refusing* a width does break it — and here it breaks it in the worst available way, a hang-up
before `Logon` with no byte sent. So liberality on the receive side is free and strictness is not.

**3. There is a seventh refused width, and item 59 counted six.** QuickFIX/J accepts **30 bytes**
— twelve fractional digits, picoseconds — and the FIX Technical Addendum on time precision
extends the timestamp type to picoseconds, on top of the EP wording that enumerates 3, 6 and 9.
This engine refuses 30 exactly as silently as it refuses 19. A survey run to settle a question
about six widths found a seventh that nothing in this repository had named.

**The testing shape, told without FIX**: the question was *"what do other implementations
accept?"*, and it was answered five times by reading five parsers. Four of the five answers
disagreed with each other, one implementation turned out **not to implement the feature at all**
while declaring the constant for it, and the survey's real payload was a case **outside the range
the question was asked about**. A survey scoped to the question you already have can only confirm
or deny that question; the finding that matters is often the row you did not think to ask for.

`[to testing-skills]`

**The testing shape, told without FIX**: a verification pass re-checked every line number and
every code fact in a table, corrected the ones that had drifted, and **passed a row whose
citation was the wrong document** — the claim was specific, formatted like every other row,
and pointed at a page that had never contained it. Re-verifying *facts* does not re-verify
*provenance*, and a citation is the kind of claim that looks strongest exactly when nobody
opens it.

`[to testing-skills]`

**`[2026-09-05]` Six of that list closed in one day**, with wave B's first plan:
`ResetOnLogon`/`ResetOnLogout`/`ResetOnDisconnect`, `LogonTimeout`/`LogoutTimeout`,
`ValidateUserDefinedFields`, `AllowUnknownMsgFields` and initiator connection settings in the
file are all recognised keys now — twenty-three of them, against QuickFIX C++'s 113. A message
log closed on 2026-09-04.

**`ValidateFieldsOutOfOrder` came off the list rather than being built, and the reason is
architectural.** QuickFIX switches off a separate ordering pass; this engine reads
header-versus-body order out of a flat tag index with one comparison inside the scan that
checks everything else, so there is no pass to skip. `docs/CONFIGURATION.md` says so where an
operator looks, and writing the key gets *"unknown key"* with its line. **A missing feature and
a feature that cannot exist in this shape are different facts**, and this list had been holding
them as one.

**`[corrected 2026-09-04]` `MaxMessageSize` was on that list and did not belong on it.** It is
not a configuration key in any engine surveyed. `SessionSettings.h` carries **113 config keys**
and this is not one; `MaxMessageSize` is `FixFieldNumbers.h:61` — **tag 383, an optional field
in the Logon message** (`spec/FIX44.xml:284`), by which the two ends *tell each other* their
limit. QuickFIX/J's configuration reference has no such setting either. The error came from
reading a tag name as a settings name, and it survived because nothing cross-checks a prior-art
claim against the source sitting in `vendor/`. `[2026-09-05]` the survey below, and the decision
not to invent the key, are
[ADR-0055](../decisions/ADR-0055-max-message-size-is-not-a-key-and-rx-is-the-answer.md); its
`113` was re-run from the checked-out tree rather than carried forward.

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

## `[researched 2026-09-09]` How five engines hand session events to an operator

The second survey of the day, and it settled a **design** question rather than a protocol one:
`STATUS.md` item 60 asked whether fixbolt's event stream losing events was a defect or a trade.
Each row read from that engine's source.

| Engine | Mechanism | Loses events? | Blocks the session thread? | Says *why* it ended? |
|---|---|---|---|---|
| QuickFIX **C++** | synchronous virtual callback — `Application::onLogout(const SessionID&)` | never | **yes**, for as long as the handler takes; `SynchronizedApplication` adds one global mutex across all sessions | **no** — no reason is carried |
| QuickFIX/**J** | `SessionStateListener`, multicast through a reflection `Proxy`, in a loop on the calling thread | never | **yes** | **no**, though it has nine kinds including `onMissedHeartBeat` and `onHeartBeatTimeout` |
| **quickfix-go** | synchronous interface callback | never | **yes** | **no** |
| **nanofix** | atomic counters and gauges (`src/metrics.rs`) | never — they are counts | no | **no**; aggregate, not per connection |
| **fixbolt** | bounded ring, non-blocking push, losses counted | **yes** | no | **yes** — `DropReason`, eighteen sites |

**Two things follow, and the second is what changed the code.**

**1. The trade is forced.** Every engine that never loses an event achieves it by running the
operator's callback on the session thread. A slow handler stalls the session — which for three of
the four is the documented behaviour, not a bug. fixbolt is the only one of the five that both
keeps its engine thread free *and* says why a connection ended, and losing on overflow is what it
pays.

**2. But most of the loss was not paying for anything.** The stream also lost events when a reader
happened to be polling, because producer and consumer shared one mutex — nobody had decided that;
it was how the ring got built.
[ADR-0059](../decisions/ADR-0059-an-event-is-lost-only-when-the-ring-is-full.md) removed it, and
the remaining loss now means exactly one thing.

**The testing shape, told without FIX**: the question was *"is this behaviour a defect or a
deliberate trade?"*, and it could not be answered from inside the project — the code compiles
either way and the tests pass either way. Reading four other implementations answered it in an
afternoon, and split the behaviour in two: **one half was a real trade forced by a constraint the
others do not have, and the other half was an implementation detail wearing the trade's clothes.**
Without the comparison, both halves would have been defended together — or abandoned together.

`[to testing-skills]`
