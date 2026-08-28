# nanofixengine

A FIX 4.4 protocol engine in Rust, built to be **the fastest acceptor that can run on
kernel TCP** — the side of the protocol that the Rust ecosystem does not currently cover
with anything production-proven.

It speaks **both sides.** Acceptor and initiator share one session core, parameterised by
role ([ADR-0004](docs/decisions/ADR-0004-bidirectional-engine.md)), and both ship in phase 1
against the same gates. The acceptor stays the headline because that is where the gap is —
not because the engine only runs one direction.

It is not an HFT client and does not do kernel bypass. FIX tag=value over the kernel stack
has a floor of roughly **10–20 µs** wire-to-wire that no codec can move; this engine's job is
to make everything above that floor vanish, and to measure the floor honestly
([DESIGN.md §8](docs/DESIGN.md#8-latency-budget-on-kernel-tcp)).

> **Status: design. No engine code exists yet.** The repository holds the decisions that
> come before it. **[docs/PRD.md](docs/PRD.md)** says what must be built and how far it is
> from QuickFIX; **[docs/DESIGN.md](docs/DESIGN.md)** says how it is built;
> [STATUS.md](STATUS.md) says where the work stands.
>
> **`nanofixengine` is a placeholder name**, taken to clear a collision. See STATUS.md.

## Why this exists

As of 2026-08-27 there is no production-proven, pure-Rust FIX acceptor. `hotfix` and
`IronFix` are initiator-only. `ferrumfix` says of itself *"wildly unstable … refrain from
using it in production prior to its 1.0 release"*. The `quickfix` crate works, but is an
FFI binding to the C++ engine and inherits its throughput ceiling and its C++ toolchain.

The survey behind that paragraph, with sources, is in
[docs/reference/prior-art.md](docs/reference/prior-art.md) — **including a name collision
that should be read before anything else.**

## Relationship to QuickFIX

nanofixengine is **not** a port of QuickFIX C++. A port is legally permitted but would import the
architecture responsible for its throughput ceiling. Instead nanofixengine takes three things
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
drives those machines; `library` is where the application's `SessionHandler` lives — called
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
                 (more crates are added one at a time, each behind an approved plan)
fuzz/            cargo-fuzz targets — nightly, outside the workspace
docs/
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
