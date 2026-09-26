# Architecture

For a developer about to change fixbolt's code: **where** behaviour lives and **what must not
break**. It is a map, not the design — *what* was decided is [docs/DESIGN.md](docs/DESIGN.md) §4,
*why* is the ADR each section names, which file holds what is
[docs/internals/](docs/internals/README.md), and the rules live once, in [CLAUDE.md](CLAUDE.md).
Names below are meant to be searched for; there are deliberately no line numbers.

## Bird's-eye view

Acceptor and initiator run on one session core, chosen by a type parameter
([ADR-0004](docs/decisions/ADR-0004-bidirectional-engine.md)). One inbound message, on the engine
thread, in order:

1. The **transport** reads bytes into a fixed per-connection buffer (`TcpTransport`).
2. The **framer** finds where the message ends from `9=` alone (`Framer`).
3. The **session** parses it in place, checks it against the generated tables, runs the session
   protocol, and hands an application message on (`Session::received`).
4. The **dispatcher** delivers it on the same thread (`InlineDispatch`) or through a ring to the
   application's thread (`RingDispatch`).
5. A reply is a pre-sorted `Template`, patched and written back; what was sent is kept in the
   `Journal` for a resend.

Time is read once per turn and passed in (`Session::tick`). Idle behaviour depends on the
**mode**: `standard` (the default) blocks; `hft` (opt-in, Linux) spins on a pinned core
([ADR-0013](docs/decisions/ADR-0013-two-modes-standard-and-hft.md)). Layering:
[DESIGN.md §2](docs/DESIGN.md#2-layers); the guiding finding:
[§1](docs/DESIGN.md#1-the-finding-this-architecture-is-built-around).

## Codemap

Crates in dependency order; the authoritative table is [DESIGN.md §3](docs/DESIGN.md#3-crates).

### `crates/codec` — parse and serialise in place (L1)

Entry points: `parse_into`, `MessageView`, `FieldIndex`, `Template`, `TemplateBuilder`,
`TimestampCache`, `GroupIter`, the `Encoding` and `Dictionary` traits (D2, D9,
[ADR-0003](docs/decisions/ADR-0003-message-representation.md)).

**Architecture Invariant:** `codec` has **no runtime dependencies** — its `Cargo.toml` has
`[dev-dependencies]` only. Rule: CLAUDE.md §6 *Dependencies*. Check: hand-check on any change to
`crates/codec/Cargo.toml`.

**Architecture Invariant:** nothing on the parse or serialise path allocates. Rule: CLAUDE.md §2
item 1. Check: `crates/codec/benches/alloc.rs`, run by `scripts/bench.sh` in the `bench` CI job.

### `crates/dict` — the FIX tables, generated at build time

Entry points: `build.rs`, the `Tables` trait, `Fix44`, `FieldType`; behind the `fix50sp2`
feature, `Fixt11Fix50Sp2Tables`. `build.rs` turns the XML in `crates/dict/spec/` into tables.

**Architecture Invariant:** field order comes from these generated tables, never from a call
site. Rule: CLAUDE.md §2 item 5 (D3). Check: hand-check; the positional acceptance comparator
goes red on a misordering the corpus exercises.

**Architecture Invariant:** **no dictionary is chosen or loaded at run time**; a session's
encoding carries its compiled table
([ADR-0080](docs/decisions/ADR-0080-the-dictionary-rides-the-encoding-and-a-fixt-session-is-one-table-built-from-two-xml-files.md)
decision 1). Rule: D3, under CLAUDE.md §2 item 5. Check: hand-check.

**Architecture Invariant:** exactly three QuickFIX files ship, under `crates/dict/spec/`,
byte-identical to a pinned commit; no QuickFIX source is copied
([ADR-0001](docs/decisions/ADR-0001-relationship-to-quickfix.md),
[ADR-0104](docs/decisions/ADR-0104-the-published-dictionary-is-quickfixs-xml-shipped-with-a-notice.md)).
Rule: CLAUDE.md §2 item 9. Check: `scripts/check-dict-spec-pin.sh`; `git add` stays the control.

### `crates/sbe` and `crates/sbe-gen` — the second encoding

`sbe`: SBE 1.0 over `&'static` tables (`SbeView`, `Sbe`, `Schema`, `MessageWriter`), `no_std`,
`forbid(unsafe_code)`. `sbe-gen`: `generate` builds those tables from a schema in a `build.rs` (D16,
[ADR-0081](docs/decisions/ADR-0081-sbe-tables-come-from-this-repositorys-generator-and-the-oracle-is-the-specs-bytes-plus-sbe-tool-behind-a-script.md)).

### `crates/session` — the FIX session protocol as a pure state machine (L2)

Entry points: `Session<E, R>` and its `received`, `tick`, `connect`, `disconnect`; the
`Application` trait (`on_message`, `on_logon`); `Role` with `Acceptor` and `Initiator`; `Config`,
`DictionaryChecks`, `ResetPolicy`; `DropReason`; the `Journal` trait; modules `schedule` and
`clock`. Output goes through a caller's `emit` closure; the state change is returned as a `Link`.

**Architecture Invariant:** the session layer has **no socket, no clock, no allocation and no
`format!`**; it depends only on `codec` and `dict`, time arrives as an argument (D13), and errors
are fieldless enums. Rule: CLAUDE.md §2 item 2 (D1). Check: `crates/session/benches/alloc.rs` for allocation; the rest is a
hand-check.

**Architecture Invariant:** a session change is not done until the 59 QuickFIX acceptance
definitions pass, 59 / 59. Rule: CLAUDE.md §2 item 3. Check: `crates/conformance`, in process
(`cargo test -p fixbolt-session --test score`) and over a socket
(`cargo test -p fixbolt-engine --test wire`); the FIXT corpus behind `fix50sp2` in the `gates`
job; `scripts/check-socket-corpus-under-contention.sh` under load
([docs/CONFORMANCE.md](docs/CONFORMANCE.md)).

### `crates/engine` — sockets, threads, and the loop that drives sessions (L3)

Every module and its decision: [DESIGN.md §3 *What `engine` contains*](docs/DESIGN.md#what-engine-contains);
reading order: [docs/internals/engine.md](docs/internals/engine.md). Start at `Engine::turn` (one
pass of the loop), then the doors: `serve`, `serve_hft`, `connect_and_serve` and their variants.
By concern:

- I/O: `transport` (`Transport`, `TcpTransport`), `frame` (`Framer`), `wait` (the idle strategy
  per mode), `poll`/`block`/`waker` (`standard` only), `transport::uring` (feature `io-uring`),
  `tls` (feature `tls`).
- Before a session exists: `presession` (`PendingSet`, `Registry`, `Table`, `Limits`, `Route`).
- Delivery: `dispatch` (`Dispatch`, `InlineDispatch`, `RingDispatch`), `ring`, `backpressure`.
- State kept: `journal` (`MemJournal`, `FileJournal`), `recovery` (`Recovery`), `msglog`
  (`MessageLog`, `FileLog`), `redact` (`mask`).
- Operating it: `observe` (`Handles`, `Observer`, `Admin`), `origin` (`Sender`), `settings`
  (`Settings`), `reconnect` (`Policy`), `affinity` and `shard` (Linux, feature `affinity`).

**Architecture Invariant:** in `hft` the engine thread never sleeps in the kernel while serving;
in `standard` it **must** block when idle (D8,
[ADR-0012](docs/decisions/ADR-0012-latency-first-and-one-session-per-polling-thread.md)). Rule:
CLAUDE.md §2 item 4. Check: `scripts/check-no-kernel-sleep.sh`,
`scripts/check-no-kernel-sleep-by-ctxt.sh` (`hft`), `scripts/check-standard-gives-the-core-back.sh`
(`standard`) — each traces a prebuilt `tools/w2w`, so first run
`cargo build -p fixbolt-w2w --release --features tls,io-uring` as the CI jobs do, or the io_uring
arms print *SKIPPED, NOT PASSED* — and the test
`the_dial_loop_sleeps_rather_than_spins_while_the_handshake_waits`; `hft` under TLS is unchecked.

**Architecture Invariant:** no heap allocation on the session or dispatch path, on either
thread. Rule: CLAUDE.md §2 item 1. Check: `crates/engine/benches/alloc.rs` in the `bench` job, and
`tools/w2w`, which asserts zero allocations on both threads over its timed window.

**Architecture Invariant:** a feature gates the `mod` declaration itself — `standard`,
`affinity`, `tls` and `io-uring` compile to nothing when off — and no `build.rs` calls an external
toolchain unless its feature is on. Rule: CLAUDE.md §2 item 6 (D5). Check: the
`no-default-features` job and `scripts/check-no-optional-deps.sh`, per crate
([why per crate](docs/reference/feature-flags-unify-across-a-workspace.md)).

**Architecture Invariant:** the engine never logs on the hot path; the message log is one ring
push per message to a writer thread (D14). Rule: CLAUDE.md §6 *Rust*. Check: hand-check.

### `crates/library` — package `fixbolt`, the application-facing API (L4)

Entry points: `Handler`, `Incoming`, `Reply`, `App`/`app`, and a curated re-export of what an
application needs (`serve`, `Settings`, `Table`, `Limits`, `Handles`, `Recovery`, `FileJournal`,
…). The shortest working program is `crates/library/examples/acceptor.rs`
([ADR-0002](docs/decisions/ADR-0002-engine-library-split.md),
[ADR-0041](docs/decisions/ADR-0041-the-library-layer-buys-an-api-with-a-template-per-message.md)).

**Architecture Invariant:** the facade deliberately does **not** re-export `Engine`, `Dispatch`,
`Transport`, `wait`, `shard`, `affinity`, `frame` or `ring`; reaching for one means depending on
`fixbolt-engine` by name. Rule: DESIGN.md §3, ADR-0041. Check: hand-check — the example names
nothing from `fixbolt_engine` or `fixbolt_session`.

### Around the core

- `crates/metrics` — package `fixbolt-metrics`: a Prometheus exporter holding an `Observer` and
  nothing else (`Exporter`, `Builder`, `series`)
  ([ADR-0170](docs/decisions/ADR-0170-the-metrics-exporter-holds-an-observer-and-nothing-else-allocates-nothing-per-scrape-and-publishes-only-after-its-kill-line.md)).
- `crates/store-sqlite` — `SqliteJournal`: a `Journal` whose durable copy is a SQLite database,
  written by its own thread; nothing in `engine` or `library` depends on it
  ([ADR-0180](docs/decisions/ADR-0180-the-sqlite-store-is-the-async-journal-with-a-database-for-a-file-one-database-per-session-and-no-synchronous-mode.md)).
- `crates/conformance` — the `.def` runner (`runner`, `script`, `compare`, `echo`, `mirror`).
- `tools/` — `w2w` (wire-to-wire harness; the binary the mode checks trace), `jrnl`, `interop`,
  `attr-scan`; see [docs/internals/tools.md](docs/internals/tools.md).
- Outside the workspace: `fuzz/` (nightly), `spikes/`; `vendor/` is fetched, never committed.

## Cross-cutting concerns

**Architecture Invariant:** no crate depends on an async runtime; adding one needs an ADR. Rule:
CLAUDE.md §6 *Dependencies*. Check: hand-check —
`grep -E '^name = "(tokio|async-std|smol)"' Cargo.lock` prints nothing today.

**Architecture Invariant:** no `panic!`, `unwrap()` or `expect()` in a library crate, and no
panicking index. Rule: CLAUDE.md §2 item 7 (D6). Check: `scripts/check-lint-config.sh`,
`scripts/check-indexing-debt.sh` (a ratchet), `scripts/check-no-crate-root-allow.sh`.

**`unsafe`.** In library code, only in `engine`, in modules behind a feature: `poll` and `waker`
(`standard`), `affinity` (`affinity`,
[ADR-0019](docs/decisions/ADR-0019-two-unsafe-blocks-and-an-error-the-enum-can-hold.md)),
`transport::uring` (`io-uring`); `sbe` forbids it. Outside library code, the counting allocators in
`crates/*/benches/alloc.rs` and `tools/w2w` (`unsafe impl GlobalAlloc`), `engine/benches/wakeup.rs`
and some tests use it too.
**Architecture Invariant:** every `unsafe` block has a plan and a comment naming what proves it
sound. Rule: CLAUDE.md §2 item 8. Check: the workspace lint `unsafe_code = "warn"`, an error under
`clippy -D warnings` unless allowed at the site; the proof comment is a hand-check.

**Generated code.** `crates/dict/build.rs` and `sbe-gen` generate tables; never edit their
output by hand — change the generator or its input.

**Testing and measurements.** Which command runs for which change is CLAUDE.md §7; gate targets
are [DESIGN.md §6](docs/DESIGN.md#6-gates). A performance figure needs its benchmark, machine and
[§9](docs/DESIGN.md#9-deployment--the-os-is-part-of-the-design) settings (CLAUDE.md §2 item 10;
hand-check); the budget is [§8](docs/DESIGN.md#8-latency-budget-on-kernel-tcp), recorded figures
[measured-costs.md](docs/reference/measured-costs.md).

**Platforms.** `hft`, `affinity`, `shard`, kTLS and `io-uring` are Linux-only and do not compile
on macOS, so a local `cargo check` on a Mac does not see them — CI on Linux does.
