# ADR-0171 — A series name is public API, `promtool` is the format oracle, and the dashboard names no data source

- **Status**: Proposed — 2026-09-24
- **Date**: 2026-09-24
- **Deciders**: written by the architect (Opus) for phase 4 rows 1–2; accepted by the owner, or by
  the manager under the owner's standing mandate.
- **Related**: [ADR-0098](ADR-0098-phase-4-is-the-owners-five-items-each-entering-behind-a-measurement-that-can-kill-it.md)
  item 5 and *Consequences* (*"A dashboard is a promise to keep series names stable"*);
  [ADR-0170](ADR-0170-the-metrics-exporter-holds-an-observer-and-nothing-else-allocates-nothing-per-scrape-and-publishes-only-after-its-kill-line.md);
  [ADR-0130](ADR-0130-a-jvm-enters-ci-as-a-second-oracle-quickfixj-by-pinned-jar-and-our-own-judge.md)
  (the pattern for a pinned external oracle in CI);
  [ADR-0160](ADR-0160-six-crates-release-in-lockstep-and-the-packaged-sources-are-the-stranger-before-crates-io-is.md)
  decision 7 (`cargo-semver-checks` cannot see a string); plan
  [2026-09-24-p4-metrics-exporter](../plans/2026-09-24-p4-metrics-exporter.md)

## Context

ADR-0098 accepted that series names become public API and that a test must compare the
dashboard's series with the exporter's. Three things it did not settle:

1. **Nothing in the toolchain sees a renamed string.** `cargo-semver-checks` compares Rust items;
   a series renamed from `fixbolt_connections` to `fixbolt_connections_open` is a green build, a
   green semver job, and a user's Grafana panel that silently reads *No data*.
2. **Our encoder checking our encoder proves nothing about Prometheus.** A test that parses the
   exporter's output with a parser written beside it agrees with itself; the format has corners
   (escaping, grouping, one `TYPE` per name, the final newline, the exact `Content-Type` —
   ADR-0170 *Research*) where both halves can be wrong the same way.
3. **Grafana's export format does not survive file provisioning.** A dashboard exported "for
   sharing" carries `__inputs` and `${DS_PROMETHEUS}`, which the import dialog fills in and file
   provisioning does not — the dashboard then fails with *Datasource `${DS_PROMETHEUS}` was not
   found* (grafana/grafana#10786, #29636, discussion #67075).

## Research

Read 2026-09-24.

| Source | What it says | What it means here |
|---|---|---|
| <https://prometheus.io/docs/practices/naming/> | one base unit (seconds, bytes), a unit suffix, `_total` on counters; *"Do not use labels to store dimensions with high cardinality"* | names below follow it; per-session series are bounded by `MAX_SESSIONS` |
| <https://prometheus.io/docs/prometheus/latest/command-line/promtool/> | `promtool check metrics` reads the text format on stdin and lints it | the format's owner ships a checker; use it rather than write one |
| <https://github.com/grafana/grafana/issues/10786>, <https://github.com/grafana/grafana/issues/29636>, <https://github.com/grafana/grafana/discussions/67075> | `${DS_PROMETHEUS}` from `__inputs` is not resolved under file provisioning | the committed JSON must use a `datasource`-type template variable instead |
| <https://grafana.com/docs/grafana/latest/visualizations/dashboards/build-dashboards/view-dashboard-json-model/> | the JSON model: `schemaVersion`, `templating.list`, panel `targets[].expr`, data source refs by `type`/`uid` | the shape the check script reads |

**Searched and not found:** a Rust crate that validates a Grafana dashboard offline; a FIX engine
publishing a stable metric-name contract.

## Decision

1. **The names, types and labels live in one table in `crates/metrics/src/series.rs`**, which the
   encoder reads and nothing else defines. A second, literal copy of the list lives in
   `crates/metrics/tests/series_names.rs`; the test fails on any difference. Changing a name
   therefore means editing both, which is the moment `CHANGELOG.md` gets its line — the test's
   failure message says so. Removing or renaming a series is a breaking change under the
   crate's semver even though no tool can see it.
2. **Names follow Prometheus's guide**: prefix `fixbolt_`, base units (`_seconds`, `_bytes`),
   `_total` only on counters, gauges for levels. Booleans are gauges of 0/1.
3. **Labels are bounded.** `engine` (caller-chosen, one per observed engine) on every series;
   `conn` (the `ConnId`) on per-session series only, at most `MAX_SESSIONS` live per engine;
   `kind` and `reason` from fixed `'static` sets. No label ever carries a value read off the wire.
4. **`promtool check metrics` is the format oracle, in CI.** A pinned Prometheus release
   (version and SHA-256 written in the script) is downloaded in the `gates` job; a real scrape of
   the test engine is piped through it; any finding fails the job. Where `promtool` is absent
   locally the script prints `SKIPPED, NOT PASSED` and exits non-zero unless told
   `--allow-skip`, so a laptop cannot mistake absence for a pass.
5. **`tools/grafana/fixbolt.json` names no data source.** It declares one template variable of
   type `datasource` (query `prometheus`), and every panel refers to `${datasource}`; it carries
   no `__inputs`, so the same file works through the import dialog and through file
   provisioning. `scripts/check-grafana-dashboard.py` holds that (valid JSON, no `__inputs`, no
   `${DS_`, every panel's data source is the variable, every target has an `expr`, panel ids
   unique), and `crates/metrics/tests/dashboard.rs` holds that every `fixbolt_*` name in the file
   is a series a real scrape produced.
6. **Series that only exist in some deployments are named as such.** The ring series appear only
   under `RingDispatch`, the pre-session series only behind a front door, the event counters only
   with `with_events` (ADR-0170 decisions 7, 8). The dashboard test scrapes a fixture that has all
   three; the dashboard's panel descriptions say which deployment feeds them.

## Consequences

**Good**

- A rename is red in `cargo test` before it is anyone's broken panel.
- The exporter is checked against Prometheus's own tool, not against our reading of the format.
- The dashboard file works for the two ways people load dashboards.

**Bad — and accepted**

- **A Go binary enters CI** as a test oracle (never a build dependency, never shipped), pinned
  and hashed like ADR-0130's jars. A Prometheus release that changes `promtool`'s lints needs the
  pin moved on purpose.
- **Two copies of the series list**, on purpose: the duplication *is* the check. It is the one
  place in this repository where "one rule, one place" is traded for a tripwire.
- **`promtool check metrics` lints; it does not scrape.** The `Content-Type` header and HTTP
  behaviour are held by the crate's own tests, and a real Prometheus scraping it is a manual check
  (plan step 6) until someone makes it a job.
- **No Grafana runs in CI.** The script checks structure; that the panels draw is proven only by
  the live check the plan names, on a machine with a container runtime.
- **Per-session series churn on reconnect** (`conn` is a new id each time). Prometheus handles
  this, but a panel keyed by `conn` shows a new line per reconnect, and there is no counterparty
  name to key it by until `SessionSnapshot` carries one.

## Sources

The table under *Research*. Nothing was measured for this ADR.
