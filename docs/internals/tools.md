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
| `w2w` | `src/main.rs` | Wire-to-wire harness: the two mode checks trace this binary; counts allocations on both threads over the timed window and asserts zero |
| | `src/pair.rs` | Pairing a request's hardware RX stamp with its reply's hardware TX stamp — pure, no socket, no clock |
| `interop` | `src/main.rs` | Both roles against a real `libquickfix` over kernel TCP, `--role initiator` and `--role acceptor`; behind `#[cfg(all(feature = "tls", target_os = "linux"))]`, the `acceptor` role's TLS branch (`into_tls_table` + `load_pem` + `serve_tls_requiring`) and a background thread printing `interop: event <kind>` off `Observer::events` |
| | `src/desk.rs` | The application behind `--role acceptor`: fills orders, answers nothing else — the tool's own handler, not `library`'s example. Also the application behind `--role dial` and behind both QuickFIX/J arms, so a change to it moves three gates at once |
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
  and the source of every wire-to-wire figure in `docs/reference/measured-costs.md`
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
