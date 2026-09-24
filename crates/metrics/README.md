# fixbolt-metrics

A Prometheus exporter for [fixbolt](../../README.md)
engines. It holds an `Observer` for each engine it watches and nothing else: it can look, and it
cannot change, stop, or speak for an engine. One thread, named `fixbolt-metrics`, wakes every
`tick`, answers the scrapes that are waiting, and sleeps. It allocates nothing per scrape.

**Not released yet.** This crate is `publish = false` and outside the tagged release family
until its cost to a running engine is measured and found inside the kill line
([ADR-0170](../../docs/decisions/ADR-0170-the-metrics-exporter-holds-an-observer-and-nothing-else-allocates-nothing-per-scrape-and-publishes-only-after-its-kill-line.md)
decision 10).

```rust,no_run
use fixbolt_engine::observe::Handles;
use fixbolt_metrics::Exporter;

let handles = Handles::new();
// Before the engine thread pins itself: the exporter inherits the spawning
// thread's CPU affinity.
let exporter = Exporter::builder("127.0.0.1:9464".parse().unwrap())
    .engine("acceptor", handles.observer())
    .spawn()
    .unwrap();
// … hand `handles` to `fixbolt_engine::serve` …
exporter.stop();
```

| Endpoint | Answer |
|---|---|
| `GET`/`HEAD /metrics` | `200`, `Content-Type: text/plain; version=0.0.4; charset=utf-8` |
| `GET`/`HEAD /healthz` | `200` when every engine's `Snapshot::healthy()`, else `503` |

Every series name, type and label set is public API and listed in `src/series.rs`
(ADR-0171). An engine is asked for a snapshot at most once per `min_request_interval`
(100 ms) whatever the scrape rate, and an engine asleep in `standard` mode is never woken —
`fixbolt_snapshot_age_seconds` grows instead. Events are read only after `with_events`,
because reading one takes it.

No TLS and no authentication: bind to loopback or a private interface.

## Licence

Dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option.
