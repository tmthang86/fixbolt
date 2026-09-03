# fixbolt

A FIX 4.4 protocol engine in Rust, built to be **the fastest acceptor that can run on
kernel TCP** — the side of the protocol that the Rust ecosystem does not currently cover
with anything production-proven.

It speaks **both sides.** Acceptor and initiator share one session core, parameterised by
role ([ADR-0004](docs/decisions/ADR-0004-bidirectional-engine.md)), and both ship in phase 1
against the same gates. The acceptor stays the headline because that is where the gap is —
not because the engine only runs one direction.

It is not an HFT client and does not do kernel bypass. FIX tag=value over the kernel stack
has a floor of roughly **10–20 µs** wire-to-wire that no codec can move — `[measured
2026-09-02]` **16.0 µs for a round trip over loopback on an isolated core**, of which this
design's own user-space work is **2.9%**; this engine's job is
to make everything above that floor vanish, and to measure the floor honestly
([DESIGN.md §8](docs/DESIGN.md#8-latency-budget-on-kernel-tcp)).

**Two modes, and the default is the portable one**
([ADR-0013](docs/decisions/ADR-0013-two-modes-standard-and-hft.md)). **`standard`** blocks on
readiness, gives the core back and runs on any OS and any hardware — it is what you get if you
say nothing. **`hft`** is opt-in, Linux-only, pins its polling threads to isolated cores and
**burns a core per thread** to buy the microsecond. An engine whose out-of-the-box configuration
pins a core at 100% is one most people cannot evaluate, so `hft` is the claim and `standard` is
the front door. `[2026-08-30]` **`standard` is built and is the default** — `serve` blocks, `serve_hft` spins ([ADR-0014](docs/decisions/ADR-0014-standard-mode-blocks-on-poll.md)).

**Inside `hft`, latency beats session density, and that is a rule rather than a preference**
([ADR-0012](docs/decisions/ADR-0012-latency-first-and-one-session-per-polling-thread.md)). The
shape this engine is built for is **one session on one isolated polling thread**. `[measured
2026-08-31]` an idle turn is one non-blocking `read` per connection at **449 ns** on the line
`DESIGN.md` §9 now describes, flat from 1 to 16 sockets within 2%, so a sweep costs
`N × 449 ns` — **two sessions on a thread exceed the whole user-space budget in polling
alone**. (This line read 703 ns until 2026-08-31; that figure was a C program's bare `read` on
a `nohz_full` core, and §9 stopped asking for `nohz_full` —
[ADR-0021](docs/decisions/ADR-0021-nohz-full-leaves-section-9.md).) Many sessions per thread is supported, is named
`density`, and carries that term instead of these figures. **Every latency number here names
its session count.**

> **Status: it speaks FIX over a socket.** `[measured 2026-08-30]` the 59 QuickFIX acceptance
> definitions pass **59 / 59 through kernel TCP**, on an Apple M5 and on a Linux x86_64 box.
> It scored 39 / 59 on the second machine until 2026-08-30, and the cause turned out to be
> Nagle on the test harness's own client socket rather than anything in the engine — the story,
> including a confident wrong diagnosis and what it cost, is in
> [reference/measured-costs.md](docs/reference/measured-costs.md). `codec`, `dict`, `session`
> and `engine` exist, with dispatch, backpressure and the journal built, and **`tools/w2w`**
> now runs — which is what finally gave *the engine thread never sleeps in the kernel* a
> machine check (`scripts/check-no-kernel-sleep.sh`). `[2026-09-02]` **the application-facing
> `library` exists**: `crates/library`, package `fixbolt`, with a worked acceptor in
> `examples/` that the end-to-end test drives through a real socket. It is a **convenience
> layer and not the `hft` path** — `[measured 2026-09-02, on a machine that fails §9]` a reply
> through it costs **~956 ns** against 40 ns for a template built once — it was ~2.1 µs until
> [ADR-0044](docs/decisions/ADR-0044-a-builder-that-is-not-moved-per-field.md) stopped
> `TemplateBuilder` copying itself once per field — and
> [ADR-0041](docs/decisions/ADR-0041-the-library-layer-buys-an-api-with-a-template-per-message.md)
> is that number, why it is accepted, and what would remove it. The raw
> `fixbolt_session::Application` seam is untouched and stays the way to write a handler that
> cares. `[2026-09-02]` **the initiator is interop-green against a real `libquickfix`** — seven
> steps over a kernel socket, blocking in CI, which closes **phase 1 exit criterion 4**
> ([ADR-0042](docs/decisions/ADR-0042-a-second-implementation-is-the-only-independent-opinion.md)).
> Its first run found that the initiator answered a `Logon` with a `Logon`, a defect green in
> the 59 / 59 acceptance score and in 430 other tests. `[2026-09-02]` **and criterion 6 closed
> that evening, so PHASE 1'S SEVEN EXIT CRITERIA ARE ALL MET.** `tools/w2w` ran on a bare-metal
> Ryzen 3700X reading `pass 12  fail 0  unknown 1` against §9, engine pinned to an isolated
> core: `hft` **p50 16 010 / p99 20 589 / p99.9 22 127 ns** for an administrative round trip and
> **19 908 / 24 657 / 26 150** through an application, medians of 20 runs of 20 000 round trips,
> **zero allocations in the timed window**. `hft` is worth **17.7%** over `standard` end to end,
> and pinning to an isolated core is worth **nothing at p50 and 11× at p99.9**. **Loopback, not
> NIC to NIC** — `DESIGN.md` §6 keeps the stricter row open, and says so.
> **Where to read what, and in which order, is below.**
>
> **The name is `fixbolt`**, decided 2026-08-30 — it replaced the placeholder
> `nanofixengine`, which collided with `matthart1983/nanofix`. See STATUS.md.

## Where to start

**Nothing is published yet.** Every crate is `version = "0.0.0"` and `publish = false`, so there
is no `cargo add`. Clone it, and **run the bootstrap first**:

```sh
scripts/fetch-quickfix-assets.sh    # required — nothing builds without it
cargo test --all
```

`vendor/` is gitignored ([CLAUDE.md](CLAUDE.md) §8) and holds the FIX 4.4 XML that
`crates/dict/build.rs` generates its tables from, plus the 59 acceptance definitions that gate
every session-layer change. **Skip the script and the build fails**, because the dictionary has
nothing to generate from.

On a machine you intend to *measure* on, read `scripts/check-machine.sh` as well —
[DESIGN.md §9](docs/DESIGN.md#9-deployment--the-os-is-part-of-the-design) is part of the design,
and five of its rows do not survive a reboot.

**Then read in this order, depending on why you are here.**

| You are… | Read |
|---|---|
| **embedding the engine** | **[docs/GUIDE.md](docs/GUIDE.md)** — the constraints that show up as latency or lost messages rather than as compile errors, and the one document written for a user of this framework rather than a builder of it |
| **deciding whether to use it** | [docs/PRD.md](docs/PRD.md) — what it must do, in which phase, and how far it is from QuickFIX. Then [docs/reference/prior-art.md](docs/reference/prior-art.md) for the alternatives, **including a name collision worth reading before anything else** |
| **changing the code** | [docs/DESIGN.md](docs/DESIGN.md) — §4's D1–D10 are the decisions, §8 the latency budget, §9 the OS checklist. Then [CLAUDE.md](CLAUDE.md) §1 and §2: no code without an approved plan, and ten non-negotiables. [docs/decisions/](docs/decisions/) says why each was chosen and at what cost |
| **looking for where the work stands** | [STATUS.md](STATUS.md) — a pointer, not a store, and the open-items table is the backlog |
| **wondering what a measurement here means** | [docs/reference/measured-costs.md](docs/reference/measured-costs.md) — every figure this project quotes, with the machine and the command. No number here is published without its benchmark, its machine and its §9 settings |

**The shortest working code is `crates/library/examples/acceptor.rs`**, with its
`acceptor.cfg` beside it — a counterparty is a stanza in that file rather than a rebuild. Below
it, `crates/engine/tests/` shows the raw seam, and `crates/conformance/src/echo.rs` is a real
`Application` that rebuilds every reply through the dictionary rather than echoing bytes.

**What is still missing is the prose around them.** There is no getting-started page, no
tutorial, and no configuration reference — `engine::settings` accepts ten keys and two of them,
`EndDay` and `MaxSkewMillis`, are named in no document at all. That is
[STATUS.md](STATUS.md) open item 33, and it is tracked rather than unnoticed.

## Why this exists

As of 2026-08-27 there is no production-proven, pure-Rust FIX acceptor. `hotfix` and
`IronFix` are initiator-only. `ferrumfix` says of itself *"wildly unstable … refrain from
using it in production prior to its 1.0 release"*. The `quickfix` crate works, but is an
FFI binding to the C++ engine and inherits its throughput ceiling and its C++ toolchain.

The survey behind that paragraph, with sources, is in
[docs/reference/prior-art.md](docs/reference/prior-art.md) — **including a name collision
that should be read before anything else.**

## Relationship to QuickFIX

fixbolt is **not** a port of QuickFIX C++. A port is legally permitted but would import the
architecture responsible for its throughput ceiling. Instead fixbolt takes three things
from that project as *data*: the FIX XML dictionaries, the 59 FIX 4.4 session acceptance
tests, and `Session.cpp` as a reference for behaviour.

The reasoning, the licence analysis and the costs are in
[ADR-0001](docs/decisions/ADR-0001-relationship-to-quickfix.md).

The codec design follows [`hffix`](https://jamesdbrock.github.io/hffix/) instead: parse and
serialise in place at the I/O buffer, with no heap allocation on the hot path.

## Architecture, in one paragraph

Six layers, split so that the framework stays off the hot path. `codec` parses and
serialises in place at the I/O buffer with no allocation. `session` is the FIX session
protocol as a **pure state machine with no I/O** — which is what makes the 59 QuickFIX
acceptance definitions run as unit tests — and it takes `Role { Acceptor, Initiator }` as a
parameter rather than being written twice. `engine` opens and accepts the TCP connections and
drives those machines; `library` is where the application's `Handler` lives — called
**inline on the engine thread by default** (zero hops), or behind a ring buffer on its own
thread for applications that may block. The engine thread busy-polls and never sleeps in the
kernel; outbound messages are pre-encoded templates patched per send. TLS, when enabled, is a
second `transport` implementation: `rustls` for the handshake, then kTLS on Linux so the
kernel hands back plaintext and parse-in-place survives
([ADR-0005](docs/decisions/ADR-0005-tls.md)). The full reasoning, with the measurements behind
it, is in [docs/DESIGN.md](docs/DESIGN.md),
[ADR-0002](docs/decisions/ADR-0002-engine-library-split.md) and
[ADR-0003](docs/decisions/ADR-0003-message-representation.md).

## Layout

```
crates/
  codec/         parse and serialise in place, no allocation, no dependencies
  dict/          FIX 4.4 tables generated from the QuickFIX XML at build time
  conformance/   runs the 59 acceptance definitions in process, no socket
  session/       the FIX session state machine — pure, no I/O, role-parameterised
  engine/        TCP acceptor and connector; a thread that never sleeps in the kernel
  library/       package `fixbolt` — the application-facing API: one crate to depend
                 on, a Handler that receives a parsed message and answers through a
                 Reply that writes the seven fields an application does not own
                 (more crates are added one at a time, each behind an approved plan)
tools/
  w2w/           wire-to-wire harness — NIC to NIC, and the binary the mode checks trace
  jrnl/          reads a journal file from outside the process that wrote it
  interop/       drives this engine's initiator into a real libquickfix acceptor —
                 phase 1 exit criterion 4. Rust; the C++ counterparty is built by
                 scripts/interop.sh and by CI, never by cargo
benches/         baselines.tsv — one recorded timing baseline per (CPU model, case);
                 DESIGN.md §6 gates against this, not against an absolute target
fuzz/            cargo-fuzz targets — nightly, outside the workspace
spikes/ktls/     answers ADR-0005's kTLS question and stops — outside the workspace,
                 nothing depends on it, no code from it is merged
docs/
  GUIDE.md       how to embed the engine without losing latency or messages
  PRD.md         what must be built, in which phase, and the distance from QuickFIX
  DESIGN.md      how the system is built, and the latency budget it is built against
  decisions/     ADRs — expensive or hard-to-reverse decisions
  reference/     protocol facts, prior art, traps
  plans/         what is about to be built (Vietnamese; see CLAUDE.md §6)
vendor/          QuickFIX XML, acceptance definitions and generated C++ — fetched by
                 script, read as a test oracle, gitignored, never committed
```

## Licence

Dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option.
