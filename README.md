# fixbolt

A FIX 4.4 protocol engine written in Rust. It is built to be **the fastest FIX acceptor that
runs on ordinary kernel TCP**, which is the part of the protocol the Rust ecosystem does not
yet cover with anything production-proven.

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
  `standard` blocks when idle, gives the core back, and runs on any OS and any hardware. It
  is what you get if you say nothing. `hft` is opt-in and Linux-only: it pins a polling thread
  to an isolated core and spins there, burning that core to save microseconds. `serve` is
  `standard`; `serve_hft` is `hft`
  ([ADR-0014](docs/decisions/ADR-0014-standard-mode-blocks-on-poll.md)).
- **In `hft`, one session per polling thread.** An idle turn of the engine costs
  `[measured 2026-08-31]` **449 ns per session** on a tuned Linux core, flat from 1 to 16
  sockets. Two sessions on one `hft` thread already cost more in polling than the whole
  user-space budget. Many sessions per thread is supported and called `density`, but it does
  not inherit the headline latency figures
  ([ADR-0012](docs/decisions/ADR-0012-latency-first-and-one-session-per-polling-thread.md)).
  Every latency number in this repository names its session count.

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

**Nothing is published yet.** Every crate is `version = "0.0.0"` and `publish = false`, so
there is no `cargo add`. Clone the repository and run the bootstrap script first:

```sh
scripts/fetch-quickfix-assets.sh    # required — nothing builds without it
cargo test --all
```

The script fetches the FIX 4.4 XML dictionary and the 59 acceptance definitions into
`vendor/`, which is gitignored. `crates/dict/build.rs` generates its tables from that XML, so
**without the script the build fails**.

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
pre-encoded templates patched per send. TLS, when it lands, is a second transport: `rustls`
for the handshake, then kTLS on Linux so the kernel hands back plaintext and parse-in-place
still works ([ADR-0005](docs/decisions/ADR-0005-tls.md)). The full reasoning with the
measurements behind it is in [docs/DESIGN.md](docs/DESIGN.md),
[ADR-0002](docs/decisions/ADR-0002-engine-library-split.md) and
[ADR-0003](docs/decisions/ADR-0003-message-representation.md).

## Layout

```
crates/
  codec/         parse and serialise in place; no allocation, no dependencies
  dict/          FIX 4.4 tables generated from the QuickFIX XML at build time
  conformance/   runs the 59 acceptance definitions in process, no socket
  session/       the FIX session state machine: pure, no I/O, role as a type parameter
  engine/        TCP acceptor and connector; the thread that drives the sessions
  library/       package `fixbolt`: the application-facing API. One crate to depend on,
                 a Handler that receives a parsed message and answers through a Reply
tools/
  w2w/           wire-to-wire harness; the binary the two mode checks trace
  jrnl/          reads a journal file from outside the process that wrote it
  interop/       both roles against a real libquickfix over kernel TCP. The C++
                 counterparties are built by scripts/interop.sh and by CI, never by cargo
benches/         baselines.tsv: one recorded timing baseline per (CPU model, case).
                 DESIGN.md §6 gates against this, not against an absolute target
fuzz/            cargo-fuzz targets; nightly, outside the workspace
spikes/ktls/     answers ADR-0005's kTLS question and stops; nothing depends on it
docs/            see the table above; decisions/ holds the ADRs, reference/ the
                 measured facts and traps, plans/ what is about to be built (Vietnamese)
vendor/          QuickFIX XML and acceptance definitions, fetched by script, gitignored,
                 never committed
```

## Licence

Dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option.
