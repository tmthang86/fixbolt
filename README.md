# fixbolt

A FIX 4.4 protocol engine written in Rust, as a library you embed in your own application. It is
built to be **a FIX acceptor on ordinary kernel TCP whose latency is a published, reproduced
number** — a kernel-bypass figure, where one exists, is only a second, labelled row beside a
kernel-TCP figure from the same boot
([ADR-0099](docs/decisions/ADR-0099-kernel-tcp-stays-the-default-and-the-headline-and-a-bypass-figure-is-a-second-labelled-row.md)).
It plays the initiator role too, over the same session state machine. It is not a port of
QuickFIX: it takes QuickFIX's dictionaries and acceptance tests as data
([ADR-0001](docs/decisions/ADR-0001-relationship-to-quickfix.md)).

**Documentation** is a book at <https://tmthang86.github.io/fixbolt/>, rendered from the Markdown
under [`docs/`](docs/) in this repository, which stays the authored copy.

## Who it is for

A Rust developer who has to **accept FIX 4.4 sessions** — counterparties that connect in, log on
and send orders — inside their own process, or dial out to one, and who wants to know what the
engine costs in latency instead of being told it is fast. Your code is a `Handler`: the engine
reads the socket, runs the session protocol, and calls your handler with a borrowed view of each
application message and a `Reply` to answer through.

It is a protocol engine, not a trading system: no order book, no matching, no risk checks, no
clustering or replication ([PRD.md §5](docs/PRD.md#5-permanent-non-goals)).

## What it is

- **Both roles.** Acceptor and initiator share one session state machine, chosen by a type
  parameter ([ADR-0004](docs/decisions/ADR-0004-bidirectional-engine.md)). Both ship in
  phase 1 and pass the same gates. The acceptor is the headline because that is where the
  gap in the ecosystem is.
- **Not kernel bypass, and not an HFT client.** FIX over the kernel TCP stack has a floor of
  roughly 10–20 µs per round trip that no codec can move. The engine's job is to make
  everything above that floor disappear and to measure the floor honestly. `[measured
  2026-09-02]` a round trip over loopback on an isolated core is **16.0 µs**, and this
  engine's own user-space work is **2.9%** of it
  ([DESIGN.md §8](docs/DESIGN.md#8-latency-budget-on-kernel-tcp)).
- **Two modes, and the default is the portable one**
  ([ADR-0013](docs/decisions/ADR-0013-two-modes-standard-and-hft.md)).
  `standard` blocks when idle, gives the core back, and runs on Unix: Linux, and macOS, where
  it is developed. `serve` does not exist on a target that is not Unix. It is what you get if you say
  nothing. `hft` is opt-in and Linux-only: it pins a polling thread to an isolated core and
  spins there, burning that core to save microseconds. `serve` is `standard`; `serve_hft` is
  `hft` ([ADR-0014](docs/decisions/ADR-0014-standard-mode-blocks-on-poll.md)).
- **In `hft`, one session per polling thread.** An idle turn of the engine costs
  `[measured 2026-08-31]` **449 ns per session** on a tuned Linux core, flat from 1 to 16
  sockets. Two sessions on one `hft` thread already cost more in polling than the whole
  user-space budget. Many sessions per thread is supported and called `density`, but it does
  not inherit the headline latency figures
  ([ADR-0012](docs/decisions/ADR-0012-latency-first-and-one-session-per-polling-thread.md)).
  Every latency number in this repository names its session count.

## What it does not do yet

Stated here because anyone comparing FIX engines will find them.
[PRD.md §3](docs/PRD.md#3-where-this-stands-against-quickfix) sets fixbolt against QuickFIX
capability by capability, and [GUIDE.md §9](docs/GUIDE.md#9-what-this-engine-does-not-do-for-you)
lists what an embedder still has to do themselves.

- **No production track record.** No deployment this repository did not write has been reported.
  No amount of testing stands in for counterparties having found an engine's bugs.
- **FIX 4.4 is the version the `fixbolt` crate serves.** A FIXT 1.1 / FIX 5.0 SP2 session exists
  in the lower crates behind their off-by-default `fix50sp2` feature
  ([CONFORMANCE.md §9](docs/CONFORMANCE.md#9-fixt-11--fix-50-sp2-added-2026-09-19)); `fixbolt`
  has no feature that turns it on.
- **The dictionary is compiled in.** Its tables are generated at build time from QuickFIX's FIX
  4.4 XML; changing it means rebuilding, and today it means replacing the whole file
  ([CONFIGURATION.md §5](docs/CONFIGURATION.md#5-build-time-environment-variables)). A venue's
  own fields as an overlay on FIX 4.4 is phase 5 work, not yet built
  ([PRD.md §2](docs/PRD.md#2-phases)).
- **Logon admits by identity only.** A counterparty presenting a configured comp-ID pair is
  admitted; there is no password, no `553` / `554` rejection and no IP allowlist.
- **No Windows.** `standard` needs a Unix target, `hft` needs Linux, and TLS is Linux-only,
  behind `fixbolt-engine`'s `tls` feature.
- **Session schedules are UTC only**, and a configuration file is not reloaded while the engine
  runs.
- **Not on crates.io, no docs.rs page, and pre-1.0.** It is released as a git tag; the install
  line and the conditions for `1.0` are under *Getting started* below.

## Start here

- **An acceptor running now:** [docs/GETTING-STARTED.md](docs/GETTING-STARTED.md), a
  configuration file and one Rust file.
- **The same acceptor built step by step, down to the bytes on the wire:**
  [docs/TUTORIAL.md](docs/TUTORIAL.md).
- **Why it is built this way, and what that costs:** [docs/explanation/](docs/explanation/index.md),
  three pages for a developer deciding whether to embed it.
- **New to FIX:** [docs/INTRODUCTION.md](docs/INTRODUCTION.md).
- **Embedding it in a real application:** [docs/GUIDE.md](docs/GUIDE.md), every constraint the
  compiler cannot check for you.
- **Everything, as one site:** [the book](https://tmthang86.github.io/fixbolt/). The full reading
  list is the table under *As a contributor* below; a codemap, ARCHITECTURE.md, is being written.

## Where it stands

**Phase 1 is complete.** All seven exit criteria in [PRD.md](docs/PRD.md) are met.

| Claim | Evidence |
|---|---|
| The 59 QuickFIX acceptance definitions pass | **59 / 59**, in process and through a kernel TCP socket, on an Apple M5 and on Linux x86_64 `[measured 2026-08-30]` |
| Both roles work against a real second implementation | **7 / 7 each way** against `libquickfix`, over kernel TCP, blocking in CI `[measured 2026-09-04]` — [CONFORMANCE.md §7](docs/CONFORMANCE.md) |
| Zero heap allocation on the hot path | `benches/alloc.rs` counts every hot path and asserts 0; `tools/w2w` counts both threads in its timed window and reads 0 |
| Wire-to-wire latency on a tuned Linux box | `hft` **p50 16 010 / p99 20 589 / p99.9 22 127 ns** administrative, **19 908 / 24 657 / 26 150 ns** through an application; medians of 20 runs of 20 000 round trips over loopback, AMD Ryzen 7 3700X `[measured 2026-09-02]` |
| The engine thread never sleeps in `hft`, and blocks in `standard` | `scripts/check-no-kernel-sleep.sh` and `scripts/check-standard-gives-the-core-back.sh`, both proven by reversal |

Two things those numbers do **not** say. They are loopback figures, so the NIC-to-NIC row in
[DESIGN.md §6](docs/DESIGN.md) is still open. And the application-facing `fixbolt` crate is a
convenience layer, not the `hft` path: `[measured 2026-09-05]` on the §9 desktop a reply
through it costs **804 ns** against **238 ns** for a template built once — about **3.4×**
([ADR-0051](docs/decisions/ADR-0051-item-34-is-a-third-of-the-size-it-was-recorded-at.md);
the *50×* this line carried came from a denominator that had no committed benchmark).

Where each piece of work stands, day by day, is in [STATUS.md](STATUS.md).

## Getting started

### As a user: install the crate

```sh
cargo add fixbolt --git https://github.com/tmthang86/fixbolt --tag v0.1.0
```

```toml
fixbolt = { git = "https://github.com/tmthang86/fixbolt", tag = "v0.1.0" }
```

**Not on crates.io** — by decision, not by omission: `0.1.0` is released as the git tag `v0.1.0`,
not a crates.io upload
([ADR-0161](docs/decisions/ADR-0161-0-1-0-is-a-git-tag-not-a-crates-io-upload-and-the-stranger-and-the-semver-gate-read-the-tag.md)).
This is the whole install unless a later ADR decides to publish; there is no docs.rs page either
— API documentation is `cargo doc --open` in your own checkout of the dependency.

`crates/dict` ships QuickFIX's FIX 4.4, FIXT 1.1 and FIX 5.0 SP2 dictionaries inside the crate
itself, at `crates/dict/spec/`, under [`NOTICE`](NOTICE) — so `fixbolt-dict`, and everything
built on top of it (including `fixbolt`), builds with **nothing but GitHub and crates.io**
reachable (the tree itself comes from GitHub at the tag; a handful of ordinary dependencies such
as `roxmltree` and `libc` still come from crates.io the normal way): no `vendor/` checkout, no
external toolchain (ADR-0104). Two steps to a running acceptor:
[`docs/GETTING-STARTED.md`](docs/GETTING-STARTED.md).

### Support level

fixbolt has a development team; today that team is one developer, its owner. Issues and pull
requests are read, but there is no SLA and no on-call, and contributions from outside the team
are not accepted yet ([docs/contributing.md](docs/contributing.md)). `docs/PRD.md` says what is built and what
is not; `CHANGELOG.md`'s *Conditions to reach 1.0* says what has to be true — a deployment this
repository did not write, reported publicly, and one minor release with no `semver-checks`
exemption — before this project calls itself `1.0`.

### As a contributor: build the workspace

Clone the repository and run the bootstrap script first:

```sh
scripts/fetch-quickfix-assets.sh    # required for the tests and the conformance oracle —
                                     # everything a user depends on builds without it
cargo test --all
```

The bootstrap script fetches the 59 acceptance definitions and QuickFIX's own generated C++
into `vendor/`, which is gitignored, and those are the oracle the rest of `cargo test --all`
checks the tables against — **without the script the full test suite fails**, even though the
crate that generates the tables does not need it.

Then read, depending on what you want:

| You want to… | Read |
|---|---|
| Understand FIX and the vocabulary | [docs/INTRODUCTION.md](docs/INTRODUCTION.md) |
| Run an acceptor in three steps | [docs/GETTING-STARTED.md](docs/GETTING-STARTED.md), then [docs/TUTORIAL.md](docs/TUTORIAL.md) |
| Look up a setting | [docs/CONFIGURATION.md](docs/CONFIGURATION.md) |
| Embed the engine without losing latency or messages | **[docs/GUIDE.md](docs/GUIDE.md)** — the constraints the compiler cannot check for you |
| Know what the session layer does before your code sees a message | [docs/SESSION-BEHAVIOUR.md](docs/SESSION-BEHAVIOUR.md) |
| See the measured results and what is not proven | [docs/CONFORMANCE.md](docs/CONFORMANCE.md) |
| Run it in production | [docs/best-practices-standard.md](docs/best-practices-standard.md) or [docs/best-practices-hft.md](docs/best-practices-hft.md), and the [HFT playbook](docs/hft-playbook.md) |
| Decide whether to use it at all | [docs/PRD.md](docs/PRD.md) for the gaps against QuickFIX, then [docs/reference/prior-art.md](docs/reference/prior-art.md) |
| Change the code | [docs/DESIGN.md](docs/DESIGN.md), then [CLAUDE.md](CLAUDE.md) §1 and §2 (plan first, ten non-negotiables), then [docs/decisions/](docs/decisions/) |
| Know what a number here means | [docs/reference/measured-costs.md](docs/reference/measured-costs.md) — every figure with its benchmark, machine and settings |

The shortest working code is `crates/library/examples/acceptor.rs` with `acceptor.cfg` next
to it. The end-to-end test in `crates/library/tests/end_to_end.rs` drives that same example
through a real socket.

If you intend to **measure** on a machine, read `scripts/check-machine.sh` as well.
[DESIGN.md §9](docs/DESIGN.md#9-deployment--the-os-is-part-of-the-design) lists the OS
settings a latency number depends on, and several of them do not survive a reboot.

## Why this exists

As of 2026-08-27 there is no production-proven, pure-Rust FIX acceptor. `hotfix` and `IronFix`
only do the initiator role. `ferrumfix` describes itself as *"wildly unstable"* and asks
users not to run it in production. The `quickfix` crate works, but it is a binding to the C++
engine and inherits its throughput ceiling and its C++ toolchain. The survey with sources is
in [docs/reference/prior-art.md](docs/reference/prior-art.md).

## Relationship to QuickFIX

fixbolt is **not** a port of QuickFIX. A port is legally allowed, but it would bring along the
architecture that limits QuickFIX's throughput. Instead, three things are taken from QuickFIX
as *data*: the FIX XML dictionaries, the 59 FIX 4.4 acceptance tests, and `Session.cpp` as a
reference for behaviour. The reasoning and the licence analysis are in
[ADR-0001](docs/decisions/ADR-0001-relationship-to-quickfix.md).

The FIX 4.4, FIXT 1.1 and FIX 5.0 SP2 XML dictionaries are shipped inside `fixbolt-dict`,
byte-identical to a pinned QuickFIX commit, under the QuickFIX Software License — see
[`NOTICE`](NOTICE). **Anyone distributing a binary built with fixbolt carries QuickFIX-derived
tables and owes that licence's attribution conditions**; `fixbolt_dict::NOTICE` (re-exported
as `fixbolt::NOTICE`) is meant to be printed wherever such an application lists its
third-party notices. The full reasoning is in
[ADR-0104](docs/decisions/ADR-0104-the-published-dictionary-is-quickfixs-xml-shipped-with-a-notice.md).

The codec follows [`hffix`](https://jamesdbrock.github.io/hffix/) instead: parse and
serialise in place in the I/O buffer, with no heap allocation on the hot path.

## Architecture in one paragraph

Six layers, split so that the framework stays off the hot path. `codec` parses and serialises
in place with no allocation. `session` is the FIX session protocol as a **pure state machine
with no I/O**, which is what lets the 59 acceptance definitions run as unit tests; its role
(acceptor or initiator) is a type parameter. `engine` opens and accepts TCP connections and
drives those state machines. `library` (package `fixbolt`) is where your `Handler` lives. By
default the handler runs **inline on the engine thread** with zero hops; an application that
may block can run behind a ring buffer on its own thread instead. Outbound messages are
pre-encoded templates patched per send. TLS is a second transport, behind `fixbolt-engine`'s `tls` feature on Linux: `rustls`
for the handshake, then kTLS on Linux so the kernel hands back plaintext and parse-in-place
still works ([ADR-0005](docs/decisions/ADR-0005-tls.md)). The full reasoning with the
measurements behind it is in [docs/DESIGN.md](docs/DESIGN.md),
[ADR-0002](docs/decisions/ADR-0002-engine-library-split.md) and
[ADR-0003](docs/decisions/ADR-0003-message-representation.md).

## Layout

```
crates/
  codec/         parse and serialise in place; no allocation, no dependencies
  sbe/           SBE 1.0 over generated tables; no_std, forbids unsafe, a codec
                 you bring your own transport to — no session (behind feature `sbe`)
  sbe-gen/       generates the tables sbe reads from an SBE 1.0 schema
  dict/          FIX 4.4 tables generated at build time from QuickFIX's XML, shipped
                 inside the crate under NOTICE (ADR-0104) — builds without vendor/
  conformance/   runs the 59 acceptance definitions in process, no socket
  session/       the FIX session state machine: pure, no I/O, role as a type parameter
  engine/        TCP acceptor and connector; the thread that drives the sessions
  library/       package `fixbolt`: the application-facing API. One crate to depend on,
                 a Handler that receives a parsed message and answers through a Reply
  metrics/       package `fixbolt-metrics`: a Prometheus exporter holding an Observer and
                 nothing else; allocates nothing per scrape. Not released yet (ADR-0170)
  store-sqlite/  a journal whose durable copy is a SQLite database, one per session; the
                 Async journal's engine-thread cost, a writer thread commits (feature
                 `sqlite`, on by default; off, the crate is empty and compiles no C)
tools/
  w2w/           wire-to-wire harness; the binary the two mode checks trace
  jrnl/          reads a journal file from outside the process that wrote it
  interop/       both roles against a real libquickfix over kernel TCP, and (--role dial,
                 --features tls) against QuickFIX/J. The C++ counterparty is built by
                 scripts/interop.sh and by CI, never by cargo
  attr-scan/     lexes a crate root with proc-macro2 and lists its inner attributes;
                 the eyes of scripts/check-no-crate-root-allow.sh. Nothing depends on it
  interop-qfj/   not a crate: Judge.java, this repository's own judge against a real
                 QuickFIX/J, both roles, plaintext and TLS. Built by scripts/interop-qfj.sh
                 and by CI; the six jars it needs are fetched, checked and never committed
benches/         baselines.tsv: one recorded timing baseline per (CPU model, case).
                 DESIGN.md §6 gates against this, not against an absolute target
fuzz/            cargo-fuzz targets; nightly, outside the workspace
spikes/ktls/     answers ADR-0005's kTLS question and stops; nothing depends on it
spikes/fixp-probe/
                 a Rust probe speaks Artio's Binary EntryPoint schema (B3, 5.6) against our own
                 referee (Referee.java, Artio's public API only); scripts/fixp-spike.sh's three
                 arms are CI job `fixp-spike`, BLOCKING (ADR-0140) — nothing in crates/ or tools/
                 depends on it, and no FIXP session is built here (ADR-0097 decision 5)
docs/            see the table above; decisions/ holds the ADRs, reference/ the
                 measured facts and traps, plans/ what is about to be built (Vietnamese),
                 internals/ a map of which file in which crate holds what
book.toml        the documentation site: an mdBook over docs/ in place (ADR-0206), its
                 table of contents docs/SUMMARY.md, built to target/book, never edited
vendor/          QuickFIX's acceptance definitions and generated C++ (the test oracle),
                 fetched by script, gitignored, never committed — crates/dict/spec/ holds
                 the three XML files that DO ship, byte-identical to vendor/'s own copy
```

`docs/internals/` is that map, one page per crate: which file holds what, in what order to
read it, and which test guards it — start at
[docs/internals/README.md](docs/internals/README.md).

## Licence

The code is dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your
option. `fixbolt-dict` also ships QuickFIX's FIX 4.4, FIXT 1.1 and FIX 5.0 SP2 XML dictionaries,
under the QuickFIX Software License; [`NOTICE`](NOTICE) holds its text, and *Relationship to
QuickFIX* above says what it asks of a binary you distribute.
