# `tools/` — internals

Not a crate under `crates/` — four standalone binaries, each in
[DESIGN.md §3](../DESIGN.md#3-crates)'s own crate table (`tools/w2w`, `tools/jrnl`,
`tools/interop`, `tools/attr-scan`). Each is either the binary a machine check traces or a
counterparty this repository builds no other way.

## Tools, files, and what each keeps

| Tool | Files | Keeps |
|---|---|---|
| `w2w` | `src/main.rs` | Wire-to-wire harness: the two mode checks trace this binary; counts allocations on both threads over the timed window and asserts zero |
| | `src/pair.rs` | Pairing a request's hardware RX stamp with its reply's hardware TX stamp — pure, no socket, no clock |
| `interop` | `src/main.rs` | Both roles against a real `libquickfix` over kernel TCP, `--role initiator` and `--role acceptor` |
| | `src/desk.rs` | The application behind `--role acceptor`: fills orders, answers nothing else — the tool's own handler, not `library`'s example |
| | `src/reconnect.rs` | `--role reconnect`: the engine's own reconnect loop against a `libquickfix` acceptor that dies and restarts |
| `jrnl` | `src/main.rs` | Reads a journal file from outside the process that wrote it; warns on a torn tail or bad checksum, exit code 2 |
| `attr-scan` | `src/main.rs` | Prints every inner attribute at a crate root, as the Rust lexer sees it — the eyes of `scripts/check-no-crate-root-allow.sh` |

## Read in this order

Each tool is independent; read whichever one a task needs. Within a tool:

- `w2w`: `main.rs` before `pair.rs` — `pair.rs` is a pure helper `main.rs` calls
- `interop`: `main.rs` before `desk.rs` and `reconnect.rs` — the two roles the entry point
  dispatches to

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
