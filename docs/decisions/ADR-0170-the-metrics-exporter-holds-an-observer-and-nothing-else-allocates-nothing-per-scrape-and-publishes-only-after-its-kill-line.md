# ADR-0170 — The metrics exporter holds an `Observer` and nothing else, allocates nothing per scrape, and is published only after its kill line

- **Status**: **Accepted — 2026-09-24, by the manager under the owner's delegation of
  2026-09-18.** Proposed 2026-09-24.
- Revised in place 2026-09-24, while still Proposed: decision 10 no longer has row 2 publish the
  crate to crates.io. The owner decided 2026-09-24 that phase 3 closes with the git tag `v0.1.0`
  and that no crates.io publish is planned (ADR-0161, being written). Row 2 now brings the crate
  into the tagged release family instead. "Published" in this ADR's title means "released by tag".
- Refined, not changed, 2026-09-24 by plan [2026-09-24-p4-metrics-exporter](../plans/2026-09-24-p4-metrics-exporter.md)
  *Sửa 1* (senior review of PR #108, findings F2 and F5): decision 7's handler is
  `FnMut(&'static str, &Event)`, the first argument being the name given to `.engine(name, …)`,
  because `ConnId` restarts at 0 in every engine; decision 3's ask is a new `Observer::ask()`,
  which raises the flag without copying the cell, in place of a `request()` whose copy was thrown
  away. What this ADR decides is unchanged, so no superseding ADR.
- Clarified 2026-09-24 (review of #108): the exporter bounds each connection by one deadline from
  accept (`read_timeout`, default 1 s) covering request and reply; N slow or silent clients delay
  a scrape queued behind them by up to N × `read_timeout` (+ one `fresh_wait`) — see plan *Sửa 1*
  and [a-timeout-per-read-does-not-bound-a-client-that-trickles](../reference/a-timeout-per-read-does-not-bound-a-client-that-trickles.md).
- **Date**: 2026-09-24
- **Deciders**: written by the architect (Opus) for phase 4 row 1; accepted by the owner, or by
  the manager under the owner's standing mandate.
- **Related**: [ADR-0098](ADR-0098-phase-4-is-the-owners-five-items-each-entering-behind-a-measurement-that-can-kill-it.md)
  item 5 (entry, shape, kill line — this ADR does not change any of it, it decides what that
  item left open); [ADR-0032](ADR-0032-observation-is-a-snapshot-taken-on-request.md);
  [ADR-0035](ADR-0035-an-event-is-pushed-and-a-loss-is-counted.md);
  [ADR-0036](ADR-0036-one-mechanism-two-capabilities.md);
  [ADR-0007](ADR-0007-spsc-ring-without-unsafe.md);
  [ADR-0160](ADR-0160-six-crates-release-in-lockstep-and-the-packaged-sources-are-the-stranger-before-crates-io-is.md);
  [ADR-0171](ADR-0171-a-series-name-is-public-api-promtool-is-the-format-oracle-and-the-dashboard-names-no-data-source.md);
  `DESIGN.md` §3, D4, D8, D10b; `PRD.md` §3 (*"Still missing: ring depth and pending-set
  occupancy"*), §5; `CLAUDE.md` §2 non-negotiables 1, 4, 6, 7, 10; plan
  [2026-09-24-p4-metrics-exporter](../plans/2026-09-24-p4-metrics-exporter.md)

## Context

ADR-0098 item 5 fixed the frame: a crate `fixbolt-metrics`, on its own thread, reading only
`Observer::snapshot` on request and the event stream, serving Prometheus text from a
`std::net::TcpListener`, no async runtime, a hand-written encoder, two new numbers in
`Snapshot`, and a kill line (a 10 Hz scrape keeps `w2w` p50/p99 in band and the engine thread
allocates nothing). Reading the code on `main` at `094bfc3` (2026-09-24) leaves seven questions
that frame does not answer:

1. **`Observer::request()` returns the *previous* snapshot and asks for the next**
   (`crates/engine/src/observe.rs:497-503`). A scraper calling it once per scrape at
   Prometheus's default 15 s interval would serve a figure 15 s old every time.
2. **A `standard` engine asleep in `poll` does not see the request** until something wakes it —
   `Shared::wanted()` is read at the top of `Engine::turn` (`crates/engine/src/lib.rs:1204-1216`)
   and nothing in `request()` wakes the engine. A fresh snapshot is therefore not guaranteed on
   any schedule the exporter controls.
3. **Every request costs the engine thread one snapshot build** — up to 64 `SessionSnapshot`s on
   the stack and one `try_lock` + copy into the cell (`observe.rs:400-406`). Two Prometheus
   replicas (the usual HA pair), or a `curl` loop, multiply that by whatever they like.
4. **`Observer::events()` drains** (`observe.rs:517-519`). An exporter that reads the event stream
   takes the events away from any other reader the application has.
5. **`tools/w2w` counts allocations on every thread** and asserts zero
   (`tools/w2w/src/main.rs:294-343`); `DESIGN.md` §6 *Allocation* says so. The kill-line pair is
   measured with `w2w`, so an exporter thread that allocates per scrape fails the pair for a
   reason that is not the engine's.
6. **`PRD.md` §3's two missing numbers have no home.** Ring depth is readable from
   `ring::Producer::free()` (`crates/engine/src/ring.rs:195-200`), which only `RingDispatch`
   holds, behind the `Dispatch` trait. Pending-set occupancy is `PendingSet::len()` plus the
   parked set (`lib.rs:3500`), which the serving loop owns and the engine does not — the same
   shape as `unframeable_prelogon`, which the loop hands over each turn through
   `Engine::note_unframeable` (`lib.rs:836-838`).
7. **ADR-0160 publishes six crates in lockstep.** A seventh crate whose kill line is measured
   *after* it merges could otherwise ride the next release before its verdict exists.

## Research

Read 2026-09-24.

| Source | What it says | What it means here |
|---|---|---|
| <https://prometheus.io/docs/instrumenting/exposition_formats/> | text format 0.0.4: `HELP`/`TYPE` before samples, one group per metric, label values escape `\\`, `\"`, `\n`, the last line ends in `\n`, timestamps optional; *"Starting with Prometheus 3.0, scrape targets **must** return a valid `Content-Type` header … the scrape will fail"* | the encoder is small; the header is not optional |
| <https://prometheus.io/docs/instrumenting/content_negotiation/> | Prometheus's default `Accept` lists OpenMetrics 1.0.0, 0.0.1, text 1.0.0, text 0.0.4; a server with no match *"MUST use PrometheusText0.0.4 as a last resort"* | answering 0.0.4 regardless of `Accept` is within the protocol |
| <https://github.com/prometheus/prometheus/issues/15777> | `text/plain; version=0.0.4, charset=utf-8` (comma) failed to parse in Prometheus 3 | the header is written once, as a constant, and a test pins its exact bytes |
| <https://docs.rs/crate/metrics-exporter-prometheus/latest/source/Cargo.toml.orig> | default features `http-listener` → `async-runtime = ["tokio", …]` and `_hyper-server` | the ready-made exporter brings Tokio and hyper: an ADR-grade dependency (`CLAUDE.md` §6) for ~20 series |
| <https://docs.rs/crate/prometheus-client/latest/source/Cargo.toml.orig> | 0.25.1: `dtoa`, `itoa`, `parking_lot`, a derive crate; no HTTP server | still needs the HTTP half written here; its registry model (atomics you increment) does not fit a value that is *pulled* from a `Snapshot` |
| <https://docs.rs/crate/prometheus/latest/source/Cargo.toml.orig> | 0.14.0: `protobuf` on by default, `parking_lot`, `lazy_static`, `thiserror`, `fnv`, `memchr`; no HTTP server | same, with more |
| <https://github.com/aeron-io/aeron/wiki/Monitoring-and-Debugging>, <https://aeron.io/docs/cookbook-content/aeron-read-counters/> | Aeron keeps counters as atomics in a memory-mapped CnC file that `AeronStat` reads from another process *"with no significant impact"* | the low-latency norm is: the hot path writes plain counters, a reader elsewhere copies them; the reader never calls into the hot thread |
| <https://github.com/artiofix/artio/wiki/Operational-Concerns> (via ADR-0098) | Artio exposes its counters through Aeron's counter file | same pattern, in a FIX engine |
| <https://github.com/rigtorp/Seqlock> | a seqlock never blocks its writer; the reader retries | the other classic answer to "copy a struct off a hot thread" |
| <https://github.com/rust-lang/rfcs/pull/3301>, <https://mara.nl/atomics/inspiration.html> | in Rust (and C++), a seqlock over non-atomic data is a data race — UB even when the reader discards the copy — until per-byte atomic memcpy exists | a seqlock here needs `unsafe` (non-negotiable 8) and is not sound in today's model; the existing `try_lock` cell (ADR-0032, ADR-0007's preference) stays |

**Searched and not found:** an open-source FIX engine shipping its own Prometheus exporter
(QuickFIX/J goes through JMX agents, Artio through Aeron counters — ADR-0098 *Research*); a
measurement of any exporter's effect on a co-located low-latency thread.

## Decision

1. **The exporter holds an `Observer` — never `Handles`, never an `Admin`.** It can look and it
   cannot change, stop, or speak for the engine (ADR-0036's capability split, now a type-level
   fact about the crate). One exporter serves **one or more named engines**
   (`engine="<name>"` label), because a sharded deployment is several engines.
2. **Snapshots are requested at most once per `min_request_interval` (default 100 ms), whatever
   the scrape rate.** A scrape inside that interval is served from the exporter's copy. This caps
   the engine-thread cost of observation at ten snapshot builds a second regardless of how many
   scrapers there are — the kill line's 10 Hz becomes a ceiling the exporter enforces, not a
   rate a user must remember.
3. **Freshness is asked for, waited for briefly, and then reported, never faked.** On a scrape
   that is allowed to ask: note `Observer::published()`, `request()`, sleep-poll on the
   **exporter** thread for up to `fresh_wait` (default 50 ms) for the count to move, then read
   with a new **`Observer::latest()`** — the cell's current copy without raising the flag (an
   additive engine API; a second `request()` would cost the engine a second build for nothing).
   The exporter publishes `fixbolt_snapshot_age_seconds`: time since it last saw `published()`
   move. A `standard` engine asleep in `poll` is not woken by the exporter — that would make
   observation a cause of engine work — so its age grows honestly until it wakes.
4. **The exporter thread allocates nothing per scrape after start-up.** Its response buffer is
   reserved once for the worst case (every engine at `MAX_SESSIONS`, every per-session series,
   the longest value of each), its request buffer is a fixed array, its event buffer is reserved
   to `EVENT_CAPACITY`, numbers are written without floats (the skew is milliseconds printed as
   seconds with three decimals). Not required by non-negotiable 1, which is about the engine
   thread; required because (a) `w2w`, the kill line's instrument, counts every thread and must
   keep asserting zero with the exporter in the process, and (b) a neighbour thread in `malloc`
   shares the allocator with the engine. Proven by `crates/metrics/benches/alloc.rs` (process-wide
   count over a window of scrapes) and by the `w2w` pair itself.
5. **One loop, sleeping.** The exporter thread wakes every `tick` (default 100 ms): drains events
   if it owns them, accepts every pending connection (non-blocking listener), answers each on a
   blocking socket with a read timeout (default 1 s) and a 4 KiB request cap, then sleeps. It
   never spins and never shares a core it was not given: it is spawned by the caller's thread and
   inherits that thread's affinity, which `GUIDE.md` names as the trap it is (spawn it before the
   engine thread pins itself). Its thread is named `fixbolt-metrics` so `ps -L` can find it.
6. **HTTP is the least that Prometheus needs, written here.** `GET`/`HEAD` of `/metrics` → 200
   with exactly `Content-Type: text/plain; version=0.0.4; charset=utf-8`, `Content-Length`,
   `Connection: close`; `/healthz` → 200 or 503 from `Snapshot::healthy()` (the function whose
   rustdoc already says a health endpoint must use it); anything else 404/405/400. No keep-alive,
   no compression, no TLS, no authentication: the listener binds where the caller says, and
   `GUIDE.md` says loopback or a private interface.
7. **Events are opt-in, because reading them takes them.** By default the exporter does not
   drain the event stream and exports only `events_lost`. `with_events(handler)` makes the
   exporter the stream's only reader: it counts `fixbolt_events_total{kind}` and
   `fixbolt_session_ends_total{reason}` over fixed, `'static` label sets (`DropReason` is
   fieldless, ~24 variants; an unknown future variant is `reason="other"`), and hands each event
   to the caller's closure on the exporter thread.
8. **Two numbers enter `Snapshot`, as one new type.** `observe::Occupancy { used, capacity }`
   (`Copy`, two `usize`). `Snapshot::ring_to_app() -> Option<Occupancy>` in **bytes**, filled from
   a new provided method `Dispatch::ring_to_app(&self) -> Option<Occupancy>` (default `None`;
   `RingDispatch` answers from `Producer::free()` and a new `Producer::capacity()`), read only
   when a snapshot is built. `Snapshot::presession_slots() -> Option<Occupancy>`, handed to the
   engine by the serving loop through a new `Engine::note_presession_slots(used, capacity)` beside
   every existing `note_unframeable` call (`used` = pending + parked, `capacity` =
   `Limits::pending()`), one store per loop iteration. **`None` means "nothing reported"** —
   `InlineDispatch`, an initiator, a hand-built engine, `serve_sharded_hft`'s fan — and the
   exporter then omits the series rather than exporting a zero that would read as "empty". The
   engine→application direction only: the reply ring's fullness is the application's own
   `push` result.
9. **Zero runtime dependencies besides `fixbolt-engine`, taken with `default-features = false`.**
   The crate needs only `observe`, which no feature gates; a user building `hft` without
   `standard` gets no `libc` from it. `scripts/check-no-optional-deps.sh` gains
   `fixbolt-metrics:libc`.
10. **It joins the lockstep family, and ships only after its kill line.** `version.workspace =
    true`, `fixbolt-engine = { version = "=<workspace version>", path, default-features = false }`,
    the `include` allowlist (`src/**`, `README.md`, the two licence copies — not `examples/**`,
    whose examples need `fixbolt-engine`'s `standard` through a path-only dev-dependency that the
    `.crate` strips),
    inherited `rust-version`, docs.rs defaults — ADR-0160's mould, held by
    `scripts/check-release-versions.sh` from the day it merges. It merges with **`publish =
    false`**, outside the release family. Phase 4 row 2 brings it **into the tagged release
    family** — adds it to that script's list of released crates, with its `publish` field set the
    way ADR-0161 sets it for the other six — **in the same commit as the scrape-on/scrape-off
    `w2w` pair that passes ADR-0098's kill line**. **Publishing to crates.io is not planned**
    (ADR-0161). If the pair fails, the crate stays outside the release family and is redesigned
    (ADR-0098), and the next tagged release leaves it out without anyone having to remember.

## Consequences

**Good**

- The kill line is enforced by construction as well as measured: no scrape pattern can make the
  engine build more than ten snapshots a second, and the engine never learns an exporter exists
  beyond the flag it already had.
- `PRD.md` §3's last two missing observations close with no new mechanism: two reads at
  snapshot time, one store per serving-loop iteration.
- The exporter cannot become a way to stop or steer the engine; giving an operator tool a
  `/metrics` port gives it nothing else.
- `w2w` measures the kill-line pair without learning about threads it did not start: its
  existing every-thread zero stays the assertion.
- Stays inside `CLAUDE.md` §6: no dependency to justify, no runtime, no ADR for Tokio.

**Bad — and accepted**

- **A snapshot can be up to `min_request_interval` + `fresh_wait` old at best, and much older
  from a sleeping `standard` engine.** `fixbolt_snapshot_age_seconds` says so; nothing hides it.
- **Hand-written HTTP is a surface we own.** It is bounded (4 KiB request, 1 s read timeout,
  one connection at a time, `Connection: close`) and served from one thread, so a slow client
  delays other scrapes by up to the timeout; it is not hardened for the open internet and the
  documentation says to keep it off it.
- **One scraper at a time.** A second scraper waits behind the first; at the rates Prometheus
  uses this is milliseconds, and it is the price of a single sleeping thread.
- **Opting into events takes them from everyone else.** Documented; the closure is the way to
  keep reading them.
- **The per-session series are labelled by `ConnId`,** which is new on every connection, so a
  reconnect starts a new series ([ADR-0171](ADR-0171-a-series-name-is-public-api-promtool-is-the-format-oracle-and-the-dashboard-names-no-data-source.md)
  decision 3). Bounded by `MAX_SESSIONS` live at once; not a counterparty's name, because
  `SessionSnapshot` does not carry one today.
- **Six additive engine items** (`Occupancy`, `Snapshot::{ring_to_app, presession_slots}`,
  `Dispatch::ring_to_app`, `Producer::capacity`, `Observer::latest`,
  `Engine::note_presession_slots`) arrive for one optional crate, under `cargo-semver-checks`.
  A provided trait method can collide with a same-named method a user added to their own
  `Dispatch` implementation; the name is chosen to make that unlikely, not impossible.
- **One store per serving-loop iteration** is added to every front door whether or not anybody
  observes. `[unmeasured]`; the row-2 `w2w` pair runs on code that already carries it in both
  arms, so it cannot see it — the next `--strict` read of `turn.rs` on the §9 line is where it
  would show, and it is named there as a suspect if anything moves.
- **The exporter inherits its spawner's affinity.** Only documentation guards it; a pinned
  engine thread that spawns the exporter puts both on one isolated core.

## Sources

The table under *Research*. In-repository: the file and line references in *Context*, read at
`094bfc3` in worktree `fb-p4r1`, 2026-09-24. Nothing was measured for this ADR.
