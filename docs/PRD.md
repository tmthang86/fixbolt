# fixbolt — Product Requirements

What this engine is for, what ships in which phase, and, stated plainly, **how far it is from
QuickFIX**, which is the only honest baseline for a FIX engine.

[DESIGN.md](DESIGN.md) says *how* it is built. This page says *what* it must do and *when*.
Decisions live in [decisions/](decisions/); what is about to be built lives in [plans/](plans/);
where the work stands today is [STATUS.md](../STATUS.md).

**Evidence marks used throughout:**
`[measured]` counted or run in this repository on the stated date ·
`[documented]` read off another project's own documentation, not verified here ·
`[unproven]` neither; an intention.

---

## 1. Who this is for

### Two modes, and `standard` is the default

[ADR-0013](decisions/ADR-0013-two-modes-standard-and-hft.md):

| Mode | For | Buys | Costs |
|---|---|---|---|
| **`standard`** (default) | anybody, any OS, any hardware, a container, a laptop | portability, and the core back: it blocks when idle | the microsecond |
| **`hft`** (opt-in, Linux only) | a tuned box with isolated cores | the microsecond | a core burned per polling thread, and a machine that satisfies [DESIGN.md §9](DESIGN.md) |

An engine whose default configuration pins a core at 100% looks broken to most people who try
it. So `hft` is the claim and `standard` is the front door. Every figure this project
publishes names its mode, its session count and its machine.

### Inside `hft`, latency wins over session density

[ADR-0012](decisions/ADR-0012-latency-first-and-one-session-per-polling-thread.md): when
latency and sessions-per-core conflict inside `hft`, latency wins, and reversing that needs
its own ADR. The shape `hft` is optimised, budgeted and measured for is **one session on one
isolated polling thread**. Inside `standard` the tie-breaker is portability, then giving the
core back; many sessions on one blocked thread is simply how it runs.

The reason is arithmetic. `[measured 2026-08-31]` an idle `Engine::turn` is one non-blocking
`read` per connection and costs **449 ns per session** on a core set up to §9, flat from 1 to
16 sessions within 2%. A message waits up to one whole sweep before it is seen, so two sessions
on one polling thread already exceed the whole user-space budget in [DESIGN.md §8](DESIGN.md).
(The figure was 703 ns until 2026-08-31; that was a C program's bare `read` on a `nohz_full`
core, and §9 no longer asks for `nohz_full`:
[ADR-0021](decisions/ADR-0021-nohz-full-leaves-section-9.md),
[reference/measured-costs.md](reference/measured-costs.md).)

### Users

| User | What they need | Served in |
|---|---|---|
| A firm on a latency-critical path to a venue | An acceptor or initiator on a dedicated polling thread, budgeted end to end | Phase 1: **the shape this engine is built for** |
| A venue or broker running a FIX gateway | Many sessions per core, the `density` shape. Supported, with its own budget of `N × 449 ns` per polling thread where `N` is sessions **per shard** ([GUIDE.md §1a](GUIDE.md)). `fixbolt_engine::shard` gives M pinned threads of N sessions each, and each socket is routed to its shard by the identity in its Logon, so the single-logon rule holds across shards | Phase 1 |
| A firm connecting out to venues | An initiator with reconnect, schedules and sequence persistence | Phase 1 ([ADR-0004](decisions/ADR-0004-bidirectional-engine.md)) |
| A team building a simulator or a QA exchange | Both roles, plus a dispatch mode that survives an application that blocks | Phase 1 ([ADR-0002](decisions/ADR-0002-engine-library-split.md)) |
| A market-data consumer | Binary encodings: SBE in phase 2; FAST is out of scope for phase 2 ([ADR-0078](decisions/ADR-0078-sbe-enters-as-an-encoding-without-a-session-and-fixp-is-its-own-phase.md) decision 4) | Phase 2 for SBE |
| A post-trade or clearing integration | FIXML, out of scope for phase 2 (ADR-0078 decision 4) | Not scoped |

**Every latency figure names its session count.** A figure without `N` is not a figure. Every
number published before 2026-08-30 was taken at N = 1 in benchmarks holding no socket, which
is the best case, and it was not labelled as such.

**The differentiator is the acceptor.** `[documented]` As of 2026-08-27 the Rust ecosystem
has no production-proven FIX acceptor: `hotfix` and `IronFix` are initiator-only, `ferrumfix`
calls itself *"wildly unstable"*, and the `quickfix` crate is a binding to C++
([reference/prior-art.md](reference/prior-art.md)).

## 2. Phases

### Phase 1: a FIX 4.4 engine you can deploy — complete

Tag=value, both roles, 59 / 59, repeating groups, full dictionary validation. Every component
below is built and merged.

| Component | Done | Notes |
|---|---|---|
| `codec` + `dict` | 2026-08-28 | |
| conformance runner | 2026-08-28 | |
| `session`, acceptor role | 2026-08-29 | 59 / 59 |
| `session`, initiator role | 2026-09-02 | Six operator-ordered sends; interop-green against `libquickfix` ([ADR-0042](decisions/ADR-0042-a-second-implementation-is-the-only-independent-opinion.md)). The mirrored corpus is the secondary gate at 10 / 50, ceiling 45 in doubt (STATUS item 36) |
| `engine`: accept and connect drivers, journal, dispatch, backpressure | 2026-08-30 | |
| `tools/w2w` | 2026-08-30 | Wire-to-wire harness; the binary the two mode checks trace |
| many counterparties | 2026-09-01 | `presession::Registry` / `Table` ([ADR-0026](decisions/ADR-0026-a-counterparty-registry-in-the-pre-session-stage.md), [ADR-0030](decisions/ADR-0030-one-engine-holds-many-counterparties.md)); a configuration file since 2026-09-02 ([ADR-0040](decisions/ADR-0040-a-configuration-file-refuses-what-it-does-not-understand.md)). Still missing, deliberately: credentials (ADR-0026 decision 3 makes `lookup` the only auth hook) and reload while running |
| session schedules | 2026-09-02 | Daily, weekly, weekday filter, windows across midnight, reset decided by `same_session` ([ADR-0033](decisions/ADR-0033-a-schedule-is-utc-arithmetic-and-the-calendar-stays-outside.md)). UTC only; timezone names are the caller's. `last_active_ms` is persisted ([ADR-0039](decisions/ADR-0039-a-fresh-journal-is-the-deployments-to-build.md)) |
| operability | 2026-09-02 | Snapshot, health probe, event stream with the drop reason, sequence-number admin, offline journal reader, ordered shutdown. STATUS item 30 closed |
| recovery on disk | 2026-09-02 | `FileJournal` through `serve_with_recovery`, and the instant a session was last alive. Sharded recovery remains (STATUS item 32 a) |
| `library` | 2026-09-02 | `crates/library`, package `fixbolt`: `Handler` / `Incoming` / `Reply` / `App` over the existing `Application` seam, plus a worked acceptor in `examples/` that the end-to-end test drives through a real socket. Costs ~956 ns a reply against 40 ns for a template built once ([ADR-0041](decisions/ADR-0041-the-library-layer-buys-an-api-with-a-template-per-message.md), [ADR-0044](decisions/ADR-0044-a-builder-that-is-not-moved-per-field.md)) |
| interop, initiator | 2026-09-02 | `tools/interop` + `scripts/interop.sh` + a blocking CI job, 7 / 7. Its first run found a defect six green gates could not see |
| interop, acceptor | 2026-09-04 | Same script and job, 7 / 7, `fixbolt::serve` under a `libquickfix` initiator (STATUS item 42) |
| resend from the ring | 2026-09-04 | 4096 slots, O(1) `get`, batched replay, two counters for what cannot be replayed ([ADR-0046](decisions/ADR-0046-the-ring-is-the-resend-store-and-a-replay-goes-in-batches.md)) |
| message log | 2026-09-04 | `FileLogPath`: both directions, refusals included, one line per message (DESIGN.md D14) |

### Phase 2: the encoding axis and the version axis

Scope decided by [ADR-0078](decisions/ADR-0078-sbe-enters-as-an-encoding-without-a-session-and-fixp-is-its-own-phase.md)
decision 4: **encoding only, no session** — the `Encoding` trait, SBE, and the FIX 5.0 /
FIXT 1.1 dictionaries the `ApplVerID` work drags in, plus `docs/internals`. Not FIXP, not
FAST, not FIXML.

| Item | Note |
|---|---|
| `Encoding` trait | the architectural gate for everything below ([ADR-0079](decisions/ADR-0079-one-view-per-encoding-and-one-trait-over-them.md)) |
| SBE | the encoding modern venues actually use; no session of its own |
| FIX 5.0 / FIXT 1.1 | SBE needs `ApplVerID`, so the two arrive together |

FIXP is deferred to its own phase 3 plan and its own ADR, opened only when there is a target
venue and an oracle for it ([ADR-0078](decisions/ADR-0078-sbe-enters-as-an-encoding-without-a-session-and-fixp-is-its-own-phase.md)
decision 2). FAST and FIXML are out of scope for phase 2.

### Phase 3: dependable by a stranger — *Accepted — ADR-0097, 2026-09-23*

**Approved by the owner on 2026-09-23, not yet built.** Decided by
[ADR-0097](decisions/ADR-0097-phase-3-makes-the-engine-dependable-by-a-stranger-and-fixp-waits-on-a-running-oracle.md)
with all eight of its recommendations as proposed; the plan is
[plans/2026-09-23-phase-3-scope.md](plans/2026-09-23-phase-3-scope.md). The published crates are
`fixbolt-codec`, `-dict`, `-session`, `-engine`, `-sbe` and `fixbolt`; released as the git tag
`v0.1.0`, not a crates.io upload
([ADR-0161](decisions/ADR-0161-0-1-0-is-a-git-tag-not-a-crates-io-upload-and-the-stranger-and-the-semver-gate-read-the-tag.md)).
The candidates it leaves out — kernel bypass, SIMD, clustering, HA,
replication — stay out of phase 3; bypass (Onload over AF_XDP) and SIMD are phase 4's, each behind
a kill line (ADR-0098), and clustering, HA and replication stay in §5.

Scope: make the engine something a stranger can `cargo add`, read, trust and upgrade. It moves
§3's largest gap — a track record of zero — the only way that gap moves, by being downloadable.
`[measured 2026-09-23]` it is not downloadable today: every crate is `0.0.0` / `publish = false`,
and `fixbolt-dict`'s build reads QuickFIX's XML from gitignored `vendor/`, so a published crate
could not build.

| Item | Note |
|---|---|
| A dictionary a published crate can build | Its own ADR first: Apache-2.0 FIX Orchestra files with QuickFIX's XML kept as the oracle (recommended), or QuickFIX-derived tables with a `NOTICE` ([ADR-0001](decisions/ADR-0001-relationship-to-quickfix.md) decision 5) |
| No credential reaches disk | `554=` / `96=` redacted in the message log and journal. The admission hook (`Registry::admit`) already exists |
| `Decimal` | [ADR-0028](decisions/ADR-0028-a-decimal-is-a-copy-value-parsed-on-demand.md), accepted 2026-09-01; shape revised and built by [ADR-0120](decisions/ADR-0120-a-decimal-is-a-mantissa-and-a-signed-exponent-read-by-a-free-function-and-round-trips-only-in-canonical-form.md), 2026-09-23 |
| A second engine family in the interop gate | QuickFIX/J, both roles, plaintext and TLS — §3 gap 2 is that TLS has never spoken to another engine |
| Packaging and an API gate | `cargo publish --workspace --dry-run` with no `vendor/`; `cargo-semver-checks` from the first release |
| First release, `0.1.0` as the git tag `v0.1.0` | Not `1.0`: the `1.0` condition (one outside deployment, one clean minor) is written into `CHANGELOG.md` and is not a phase-3 criterion. Not a crates.io upload (ADR-0161) |
| FIXP oracle spike, *conditional* | Only if the owner names a target venue (B3 Binary EntryPoint is the candidate; Artio's acceptor is the second implementation). Its output is the FIXP ADR, not a session machine ([ADR-0078](decisions/ADR-0078-sbe-enters-as-an-encoding-without-a-session-and-fixp-is-its-own-phase.md) decision 2). **Done 2026-09-23 (row 9)**: `scripts/fixp-spike.sh` — `accept PASS 5/5, reject-timestamp PASS, reject-credentials PASS` against Artio 0.184, blocking CI job `fixp-spike` ([ADR-0140](decisions/ADR-0140-the-fixp-spike-speaks-the-referees-own-schema-through-a-detached-probe-and-its-ci-job-blocks.md), [CONFORMANCE.md §11](CONFORMANCE.md)); the FIXP ADR is [ADR-0141](decisions/ADR-0141-fixp-is-built-only-in-the-venues-current-dialect-in-a-role-that-dialect-has-a-referee-for-and-not-before-phase-5.md) (Proposed): no session before phase 5, built only in B3's current dialect and in a role with a running referee | |

**Not in phase 3, by name:** HA, replication, clustering, hot standby; kernel bypass, Onload,
`ef_vi`, AF_XDP, DPDK; `io_uring` / `recvmmsg`
([ADR-0074](decisions/ADR-0074-kernel-bypass-io-uring-and-the-logon-hop-stay-unmeasured-by-decision.md));
SIMD ([ADR-0045](decisions/ADR-0045-parse-is-under-one-percent-of-the-wire-and-simd-is-declined.md));
FAST, FIXML, SBE inside FIXT; database stores; metrics exporters and dashboards; a FIXP session
machine; a `1.0` release; sharded ordered shutdown (stays a `STATUS.md` item). §5 is unchanged.

**Phase 3 exit criteria.** Each is a command that passes or fails on the closing
commit, with a CI run id.

| # | Criterion | Gate |
|---|---|---|
| 1 | Builds with no `vendor/` | `cargo build -p fixbolt-dict` and `cargo build -p fixbolt --no-default-features` on a checkout with `vendor/` absent |
| 2 | Tables agree with the oracle | the dictionary agreement test: 912 / 912 tags, 12 524 / 12 524 pairs, 1 708 / 1 708 enums, or each divergence named by content; `--test score` still 59 / 59 |
| 3 | Packages as published | `cargo publish --workspace --dry-run` in CI with no `vendor/` |
| 4 | No credential reaches disk | a test writes a Logon with `554=` / `96=` through the message log and `FileJournal` and fails on finding the secret; proven by reversal |
| 5 | `Decimal` ([ADR-0120](decisions/ADR-0120-a-decimal-is-a-mantissa-and-a-signed-exponent-read-by-a-free-function-and-round-trips-only-in-canonical-form.md)) | `cargo test -p fixbolt-codec decimal`; its `benches/alloc.rs` case reads 0, proven by injection |
| 6 | Second engine family | `scripts/interop.sh` against QuickFIX/J, both roles, 7 / 7, plaintext and TLS, blocking in CI |
| 7 | A stranger can depend on it | a scratch crate outside the tree, `cargo add fixbolt --git https://github.com/tmthang86/fixbolt --tag v0.1.0`, `GETTING-STARTED.md`'s code verbatim, one Logon / Logout, proof the build came from GitHub at that tag; exits 0 (ADR-0161 decision 4) |
| 8 | The API is watched | `scripts/check-semver-against-tag.sh`, no argument (`cargo semver-checks --baseline-rev` the newest release tag `HEAD` descends from — `v0.1.0` today — asserting a real comparison ran), blocking in CI (ADR-0161 decision 5, [ADR-0162](decisions/ADR-0162-the-semver-baseline-is-the-newest-release-tag-head-descends-from-and-zero-checks-are-excused-only-by-a-major-bump.md)) |
| — | Phases 1 and 2 still hold | 59 / 59 in process and on a socket; FIXT 179 / 180 with its pinned divergence; `libquickfix` interop 7 / 7; allocation benches 0 |

### Phase 4: the owner's five items, each behind a kill line — *Accepted — ADR-0098, 2026-09-23*

**Approved by the owner on 2026-09-23, not yet built; nothing starts before phase 3 closes.**
All eight of the plan's questions were answered as recommended — SQLite, not Postgres; Onload
over AF_XDP, no TCP stack of this project's own; no `ef_vi` card; SQPOLL an `hft` arm only.
Decided by
[ADR-0098](decisions/ADR-0098-phase-4-is-the-owners-five-items-each-entering-behind-a-measurement-that-can-kill-it.md),
with [ADR-0099](decisions/ADR-0099-kernel-tcp-stays-the-default-and-the-headline-and-a-bypass-figure-is-a-second-labelled-row.md)
(supersedes ADR-0077 decision 2 and ADR-0074 decision 1) and
[ADR-0100](decisions/ADR-0100-simd-is-reopened-as-an-experiment-whose-kill-line-is-written-before-the-code.md)
(supersedes ADR-0045 decision 1); explained for the owner in
[plans/2026-09-23-phase-4-scope.md](plans/2026-09-23-phase-4-scope.md). The owner chose the
contents; ADR-0098 decides how each enters without breaking an accepted decision, and writes the
line that removes it if it does not pay. A removed item is a completed item.

| Order | Item | Enters how | Kept only if (§9 desk, same boot, two procedures) |
|---|---|---|---|
| 1 | Metrics exporter + Grafana dashboard | new crate `fixbolt-metrics`, own thread, reads `Snapshot` and the event stream; no async runtime | engine-thread allocations 0 under a 10 Hz scrape; wire p50 / p99 within the band, scrape on vs off |
| 2 | SQLite-backed store | new crate `fixbolt-store-sqlite`, `FileJournal` `Async`'s shape: ring on the engine thread, batched commits on a writer thread; no synchronous-durability mode | engine-thread allocations 0; wire p50 within the band of `FileJournal` `Async`; 50 000 msg/s for 60 s with no dropped record |
| 3 | `io_uring` transport | kernel TCP still, so no supersession — ADR-0074 decision 2's trigger (an interval-0 NIC figure) fired on 2026-09-18; feature `io-uring`, off by default, refuses to start rather than fall back | NIC wire p50 ≥ 3 % better in both procedures, or idle turn at N = 16 ≥ 25 % better |
| 4 | Kernel bypass: Onload over AF_XDP on the I211 | ADR-0099; no engine code; `hft` only, plaintext only; compared generator-side against a same-boot kernel twin | generator-side p50 ≥ 10 % better in both procedures, p99 no worse, zero-copy bound, `--test wire` 59 / 59 under `onload` |
| 5 | SIMD (SWAR first) in `codec` | ADR-0100; measured last because its denominator comes from items 3–4 | codec cases ≥ 15 % better **and** parse ≥ 2 % of the fastest surviving round trip or density at N = 64 ≥ 3 % better |

**Not in phase 4:** a native AF_XDP transport or any userspace TCP stack; `ef_vi` (no hardware);
DPDK (never); `io_uring` zero-copy receive; SQPOLL in `standard`; Postgres unless its own ADR is
accepted (the `postgres` crate carries a Tokio runtime); a web UI of this project's own; HA.

**Phase 4 exit criteria.** Each a command that passes or fails on the closing commit,
with a CI run id; the measured rows quote `scripts/check-machine.sh` from the desk.

| # | Criterion | Gate |
|---|---|---|
| 1 | Nothing new built by default | `cargo build --workspace --no-default-features`; `scripts/check-no-optional-deps.sh` |
| 2 | `io_uring` verdict applied | kept: `--features io-uring --test wire` 59 / 59, both mode scripts pass and are tripped by the wrong mode, alloc 0; killed: feature absent, pair in `measured-costs.md` |
| 3 | Bypass verdict applied | kept: second labelled row in `DESIGN.md` §8 beside its kernel twin, 59 / 59 under `onload`; killed: negative pair in `measured-costs.md` |
| 4 | SIMD verdict applied | `bench.sh --strict` A/B quoted; kept: differential fuzz and Miri on the SWAR arm green; killed: code absent |
| 5 | The store recovers | `cargo test -p fixbolt-store-sqlite` with a crash-and-recover test; engine-thread alloc 0; the throughput run quoted |
| 6 | The exporter stays off the hot path | `cargo test -p fixbolt-metrics`; alloc 0 under scrape; scrape on / off pair within the band |
| — | Phases 1–3 hold | 59 / 59, FIXT 179 / 180, interop 7 / 7 with both peers, `cargo semver-checks` |

### Phase 1 exit criteria

Every criterion is a command that passes or fails. **All seven are met.**

| # | Criterion | Gate and result |
|---|---|---|
| 1 | Session conformance | `cargo test -p fixbolt-session --test score` **59 / 59** in process `[measured 2026-08-29]`; `cargo test -p fixbolt-engine --test wire` **59 / 59** through a real socket on an Apple M5 and on Linux `[measured 2026-08-30]`. It read 39 / 59 on Linux until the harness's own client socket was given `TCP_NODELAY` |
| 2 | Repeating groups | **Met 2026-08-28.** All 93 groups read and written; order agreed with QuickFIX's generated C++ on 730 / 730. The 59 definitions do not test this (§4) |
| 3 | Dictionary validation | **Met.** 912 tags, 93 message types, 12 524 (message, tag) pairs, 23 field types, 1 708 enum values, generated from XML with `<component>` recursion, applied by the session. All twelve `373` codes are produced, and a test asserts that rather than inferring it from the file count |
| 4 | Both roles | Acceptor 59 / 59. Initiator interop-green against `libquickfix` in CI, **met 2026-09-02** ([ADR-0042](decisions/ADR-0042-a-second-implementation-is-the-only-independent-opinion.md)): logon → application messages in → an unprompted heartbeat → a TestRequest with our own `112=` → a ResendRequest answered by replay → a gap this end opens and gap-fills → logout, **7 / 7**. Its first run found the initiator answering a Logon with a Logon, a defect green in 59 / 59 and in 430 other tests |
| 5 | Allocations on the hot path | **0**, proven by `benches/alloc.rs` on every crate with a hot path, each case proven by injection `[measured 2026-08-30]` |
| 6 | Wire-to-wire | **Met 2026-09-02.** `tools/w2w` on an AMD Ryzen 7 3700X, Linux 7.0.0-30, bare metal, `scripts/check-machine.sh` `pass 12 fail 0 unknown 1`, `isolcpus=6,7,14,15 rcu_nocbs=6,7,14,15 processor.max_cstate=1`, no `nohz_full`, mitigations on, engine pinned to `cpu6` and client to `cpu7`, medians of 20 runs of 20 000 round trips over loopback: `hft` **16 010 / 20 589 / 22 127 ns** administrative and **19 908 / 24 657 / 26 150** application; `standard` **19 447 / 24 106 / 25 609** and **20 920 / 25 618 / 27 092**. Run-to-run spread of the p50 0.3–0.6%. Allocations in the timed window **0**, counted on both threads and proven by reversal. `scripts/w2w-baseline.sh` is the procedure |
| 7 | Builds clean | `cargo clippy -D warnings`, `--no-default-features`, no `unwrap` / `expect` / `panic` in a library crate |

**What the ticks do not buy**, so nobody reads more into them:

- **Criterion 6 is loopback.** [DESIGN.md §6](DESIGN.md) has a stricter row, NIC to NIC with
  `SO_TIMESTAMPING` and a load generator on another machine, ~~and that row is open (STATUS
  item 40)~~ — `[measured 2026-09-18]` **that row is now met at p50 and p99 for `hft`, back to
  back over a cable** (admin 27 050 ‖ 27 114 ns, application 28 894 ‖ 28 878 at p50, two
  procedures; p99.9 published marked, not reproduced; `standard` not wire-stamped, ADR-0073),
  which phase 1 never asked for. Until 2026-09-02 `tools/w2w` pinned no threads and printed no
  p99.9, so no earlier run could have met this criterion however well the box was tuned.
- **Criterion 4 is 14 cases, not a second corpus.** One scenario per direction against one
  counterparty. It does not cover `hft` mode, TLS, more than one counterparty, session
  schedules, reconnect or backoff, and neither do the `.def` files. `[measured 2026-09-04]`
  the acceptor direction's first run was red, and the red was the test's: it waited for a
  reply to the message that opened the gap, and the counterparty's own gap fill had covered
  that number ([reference/a-gap-fill-can-swallow-the-question.md](reference/a-gap-fill-can-swallow-the-question.md)).

What remains in [STATUS.md](../STATUS.md) is work phase 1 never asked for: the NIC-to-NIC row
(item 40), the dictionary pass nobody has timed (item 39), the library's per-message template
(item 34), the mirrored corpus's ceiling (item 36), sharded recovery (item 32 a), and the
engine's inability to originate an application message (item 46). Item 45 holds the order.

### Phase 2 starts with an architectural decision

**`MessageView` as designed does not generalise.** [ADR-0003](decisions/ADR-0003-message-representation.md)
defines `FieldEntry { tag, offset, len }`, which presupposes tags on the wire. SBE has none:
fields sit at schema-determined offsets. So phase 2 does not begin with "add SBE"; it begins
with deciding whether there is one view type or several, in its own ADR.

Each encoding also drags something with it:

| Encoding | What it really costs | Status |
|---|---|---|
| **SBE** | Decided by [ADR-0078](decisions/ADR-0078-sbe-enters-as-an-encoding-without-a-session-and-fixp-is-its-own-phase.md): rides no session, tag=value or otherwise. Built as `crates/sbe` + `crates/sbe-gen` under `Encoding` ([ADR-0081](decisions/ADR-0081-sbe-tables-come-from-this-repositorys-generator-and-the-oracle-is-the-specs-bytes-plus-sbe-tool-behind-a-script.md); the session boundary is [ADR-0082](decisions/ADR-0082-the-session-is-generic-over-tag-value-encodings-and-the-boundary-to-sbe-is-the-session-not-the-trait.md) decision 4), re-exported as `fixbolt::sbe` behind feature `sbe`. `[measured 2026-09-19]` unit and integration tests green on both crates, the three RC4 spec dumps and Real Logic's `Car` round-trip byte for byte, alloc bench reads 0 on four paths. **Not yet measured or built**: a timing baseline (`benches/sbe.rs` prints `NO BASELINE`) and `tools/sbe-interop`, the second-implementation check against Real Logic's own generator (ADR-0042) | `[measured]` for what is tested; timing and interop `[unproven]` |
| **FIX 5.0 / FIXT 1.1** | `[measured 2026-09-19]` **built**, behind the off-by-default `fix50sp2` feature: one table from `FIXT11.xml` + `FIX50SP2.xml`, the session's four FIXT boundary rules, **179 / 180** in process with the single divergence pinned by content, **60 / 60** through a real socket, and interop against `libquickfix` **7 / 7 in both roles** under `FIXT.1.1` / `FIX.5.0SP2`. It was **not** the cheapest item in phase 2: the 180 `.def` files are FIX 4.4 tests replayed against FIXT dictionaries, so two boundary behaviours have no oracle at all, and 180 / 180 is unreachable with one SP2 table ([ADR-0084](decisions/ADR-0084-a-session-message-is-checked-against-the-layer-that-defines-it-a-members-value-waits-for-the-count-and-fix50-is-its-own-oracle.md)). See [CONFORMANCE.md](CONFORMANCE.md) §9 | `[measured]` except the two no-oracle behaviours, which stay `[unproven]` |
| **FAST** | Stateful decoding: message N depends on state left by message N−1. `parse_into` is stateless by design. Legacy; most venues moved to SBE | `[unproven]` |
| **FIXML** | Needs an XML parser, which allocates. FIXML lives outside non-negotiable 1 because post-trade is not a hot path; stated here so it is a decision, not a leak | `[unproven]` |

`[measured]` QuickFIX ships no SBE, FAST or FIXML schema, so phase 2 has no free test oracle
from that direction.

## 3. Where this stands against QuickFIX

**Short answer: not close on breadth, and the largest gap does not close by writing code.**

QuickFIX is 20+ years old with complete spec coverage. Matching it is not a phase-1 goal, but
the gaps have to be named, because anyone comparing the two will find them.

| Capability | QuickFIX | fixbolt | Phase |
|---|---|---|---|
| FIX versions | 4.0–5.0 SP2 + FIXT 1.1, 8 dictionaries `[measured]` | 4.4 | 5.0 / FIXT → P2 |
| Acceptor | yes `[documented]` | yes | P1 |
| Initiator | yes `[documented]` | yes | P1 ([ADR-0004](decisions/ADR-0004-bidirectional-engine.md)) |
| Repeating groups | full, 93 groups in FIX 4.4 `[measured]` | read and written, nested to depth 4; order agreed with QuickFIX's generated C++ on 730 / 730 `[measured]` | P1, closed |
| Dictionary validation: types, enums, structure | full `[documented]` | 10 generated tables with `<component>` recursion; agreed with QuickFIX on 912 / 912 tag numbers, 898 / 912 type names (14 named differences), 12 524 / 12 524 message-tag pairs and 1 708 / 1 708 enum values; applied by the session's Reject `[measured 2026-08-28]` | P1, closed for the session layer; application-message validation is P2 |
| Decimal / price types | yes `[documented]` | ~~**decided, not built.** [ADR-0028](decisions/ADR-0028-a-decimal-is-a-copy-value-parsed-on-demand.md): `Decimal { value: i64, scale: u8 }`, `Copy`, parsed on demand, no `f64` in the public API. Today the application gets bytes~~ `[2026-09-23]` **built.** [ADR-0120](decisions/ADR-0120-a-decimal-is-a-mantissa-and-a-signed-exponent-read-by-a-free-function-and-round-trips-only-in-canonical-form.md) revises the shape: `Decimal { mantissa: i64, exponent: i8 }`, `Copy`, 16 bytes, read on demand as `as_decimal(view.get(tag)?)` beside `as_i64`, no `f64` in the public API. `crates/codec/tests/decimal.rs`, 30 tests | P1, gap |
| Session schedules | yes `[documented]` | `Schedule` in `session`: daily, weekly, weekday filter, windows across midnight, reset decided by `same_session` ([ADR-0033](decisions/ADR-0033-a-schedule-is-utc-arithmetic-and-the-calendar-stays-outside.md)). **UTC only**: no timezone names, because an IANA database cannot live in a pure layer; the caller resolves the offset and rebuilds. `last_active_ms` persisted since 2026-09-02 | P1, narrowed |
| Message stores | file, memory and SQL backends `[documented]` | in-memory ring (`MemJournal`) and an append-only file (`FileJournal`) with `Async` / `Fsync` durability ([ADR-0008](decisions/ADR-0008-journal-is-a-trait.md)); readable offline by `tools/jrnl` | P1, narrower by design |
| Logging | file, screen, SQL backends `[documented]` | `tracing` behind a feature flag, never on the hot path; and a message log, both directions with refusals, one line per message (D14) | P1 |
| SSL / TLS | yes `[documented]` | [ADR-0005](decisions/ADR-0005-tls.md) accepted: `rustls` handshake, kTLS steady state on Linux. `[measured 2026-08-31]` kTLS is drivable from a plain non-blocking socket with no async runtime ([ADR-0018](decisions/ADR-0018-ktls-on-a-plain-socket-answers-adr-0005.md)). ~~**No TLS code is merged**; a plan is drafted~~ `[2026-09-14]` built behind `--features tls`, on Linux, for both roles — `serve_tls*` and `connect_and_serve_tls*` ([plans/2026-09-04-tls.md](plans/2026-09-04-tls.md)); the steady-state round trip is in [DESIGN.md](DESIGN.md) §8 `[measured 2026-09-14]` | **P1, gap** |
| Configuration file | yes `[documented]` | yes, QuickFIX's shape, eleven keys, strict ([ADR-0040](decisions/ADR-0040-a-configuration-file-refuses-what-it-does-not-understand.md)). No credentials, no reload while running | P1, closed |
| Typed message classes / cracker | generated per message `[documented]` | borrowed `MessageView` over the wire buffer | deliberate ([ADR-0003](decisions/ADR-0003-message-representation.md)) |
| Code generation targets | C++, Python, Ruby `[measured]` | Rust only | not a goal |
| SBE / FAST / FIXML | none `[measured]` | phase 2 | P2, ahead of QuickFIX |
| Throughput | 6 000–8 000 msg/s per session `[documented]` | target an order of magnitude more `[unproven]` | P1 |
| Many counterparties on one acceptor | yes `[documented]` | yes: `presession::Registry`, one engine holds all of them ([ADR-0026](decisions/ADR-0026-a-counterparty-registry-in-the-pre-session-stage.md), [ADR-0030](decisions/ADR-0030-one-engine-holds-many-counterparties.md)). The corpus scores 59 through two shards | P1, closed 2026-09-01 |
| Logon authentication: `553` / `554`, per-counterparty credentials, IP allowlist | yes `[documented]` | **nothing beyond identity.** Anything presenting a configured comp-ID pair is admitted. The hook exists (`Registry::lookup` returning `None`); no credential check is behind it | P1, gap |
| Ordered shutdown: Logout, journal flush, drain | yes `[documented]` | `Admin::shutdown(grace_ms)`; `serve` returns a `Shutdown` naming who never answered ([ADR-0038](decisions/ADR-0038-an-ordered-shutdown-is-a-state-not-a-flag.md)). `serve_sharded_hft` cannot be stopped | P1, mostly closed |
| Operator visibility: state, sequence numbers, counters | via callbacks and a log backend `[documented]` | `Handles::observer()` → `Snapshot` on request, through `serve` as well as a hand-built `Engine` ([ADR-0054](decisions/ADR-0054-the-handles-are-made-before-the-engine-and-the-engine-adopts-them.md)) ([ADR-0032](decisions/ADR-0032-observation-is-a-snapshot-taken-on-request.md)); `Snapshot::healthy()`; `Observer::events()` pushes `EventKind::Ended(DropReason)` with losses counted ([ADR-0035](decisions/ADR-0035-an-event-is-pushed-and-a-loss-is-counted.md)); ring depth (`Snapshot::ring_to_app`) and pre-session occupancy (`Snapshot::presession_slots`) `[2026-09-24]`, `None` until a `Dispatch`/front door reports one ([ADR-0170](decisions/ADR-0170-the-metrics-exporter-holds-an-observer-and-nothing-else-allocates-nothing-per-scrape-and-publishes-only-after-its-kill-line.md)). A Prometheus exporter over all of it (`fixbolt-metrics`) is **built, not yet kept — its kill line is row 2's pair on the §9 desk** (ADR-0098 item 5, ADR-0170) | P1, mostly closed |
| Sequence-number administration while running | yes `[documented]` | `Handles::admin()`: `SetNextOut`, `SetNextIn`, `SendSequenceReset`, applied before a turn numbers anything ([ADR-0036](decisions/ADR-0036-one-mechanism-two-capabilities.md)) | P1, closed |
| Reading the message store offline | plain, readable formats `[documented]` | `tools/jrnl` and `journal::Reader`, over the whole file, outside the process ([ADR-0037](decisions/ADR-0037-reading-a-journal-is-not-recovering-from-one.md)) | P1, closed |
| Health / readiness probe | n/a | `Snapshot::healthy()` | P1, closed |
| Production track record | thousands of counterparties `[documented]` | **zero** | — |

**The two gaps that matter most:**

1. **Production track record.** [ADR-0001](decisions/ADR-0001-relationship-to-quickfix.md)
   says it directly: QuickFIX's real value is that thousands of counterparties have already
   found its bugs. No test coverage substitutes. This gap closes by being deployed, not by
   writing code.
2. **TLS.** The blocking question is answered ~~and the plan is drafted, but no TLS code is
   merged, no TLS latency number exists, and nothing yet says which of the three TLS modes
   (D11) a session is actually in~~. `[2026-09-14]` TLS is built behind `fixbolt-engine`'s `tls`
   feature on Linux for both roles (`serve_tls`, `connect_and_serve_tls`); the steady-state round
   trip is published in [DESIGN.md](DESIGN.md) §8; and a session's mode is reported and can be
   required (`EventKind::TlsFellBackToUserspace`, `Engine::tls_mode`, `TlsRequireKernel` —
   [DESIGN.md](DESIGN.md) §6, *Which TLS mode is actually in force*). What is still missing is a
   counterparty this repository did not write: nothing here has spoken TLS to another engine.

The one that was the largest, **many counterparties on one acceptor**, closed on 2026-09-01.
Until then every entry point took one `Config`, so the engine was a link rather than an
acceptor; `presession::Registry` maps an identity to its own configuration and the single-logon
rule compares identities instead of counting connections.

## 4. What "done" does not mean

The acceptance suite is the primary gate, and it has a stated blind spot. Of the 59 FIX 4.4
definitions:

- **Repeating groups: untested.** One populated group, in a negative test (`386=3` in
  `14i_RepeatingGroupCountNotEqual.def`, a file whose purpose is testing a *wrong* count).
  The gates that do test them are `group_roundtrip.rs` (357 top-level positions round-tripped
  byte for byte) and `interop_quickfix_order.rs` (730 / 730 groups agreed with QuickFIX's
  generated C++). The second exists because the first reads the same table the encoder does.
- **Application-message semantics: untested.** It is a session-layer suite. `[measured
  2026-08-29]` it hands 42 application messages to an application, only to have them echoed
  back. Nothing in it asks what an order means.
- **Reconnect, backoff, session schedules: no definition covers any of them.** All 59 run
  inside one interval. Schedules are tested by `crates/session/tests/schedule.rs` alone, and
  reconnect by `crates/engine/tests/reconnect.rs`, whose cases are all this project's own.
- **Field types, enum values, decimal precision: untested** by the corpus.
- ~~**Application-message resend: implemented and never exercised.**~~ Written here on
  2026-09-01 and refuted the same day by running it: `cargo test -p fixbolt-engine --test
  journal` feeds a real `35=D` from `8_OnlyApplicationMessages.def` and asserts one replay with
  `43=Y`, a fresh `52=` and the original as `122=`. The error was a stale STATUS bullet that
  was believed. Kept here struck, because a blind-spot list that quietly drops its own errors
  is the failure this section exists to name.

59 / 59 means the session state machine is right. It does not mean the engine is usable. Exit
criteria 2, 3, 4 and 6 exist because of this paragraph.

## 5. Permanent non-goals

Out unless a new ADR reverses them:

- **Kernel bypass, except Onload over AF_XDP** (DPDK, `ef_vi`, a userspace TCP stack of this
  project's own). *Narrowed 2026-09-23 by
  [ADR-0098](decisions/ADR-0098-phase-4-is-the-owners-five-items-each-entering-behind-a-measurement-that-can-kill-it.md)
  and [ADR-0099](decisions/ADR-0099-kernel-tcp-stays-the-default-and-the-headline-and-a-bypass-figure-is-a-second-labelled-row.md):*
  Onload over AF_XDP left this list as phase 4's measured bypass arm — engine unchanged, `hft`
  only, published only as a second labelled row beside a kernel-TCP figure from the same boot,
  and removed if it misses its kill line. The rest stays out: DPDK never, because it ships no
  TCP stack; `ef_vi` as a second `Transport` only once a Solarflare / X2-class NIC exists here;
  no TCP stack of our own. Bypass is plaintext only; it excludes TLS (D11). STATUS item 14.
- **Clustering, HA, replication.**
- **A web UI of this project's own.** *Narrowed 2026-09-23 by ADR-0098 item 5:* a
  Prometheus-format exporter on its own thread (`fixbolt-metrics`) and a committed Grafana
  dashboard definition left this list for phase 4; Grafana is the UI, and building one here
  stays a non-goal.
- **Code generation for languages other than Rust.**
- **Matching engine, order book, risk.** This is a protocol engine.
- **Record retention, immutability, tamper evidence and search.**
  [ADR-0027](decisions/ADR-0027-the-engine-owes-a-byte-stream-not-an-archive.md): the engine
  emits a faithful, ordered, timestamped copy of both directions at a boundary and owns nothing
  past it. `[documented]` MiFID II wants five to seven years in a tamper-evident, searchable
  archive; every one of those is a storage-system property. Drop copy is a deployment topology
  (a second FIX session, an ordinary registry entry), not an engine feature.

## 6. Open decisions

| # | Question | Status |
|---|---|---|
| 1 | Does the headline positioning stay "the fastest acceptor on kernel TCP" now that the engine is bidirectional? | **Answered by [ADR-0077](decisions/ADR-0077-acceptor-first-stays-and-fastest-is-said-only-beside-a-reproduced-pair.md)**: acceptor-first stays, but "the fastest" is retired from every headline in favour of *a FIX 4.4 acceptor on kernel TCP whose latency is a published, reproduced number*; a superlative may appear only beside the pair that supports it. **Decision 2 superseded 2026-09-23 by [ADR-0099](decisions/ADR-0099-kernel-tcp-stays-the-default-and-the-headline-and-a-bypass-figure-is-a-second-labelled-row.md)**: kernel TCP stays the headline, and a kernel-bypass figure is published only as a second labelled row measured in the same boot |
| 2 | Which plan owns repeating groups? | **Answered:** [their own plan](plans/2026-08-27-repeating-groups.md), after `codec` step 1 |
| 3 | Does SBE ride a tag=value session, or does FIXP enter scope? | **Answered by [ADR-0078](decisions/ADR-0078-sbe-enters-as-an-encoding-without-a-session-and-fixp-is-its-own-phase.md)**: SBE enters phase 2 as an encoding with no session of its own; FIXP is its own phase, its own plan and its own ADR |
| 4 | One view type or several, once encodings stop having tags on the wire? | **Answered by [ADR-0079](decisions/ADR-0079-one-view-per-encoding-and-one-trait-over-them.md)**: several view types, one per encoding, held under one `Encoding` trait; `MessageView` and `codec`'s public API today are unchanged |
| 5 | TLS: own implementation, `rustls`, or terminate outside the process? | **Answered** by [ADR-0005](decisions/ADR-0005-tls.md), which raised six questions of its own. The blocking one (can kTLS be driven from a non-blocking socket with no async runtime?) was answered yes on 2026-08-31 by [ADR-0018](decisions/ADR-0018-ktls-on-a-plain-socket-answers-adr-0005.md). ~~Five remain; question 2 (which kernel and which cipher suites are the floor) decides how deployable TLS is~~ `[2026-09-14]` two remain open: 4 (mutual TLS in phase 1) and 5 (SNI and several certificates). Question 3 was answered as of step 4b (2026-09-12), 6 at phase-1 level on 2026-09-13, and 2 only at the level measured on 2026-09-14 — one suite on one kernel, no floor ([DESIGN.md](DESIGN.md) §9) |
| 6 | Final name | **Decided 2026-08-30: `fixbolt`** |
| 7 | Can the engine configure itself from the machine (mode, cores, bypass) instead of making the caller do it? | **Proposed answer:** it detects and advises, never applies, and `hft` has a hard ceiling of four sessions per engine that refuses the fifth. Four because `2000 / 448.9 = 4.46`: the largest N that beats an `epoll`-class wakeup under both ends of the 2–5 µs literature range. [ADR-0025](decisions/ADR-0025-hft-has-a-hard-session-ceiling-and-the-engine-advises-rather-than-applies.md) accepted once C-PRD7 records the number; deliberately still `Proposed` because its number rests on a run nobody has taken. Bypass detection stays in phase 3 |
| 8 | Where does the counterparty registry live? | **Answered 2026-09-01** by [ADR-0026](decisions/ADR-0026-a-counterparty-registry-in-the-pre-session-stage.md): in `presession`, as a trait. All three engines surveyed (QuickFIX, QuickFIX/J, Artio) decide at the Logon through a provider or callback. `lookup` is synchronous, because an accept path that awaits a network call is a denial-of-service surface. Found a real defect: `Identity` was `(49, 56)` only, so `50=` / `57=` could not be served; they are carried now |
| 9 | How much does this engine owe an auditor? | **Answered 2026-09-01** by [ADR-0027](decisions/ADR-0027-the-engine-owes-a-byte-stream-not-an-archive.md): a faithful copy of both directions at a boundary, off the hot path, and nothing beyond it. Built as the message log (D14) on 2026-09-04 |
| 10 | Does the application get bytes, or typed values? | **Answered 2026-09-01** by [ADR-0028](decisions/ADR-0028-a-decimal-is-a-copy-value-parsed-on-demand.md): a `Copy` `Decimal { value, scale }` parsed on demand, no `f64`. It overturned this row's original guess that the gap was mislabelled. ~~Not built yet~~ `[2026-09-23]` **built.** [ADR-0120](decisions/ADR-0120-a-decimal-is-a-mantissa-and-a-signed-exponent-read-by-a-free-function-and-round-trips-only-in-canonical-form.md) revises the shape to `Decimal { mantissa: i64, exponent: i8 }`, `Copy`, read on demand as `as_decimal(view.get(tag)?)`, no `f64`. `crates/codec/tests/decimal.rs`, 30 tests |
