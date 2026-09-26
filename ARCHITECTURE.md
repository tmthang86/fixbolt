# Architecture

This file is for a developer who is about to change fixbolt's code and needs to know **where** a
piece of behaviour lives and **what must not break**. It is a map, not the design: *what* was
decided is [docs/DESIGN.md](docs/DESIGN.md) §4 (D1–D16), *why* and at what cost is the ADR each
section names in [docs/decisions/](docs/decisions/), and which file of a crate holds what is
[docs/internals/](docs/internals/README.md). The rules themselves live once, in
[CLAUDE.md](CLAUDE.md); this file links to them and does not restate them.

It changes only when a crate boundary or an invariant changes (CLAUDE.md §4). Names below are
meant to be searched for — a symbol search in your editor, or `rg 'fn received'` — and there are
deliberately no line numbers, because they rot.

## Bird's-eye view

fixbolt is a FIX 4.4 engine: bytes arrive on a TCP connection, a FIX session decides what they
mean, an application sees the messages that are its business, and replies go back out. Both
roles — acceptor and initiator — run on one session core, chosen by a type parameter
([ADR-0004](docs/decisions/ADR-0004-bidirectional-engine.md)).

One inbound message, on the engine thread, in order:

1. The **transport** reads bytes into a fixed per-connection buffer (`Transport`,
   `TcpTransport`).
2. The **framer** finds where one message ends by reading `9=` and nothing else (`Framer`).
3. The **session** parses the frame in place into a borrowed view, checks it against the
   generated dictionary tables, runs the FIX session protocol (sequence numbers, heartbeats,
   resends, logout), and hands an application message on (`Session::received`).
4. The **dispatcher** gives it to the application — on the same thread by default
   (`InlineDispatch`), or through a ring to the application's own thread (`RingDispatch`).
5. A reply is a pre-sorted **template**, patched and written back through the transport
   (`Template`); what was sent is kept in the **journal** for a resend (`Journal`).

Time is read once per turn by the engine and passed into the session as milliseconds
(`Session::tick`); the session never reads a clock. The engine thread's idle behaviour depends
on the **mode**: `standard` (the default) blocks and gives the core back; `hft` (opt-in, Linux)
spins on a pinned core ([ADR-0013](docs/decisions/ADR-0013-two-modes-standard-and-hft.md)).

The layering is in [DESIGN.md §2](docs/DESIGN.md#2-layers); the guiding finding — keep the
framework off the hot path — is [§1](docs/DESIGN.md#1-the-finding-this-architecture-is-built-around).

## Codemap

Crates in dependency order. Each entry: what it is, where to start reading, names to search for.
The authoritative crate table is [DESIGN.md §3](docs/DESIGN.md#3-crates); the order they were
built in is [§7](docs/DESIGN.md#7-build-order).

### `crates/codec` — parse and serialise in place (L1)

Entry points: `parse_into`, `MessageView`, `FieldIndex`, `Template`, `TemplateBuilder`,
`TimestampCache`, `GroupIter`, the `Encoding` trait and `TagValue`, the `Dictionary` trait.
A parsed message is a `MessageView` — a borrowed view into the caller's buffer, indexed by a
`FieldIndex<N>` whose size the caller picks (D2,
[ADR-0003](docs/decisions/ADR-0003-message-representation.md)). Outbound messages are
`Template`s patched per send, with the timestamp patched from `TimestampCache` (D9).

**Architecture Invariant:** `codec` has **no runtime dependencies** — its `Cargo.toml` has
`[dev-dependencies]` only. Rule: CLAUDE.md §6 *Dependencies*. Check: hand-check on any change to
`crates/codec/Cargo.toml`.

**Architecture Invariant:** nothing on the parse or serialise path allocates. Rule: CLAUDE.md §2
item 1. Check: `crates/codec/benches/alloc.rs`, run by `scripts/bench.sh` in the `bench` CI job.

### `crates/dict` — the FIX tables, generated at build time

Entry points: `build.rs`, the `Tables` trait, `Fix44`, `FieldType`; behind the `fix50sp2`
feature, `Fixt11Fix50Sp2Tables`. `build.rs` reads the QuickFIX XML shipped in `crates/dict/spec/`
and emits tag constants, required-field and ordering tables, group layouts and validation tables.

**Architecture Invariant:** field order comes from these generated tables, never from a call
site. Rule: CLAUDE.md §2 item 5 (D3). Check: hand-check; the acceptance comparator in
`crates/conformance` is positional, so a misordering in a message the corpus exercises goes red
there.

**Architecture Invariant:** there is **no dictionary chosen or loaded at run time**. A dictionary
is a type compiled into the build; a session's encoding carries its table, and the registry
chooses among compiled encodings, never among dictionaries
([ADR-0080](docs/decisions/ADR-0080-the-dictionary-rides-the-encoding-and-a-fixt-session-is-one-table-built-from-two-xml-files.md)
decision 1). Rule: D3, under CLAUDE.md §2 item 5. Check: hand-check.

**Architecture Invariant:** exactly three QuickFIX files ship — `FIX44.xml`, `FIXT11.xml`,
`FIX50SP2.xml` under `crates/dict/spec/`, byte-identical to a pinned commit, under `NOTICE`; no
QuickFIX source is copied anywhere
([ADR-0001](docs/decisions/ADR-0001-relationship-to-quickfix.md),
[ADR-0104](docs/decisions/ADR-0104-the-published-dictionary-is-quickfixs-xml-shipped-with-a-notice.md)).
Rule: CLAUDE.md §2 item 9. Check: `scripts/check-dict-spec-pin.sh`; it cannot see a QuickFIX file
committed under another name, so `git add` stays the control.

### `crates/sbe` and `crates/sbe-gen` — the second encoding

`sbe`: SBE 1.0 decode and encode over `&'static` tables (`SbeView`, `Sbe`, `Schema`,
`MessageWriter`), `#![no_std]` and `#![forbid(unsafe_code)]`. `sbe-gen`: `generate` turns an SBE
schema into those tables, called from a `build.rs`. Both sit under the `Encoding` trait (D16,
[ADR-0081](docs/decisions/ADR-0081-sbe-tables-come-from-this-repositorys-generator-and-the-oracle-is-the-specs-bytes-plus-sbe-tool-behind-a-script.md)).

### `crates/session` — the FIX session protocol as a pure state machine (L2)

Entry points: `Session<E, R>` and its `received`, `tick`, `connect`, `disconnect`; the
`Application` trait (`on_message`, `on_logon`); `Role` with `Acceptor` and `Initiator`; `Config`,
`DictionaryChecks`, `ResetPolicy`; `DropReason`; the `Journal` trait; modules `schedule` and
`clock`. Output is written through an `emit` closure the caller passes in; the state change is
returned as a `Link`.

**Architecture Invariant:** the session layer has **no socket, no clock, no allocation and no
`format!`**. Its only dependencies are `codec` and `dict`; time arrives as an argument in
milliseconds from year zero (D13); errors are fieldless enums. This is what lets the acceptance
definitions run as plain unit tests. Rule: CLAUDE.md §2 item 2 (D1). Check: the allocation half is
`crates/session/benches/alloc.rs`; the rest is a hand-check.

**Architecture Invariant:** a session change is not done until the 59 QuickFIX acceptance
definitions pass, 59 / 59. Rule: CLAUDE.md §2 item 3. Check: `crates/conformance`, in process
(`cargo test -p fixbolt-session --test score`) and over a socket
(`cargo test -p fixbolt-engine --test wire`); the FIXT corpus behind `fix50sp2` in the `gates`
job; `scripts/check-socket-corpus-under-contention.sh` for the socket harness under load.
Commands and scores: [docs/CONFORMANCE.md](docs/CONFORMANCE.md).

### `crates/engine` — sockets, threads, and the loop that drives sessions (L3)

The largest crate; [DESIGN.md §3 *What `engine` contains*](docs/DESIGN.md#what-engine-contains)
lists every module with its decision, and [docs/internals/engine.md](docs/internals/engine.md) the
reading order. Start at `Engine` and `Engine::turn` — one pass of the loop — then the doors that
build one: `serve` (`standard`), `serve_hft` (`hft`), `connect_and_serve` (initiator), and their
`_with`, `_with_recovery` and `serve_tls*` variants. Then, by concern:

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
in `standard` it **must** block when idle. Each half is a rule, and a claim that does not name its
mode is incomplete (D8, [ADR-0012](docs/decisions/ADR-0012-latency-first-and-one-session-per-polling-thread.md),
[ADR-0013](docs/decisions/ADR-0013-two-modes-standard-and-hft.md)). Rule: CLAUDE.md §2 item 4.
Check: `scripts/check-no-kernel-sleep.sh`, `scripts/check-no-kernel-sleep-by-ctxt.sh` (`hft`),
`scripts/check-standard-gives-the-core-back.sh` (`standard`), and the test
`the_dial_loop_sleeps_rather_than_spins_while_the_handshake_waits`; `hft` under TLS is unchecked.

**Architecture Invariant:** no heap allocation on the session or dispatch path, on either
thread. Rule: CLAUDE.md §2 item 1 (D2, D9). Check: `crates/engine/benches/alloc.rs` in the
`bench` job, and `tools/w2w`, which counts allocations on both threads over its timed window and
asserts zero.

**Architecture Invariant:** a feature gates the `mod` declaration itself, not only the manifest
entry — `standard`, `affinity`, `tls` and `io-uring` each compile to nothing when off, and no
`build.rs` calls an external toolchain unless its feature is on. Rule: CLAUDE.md §2 item 6 (D5).
Check: the `no-default-features` CI job and `scripts/check-no-optional-deps.sh`, per crate,
because cargo unifies features across one invocation
([reference](docs/reference/feature-flags-unify-across-a-workspace.md)).

**Architecture Invariant:** the engine never logs on the hot path, and no logging framework is a
dependency of any crate. The message log is not a log call: it is one ring push per message to a
writer thread (D14). Rule: CLAUDE.md §6 *Rust*. Check: hand-check.

### `crates/library` — package `fixbolt`, the application-facing API (L4)

Entry points: `Handler`, `Incoming`, `Reply`, `App`/`app`, and a curated re-export of what an
application needs (`serve`, `Settings`, `Table`, `Limits`, `Handles`, `Recovery`, `FileJournal`,
…). `App` adapts a `Handler` to `session::Application`. The shortest working program is
`crates/library/examples/acceptor.rs`
([ADR-0002](docs/decisions/ADR-0002-engine-library-split.md),
[ADR-0041](docs/decisions/ADR-0041-the-library-layer-buys-an-api-with-a-template-per-message.md)).

**Architecture Invariant:** the facade deliberately does **not** re-export `Engine`, `Dispatch`,
`Transport`, `wait`, `shard`, `affinity`, `frame` or `ring`. Reaching for one means depending on
`fixbolt-engine` by name. The example names nothing from `fixbolt_engine` or `fixbolt_session`,
which is the facade's own test. Rule: DESIGN.md §3 and ADR-0041. Check: hand-check.

### Around the core

- `crates/metrics` — package `fixbolt-metrics`: a Prometheus exporter holding an `Observer` and
  nothing else (`Exporter`, `Builder`, `series`)
  ([ADR-0170](docs/decisions/ADR-0170-the-metrics-exporter-holds-an-observer-and-nothing-else-allocates-nothing-per-scrape-and-publishes-only-after-its-kill-line.md)).
- `crates/store-sqlite` — `SqliteJournal`: a `Journal` whose durable copy is a SQLite database,
  written by its own thread; nothing in `engine` or `library` depends on it
  ([ADR-0180](docs/decisions/ADR-0180-the-sqlite-store-is-the-async-journal-with-a-database-for-a-file-one-database-per-session-and-no-synchronous-mode.md)).
- `crates/conformance` — the `.def` runner (`runner`, `script`, `compare`, `echo`, `mirror`).
  Built before `session`, so the gate existed before the thing it gates.
- `tools/` — `w2w` (the wire-to-wire harness, and the binary the mode checks trace), `jrnl`
  (reads a journal file), `interop` (against a real `libquickfix` and QuickFIX/J), `attr-scan`
  (for `scripts/check-no-crate-root-allow.sh`); `tools/interop-qfj` is Java, not a crate.
  See [docs/internals/tools.md](docs/internals/tools.md).
- Outside the workspace: `fuzz/` (nightly), `spikes/` (answer one question, ship nothing).
  `vendor/` is fetched by script, gitignored and never committed.

## Cross-cutting concerns

**No async runtime.** The engine is a thread that polls or blocks; there are no futures.
**Architecture Invariant:** no crate depends on an async runtime, and adding one needs an ADR.
Rule: CLAUDE.md §6 *Dependencies*. Check: hand-check —
`grep -E '^name = "(tokio|async-std|smol)"' Cargo.lock` prints nothing today.

**Errors and panics.** Errors are typed — fieldless enums on a hot path, `thiserror` elsewhere,
never `Box<dyn Error>` in a public API (CLAUDE.md §6). **Architecture Invariant:** no `panic!`,
`unwrap()` or `expect()` in a library crate, and no panicking index. Rule: CLAUDE.md §2 item 7
(D6). Check: workspace clippy lints (`scripts/check-lint-config.sh`, proven by reversal),
`scripts/check-indexing-debt.sh` (a ratchet), `scripts/check-no-crate-root-allow.sh`.

**`unsafe`.** Only in `engine`, and only in modules behind a feature: `poll` and `waker`
(`standard`), `affinity`
([ADR-0019](docs/decisions/ADR-0019-two-unsafe-blocks-and-an-error-the-enum-can-hold.md)),
`transport::uring` (`io-uring`). `sbe` forbids it outright.
**Architecture Invariant:** every `unsafe` block has a plan and a comment naming what proves it
sound — a Miri run, a fuzz target, a test. Rule: CLAUDE.md §2 item 8. Check: the workspace lint
`unsafe_code = "warn"`, which `clippy -D warnings` turns into an error unless allowed at the site;
the proof comment is a hand-check.

**Generated code.** Two generators: `crates/dict/build.rs` (FIX XML → tables) and `sbe-gen`
(SBE schema → tables). Generated output is never edited by hand; change the generator or its
input.

**Testing.** The acceptance definitions are the session's gate; `benches/alloc.rs` in each crate
is the allocation gate; the mode scripts are the idle-behaviour gate; a guard is proven by
reversal. Which command runs for which change is CLAUDE.md §7; the gate targets are
[DESIGN.md §6](docs/DESIGN.md#6-gates).

**Measurements.** A latency or throughput figure is quoted only with the committed benchmark
that produced it, the machine, and the [DESIGN.md §9](docs/DESIGN.md#9-deployment--the-os-is-part-of-the-design)
settings in force (CLAUDE.md §2 item 10; hand-check). The latency budget is
[DESIGN.md §8](docs/DESIGN.md#8-latency-budget-on-kernel-tcp); recorded figures are in
[docs/reference/measured-costs.md](docs/reference/measured-costs.md).

**Platforms.** `standard` runs on any Unix; `hft`, `affinity`, `shard`, kTLS and `io-uring` are
Linux. Some of that code does not compile on macOS at all, so a local `cargo check` on a Mac does
not see it — CI on Linux does.
