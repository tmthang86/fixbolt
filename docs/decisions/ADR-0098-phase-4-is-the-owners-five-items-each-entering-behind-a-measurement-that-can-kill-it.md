# ADR-0098 — Phase 4 is the owner's five items, each entering behind a measurement that can kill it

- **Status**: **Accepted — 2026-09-23, by the owner**, with ADR-0099 and ADR-0100, all eight
  questions of [plans/2026-09-23-phase-4-scope](../plans/2026-09-23-phase-4-scope.md) answered
  as recommended. Nothing in it starts before phase 3
  ([ADR-0097](ADR-0097-phase-3-makes-the-engine-dependable-by-a-stranger-and-fixp-waits-on-a-running-oracle.md))
  has closed. The owner's answers, in conversation 2026-09-23:
  Q1 **yes** — every item's kill line is written before its code, and a killed item counts as
  done; Q2 **yes** — ADR-0099 is accepted and the positioning sentence is rewritten to match it
  (kernel TCP stays the headline; a bypass figure is a second labelled row measured in the same
  boot); Q3 **no TCP stack of this project's own** — bypass is Onload over AF_XDP only; Q4
  **SQLite** (no Postgres ADR is opened); Q5 **no Solarflare / `ef_vi` card** in phase 4; Q6
  **the ≥ 2 % round-trip clause stays**, with the density arm (ADR-0100 decision 3); Q7 **the
  kill-line numbers stand as proposed** (3 %, 25 %, 10 %, 15 %, 2 %, 3 %, 50 000 msg/s × 60 s);
  Q8 **SQPOLL in `hft` is measured as an arm only, never a default**.
- **Date**: 2026-09-23
- **Deciders**: Tran Manh Thang. **The contents were chosen by the owner** (2026-09-23, in
  conversation): kernel bypass / AF_XDP, `io_uring`, SIMD, a database-backed store, a dashboard.
  Written by the architect (Opus); this ADR decides *how each enters honestly*, not *whether*.
- **Related**: `PRD.md` §2 *Phase 4*, §5; `DESIGN.md` D5, D7, D8, D11, §8, §9;
  [ADR-0012](ADR-0012-latency-first-and-one-session-per-polling-thread.md),
  [ADR-0013](ADR-0013-two-modes-standard-and-hft.md),
  [ADR-0032](ADR-0032-observation-is-a-snapshot-taken-on-request.md),
  [ADR-0045](ADR-0045-parse-is-under-one-percent-of-the-wire-and-simd-is-declined.md),
  [ADR-0068](ADR-0068-a-published-figure-is-two-procedures-shown-side-by-side.md),
  [ADR-0071](ADR-0071-a-skipped-tx-stamp-is-a-missing-sample-not-a-failed-run.md),
  [ADR-0074](ADR-0074-kernel-bypass-io-uring-and-the-logon-hop-stay-unmeasured-by-decision.md),
  [ADR-0077](ADR-0077-acceptor-first-stays-and-fastest-is-said-only-beside-a-reproduced-pair.md);
  drafted with it: [ADR-0099](ADR-0099-kernel-tcp-stays-the-default-and-the-headline-and-a-bypass-figure-is-a-second-labelled-row.md)
  (supersedes ADR-0077 decision 2 and ADR-0074 decision 1),
  [ADR-0100](ADR-0100-simd-is-reopened-as-an-experiment-whose-kill-line-is-written-before-the-code.md)
  (supersedes ADR-0045 decision 1).

## Context

ADR-0097 put every one of these five items out of phase 3, and four of them collide with an
accepted decision or a non-negotiable:

| Item | Collides with |
|---|---|
| Kernel bypass / AF_XDP | ADR-0077 decision 2 (*"on kernel TCP"*); ADR-0074 decision 1 (an Onload run is a smoke test, never a figure); `PRD.md` §5; D11 (bypass is plaintext); non-negotiables 4, 6, 8, 10 |
| `io_uring` | ADR-0074 decision 2 (not before an interval-0 NIC figure); non-negotiables 1, 4 (SQPOLL is a spinning kernel thread); `CLAUDE.md` §6 *Dependencies* |
| SIMD | ADR-0045 decision 1 (declined: parse is 0.36–0.62 % of the round trip, the gain is below the instrument's noise); non-negotiable 8 (`core::arch` is `unsafe`); `codec`'s zero-dependency rule |
| Database store | non-negotiables 1 and 4 if it runs on the engine thread; `CLAUDE.md` §6 (an async-runtime dependency needs an ADR) |
| Dashboard | `PRD.md` §5 (*"Metrics dashboards, web UIs"*); non-negotiables 1 and 4 if observation touches the hot path |

The owner has chosen the contents. The rules still hold: an accepted ADR's substance is never
edited (a new ADR supersedes it), and a number without its benchmark, machine and §9 settings is
somebody else's claim.

**Facts that changed since the ADRs being superseded were written:**

1. **ADR-0074 decision 2's trigger has fired.** It deferred `io_uring` *"until the NIC figure
   exists at interval 0 (ADR-0071 decision 2)"*. `DESIGN.md` §6 records that figure as met on
   2026-09-18 (boot C, `hft`, back to back over a cable, admin p50 27 050 ‖ 27 114 ns,
   application 28 894 ‖ 28 878). So `io_uring` needs no supersession — only its own plan, which
   ADR-0074 already asked for.
2. **The desk's NIC now has an AF_XDP zero-copy driver path.** ADR-0074's context says the
   I211's `igb` has native XDP *"but not zero-copy AF_XDP"*. `[measured 2026-09-23]` on the desk
   (`tmt-B450-I-AORUS-PRO-WIFI`, kernel `7.0.0-31-generic`, `ethtool -i enp9s0` → `driver: igb`),
   `/proc/kallsyms` lists `igb_run_xdp_zc`, `igb_clean_rx_irq_zc`, `igb_construct_skb_zc` — the
   zero-copy path merged for Linux 6.14. **Symbol presence only**: no XSK socket was bound, so
   whether a zero-copy bind succeeds on this I211 is not known.
3. **`io_uring` is enabled on the desk**: `/proc/sys/kernel/io_uring_disabled` reads `0`
   `[measured 2026-09-23]`. It is blocked by Docker's default seccomp profile since 25.0 and
   disabled at Google, so it cannot be a default.
4. **The syscall is the cost.** `[measured 2026-08-30]` `measured-costs.md` *The engine is
   syscall-bound*: `read(socket) → EAGAIN` 703 ns, of which 354 ns is kernel entry and exit;
   an idle `Engine::turn` is 449 ns per session on the §9 line. This is the term `io_uring` and
   bypass attack, and the term that decides whether SIMD's share ever exceeds 1 % (ADR-0045
   decision 3).

## Research

| Source | What it says | Bearing |
|---|---|---|
| <https://www.phoronix.com/news/IntelIGB-AF-XDP-Zero-Copy> , <https://lwn.net/Articles/986006/> | `igb` AF_XDP zero-copy queued for Linux 6.14; I210/I211 covered, tested on an i210 | Fact 2: the desk can try zero-copy |
| <https://arxiv.org/html/2402.10513v1> | AF_XDP best mean latency 6.5 µs (ConnectX-6) and 9.7 µs (X710) with busy polling, including 5–10 µs of tracing overhead; zero-copy without polling behaved badly on the Intel driver | Busy polling is the condition — `hft` only |
| <https://github.com/Xilinx-CNS/onload/blob/master/README.md> | Onload runs on non-Solarflare NICs via AF_XDP (generic, in-driver, zero-copy); community-supported | The bypass arm needs no engine code |
| <https://github.com/Xilinx-CNS/onload/issues/139> | *"Onload AF_XDP degraded latency on AWS"* | Onload on AF_XDP can be slower than the kernel — the kill line is real |
| <https://github.com/ASherjil/ABTRDA3> | ef_vi CTPIO 1.866 µs median RTT vs DPDK 3.506 µs on Solarflare; AF_XDP also benchmarked on I225-V and others | ef_vi pays on hardware this project does not own |
| <https://github.com/smoltcp-rs/smoltcp> | Userspace TCP/IP, no heap required, ~Gbps against Linux on loopback | The only Rust TCP stack a native AF_XDP transport could use; not proven for trading |
| <https://irenezhang.net/papers/demikernel-sosp21.pdf> , <https://github.com/microsoft/demikernel> | Rust libOS, ≈50 ns per I/O on bypass; Catpowder = Linux raw sockets / Windows XDP | Research-grade, not a dependency |
| <https://github.com/DouglasGray/xsk-rs> , <https://docs.rs/xdp/latest/xdp/> | AF_XDP in Rust: `xsk-rs` (libxdp, C via `cc`), `xdp` (pure Rust) | Available if the native transport is ever built |
| <https://lwn.net/Articles/961189/> , <https://github.com/lano1106/io_uring_udp_ping> | `io_uring` NAPI busy poll: UDP ping RTT avg 37.0 → 29.8 µs; with SQPOLL 44.4 → 37.3 µs (SQPOLL *slower* than without) | SQPOLL is not free; NAPI busy poll is the lever |
| <https://arxiv.org/html/2512.04859v1> | DeferTR + NAPI best; SQPOLL worse; registered buffers negligible or slightly worse for small messages | Same: do not assume SQPOLL or registered buffers help |
| <https://docs.kernel.org/networking/iou-zcrx.html> , <https://www.phoronix.com/news/Linux-6.15-IO_uring> | Zero-copy receive (6.15) needs header/data split and a bound hardware queue | Not on an I211; out |
| <https://github.com/moby/moby/pull/46762> , <https://en.wikipedia.org/wiki/Io_uring> | Docker ≥ 25 blocks `io_uring_*` by default; Google disabled it on ChromeOS, Android apps and servers after 60 % of the exploits submitted to its 2022 kernel bug bounty targeted it; `kernel.io_uring_disabled` sysctl | Opt-in only, fails loudly, never a silent fallback |
| <https://deepwiki.com/tokio-rs/io-uring> , <https://github.com/tokio-rs/tokio-uring/blob/master/DESIGN.md> | `io-uring` crate: low-level, no runtime; `tokio-uring` is a runtime | The dependency is `io-uring`, not an async runtime |
| <https://www.klittlepage.com/articles/accelerated-fix-processing-via-avx2-vector-instructions/> | AVX2 FIX parse: **5 %** over a hand-unrolled loop on a 166-byte message; checksum ~2× over compiler-vectorised, ~5× over naive | Checksum is the likelier win; parse gain is small |
| <https://github.com/StratCraftsAI/NexusFix> , <https://cpponline.uk/wp-content/uploads/2026/03/cpponline2026_talk1.pdf> | simdjson-style SOH/`=` structural index; ~250 ns ExecutionReport parse claimed | A claim, not reproduced; this codec parses NOS in 122.6 ns without SIMD |
| <https://github.com/quickfix-j/quickfixj/issues/357> , <https://www.quickfixj.org/jira/si/jira.issueviews:issue-html/QFJ-119/QFJ-119.html> | `JdbcStore`: two IO operations per message, shared sequence-number row, no transaction around message + sequence; users batch it themselves | A per-message DB write is the known failure; batch off-thread |
| <https://pkg.go.dev/github.com/quickfixgo/quickfix/store/mongo> | QuickFIX/Go SQL and MongoDB stores | Prior art for the interface |
| <https://docs.rs/postgres/latest/postgres/> , <https://crates.io/crates/postgres_sync> | the `postgres` crate is a wrapper around `tokio-postgres` **plus a Tokio runtime**; `postgres_sync` avoids Tokio, young | Postgres = an async-runtime dependency = its own ADR (`CLAUDE.md` §6) |
| <https://quickfixj.org/docs/architecture/> | QuickFIX/J exposes session state through JMX MBeans, scraped by JMX agents into Prometheus | Observation is a pull, beside the engine |
| <https://github.com/real-logic/artio/wiki/Operational-Concerns> | Artio exposes counters (failed inbound/outbound/replay, sequence numbers) in Aeron's shared-memory counters file, read by an external tool | The model: counters written by the engine, read by someone else |
| <https://docs.rs/prometheus-client> , <https://opentelemetry.io/docs/specs/otel/metrics/sdk_exporters/prometheus/> | Prometheus text encoding works on `std::io::Write`; an exporter is a pull endpoint | No runtime needed to expose metrics |

**Searched, found nothing:** a published latency figure for Onload on AF_XDP with an Intel
`igb` NIC; a published figure for any FIX engine on `io_uring`; an open-source FIX engine that
ships a Grafana dashboard or a Prometheus exporter of its own (QuickFIX/J relies on JMX agents;
Artio on its counters file); a measured SIMD gain on a FIX parse that already indexes fields in
one pass the way ADR-0003's layout does.

## Decision

Each item enters with: **how it avoids breaking an accepted ADR**, **the non-negotiables it
pressures and how the design holds them**, **the measurement that proves it is worth its cost**
(a before/after on the §9 desk, by command), and **its kill line**, written before any code.
"Killed" means: the code is removed on the same branch, the pair that killed it is recorded in
`docs/reference/measured-costs.md`, and the ADR that let it in is marked with the result. A
killed item is a completed phase-4 item, not a failed one.

### 1. `io_uring` as a second `Transport` on kernel TCP — first hot-path item

- **Entry**: no supersession. It is still kernel TCP, so ADR-0077 holds; ADR-0074 decision 2's
  trigger has fired (fact 1), and that decision already says the first experiment is a `w2w`
  A/B with its own plan.
- **Shape**: `crates/engine/src/transport/uring.rs`, `#[cfg(feature = "io-uring")] mod uring;`
  (non-negotiable 6), Linux only, off by default. Dependency: the `io-uring` crate (no runtime;
  justified in the plan, no ADR needed under `CLAUDE.md` §6). Multishot `recv` into a provided
  buffer ring allocated and pre-faulted at startup (non-negotiable 1); one completion queue per
  engine thread, so an idle sweep is one CQ peek instead of one `read` per socket.
- **Modes (non-negotiable 4)**: `hft` submits and peeks the CQ, never `io_uring_enter` with a
  wait; `standard` waits in `io_uring_enter(min_complete = 1)` — it blocks, as it must. **SQPOLL
  is not used in `standard`** (a kernel thread spinning is a `standard` engine that spins); in
  `hft` it is one optional arm, pinned to a named isolated core, and it counts as a second core
  burned. NAPI busy poll is an `hft` arm. Both mode scripts must pass for the new transport and
  be tripped by the wrong mode.
- **Security**: never a silent fallback. If `io_uring_setup` fails (seccomp, sysctl), the engine
  refuses to start and names the cause — the `TlsRequireKernel` pattern (D11). Documented in
  `GUIDE.md` with the Docker and sysctl facts.
- **Proof it is worth it**: on the §9 desk, same boot, two procedures (ADR-0068), `hft` then
  `standard`: `tools/w2w` NIC-to-NIC, kernel `read`/`send` arm vs `io_uring` arm; and the idle
  turn sweep in `crates/engine/benches/turn.rs` at N = 1, 16, 64 (plus `density.rs` for busy
  sessions).
- **Kill line**: kept only if **either** the NIC wire p50 improves by **≥ 3 %** in both
  procedures (≥ 3× the 0.9 % the two procedures disagreed by on 2026-09-18) with p99 no worse
  than the band, **or** the idle turn at N = 16 improves by **≥ 25 %** with the single-session
  wire p50 no worse than the band. Otherwise removed.

### 2. Kernel bypass: Onload over AF_XDP on the desk's I211 — measurement first, engine unchanged

- **Entry**: supersedes **ADR-0077 decision 2** and **ADR-0074 decision 1** through
  [ADR-0099](ADR-0099-kernel-tcp-stays-the-default-and-the-headline-and-a-bypass-figure-is-a-second-labelled-row.md):
  kernel TCP stays the default and the headline; a bypass figure may be published only as a
  second, labelled row beside a kernel figure from the same boot. `PRD.md` §5's *Kernel bypass*
  bullet is narrowed, not deleted: DPDK stays *never*; `ef_vi` stays out until a Solarflare /
  X2-class NIC exists here.
- **Shape**: **no engine code.** Onload's order from item 14 and ADR-0074 stands — Onload first,
  because the engine runs unchanged under `onload`. What is written: `DESIGN.md` §9 rows for a
  bypass boot (Onload version, `afxdp/register`, XDP mode *zero-copy* vs *copy* read back from the
  kernel, `EF_POLL_USEC`), `scripts/check-machine.sh` rows for them, and a `w2w` procedure that
  runs both arms on one boot.
- **Modes**: bypass is **`hft` only**. `standard` under Onload must either pass
  `scripts/check-standard-gives-the-core-back.sh` with Onload's spinning off, or be refused and
  documented as unsupported — Onload spinning in `standard` is the defect non-negotiable 4 names.
- **TLS**: plaintext only; D11 and bypass exclude each other; stated in `GUIDE.md`.
- **Trap**: the published NIC figure is stamped by `SO_TIMESTAMPING` on the acceptor's NIC; under
  a userspace stack those stamps may not exist. Both arms are therefore **also** measured from
  the generator's side (the Mac mini's own round trip), and only the generator-side pair is
  compared across arms. If Onload yields hardware stamps too, both are shown.
- **Proof**: on one §9 boot, `hft`, two procedures: kernel arm vs `onload` arm, generator-side
  RTT p50 / p99 / p99.9; the 59-definition socket corpus (`--test wire`) run under `onload`.
- **Kill line**: kept (published as a second row, plus a `docs/hft-playbook.md` procedure) only
  if generator-side p50 improves by **≥ 10 %** in both procedures, p99 is no worse, zero-copy
  mode is confirmed bound, and the corpus is 59 / 59 under `onload`. Otherwise the pair is
  recorded as a negative result and bypass work stops for phase 4.
- **Not in phase 4**: a native AF_XDP `Transport` with a userspace TCP stack (smoltcp or other).
  It is the thing `STATUS.md` item 14 says this project claims and does not do — own a TCP
  stack — and it is only worth an ADR if Onload passes its kill line **and** Onload's
  community-supported status is judged unacceptable. `ef_vi`: no hardware. DPDK: never.

### 3. SIMD (SWAR first) in `codec` — last hot-path item, because its denominator comes from 1 and 2

- **Entry**: supersedes **ADR-0045 decision 1** through
  [ADR-0100](ADR-0100-simd-is-reopened-as-an-experiment-whose-kill-line-is-written-before-the-code.md),
  which reopens it as an experiment with its kill line written first. ADR-0045 decisions 2–4
  stand: decision 3 names the trigger (a transport that removes the kernel term), decision 4
  fixes the shape (8-byte SWAR, no `memchr`, `core::arch` only behind a measurement).
- **Shape**: SWAR for the SOH scan and the checksum, safe Rust, `codec` keeps zero dependencies
  and stays `no_std`-clean. `core::arch` (AVX2) is a second arm **only if SWAR misses the kill
  line and the gap to it is in the checksum** — then non-negotiable 8 applies: a plan, a comment
  naming the proof, a differential fuzz target against the scalar path, and Miri on the SWAR arm.
- **Proof**: `scripts/bench.sh --strict` same-boot A/B on `parse NewOrderSingle (validated)`,
  `parse Heartbeat (validated)` and a checksum-only case (none exists today; it is added and
  baselined **before** the SWAR code, so the A/B has a before); then `w2w` on the fastest transport that survived
  items 1–2.
- **Kill line** (ADR-0100): kept only if the codec cases improve by **≥ 15 %** **and** either
  parse's share of the fastest surviving round trip is **≥ 2 %** (ADR-0045's own arithmetic, new
  denominator) **or** the density bench at N = 64 improves by **≥ 3 %**. Otherwise removed. If
  items 1 and 2 are both killed, the denominator is still ~28 µs and the share line cannot be
  met by construction — which is ADR-0045's finding, and phase 4 says so rather than avoiding it.

### 4. A database-backed store — a new crate, off the engine thread

- **Entry**: no ADR is superseded. `Journal` is a trait (ADR-0008, D7); a new implementation is
  allowed. `PRD.md` §5 does not list database stores. **Postgres needs its own ADR** because the
  `postgres` crate carries a Tokio runtime (`CLAUDE.md` §6); **SQLite does not** (`rusqlite`,
  synchronous, no runtime).
- **Shape**: `crates/store-sqlite` (package `fixbolt-store-sqlite`), not a default member of the
  published set, not a dependency of `fixbolt-engine`. It is `FileJournal`'s `Async` shape with a
  different sink: the engine thread writes to the in-memory ring (which stays the resend store,
  ADR-0046) and pushes the record into a pre-allocated SPSC ring; **a writer thread** drains it
  and commits in batches (one transaction per batch — the fix QuickFIX/J users wrote by hand for
  `JdbcStore`, QFJ-119). A full ring is a record not stored, counted, and later a gap fill —
  `FileJournal` `Async`'s existing, documented policy. **There is no `Fsync`-equivalent mode**:
  waiting for a database commit on the engine thread is a blocking call non-negotiable 4 forbids.
  Recovery reads the database on the acceptor thread through `Recovery` (ADR-0034), which may
  block.
- **Non-negotiables**: 1 (engine-thread work is a ring push — proven by an `alloc.rs` case with
  the store attached), 4 (the writer thread is not the engine thread; the mode scripts run with
  the store attached), 6 (`rusqlite`'s bundled C build runs only when this crate is built;
  `scripts/check-no-optional-deps.sh` covers it), 7.
- **Proof**: a crash-and-recover test (kill the process mid-stream, restart, sequence numbers
  continue from the database); `w2w` with the store attached vs `FileJournal` `Async`, same boot.
- **Kill line**: shipped only if the engine-thread allocation count is 0, the wire p50 with the
  store attached is within the band of `FileJournal` `Async`, and the writer sustains **50 000
  messages/s for 60 s** on the desk with no dropped record. Otherwise it stays unpublished.

### 5. A metrics exporter and a dashboard — beside the engine, never in it

- **Entry**: `PRD.md` §5 *"Metrics dashboards, web UIs"* is **narrowed**: a Prometheus-format
  exporter and a committed Grafana dashboard definition leave §5; **a web UI of this project's
  own stays a non-goal**. Grafana is the UI; this project ships the data and one JSON file.
- **Shape**: `crates/metrics` (package `fixbolt-metrics`), optional, off the engine thread. It
  reads only what already exists for exactly this purpose: `Observer::snapshot()` on request
  (ADR-0032) and the pushed event stream with its loss counter (ADR-0035). It serves Prometheus
  text format from a `std::net::TcpListener` on **its own thread** — no async runtime, and the
  text encoder is ~100 lines written here rather than a dependency. `PRD.md` §3's missing
  observations — ring depth and pending-set occupancy — are added to `Snapshot` as counters the
  engine already maintains (read, never computed, on request). The dashboard is
  `tools/grafana/fixbolt.json`.
- **Non-negotiables**: 1 and 4 — scraping must not reach the engine thread beyond the existing
  snapshot handshake; proven by the `alloc.rs` engine-thread count under a 10 Hz scrape and by
  `w2w` with and without the scraper.
- **Proof**: an in-process test that scrapes `/metrics` and asserts every series the dashboard
  queries exists; `w2w` A/B, scraper off vs 10 Hz.
- **Kill line**: if the 10 Hz scrape moves wire p50 or p99 beyond the band, or the engine thread
  allocates, the exporter is redesigned before it ships — it never ships as a cost on the hot
  path.

### 6. Order

1. **Phase 3 closes first.** Phase 4's new crates are born published-shape (metadata, semver
   gate), and the transports change a published crate's API under `cargo-semver-checks`.
2. **The ADRs**: ADR-0099 and ADR-0100 accepted (or rejected) with this one; a Postgres ADR only
   if the owner asks for Postgres.
3. **Off the hot path first**: 5 (exporter + dashboard), then 4 (SQLite store). Both are useful
   without any §9 boot and serve the users phase 3 invited.
4. **Hot path, in denominator order**: 1 (`io_uring`), then 2 (Onload on AF_XDP), then 3
   (SWAR / SIMD) — because 3's kill line is computed from whatever 1 and 2 leave standing. Items
   1 and 2 each need one §9 boot; they may share a boot if the plan pre-builds both arms
   (ADR-0090).

### 7. Excluded from phase 4, by name

A native AF_XDP transport and any userspace TCP stack; `ef_vi` / TCPDirect (no hardware); DPDK
(never); `io_uring` zero-copy receive (needs header split, not on an I211); SQPOLL in `standard`;
Postgres unless its own ADR is accepted; a web UI of our own; FIXP session build (ADR-0097
decision 5 governs it); HA / replication (§5 unchanged).

### 8. Exit criteria

Each a command that passes or fails, on the closing commit, with a CI run id; the §9 rows are
run on the desk and quoted with `scripts/check-machine.sh` output.

| # | Criterion | Command |
|---|---|---|
| 1 | Nothing new is built by default | `cargo build --workspace --no-default-features` and `scripts/check-no-optional-deps.sh` green; the `no-default-features` CI job unchanged |
| 2 | `io_uring` verdict applied | if kept: `cargo test -p fixbolt-engine --features io-uring --test wire` 59 / 59 through the `io_uring` transport, both mode scripts pass for it and are tripped by the wrong mode, its `alloc.rs` case reads 0; if killed: the feature is absent and `measured-costs.md` holds the pair. Either way the A/B output is quoted from the §9 desk |
| 3 | Bypass verdict applied | the same-boot kernel vs `onload` generator-side A/B, two procedures, with `check-machine.sh` showing the bypass rows; if kept, `--test wire` 59 / 59 under `onload` and a second labelled row in `DESIGN.md` §8; if killed, the negative pair in `measured-costs.md` |
| 4 | SIMD verdict applied | `scripts/bench.sh --strict` A/B quoted; if kept, the differential fuzz target and `cargo +nightly miri test -p fixbolt-codec` on the SWAR arm green; if killed, the code absent |
| 5 | The store recovers | `cargo test -p fixbolt-store-sqlite` including the crash-and-recover test; the store's `alloc.rs` engine-thread case reads 0; the 50 000 msg/s × 60 s run quoted |
| 6 | The exporter stays off the hot path | `cargo test -p fixbolt-metrics` (every dashboard series present); engine-thread allocations 0 under a 10 Hz scrape; the scrape-on / scrape-off `w2w` pair within the band |
| — | Phases 1–3 hold | 59 / 59, FIXT 179 / 180, both interop peers 7 / 7, `cargo semver-checks` green or each break named in `CHANGELOG.md` |

## Consequences

**Good**

- The owner gets all five items, and each has a line written in advance that says when it is
  not worth its cost — the thing ADR-0045 and ADR-0074 were protecting, kept rather than thrown
  away.
- `io_uring` enters without any supersession: its own ADR's trigger fired. The density win (one
  CQ peek instead of N reads) could move the `N × 449 ns` term that has governed every gateway
  figure since 2026-08-31.
- The bypass arm costs no engine code; if it loses, phase 4 has spent a boot, not a transport.
- The store and the exporter serve the stranger phase 3 invited in, and neither can touch a
  published latency figure by construction.

**Bad — and accepted**

- **The positioning gets a second clause.** "On kernel TCP" was a one-line promise; after
  ADR-0099 it is "on kernel TCP by default, and on a named bypass with a second row". A reader
  can now mistake the bypass row for the headline; ADR-0099 forbids it and a forbidding sentence
  is weaker than a gate.
- **Three of the five items may be killed.** The research already points that way for two:
  Onload on AF_XDP has a public report of *degraded* latency, and AVX2 on a 166-byte FIX message
  gave 5 %. The owner may end phase 4 with less code than was asked for, and that is the design.
- **Every hot-path item needs the §9 desk**, a boot each (or one shared boot), with the desktop
  line swapped — the operational cost boots A–F already showed.
- **`io_uring` is a security surface** the rest of the engine does not have; a deployment that
  enables it is one Docker and Google both chose to forbid. Opt-in and loud failure do not make it
  safe; they make it a choice.
- **SQPOLL in `hft` burns a second core** per engine; ADR-0012's trade made once more.
- **The store has no synchronous-durability mode.** A regulated deployment wanting "on disk
  before sent" keeps using `FileJournal` `Fsync`; the database copy is at best one batch behind,
  and a full ring drops a record to a gap fill.
- **Two new crates to maintain**, both published under the semver gate, and `rusqlite` brings C
  into the build of anyone who opts in.
- **A dashboard is a promise to keep series names stable**; renaming a metric breaks a user's
  Grafana silently. The exporter's series names become public API.
- **The desk's I211 is 1 GbE with one pending TX stamp** (`hft-playbook.md`): a bypass figure
  taken on it says little about a 25 GbE card, and the generator-side comparison it forces
  includes the Mac mini's own unpinned stack in both arms.

## Sources

The table under *Research*, read 2026-09-23. In-repository: `DESIGN.md` D5, D7, §6 *Wire-to-wire,
NIC to NIC*; `docs/hft-playbook.md` (I211, `igb`, one pending TX stamp);
`docs/reference/measured-costs.md` *The engine is syscall-bound*; `crates/engine/src/journal.rs`
(the `Async` writer ring and its full-ring policy); `crates/session/src/journal.rs` (`Journal`).
`[measured 2026-09-23]` on the desk: `uname -nr`, `ethtool -i enp9s0`, `grep igb /proc/kallsyms`,
`cat /proc/sys/kernel/io_uring_disabled` — read-only.
