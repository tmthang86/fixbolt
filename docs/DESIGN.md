# fixbolt — Design

How the engine is built, and the latency budget it is built against. What it must do and in
which phase is [PRD.md](PRD.md); how to embed it is [GUIDE.md](GUIDE.md); the reasons behind
each decision, with their costs, are the ADRs in [decisions/](decisions/); the measurements
that justified them are in [reference/measured-costs.md](reference/measured-costs.md).

**What it is.** A FIX 4.4 engine in Rust, **bidirectional**: acceptor and initiator on one
session core, chosen by a type parameter
([ADR-0004](decisions/ADR-0004-bidirectional-engine.md)), built so that latency is a property
the design guarantees rather than one it hopes for.

**Positioning.** A FIX 4.4 acceptor on kernel TCP whose latency is a published, reproduced
number; a kernel-bypass figure appears only as a second, labelled row beside a kernel-TCP figure
from the same boot, naming its stack ([ADR-0099](decisions/ADR-0099-kernel-tcp-stays-the-default-and-the-headline-and-a-bypass-figure-is-a-second-labelled-row.md), which supersedes
[ADR-0077](decisions/ADR-0077-acceptor-first-stays-and-fastest-is-said-only-beside-a-reproduced-pair.md)
decision 2). The acceptor is
the headline because that is where the gap is: as of 2026-08-27 the Rust ecosystem has no
production-proven FIX acceptor and already has two initiators
([reference/prior-art.md](reference/prior-art.md)). The initiator ships in the same phase,
held to the same gates.

**What it is not.** Not an HFT client, not kernel bypass. FIX over the kernel stack has a
floor of roughly **10–20 µs** wire-to-wire that no codec can move (§8). The job is to make
everything above that floor disappear, and to measure the floor honestly.

**The shape it assumes.** One session on one isolated polling thread
([ADR-0012](decisions/ADR-0012-latency-first-and-one-session-per-polling-thread.md)). Inside
`hft`, latency beats session density, and reversing that needs its own ADR. Many sessions on
one thread is supported and named **`density`**; it costs `[measured 2026-08-31]`
**`N × 449 ns`** of polling per turn on a core set up to §9 and does not inherit the latency
figures on this page. Every figure here names its `N`.

**Sections.** §1 the finding the architecture is built around · §2 layers · §3 crates ·
§4 decisions D1–D16 · §5 non-goals · §6 gates · §7 build order · §8 latency budget · §9 the
OS checklist.

---

## 1. The finding this architecture is built around

A mature C++ FIX engine (fix8, 68% faster than QuickFIX) encodes a NewOrderSingle in
**2.1 µs** on production hardware, and says that **1.4 µs** of that remains with the framework
stripped out. A Rust flyweight parser on an Apple M5 parsed the same message shape in
**139 ns** on 2026-08-27.

The gap is not the bytes. It is the framework: object models, dictionary lookups at runtime,
virtual dispatch, mandatory validation. `hffix` confirms it from the other side: it deletes
the framework entirely (parse in place, no session layer) and is the fastest thing in the
survey.

Three principles follow, each learned in order:

1. **Keep the framework off the hot path.** Every layer below is shaped by it.
2. **The codec is about 1% of the wire-to-wire budget on kernel TCP.** A design that
   optimises the codec and says nothing about I/O strategy, outbound encoding, or the OS
   underneath has optimised the wrong 1%. D8–D10, §8 and §9 exist because of that.
3. **Price the I/O strategy too.** `[measured 2026-08-31]` the strategy D8 chose, one
   non-blocking `read` per connection per turn, costs a whole `Engine::turn` of **449 ns per
   session**, of which about **420 ns is the syscall** and about 30 ns is everything the
   engine itself does. This document's own parse is 122.6 ns on the same machine, so the
   syscall that discovers there is nothing to parse costs 3.8× the parse. The codec had been
   priced and the I/O strategy had not. [ADR-0012](decisions/ADR-0012-latency-first-and-one-session-per-polling-thread.md)
   is the consequence: latency wins over session density, the budget is stated end to end
   including syscalls, and every figure names its `N`.

## 2. Layers

```
┌─────────────────────────────────────────────────────────────┐
│  Application — implements Handler (library) or Application  │
└──────────────────────────▲──────────────────────────────────┘
                           │
     ┌─────────────────────┴──────────────────────┐
     │  InlineDispatch — same thread as the       │  ← the default (D4)
     │  session machine, zero hops, the borrowed  │
     │  MessageView handed straight through       │
     │                  ── or ──                  │
     │  RingDispatch — SPSC ring, application on  │  ← the option: an application
     │  its own thread                            │    that may block cannot stall
     └─────────────────────┬──────────────────────┘    the session layer
┌──────────────────────────┴──────────────────────────────────┐
│ L4  library    the application-facing API, package fixbolt  │
├─────────────────────────────────────────────────────────────┤
│ L3  engine     TCP accept and connect (ADR-0004), drives    │
│                the session machines, owns the journal       │
├─────────────────────────────────────────────────────────────┤
│ L2  session    FIX session protocol as a PURE state machine │
│                — no sockets, no clock, no I/O.              │
│                Role { Acceptor, Initiator } is a parameter  │
├─────────────────────────────────────────────────────────────┤
│ L1  codec      parse / serialise in place, zero allocation  │
├─────────────────────────────────────────────────────────────┤
│ L0  transport  trait. TCP is the default; TLS is a second   │
│                implementation behind a feature flag (D11)   │
└─────────────────────────────────────────────────────────────┘
```

## 3. Crates

Added one at a time, each behind an approved plan. All of them exist. Six are publish-shaped,
released as git tags, not uploaded — `codec`, `sbe`, `dict`, `session`, `engine` and `library`
(package `fixbolt`), in lockstep at one version; `sbe-gen`, `conformance` and every `tools/*`
crate are `publish = false`, and so are `metrics` and `store-sqlite` — each in the lockstep mould,
outside the release family until its own kill line is passed
([ADR-0170](decisions/ADR-0170-the-metrics-exporter-holds-an-observer-and-nothing-else-allocates-nothing-per-scrape-and-publishes-only-after-its-kill-line.md)
decision 10,
[ADR-0182](decisions/ADR-0182-the-sqlite-store-is-born-release-shaped-behind-a-default-feature-and-joins-the-tagged-release-family-only-when-its-kill-line-passes.md)
decision 3)
([ADR-0160](decisions/ADR-0160-six-crates-release-in-lockstep-and-the-packaged-sources-are-the-stranger-before-crates-io-is.md),
[ADR-0161](decisions/ADR-0161-0-1-0-is-a-git-tag-not-a-crates-io-upload-and-the-stranger-and-the-semver-gate-read-the-tag.md)).

| Crate | Layer | Owns | Depends on |
|---|---|---|---|
| `codec` | L1 | Parse and serialise in place. The hot path. `no_std`-compatible is the goal; zero dependencies is the rule. `encoding`: the `Encoding` trait and `TagValue<D, N>`, its tag=value implementation, which forwards unchanged to `parse_into`, `MessageView` and `Template` (D16). `decimal`: `Decimal` (16 bytes, `Copy`, mantissa × 10^exponent) and the free function `as_decimal`, read from a field's bytes on demand exactly as `as_i64` is (ADR-0120) | — |
| `sbe` | L1 | SBE 1.0 decode and encode over `&'static` tables a schema's `Schema` impl supplies: header, `SbeView` (24 bytes, `Copy`), group cursor, `varData`. `#![no_std]`, `#![forbid(unsafe_code)]`. Behind the default feature `encoding`: `Sbe<S>: codec::Encoding`, and the writer (`MessageWriter`/`GroupWriter`/`EntryWriter`) that lets `encode` fill a root block and groups/`varData` fill themselves (D16) | `codec`, only under the default feature `encoding`; none with it off |
| `dict` | build | Code generation from the FIX XML: tag constants, message shapes, required-field tables, **field ordering**, group delimiters and members, and the validation tables (defined tags, message types, per-message tag sets, field types, enum values). The three QuickFIX XML files it generates from ship **inside the crate**, at `spec/`, byte-identical to a pinned commit, under `NOTICE` — the sole exception to "no QuickFIX source is copied" (ADR-0104), made so this crate, and everything built on it, needs nothing but crates.io. `tables`: `Tables`, the nine functions the session calls, implemented for `Fix44`; the alias `Fix44TagValue` (D16). `[2026-09-19]` behind the off-by-default `fix50sp2` feature it emits a **second table**, `Fixt11Fix50Sp2Tables`, built from two XML files at once — transport from `FIXT11.xml`, application from `FIX50SP2.xml` (ADR-0080). The eighth function, `is_defined_tag_for`, exists because an admin message's body is checked against the transport file alone (ADR-0084). `[2026-09-20]` the ninth, `is_admin(msg_type)`, is generated from each `<message>`'s `msgcat`, no default method, and answers ADR-0080 decision 3's `1128` exemption rather than a hand-written list beside the call site (ADR-0086 decision 2) | `codec`; it implements `codec::Dictionary` |
| `sbe-gen` | build | `generate(xml)` / `generate_with_includes(xml, resolve)`: an SBE 1.0 schema to the `&'static` tables `sbe` reads, plus a unit struct implementing `sbe::Schema`. A construct outside ADR-0081 decision 5's scope is `Error::Unsupported(name)`, never a silently wrong table | `roxmltree`; dev-depends on `fixbolt-sbe` to compile the tables its own tests read |
| `session` | L2 | The FIX session state machine. Pure, no I/O, `Role` as a type parameter. Time enters as `Tick` in milliseconds since 0000-01-01 (D13). Module `schedule` holds when a session is open and when both ends restart at `34=1` (ADR-0033) | `codec`, `dict` |
| `engine` | L3 | TCP acceptor and connector, drives the session machines, owns the journal and the message log. `transport` is a module here until something needs it to be a crate | `session`; `libc` **only** under the `standard` or `affinity` feature |
| `library` | L4 | The application-facing API, package **`fixbolt`**: `Handler`, `Incoming`, `Reply`, `App`, and a curated re-export of what an application needs (`serve`, `Config`, `Table`, `Limits`, `Settings`, `Handles`, `Observer`, `Admin`, `Recovery`, `FileJournal`, `FileLog`, …). `Engine`, `Dispatch`, `Transport`, `wait`, `shard`, `affinity`, `frame` and `ring` are deliberately absent; reaching for one means naming `fixbolt-engine` yourself | `engine` |
| `metrics` | L4 | Package **`fixbolt-metrics`**: a Prometheus exporter that holds an `observe::Observer` per engine and nothing else. One thread, `fixbolt-metrics`, wakes every `tick`, serves `GET`/`HEAD /metrics` (text format 0.0.4) and `/healthz`, and sleeps; it asks an engine for a snapshot at most once per `min_request_interval` and never wakes one asleep in `poll`; it reads events only after `with_events`; it allocates nothing per scrape (ADR-0170). Every series name is public API, defined once in `src/series.rs` (ADR-0171). `publish = false` until phase 4 row 2 | `engine`, with `default-features = false` |
| `store-sqlite` | L3, beside `engine` | Package `fixbolt-store-sqlite`: `SqliteJournal` / `SqliteStore`, a `Journal` whose durable copy is a SQLite database, one per session — `FileJournal` `Async`'s engine-thread half (a `MemJournal` and one ring push; no `rusqlite` type in the struct), and a writer thread that commits in batches (WAL, `synchronous` `NORMAL` or `FULL`, never waited on by `put`). One appender is SQLite's own `EXCLUSIVE` lock (`WouldBlock` at once); retires, releases and idles through the engine's `WriterTicket`, `Releaser` and `ring::Idle`. Everything behind the default feature `sqlite`; without it the crate is empty and compiles no C. Not a dependency of `engine` or `library` ([ADR-0180](decisions/ADR-0180-the-sqlite-store-is-the-async-journal-with-a-database-for-a-file-one-database-per-session-and-no-synchronous-mode.md), [ADR-0181](decisions/ADR-0181-a-journal-outside-the-engine-joins-the-engines-writer-bookkeeping-through-three-public-handles.md), [ADR-0182](decisions/ADR-0182-the-sqlite-store-is-born-release-shaped-behind-a-default-feature-and-joins-the-tagged-release-family-only-when-its-kill-line-passes.md)) | `engine` (no default features), `session`; `rusqlite` (`bundled`) **only** under `sqlite` |
| `conformance` | dev | The `.def` acceptance runner for both roles, the corpus loader, and the echo application the corpus assumes. Built **before** `session`, so the gate existed before the thing it gates | `codec`, `dict` |
| `tools/w2w` | tool | The wire-to-wire harness, and the binary the two mode checks trace. Counts its own allocations on both threads over the timed window and asserts zero. Has its own `[features]` block, because a `cfg` never reaches into a dependency's features. `[2026-09-14]` splits into two processes: `--listen <addr>` runs the engine half alone and prints `mode:`/`path:`/`listening:`/`tls:` and its engine thread's allocations from the first logon to the last close, and **no latency figure**; `--connect <addr>` runs the generator half alone and prints a table headed *as the counterparty sees it*, with no `mode:` or `tls:` line. Each half refuses the flags that belong to the other, and both refuse `--tls` other than `off`. `--interval <us>` paces sends by spinning on the client thread. `--journal mem|file-async` and `--log none|file` (both default to today's behaviour) apply to the combined run and `--listen`; `--connect` refuses both. `[2026-09-14]` `--wire-timestamps --nic <ifname> --observer-core <cpu>` (Linux, wherever the engine runs; `--connect` refuses it): an observer thread reads an `AF_PACKET` tap with hardware RX stamps and the engine socket's error queue (`SO_TIMESTAMPING` TX hardware, `OPT_ID_TCP`, set by the observer on a `dup` before `engine.add`, so the engine thread makes no new call), pairs request and reply by the TCP byte stream (`src/pair.rs`), and prints `hw-rx-missing`, `hw-tx-missing` and `wire p50/p99/p99.9` beside `allocs` counted on every thread; a missing hardware stamp is counted, never replaced. **`hft` only on a hardware NIC**: `--mode standard` there is refused, because a queued TX stamp raises `POLLERR` and spins a blocking engine ([a-transmit-timestamp-wakes-a-blocking-engine](reference/a-transmit-timestamp-wakes-a-blocking-engine.md)). `--listen --warmup <n>` leaves the first `n` requests after the logon out of the wire window. Needs `cap_net_raw,cap_net_admin` by `setcap`; the gate scripts run the flag on `lo` inside `unshare -Urn` instead | `engine`, `session`, `codec`, `dict`; `libc` on Linux |
| `tools/jrnl` | tool | Reads a journal file from outside the process that wrote it; warns on a torn tail or a bad checksum with exit code 2. Takes `engine` with `default-features = false` | `engine` |
| `tools/interop` | tool | Both roles against a real `libquickfix` over kernel TCP, and (`--role dial`, feature `tls`, off by default) against QuickFIX/J: the engine's real initiator door, and the acceptor role's TLS branch (`serve_tls_requiring`). The C++ counterparties are built by `scripts/interop.sh`, never by cargo | `library` (as `fixbolt`, no default features), `session`; `fixbolt-engine` directly for `connect_and_serve` and, under `tls`, `rustls`/`ktls-core` through `fixbolt-engine/tls` |
| `tools/attr-scan` | tool | Reads the inner attributes of a crate root with `proc-macro2` — the lexer `rustc` uses — and prints one `PATH:LINE HEAD IDENTS` line each, for `scripts/check-no-crate-root-allow.sh`. Package `fixbolt-attr-scan`, binary `attr-scan`, following the three tools beside it; the gate calls `cargo run -q -p fixbolt-attr-scan`. **No crate depends on it** | `proc-macro2` (feature `span-locations`) |
| `tools/interop-qfj` | **not a crate** | `Judge.java`: this repository's own judge against a real QuickFIX/J 3.0.2, both roles, plaintext and TLS ([ADR-0130](decisions/ADR-0130-a-jvm-enters-ci-as-a-second-oracle-quickfixj-by-pinned-jar-and-our-own-judge.md)). Compiled by `javac` against five jars `scripts/interop-qfj.sh` fetches and pins by SHA-256 into gitignored `vendor/quickfixj/`; no Maven, no Gradle, no `pom.xml`, never in a `Cargo.toml` (CLAUDE.md §2 rule 6, extended to Java) | QuickFIX/J's public API only (non-negotiable 9) |

### What `engine` contains

The crate is the largest, and its modules are the record of what was built when:

| Module | What it is | Decision |
|---|---|---|
| `transport` | `Transport`, `TcpTransport`, `Loopback`, `Waiting` | D5 |
| `poll`, `block`, `waker` | `poll(2)` and `standard`'s idle turn, behind `#[cfg(all(feature = "standard", unix))]`. The crate's first external dependency and first `unsafe`, both behind that feature | [ADR-0014](decisions/ADR-0014-standard-mode-blocks-on-poll.md) |
| `affinity` | `CoreId`, `pin_current_thread` (reads the mask back), `running_on` (reads `/proc/thread-self/stat`), `Topology`, `ShardPlan`, `CorePin` (one core for `serve_hft_pinned`, validated as a `ShardPlan` of one), `spawn_pinned`. Behind `#[cfg(all(feature = "affinity", target_os = "linux"))]`, off by default. Two `unsafe` blocks, each naming its test | [ADR-0015](decisions/ADR-0015-explicit-cores-pinned-from-inside-and-read-back.md), [ADR-0019](decisions/ADR-0019-two-unsafe-blocks-and-an-error-the-enum-can-hold.md) |
| `shard` | `Shards`, `Shardable`, `serve_sharded_hft`: one pinned thread per shard, each confirming its own pin before any of them serves. `serve_sharded_hft` validates the plan before it binds the address, so nothing is acquired for a plan that will be refused. The acceptor thread blocks, because it is not an engine thread | [ADR-0020](decisions/ADR-0020-a-pre-session-stage-owns-the-socket-until-logon.md), [ADR-0022](decisions/ADR-0022-the-pre-session-stage-enforces-two-definitions.md), [ADR-0064](decisions/ADR-0064-a-door-acquires-nothing-before-it-has-validated.md) |
| `presession` | `Identity`, `identity_of`, `is_logon` (reads `49=` / `56=` / `35=` by field scan, no dictionary); `Limits`, `PendingSet` (owns a socket until its first whole message, under a deadline and a ceiling with no defaults); `Registry`, `Entry`, `Table` (which counterparty, decided before a session exists; a trait, and `None` from it is the authentication hook — `[2026-09-05]` the stage asks `admit(id, logon)`, which sees `553`/`554`/`96` and defaults to `lookup`, which could not); `field_value` (one field out of a message, borrowed, so a credential check allocates nothing); `Route`, `HashRoute` (the shard a socket goes to, decided after its Logon). Everything allocated once, to the ceiling | [ADR-0020](decisions/ADR-0020-a-pre-session-stage-owns-the-socket-until-logon.md), [ADR-0026](decisions/ADR-0026-a-counterparty-registry-in-the-pre-session-stage.md), [ADR-0029](decisions/ADR-0029-the-pre-session-stage-enforces-four-definitions.md), [ADR-0030](decisions/ADR-0030-one-engine-holds-many-counterparties.md) |
| `journal` | `MemJournal` (the resend ring), `FileJournal` (append-only file with `Durability::{Async, Fsync}`, reloaded into a ring on open), `Reader` (the whole file, for a tool; allows itself to allocate), `Store = MemJournal<4096, 512>`. Records carry a CRC32 from format version 1 | [ADR-0008](decisions/ADR-0008-journal-is-a-trait.md), [ADR-0017](decisions/ADR-0017-the-inbound-count-is-persisted-after-delivery.md), [ADR-0037](decisions/ADR-0037-reading-a-journal-is-not-recovering-from-one.md), [ADR-0039](decisions/ADR-0039-a-fresh-journal-is-the-deployments-to-build.md), [ADR-0046](decisions/ADR-0046-the-ring-is-the-resend-store-and-a-replay-goes-in-batches.md) |
| `recovery` | `Recovery` (asked once per connection, after the registry names the counterparty, on the acceptor thread), `Resumed`, `NoRecovery`, `FromFn`; `serve_with_recovery`, `serve_hft_with_recovery`, and — since 2026-09-20 — the sharded doors `serve_sharded_hft_with_recovery`, `serve_sharded_hft_with_recovery_with` (`Start<J>` crosses the shard channel with the connection), generic over the journal. `Engine::add_resumed` is the seam for a caller driving the engine | [ADR-0034](decisions/ADR-0034-recovery-is-asked-once-the-counterparty-is-known.md), [ADR-0039](decisions/ADR-0039-a-fresh-journal-is-the-deployments-to-build.md), [ADR-0088](decisions/ADR-0088-recovery-reaches-the-sharded-runtime-and-the-journal-crosses-the-channel-with-the-connection.md) |
| `observe` | `Handles` (the one cell, made before the engine, adopted by it — [ADR-0054](decisions/ADR-0054-the-handles-are-made-before-the-engine-and-the-engine-adopts-them.md)); `Observer` / `Snapshot` / `SessionSnapshot` (on request; one relaxed load per turn while nobody asks; a fixed `[SessionSnapshot; 64]` plus `truncated`); `Event` / `EventKind` (pushed on a state change, never per message, losses counted); `Admin` / `Command` (the 3 a.m. operation, applied at the top of a turn; a refused `try_lock` loses nothing, a full queue refuses at the call). `[2026-09-24]` for `fixbolt-metrics`: `Occupancy { used, capacity }`; `Snapshot::ring_to_app()` / `presession_slots() -> Option<Occupancy>` (`None` is "nothing reported," not zero); `Observer::latest()`, the published cell read without asking for a new build; `Observer::ask()`, which raises the request flag without locking the cell to copy and discard a `Snapshot` — `request()` is now `ask()` then `latest()`, unchanged in what it returns (plan *Sửa 1*, review of PR #108, finding F5) | [ADR-0032](decisions/ADR-0032-observation-is-a-snapshot-taken-on-request.md), [ADR-0035](decisions/ADR-0035-an-event-is-pushed-and-a-loss-is-counted.md), [ADR-0036](decisions/ADR-0036-one-mechanism-two-capabilities.md), [ADR-0054](decisions/ADR-0054-the-handles-are-made-before-the-engine-and-the-engine-adopts-them.md), [ADR-0170](decisions/ADR-0170-the-metrics-exporter-holds-an-observer-and-nothing-else-allocates-nothing-per-scrape-and-publishes-only-after-its-kill-line.md) |
| `settings` | `Settings`, `SettingsError`, `Problem`: the QuickFIX-shaped configuration file, no dependency, strict. An unrecognised key, a file with no `[SESSION]`, and a half-written schedule are each errors carrying a line number | [ADR-0040](decisions/ADR-0040-a-configuration-file-refuses-what-it-does-not-understand.md) |
| `reconnect` | `Policy`: doubling backoff to a ceiling, no jitter; `connect_and_serve` | [ADR-0043](decisions/ADR-0043-backoff-without-jitter-and-a-reconnect-asks-recovery-every-time.md) |
| shutdown | `Admin::shutdown`, `Shutdown`, `Session::begin_logout`, `State::LoggingOut`, `DropReason::EngineShutdown`; `run`, `serve` and `serve_hft` **return** | [ADR-0038](decisions/ADR-0038-an-ordered-shutdown-is-a-state-not-a-flag.md) |
| `msglog` | `MessageLog`, `NoLog` (compiles away), `FileLog`, `FileLog::open_pinned`: both directions, refusals included, one line per message | D14 |
| `redact` | `MASKED` — the five fields that never reach disk in clear (`554`, `925`, `1402`, `1404` always; `96` only when any `35=` in the record is `Logon`/`UserRequest`, or there is no `35=`); `mask` (overwrite in place, length kept), `carries_secret`. Two pure functions over a borrowed slice, no dictionary: splits on SOH, and a DATA field's declared length wins over the next SOH found, so a value that legally contains one (D3) cannot leave part of a secret unmasked **when its length field precedes it**; a DATA secret with no preceding length field is masked to the next SOH only (plan *Sửa 2*; `tests/redact.rs::a_raw_data_holding_an_soh_is_masked_whole`, `::raw_data_of_a_logon_behind_another_msg_type_is_masked`). `[2026-09-23]` public, called by `msglog` and `journal` | [ADR-0110](decisions/ADR-0110-a-secret-is-masked-in-the-message-log-and-leaves-only-its-number-in-the-journal-file.md) |
| `origin` | `Sender` (`Send + Sync`, a third capability on `observe`'s `Arc`), the fixed origination queue, `ORIGIN_CAPACITY` and `ORIGIN_LEN`. Drained at the top of a turn beside `Command`; `Engine::speak_first` is the other door and lives in `lib.rs` | D15, [ADR-0048](decisions/ADR-0048-an-engine-that-can-speak-first-has-two-doors.md) |
| `ring`, `dispatch` | `RingDispatch` over an SPSC ring of `AtomicU8`, safe Rust, no dependency; `InlineDispatch` | D4, [ADR-0007](decisions/ADR-0007-spsc-ring-without-unsafe.md), [ADR-0011](decisions/ADR-0011-a-full-ring-disconnects.md) |

## 4. The decisions that shape it

### D1 — The session layer is a pure state machine with no I/O

The session owns the seven administrative message types (`0 1 2 3 4 5 A`) and hands
everything else to the application. It has no socket, no clock and no allocation. Time arrives
as `Tick`. Errors are fieldless enums: no `String`, no `format!`, nothing that allocates on an
error path.

```rust
pub trait Role: sealed::Sealed { const SPEAKS_FIRST: bool; }
pub struct Acceptor;   // SPEAKS_FIRST = false
pub struct Initiator;  // SPEAKS_FIRST = true

pub trait Application {
    fn on_message(&mut self, msg: &[u8], seq: u32, stamp: &[u8], out: &mut [u8])
        -> Option<Range<usize>>;
}
```

**Why it is the highest-leverage decision in the design:** the 59 QuickFIX acceptance
definitions become unit tests. No listening socket, no timing window, no flake, and they run
in milliseconds. A session layer entangled with I/O can only be tested through a socket, and
socket tests are the ones that get muted. It also makes the engine replaceable without
touching protocol correctness.

**As built.** The four inputs are four methods, `connect`, `disconnect`, `tick` and
`received`, each taking an `emit` closure the caller supplies and each answering
`Link::{Up, Dropped}`. One input may call `emit` up to five times (`[measured]` two corpus
files need five). `Role` is a sealed trait with two marker types, so the branch resolves at
compile time. The `Application` is given the two things it does not own, the outbound
sequence number and the clock, writes its reply into a buffer the session lends it, and
returns the range it used; `None` spends no sequence number. **A session judged before its
first `tick` refuses with `DropReason::NeverTicked`**: the obligation D1 moves to the caller —
time arrives from outside, so somebody outside has to send it — is named at the boundary rather
than assumed. `[2026-09-12]`

**Two deadlines the caller may state, and both arrive through `tick`.** `[2026-09-05]`
`Config::with_logon_timeout_ms` and `with_logout_timeout_ms` — QuickFIX's `LogonTimeout` and
`LogoutTimeout` — end a connection that never completes its Logon, and one whose `Logout` is
never answered, with `DropReason::LogonTimedOut` and `LogoutTimedOut` to say which. Off by
default. **Neither is a clock in the session**: each is measured from the first `tick` after the
event it bounds, which is the only shape D1 permits, and each belongs to the connection rather
than to the session so a reconnect gets a whole new one. The logon one is the initiator's — an
acceptor has `presession::Limits` in front of it, before a `Session` exists.

**One machine, both roles.** The acceptor waits for a Logon and answers; the initiator sends
one and waits. Sequence handling, resend, heartbeat, test request and logout are the same
protocol read from the other end; [ADR-0004](decisions/ADR-0004-bidirectional-engine.md)
found that 51 of the 59 definitions mirror unchanged. The two roles are asymmetric in one
place the corpus could not show: `[measured 2026-09-02]` the inbound-Logon handler answered
with a Logon for both roles, which for an initiator starts a second handshake on a session
that already has one. It was found on the first run of `scripts/interop.sh`, by `libquickfix`
dropping the connection without a word, and the reply is now behind `!R::SPEAKS_FIRST`
([reference/a-role-can-be-wrong-in-a-direction-no-gate-runs.md](reference/a-role-can-be-wrong-in-a-direction-no-gate-runs.md)).

**An initiator needs one thing a pure machine cannot give it: intent.** `[measured
2026-08-30]` 46 of the 50 mirrorable definitions require this end to send a message nothing
on the wire asks for and no clock produces, 42 of them a Logout. So the layer has six
functions that take an operator's intent:

```rust
send_heartbeat(emit)                  // 35=0, carrying no 112=
send_test_request(id, emit)           // 35=1, the caller's 112=
send_resend_request(from, to, emit)   // 35=2, the caller's 7= and 16=
send_sequence_reset(n, emit)          // 35=4 with 123=N, and become n
begin_logout(text, emit)              // 35=5, then wait for theirs
send_application(msg, journal, emit)  // anything the session does not own
```

None of them takes whole message bytes, and that boundary is a decision
([ADR-0042](decisions/ADR-0042-a-second-implementation-is-the-only-independent-opinion.md)).
A caller supplies the fields it owns; the session builds the message from its own `Template`
and keeps `8`, `9`, `34`, `49`, `52`, `56` and `10`. `benches/alloc.rs` reads `ordered 0`.

**A connection and a session are different things.**
[ADR-0010](decisions/ADR-0010-a-reconnect-is-not-a-restart.md): FIX 4.4 numbers a
**session**, not a connection, so a session that outlives its process must keep counting.
`Session::resume(cfg, next_out, next_in)` builds one that already carries numbers and
`connect` leaves them alone; a session from `Session::new` has persisted nothing and resets
on every connection. The corpus keeps its meaning by construction: the runner builds a session
per scenario with `new`, so all seven reconnects in the corpus expect `34=1` and the score is
59 / 59 unchanged. `[measured 2026-08-31]` forcing `connect` to never reset drops it to 56 /
59, which proves the corpus exercises that branch. Recovering the numbers is the engine's job;
`Session::next_out()` and `next_in()` exist so it can persist them.

**And an operator can override all of that, in three places.** `[2026-09-05]`
`Config::with_reset(ResetPolicy)` — QuickFIX's `ResetOnLogon`, `ResetOnLogout` and
`ResetOnDisconnect` — restarts both counts at 1 from this end, where until now only the
counterparty's `141=Y` could. **It is a different question from `new` versus `resume`**: those
say what the journal still holds, this says what the session wants next time, and only the
second one is a thing a desk writes in a file. The default resets on nothing, so the corpus is
untouched. `on_logon` acts in `connect`; the other two act in `Session::end`, **after** the
message that ends the session has been written, because a `Logout` renumbered before it goes
out spends `34=1` twice in one session. `docs/SESSION-BEHAVIOUR.md` §4 names the six tests —
three for the flags and three for their absence.

**Both counts survive, and the inbound one is written after delivery.**
[ADR-0017](decisions/ADR-0017-the-inbound-count-is-persisted-after-delivery.md): the journal
carries `mark_in(seq)` and `highest_in()`, so one file holds both directions. The session
calls `mark_in` at the end of `received_with`, after judging and after draining held
messages. **The ordering is the decision.** Writing the mark *before* delivery would mean an
ill-timed crash loses the message: this end has counted it and will never ask for a resend,
while the counterparty believes it arrived. Writing it *after* means the message is delivered
twice, and the second copy carries `43=Y`. FIX has a flag for the second failure and nothing
for the first. The cost: under `Durability::Fsync` the inbound path pays a sync per message,
and an application behind this engine must be idempotent per sequence number
([GUIDE.md §6a](GUIDE.md)).

**The journal is a trait the session is handed**, not an action it emits: a resend has to
*read*, and an action is one-way. `Session::received` supplies `NoJournal` so a pure protocol
machine is unchanged ([ADR-0008](decisions/ADR-0008-journal-is-a-trait.md)).

### D2 — The field index is separate from the message view

Measured, not assumed; full detail in
[ADR-0003](decisions/ADR-0003-message-representation.md) and
[reference/measured-costs.md](reference/measured-costs.md).

```rust
#[repr(C)]                       // 12 bytes, natural alignment 4. NOT align(16)
pub struct FieldEntry { tag: u32, offset: u32, length: u16, _pad: u16 }

pub struct FieldIndex<const N: usize> { count: u16, fields: [FieldEntry; N] }  // reusable, no lifetime
pub struct MessageView<'a, const N: usize> { buf: &'a [u8], idx: &'a FieldIndex<N> }

/// Incomplete is Ok, not Err: TCP delivers bytes, not messages.
pub enum Parsed { Complete { consumed: usize }, Incomplete }

pub fn parse_into<D: Dictionary, const N: usize>(
    buf: &[u8], idx: &mut FieldIndex<N>, v: Validation,
) -> Result<Parsed, ParseError>;
```

The caller owns one `FieldIndex` and reuses it for every message on that connection. The
parser never constructs or returns a large struct.

**`MessageView` is 24 bytes**: a fat `&[u8]` (16) plus a reference to the index (8).
`[measured]` verified with `rustc -O` on 2026-08-27. On x86-64 SysV and AArch64 a struct over
16 bytes is passed indirectly, so any hot-path function taking a `MessageView` by value
carries `#[inline]`, and `crates/codec/src/index.rs` carries
`const _: () = assert!(size_of::<MessageView<64>>() == 24);` so that growing it fails to
compile rather than silently costing a spill.

`N` is a const generic, so the caller chooses: `FieldIndex<64>` for order flow,
`FieldIndex<512>` for a market-data snapshot. Overflow is `ParseError::TooManyFields`, never
silent truncation.

**Repeating groups do not change the index.** The index stays flat: `parse_into` records tags
in wire order and knows nothing about groups. A group is resolved only when asked for, by
`MessageView::group(msg_type, counter)`, which walks the flat entries. Three consequences:

- **A message with no group pays nothing.** `[measured]` parse is unchanged at 77 ns; walking
  a group is a separate 29–145 ns depending on depth (`benches/groups.rs`).
- **Nothing is allocated.** `benches/alloc.rs` walks four nesting levels and reports 0.
- **The scan steps over nested regions.** A group ends at the first tag outside its member
  set, and a nested group's members are not members of the group around it. `[measured]` 235
  of the 731 group positions in FIX 4.4 contain a nested group.

`declared()` (what the counter says) and `counted()` (what is on the wire) are reported
separately and never reconciled by the codec. Whether a mismatch is a `Reject 373=16` is the
session layer's decision.

`[added 2026-09-19]` This view is one encoding's view. `Encoding` (D16) names it as
`TagValue::View<'a>` and forwards to it unchanged; a second encoding brings a second view type
of its own rather than widening this one.

`[added 2026-09-23]` A typed value is read the same way: a price is
`as_decimal(view.get(44)?)`, a free function beside `as_i64`, not a method on `MessageView`. The
index is unchanged and nothing is decoded because a message arrived — `Decimal { mantissa: i64,
exponent: i8 }` is produced only when the caller asks, 16 bytes, `Copy`. ADR-0120 (which revises
ADR-0028 decisions 1 and 2).

### D3 — Field ordering comes from generated tables, never from hand-written code

The QuickFIX acceptance comparator compares fields **positionally**: a correct FIX message
whose fields are in a different order fails
([reference/quickfix-acceptance-def-format.md](reference/quickfix-acceptance-def-format.md)).
So the serialiser emits in an order derived from the dictionary at build time, and ordering is
never a judgement made at a call site.

**Inside a repeating group the ascending-tag rule does not apply.** `MsgType` first, then
header tags ascending, then body tags ascending governs the message; a group entry is written
in the dictionary's **declaration** order, delimiter first (`269` before `270`, `279` before
`285`). `Template::encode_with::<D>` walks `D::group_order(msg_type, counter)` and never the
order the caller supplied. `[measured]` `crates/codec/tests/group_roundtrip.rs` hands every
entry over in reverse and round-trips 357 top-level positions byte for byte.

**A round trip against your own table proves stability, not correctness**, so the order is
checked against QuickFIX's generated C++: `[measured]` the delimiter agrees on 730 / 730 groups
and QuickFIX's order is an exact subsequence of this crate's on 730 / 730
(`crates/dict/tests/interop_quickfix_order.rs`). Swapping two adjacent members in every group
leaves the round trip green and turns that test red, which is why it exists. `[2026-09-23]` the
generator's input moved from gitignored `vendor/` to `crates/dict/spec/`, shipped inside the
crate under `NOTICE` (ADR-0104) — the generated tables are unchanged, proven by the emitted
`.rs` hashing identical to the pre-switch commit, so this section's numbers still hold.

**A DATA field is written immediately behind its length field, and the encoder writes that
length.** A DATA value may legally contain `0x01`, so a reader takes its length from the field
in front. A DATA field declared without its length field fails at `TemplateBuilder::build`
with `EncodeError::DataWithoutLength`; inside a group the same case fails in `encode_with`
before a byte is written; and the length is computed from the data, never taken from the
caller. `[measured 2026-08-30]` fifteen of FIX 4.4's sixteen DATA pairs have
`length == data − 1`, so ascending-tag order was right by accident; `Signature(89)` takes
`SignatureLength(93)` and was emitted before its length. Held by
`crates/codec/tests/data_encode.rs` and by `group_roundtrip.rs`, which writes 508 DATA members
with a separator inside every value.

### D4 — Dispatch is a trait; inline is the default, the ring buffer is the option

Taken from [Artio](https://github.com/artiofix/artio), which separates the engine (owns
connections and session lifecycle) from the library (runs business logic), **but not adopted
as the default**. Artio's split is justified largely by the JVM: process isolation contains GC
pauses. Rust has no GC, so what remains is a property some applications need and others pay
for ([ADR-0002](decisions/ADR-0002-engine-library-split.md)).

- **`InlineDispatch`**: the handler runs on the engine thread, directly after the session
  machine. Zero hand-off, zero copy, the borrowed `MessageView` handed straight through. The
  HFT-standard shape: `recv → parse → decide → encode → send` on one core.
- **`RingDispatch`**: bytes are copied into an SPSC ring and an application thread consumes
  them. Costs a hop (§6) and buys the one thing inline cannot: an application that blocks
  does not stall the session layer.

**As built.** `Dispatch` carries a `const OUT_OF_BAND: bool`, `false` for `InlineDispatch`, so
the engine's "collect what the other thread produced" block compiles away on the default
engine. A reply from the ring comes back through `Session::send_application`, so the sequence
number and SendingTime are the session's own; an application on another thread cannot get
either wrong because it is never told them. A reply is routed by a **connection id, never an
index**: the engine drops a dead connection with `swap_remove`, so an index is stale the
moment anything hangs up. `crates/engine/tests/dispatch.rs` asserts that, and asserts the
property that makes the trait worth having: **the same message produces the same bytes on the
wire under either dispatch.** The ring itself is `Box<[AtomicU8]>`, safe Rust, no dependency,
a byte-at-a-time copy whose price is published ([ADR-0007](decisions/ADR-0007-spsc-ring-without-unsafe.md)).

### D5 — Transport is a trait; TCP is the only implementation that ships by default

```rust
pub trait Transport { fn recv(&mut self, buf: &mut [u8]) -> io::Result<usize>; fn send(&mut self, buf: &[u8]) -> io::Result<usize>; }
```

Two rules, both learned from reading `matthart1983/nanofix`:

1. **A feature flag gates the module declaration itself**: `#[cfg(feature = "aeron")] mod aeron;`.
   In that project the flag exists in `Cargo.toml` while `src/lib.rs` declares the module
   unconditionally, so `cargo test --no-default-features` fails to link for everyone without
   Aeron installed.
2. **`build.rs` invokes no external toolchain unless that feature is on.** In that project it
   panics regardless, with the author's home directory as a fallback search path.

Together those make a crate unbuildable for anyone but its author. Avoiding it costs nothing.

### D6 — No `panic!`, `unwrap()` or `expect()` in any library crate

Enforced by a workspace clippy lint, not by discipline. The reference implementation carries
**276** of them; discipline alone demonstrably does not hold this line.

### D7 — Persistence is a policy, and it is off the hot path

QuickFIX's `FileStore` calls `Sync()` on every write across three files, which is the dominant
latency source in its default configuration. Here the policy is the user's:

| Policy | Type | Meaning |
|---|---|---|
| none | `fixbolt_session::journal::NoJournal` | nothing kept; resend is impossible. Tests, simulators |
| in memory | `engine::journal::MemJournal` | a ring that keeps but does not persist |
| `Async` | `engine::journal::FileJournal` | appended, flushed by a background thread. Survives a process crash |
| `Fsync` | `engine::journal::FileJournal` | synced before the message is acknowledged. Survives a power loss. Regulated deployments |

**The in-memory ring is the whole resend store, in every policy.** A `FileJournal` keeps the
ring too and answers `get` from it: reading a replay back off disk would be a blocking `read`
on the thread non-negotiable 4 protects. Anything older than the ring is gap-filled, which is
legal and invisible to the counterparty's engine
([ADR-0046](decisions/ADR-0046-the-ring-is-the-resend-store-and-a-replay-goes-in-batches.md)),
and since 2026-09-04 it is **counted and emitted**: `EventKind::ResendBeyondJournal { filled,
oldest }` in messages, and `JournalRefused { count }` for a reply longer than a slot — and,
since 2026-09-24, `JournalUnwritten { count }` for records the ring kept and the file missed.

- **`SLOTS` is 4096** (it was 8 until 2026-09-04: the smallest power of two above what the
  corpus asks for, and an acceptor that had sent a hundred ExecutionReports replayed eight of
  them). `N × (LEN + 8)` ≈ 2 MiB per session; `[measured 2026-09-04]` `tools/w2w` reads
  **+2 195 456 bytes** of resident set against `SLOTS = 8`.
- **`get` is one index and one comparison**, addressed by `seq % N`. The scan it replaced
  returned the *first* slot carrying a number, which after `Admin::SetNextOut` wound the count
  back was the stale copy: correctly numbered, correctly checksummed, wrong message.
- **A replay goes out in batches** of `Config::resend_batch` messages (default 8), continued
  from `Session::tick_with` and from each judged message. Before this the whole range went out
  in one call, and a resend larger than `TX` tripped D10 and ended the session as a *slow
  consumer*. The corpus cannot see it, because no definition asks for more than three
  messages; `crates/engine/tests/backpressure.rs` does.

**Under `Async` the writer thread's buffer holds the largest record the slot allows**
(`RECORD_HEADER + LEN`, allocated once on the writer thread), and its stop signal is a one-byte
`STOP` record no journal record can be; a record `pop` had to drop is skipped, never read as
*stop*. Until 2026-09-23 a fixed 4 096-byte buffer and an empty stop record let one long message
stop the writer for good. **An idle writer sleeps**: after 1 024 consecutive empty polls
(each a `spin_loop` hint) it sleeps 1 ms per poll until a record arrives, in every mode; the
engine thread never wakes it, so its path is unchanged and a record pushed to a sleeping writer
reaches the file up to 1 ms later
([ADR-0150](decisions/ADR-0150-the-journal-writer-holds-the-largest-record-the-slot-allows-and-stops-only-on-a-record-no-message-can-be.md),
[the trap](reference/an-async-journal-record-longer-than-its-writers-buffer-stopped-the-writer.md),
[the idle writer](reference/a-writer-thread-no-gate-watched-spun-a-core.md)).

**A journal file has one appender, for the file's whole life** (2026-09-24,
[ADR-0154](decisions/ADR-0154-a-journal-file-has-one-appender-a-reconnect-waits-for-it-by-parking-and-what-the-file-missed-is-counted.md)).
`FileJournal::open` takes an exclusive `File::try_lock` (`flock`) **before it reads the file**; the
lock lives with the `File` — the writer thread's under `Async`, released after its last flush, or
the journal's under `Fsync` — and is unlocked explicitly before the close, because a child
process's momentary copy of the descriptor would otherwise keep it. A second `open` of the same
path, in this process or another, fails at once with `WouldBlock`; it never waits. That is what
makes ADR-0153's retire safe for a counterparty that reconnects at once: its recovery would
otherwise read a file the retired writer had not finished (`[measured 2026-09-24]` 50 reconnects
in 50 resumed from a short file). **`Recovery::ready`**, defaulted to `true`, is asked before `recover`, on the engine thread in
`pump`, so it must answer without a system call: a recovery keeps each journal's
`FileJournal::released()` handle — a flag the writer sets after closing its file — and answers
with one atomic load (`journal::file_busy` opens the file and can sleep, so it is for tools and
the sharded acceptor thread only,
[ADR-0155](decisions/ADR-0155-a-recovery-learns-its-writer-let-go-from-the-writer-not-from-the-filesystem.md)); *not yet* **parks** the connection — beside
the pre-session set in `pump` and on the sharded acceptor thread, in the handshake slot in `dial` —
asked again at most once per millisecond, dropped after its `LogonTimeout`. Parking is not
progress: `standard` still idles, `hft` still never sleeps. **What the file missed is counted**:
a push the writer's ring refused leaves `put` answering `true` (memory holds the message and
replays it while the process runs) and raises `Journal::unwritten()`, which the engine reports as
`EventKind::JournalUnwritten { count }`. A journal refused with its prefix, before it became a
connection, is retired rather than dropped, and `PRE <= RX` for the sharded runtime is a
compile-time assertion
([the trap](reference/a-reconnect-reopened-a-journal-its-retired-writer-still-owned.md)).

**A departing connection's journal is retired, not closed, and its writer is awaited only after
serving.** `Connection` has a `Drop` that calls `Journal::retire` (a defaulted no-op in
`fixbolt_session`, so the session stays pure), so every way a connection leaves — `turn`'s
`swap_remove`, a shutdown's `clear`, the engine's own drop — retires first. `FileJournal::retire`
makes no syscall and never spins: it pushes `STOP` once (or, if the ring is full, tells the writer
to stop when the ring runs dry), detaches the writer, and counts it among the process's retired
writers; the writer uncounts itself as its last act. `journal::wait_for_retired_writers(timeout)`
sleeps on that count, and every `serve*`, `connect_and_serve*` and sharded serve loop calls it
**after** its loop has returned, with the shutdown grace (never under 1 s) as the timeout. A
caller driving `Engine` directly must call it before exiting ([GUIDE.md §6b](GUIDE.md)).
Dropping `shard::Shards` disconnects and then **joins** every shard thread, so it returns only
after each shard's wait — `[2026-09-24]` it used to detach them, and a wait asked straight after
the drop read zero writers before any shard had retired one
([the trap](reference/a-drop-that-only-signals-is-not-a-shutdown.md)).
`[2026-09-23]` before this the drop joined the writer on the engine thread mid-serving — a
`futex` wait in `hft`, a stall of every other session in `standard`
([ADR-0153](decisions/ADR-0153-a-connections-journal-is-retired-without-waiting-and-its-writer-is-awaited-only-after-serving.md),
[the trap](reference/a-connection-end-joined-its-journal-writer-on-the-engine-thread.md); `crates/engine/tests/retire.rs`).

**The file is appended, not memory-mapped**: `mmap` means a dependency or `unsafe`, and the
engine plan authorised neither ([ADR-0008](decisions/ADR-0008-journal-is-a-trait.md)). A
record carries its own length, `seq(4) || len(4) || bytes`, and from format version 1 a CRC32,
so a torn tail and a flipped byte both stop the read rather than being replayed to a
counterparty. **Three header shapes are not messages**: `len == 0` is ADR-0017's inbound mark;
`seq == 0 && len == 8` is ADR-0039's activity mark, eight little-endian milliseconds saying when
the session was last alive, written at logon and at an ordered shutdown, never per message; and
`seq == 0 && len == 4` is
[ADR-0053](decisions/ADR-0053-the-journal-answers-two-questions-and-the-second-is-a-number.md)'s
outbound mark, the highest `34=` this session has spent. `34=0` is not a sequence number FIX has,
so none of the three cost a format change — and **the third is the last that gets that escape**:
a fourth shape would be a format only its own history can read, so the next one lifts the
version to v2.

**A journal answers two questions, and only the first is about bytes.** *What can you
replay?* — `get`, `highest`, `oldest`. *How far have you counted?* — `highest_out` and
`highest_in`. `[measured 2026-09-05]` **treating the first as an answer to the second is a
defect that reaches the wire**: `highest()` is the highest message *held*, and a `Logon`, a
`Heartbeat` and a `Logout` each spend a `34=` no journal holds bytes for, so a restart deriving
`next_out` from `highest()` is short by every administrative message since the last application
one. A real `libquickfix` refused a resumed session over a difference of exactly one. So the
session tells the journal the count it has spent — `mark_out`, a high-water mark — and
`Resumed::from_journal` does the arithmetic once (ADR-0053). `FileJournal::open` reads the file
before appending, `Session::resume` and `Engine::add_resumed` carry the numbers into a running
engine, and `Recovery` is the seam the serving loop asks
([ADR-0034](decisions/ADR-0034-recovery-is-asked-once-the-counterparty-is-known.md),
[ADR-0039](decisions/ADR-0039-a-fresh-journal-is-the-deployments-to-build.md)). Held by
`crates/engine/tests/recovery.rs`, `tests/on_disk.rs` and
`crates/session/tests/numbering.rs`.

**The journal is not a message log.** It keeps outbound application **messages** for resend and,
of everything else, only **numbers** — one inbound, one outbound. No administrative traffic and
no refused frame is in it; that is D14. **`[2026-09-23]` Nor does it keep an application message
that carries a secret** `engine::redact::MASKED` names: the file gets the ADR-0053 outbound mark
for its number instead of the message record, and a resumed session gap-fills that number rather
than replaying a credential. The in-memory ring still holds the message verbatim, so a
`ResendRequest` inside the same process replays it unaffected
([ADR-0110](decisions/ADR-0110-a-secret-is-masked-in-the-message-log-and-leaves-only-its-number-in-the-journal-file.md)
decision 4; [SESSION-BEHAVIOUR.md §4](SESSION-BEHAVIOUR.md)).

### D8 — In `hft` the engine thread busy-polls; in `standard` it blocks

Mode-scoped, and `standard` is the default
([ADR-0013](decisions/ADR-0013-two-modes-standard-and-hft.md)):

| | `standard` (default) | `hft` (opt-in, Linux only) |
|---|---|---|
| Idle behaviour | blocks on readiness with a timeout, and gives the core back | spins on non-blocking sockets, never enters the kernel |
| Cost of a wakeup | `epoll`-class: **`[measured 2026-09-18]` 4 960 ns p50 (`epoll_wait`), 4 819 ns (`poll`)**, p99 ≈ 7.9 µs, p99.9 ≈ 8.9 µs — `crates/engine/benches/wakeup.rs`, two isolated §9 cores, 20 runs × 20 000, [ADR-0025](decisions/ADR-0025-hft-has-a-hard-session-ceiling-and-the-engine-advises-rather-than-applies.md) *Measured* (the literature said 2–5 µs; this desk reads the top of it) | `[measured 2026-08-31]` one turn at **449 ns per session** on a §9 core; the arithmetic crossover with the measured wakeup is N ≈ 11, and the `hft` ceiling stays 4 until the busy turn at N > 1 is measured |
| Pinning | none | `serve_sharded_hft` and `serve_hft_pinned` validate the core and pin from inside; `serve_hft` pins nothing and says so |
| Runs on | any OS, any hardware, a container, a laptop | a machine that satisfies §9 |
| Rule 4 says | it **must** block | it must **not** sleep |

**Why `hft` spins.** An `epoll` wakeup costs 2–5 µs and brings scheduler jitter with it. On a
design whose entire user-space path is under 1 µs, a blocking wait is the single largest cost
the engine controls. `[measured 2026-09-02]` end to end the spin is worth **3 437 ns, 17.7%**,
at p50 against `standard` on the identical path (§8).

**Why `standard` exists and is the default.** An engine whose out-of-the-box configuration
pins a core at 100% looks broken to most people who try it. And the spin is not free: at
449 ns per session per turn, the trade wins at N = 1 and loses by N = 8, because 8 × 449 ns is
3.6 µs and clears the top of `epoll`'s range. `standard` is the honest default for everything
that is not one session on an isolated core.

**The listener is asked on a cadence, in the spin half only.** `pump` (the loop `serve`,
`serve_hft` and `serve_tls` share) keeps one `u32` countdown, `Limits::listener_every`
(`ListenerEveryTurns`, default 1 — today's loop, iteration for iteration): the accept loop runs
when the countdown reads zero, otherwise it is decremented and skipped. In `standard` the
countdown is reset to zero after every return from `idle_with`, so the listener is asked
unconditionally on the turn right after a wake — a wake is always answered by an accept, which
is what keeps the poll set drained and rule 4's `standard` half true. `serve_sharded_hft` never
enters this code; its acceptor is its own blocking thread. `ADR-0069` (Proposed) has the
alternatives and the consequences; no default other than 1 is set without a figure.

**As built, the shared loop.** `Engine::turn` is one non-blocking pass over every connection:
flush what is queued, **tick the clock**, read once, cut whole messages out, judge them, flush
again. `Engine::run` is `loop { if !turn() { wait.idle() } }`. Reading *once* per turn rather
than until the socket is empty is deliberate: a counterparty that writes faster than this end
processes must not starve the other connections on the thread. **The tick comes before the
read** because the session judges SendingTime against the last tick it was given, and a
session that has never ticked holds zero; reading first would refuse the first message on
every connection for skew. Keeping the pass separate from the loop is what lets the 59
definitions run through a real socket with no background thread and no timing window:
`crates/engine/tests/wire.rs` drives `turn` by hand.

**As built, `standard`** ([ADR-0014](decisions/ADR-0014-standard-mode-blocks-on-poll.md)).
`Waiting::idle` is handed the source list and `Transport` names its own descriptor. The
mechanism is `poll(2)` through `libc` behind the default-on `standard` feature; `epoll` is
O(1) where this is O(N) and is a later ADR with numbers. `Block` blocks at a **100 ms**
timeout, which is a correctness parameter rather than a knob: in `standard` that timeout is
what delivers `Input::Tick` to a session with no clock. The source list is one interest per
connection, readable always and writable only while bytes are queued, rebuilt every turn
because a `Source` borrows a descriptor that may have been reissued. `serve` hands the
listener to the poller, so a connection is accepted on the connect rather than on the next
timeout. A self-pipe wakes the poller for a reply produced on the application's thread, and
the engine drains it after every wait, because an undrained pipe makes every subsequent `poll`
return instantly: a working engine, burning a core. Pairing a blocking strategy with a
transport that cannot name a source does not compile. `connect_and_serve`'s dial loop idles the
same way while it waits to reconnect — on nothing but `Block`'s own timeout — and each wake goes
through `turn`, so an `Admin::shutdown` is heard within one timeout rather than when the redial
timer fires (`crates/engine/tests/reconnect_wire.rs`, the stop and the CPU both asserted;
`dial` exists only in `standard`).

**`wait::Yield` is neither mode.** It is `std::thread::yield_now()`, which yields the scheduler
and does not block, so it burns its core without giving `hft` its tight poll. Its rustdoc says
it fails both gates, and §6's `standard` gate demonstrates it.

**Pinning is something the code does, not something this paragraph asserts.**
`affinity::pin_current_thread` pins the calling thread and confirms it with
`sched_getaffinity`; `tests/affinity.rs` watches the scheduler's own `processor` field while
the thread works, and with the pin removed the same thread was observed on cpu0, cpu4 and cpu5
in one run. `Topology` and `ShardPlan::validate()` refuse a bad plan before any thread exists,
and `shard::Shards` runs one pinned engine per core, routing each socket by the identity in
its Logon. **One entry point pins nothing: `serve_hft`.** It spawns no thread of its own, so
the thread that calls it is the caller's to pin ([GUIDE.md §9](GUIDE.md), STATUS item 21).

**Both halves of rule 4 are machine-checked.** `scripts/check-no-kernel-sleep.sh` traces
`tools/w2w` with `strace -f` on Linux, attributes syscalls to the engine thread by tid, and
runs the binary a second time in `standard` mode requiring that run to trip the check.
`scripts/check-standard-gives-the-core-back.sh` asserts four things at once, because CPU near
zero is passable by three different broken engines (§6). `[measured 2026-08-30]` the 59
definitions pass in `standard` too, with the engine blocking between steps.

**Rule 4 holds when a session ends, not only while it runs.** A connection leaving mid-serving
retires its journal instead of joining its writer (D7), so neither mode waits for a writer on the
engine thread; the wait is teardown, after the serving loop, which ADR-0152 decision 1 allows.
`crates/engine/tests/retire.rs` counts the engine thread's voluntary context switches while 20
sessions with a `FileJournal` end: 0 in `hft`, no more than with a `MemJournal` in `standard`
([ADR-0153](decisions/ADR-0153-a-connections-journal-is-retired-without-waiting-and-its-writer-is-awaited-only-after-serving.md)).

### D9 — Outbound messages are templates: a pre-sorted parts list, patched, not built

An ExecutionReport from a given session has a fixed skeleton: BeginString, SenderCompID,
TargetCompID, MsgType, and the field order (D3). That skeleton is encoded **once per session
per message type** into a scratch buffer the template owns.

```rust
enum Part { Static(Range<u16>), Slot(u32) }        // ranges into the template's own scratch
pub struct Template<const P: usize, const S: usize> {
    scratch: [u8; S], parts: [Part; P], len: u8,
}
pub fn encode(&self, out: &mut [u8], slots: &[(u32, &[u8])]) -> Result<Range<usize>, EncodeError>;
```

Three properties the first sketch got wrong:

- **The parts are sorted at build time** (D3), so `encode` walks them in order and never makes
  an ordering judgement. A slot the caller does not supply is skipped, so one template serves
  messages that differ in their optional fields.
- **The body is written first; the prefix is then written right-aligned in front of it.**
  `BodyLength` is variable-width, so writing the prefix first would mean shifting the body once
  its width is known. That is why `encode` returns a `Range` and not a length.
- **SendingTime is the hidden cost.** Naive formatting is 50–100 ns, as much as a parse. The
  `YYYYMMDD-HH:MM` prefix is cached and re-derived once a minute; only `SS.sss` is formatted per
  message. `[measured 2026-08-31]` 4.9 ns from the cache, AMD Ryzen 7 3700X, n = 20,
  `benches/baselines.tsv:141`.
- **`[added 2026-09-09]` The width of that field is a session's configuration, and the cache
  carries the precision as a field rather than as a `const` parameter.** `TimestampPrecision`
  arrives from a settings file at run time, so a const generic would force `Session` and the
  whole engine to be monomorphised three ways behind a runtime `match`
  ([ADR-0057](decisions/ADR-0057-sub-millisecond-time-arrives-beside-the-tick.md), and the plan's
  Sửa 2). **The branch it costs instead was measured rather than argued**, one variable at a
  time, `[measured 2026-09-09]` on an Intel Xeon @ 2.80GHz — a shared cloud VM and **not** the
  §9 desktop, three runs per arm, `benches/serialize.rs`:

  | | ns/op |
  |---|---|
  | before the change, same machine, same run conditions | **4.7** |
  | returning a slice instead of `&[u8; 21]`, no precision branch | **4.8** |
  | as shipped, at the default `Millis` | **5.4** |
  | at `Micros` | **10.5** |
  | at `Nanos` | **13.0** |

  **+0.7 ns at the default, of which the runtime branch is 0.6 and the slice return is 0.1.**
  The 4.9 ns row above was **not** re-measured; these stand beside it on a different machine and
  do not replace it. The §9 figure is owed and is named in `STATUS.md`'s *Not proven*.

This shape is how the fastest commercial engines are reported to reach tens of nanoseconds per
serialise. That figure was once §6's published target, 60 ns, and
[ADR-0016](decisions/ADR-0016-per-machine-baselines-replace-absolute-targets.md) withdrew it:
it described other people's software, no machine here came within 1.5× of it, and
`[measured 2026-08-31]` the floor of this `Part` shape is about 116 ns even with the slot scan
removed. The cached-timestamp design is still right; what was wrong was borrowing somebody
else's number to grade it against.

### D10 — TCP send backpressure has a stated policy

**Two ends can fall behind, and they are different questions.** D10 is about the counterparty
on the wire; D10b is about our own application behind the ring. On the wire, a counterparty
that cannot keep up is broken; behind the ring, the counterparty is faultless and we are the
ones who stopped reading.

A slow counterparty fills the socket send buffer and `send` returns `EAGAIN`. At 50 000
ExecutionReports per second against a QA application this **will** happen. The engine must
not block the session machine and must not drop protocol messages silently.

| Policy | Behaviour |
|---|---|
| `Queue { max_bytes }` | buffer in the connection's `TX` up to a bound, then… |
| `Disconnect` | …drop the session with `Logout 58=slow consumer`. **The default**: a FIX counterparty that cannot keep up is a broken counterparty |
| `Block` | …spin until the socket drains. For tests; never the default, because one slow counterparty then stops every other session on the thread |

**As built.** `Backpressure` on `Engine` or on a single `Connection`; the queue is the
connection's `TX` buffer and `Queue { max_bytes }` only tightens the bound. Three rules
`crates/engine/tests/backpressure.rs` holds: a message goes in whole or not at all (a partial
frame is unrecoverable at the other end); the Logout is not subject to `max_bytes`, because
the queue is discarded first so the one message that matters has room; and a socket that has
died ends the connection even with bytes queued (`[measured 2026-08-30]` before this, killing
the socket mid-write left the connection `Up` for as long as it was turned).

### D10b — A full ring to the application ends the connection

[ADR-0011](decisions/ADR-0011-a-full-ring-disconnects.md). Under `RingDispatch` the
application is on another thread. If it stops draining, the ring fills, and until 2026-08-31
the answer was a counter nobody read. A message counted there is one the session accepted,
numbered, journalled and acknowledged by sequence number, that the application never saw:
for order flow that is silent loss, not backpressure.

| | D10, the wire | D10b, the ring |
|---|---|---|
| Whose fault | the counterparty's | ours |
| `58=` text | `slow consumer` | `slow application`, a different constant so neither side is told the wrong one is at fault |
| The queue | discarded first, so the Logout has room | kept, because the socket is draining perfectly |
| `Block` offered | yes, for tests | no: spinning until an application thread drains makes the engine's progress depend on code it does not control, and the rule-4 gate cannot tell a spin that finishes from one that does not |
| Default capacity | `TX`, the caller's | `ring::DEFAULT_CAPACITY`, 4 MiB |

The signal is a defaulted method on `Dispatch`, `fn take_refusal(&mut self) -> bool { false }`,
because `deliver` is reached through the pure session layer's `Application::on_message`,
which cannot carry it. The engine asks immediately after one connection's turn, so a `true`
belongs to that connection. `InlineDispatch` takes the default and the branch folds away.

**Two costs, stated.** 4 MiB resident per ring; and an application that pauses longer than the
ring holds now drops the session. `[measured 2026-08-31]` 4 MiB gives **5.05–5.36 ms** of slack
over four runs on the §9 desktop, 22 550 messages, against 47.7 µs at the old 64 KiB. That is
above the 1.6–3.6 ms ADR-0011 derived by scaling, because the per-message cost goes 135 →
~230 ns once the buffer stops fitting in cache: a ring that fills more slowly gives the
application more time. Still true: no real application has ever stalled against this ring, so
the policy and the capacity come from one synthetic saturation run plus reasoning.

### D11 — TLS is a transport implementation, and the guarantee is stated per mode

Decided in [ADR-0005](decisions/ADR-0005-tls.md). It needs a decision because of one
collision: the codec parses in place at the I/O buffer, and encrypted bytes cannot be parsed
in place. Userspace TLS reintroduces exactly the copy ADR-0003 spent its length removing.
kTLS stays the `hft` steady state for the non-negotiable-1 guarantee it is the only path that
keeps, not for latency — measured slower than userspace on loopback
([ADR-0070](decisions/ADR-0070-ktls-stays-the-hft-steady-state-for-the-guarantee-not-for-latency.md)).

| Mode | When | Hot-path guarantee |
|---|---|---|
| Handshake, `rustls`, userspace | once per session, before any message flows | **allocation permitted**: a named, bounded carve-out from non-negotiable 1 |
| Steady state, **kTLS** | Linux, and a cipher suite the kernel carries | **met.** The kernel delivers plaintext into the read buffer; the D8 loop and parse-in-place survive unchanged |
| Steady state, userspace `rustls` | macOS, older kernels, unsupported suites | **not met, and the documentation says so.** One copy each way, and it allocates. A number measured in this mode is never quoted as the engine's |

`cargo build --no-default-features` produces a binary with no TLS code and no crypto
dependency (D5).

**Verified 2026-08-31, and it was load-bearing.** `ktls-core` *can* be driven from a plain
non-blocking socket with no async runtime: `strace -f` over 1000 round trips shows `recvfrom`
and `sendto` and nothing else. It costs four conditions: every read error goes to
`ktls_core::Context::handle_io_error`, the transport never reads the socket outside the
offload, the handshake hands over with an empty buffer, and `setup_ulp` needs an
`ESTABLISHED` socket ([reference/ktls-on-a-plain-socket.md](reference/ktls-on-a-plain-socket.md),
[ADR-0018](decisions/ADR-0018-ktls-on-a-plain-socket-answers-adr-0005.md),
`scripts/check-ktls-on-a-plain-socket.sh`).

**As built, `[2026-09-10]`.** `crates/engine/src/tls.rs` carries `TlsTransport`: a handshake
driven from inside `recv`/`send` so it yields instead of spinning, the kTLS handover, and the
userspace fallback. `serve_tls` and `serve_tls_with` are the front doors, and they take a
**certificate and a key rather than a `rustls::ServerConfig`** — `enable_secret_extraction` and
the TLS 1.3 / `AES-128-GCM` narrowing are load-bearing for the handover and are held by not
offering the caller the choice. All of it is behind `--features tls`, on Linux.

**The handshake is where the plan's Sửa 1 put it**, not in a pre-session stage:
`PendingSet<T, R, PRE>` holds one transport *type* from `admit` to `take`, so a stage that
changes the socket's type cannot be expressed. `presession.rs` was not modified then; it
has since gained one count, below, and still holds one transport type throughout.

**A handshake TLS refuses is said on the wire and counted, `[2026-09-23]`**
([ADR-0151](decisions/ADR-0151-a-tls-handshake-this-end-refuses-sends-its-alert-and-is-counted-and-a-peer-that-leaves-is-not.md),
plan `2026-09-23-phase-3-found-defects` row D3). When `rustls` returns an error — no cipher
suite in common, a peer's alert, bytes that are not TLS — `Handshake::pump` applies the
`discard`, calls `process_tls_records` again at most four times to take the alert `rustls`
queued (the unbuffered API hands a queued record out only on the *next* call, read from the
0.23.45 source and not documented), flushes once without waiting, and returns the new
`tls::Step::Refused`; `Step::Failed` keeps meaning a socket that failed or a peer that left.
`Transport::handshake_refused()` (defaulted `false`, the shape of `tls_mode()`) answers `true`
on a `TlsTransport` after `Refused`. The pre-session stage counts such a socket in the new
`presession::Progress::tls_refused` rather than `gone`; `serve_tls*` hands the count to
`Engine::note_tls_refused`, which raises `observe::EventKind::TlsHandshakeRefused { count }`
under `ConnId::MAX` once per turn that saw any, and `dial` raises the same event with
`count: 1` when a venue refuses. **A peer that connects and leaves stays in `gone` and raises
nothing**, so a health check is not a refusal. Guarded by
`tests/tls.rs::a_client_with_no_suite_in_common_is_sent_a_handshake_failure_alert` (the client
reads `AlertReceived(HandshakeFailure)`, not `EOF`), `…::a_peer_that_leaves_mid_handshake_is_not_a_refusal`,
`tests/tls_wire.rs::a_refused_handshake_is_an_event_not_silence` and
`tests/tls_initiator_wire.rs::an_initiator_refused_by_its_venue_says_so`. None of it is on the
hot path: it runs before any session exists, in the handshake carve-out above, and the event is
one `Copy` value in the fixed ring.

**The initiator side landed in Sửa 6, `[2026-09-13]`.** `connect_and_serve_tls` and
`connect_and_serve_tls_with` dial over `tls::ClientTls`/`tls::Client`, generic `tls.rs`
over the `Side` trait (`Server`/`Client` are its two implementors) so the acceptor's four
`tests/tls*.rs` did not change a line.
`dial` bounds the handshake by the connection's `LogonTimeout` (`0` = unbounded), because the
session-level timer does not watch a handshake that has produced no `Logon` yet. **The
initiator never resumes a TLS session — every redial is a full handshake**
([ADR-0063](decisions/ADR-0063-a-peers-key-update-is-the-second-named-carve-out-and-a-ticket-is-not-read.md)
decision 1): `tls::client_config` sets `Resumption::disabled()`, and the kernel-mode session
type for `Client` (`tls::side::Ticketless`) acknowledges a `NewSessionTicket` without reading
it, counted by `TlsTransport<Client>::tickets_ignored()`.

**A peer's TLS 1.3 KeyUpdate is handled — the second named carve-out from non-negotiable 1,
`[2026-09-13]`.** ktls-core 0.0.5 aborts a peer's KeyUpdate with an `InternalError` alert unless
its `tls13-key-update` feature is on; it is on now. The control-record buffer is taken at the
handover, sized to what ktls-core reserves (64 KiB, not the plan's original 16 KiB + 5).
**This engine and ktls-core allocate nothing handling a rekey; rustls's own key schedule
allocates two boxes of 184 bytes per direction rekeyed — four under `update_requested`, two
under `update_not_requested` — of this lock's rustls 0.23.44 and ring 0.17.14**, asserted
exactly, not `<=`, by `crates/engine/tests/tls_key_update.rs` — a ceiling would stay green if
the number silently changed either way. `Resumption::disabled()` alone removes none of the
sixteen allocations a client's two session tickets used to cost; the newtype above is the whole
of that fix. Full reasoning, sources and the alternatives rejected: ADR-0063.

**Published, `[measured 2026-09-14]`:** the TLS round trip — `w2w --tls` (`off`/`ktls`/
`userspace`, read back through `Engine::tls_mode`) on the §9 desktop, in §8 *The round trip
under TLS, measured*. **Still not built:** any timing of a rekey — nothing here has timed one.
**Question 2 — which kernel version and cipher suites are the floor — is answered at the level
measured, `[2026-09-14]`:** `TLS13_AES_128_GCM_SHA256`, the one suite this engine offers
(`crates/engine/src/tls.rs`, `offloadable_provider`), is taken by the kernel on
`7.0.0-31-generic`, and on the CI runner by the `tls` job; no other suite and no minimum kernel
was measured (§9's TLS row says what the CI half does and does not record).
**Question 6 — whether a session survives a TLS 1.3 key update under kTLS — is
answered at phase-1 level**: it does, for a peer that rekeys on its own the way rustls does; a
peer that never initiates one automatically, such as OpenSSL
([openssl#23566](https://github.com/openssl/openssl/issues/23566), open), never exercises this
path at all, and a peer that rekeys faster than RFC 8446's AES-GCM ceiling is the deployment's
own concern (ADR-0063). **Question 3 — what asserts which mode is live — is answered, as of
step 4b [merged 2026-09-12, `e728c16`]:** `Transport::tls_mode()` is read by
`serve_tls_with_offload`, a handshake that lands in userspace raises
`observe::EventKind::TlsFellBackToUserspace` regardless of `TlsRequireKernel`, and
`TlsRequireKernel=Y` refuses a deployment whose kernel cannot offload — both halves of
ADR-0060 decision 1, with `crates/engine/tests/tls_mode.rs` driving every arm. **The
read-back itself, `Engine::tls_mode(ConnId)`, is a method on `Engine`, not on anything a front
door hands back**: `serve_tls*` and `connect_and_serve_tls*` build the `Engine` inside
themselves and return only a `Shutdown` summary once the loop ends, so a deployment going
through one of those doors cannot call it on either role — only a caller that builds `Engine`
directly, bypassing the front doors, can read a connection's mode back this way. `tools/w2w`
does exactly that (its own `TLS_SEEN`/`tls:` line comes from the same read-back, not from a
front door). The
configuration-file keys landed too, as of step 4c [2026-09-12] and step 5c [2026-09-13]:**
`SocketUseSSL` and `TlsRequireKernel` for either role, `ServerCertificateFile`/
`ServerCertificateKeyFile` acceptor-only, `CertificationAuthoritiesFile`/
`ClientCertificateFile`/`ClientCertificateKeyFile` initiator-only — seven keys, all
`[DEFAULT]`-only — `docs/CONFIGURATION.md` §1 has the table, `crates/engine/src/settings.rs`
the code. **The §8 TLS row is filled, `[measured 2026-09-14]`**:
`scripts/check-no-kernel-sleep.sh` and `scripts/check-standard-gives-the-core-back.sh` both run
a kTLS arm (Sửa 6, step 6b), both green on the §9 desktop that day, and the round trip under
TLS is §8's second table, for `hft` and for `standard`. **On loopback, kTLS was the slower of
the two steady-state modes in both `hft` paths** — a measurement without a cause, and not by itself a
reversal of the table above, whose column is the hot-path guarantee rather than speed
(`STATUS.md` open item 84).

**One thing measured on the way, because the obvious guess about it is wrong.**
`[measured 2026-09-10]` only a `setup_ulp` refusal reaches the userspace fallback. A refusal from
`dangerous_into_kernel_connection` arrives *after* that call has consumed the rustls connection,
so there is nothing left to fall back to and the connection is dropped — the counterparty reads
`ECONNRESET` with no FIX-level explanation, because there is no session yet to carry one. That is
why the two questions are asked in that order, and it is proven by reversal 1 of
`crates/engine/tests/tls_wire.rs`.

### D13 — `Tick` counts milliseconds from year zero, not from the Unix epoch

SendingTime is `YYYYMMDD-HH:MM:SS[.sss]`, four year digits, so the wire can name any instant
from 0000 to 9999. Counted from 1970 in a `u64`, more than a fifth of that range does not
exist: a counterparty sending `52=19600101-00:00:00` would wrap the skew subtraction into a
difference of half a billion years, failing no check and crossing one, silently.

So `Tick` and every parsed SendingTime are **milliseconds since 0000-01-01T00:00:00Z**,
proleptic Gregorian. Every timestamp FIX can express is a non-negative `u64`, the skew is a
plain `abs_diff` that cannot wrap, and the session needs no signed arithmetic. The engine
converts once at the edge: `tick = unix_millis + clock::MILLIS_YEAR_ZERO_TO_EPOCH`.
`codec::TimestampCache` still takes Unix milliseconds because it is `no_std` and shared with
callers that have no session; bridging the two is the session's job.

### D14 — The message log is a second file, written by the journal's pattern, and it records refusals

The journal answers *"what did we send, numbered `seq`"*, which is what a ResendRequest needs.
It cannot answer the first question a desk asks in a dispute: *"at 10:32:07, what did we
receive, and what did we turn away?"* Inbound frames are kept as a number and no bytes
(ADR-0017), and a frame refused before the session saw it (a wrong `56=`, a duplicate identity,
garbage) disappears the moment the connection ends.

**It is a second file, not an extension of the journal, because the journal's key is `seq`.**
The three things this file exists for have no sequence number. `Journal` is also a `session`
trait, and a pre-session refusal is bytes the session never sees, so merging would put in
`session` something D1 forbids it to know. And `Durability::Fsync` blocks the engine thread
deliberately while a diagnostic must never, and one file cannot serve two durability policies
without branching on record kind inside the loop that must not branch. The full argument is
[reference/why-the-message-log-is-not-the-journal.md](reference/why-the-message-log-is-not-the-journal.md).

**The mechanism is ADR-0007's, unchanged**: one `Producer::push` per message per direction
into a ring, a writer thread that formats and appends, and losses dropped and counted rather
than waited for. The writer is allowed to allocate, for the reason `journal::Reader` is.
**It waits on an empty ring by the journal writer's rule** (`ring.rs` `Idle`): 1 024 empty
polls spun, then 1 ms sleeps, never woken by the engine. It `yield_now`ed until 2026-09-23,
which on an idle machine is a core burnt for nothing
([ADR-0150](decisions/ADR-0150-the-journal-writer-holds-the-largest-record-the-slot-allows-and-stops-only-on-a-record-no-message-can-be.md) decision 4).

**`NoLog` is the default and it compiles away.** `MessageLog::LOGS` is an associated
constant, so an engine never given a log carries no branch, no field and no cost.

**`[2026-09-23]` The writer masks a secret before it escapes.** Every value byte of a field
`engine::redact::MASKED` names becomes `*`, length kept, on the writer thread, in the writer's
own buffer — `9=`, the LENGTH fields and `10=` are left exactly as received, so a masked line
still frames but deliberately no longer checksums, the mark that it was altered. The engine
thread's `record` call is unchanged
([ADR-0110](decisions/ADR-0110-a-secret-is-masked-in-the-message-log-and-leaves-only-its-number-in-the-journal-file.md)
decision 3).

### D15 — An application can speak first, through two doors, and neither is told a sequence number

`[added 2026-09-05]` Until this decision **every application message this engine could send was
a reply.** `Handler::on_message` answered one inbound message; `Admin::Command` moved sequence
numbers. So there was no `ExecutionReport` for a fill that lands a second after the order, no
quote stream, no out-of-band `35=j`, and nothing to say to a counterparty that is connected and
quiet ([ADR-0048](decisions/ADR-0048-an-engine-that-can-speak-first-has-two-doors.md),
`STATUS.md` item 46).

**The primitive already existed.** `Session::send_application` takes a whole message, rewrites
`8=`, `9=`, `34=`, `52=` and `10=`, orders the rest from the generated tables, journals it and
spends the number. Its only caller was D4's `OUT_OF_BAND` block, which is `false` for
`InlineDispatch` — so it was reachable only by an application that had already moved to another
thread. What was missing was not a mechanism but a door.

| Door | Where the application is | For |
|---|---|---|
| `Handler::on_logon` | the engine thread, once per session | anything that must be said *as the session opens* — a subscription, a state dump, the two `35=B` the interop gate wants |
| `Sender` | any thread, any time | a fill that lands later, a quote stream, an out-of-band `35=j` |

**Neither door is told a sequence number or a clock**, so an application cannot get either
wrong: the session writes both on the way out and ignores whatever was there. That is `Reply`'s
existing rule extended to the message nobody asked for.

**`on_logon` is asked repeatedly and the engine owns the loop**: `nth = 0, 1, 2, …` until the
application answers `None`, each message sent as it comes, bounded by `MAX_ON_LOGON` (16) so a
handler that never stops cannot hold the engine thread. Reaching the bound emits
`EventKind::SpokeFirstToTheBound` rather than passing in silence.

**`[amended 2026-09-05, ADR-0054]` and all three are reachable from the front door.** Until
item 47 they were not: every one came off an `Engine`, and `serve` builds its engine inside
itself and returns only a `Shutdown`. So door 2 existed and a `serve` deployment could not open
it. `observe::Handles` is the same cell made **before** the engine, which then adopts it; the ten
`serve*`/`connect_and_serve*` entry points take one as their last argument. See D15's own row in
§3 and [ADR-0054](decisions/ADR-0054-the-handles-are-made-before-the-engine-and-the-engine-adopts-them.md).

**`Sender` rides the `Arc` that `Observer` and `Admin` already ride**, with a third capability
and its own fixed queue — `ORIGIN_CAPACITY` (64) slots of `ORIGIN_LEN` (512) bytes, filled once.
It copies `Commands` where `Commands` was right: `send` answers `false` at the call when the
queue is full or the message is too long, so a loss is never silent; the engine drains with
`try_lock` and never `lock`; and a relaxed load comes before the lock is attempted, so an engine
nobody sends through pays one load per turn. `Sender::drains()` is what keeps that falsifiable —
`[measured 2026-09-05]` removing the load makes it read 20 over 20 turns instead of 0.

**The gate is deliberately not one of this repository's own runners.** `scripts/interop.sh`'s
acceptor role has two steps, `news` and `resend`, that were red **by design** because the
acceptor could not originate; the tool said so in a comment. Both are green now, and the
exemption is gone from the code, so a red there is a red. `[measured 2026-09-05]` the two roles
pointed at each other read `PASS 7/7`, up from 5/7 before this work and 6/7 with the door open
but the message malformed — see
[reference/a-message-on-the-wire-is-not-a-message-delivered.md](reference/a-message-on-the-wire-is-not-a-message-delivered.md).

**What it costs**, and the ADR prices all of it: `crates/session` gains an
`Application::on_logon` with a default body that nothing in that crate calls, plus three `Config`
getters; there is a second bounded queue and a fifth buffer ceiling; and `MAX_ON_LOGON` is a
number with no measurement behind it, labelled as a guard rather than a knob.

**What it costs the engine thread is not measured yet**: one ring copy per message per
direction, about 1.7 ns/byte, so about 340 ns for a 200-byte message and 680 ns for a
request/reply pair, `[unproven]`, arithmetic from §6's dispatch row. What is measured is that
it allocates nothing on that thread: `benches/alloc.rs` cases `log-record`, `log-idle` and
`log-busy`.

**`OUT` means queued, not sent, and the gap is counted.** The line is written when a message
reaches the outbound buffer, and a dying socket discards that buffer.
`EventKind::MessageLogUnsent { bytes }` says how much of that connection's tail the file is
wrong about.

### D16 — `Encoding` is a trait, each encoding has its own view, and `Session` is generic over tag=value

`[added 2026-09-19]` Phase 2 brings a second wire format (SBE) and a second dictionary
(FIXT 1.1 / FIX 5.0 SP2) to a codec, a session and an engine written for FIX 4.4 tag=value
alone. [ADR-0079](decisions/ADR-0079-one-view-per-encoding-and-one-trait-over-them.md) decides
how they share one engine;
[ADR-0082](decisions/ADR-0082-the-session-is-generic-over-tag-value-encodings-and-the-boundary-to-sbe-is-the-session-not-the-trait.md)
decides where the session's genericity stops. The plan is
[phase-2-fixt-and-sbe](plans/2026-09-19-phase-2-fixt-and-sbe.md), PR A.

**Each encoding keeps its own view type** (ADR-0079 decision 1). `MessageView` is unchanged —
its name, its 24 bytes, `Copy`, its API — and nothing in `codec`'s existing public API is renamed
or re-typed by phase 2; `crates/codec/tests/encoding.rs::api_unchanged` holds the sizes and the
old call signatures. SBE gets its own `Copy` view, generated per schema and sized to stay
≤ 24 bytes, when step 9 (§7) lands.

**One trait over them, `Encoding`** (ADR-0079 decision 2), in `crates/codec/src/encoding.rs`,
as built rather than as the plan first sketched it:

```rust
pub trait Encoding {
    type View<'a>: Copy;                              // MessageView<'a, N> for tag=value
    type Field: Copy;                                 // u32 — a tag — for tag=value
    type Dict: Dictionary;                            // codec::Dictionary only: codec has no dependencies
    type Scratch: Default;                            // FieldIndex<N> for tag=value
    type Template<const P: usize, const S: usize>;    // the D9 parts list; P, S the caller's
    type ParseError: Copy;
    type EncodeError: Copy;

    fn parse(buf: &[u8], scratch: &mut Self::Scratch, v: Validation)
        -> Result<Parsed, Self::ParseError>;
    fn view<'a>(scratch: &'a Self::Scratch, buf: &'a [u8]) -> Self::View<'a>;
    fn field<'a>(view: Self::View<'a>, f: Self::Field) -> Option<&'a [u8]>;
    fn session_fields<'a>(view: Self::View<'a>) -> Option<SessionFields<'a>>;
    fn encode<const P: usize, const S: usize>(
        t: &Self::Template<P, S>, out: &mut [u8], slots: &[(Self::Field, &[u8])],
    ) -> Result<Range<usize>, Self::EncodeError>;
}

pub struct TagValue<D, const N: usize>(PhantomData<D>);   // impl Encoding for TagValue<D: Dictionary, N>
pub type Fix44TagValue = TagValue<Fix44, 64>;              // lives in `dict`, beside `Fix44`
```

**The dictionary rides the encoding** (ADR-0080 decision 1). `Encoding::Dict` is the table the
session asks about tags, and it is an associated type rather than a second parameter of
`Session`, so changing dictionary changes one type and nothing else. Two things follow, and both
are load-bearing:

* **`Encoding::Dict` requires only `codec::Dictionary`, never `dict::Tables`.** `codec` has zero
  dependencies and cannot see `dict`. The stronger bound lives on `Session` — `where E::Dict:
  Tables` — which is where the session's extra questions (required fields, enum values, field
  types, which layer defines a tag) are actually asked.
* **A FIXT 1.1 session is that one type substitution.** `TagValue<Fix44, 256>` becomes
  `TagValue<Fixt11Fix50Sp2Tables, 256>` and the state machine is untouched; the differences at
  the boundary are the dictionary's, not the machine's. The four that are the session's own —
  `1137` required on a FIXT Logon, `1137` on the way out, `1128` outside the FIX 5.0 family, and
  an admin body checked against the transport file alone — are in
  [SESSION-BEHAVIOUR.md](SESSION-BEHAVIOUR.md) §5b with what guards each.

`[measured 2026-09-19]` the second table is **not** "about twice the size", which is what
ADR-0080 predicted: `ALLOWED` is a bitset over `0..=max_tag`, SP2's highest tag is 50002 against
FIX 4.4's 956, and the generated file goes 156 KB to 4.0 MB — 25.6×, on 6.6× the fields. Nothing
on the hot path got slower and no allocation was added; the cost is build time and binary size,
and it is written up in
[a-bitset-keyed-by-tag-scales-with-the-highest-tag](reference/a-bitset-keyed-by-tag-scales-with-the-highest-tag-not-the-field-count.md).

Four shared operations — parse, view, read a field, encode — plus `session_fields`, the one hook
the session gets; the `Option` on it ("do you carry a FIX session header at all") is the whole
provision this trait makes for an encoding that does not. **Static dispatch only**: every method
is an associated function with no receiver, every one is `#[inline]`, and `TagValue`'s forward
unchanged to `parse_into`, `FieldIndex::view`, `MessageView::get` and `Template::encode_with`. The
trait adds a name, not a branch, which is why `crates/codec/benches/alloc.rs` counts
`parse via Encoding` beside the direct parse and both must read 0. No `dyn Encoding` on any path
a message takes. Three shapes follow from "forwards unchanged": `parse` returns the existing
`Parsed` and `view` is its own method, so a view can still be built over the index after
`ParseError::BadTag` (`14a_BadField`); `encode` is one call over an immutable D9 template, not
`patch` then `encode`; and the two error types are the two `codec` already returns. Repeating
groups are deliberately absent from the trait — `GroupData` is a tag=value shape. ADR-0079
decision 5 is the guard that this costs the tag=value path nothing: `benches/parse.rs`,
`serialize.rs` and `alloc.rs` inside the ADR-0031 band on the §9 machine, same commit, before any
SBE line; a band miss is a stop.

`[measured 2026-09-22, boot D step D4]` **The guard was finally run on the §9 machine, and it
holds: 22 cases of 22 in band, over n = 20 per arm, one rotation against a shared control.** The
`parse` and `serialize` cases move by **0.35% or less** — the largest is `encode ExecutionReport
(template)` at −0.31%, a `serialize.rs` case — and the three counting benches per arm (`codec`, `session`, `engine`) read 0 on every case, which is what
decision 5 asks for outright. The engine turn is a different sentence: **+4.14…+5.11% by session
count, +3.56…+4.25% by ring size, +2.16% on the admin turn**, inside the 1.10 band and not free.
The `validate` cases go the other way, 0.5–5.9% faster.
[reference/measured-costs.md](reference/measured-costs.md) *Boot D … A-desk*.

**`Session` is generic over tag=value encodings; `Session<Sbe<S>>` does not compile (ADR-0082).**
`Session<E: Encoding, R: Role, const APP: usize>`: the index capacity `N` that used to be a
parameter now rides in `E`, and the state machine does not change with `E` — a FIXT 1.1 /
FIX 5.0 SP2 session is this machine with a different `E::Dict`. But `impl Session` needs more of
`E` than the trait gives, and says so in six bounds rather than leaving it to a compile error
four crates away. In the words of the rustdoc on that `impl` (`crates/session/src/lib.rs`, *What
the session needs of an `Encoding`, beyond the trait*):

```rust
impl<E, R: Role, const N: usize, const APP: usize> Session<E, R, APP>
where
    E: for<'a> Encoding<
            View<'a> = MessageView<'a, N>,
            Scratch = FieldIndex<N>,
            Template<24, 320> = Template<24, 320>,
            Field = u32,
            ParseError = ParseError,
        >,
    E::Dict: Tables,
```

- `View<'a> = MessageView<'a, N>`: the validation pass walks a message in wire order
  (`MessageView::field_at`, `len`, `find_from`, `group`) and the trait exposes only `field`.
  **`N` is not a parameter of `Session` — it is pinned by this binding and by the one below**, so
  it stays the caller's choice (`CLAUDE.md` §6) while `Session<E, R, APP>` keeps three
  parameters.
- `Scratch = FieldIndex<N>`: the same `N`, and what `Encoding::view` is handed.
- `Template<24, 320> = Template<24, 320>`: the seven outbound skeletons are laid out by `codec`'s
  `TemplateBuilder`, because the trait offers no way to build one (`out::Outbound::new`).
- `Field = u32`: the slots this layer fills are named by FIX tag, and the `tag` module is the list
  of them.
- `ParseError = ParseError`: `judge` answers `BadTag` differently from every other failure —
  `14a_BadField.def` against `2d_GarbledMessage.def` — so it matches on the variants, not on an
  opaque `Copy` value.
- `E::Dict: Tables`: the `373=` questions live in `dict`, and `Encoding::Dict` may only require
  `codec::Dictionary` because `codec` has zero dependencies (ADR-0080 decision 1). `Tables` is a
  bound on `Session`, never on the trait.

Every tag=value encoding satisfies all of them, FIXT 1.1 included. An encoding with its own view
and its own skeleton — SBE — satisfies none of them and gets no session, which is what ADR-0078
decided. The boundary to SBE is therefore **a type error at the session's `where` clause, not a
trait method**, and the trait does not grow to erase it: an ordered walk, a builder and an error
probe on `Encoding` would be tag=value shapes under generic names, and the type-level harness for
"SBE inside a FIXT session" that ADR-0078 decision 3 declined for lack of any venue (ADR-0082,
*The alternative, not chosen*). The FIX Session Layer specification draws the same line — a valid
FIX message is a tagvalue string (§3.1.4) and both ends validate tagvalue (§4.5) — and the
encoding-independent session is FIXP, phase 3, a second `impl` beside `Session` (ADR-0078
decision 2). ADR-0082 decision 2 amends ADR-0079 decision 3 accordingly, and decision 4 follows:
`SbeTables<S>` implements `codec::Dictionary` only, and `sbe` does not depend on `dict`. The 59
definitions run against `Session<TagValue<Fix44, 256>, _>` exactly as before, in process and over
a socket, which is what proves the generalisation moved nothing.

**Generic sessions mean generic front doors, and the doors kept their shape.** `Connection` and
`Engine` take a trailing `E: Encoding = TagValue<Fix44, N>`, as do the six engine aliases
(`TcpAcceptorEngine`, `AcceptorEngineOver`, `HftAcceptorEngine`, `StandardAcceptorEngine`,
`TcpInitiatorEngine`, `InitiatorEngineOver`); `AcceptorFix44<N = 256, APP>` and
`InitiatorFix44<N = 256, APP>` name `Session<TagValue<Fix44, N>, Acceptor | Initiator, APP>`,
which is what `Session<Acceptor, N, APP>` meant before. Rust allows no default on a function's
type parameter and ADR-0047 refuses a partial turbofish, so the default sits on `Engine` and the
aliases and **every `serve*` signature is unchanged** — `crates/library/examples/acceptor.rs`
compiles untouched. `engine` re-exports `TagValue` and `Fix44` so the default is a type a caller
can name and therefore substitute.

**`[added 2026-09-19, step 9]` `Sbe<S>: Encoding`, and the boundary above is now code, not a
prediction.** `crates/sbe` implements the trait for any `S: Schema` (ADR-0081):
`View<'a> = SbeView<'a>`, 24 bytes and `Copy`, the same shape discipline as `MessageView`;
`Field = FieldId`, a template-relative field id, never a FIX tag; `Dict = SbeTables<S>`, which
implements `codec::Dictionary` **only** — `sbe` does not depend on `dict`, so it cannot implement
`dict::Tables`, and ADR-0082 decision 4 says it should not try; `session_fields` returns `None`
unconditionally, spending the trait's one hook for "does this encoding carry a FIX session
header" on the answer "no". `encode` writes the root block only, from a pre-built
`SbeTemplate<S, N>` (D9's shape, reused because the trait's `Template<P, S>` offers no group or
`varData` slots of its own); groups and `varData` are written after it, in schema order, through
`sbe`'s native `MessageWriter` / `GroupWriter` / `EntryWriter`, not through anything `Encoding`
declares. **`Session<Sbe<S>>` does not compile**, and that sentence has a proof rather than an
assertion behind it: `crates/library/src/lib.rs`'s `sbe` module carries a `compile_fail` doctest
naming exactly why — `Sbe<S>`'s `View`/`Scratch` are `SbeView`/`MessageHeader`, never
`MessageView<'_, N>`/`FieldIndex<N>` for any `N`, and `SbeTables<S>` has no `dict::Tables` impl —
checked on every build of the `sbe` feature. The `encoding` feature on `sbe` (on by default) is
what gates the module that implements `Encoding`, and with it the crate's only dependency,
`codec`; `--no-default-features` leaves `sbe` at zero dependencies — header, view, group and
`varData` only, the runtime `sbe-gen`'s tables are read by.

## 5. Non-goals for v1

The full list is [PRD.md §5](PRD.md); this is the subset that shapes the architecture.

- **FIX 5.0 / FIXT 1.1**: phase 2, together with SBE, because SBE messages are versioned by
  `ApplVerID`.
- **SBE, FAST, FIXML**: phase 2, and an encoding ADR comes first, because `MessageView`
  presupposes tags on the wire and SBE has none.
- **Kernel bypass**: not before an ordinary TCP path has been measured and found to be the
  limit, which §8 puts at 10–20 µs. If it ever happens: Onload first (the D8 loop and the
  socket API survive unchanged), `ef_vi` second as an `impl Transport` behind a D5-style flag,
  DPDK never (no TCP stack). Plaintext only, so it and D11 exclude each other. STATUS item 14.
- **SIMD delimiter scan and checksum**: declined by
  [ADR-0045](decisions/ADR-0045-parse-is-under-one-percent-of-the-wire-and-simd-is-declined.md),
  because parse is 0.62% of a round trip. `matthart1983/nanofix` has SIMD and parses 4–6×
  slower, because layout beat it.
- **Clustering, HA, replication. Metrics dashboards and web UIs. Matching engine, order book,
  risk.** This is a protocol engine.

**No longer a non-goal:** the initiator. [ADR-0004](decisions/ADR-0004-bidirectional-engine.md)
moved it into phase 1 on the finding that the two roles differ by about one enum's worth of
behaviour, and that a session core which cannot invert is a rewrite later.

## 6. Gates

Each is a committed benchmark or test, named. **A target without a runnable gate is a wish.**
Timing gates are judged against this machine's own recorded baseline, never an absolute
number ([ADR-0016](decisions/ADR-0016-per-machine-baselines-replace-absolute-targets.md), see
below).

### Correctness

| Gate | Target | Proven by |
|---|---|---|
| Session conformance, acceptor, in process | **59 / 59** | `cargo test -p fixbolt-session --test score` `[measured 2026-08-29]` |
| Session conformance, acceptor, through a real socket | **59 / 59 on every machine** | `cargo test -p fixbolt-engine --test wire`: kernel sockets, the real framer, the real session, the real application; only the clock is injected, because every `I` line in the corpus carries a fixed instant. `[measured 2026-08-30]` 59 / 59 on the M5 and on Linux. It read 39 / 59 on Linux until the harness's client socket was given `TCP_NODELAY`. `[changed 2026-09-20]` **no timing bound decides this gate any more** ([ADR-0087](decisions/ADR-0087-a-socket-harness-settles-on-counted-records-and-the-clock-waits-for-the-engine.md) decision 1): a step settles on two counted facts — the engine has consumed every framable `I` line the harness sent it, and the harness has read every record the engine wrote — and a `Tick` may not move the simulated clock until the first of those holds (decision 2). `STEP_QUIET` = 1 ms is belt and braces; `STEP_LIFELINE` = 5 s is reported, counted and asserted zero, and is never a pass (decision 3) |
| Session conformance, FIXT 1.1 / FIX 5.0 SP2, in process | **179 / 180**, one divergence pinned by content | `cargo test -p fixbolt-session --features fix50sp2 --test score_fixt`. Three corpora of 60 — `fix50` 59/60, `fix50sp1` 60/60, `fix50sp2` 60/60. The one failing file is asserted by name, line and the engine's actual wire bytes, and **180 / 180 is not reachable with one SP2 table**: tag 336 carries 0 enumerated values in `FIX50.xml` and 7 in `FIX50SP2.xml`, so scoring the `fix50` corpus against the SP2 table is stricter than QuickFIX was ([ADR-0084](decisions/ADR-0084-a-session-message-is-checked-against-the-layer-that-defines-it-a-members-value-waits-for-the-count-and-fix50-is-its-own-oracle.md) decision 3). No fixture was edited and no exclusion list exists |
| Session conformance, FIXT 1.1 / FIX 5.0 SP2, through a real socket | **60 / 60** | `cargo test -p fixbolt-engine --features fix50sp2 --test wire_fixt`: the `fix50sp2` corpus through kernel sockets, the same harness as the 59. `[measured 2026-09-19]` it reached 60 / 60 first run, because `TCP_NODELAY` and the bounded-turns pump were copied from `tests/wire.rs` rather than rediscovered |
| Session conformance, acceptor, in `standard` mode | **59 / 59** with the engine blocking between steps | the same wire test, second case. The only place the corpus meets `standard`. It proves the protocol, not the wiring: `[measured 2026-08-30]` with `Block` made to ignore readiness the run took 3.30 s against a 3.28 s baseline — under the wall-time settle of the day, one block satisfied the criterion whether it returned on data or on its 5 ms timeout, so the run was `steps × 5 ms` either way. `[changed 2026-09-20]` that settle is gone: the step now ends on the two counted facts [ADR-0087](decisions/ADR-0087-a-socket-harness-settles-on-counted-records-and-the-clock-waits-for-the-engine.md) decision 1 names, and **the timing reversal has not been re-run against it** — the wiring is still proven elsewhere on purpose (`tests/standard.rs`, `scripts/check-standard-gives-the-core-back.sh`), which is why nothing here depends on it. This case also asserts `lifeline hit: 0` |
| Both socket corpora survive concurrent copies of themselves | **`0 red in 16`** for each of the two test binaries, with the `lifeline hit:` count printed on the same line | `scripts/check-socket-corpus-under-contention.sh 2 8`, one step of the `gates` job: it runs the **prebuilt** `wire` and `wire_fixt` binaries `rounds × copies` times, `copies` at a time, and exits non-zero on any red, printing `N red in M` ([ADR-0087](decisions/ADR-0087-a-socket-harness-settles-on-counted-records-and-the-clock-waits-for-the-engine.md) decision 5). It is a **count, not a measurement** — non-negotiable 10 is about timing numbers and does not apply. `[measured 2026-09-20]` `3 10` → `0 red in 30` on both targets on the desk. **Its reversal is owed**: the red it was built for is not reproducible on Linux at all, because the lateness lives in the loopback stack, and the macOS run that has it is still outstanding ([ADR-0091](decisions/ADR-0091-the-socket-harness-race-is-the-loopback-stacks-not-the-schedulers-and-its-reversal-runs-on-macos.md), `docs/CONFORMANCE.md` §9) |
| Session conformance, acceptor, through the shard runtime | **59 / 59 through one shard and through two** | `cargo test -p fixbolt-engine --features affinity --test shard_wire`. `[measured 2026-08-31]` it read 57 through two; `[measured 2026-09-01]` 59 ([ADR-0020](decisions/ADR-0020-a-pre-session-stage-owns-the-socket-until-logon.md)). The test also counts how the pre-session stage disposed of every socket, because a dropped connection is indistinguishable from a refused duplicate ([ADR-0022](decisions/ADR-0022-the-pre-session-stage-enforces-two-definitions.md)) |
| Session conformance, initiator, mirrored corpus | **10 / 50** `[measured 2026-09-02]`, ceiling of 45 in doubt, and the harness's own drive count asserted beside it | `cargo test -p fixbolt-session --test mirror`. The secondary gate. It asserted `passed == 0` for three days, and a gate pinned at a constant reports nothing about the code under it. The jump to 10 was two real defects: a session that said goodbye first answered the acknowledgement with a third Logout, and `begin_logout(b"")` wrote an empty `58=`. `Report::driven` counts every time the harness played the operator, by MsgType, because a score a harness can raise by driving harder is not a score. Reversals: `make_receivable` neutered takes it to 0 / 50; letting the acceptor corpus be driven makes `tests/score.rs` panic |
| Every `373` code the corpus asks for is produced | **12 / 12**, read from the corpus's `E` lines | `crates/session/tests/score.rs`. The file count cannot say this: `14a_BadField.def` holds four cases, and a session answering all four with one code still passes the file |
| The session rules the corpus cannot tell apart | each has its own test | `crates/session/tests/logon.rs`, `reject.rs`, `heartbeat.rs`, `resend.rs`. Examples: deleting the "first message must be a Logon" check leaves the score unchanged, because `1e_NotLogonMessage.def` also carries a wrong `56=`; stamping `52=` from a constant leaves it unchanged, because `52` is matched by shape; all three heartbeat thresholds are invisible to whole-interval ticks; every file that opens a gap ends before opening a second one |
| The conformance runner can tell right from wrong | a fake that replays each file's own expected output scores **59 / 59** | `crates/conformance/tests/fix44.rs`. Without it, 0 / 59 is also what a broken runner reports |
| The journal keeps what a resend needs, under each policy | no journal fills over everything; `MemJournal` and `FileJournal` replay; a message longer than a slot is refused, not truncated | `crates/engine/tests/journal.rs`. Reversal: making `put` keep nothing turns four tests red **and drops the acceptance score**, which proves the score depends on the journal |
| A resend larger than the transmit buffer does not end the session | 100 messages of ~200 bytes, `TX` 8 KiB, `Backpressure::Disconnect`: all 100 come back with `43=Y`, in order, no `58=slow consumer` | `crates/engine/tests/backpressure.rs::a_resend_larger_than_tx_does_not_end_the_session` (ADR-0046). Reversal: `with_resend_batch(10_000)` ends the session on turn 1, **and `--test score` stays 59 / 59**, which says the corpus is blind to this |
| A resend past the ring is counted and reaches an operator | 20 orders through an 8-slot ring, `7=2 16=0`: one `ResendBeyondJournal { filled: 12, oldest: Some(14) }`, `events_lost() == 0` | `crates/engine/tests/events.rs::a_resend_past_the_ring_is_an_event_with_the_numbers`. One event per turn that changed, not one per message |
| A journal on disk says when its session was last alive | the instant survives the process; the latest mark answers; a file with no mark reads as before | `crates/engine/tests/on_disk.rs`, six tests, one through a real socket and the serving loop. Reversal: `mark_active` writing nothing turns three red ([a-reversal-that-must-not-compile](reference/a-reversal-that-must-not-compile.md)) |
| The serving loop does not require the journal to have a `Default` | putting the bound back **must not compile** | `crates/engine/tests/on_disk.rs::serving`, which uses a `FileJournal`. `[measured 2026-09-02]` restoring `J::default()` in `pump` gives `error[E0599]`. No runnable test can hold this claim |
| Counterparties come out of a configuration file | two named only in a file both log on through a real socket; one the file does not name gets nothing; one whose window closed is refused while one open now is served | `crates/engine/tests/settings.rs` (30 tests) and `tests/settings_wire.rs` (4, one a `#[should_panic]` control proving the harness can tell a closed socket from a hung one). Reversals: ignoring an unknown key turns 1 red, accepting a file with no `[SESSION]` turns 2, letting `[DEFAULT]` win over `[SESSION]` turns 1, dropping the schedule turns 5. One reversal not in the plan, keeping only the first counterparty, left all three wire tests green, because an unserved identity and a closed window are the same silence; the fix asserts the registry's length ([two-time-rules-share-one-observable](reference/two-time-rules-share-one-observable.md)) |
| An initiator comes back after its counterparty hangs up | the loop dials again; a policy that says stop opens no socket | `crates/engine/tests/reconnect_wire.rs`, over a real listener that answers one Logon and closes. Two orthogonal reversals. `[measured 2026-09-02]` the first originally made the suite hang, so the control now runs on a thread with a deadline ([a-reversal-can-fail-by-hanging](reference/a-reversal-can-fail-by-hanging.md)) |
| The reconnect loop, against a real `libquickfix` that dies and comes back | **6 / 6 × 3 scenarios.** `SIGKILL`: nobody said goodbye · it came back unprompted · **at one past the number it last sent** · the venue's next messages were *delivered to the application*, not gap-filled · no `35=2`, `141=Y` or `MsgSeqNum too low` anywhere · and the journal is never behind what an `Observer` saw spent. `SIGTERM`: the same five, with *the goodbye was answered* in place of the first. `HeartBtInt=1` with a pause before the kill: the `SIGKILL` five again, with an administrative message guaranteed after the last application one | `scripts/interop.sh` §4d/4e/4e-bis and the blocking `interop` CI job. `STATUS.md` items 38, 48, 47 — until 2026-09-05 every test of `connect_and_serve` was this repository's own reading, which ADR-0043's own *Consequences* said out loud. The counterparty's `FileStore` is what remembers the numbering, and its three `ResetOn*` are `N`: under the `Y` the other two directions use, both ends restart at 1 and a broken engine passes. `[measured 2026-09-05]` the first run refused the resumed session and named the cause in English ([a-journal-holds-messages-not-numbering](reference/a-journal-holds-messages-not-numbering.md)); the `SIGTERM` scenario asserted **3 / 3** with a pinned `known_gap` until ADR-0053 closed it, and the sixth assertion needs ADR-0054's handles to exist at all |
| The reconnect ladder, no I/O, no clock | doubling to a ceiling that holds; `logged_on` resets it; a shut venue outranks it | `crates/engine/tests/reconnect.rs`, 8 cases ([ADR-0043](decisions/ADR-0043-backoff-without-jitter-and-a-reconnect-asks-recovery-every-time.md)). **Every case is invented**: no corpus here covers reconnect. The ordering reversal was a no-op until the assertion moved to an instant where the two orderings disagree ([a-reversal-needs-an-input-where-the-answers-differ](reference/a-reversal-needs-an-input-where-the-answers-differ.md)) |
| The initiator, against a real `libquickfix` | **7 / 7**: logon · application messages in · an unprompted heartbeat · a TestRequest with this end's own `112=` · a ResendRequest answered by replay at the numbers asked for · a gap this end opens and gap-fills · logout | `scripts/interop.sh` and the blocking `interop` CI job. Phase 1 exit criterion 4 ([ADR-0042](decisions/ADR-0042-a-second-implementation-is-the-only-independent-opinion.md)). Builds QuickFIX at the same commit `fetch-quickfix-assets.sh` pins and refuses to run if the pins drift. Reads the transcript, not the exit code. `[measured 2026-09-02]` its first run found the initiator answering a Logon with a Logon. Reversal 2 was a no-op until the resend step named the sequence numbers it wanted ([a-resend-answer-has-two-legal-shapes](reference/a-resend-answer-has-two-legal-shapes.md)) |
| The acceptor can originate, and a second implementation says so | `news` and `resend` in the acceptor role of `scripts/interop.sh`, **red by design until 2026-09-05** and green now, with the exemption removed from `tools/interop` so a red is a red | D15, [ADR-0048](decisions/ADR-0048-an-engine-that-can-speak-first-has-two-doors.md). `[measured 2026-09-05]` this repository's two roles pointed at each other read **PASS 7/7**, from 5/7. The step in between, 6/7, is its own lesson: the door was open and the message was still refused, for a required group nobody had read off the XML ([a-message-on-the-wire-is-not-a-message-delivered](reference/a-message-on-the-wire-is-not-a-message-delivered.md)) |
| The acceptor, against a real `libquickfix` | **7 / 7**: logon with `141=Y` echoed · two `35=D` answered by two `35=8` paired on `11=` · an unprompted heartbeat with no `112=` · a TestRequest · a ResendRequest answered by replay at the two numbers asked for · a gap the counterparty opens · logout | the same script and CI job. **The differentiator's first independent opinion**: until 2026-09-04 the acceptor's whole evidence was 59 `.def` files read by this repository's own runner. Under test is the whole stack: `fixbolt::serve`, the poller, the pre-session table, the settings file, the library `Handler`. `[measured 2026-09-04]` its first run was red on `gapfill`, and the red was the test's ([a-gap-fill-can-swallow-the-question](reference/a-gap-fill-can-swallow-the-question.md)) |
| Both roles, against a real QuickFIX/J, plaintext and TLS | **four arms, 7 / 7 each**, plus `shutdown` and `clean` on every arm and `kernel` on the two TLS arms | `scripts/interop-qfj.sh` and the blocking `interop-qfj` CI job ([ADR-0097](decisions/ADR-0097-phase-3-makes-the-engine-dependable-by-a-stranger-and-fixp-waits-on-a-running-oracle.md) exit criterion 6, [ADR-0130](decisions/ADR-0130-a-jvm-enters-ci-as-a-second-oracle-quickfixj-by-pinned-jar-and-our-own-judge.md)). A second family entirely from `libquickfix` above — a JVM, judged by this repository's own `tools/interop-qfj/Judge.java` on raw wire bytes. The initiator arms run the engine's real door (`--role dial`), not the pure session `libquickfix`'s initiator direction drives, because TLS on that side exists only inside the engine. `kernel` reads `/proc/net/tls_stat`'s `TlsTxSw`/`TlsRxSw` rising and 0 `TlsFellBackToUserspace` events — the kernel's own count, which the engine cannot write. See `docs/CONFORMANCE.md` §10 for the command, the machine, the JDK and the CI run id |
| Repeating groups, read | every group found, to depth 4, at all **731** positions | `crates/codec/tests/groups.rs` |
| Repeating groups, written | parse → encode byte-identical at all **357** top-level positions, all 59 counters, depth 4 | `crates/codec/tests/group_roundtrip.rs` |
| Every tag number matches another implementation | **912 / 912** against QuickFIX's `FixFieldNumbers.h`, and 5 168 names FIX 4.4 does not define are refused | `crates/dict/tests/interop_quickfix_fields.rs`. The negative half stops `is_defined_tag` being `true` for everything |
| Every field type matches another implementation | **898 / 912**, 14 differences each named by tag | same test. QuickFIX's `FixFields.h` is shared across versions; the XML is the source of truth |
| Every (message, tag) pair matches another implementation | **12 524 / 12 524**, checked as 84 816 answers | `crates/dict/tests/interop_quickfix_messages.rs` |
| Every enum value is one QuickFIX also knows | **245 / 245** fields, **1 708 / 1 708** values | `crates/dict/tests/enums.rs`. One-directional: QuickFIX lists every version's values, so it can confirm but never forbid |
| What each of the 23 field types accepts | at least one accepted and one refused value per type | `crates/dict/tests/field_types.rs`. **Invented cases**: the corpus supplies two |
| In-group field order matches another implementation | delimiter exact on all **730** groups; QuickFIX's `message_order` an exact subsequence of this crate's on all 730 | `crates/dict/tests/interop_quickfix_order.rs`. Exists because the round-trip test reads the same table the encoder does |
| `parse_into` never panics on hostile input | `[measured 2026-08-28]` 304 230 294 executions, 0 crashes | `fuzz/fuzz_targets/parse.rs`, `cargo +nightly fuzz run parse` |
| `docs/CONFIGURATION.md` §1 says what the parser does `[2026-09-13]` | **seven probes** — probe 7 holds a bound a *Values* cell names (`positive`, `non-negative`, `` `a`–`b` ``) against the parser (≥ 5 cells) — and probe 3 also reads every call site above `mod doc_table` so the function that reads a key agrees with the reader the probe was told (≥ 25 call sites), and its reverse search tries the 22 boolean spellings of YAML 1.1 (`yes`, `true`, `on`, …) against every listed cell — a spelling of three or more characters outside that list (`always`) is still not seen; *Meaning* and notes cells are prose, read by a person | `cargo test -p fixbolt-engine --lib doc_table`, and with `--features tls` |
| Every feature set of every workspace crate, to depth two, builds under clippy and under rustdoc `[2026-09-13]` | `clippy --all-targets -- -D warnings` and `doc --no-deps` under `RUSTDOCFLAGS="-D warnings"` ([ADR-0066](decisions/ADR-0066-the-rustdoc-gate-denies-every-warning.md)) both green, per crate, over no features, each feature alone and each pair — with cargo-hack counting `default` as a feature, so ten sets for `fixbolt-engine` and for `tools/w2w` — **and then once more under `--all-features`, which is not a member of that powerset** `[corrected 2026-09-13]` | `cargo hack clippy --workspace --all-targets --feature-powerset --depth 2 --keep-going -- -D warnings` and `cargo hack doc --workspace --no-deps --feature-powerset --depth 2 --keep-going` under those `RUSTDOCFLAGS`, then `cargo clippy --workspace --all-targets --all-features -- -D warnings` and `cargo doc --workspace --no-deps --all-features` under the same flags, the `feature-sets` CI job ([ADR-0065](decisions/ADR-0065-every-feature-set-to-depth-two-is-built-linted-and-documented.md)). Depth stops at two because every defect of this class found here needed exactly one feature on and one off — `tls` off (item 61), `standard` off with `affinity` on (items 78, 79); a defect needing three named features that does not also appear with every feature on would pass this gate green today |

### Allocation

| Gate | Target | Proven by |
|---|---|---|
| Allocations on the hot path, codec | **0** | `crates/codec/benches/alloc.rs`, counting allocator. `[2026-09-19]` the `parse via Encoding` case counts the same parse through `Encoding::parse` beside the direct `parse_into` case, and each asserts its own path is live (D16) |
| Allocations on the hot path, sbe `[2026-09-19]` | **0** on four paths: parse nested (header+root), field (through `Encoding::field`), walk nested group + `varData`, encode NewOrderSingle (through `Encoding::encode` with an `SbeTemplate`) | `crates/sbe/benches/alloc.rs`, counting allocator, injection-proven the same way as `codec`'s. `benches/sbe.rs` times the same four through the shared harness and prints `NO BASELINE`; no timing baseline has been recorded yet (D16, step 9) |
| Allocations on the hot path, session | **0** on sixteen paths: accept, refuse, tick, beat, answer, gap, fill, deliver, resend, logon_out, originate, ordered, clock, text, schedule-open, schedule-shut | `crates/session/benches/alloc.rs`. The refusal path is counted apart because a hostile counterparty controls it and a `format!` is easiest to reach for there. `[measured 2026-09-02]` injecting one into `ordered` reads 10 000 |
| Allocations on the hot path, engine | **0** on thirty-three paths (`cargo bench -p fixbolt-engine --bench alloc`'s `allocations:` line, 2026-09-14, `[2026-09-23]` +2): **mark-out-mem, mark-out-file-async, journal-async-busy**, idle, send, recv, frame, turn, shard-turn, busy, ring, interests, pending-idle, pending-busy, pending-cycle, registry-lookup, observe-idle, observe-asked, events-idle, events-busy, admin-idle, admin-busy, shutdown, reconnect, log-record, log-idle, log-busy, **origin-idle, origin-busy**, adopt-idle, **logon-first**, **redact-mask, redact-scan** | `crates/engine/benches/alloc.rs`. `busy` asserts the session is still logged on at the end of the count, because an earlier version measured a connection dropped at message two. `log-record` calls `MessageLog::record` a thousand times with no engine in the window; `[measured 2026-09-04]` making it allocate once reads 1000. `[measured 2026-09-05]` the two ADR-0048 cases read **2000** and **16** under an injected `format!`; `logon-first` is sixteen exact calls rather than thousands because `speak_first` runs once per session and the fixture cannot cycle sessions — one `Config` means a second concurrent session is refused as a duplicate, and a dropped `Loopback` peer signals no EOF, so an early version of that case read `1 sends over 500 sessions`. `[2026-09-23]` **`redact-mask`/`redact-scan`** are ADR-0110's `mask`/`carries_secret`, each called 1 000 times over a fixture carrying a secret; each case asserts its own path actually found it (`redact-mask` masked 2 000 fields over the Logon it ran twice, `redact-scan` saw a secret all 1 000 times), so their zero is not read off a path that never ran — injecting `std::hint::black_box(Vec::with_capacity(1))` into `mask` reads 1 000, not 0 (docs/plans/2026-09-23-p3-redact-secrets.md reversal R5; a version without `black_box` was optimised away and proved nothing, `docs/reference/measured-costs.md`). What no bench here proves is that the writer thread allocates nothing while the engine runs; `tools/w2w` is where a both-threads number belongs |
| A peer's TLS 1.3 KeyUpdate allocates exactly what rustls's key schedule forces, and nothing else `[2026-09-13]` | engine and ktls-core: **0**; rustls: **two** boxes of **exactly 184** bytes per direction rekeyed — **four** under `update_requested`, **two** under `update_not_requested`, of this lock's rustls 0.23.44 and ring 0.17.14 — asserted `==`, not `<=` — the second named carve-out from non-negotiable 1 | `crates/engine/tests/tls_key_update.rs::a_key_update_allocates_only_the_boxes_rustls_key_schedule_forces` (`update_requested`, 4 boxes) and `::a_key_update_without_update_requested_rekeys_one_direction_and_allocates_two_boxes` (`update_not_requested`, 2 boxes), each run reading the same count ([ADR-0063](decisions/ADR-0063-a-peers-key-update-is-the-second-named-carve-out-and-a-ticket-is-not-read.md)). A ceiling (`<= 4`) was what step 6c shipped under a name that said "nothing"; an exact count is the assertion that forces a re-read on the next rustls or ring bump |
| A client's session tickets allocate nothing after the handover `[2026-09-13]` | **0** over the ticket window, **and** `tickets_ignored() == 2`, so an absent ticket cannot pass as an unallocating one | `crates/engine/tests/tls_key_update.rs::a_session_ticket_after_the_handover_allocates_nothing`. `[measured 2026-09-13]` `Resumption::disabled()` alone left this at 16 allocations (`Window { count: 16, largest: 354 }`); the newtype `tls::side::Ticketless` is the whole of the fix (ADR-0063 Revision) |

### Mode and machine

| Gate | Target | Proven by |
|---|---|---|
| The engine thread never sleeps in the kernel (`hft`) | no blocking syscall on that thread | `scripts/check-no-kernel-sleep.sh`: traces `tools/w2w` with `strace -f` and attributes calls to the engine thread by tid. `[measured 2026-08-30]` Linux 6.18: `accept4`, `recvfrom`, `sendto` and zero of `epoll_wait` / `poll` / `select` / `futex` / `nanosleep` / `sched_yield`. Runs the binary again in `standard` mode and fails if that run does not trip it: rule 4 had two machine checks before this one and both were green with a sleep present. A second, tracer-free gate needs no capability: `scripts/check-no-kernel-sleep-by-ctxt.sh` reads the engine thread's voluntary context switches and asserts `hft` reads 0, `standard` reads > 0 ([ADR-0072](decisions/ADR-0072-a-tracer-free-check-that-the-hft-engine-thread-never-sleeps.md)) |
| The engine thread never sleeps in the kernel (`hft`), **TLS arm** `[2026-09-13]` | `--tls ktls` traces the same, with no sleeper and the read-back confirming which arm ran | the same script, Sửa 6 step 6b: runs `hft --tls ktls` (no sleeper, the usual socket calls, `tls: kernel` read back) and `--tls userspace` (must read back `tls: userspace`, so the two arms cannot be mistaken for each other). A build without the `tls` feature reports both old halves, then `TLS arm SKIPPED, NOT PASSED`, exit 2, rather than a silent pass |
| A `standard` engine gives the core back | engine-thread CPU under 5% over a wall-clock window, found sleeping rather than running, **and** a round-trip p50 far below the poll timeout | `scripts/check-standard-gives-the-core-back.sh`. Four assertions, because CPU near zero is also what a dead thread, a run that never reached the mode, and an engine woken by its own timeout report. `[measured 2026-08-30]` a `Block` made to ignore readiness reads 0% CPU, sleeping 20 / 20, p50 99 046 599 ns; only the p50 catches it. Requires `hft` and `yield` to trip it |
| A `standard` engine gives the core back, **TLS arm** `[2026-09-13]` | the same four assertions, on `standard --tls ktls` (not `hft`), plus the mode read back as `kernel` | `scripts/check-standard-gives-the-core-back.sh`'s own TLS-arm block, Sửa 6 step 6b (script lines 221-250): runs `--mode standard --tls ktls` once and judges it green-or-not by the same four assertions as the plain `standard` case, plus the `tls:` read-back. **This arm has no scripted red half** — the script's `for red in hft yield` reversal loop (lines 209-219) covers only the plain arm; nothing here automates a TLS-mode-mismatch reversal. The 99.53% CPU figure is not this script's output: it is a **hand-run** reversal — the standard-mode judgement invoked by hand against an `hft --tls ktls` run — recorded only in commit `da9fe6e`'s message ("standard check on hft --tls ktls: engine CPU 99.53%, red"), not reproduced by any committed script invocation |
| kTLS can be driven from a plain non-blocking socket | 15 assertions green, and the offloaded data path makes no blocking syscall | `scripts/check-ktls-on-a-plain-socket.sh` (D11, [ADR-0018](decisions/ADR-0018-ktls-on-a-plain-socket-answers-adr-0005.md)). `[measured 2026-08-31]` `recvfrom` 3033 + `sendto` 1000 over 1000 round trips and nothing else. Runs a second time with `poll(2)` in the loop and fails if that does not trip it. Skips with exit 2, not a pass, on a kernel that cannot offload |
| Which TLS mode is actually in force | a session that fell back to the userspace path is detected, not assumed | `crates/engine/tests/tls_mode.rs` drives every arm, since step 4b [2026-09-12, `e728c16`]: `serve_tls_with_offload` reads `Transport::tls_mode()`, a handshake landing in userspace raises `observe::EventKind::TlsFellBackToUserspace` whether or not `TlsRequireKernel` is set, and `TlsRequireKernel=Y` refuses the deployment. **This cell read "no gate exists yet" for two days after the gate existed**, while D11 above said the opposite — one rule, two places, and the table is the place a reader checks. ADR-0005 question 3 is answered; ADR-0005 itself is not edited (§5), the answer is dated here |
| The lint config denies `unwrap` / `expect` / `panic` | red on a crate carrying all three, green once they are gone | `scripts/check-lint-config.sh`, in CI on every push |
| No crate root switches a lint back off | red on every inner attribute the Rust lexer sees at a crate root that is an `allow`/`expect`, or a `warn` naming a lint the workspace denies — comments, whitespace and newlines carry no meaning, and a string stays a string | `scripts/check-no-crate-root-allow.sh`, in the `lint-config` CI job beside the row above, reading `tools/attr-scan`'s output rather than a regex. 6 crate roots, 10 manifests and 4 deny lints from `cargo metadata`/`Cargo.toml`, never a file glob, and 0 crate roots or 0 deny lints is an error rather than a pass. Measured against the four reversals that go red — a `/* */` comment inside the parens, `# ! [ … ]` as three separate tokens, a bare `#!` then a newline before `[cfg_attr(test, allow(dead_code))]`, and a `warn` lowering a lint the workspace denies — and the one deliberate green control, `#![doc = "#![allow(…)]"]`, which stays green while the attribute count it reads goes 55 → 56, so the attribute was read and judged rather than overlooked — and **no further**: an attribute inside `mod x { … }` is still item 55, unchanged, and `RUSTFLAGS`, `--cap-lints` and a file pulled in by `include!` are still outside what it can see. `[measured 2026-09-12]` **its own A2 reversal went green twice**: first a character class excluding `:` could not match a lint written after `clippy::` ([a-matcher-excluded-the-separator-every-real-name-uses](reference/a-matcher-excluded-the-separator-every-real-name-uses.md)); then a senior review found the deny-list derivation itself matched only a bare identifier key, so `"unwrap_used" = "deny"` (quoted TOML, valid and cargo-honoured) passed through unmatched and `#![warn(clippy::unwrap_used)]` read `ok`. Quoted, dotted (`clippy.unwrap_used = "deny"`) and inline-table (`unwrap_used = { level = "deny" }`) keys, and a literal-string value (`'deny'`), are now recognised, and the `ok` line prints the count so it is visible like the other two. R-A4 ties the textual check to the compiler once — with the crate-root allow in place a fresh `v[0]` in a clean submodule of `dict` is exit 0 and silent, without it exit 101 `indexing may panic` |
| A gate's scratch fixture cannot inherit the machine | red when a line enters a directory outside the tree without copying every pinning artefact found at the repository root, by three rules: a `#`-first line is never read, `cp` counts only in command position, and any assignment prefix seeds the scratch variable (a `cd`/`pushd` naming no variable at all fails outright) | `scripts/check-scratch-fixtures.sh`, same job. 20 scripts, 1 entering a scratch dir, 1 pin; the artefact set is derived from what exists there and an empty set is an error. No allow-list, so the other `mktemp` scripts pass by never leaving the tree rather than by being named. `[measured 2026-09-12]` a senior review got past it four more ways — a `cp` reached only through a trailing comment, and a script with no `.sh` name, both now fixed (a `#`-first line is never read at any step, and scope is by shebang rather than extension) — and four remain, `[measured 2026-09-12]` by decision rather than by oversight: a `cp` inside a heredoc body, a `cp` copying out of the scratch dir rather than into it, and two seeding misses, `read -r TMP < <(mktemp -d)` and a bare `TMP=/tmp`. Each is one more regex, and the gate stays a regex anyway — the reasoning, and why a real parser does not close the gap either, is `docs/decisions/ADR-0061-the-scratch-fixture-gate-is-a-regex-and-says-so.md` ([a-scratch-fixture-inherits-the-machine](reference/a-scratch-fixture-inherits-the-machine.md)) |
| Every `tls`-gated test the build lists actually ran | red when a test the `--features tls` build lists cannot be accounted for in the job's own log, by three order-independent assertions rather than by attributing a log line to a binary | `scripts/check-feature-gated-tests-ran.sh`, in the `tls` CI job: the listing half asks `cargo test -p fixbolt-engine --tests --features tls --no-run --message-format=json` for the compiled binaries and runs each one's own `--list`, so attribution to a binary is exact by construction rather than read from the log. The reconcile half reads a real run's log, which interleaves `cargo`'s `Running <path>` (stderr) with each binary's own test lines (stdout) with no guaranteed order — `[measured 2026-09-12]` the first version merged the two with `2>&1` and inferred attribution from their relative order, which held on the author's desk and read a test as running before any `Running` line on CI ([two-streams-through-one-pipe-have-no-guaranteed-order](reference/two-streams-through-one-pipe-have-no-guaranteed-order.md)) — so it now asserts only what does not depend on order: every listed name appears in the log carrying a run status as many times as it was listed, the set of `Running` binaries equals the set built, and the ignored count is at the ceiling. `[measured 2026-09-12]` 38 binaries, 38 `Running` lines, 326 listed, 326 accounted for, 0 ignored (ceiling 0) — 322 was the count at `161744a`, before step 4 added four `doc_table` lib tests. Listing nothing is an error rather than a pass; a canary test added to `tests/admin.rs` and left off a synthetic log reproduces the FAIL sentence |
| Builds with nothing optional installed | `--no-default-features` on a clean runner | `.github/workflows/ci.yml`, its own job. `[measured 2026-08-30]` the workspace-wide command alone is not enough: cargo unifies features across one invocation, so a sibling crate switched the flag back on ([feature-flags-unify-across-a-workspace](reference/feature-flags-unify-across-a-workspace.md)) |
| `Cargo.lock` is current for the commit under test `[2026-09-13]` | a manifest edit committed without its matching lock is red, not a silent re-resolve | `cargo metadata --locked --format-version 1` as the first step of the `gates` job (`.github/workflows/ci.yml`), plus `--locked` on both `cargo test` invocations of the `tls` job — the job whose assertions (D11 above) are pinned to this lock's rustls and ring. Not `--frozen`: CI still needs the network to fetch what the lock names. Sửa 8 §8.3, `docs/plans/2026-09-04-tls.md` |
| An optional dependency is really optional | absent from the crate's graph with no features on, and the crate still builds and tests that way | `scripts/check-no-optional-deps.sh`, per crate. Reversal: removing `optional = true` from `libc` turns it red with the graph printed |
| No documentation link points at a missing file | every internal link resolves | `scripts/check-links.py`, in CI. `[measured 2026-09-13]` an absolute URL is judged by two rules, not one right-to-left tail match: into this repository's own path (`OWN_REPO`; host, owner and name compared without case, `www.` dropped — `[measured 2026-09-13]` compared as written, a mixed-case link to this repository's own `CHANGELOG.md` passed as foreign) any matching tail counts, even one segment; into any other path only a tail of two or more segments counts, because a bare filename such as `CHANGELOG.md` is not evidence of which repository it belongs to — the one rule read a citation of `EmbarkStudios/cargo-deny`'s `CHANGELOG.md` as a link to this repository's own. The class that leaves is counted and printed, never dropped silently: an absolute URL to this repository's own root file under the wrong organisation (`https://github.com/fixbolt/CHANGELOG.md`) now passes unreported ([a-bare-filename-is-not-evidence-of-a-repository](reference/a-bare-filename-is-not-evidence-of-a-repository.md)). Rule (d), checked first: an absolute URL into this repository's own path (current name or a former one) whose verb is `blob`, `tree`, `raw` or `blame` is judged by reading its own path and testing it against the tree directly, not by any tail match — closing the case where no suffix of a wrong path matches any real file, README's exemption included, since that exemption waives only "must be relative", never "must exist". `[measured 2026-09-13]` a senior review of PR #69 found rule (d) itself had gone silent on a verb-less own-repository URL, on `<...>` autolinks and `raw.githubusercontent.com` links it never read at all, and on a wrong-case path that macOS resolves but Linux CI does not — all fixed, plus a bare ref no longer inflates `OWN_REPO_FLOOR` and a Markdown link title no longer joins the path; `%2F` in a ref, a `fixbolt.git/...` URL, and the `edit`/`commits` verbs remain stated limits, not fixed |
| `unsafe` blocks | each names what proves it sound | code review + Miri |

### Timing, per machine

| Gate | Result on the §9 desktop | Proven by |
|---|---|---|
| Parse NewOrderSingle | `[measured 2026-09-05]` **120.4 ns** validated, 113.5 raw, **60.9** for a Heartbeat. The Heartbeat figure was 56.3 until its fixture was corrected — it declared `9=49` against a body of 51 and the parse returned before its own checksum ([a benchmark parsed a message the parser rejects](reference/a-benchmark-parsed-a-message-the-parser-rejects.md)) | `benches/parse.rs` against `benches/baselines.tsv`, and the fixture is asserted valid before anything is timed |
| Dictionary pass, per inbound message | `[measured 2026-09-05]` **897.3 ns** for the `NewOrderSingle` `tools/w2w` sends, **218.4 ns** for its `TestRequest`; 882.1 and 169.5 for the two shapes `parse.rs` uses. **Seven times the parse it follows**, and the largest single piece of user-space work in §8 | `crates/session/benches/validate.rs` through `fixbolt_session::validate` ([ADR-0050](decisions/ADR-0050-the-dictionary-pass-is-public-so-it-can-be-timed.md)), against `baselines.tsv`. Proven by reversal: `validate` returning `None` immediately reads 1.1 ns |
| Serialise ExecutionReport (template, D9) | `[measured 2026-09-05]` **237.6 ns**, alignment pinned (ADR-0049); the 239.1 recorded 2026-08-31 was the same encoder at a different address. The 60 ns absolute target is withdrawn (ADR-0016): 93.8 (M5) · 177.6–199.4 (container) · 239.1 (desktop), and none came close | `benches/serialize.rs` against `baselines.tsv` |
| `RingDispatch` hop vs `InlineDispatch` | `[measured 2026-09-01]` inline **8.5 ns**, ring **267.4 ns** one way, **515.7 ns** round trip, on a 163-byte NewOrderSingle: about 31×, and about 1.7 ns of every byte is the `AtomicU8` copy. The inline figure was published as 1.3 ns for a day; that was the optimiser deleting the 163-byte copy, found by the arithmetic that 163 bytes in 1.3 ns is 125 GB/s from one core ([a-benchmark-can-delete-its-own-work](reference/a-benchmark-can-delete-its-own-work.md)) | `crates/engine/benches/dispatch.rs` against `baselines.tsv` |
| Per-message cost at N sessions on one thread | `[measured 2026-09-05]` **1 659.8 ns at N=1, 1 890.8 at N=64 — a ramp of 13.9%, and no step at the L2 edge.** That absence is the result: a message touching most of its 21 KiB connection would put a step at N ≈ 20. Converted through this machine's own latency-by-working-set table, a message touches **~2 to 4 KiB** | `crates/engine/benches/density.rs`, medians of 20 clean runs of 22, against `baselines.tsv`. Setup asserts each of the N sessions delivers exactly one order per turn, because a sweep whose sessions were dropped at logon is flat, fast and meaningless |
| Keeping a message for resend | `[measured 2026-09-05]` **8.9 ns** for a 191-byte `ExecutionReport` into `MemJournal<4096,512>`, walking the ring as the engine does; **6.3 ns** pinned to one slot. `[re-measured 2026-09-21, boot D step D3]` the one-slot case now reads **7.4 ns** (n = 20, `pass 16 fail 0 unknown 0`) and its line carries the ladder's floor 1.10 instead of the 1.35 it had. `[2026-09-23, boot F]` a later 12.4 on the same binary was the length of `baselines.tsv` moving the heap (item 99), fixed by a fixed-size read buffer; a file-length sweep on the fixed binary reads 7.4–8.3. The 6.3 was a fixed point of a **compiled-in** baseline table; ADR-0067 made the table read at run time, and 8.2, 6.3 and 7.4 are one case on one desk differing only in which binary was asked — [recording-a-baseline-changed-the-baseline](reference/recording-a-baseline-changed-the-baseline.md) *n = 20 on the run-time table*. A 2 MiB ring is not a cache cost — 191 bytes at a 512-byte stride is what a prefetcher is for | `crates/engine/benches/journal.rs` against `baselines.tsv`, every case reading back what it wrote before anything is timed |
| What a bigger message costs the kernel | `[measured 2026-09-05]` **0.1443 ns per byte** written and read, from an 8 → 8192 byte lever. The two real `tools/w2w` sizes are cases of their own and **their difference is under this instrument's resolution**, which the module doc says where the number is | `crates/engine/benches/payload.rs` against `baselines.tsv`. Absolute figures here are environment-bound, not a round-trip claim — [a loopback write costs thirty-two syscalls](reference/a-loopback-write-costs-thirty-two-syscalls.md) |
| Wire-to-wire, loopback | `[measured 2026-09-02]` **met**: `pass 12 fail 0 unknown 1`, engine pinned to isolated `cpu6`, client to `cpu7`, medians of 20 runs of 20 000 round trips. `hft` **16 010 / 20 589 / 22 127 ns** administrative, **19 908 / 24 657 / 26 150** application; `standard` **19 447 / 24 106 / 25 609** and **20 920 / 25 618 / 27 092**. p99 ≤ 50 µs holds in all four arms. Allocations in the timed window 0 on both threads | `tools/w2w --features affinity`, driven by `scripts/w2w-baseline.sh`. Phase 1 exit criterion 6 |
| Wire-to-wire, NIC to NIC | **MET at p50 and p99, `hft`, back to back — `[measured 2026-09-18]` boot C, C-40**: acceptor NIC-in → NIC-out, hardware stamps both directions on `enp9s0` (I211), the generator on a Mac mini over a direct cable, interval 0, 20 000 requests × 10 runs per arm, two procedures ([ADR-0068](decisions/ADR-0068-a-published-figure-is-two-procedures-shown-side-by-side.md)): **admin p50 27 050 ‖ 27 114, p99 31 982 ‖ 32 254 ns**; **application p50 28 894 ‖ 28 878, p99 34 058 ‖ 34 110** — p50 and p99 within 0.9%, p99 ≤ 50 µs in both arms. **p99.9 is published marked, not met**: 35 374 ‖ 49 146 admin, 62 306 ‖ 88 630 app (27–42% apart, a tail two procedures did not agree on). The rate is the one this row asks for — each request sent as soon as the previous reply arrived, ~37 000 round trips a second — with 6/10 ‖ 8/10 runs qualifying under [ADR-0071](decisions/ADR-0071-a-skipped-tx-stamp-is-a-missing-sample-not-a-failed-run.md)'s 0.1% TX-missing line. **`hft` only**: `standard` waits on BPF sock_ops, phase 2 ([ADR-0073](decisions/ADR-0073-the-standard-wire-figure-goes-through-bpf-sock-ops-and-not-before-phase-2.md)). ~~**not met.**~~ Loopback has no driver, no IRQ and no wire, which is why §9's NIC IRQ affinity row reads `unknown` beside every loopback figure above. `[measured 2026-09-15]` the first hardware-stamped figures were paced at one message a second — wire p50 admin 45 146 ‖ 42 918 and application 49 626 ‖ 45 082 ns (§8 *Boot B*), ~18 µs *slower* than back to back: the cold figure beside the hot one; back to back, `igb` skipped a TX stamp within 1–4 runs every time and the procedure then failed a run for it | `tools/w2w` with `SO_TIMESTAMPING`, HdrHistogram, a load generator on a separate machine. STATUS item 40. `[2026-09-14]` the two halves exist (`--listen`, `--connect`), and so does `--wire-timestamps` on the engine half — hardware RX and TX stamps on the acceptor's NIC, one PHC, no clock sync. **Still not met** on that day: no cable, so no hardware stamp had been read (read on 2026-09-15 — the first column); on `lo` the tool counts every stamp missing and prints no wire column, which was the only arm run at the time. A figure is published from a run whose missing-stamp count is ≤ 0.1% of the timed requests and
`hw-rx-missing 0` ([ADR-0071](decisions/ADR-0071-a-skipped-tx-stamp-is-a-missing-sample-not-a-failed-run.md)
decision 1) — a skipped TX stamp costs one sample, it does not fail the run. **Mode `hft`
only**: `w2w` refuses `--mode standard --wire-timestamps` on a hardware NIC
([a-transmit-timestamp-wakes-a-blocking-engine](reference/a-transmit-timestamp-wakes-a-blocking-engine.md));
`standard` on a NIC is the generator's table, *as the counterparty sees it*, and never a wire
figure — a `standard` wire figure of its own waits on BPF sock_ops, phase 2
([ADR-0073](decisions/ADR-0073-the-standard-wire-figure-goes-through-bpf-sock-ops-and-not-before-phase-2.md)) |

The wire-to-wire row is the only one that measures what a counterparty experiences. Every
other row is an internal number; without this one they are unfalsifiable.

### Timing gates are per machine, not absolute

[ADR-0016](decisions/ADR-0016-per-machine-baselines-replace-absolute-targets.md),
[ADR-0031](decisions/ADR-0031-a-baseline-is-a-band.md). **There is no single published
nanosecond target.** Every timing row is judged against the figure this project measured for
that case on the CPU it is running on, inside a band `[baseline / margin, baseline × margin]`.
Both live in [`benches/baselines.tsv`](../benches/baselines.tsv), each line carrying its sample
size, its date, and the `check-machine.sh` verdict of the run that produced it. **The table is
not part of any bench binary**: the harness reads it at run time from a path fixed at compile
time, a missing or malformed file is a non-zero exit in every mode, and appending a line
changes no byte of the binary it is compared against
([ADR-0067](decisions/ADR-0067-the-baselines-are-read-at-run-time-not-compiled-in.md) —
until then it was `include_str!`'d, and recording a baseline changed the baseline,
`STATUS.md` item 52).

`[2026-09-23]` **A figure measured outside the harness meets the same band, and the file's
length stays out of the bench's heap**
([ADR-0096](decisions/ADR-0096-a-figure-measured-elsewhere-meets-the-same-band-the-baseline-file-stays-out-of-the-heap-and-a-boot-may-rebuild-on-its-housekeeping-cores.md)
decisions 1 and 2). `Suite::figure(name, ns)` judges a number the bench sampled itself — the two
`wakeup` p50s, 20 000 cross-thread samples per run — against its `baselines.tsv` line with the
same band, the same `OVER` / `UNDER` / `NO BASELINE` line and the same tallies as
`Suite::bench`; their lines are unpinned (as `bench.sh --strict` runs them), baseline = median of
the per-run p50 over 19 runs. p99 and above stay printed, not banded. And the harness reads the
file into a fixed 1 MiB buffer, because a buffer sized to the file let its length move a pinned
L1 case ×1.68 (`STATUS.md` item 99; [recording-a-baseline-changed-the-baseline](reference/recording-a-baseline-changed-the-baseline.md)
*The second time, at run time*).

Two findings retired the absolute column. The 60 ns serialise target was never a measurement
of this engine, only of what commercial engines are reported to reach, and no machine came
within 1.5× of it. And the ceilings had the same disease from the other side: `ring, one way`
measured 260.9 ns on a Ryzen 7 3700X, 270.7–272.9 on an EPYC 9V74 and 327.2–331.1 on an EPYC
7763, 21% between two machines of one vendor against ~1% within either, while the single
260 ns ceiling sat 0.3% *below the fastest of the three*. A ceiling no machine passes is a
ceiling somebody switches off.

**Recorded baselines**, medians of **20** qualifying `scripts/bench.sh` runs
`[measured 2026-09-05]`, on a box reading `pass 12 fail 0 unknown 1` for every run counted,
**with function alignment pinned** (ADR-0049). Every line was re-recorded that day: the
measurement changed, so what it is compared against changed with it. This machine is the only
one in `baselines.tsv`, so nothing else was invalidated.

`[2026-09-05, later the same day]` **four `validate` lines were added at n = 21, and one older
line was corrected rather than re-recorded.** `parse Heartbeat (validated)` goes **56.3 → 60.9
ns**: its fixture declared `9=49` against a body of 51, so `parse_into` returned
`Err(BadBodyLength)` on the line *before* the checksum block and the case had never summed its
own 51 bytes ([a benchmark parsed a message the parser
rejects](reference/a-benchmark-parsed-a-message-the-parser-rejects.md)). The two
`parse NewOrderSingle` lines were re-measured in the same runs, came in at 120.0 and 114.1
inside their bands, and **were not touched**.

| Case | AMD Ryzen 7 3700X | margin |
|---|---|---|
| parse NewOrderSingle (validated) | 120.4 ns | 1.10 |
| parse NewOrderSingle (no checks) | 113.5 ns | 1.15 |
| parse Heartbeat (validated) | 60.9 ns | 1.10 |
| encode ExecutionReport (template) | 237.6 ns | **1.15** |
| SendingTime from the cache | 4.9 ns | 1.10 |
| walk 1 group, 2 entries | 56.2 ns | 1.10 |
| walk 4 levels, 61-tag member list | 339.5 ns | 1.10 |
| `group_members` contains, 61 tags | 9.0 ns | 1.10 |
| encode 1 group, 2 entries | 108.8 ns | 1.10 |
| inline deliver + reply | 8.1 ns | 1.10 |
| ring, one way | 258.2 ns | 1.30 |
| ring, round trip | 496.0 ns | 1.20 |
| recv on a quiet socket | 418.5 ns | 1.10 |
| engine turn, 1 idle session | 481.0 ns | 1.10 |
| engine turn, 4 idle sessions | 1896.1 ns | 1.10 |
| engine turn, 16 idle sessions | 7694.8 ns | 1.10 |
| presession sweep, 1 quiet socket | 435.0 ns | 1.10 |
| presession sweep, 16 quiet sockets | 6819.5 ns | 1.10 |
| presession, read and route an identity | 186.5 ns | 1.10 |
| presession, registry lookup of 1 | 10.8 ns | 1.10 |
| presession, registry lookup of 40 | 100.8 ns | 1.10 |
| library, parse only | 159.6 ns | 1.10 |
| library, reply only | 804.1 ns | 1.10 |
| library, on_message | 1028.6 ns | 1.10 |
| validate NewOrderSingle | 882.1 ns | 1.10 |
| validate Heartbeat | 169.5 ns | 1.10 |
| validate TestRequest, w2w bytes | 218.4 ns | 1.10 |
| validate NewOrderSingle, w2w bytes | 897.3 ns | 1.10 |

**No other machine has a baseline, and none is invented.** The Apple M5 and the CI EPYCs have
figures scattered through this repository, but none was taken by the procedure above, so they
report `NO BASELINE`, which is counted on its own summary row and is not a pass.

**The margin is per case because one margin cannot work.** Nine of twelve cases hold inside
7.6% of their own median across a run set, while `ring, one way` draws a second mode at +24%
on roughly 1 run in 15. A single margin wide enough for that case would let `encode
ExecutionReport` drift 236 → 319 ns unnoticed.

**Bench builds pin function alignment, and the flag is read back**
([ADR-0049](decisions/ADR-0049-bench-builds-pin-function-alignment-and-the-flag-is-read-back.md)).
`scripts/bench.sh` exports `-C llvm-args=-align-all-functions=6` and then verifies, from the
built binaries, that it took — `scripts/check-bench-alignment.sh`, `[measured 2026-09-05]`
**23 of 23 own-crate text symbols on a 64-byte boundary pinned, 5 of 23 unpinned**.

It exists because `encode ExecutionReport (template)` was a 16% measurement of *where the
compiler put the code*. Its baseline was recorded at 239.1 ns and the same encoder read 280.4
four days later; the whole jump is one commit that touches no `crates/*/src/` file, and adding
inert functions the encoder never calls walks the case across 236.5–292.4 ns. Under the flag
the same perturbation holds inside 4.0%, which is why that one case carries 1.15 rather than
1.10 — pinning shrinks the layout term, it does not remove it.

**The cost, stated:** these figures are of a binary that is not the one that ships. That is
tolerable here because every §6 timing row is compared against *itself over time on one
machine*; it would not be tolerable for a published absolute. `tools/w2w` and §8's budget do
not come through `bench.sh` and are unaffected.

**Stretch, and not a gate:** serialise at about 116 ns on the §9 desktop, the measured floor of
the current `Part` shape. Reaching it needs the ~31 ns fixed prefix cost and the ~7 ns per
field in `put` attacked, not the slot scan.

### How the benchmarks are run

`scripts/bench.sh` runs every bench target and the `bench` CI job runs it on every push.
`scripts/ab-rotation.sh` is the other driver: for a measurement boot it interleaves several
**pre-built** worktrees round-robin against one shared control, reads a quiet row per arm per
round, drops a whole round if any arm sees load, and **refuses to compile** — a binary whose
sha256 has moved stops the run rather than being rebuilt (ADR-0090).
(Until 2026-08-30 nothing ran them: `cargo test --all` does not run a `harness = false` bench
target, and every ceiling above was an assertion no machine executed.) The script splits the
two kinds of benchmark by what decides their result:

| | What it measures | On a failure |
|---|---|---|
| **Invariant**: `alloc` × 3, `ring_full` | allocation counts, message counts | **CI red.** The answer is the same on every machine |
| **Timing**: `parse`, `serialize`, `groups`, `dispatch`, `turn`, `presession`, `validate`, `cost` | ns/op against this machine's own band, from a build with **function alignment pinned and read back** (ADR-0049) | **Reported, never red on a shared runner.** `bench.sh --strict`, which a §9 machine runs, is fatal on a case with no baseline for its CPU and on one that came in under its floor, because a benchmark that stops measuring reads far under its limit |

Timing ceilings are not enforced on a shared runner because they cannot be: `[measured
2026-08-30]` five runs on a 4 vCPU container gave a run-to-run spread of 5–232%, and on the
2-core CI EPYC `ring, one way` ranged 270.7–331.1 ns across four runs while single-threaded
cases held to ~3%. The spread follows whether a case crosses threads. A gate that goes red at
random gets switched off.

Every case is measured and printed before any is allowed to fail. The harness used to assert
inside each case, so the first one over its ceiling ended the process and a fourth `groups`
case over its ceiling was never seen. The script also fails when a target produces no
measurement: cargo had auto-discovered `benches/harness.rs`, a module with no case, as a ninth
bench target that reported `0 measured` and exited 0.

**A baseline must be taken through the path that will judge it.** `[measured 2026-08-31]` the
first attempt recorded medians from running the timing targets directly, and `encode
ExecutionReport` then went over its limit on the first `bench.sh` run, because `bench.sh` runs
eight targets and the case is not measured in the same state. Back-to-back runs leave the
previous suite inside `check-machine.sh`'s one-second window and disqualify their own
measurements.

`scripts/ab-rotation.sh` reads a suite's real exit status rather than the presence of output
([ADR-0092](decisions/ADR-0092-the-rotation-driver-reads-a-row-by-its-shape-and-a-panicking-finish-is-a-verdict-not-a-lost-round.md)):
a bench binary that panics because one of its own cases is over the machine's recorded baseline
still carries its measurement rows, so `OVER` is its own state — the round stays `complete` — and
is kept apart from `FAILED` — any other outcome: a non-zero exit without the harness's own verdict line, a non-zero exit whose row count disagrees with the panic's `<m>`, or **zero rows at any exit, `0` included** — which drops the round the way a
busy machine does. `--reextract <evidence-dir>` rebuilds `runs.reextracted.txt` from a round's
raw captures under `<evidence-dir>/raw/` without re-running anything and without ever writing
over `runs.txt`.

**A line that moves after a commit that did not touch its code is first counted, not timed**
([ADR-0102](decisions/ADR-0102-a-line-that-moves-while-its-instruction-count-does-not-is-a-layout-move-and-the-count-is-read-off-the-desk.md)).
Every `Suite::bench::<F>` is inlined into one `harness::suite::<closure>` per bench file, so an
edit to the harness, or a case added to the file, moves every other case's timed loop. ADR-0049
pins a function's start and nothing inside it. The band cannot tell that from a regression:
`[measured 2026-09-23, a diagnostic from the desk on its desktop grub line — not the §9 line,
not a published figure]` `6b2833b` moved `walk nested group + varData` about +10 % in ns/op while
the binary retired 0.032 % *fewer* instructions. `scripts/bench-instructions.sh A B` compares the binary a
line was recorded with against the current one by `instructions:u`, run with
`FIXBOLT_BENCH_COUNT_ONLY=1` so a binary whose case is `OVER` its line still runs to the end (`same-work` at ≤ 0.1 %,
`work-changed` above, `unstable` if an arm's own spread exceeds 0.01 %). The count is independent
of layout and valid on either grub line, but it needs a PMU, so it runs on the desk. CI runs only
its stub self-test, `scripts/check-bench-instructions.sh`, in `gates`. `same-work`, with no other
case of the binary moving the opposite way, makes *layout* a named cause for the re-record
ADR-0095 decision 2 demands; `work-changed` sends the case to the code. Every re-record keeps its
binary in `target/baseline-bins/` so the next comparison has its first arm.

### Packaging

[ADR-0160](decisions/ADR-0160-six-crates-release-in-lockstep-and-the-packaged-sources-are-the-stranger-before-crates-io-is.md).
Plan [2026-09-23-p3-packaging-and-first-release](plans/2026-09-23-p3-packaging-and-first-release.md)
rows 6a–6c. These gates ask a question the correctness and allocation gates above cannot: not
"does the tree build", but "does what a `.crate` upload would actually contain build, on the
toolchain the manifest claims, over the feature sets a stranger can pick" — a `cargo publish
--workspace --dry-run` alone only builds each crate's *default* features
([publishing-a-workspace-to-crates-io](reference/publishing-a-workspace-to-crates-io.md) trap 7).

| Gate | Target | Proven by |
|---|---|---|
| Every internal normal dependency of the six published crates is pinned `=`, and licence files reach each crate byte-identical to the root copies | **0 failures** | `scripts/check-release-versions.sh`, the `package` CI job. Reversal (6a): a caret requirement, or a one-byte edit to a copied `LICENSE-MIT`, both go red |
| What a `.crate` actually contains, for each of the six | `fixbolt-dict` ships `NOTICE` + its three `spec/*.xml`; every crate ships `README.md`, `LICENSE-MIT`, `LICENSE-APACHE`; none ships `tests/`, `benches/`, `vendor` or a `.def` fixture | `scripts/check-package-contents.sh`, reading `cargo package --list -p <crate>` per crate — packages nothing to disk. `[measured 2026-09-23]` reversal: dropping `NOTICE` from `fixbolt-dict`'s `include` reads `FAIL fixbolt-dict: NOTICE missing from package`; adding `tests/**` to `fixbolt-codec`'s reads `FAIL fixbolt-codec: tests/ shipped (tests/<name>.rs)` once per file |
| The packaged sources build, on the pinned toolchain and on the declared `rust-version`, over the feature sets a user can pick | **12 cases**: 9 on the pinned default toolchain (one feature at a time beside each crate's default, plus `fixbolt-codec` alone, plus each of `fixbolt`/`fixbolt-engine`/`fixbolt-sbe` with `default-features = false`), 3 on `+1.89.0` (the combined-everything build for `fixbolt` and for `fixbolt-engine`, plus `fixbolt --no-default-features`) — declared `rust-version` is now `1.89` (ADR-0154 decision 1, `File::try_lock`), superseding the `1.88` this row measured against | `scripts/check-packaged-build.sh`: a scratch crate per case, outside the workspace (its own `[workspace]`), depending on the crate under test through an ordinary version requirement that `[patch.crates-io]` redirects to `target/package/<name>-<version>/` — the directory the dry run above leaves behind, byte-identical to a `.crate` upload. `[measured 2026-09-23]` all 12 `Finished` on the desk, `cargo` 1.98.0 + `1.88.0` (the then-declared MSRV). Reversal: reverting the declared `rust-version` to `1.85` while the source still uses `1.88`-only syntax (post 6a', let chains) and building on `+1.88.0`'s sibling `+1.85.0` reads `error[E0658]: 'let' expressions in this position are unstable` in `fixbolt-codec`'s `src/template.rs` — the exact trap [publishing-a-workspace-to-crates-io](reference/publishing-a-workspace-to-crates-io.md) trap 3 found by hand |
| A stranger's binary against the packaged sources / against GitHub at the release tag | a real Logon/Logout, from `docs/GETTING-STARTED.md`'s own pasted code | `scripts/stranger-check.sh --from packaged` (plan row 8a, every PR) and `--from git --tag v0.1.0` (row 8b, blocking from the first PR after the tag exists — `--from registry` stays in the script, unused, per ADR-0161 decision 1 "Không publish") |
| The public API is compared against something real | blocking from the `v0.1.0` tag onward | `scripts/check-semver-against-tag.sh`, no argument (`cargo-semver-checks` 0.50.0, `--baseline-rev` the newest `vX.Y.Z` tag `HEAD` descends from, compared as integers and printed as `baseline v0.1.0 = highest of: v0.1.0`), the `semver` CI job, no `continue-on-error` (ADR-0160 decision 7, ADR-0161 decision 5, [ADR-0162](decisions/ADR-0162-the-semver-baseline-is-the-newest-release-tag-head-descends-from-and-zero-checks-are-excused-only-by-a-major-bump.md)). The baseline is derived, never written down: a literal left at an old tag would read every later version as a major-level bump and compare nothing. The wrapper exists because an unchanged `0.y.z` version runs the real 196-check lint set, but a `0.y` -> `0.(y+1)` bump is read as a MAJOR change and skips every lint while still exiting 0 (`0 checks: 0 pass, 254 skip` per crate — first measured at `0.0.0` -> `0.1.0`, before row 6a existed, and it would recur at any future major-looking bump): the wrapper asserts each of the six published crates ran N > 0 checks, excusing a 0-check crate only when the workspace version is a major-level bump over the baseline by Cargo's rule (`0.1.0` -> `0.2.0`; `0.1.0` -> `0.1.1` with 0 checks is a FAIL), and refuses a baseline tag whose own manifest disagrees with its name or a workspace version already released under a tag `HEAD` does not descend from (ADR-0162 rules 1–4). Reversal, proven against a same-version baseline (an unmodified worktree at `0.1.0` against the `v0.1.0` tag): renaming `fixbolt_codec::checksum::checksum` reads `failure function_missing: pub fn removed or renamed`, naming both the function and its `fixbolt_codec::checksum` re-export, exit 100. Reversal of the wrapper's own assertion: adding `--release-type major` to its invocation reads `FAIL semver: fixbolt-codec ran 0 checks against v0.1.0`. No `--exclude` is needed for `fixbolt-conformance`, `fixbolt-sbe-gen` or `tools/*`: `cargo-semver-checks --workspace` already reads `publish = false` the same way `cargo publish --workspace` does and never mentions them |

## 7. Build order

Each step was a plan, a branch and a merge. **Steps 1–8 are complete as of 2026-09-02. Step 9 is
in flight: `sbe` and `sbe-gen` are built and tested as of 2026-09-19; `tools/sbe-interop`, the
second-implementation check the step also names, is not — it is routed to the cloud session
(Linux + Java) and has not landed.**

1. **`codec` + `dict`**: parse, serialise, generated tables ([plan](plans/2026-08-27-codec-dict.md)).
2. **Repeating groups**: `GroupIter` over the flat index, `<component>` recursion in `dict`
   ([plan](plans/2026-08-27-repeating-groups.md)).
3. **`conformance`**: the `.def` runner. Before step 4, so the gate existed before the thing
   it gates.
4. **`session`, acceptor role**: driven to 59 / 59.
5. **`session`, initiator role**: against the mirrored definitions, then interop against
   `libquickfix` in CI. A separate step because the oracle differs, not the code (ADR-0004).
   Paused after its own step 2 and resumed after step 6, because the mirrored gate measures
   framing rather than protocol wherever the harness plays the operator
   ([plan](plans/2026-08-29-session-initiator.md)).
6. **`engine`**: acceptor and connector (D8), journal, both dispatchers (D4), backpressure
   (D10).
7. **`tools/w2w`**: the wire-to-wire harness, run on Linux before step 8. `tools/jrnl` is
   beside it.
8. **`library`**: package `fixbolt`, with `examples/acceptor.rs` and `tests/end_to_end.rs`,
   which pulls in the same handler file with `#[path]` and drives it through a kernel socket.
   The example names nothing from `fixbolt_engine` or `fixbolt_session`, which is the facade's
   own test ([ADR-0041](decisions/ADR-0041-the-library-layer-buys-an-api-with-a-template-per-message.md)).
9. **`sbe` + `sbe-gen`**: the SBE codec under the `Encoding` trait (D16) and the generator that
   emits its layout tables from a schema. One step, like step 1, because the runtime is only
   usable with generated tables and the generator is only testable against the runtime. Under
   the phase 2 [plan](plans/2026-09-19-phase-2-fixt-and-sbe.md), the one exception to "one
   crate per plan" in `CLAUDE.md` §1, decided by the owner on 2026-09-19. `tools/sbe-interop`
   sits beside it as `tools/interop` sits beside step 5.

TLS (D11) has no step here. It landed beside step 6, as a second `Transport` —
`TlsTransport` in `crates/engine/src/tls.rs`, behind `--features tls`
([plan](plans/2026-09-04-tls.md)).

## 8. Latency budget on kernel TCP

Where the time goes for one inbound NewOrderSingle → outbound ExecutionReport, Linux, kernel
TCP, no bypass.

### The round trip, measured

`[measured 2026-09-02]` on the §9 desktop, `check-machine.sh` `pass 12 fail 0 unknown 1`,
engine pinned to isolated `cpu6` and the client to `cpu7`, medians of 20 runs of 20 000 timed
round trips each, **over loopback**:

| Round trip | `hft` | `standard` |
|---|---|---|
| TestRequest → Heartbeat (no application) | p50 **16 010** · p99 **20 589** · p99.9 **22 127** ns | p50 **19 447** · p99 **24 106** · p99.9 **25 609** ns |
| NewOrderSingle → ExecutionReport (through an application) | p50 **19 908** · p99 **24 657** · p99.9 **26 150** ns | p50 **20 920** · p99 **25 618** · p99.9 **27 092** ns |
| if `FileLog` is on, added per round trip (one inbound record and one outbound record; half of that, per direction, was not separately measured) | +901 to +1 032 ns `[measured 2026-09-15]`, tmpfs, from arms that did not reproduce — *Boot B* below | +896 to +922 ns, same |

`scripts/w2w-baseline.sh` is the procedure and
[reference/measured-costs.md](reference/measured-costs.md) the whole reading. **Loopback is
not a NIC**: §6's NIC-to-NIC row stays open, and these figures contain no driver and no
interrupt. **`[2026-09-23]` The floor is measured with the CPU mitigations on and the host's
netfilter rules loaded, and both are large terms of it** (ADR-0096 decision 5, `STATUS.md` item
51): on this desk `mitigations=off` took 58.4 % off a TCP loopback round trip (boot C) and
flushing tailscaled's ruleset took 22.6 % (boot F), each an A/B on its own boot, not additive —
[measured-costs](reference/measured-costs.md) *Item 51's last arm* and *Boot F, item 51*.
**`[2026-09-15]` A figure published from here on is two procedures shown side by
side** ([ADR-0068](decisions/ADR-0068-a-published-figure-is-two-procedures-shown-side-by-side.md)):
same commit, same binary, clean tree, at least 30 minutes apart; both medians appear, never
averaged; *reproduced* means within 5% of the smaller at every published percentile; an A/B arm
is one procedure of 10 runs and is only ever a difference. The table above predates the rule and
is one procedure.

### Boot B, 2026-09-15: every zero-interval loopback arm was faster the second time; nine of ten by more than 5%

`[measured 2026-09-15]` §9 desktop, `FIXBOLT_NIC=enp9s0 scripts/check-machine.sh` → `pass 15
fail 0 unknown 0` (busy row re-read before every run, 0 runs disqualified), kernel 7.0.0-31,
commit `5ca3889`, clean tree, `w2w` sha256 `350d3c17320f`, engine on `cpu6`, client on `cpu7`,
20 runs × 20 000 round trips unless noted; procedure 1 00:58–02:54, procedure 2 02:54–04:47.
ns, p50 / p99 / p99.9, procedure 1 ‖ procedure 2. The whole reading, A/B arms and diagnostics:
[measured-costs](reference/measured-costs.md), *Boot B*.

| Loopback round trip | procedure 1 | procedure 2 | reproduced? |
|---|---|---|---|
| `hft`, TestRequest → Heartbeat | 16 021 / 20 258 / 22 072 | 15 149 / 19 577 / 21 355 | **no** — p50 5.8% |
| `hft`, NewOrderSingle → ExecutionReport | 20 219 / 24 857 / 26 700 | 18 951 / 23 354 / 25 082 | **no** — 6.7 / 6.4 / 6.5% |
| `standard`, TestRequest → Heartbeat | 19 452 / 24 081 / 25 528 | 18 455 / 22 934 / 24 677 | **no** — p50 5.4%, p99 5.0% |
| `standard`, NewOrderSingle → ExecutionReport | 21 005 / 25 844 / 27 372 | 19 918 / 24 732 / 26 175 | **no** — p50 5.5% |

**Zero-interval loopback arms were measured twice over: B2's four above, B5's four journal/log
arms and B8's two fixbolt arms — ten in all, and all ten were faster in procedure 2, p50
differences 4.7–6.7%.** Nine of the ten did not reproduce; the exception is `standard` app
`--journal file-async` (4.7 / 4.1 / 3.3%). Across those ten, in-procedure p50 dispersion sat
between min/median ≥ 0.992 and max/median ≤ 1.013 (B2 alone: ≥ 0.995, ≤ 1.011). That is
[STATUS.md](../STATUS.md) item 85 again, at a larger scale and all in one direction. A
third B2 run at 04:48 — diagnostic, not a column — matched procedure 2 within 0.1% in `hft` and
read 2.0–2.2% slower in `standard`, so procedure 1 is the outlier rather than a drift. Procedure 1
began about seven minutes after a build slot (cargo, rustdoc) ended: a candidate, not a cause.
**The paced loopback arms below moved 0.4–3.9% at every percentile and reproduced**; the nanofix
comparison in measured-costs reproduced too (1.8–3.1%), but B8's own fixbolt arms, run the same
way, did not (p50 5.6% and 5.2%).

| `hft` admin, loopback, paced (`--interval`) | procedure 1 | procedure 2 | reproduced? |
|---|---|---|---|
| 1 ms, 20 000 × 20 | 19 296 / 23 785 / 26 250 | 19 066 / 23 599 / 26 129 | yes — 1.2% |
| 10 ms, 5 000 × 20 | 21 746 / 25 759 / 28 594 | 20 934 / 24 857 / 27 557 | yes — 3.9% |
| 1 s, 100 × 10, p50 only | 33 784 | 32 506 | yes — 3.9% |
| 1 s, `standard`, 100 × 10, p50 only | 32 121 | 31 916 | yes — 0.6% |

**Latency at 3 a.m.**: an `hft` engine answering one message a second is ~2.1× slower at p50 than
one answering back to back, and at 1 s `standard` read **lower** than `hft` in both procedures —
32 121 vs 33 784 (5.2%) and 31 916 vs 32 506 (1.8%) — the two modes are within a few µs at 1 s, and
`hft` is not faster there: D8's loopback advantage is not visible on a link idle for a second. The
1 s arms carry 100 samples, so only p50
is published. The plan asked for 120; `w2w` renders `SendingTime` before the clock starts, and
at one message a second the 121st is older than the session's 120 s `MaxLatency` — the engine
rejected it, correctly ([a-paced-run-outlived-the-sessions-maxlatency](reference/a-paced-run-outlived-the-sessions-maxlatency.md)).

| Application round trip, what the journal and the log add | `hft` p50, proc 1 ‖ 2 | `standard` p50, proc 1 ‖ 2 |
|---|---|---|
| `--journal file-async` (`FileJournal<4096, 512>`, `Durability::Async`) | +495 ‖ +586 | +450 ‖ +570 |
| `--log file` (`FileLog`, one record in, one out) | +901 ‖ +1 032 | +896 ‖ +922 |

Each is the arm minus the same procedure's plain application arm above. Both files are on
**tmpfs**: these are not disk figures. The journal and the in-memory `Store` now have the same
4 096 slots (STATUS item 88), so the difference is the kind of journal alone.

| Over a cable, `hft`, wire (NIC in → NIC out at the acceptor, hardware stamps) | procedure 1 | procedure 2 | reproduced? |
|---|---|---|---|
| admin, paced 1 s, 100 × 10, p50 | 45 146 | 42 918 | **no** — 5.2% |
| application, paced 1 s, 100 × 10, p50 | 49 626 | 45 082 | **no** — 10.1% |
| admin, back to back (the only arm that ran at interval 0 — the script stops at the first FAIL, so application never started) | **not measured** — an `igb` TX stamp was skipped within 1–4 runs every time | | |
| `standard` admin, back to back, *as the Mac sees it* (no wire stamps) | 260 771 / 277 749 / 322 000 | 260 625 / 277 791 / 321 729 | yes — 0.1% |
| `standard` application, same | 257 833 / 277 062 / 300 146 | 257 833 / 277 292 / 300 437 | yes — 0.1% |

The two 1 s procedures above ran close together — 05:32–06:10 and 06:20–06:58, starts 48 minutes
apart but only 10 minutes between the end of one and the start of the other, short of ADR-0068's
rule 4 (at least 30 minutes) — and the pair did not reproduce anyway.

Direct cable, desk `enp9s0` Intel I211 (`igb`) ↔ a Mac mini's built-in port, 1000baseT, **EEE
off at the desk**, pause on at both ends with every flow-control counter 0, `rx-usecs 0`, MTU
1500, IRQs on `cpu4`, observer on `cpu7`, generator unpinned on the Mac; `hw-rx-missing 0 and
hw-tx-missing 0` in every published run. **These are the first hardware-stamped figures this
repository has.** The back-to-back wire figure is what §6 asks for and it is not here: `igb`
holds one pending TX stamp, `tx_hwtstamp_skipped` rose with every failed run, and the procedure
fails a run with a missing stamp. **A failing run's own wire p50 is not a figure.** The runs that
did complete clean (diagnostic, not reproduced): EEE off, busy_poll 50, IRQs on cpu4 —
**26 178–26 218 ns, n = 4**; IRQs on cpu6 — **29 082, 29 138 ns, n = 2**, about +2.9 µs over the
EEE-off runs; EEE on — **39 522, 39 546 ns, n = 2**, about +13.3 µs. `busy_poll`/`busy_read` 0 had
**no clean run**. The 2 000-round-trip smoke and the discard run are not comparable and are
excluded from every range above. The Mac's table contains the Mac's own stack and an
unpinned generator both ways; it is not the acceptor's latency.

Three readings:

- **`hft` is worth 3 437 ns, 17.7%, against `standard` on the identical path.** That
  difference is D8's entire case. It also prices the wakeup row below at about 3.9 µs (the
  delta plus the ~449 ns sweep it replaces), inside the 2–5 µs this table has carried from the
  literature since it was written.
- **This engine's user-space path is 2.9% of the total**: 0.46 µs of a 16.0 µs `hft` round
  trip. The design makes that 2.9% as small as it can be, and through D8 trades `epoll`'s
  wakeup for a 449 ns poll. That trade wins at N = 1 and loses by N = 11.
- **The application round trip is 3 898 ns above the administrative one, and the committed
  benchmarks now account for about 1 094 ns of that — 28%.** `[measured 2026-09-05]` the
  dictionary pass, which STATUS item 39 named as the largest untimed candidate, is **679 ns of
  it, 17.4%**, and the gap is still **~2 804 ns unexplained**. The arithmetic is below the
  stage table. [ADR-0045](decisions/ADR-0045-parse-is-under-one-percent-of-the-wire-and-simd-is-declined.md)
  declines SIMD on this basis: parse is 0.62% of the application round trip.

### Boot C, 2026-09-18: the listener asked every 16th iteration takes 2.6 µs off the application round trip, and 0.3 µs onto the administrative one

`[measured 2026-09-18]` §9 desktop, `FIXBOLT_NIC=enp9s0 scripts/check-machine.sh` → `pass 16
fail 0 unknown 0` before every qualifying run, kernel 7.0.0-31, commit `55a1549`'s code (tree
clean except untracked docs), engine on `cpu6`, client on `cpu7`, **loopback, `hft`**, 10 runs ×
20 000 round trips per arm; procedure 1 ran N = 1, 16, 256 in that order, procedure 2 in reverse.
`N` is `Limits::listener_every` (`ListenerEveryTurns`,
[ADR-0069](decisions/ADR-0069-the-listener-is-polled-on-a-cadence-in-hft.md)): how many loop
iterations pass between two asks of the listener while the engine is busy. ns, p50 / p99 /
p99.9, procedure 1 ‖ procedure 2 (qualifying runs of 10). The whole reading:
[measured-costs](reference/measured-costs.md), *Boot C*.

| Loopback round trip, `hft` | N | procedure 1 | procedure 2 | reproduced? |
|---|---|---|---|---|
| TestRequest → Heartbeat | 1 | 16 070 / 21 050 / 22 753 (9) | 16 080 / 20 719 / 22 798 (10) | yes — ≤ 1.6% |
| | **16** | 16 381 / 21 591 / 23 695 (9) | 16 361 / 20 970 / 22 923 (9) | yes — ≤ 3.4% |
| | 256 | 16 376 / 21 410 / 24 060 (10) | 16 361 / 21 531 / 23 866 (9) | yes — ≤ 0.8% |
| NewOrderSingle → ExecutionReport | 1 | 20 209 / 25 188 / 27 202 (9) | 20 228 / 25 198 / 26 956 (8) | yes — ≤ 0.9% |
| | **16** | 17 644 / 22 763 / 26 535 (8) | 17 603 / 22 628 / 26 765 (10) | yes — ≤ 0.9% |
| | 256 | 17 608 / 22 603 / 27 242 (10) | 17 633 / 22 773 / 26 285 (10) | yes — ≤ 3.6% |

Three readings, and the decision:

- **The application path gains 12.7 ‖ 13.0% at p50 going from N = 1 to 16** — about 2.6 µs,
  the size of one non-blocking `accept4` with the socket file it allocates and frees (B9's
  profile), taken out from between two `recvfrom` calls. **N = 256 reads the same as 16 within
  0.3%**, so what the listener still costs at 16 is inside the noise, and moving it off the
  thread (ADR-0069 option d) is not worth a plan.
- **The administrative path loses 1.9 ‖ 1.7% at p50 at N = 16** — about 0.3 µs, reproduced,
  and **unexplained**. The candidate recorded in ADR-0069 (*Measured*) is the socket-lock /
  backlog path taken more often when `recvfrom` is polled without an `accept4` between; nothing
  was varied to test it. A cost stated is not a cost understood.
- **A connect is not visibly slower**: `connect-rtt` 68–85 µs and `logon-rtt` 1.07–1.12 ms in
  every run, no trend with N. Rule 4's gate stayed green with and without `--listener-every
  256`, `w2w` built with `affinity,tls`.
- **The default is 16** from the commit that publishes this block (ADR-0069 decision 2,
  revised): the application figure wins by more than ADR-0068's 5% in both procedures; the
  administrative cost is stated above; 16 rather than 256 because the worst-case accept delay
  is N iterations and nothing was gained by the larger N. The table at the top of §8 (N = 1,
  2026-09-02) is superseded for the application path by the N = 16 row here and is left in
  place as the figure it was.

### Boot C, over the cable: the first back-to-back wire figure — 27.1 µs admin, 28.9 µs application at p50

`[measured 2026-09-18]` §9 desktop, `FIXBOLT_NIC=enp9s0 scripts/check-machine.sh` → `pass 16
fail 0 unknown 0`, commit `ecfdd81`'s code (the Mac's `w2w` `2f93af5c`, built from `main` at
`ece17e7`), `enp9s0` (Intel I211, `igb`) ↔ a Mac mini over a direct cable, EEE off, IRQs on
`cpu4`, engine on `cpu6`, observer on `cpu7`, **hardware RX and TX stamps on the acceptor's NIC,
one PHC, no clock sync**; `hft`, **interval 0**, 20 000 requests per run, 10 runs per arm, two
procedures; every qualifying run's `--dump` join read `join: PASS` (ADR-0071 decision 3 — the
dropped samples did not move the counterparty's distribution). Wire figure = NIC-in of the
request → NIC-out of the reply, at the acceptor. ns, p50 / p99 / p99.9, procedure 1 ‖ 2,
qualifying runs of 10 in parentheses — a run over 0.1% TX-missing is disqualified, not fatal
([ADR-0071](decisions/ADR-0071-a-skipped-tx-stamp-is-a-missing-sample-not-a-failed-run.md)
decision 1 as revised):

| Wire round trip, `hft`, back to back | procedure 1 | procedure 2 | reproduced? |
|---|---|---|---|
| TestRequest → Heartbeat | **27 050** / **31 982** / 35 374 (6) | **27 114** / **32 254** / 49 146 (8) | **p50, p99 yes** (0.2%, 0.9%); p99.9 **no** (39%) |
| NewOrderSingle → ExecutionReport | **28 894** / **34 058** / 62 306 (6) | **28 878** / **34 110** / 88 630 (8) | **p50, p99 yes** (0.1%, 0.2%); p99.9 **no** (42%) |

Per-run p50 sat inside 27 034..27 178 ‖ 27 010..27 234 (admin) and 28 778..28 954 ‖
28 810..29 090 (application). **p50 and p99 are the figure; p99.9 is published as a pair that
did not reproduce and meets nothing** (ADR-0068 decision 3) — the tail over a cable is where the
I211's IRQ path and the Mac's unpinned generator show, and two procedures did not agree on it.
The counterparty's own table (the Mac, unpinned, its whole stack both ways) read p50 ~232 µs in
both arms and p99.9 0.37–0.95 ms: not this engine's figure, printed for the join only.

Three readings:

- **The wire adds ~11 µs to the loopback round trip** — admin 27.1 µs against 16.1 µs loopback
  (boot C N = 1), application 28.9 against 20.2 — which is the driver, the interrupt on `cpu4`,
  two PHY crossings and two frames on a 1 Gb/s wire (~1.9 µs each way for ~240 bytes), none of
  which loopback contains. §9's NIC rows are no longer `unknown` beside this figure.
- **The application path costs 1.8 µs over the administrative one on the wire**, against 3.9–4.1
  µs on loopback at N = 1 and 1.2 µs at N = 16: the wire figure was taken at `w2w`'s cadence of
  N = 1 (the C-89 trap) and still reads a smaller gap than loopback did, which is one more
  reading that item 49's remainder is not payload work.
- **Back to back is ~18 µs faster than paced at one message a second** (boot B's 45.1 ‖ 42.9
  admin, 49.6 ‖ 45.1 application): the hot figure beside the cold one, as ADR-0071 decision 2
  anticipated, and the reason both are published.

The sweep and the skip statistics — pacing 0–50 µs does **not** stop the I211 skipping stamps —
are in [measured-costs](reference/measured-costs.md), *Boot C*, C-40.

### The round trip under TLS, measured

`[measured 2026-09-14]` on the same §9 desktop — Linux `7.0.0-31-generic`, `check-machine.sh`
`pass 12 fail 0 unknown 1` in each procedure's header and straight after it — engine pinned to
isolated `cpu6` and the client to `cpu7`, medians of 20 runs of 20 000 timed round trips each (18
qualifying for `hft` application `off` in procedure 1), **over loopback**, with **TLS 1.3
`TLS13_AES_128_GCM_SHA256` at both ends of the socket** in every TLS arm: `tools/w2w` puts a
`TlsTransport` on its client as well as on the engine (`tools/w2w/src/main.rs`: `serve`'s
`EngineSide::Tls` arm on the engine side, `tls_arm::connect` on the client side). **The identical procedure ran twice in one boot**, 07:25–07:51 and
07:56–08:22; both are shown and neither is averaged, because an arm's p50 and two arms' p99 did
not reproduce. p50 · p99 · p99.9, in ns:

| Round trip | TLS | procedure 1 | procedure 2 |
|---|---|---|---|
| `hft`, TestRequest → Heartbeat | off | 16 065 · 20 804 · 22 232 | 15 670 · 20 439 · 22 037 |
| | kTLS | 25 298 · 37 345 ³ · 38 924 | 24 657 · 30 447 ³ · 37 872 |
| | userspace ¹ | 20 774 ² · 25 303 · 27 617 | **17 473** ² · 22 307 · 25 538 |
| `hft`, NewOrderSingle → ExecutionReport | off | 20 289 · 25 107 · 26 936 | 19 998 · 24 055 · 26 841 |
| | kTLS | 29 591 · 36 409 ³ · 42 571 | 29 501 · 39 029 ³ · 41 794 |
| | userspace ¹ | 21 641 · 26 385 · 28 198 | 21 596 · 26 500 · 28 238 |
| **`standard`** ⁴, TestRequest → Heartbeat | off | 19 522 · 24 286 · 25 879 | 19 151 · 23 895 · 25 388 |
| | kTLS | 29 135 · 34 475 · 40 547 | 28 840 · 34 300 · 40 031 |

Spread, the largest per-run p50 over the median: 1.003–1.014 across the arms of procedure 1,
1.005–1.019 across procedure 2.

¹ **Userspace `rustls` leaves the hot-path guarantee** (D11, ADR-0005 decision 3): `[measured
2026-09-14]` **80 000 allocations over 20 000 round trips**, both threads, in single runs on the
same boot between the two procedures, with no per-run quiet check; `off` and kTLS assert **0** in
every run of both procedures. These are the userspace mode's figures, not the engine's.
² **This arm's p50 moved 15.9% between two identical procedures** — 20 639 .. 20 930 across the
runs of procedure 1, whose spread read 1.008, and 17 333 .. 17 813 across procedure 2's, spread
1.019 — while every other arm's p50 moved 0.2–2.5%, all faster. Its p99 moved 11.8% and its
p99.9 7.5%. No cause is claimed:
[reference/a-tight-spread-inside-one-procedure-did-not-reproduce-across-two.md](reference/a-tight-spread-inside-one-procedure-did-not-reproduce-across-two.md),
`STATUS.md` open item 85.
³ **The kTLS p99 did not reproduce either.** `hft` administrative: 37 345 → 30 447 (−18.5%);
procedure 1's per-run p99 fell in two clusters, 8 runs at 30 928–32 141 and 12 at 37 220–37 841,
and procedure 2's 20 runs all read 29 887–30 699. `hft` application: 36 409 → 39 029 (+7.2%). The
p50 of both arms moved under 2.6%. No cause is claimed.
⁴ **`standard` rows fill no row of the `hft` stage table below**, and the `tls` plan's §6.7 item 6
said they would not be published in §8 at all; they are, labelled by mode (ADR-0013 decision 4),
because they were measured for the other half of the rule — a transport change proven in one mode
is proven in neither (`CLAUDE.md` §7). The plan's delivery log declares it as a deviation. Never
compare a `standard` figure with an `hft` one except as a difference.

`scripts/w2w-baseline.sh` with the eight arms named in `ARMS` is the procedure, and
[reference/measured-costs.md](reference/measured-costs.md), *TLS on the wire*, is the whole
reading, with both procedures' summary blocks verbatim. **The plain `off` arms differ from the
first table by at most 2.1% at p50, 2.4% at p99 and 3.0% at p99.9** — on a newer kernel patch level
and newer code, so it is agreement rather than a controlled repeat.

Three readings:

- **kTLS was the slower of the two in both `hft` paths, in both procedures, at p50, p99 and
  p99.9.** At p50 it added **9.0–9.5 µs** over `off` in `hft` and 9.6–9.7 µs in `standard`;
  userspace `rustls`, run in `hft` only, added **1.8–4.7 µs** on the administrative path and
  **1.4–1.6 µs** on the application path. **What this does not show**: anything about a NIC —
  over loopback neither arm has hardware offload to use (`ethtool -k lo`, below) — or about another
  kernel, or any cause; nothing was isolated. It does not by itself reverse D11, whose case for
  kTLS is the hot-path guarantee (D8, parse-in-place, no allocation), and the kTLS arm still counts
  zero allocations. Whether `hft` should still prefer kTLS is a design question, `STATUS.md` open
  item 84.
- **The TLS figure is a round-trip delta with TLS at both ends, not a stage cost.** No benchmark
  here isolates one end's record processing, which is why the stage table below carries the delta
  and not a decrypt figure.
- **One twenty-run median is one observation.** The userspace administrative p50 and the kTLS p99
  this procedure would have published, run once, depended on which half hour it ran in (² and ³
  above).

### Boot C, mixed TLS: the engine's own kTLS costs ~0.4 µs over userspace; the client's kTLS costs the rest

`[measured 2026-09-18]` same §9 desktop, `pass 16 fail 0 unknown 0`, commit `85460c1`'s code,
loopback, **`hft` admin**, **`tools/w2w` at listener cadence N = 1** (its `--listener-every`
defaulted to 1 and did not follow `Limits`' new default of 16 — the trap under *Boot C* above),
10 runs × 20 000 per arm,
two procedures in opposite arm order — every arm reproduced (largest difference 2.9%, at
p99.9). `tools/w2w` now takes a TLS mode per end (`--client-tls`), so the two mixed arms split
the 9 µs the 2026-09-14 table could not. p50 ns, procedure 1 ‖ procedure 2; *added* is against
the same procedure's `off` arm at the same cadence (boot C N = 1: 16 070 ‖ 16 080):

| engine | client | p50, proc 1 ‖ 2 | added over `off` |
|---|---|---|---|
| kTLS | kTLS | 25 503 ‖ 25 473 | **+9 433 ‖ +9 393** |
| userspace | userspace | 20 824 ‖ 20 719 | +4 754 ‖ +4 639 |
| **kTLS** | userspace | 21 175 ‖ 21 250 | +5 105 ‖ +5 170 |
| userspace | **kTLS** | 22 177 ‖ 22 147 | +6 107 ‖ +6 067 |

Three readings ([ADR-0070](decisions/ADR-0070-ktls-stays-the-hft-steady-state-for-the-guarantee-not-for-latency.md)
decision 5):

- **The engine's kTLS against the engine's userspace `rustls`, same userspace client: +351 ‖
  +531 ns** — roughly 0.4 µs, about 2% of the round trip. That is the engine-side price of the
  mode the design ships, and it is small.
- **The client's kTLS against the client's userspace, same userspace engine: +1 353 ‖
  +1 428 ns** — `w2w`'s client is a plain blocking thread, and its kernel record layer costs it
  three to four times what the engine's costs the engine.
- **Both-kTLS is superadditive**: the sum of the two one-sided costs on top of userspace/userspace
  predicts 22 528 ‖ 22 678; measured 25 503 ‖ 25 473 — **~2.8–3.0 µs more than the parts**. Two
  kernel record layers on one loopback pair interact (a candidate: each end's `recvmsg`
  decrypts on the calling CPU only when the whole record has arrived, so the two ends serialise
  where userspace pipelines); not isolated, recorded as a candidate.

**The mixed arms print their allocations and do not assert them** (4 001 on the client thread
in the userspace-client arms — the client's own `rustls`, not the engine); the symmetric arms
keep their assertions. The 2026-09-14 table above stands as the both-ends figure it was.

### Stage by stage (`hft`, N = 1)

**The Logon hop is not in this table, by decision.** A connection's first message pays the
pre-session stage once — the `Logon` is read there, routed to an engine over a channel, and the
socket handed across threads — and that cost is off the message path: bounded by the
`presession` bench (426.2 ns per socket sweep, ~84 ns to read both comp IDs and pick a shard,
`[measured 2026-09-01]`) plus one cross-thread wake, which §9 already keeps off the engine
core. It is not measured, and becomes a measurement only if a user reports Logon latency
([ADR-0074](decisions/ADR-0074-kernel-bypass-io-uring-and-the-logon-hop-stay-unmeasured-by-decision.md)
decision 3).


| Stage | Cost | Who controls it |
|---|---|---|
| NIC → kernel → socket buffer | 3–8 µs, from the literature | kernel, IRQ affinity, driver |
| TLS record processing, if enabled | `[measured 2026-09-14]` **not separable by stage.** The round trip with TLS at **both** ends, over loopback on the §9 desktop, adds at p50 over `off`, in `hft`, **~9.0–9.5 µs under kTLS** and **~1.4–4.7 µs under userspace `rustls`** (*The round trip under TLS, measured*, above; both procedures). **kTLS was the slower of the two on loopback, in both `hft` paths**, with no cause claimed. kTLS: crypto in the kernel, **no extra userspace copy — a statement about copies, not about speed**. `[read 2026-09-14]` by the manager on this desk, `7.0.0-31-generic`: the highest-priority `gcm(aes)` in `/proc/crypto` is `generic-gcm-aesni-avx` (priority 500; `generic-gcm-aesni` is 400) — which one the kernel's TLS bound at runtime was not read; `ethtool -k lo` reads `tls-hw-tx-offload: off [fixed]`, `tls-hw-rx-offload: off [fixed]`, `tls-hw-record: off [fixed]`. Userspace: one copy each way plus allocation (D11) | this design, and the kernel |
| Wakeup, `standard` | 2–5 µs, `epoll`-class, from the literature; the core is given back | this design, D8 |
| Wakeup, `hft` | `[measured 2026-08-31]` `Engine::turn` **~449 ns × N**, N = sockets on the thread; a core is burned. ~670 ns on a core carrying `nohz_full`, which §9 no longer asks for (ADR-0021) | this design, D8, `benches/turn.rs` |
| Parse (D2) | `[measured 2026-09-05]` **0.12 µs** for a `NewOrderSingle`, **0.06 µs** for a `Heartbeat`: framing, field indexing, `9=` and `10=` only. `benches/parse.rs` parses with `NoDict`, so this row is **not** the dictionary pass — that is the row below. `[measured 2026-09-05]` the `Heartbeat` figure was 56.3 ns until its fixture was corrected; it declared `9=49` against a body of 51, so the parse returned before its own checksum ([a benchmark parsed a message the parser rejects](reference/a-benchmark-parsed-a-message-the-parser-rejects.md)) | this design |
| Dictionary pass (D1) | `[measured 2026-09-05]` **0.90 µs** for a `NewOrderSingle`, **0.22 µs** for a `TestRequest` — every field asked `is_defined_tag`, `field_type`, `allows`, `enum_allows` and `accepts`, a duplicate check that rescans the index, then `view.get(tag)` once per required header and body tag, each a linear scan. `crates/session/benches/validate.rs` through `fixbolt_session::validate` ([ADR-0050](decisions/ADR-0050-the-dictionary-pass-is-public-so-it-can-be-timed.md)). **This row is the single largest piece of user-space work in the table, and it is seven times the parse it follows** | this design |
| Session machine (D1), everything else | ~0.1 µs `[unmeasured]` | this design |
| Dispatch, inline vs ring (D4) | `[measured 2026-09-01]` **0.0085 µs** inline vs **0.27 µs** ring one way | the application's choice |
| Serialise, template (D9) | `[measured 2026-08-31]` **0.24 µs** (239.1 ns); carried as ~0.05 µs from the literature until ADR-0016 | this design |
| `send` syscall → NIC | 3–10 µs, from the literature | kernel |
| **Floor** | **~10–20 µs** | kernel |
| **User-space work only, application message** | `[measured 2026-09-05]` **~1.36 µs** at N = 1, inline: parse 0.120 + dictionary pass 0.897 + session ~0.1 + dispatch 0.0085 + serialise 0.233 | the half that was always cheap, and was not |
| **User-space work only, administrative message** | `[measured 2026-09-05]` **~0.4 µs**: parse 0.060 + dictionary pass 0.218 + session ~0.1 + the `Heartbeat` serialise, which has no committed case | |
| **Everything this design controls, N = 1** | **~1.81 µs** application, **~0.85 µs** administrative: the rows above plus one turn | |
| **Everything this design controls, N sessions** | **~1.36 µs + N × 449 ns** application | |

`[2026-09-05]` **The user-space total was ~0.46 µs on this page until the dictionary pass was
timed, and it was wrong in two ways at once**: it added a `NewOrderSingle`'s parse to an
`ExecutionReport`'s serialise and compared the sum against an *administrative* round trip, and
its `session ~0.1 µs` silently stood in for a pass that costs 0.90 µs on an application
message. The 2.9%-of-the-total reading above is the same arithmetic and now reads **8.5%** for
an application message on the application round trip — still small against a 10–20 µs floor,
which is why nothing in the design moves, but it is three times what this page said.

### The 3 898 ns, added back

`[measured 2026-09-05]` `--path app` costs 3 898 ns more than `--path admin` at p50. What
`--path app` **adds**, from committed benchmarks on this box, `pass 12 fail 0 unknown 1`:

| What the application path adds | ns | Case |
|---|---|---|
| inbound parse, a `NewOrderSingle` instead of a `TestRequest` | ~**+60** | `parse NewOrderSingle (validated)` 120.0 − `parse Heartbeat (validated)` 60.4. **Nearest committed cases, not `w2w`'s exact bytes** — 15 fields against 8, where `w2w` sends 16 against 9 |
| **the dictionary pass, the same substitution** | **+679** | `validate NewOrderSingle, w2w bytes` 897.3 − `validate TestRequest, w2w bytes` 218.4. These two cases *are* `w2w`'s exact bytes, copied field for field, which is the only reason the subtraction is allowed |
| dispatch to the application | +9 | `inline deliver + reply` 8.5 |
| the application's own parse | +114 | `parse NewOrderSingle (no checks)` 114.1 — `w2w`'s `Desk` re-parses with `Validation::NONE`, since the session already validated |
| the application's template patch and encode | +233 | `encode ExecutionReport (template)` 232.8 |
| **kernel copies of the bigger payload, both directions** | **+24.5** | `[measured 2026-09-05]` `TCP loopback` slope, `(14 890.9 − 12 528.8) / 16 368` = 0.1443 ns/byte, times the 170 extra bytes. **Read off the 8 → 8192 lever, not off the two real sizes** — those differ by tens of ns inside 12 600 and their direct difference read −4, +13 and +46 ns over three repetitions |
| **`Journal::put` of the reply into the ring** | **+8.9** | `journal put, 191 bytes, walking`. The administrative path never does it, so the whole figure counts |
| **measured subtotal** | **~1 128** | **28.9% of the gap** |
| the session's own `Heartbeat` serialise, which the application path does *not* do | −? | **no committed case**, so it is not subtracted |
| the kernel's own share, **measured in situ** rather than by slope: `sendto` and `recvfrom` on the engine tid, application minus administrative | **~0 — ≤ 50 ns, of both signs** | `[measured 2026-09-18]` boot C step C-49, `perf trace -s` over whole `hft` runs (the tracer inflates every syscall ~2× — p50 under trace 35.2–35.8 µs on *both* paths — so only differences count), 22 001 `sendto` per run: admin 186.608 / 186.508 ms total (avg 8.48 µs), app 187.663 / 183.876 ms (avg 8.53 / 8.36 µs); `recvfrom` avg 2.47 µs admin, 2.45 µs app. **The kernel does not charge the application path for its bigger payload at this instrument's resolution.** This confirms the +24.5 ns slope row and retires "kernel work proportional to payload" as a candidate for the remainder |
| **still unattributed** | **~2 770** | **71.1%** — and, `[2026-09-18]` after C-49, **not the kernel's payload work, not the client's loop (identical on both paths), not `Journal::put`, not the dictionary pass beyond its 679 ns.** What is left is ~3.1 µs *outside* one engine turn (D_in is 765.5 ns) and outside proportional syscall cost. It is published as unattributed **by decision** ([STATUS.md](../STATUS.md) item 49's closing row): three probes have retired the named candidates and each cost a boot. The one probe that would split it is named and not scheduled — software `SO_TIMESTAMPING` stamps (TX/RX software, which loopback supports) on **both** sockets, so a round trip reads as four segments: client stack out, engine-side dwell (socket in → user → socket out), engine stack out, client stack in. `w2w --wire-timestamps` already parses the cmsg (software stamp = `ts[0]`); it is the `--stamp software` arm of a later boot, if anyone needs the split |

**The largest candidate this page named turned out to be a sixth of the answer.** STATUS item
39 wrote the dictionary pass down as the leading explanation for the 3 898 ns and it is
**17.4%** of it. That is what the item asked for and it is not a cause; the arithmetic is here
so that nobody has to take the size of a number for an explanation again.

**Four candidates were named for the remainder. `[measured 2026-09-05]` two are now priced and
both are noise, and the remainder barely moved — ~2 804 to ~2 770 ns.**

| Candidate | Verdict |
|---|---|
| Two kernel copies of a larger payload each way, and the client's blocking `read` on a bigger message | **Dead: ~25 ns, 0.9%.** And three of the four byte counts this table used to quote were wrong — `strace` on the release binary reads **83/87** administrative and **149/191** application, not "79 and ~70" against "149 and ~200" |
| `Journal::put` of the outbound `ExecutionReport` | **Dead: 8.9 ns.** Confirmed in situ as well: one session, identical work, the ring swept 8 → 64 → 512 → 4 096 slots (4 KiB to 2 MiB) reads **1 745.8 / 1 766.6 / 1 778.3 / 1 775.9** `[measured 2026-09-23, Boot E, S4 re-record]` (2026-09-05 origin: 1 659.8 / 1 635.5 / 1 654.8 / 1 657.7) — a 1.9% spread that is **not monotone** |
| The engine's framing and read-buffer management | **Open, and now holds almost all of it.** No benchmark isolates it |
| The session's own `Heartbeat` serialise on the administrative side | **Open.** No committed case, so it is not subtracted in either direction |

**`[measured 2026-09-18]` The engine-turn lines of 2026-09-05 read 7–10% under today's code,
and the baselines are deliberately not moved.** Boot C step C-91 ran the commit that recorded
them (`0149b26`) beside `85460c1` in one boot: `engine turn, 1 busy sessions` 1 660.2 → 1 817.2
ns (+9.5%), every engine-turn case +6.7–9.8%, the kernel unchanged as a cause. C-91b bisected
it: **`588b350`** (`52=` read at every precision) is the first commit over a 5% line, worth
~+33 ns, and the rest is **four more steps of 2–3% each** spread over 2026-09-06 → 09-18
([measured-costs](reference/measured-costs.md) *Boot C*, C-91 and C-91b; STATUS items 91, 93).
The `density` and `validate` baselines stay at their 2026-09-05 values until each step has a
name and a decision — a baseline re-recorded to cover a regression is the failure ADR-0016 was
written against — so `bench.sh --strict` reads those cases *over baseline* on purpose until then.

**`[measured 2026-09-15]` The engine's whole share, measured in one piece: D_in = 765.5 ns.**
`engine turn, 1 busy sessions` − `engine turn, 1 busy, admin` (`crates/engine/benches/density.rs`,
module doc *The administrative twin*): median 1 719.7 − 954.2 over the same 20 runs, paired
per-run median 767.7 ns (734.3 .. 781.3). §9 desktop, `FIXBOLT_NIC=enp9s0
scripts/check-machine.sh` → `pass 15 fail 0 unknown 0`, HEAD `f43d7e8`; boot B step B1 of
[the-second-linux-desk](plans/2026-09-04-the-second-linux-desk.md). Both cases run the same
engine with no kernel underneath, so D_in holds everything the process does differently —
framing, read-buffer management, session, dispatch, the application, serialise, and the
administrative side's own `Heartbeat` — and nothing a socket does. Two arithmetic facts, **no
cause claimed**:

- **The in-process rows above add to more than D_in** — ~1 104 ns without the kernel row, against
  765.5. They were measured as separate cases, and their inputs moved since: `validate
  NewOrderSingle, w2w bytes` reads 994.5 today against the 897.3 the dictionary row used. They are
  not independent terms that add.
- **Against the 3 898 ns, ~3 130 ns is outside one engine turn** — kernel, copies, syscalls, the
  client and the read loop's wakeups. That makes *the engine's framing and read-buffer management*,
  which the table above says holds almost all of the remainder, **at most part of 765.5 ns**. Two
  conditions stand: the 3 898 ns was measured on the 2026-09-05 code, and boot B's B2 re-measured
  both paths — `hft` app − admin p50 reads **4 198 ‖ 3 802 ns** (procedures 1 ‖ 2, from arms that
  did not reproduce), stated beside the 3 898 ns above; and `Feed`'s and `AdminFeed`'s bytes are
  not asserted to be `w2w`'s.

**Twice now the largest named candidate has not been the answer.** The dictionary pass is real,
is the biggest single row on this page, and is 17.4% of what it was nominated to explain; the
payload term was the intuitive one and is 0.9%. STATUS item 49.

**Which mode the stage table is about: `hft`.** `standard`'s round trip is measured and is in
the first table; what is unmeasured is its stage breakdown, because the wakeup is one opaque
term. A `standard` figure and an `hft` figure are not comparable and must not be quoted as if
they were; they are comparable as a *difference*, which is the one thing a difference is for
(ADR-0013 decision 4).

**The poll row is `Engine::turn`, and it carried a number nobody expected.** `[measured
2026-08-31]` `crates/engine/benches/turn.rs` measures the real sweep over real sockets: 449 ns
per session, flat from 1 to 16 within 2%, of which ~420 ns is the `recv` syscall. The same
benchmark under `taskset` found that the core §9 used to recommend was 36% slower at that
syscall, and a second boot separated the three isolation options: `isolcpus` 494.8 ns,
`rcu_nocbs` 498.2, untouched 501.8, `nohz_full` 670.7. Naming any CPU in `nohz_full` also costs
about 45 ns per kernel entry on every other CPU, so removing it moved the untouched cores too,
and the row settled at 449 ns with 24 fresh baseline runs (ADR-0021).

**Why serialise costing 239 ns rather than 60 changed nothing that matters.** The user-space
rows total ~0.46 µs against a floor of 10–20 µs, so 180 ns moves the wire-to-wire figure by
under 2% of the floor. That is why §6 could withdraw the target without the design changing.
**At N = 2 the polling sweep alone exceeds the whole user-space budget**, which is the sentence
this table did not contain until the poll was measured, and which ADR-0012 settles: one
session per polling thread is the shape this table describes, `density` carries the
`N × 449 ns` term, and no latency figure is published without its `N`.

**The TLS row is filled, `[measured 2026-09-14]`, on the condition ADR-0005 decision 5 set for
it**: `tools/w2w` ran the same load three ways, TLS off, kTLS and userspace `rustls`, on one Linux
box — the §9 desktop — and the figures are *The round trip under TLS, measured*, above. What the
row holds is a round-trip delta with TLS at both ends, not the per-message decrypt cost the row
was drawn for — it was named *TLS record decrypt* until this measurement — and on loopback kTLS
was the slower of the two modes. Going below the floor means kernel bypass, which is L0's job
behind a feature flag that actually gates (D5), and is not v1.

## 9. Deployment — the OS is part of the design

"Rust has no GC" does not mean "no jitter". p99.9 on a correct engine is usually lost to the
machine, not the code. None of this is optional for a latency measurement to mean anything.

| Setting | Why |
|---|---|
| **The machine is not a guest** | Governor, turbo, C-states, SMT and NIC IRQ affinity are **host** properties. A VM cannot set them and does not fail them loudly; the `/sys` files are simply absent. `check-machine.sh` reports `systemd-detect-virt` and steal time; a guest is a **FAIL**. Development may move to a cloud VM; measurement cannot |
| **Nothing else is running on the machine** | `[measured 2026-08-30]` the row that dominates every other row. On the Ryzen 7 3700X, all six tuning rows below move the `ring, one way` median by **0.8%** (260.6 → 259.7 ns); competing CPU load moves it by **71%** (262 → 449 ns) and takes a second mode near 324 ns from ~5% to 92% of samples. `check-machine.sh` reads CPU busy over a one-second window and **FAILs above 3%**, naming the processes |
| **No systemd timer due inside the campaign window** | `[measured 2026-09-22]` boot D lost rounds 13–20 to `apt-daily-upgrade.timer` at 06:51 while the quiet row read green before and after; a quiet check reads one second and says nothing about what systemd has already scheduled. `check-machine.sh` reads `systemctl list-timers --all --output=json` and **FAILs** on any timer due inside `FIXBOLT_TIMER_WINDOW` hours (default 12, the longest campaign on record), naming the units; the fix is `systemctl stop`, not `disable`, so the next boot restores them. `ab-rotation.sh` refuses to start on that FAIL ([ADR-0093](decisions/ADR-0093-a-campaign-driver-is-committed-sudo-in-a-committed-script-names-what-root-can-find-and-a-timer-due-inside-the-window-is-a-fail-row.md)) |
| `isolcpus` + `rcu_nocbs` for the engine core, **and the engine thread pinned to it** | No other tenants and no RCU callbacks on the engine core. `[measured 2026-08-31]` free: 494.8 ns and 498.2 ns per turn against 501.8 untouched. `[measured 2026-09-02]` worth **11× at p99.9 and nothing at p50**, wire-to-wire, application path, one variable, both arms inside one CCD: p50 19 968 against 19 407 (the isolated core is 2.9% *slower*), p99.9 **26 300 against 266 887**, and 293 749 with no pinning at all. A 20 000-sample benchmark of a 500 ns operation could not see it; the excursion is 250 µs long |
| **CPU speculation mitigations IN FORCE** | `[measured 2026-09-01]` the single largest term in this design's budget. Turning them off makes every syscall **59–63%** cheaper: `Engine::turn` 448.9 → 175.2 ns, `recv` 420.5 → 156.9, while thirteen pure user-space benchmarks move −4.1% to +4.1% with no direction. All of it is `retbleed`'s untrained return thunk plus `spec_rstack_overflow`'s Safe RET; `vmscape` costs nothing. **This row requires them ON**, because `baselines.tsv` was recorded with them and a machine without them is not comparable. It is not advice to disable them ([ADR-0023](decisions/ADR-0023-section-9-records-the-cpu-mitigations.md)) |
| `nohz_full`: **NOT recommended** | `[measured 2026-08-31]` 160 ns on every kernel entry, 670.7 ns per turn against 494.8. What it buys is the far tail only: p50 376 against 216, p99 376 against 224, p99.9 384 against 224, and it wins from p99.99 outward (504 against 2 848). A busy `hft` engine makes ~2 000 000 kernel entries per second and this removes ~1 100 excursions of 3 µs: 0.32 s of tax against 0.0033 s of tail. Take it only for a p99.99 objective ([ADR-0021](decisions/ADR-0021-nohz-full-leaves-section-9.md)) |
| IRQ affinity: NIC queue → a core that is *not* the engine core | The engine never takes an interrupt. `scripts/check-machine.sh` judges this row for real, not just reports it, once a NIC is selected (`FIXBOLT_NIC=<name>`, or auto-selected by `pick_nic`: a physical wired device — `type` 1, a bus `device`, not wireless — with carrier breaking a tie only, and printed on the header's `nic` line; `[2026-09-15]` it used to select by carrier, and a four-second link bounce silently dropped the NIC rows — [a-machine-check-narrowed-its-own-scope-when-the-link-bounced](reference/a-machine-check-narrowed-its-own-scope-when-the-link-bounced.md)): it reads every `/proc/irq/<n>/smp_affinity_list` naming that NIC and checks it against `/sys/devices/system/cpu/isolated`, and separately checks that `irqbalance` is not active — an active `irqbalance` redistributes IRQs regardless of any pin written here. Without a NIC selected, the row stays the `? ? ?` it always was |
| Interrupt coalescing **off** | A NIC batching interrupts (`rx-usecs > 0`) trades latency for fewer interrupts — the wrong trade on the engine's receive path. `scripts/check-machine.sh` judges it once a NIC is selected (same selection rule as the IRQ affinity row above): `ethtool -c <nic>`, PASS when `rx-usecs: 0` |
| EEE (802.3az) **off** on the measurement NIC | A NIC in Low Power Idle must wake its link partner before it can send, ~16.5 µs at 1000BASE-T, and that wait sits inside the acceptor's NIC-in → NIC-out window. `[measured 2026-09-15]` one A/B on the §9 desktop, `enp9s0` (I211) cabled to a Mac, `hft` admin paced at 1 s, same hour: **EEE on added +14.6 µs to wire p50** ([measured-costs](reference/measured-costs.md), boot B — the raw pair is A/B only, never a figure). `scripts/check-machine.sh` row `eee`, once a NIC is selected: `ethtool --show-eee <nic>`, PASS on `EEE status: disabled`, FAIL on `enabled - active` and `enabled - inactive` (inactive only means the far end does not advertise it today), UNKNOWN when there is no status to read. `--set-eee` bounces an `igb` link for ~4 s: wait for `Link detected: yes` before measuring |
| `mlockall` + pre-faulted buffers | No page fault on the hot path. The reference project's `pool.rs` touches every page at startup; copy that |
| Transparent huge pages **off** | THP compaction stalls are multi-millisecond |
| CPU frequency governor `performance`, C-states off | A core waking from C6 costs ~100 µs |
| `SO_BUSY_POLL` / `net.core.busy_poll` | Lets the kernel's own receive path spin instead of sleeping. `scripts/check-machine.sh` also reads `net.core.busy_read` once a NIC is selected (same selection rule as the IRQ affinity row above), naming both values; without a NIC selected it reads `net.core.busy_poll` alone, as before |
| **If TLS is on:** a kernel that carries the negotiated cipher suite in kTLS | kTLS support is narrower than what `rustls` will negotiate. A session that negotiates outside it drops silently to the userspace path and off the hot-path guarantee (D11). **Which kernel and which suites — ADR-0005 open question 2, answered at the level measured.** `[measured 2026-09-14]` `TLS13_AES_128_GCM_SHA256`, the only suite this engine offers (`crates/engine/src/tls.rs`, `offloadable_provider`), is taken by the kernel on **`7.0.0-31-generic`**: every kTLS run on the §9 desktop read back `tls: kernel`. **Not measured**: any other suite — the `tls` plan's optional suite step did not run — and the minimum kernel. **On the CI runner**, the `tls` job (*TLS, with the kernel it needs*, job `103750342469` of run [`34767259852`](https://github.com/tmthang86/fixbolt/actions/runs/34767259852), commit `1178f4d`) asserts that the kernel took `TLS13_AES_128_GCM_SHA256` keys — `crates/engine/tests/tls.rs:40-45` narrows to that suite and `:488-497` requires `/proc/net/tls_stat` `TlsTxSw` and `TlsRxSw` to move — and its log reads `verdict: READY` and every test `ok`. **That job prints no kernel version.** The runner kernel `6.17.0-1022-azure` is recorded on 2026-09-10 ([CONFORMANCE.md](CONFORMANCE.md) §8) and, in the same run, by another job (*The machine probes reach the right verdicts*, `kernel: Linux 6.17.0-1022-azure`) — not by the job that took the keys. That gap is the residue |

A latency number published without stating which of these were set is not a number.

### Checking the machine

`scripts/check-machine.sh` reads every row off the running machine and prints `PASS`, `FAIL`
or `? ? ?` for each, with the command that fixes a failing one. It reads only; applying these
is root, machine-specific, and belongs to the person at the box. `unknown` is deliberately
**not** a pass: a container that cannot read `/sys` must not be able to look like a tuned
host.

`[2026-09-14]` step A5 of
[plans/2026-09-04-the-second-linux-desk.md](plans/2026-09-04-the-second-linux-desk.md):
`check-machine.sh` also judges NIC IRQ affinity, interrupt coalescing (`rx-usecs 0`),
`irqbalance` inactive and `net.core.busy_read`, once a NIC is selected — `FIXBOLT_NIC=<name>`,
or auto-selected as the first interface that has carrier, excluding `lo`, `tailscale*`,
`docker*`, `veth*`, `br-*` and wireless `wl*`. Without `FIXBOLT_NIC` and without any such NIC
carrying a signal, the output is unchanged from before this step: the NIC IRQ affinity row
stays the same `? ? ?` it always was, no new rows print, and the string `pass 12 fail 0
unknown 1` recorded in `benches/baselines.tsv` still names the loopback machine it always
named.

```
scripts/check-machine.sh          # what is in force, and how to fix what is not
scripts/bench.sh                  # counts and A/B comparisons, on any machine
scripts/bench.sh --strict         # refuses unless check-machine.sh is clean
```

`--strict` is what makes non-negotiable 10 real: it fails before it looks at a single ceiling
if the machine is not set up. Without `--strict` the run is still useful: allocation counts are
machine-independent, and an A/B comparison on the same box is valid whatever the box is.
`[measured 2026-08-30]` on the shared container this repository was developed in, the script
reports `pass 1 fail 5 unknown 3` and `bench.sh --strict` refuses, which is the correct answer
for that machine.
