# `tools/` — internals

Not a crate under `crates/` — four standalone binaries, each in
[DESIGN.md §3](../DESIGN.md#3-crates)'s own crate table (`tools/w2w`, `tools/jrnl`,
`tools/interop`, `tools/attr-scan`), plus `tools/interop-qfj`, which is not a crate at all
([ADR-0130](../decisions/ADR-0130-a-jvm-enters-ci-as-a-second-oracle-quickfixj-by-pinned-jar-and-our-own-judge.md)).
Each is either the binary a machine check traces or a counterparty this repository builds no
other way.

## Tools, files, and what each keeps

| Tool | Files | Keeps |
|---|---|---|
| `w2w` | `src/main.rs` | Wire-to-wire harness: the two mode checks trace this binary; counts allocations on both threads over the timed window and asserts zero. `[2026-09-24]` `--metrics <addr>` spawns a `fixbolt-metrics` exporter on the main thread, before any core is pinned, and prints `metrics: <addr>`; refused by `--connect` (ADR-0170) |
| | `src/pair.rs` | Pairing a request's hardware RX stamp with its reply's hardware TX stamp — pure, no socket, no clock |
| `interop` | `src/main.rs` | Both roles against a real `libquickfix` over kernel TCP, `--role initiator` and `--role acceptor`; behind `#[cfg(all(feature = "tls", target_os = "linux"))]`, the `acceptor` role's TLS branch (`into_tls_table` + `load_pem` + `serve_tls_requiring`) and a background thread printing `interop: event <kind>` off `Observer::events` |
| | `src/desk.rs` | The application behind `--role acceptor`: acknowledges each order as New (`35=8` with `150=0`, `39=0`, `11=` echoed — nothing is filled), answers nothing else — the tool's own handler, not `library`'s example. Also the application behind `--role dial` and behind both QuickFIX/J arms, so a change to it moves three gates at once |
| | `src/reconnect.rs` | `--role reconnect`: the engine's own reconnect loop against a `libquickfix` acceptor that dies and restarts |
| | `src/dial.rs` | `--role dial` — the engine's real initiator door (`fixbolt::connect_and_serve`, and under `tls` + Linux `fixbolt_engine::connect_and_serve_tls`) against a settings file, for `scripts/interop-qfj.sh`'s `initiator-plain` / `initiator-tls` arms. Unlike `--role initiator` (a hand-rolled `Session` over a blocking socket), this role runs the production engine loop, because TLS on the initiator side exists only inside it (ADR-0130) |
| `jrnl` | `src/main.rs` | Reads a journal file from outside the process that wrote it; warns on a torn tail or bad checksum, exit code 2 |
| `attr-scan` | `src/main.rs` | Prints every inner attribute at a crate root, as the Rust lexer sees it — the eyes of `scripts/check-no-crate-root-allow.sh` |
| `interop-qfj` *(not a crate)* | `Judge.java` | This repository's own judge against QuickFIX/J 3.0.2 (non-negotiable 9: no QuickFIX source copied, public API only), one file, both roles (`initiator` and `acceptor`). Its own `quickfix.Log` (`RawLog`) records every raw wire string and prints `qfj: in\|out <string>`; every one of the seven steps and the `PASS n/7` / `FAIL n/7` line is judged on those raw strings, never on QFJ's `fromApp`/`fromAdmin` callbacks, because QFJ hands the application a `43=Y` replay only after consuming the admin messages around it. Compiled by `javac` against the five jars `scripts/interop-qfj.sh` pins — no Maven, no Gradle, no `pom.xml` |

## Read in this order

Each tool is independent; read whichever one a task needs. Within a tool:

- `w2w`: `main.rs` before `pair.rs` — `pair.rs` is a pure helper `main.rs` calls
- `interop`: `main.rs` before `desk.rs`, `reconnect.rs` and `dial.rs` — the roles the entry point
  dispatches to
- `interop-qfj`: `Judge.java` alone — one file, both roles selected by its first argument

## Tests and gates that guard them

- `w2w` — no test crate; it is itself the proof for
  `scripts/check-no-kernel-sleep-by-ctxt.sh` ([ADR-0072](../decisions/ADR-0072-a-tracer-free-check-that-the-hft-engine-thread-never-sleeps.md))
  and the source of every wire-to-wire figure in `docs/reference/measured-costs.md`.
  `[2026-09-24]` `--metrics` is loaded, not driven, by `scripts/scrape-loop.sh <addr> <hz>
  <seconds>` — another process, so its own allocations never enter `w2w`'s count; it paces
  attempts on a fixed schedule rather than waiting for each answer
  ([a-synchronous-scrape-waits-for-the-exporters-next-tick](../reference/a-synchronous-scrape-waits-for-the-exporters-next-tick.md)),
  and both mode scripts run with it and `W2W_EXTRA="--metrics <addr>"` beside them
- `interop` — driven by `scripts/interop.sh`, which builds the C++ counterparties and reports
  a pass count per role; never built or run by `cargo test`
- `jrnl` — `tests/cli.rs`, run as the built binary; `crates/engine/tests/journal_reader.rs`
  covers the library reader this binary wraps
- `attr-scan` — driven by `scripts/check-no-crate-root-allow.sh`; no `cargo test` of its own,
  since its whole job is what that script does with its output
- `interop-qfj` — driven by `scripts/interop-qfj.sh`, and by the blocking CI job of the same
  name; four arms, each `7 / 7` plus `shutdown`/`clean` (and `kernel` on the two TLS arms). Its
  own gate: `scripts/check-no-optional-deps.sh` asks `fixbolt-interop:rustls` and
  `fixbolt-interop:ktls-core` separately, so the `tls` feature this tool's `Cargo.toml` declares
  cannot leak into `cargo test --all --no-default-features` by forwarding silently

## `tools/grafana/` — not a crate, not a binary

`[2026-09-24]` `tools/grafana/fixbolt.json` is the one committed Grafana dashboard, checked in
because it names no data source of its own (a `datasource`-type template variable instead) and
so loads unchanged through the import dialog or through file provisioning
([ADR-0171](../decisions/ADR-0171-a-series-name-is-public-api-promtool-is-the-format-oracle-and-the-dashboard-names-no-data-source.md)
decision 5). Its panels, rows and what each needs are described where they are decided, not
repeated here: ADR-0171 and the plan's *Cách làm* §D. Two things guard it, both in the `gates`
CI job: `scripts/check-grafana-dashboard.py` reads its JSON shape (no `__inputs`, no `${DS_`,
every panel on the `${datasource}` variable, every target has an `expr`, panel ids unique), and
`crates/metrics/tests/dashboard.rs::every_series_the_dashboard_queries_is_scraped` scrapes a
fixture engine and asserts every `fixbolt_*` name the file queries is a series that scrape
actually produced — a renamed or removed series fails there before it is anyone's broken panel.
