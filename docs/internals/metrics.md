# `metrics` — internals

Layer L4 in [DESIGN.md §3](../DESIGN.md#3-crates), the package **`fixbolt-metrics`**: a
Prometheus exporter that holds an `observe::Observer` for each engine it watches and nothing
else. Why it is shaped this way — one sleeping thread, a ceiling on how often it asks, nothing
allocated per scrape, events only on request, `publish = false` until its kill line — is
[ADR-0170](../decisions/ADR-0170-the-metrics-exporter-holds-an-observer-and-nothing-else-allocates-nothing-per-scrape-and-publishes-only-after-its-kill-line.md);
why a series name is public API is
[ADR-0171](../decisions/ADR-0171-a-series-name-is-public-api-promtool-is-the-format-oracle-and-the-dashboard-names-no-data-source.md).
The two numbers it needed from the engine — `Snapshot::ring_to_app`,
`Snapshot::presession_slots` — and `Observer::latest` live in `engine`'s `observe`, not here.

## Files, and what each keeps

| File | Keeps |
|---|---|
| `src/lib.rs` | The public surface: `Exporter`, its `Builder`, `ExportError`, the `DEFAULT_*` durations. `spawn` binds, reserves every buffer and names the thread; `stop` and `Drop` join it |
| `src/series.rs` | **The one definition of every series**: name, type, labels, `HELP`, and which value it prints (`Source`). The fixed `kind` and `reason` label sets, and the mapping from an event to them — `DropReason` by its `Debug` name, because the engine does not re-export the type |
| `src/encode.rs` | The text format 0.0.4 over an internal `EngineView`, into a buffer that refuses to grow; `capacity_for`, the worst-case size worked out from the series table; integer and milliseconds-as-seconds printing without floats |
| `src/http.rs` | Reading one request into a fixed 4 KiB array, judging its request line, and writing a response header on the stack. The exact `Content-Type` |
| `src/thread.rs` | The loop: drain events if owned, accept every pending connection, answer each, sleep. `refresh` is where the ceiling on asking and the bounded wait for a fresh snapshot live |

## Read in this order

1. `src/series.rs` — what a scrape contains, and nothing about how
2. `src/lib.rs` — the builder, and what `spawn` sets up once
3. `src/thread.rs` — one wake, then `refresh`: ask at most once per interval, wait on this
   thread, read with `latest`
4. `src/encode.rs` — `each_sample` is the one place that decides what is printed and what is
   left out; `capacity_for` is why nothing grows
5. `src/http.rs` — last, because it is the least interesting and the most likely to be right

## Tests that guard it

- `tests/series_names.rs` — the series list written out by hand; a rename fails here and the
  message says to add a `CHANGELOG.md` line
- `tests/exporter.rs` — a real engine behind a real socket: the exact `Content-Type`, the scrape
  storm's ceiling on snapshot builds, the snapshot age growing while an engine sleeps,
  `/healthz` following `Snapshot::healthy`, refusal of oversized and silent requests, events
  left alone unless handed over, a series with no source left out, `stop` joining the thread
- unit tests in `src/encode.rs` (`a_full_snapshot_fits_the_reserved_buffer`, escaping, skew
  printing), `src/series.rs` (`with_events_every_drop_reason_today_has_its_own_label`) and
  `src/http.rs` (request-line judging)
- `benches/alloc.rs` — `metrics-idle` and `metrics-scraped`, counting allocations on **every**
  thread of the process, each case checking its own path ran
- `examples/acceptor_with_metrics.rs` — `serve` with an exporter beside it, spawned in the
  order the affinity trap requires
