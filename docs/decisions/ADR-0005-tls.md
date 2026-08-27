# ADR-0005 — TLS is a transport implementation, and the hot-path guarantee is stated per mode

- **Status**: Accepted — 2026-08-27
- **Date**: 2026-08-27
- **Deciders**: Tran Manh Thang
- **Related**: [ADR-0001](ADR-0001-relationship-to-quickfix.md),
  [ADR-0003](ADR-0003-message-representation.md), [DESIGN.md D5, D8, §8](../DESIGN.md),
  [PRD.md §3](../PRD.md)
- **Carves out**: `CLAUDE.md` §2 non-negotiable 1 — *no heap allocation on the parse,
  serialise, session or dispatch hot path* — for the TLS handshake only. Bounded and named
  below rather than left to leak.

## Context

[PRD.md §3](../PRD.md) lists TLS as a phase-1 gap with **nothing specified**, next to the
observation that real venues require it. Which venues is not uniform: a colocated
cross-connect frequently carries FIX in the clear because the wire is physically private,
while anything internet-facing or cloud-hosted requires TLS and often mutual TLS. So this is
a deployment mode, not a universal requirement — but "not specified" is not a mode.

**TLS collides with three commitments this design has already made**, and the third is the
one that matters.

**1. Non-negotiable 1 — no allocation on the hot path.** A userspace TLS library holds
per-connection state and buffers. `rustls` allocates, most of it during the handshake, some
of it per record.

**2. D8 — the engine thread busy-polls and never sleeps in the kernel.** A TLS library that
owns the socket and offers a blocking `read` cannot be dropped into a spin loop. The
`Transport` trait's `recv`/`send` shape (D5) survives this only if the TLS layer is willing to
be driven, not to drive.

**3. Parse in place — and this is the structural cost.** The whole codec design, taken from
`hffix` via [ADR-0001](ADR-0001-relationship-to-quickfix.md), is *parse at the I/O buffer, copy
nothing*. Encrypted bytes cannot be parsed. Something must decrypt them into a plaintext
buffer first, which is exactly the copy the design spent [ADR-0003](ADR-0003-message-representation.md)
eliminating. **Userspace TLS reintroduces a copy per message on the read path, and a second on
the write path.** No amount of care removes it, because it is what TLS is.

There is one way out, and it is Linux-specific. **Kernel TLS (kTLS)** moves record framing and
crypto into the kernel after the handshake: the application sets the negotiated keys with
`setsockopt`, and then reads and writes the socket as if it were plaintext TCP. Crypto uses
AES-NI, and on capable NICs it offloads further. `[documented]` The Rust side of this exists —
`ktls-core` provides the low-level setup and `ktls-stream` a `TcpStream` drop-in; `ktls` now
sits under the rustls organisation. All of it is Linux-only.

`[unproven]` Those crates are documented largely in terms of `tokio-rustls`. This engine has no
async runtime and will not acquire one. Whether `ktls-core` can be driven from a plain
non-blocking socket in a spin loop is the single most important thing to verify before any of
this is built, and it is an open question below, not an assumption.

## Decision

**Accepted 2026-08-27 on the strength of the reasoning, not on measurement** — the same basis
STATUS.md records for ADR-0001, -0002 and -0003. Open question 1 below is load-bearing and
still unanswered: if `ktls-core` cannot be driven without an async runtime, this decision is
superseded rather than patched. It is tracked as STATUS open item 10.

**TLS is a `transport` implementation behind a feature flag, with two steady-state modes. The
hot-path guarantee is stated separately for each mode instead of being claimed for both.**

1. **The handshake runs in userspace, via `rustls`.** It happens once per session, before any
   application message flows, and it is off the hot path by construction. **Allocation is
   permitted here.** This is the carve-out from non-negotiable 1, and it is bounded to the
   handshake — not to the connection, and not to steady state.

2. **Steady state on Linux: kTLS.** After the handshake, the negotiated keys are handed to the
   kernel. `recv` and `send` stay ordinary non-blocking syscalls, so:
   - **D8 is preserved unchanged** — the engine thread still spins, still never sleeps.
   - **Parse-in-place is preserved** — the kernel delivers plaintext into the read buffer the
     codec already parses. No extra userspace copy.
   - Crypto gets AES-NI, and NIC offload where the hardware supports it.

   This is the mode that ships to production, and it is the only one that meets the hot-path
   guarantee.

3. **Steady state elsewhere: userspace `rustls`.** macOS, any kernel without kTLS, or a
   negotiated cipher suite kTLS does not carry. It costs one copy each way and it allocates.
   **This mode does not meet the hot-path guarantee, and the documentation says so in those
   words.** The feature name, the rustdoc and `DESIGN.md` §8 all carry the distinction. A
   number measured in this mode is never quoted as the engine's number.

4. **The feature flag gates the `mod` declaration itself** (D5, non-negotiable 6).
   `cargo build --no-default-features` produces a binary with no TLS code and no crypto
   dependency, on a machine with neither installed. CI proves it.

5. **`DESIGN.md` §8 gains a TLS row** with two entries — kTLS and userspace — marked as
   literature until `tools/w2w` measures the same load with TLS off, with kTLS, and with
   userspace rustls, on the same Linux box.

## Consequences

**Good**

- kTLS preserves the two properties TLS normally destroys in a design like this: the spin loop
  and parse-in-place. That is the entire reason to prefer it over the obvious choice.
- The carve-out is **bounded and named**. "Allocation is permitted during the handshake" is a
  sentence a reviewer can check; "TLS allocates a bit" is not.
- Deployments that do not need TLS pay nothing at all — the module does not exist in the
  binary, and neither does the crypto dependency.
- `rustls` is pure Rust, so no C toolchain enters the shipping path. This is the same line
  ADR-0001 drew when it rejected the FFI wrapper, held in a place where it would have been
  easy to give up.
- Stating the guarantee per mode is honest in a way that a single "TLS supported" line is not,
  and it makes the Linux requirement visible at decision time rather than at deployment time.

**Bad — and these are real**

- **Two steady-state paths, and the wrong one is the convenient one.** Every developer on a
  macOS laptop exercises the userspace path; the kTLS path only runs on Linux and in CI. That
  is the same failure shape as the two dispatchers in [ADR-0002](ADR-0002-engine-library-split.md),
  except here the tested path is not the shipped path. This is the weakest part of this
  decision.
- **kTLS is Linux-only and version-sensitive.** Kernel support for TLS 1.3 and for individual
  cipher suites has moved over time, and it is narrower than what `rustls` will happily
  negotiate. A session can negotiate its way out of the fast path without anyone noticing
  unless something asserts which mode is active.
- **The first substantial runtime dependency outside `codec`.** `rustls` brings a crypto
  provider with it. `codec` keeps its zero-dependency rule; `transport` does not, and that is
  a line being crossed for the first time.
- **Key update under kTLS is a real edge case.** TLS 1.3 rekeys; the kernel has to be told.
  Getting this wrong produces a session that dies after a duration rather than immediately,
  which is the expensive kind of bug.
- **The carve-out weakens the rule it carves out of.** Non-negotiable 1 goes from *always* to
  *always except the handshake*. The next carve-out will cite this one. The mitigation is that
  `benches/alloc.rs` must still assert zero across a **steady-state** message under kTLS.
- **No oracle, again.** No acceptance definition tests TLS. Correctness here is proven by
  interop against a real TLS peer, not by a suite — the same shape of gap as ADR-0004 and the
  repeating groups.
- **Mutual TLS is common at venues and is not answered here.** Client certificates, chain
  validation and rotation are a body of work this ADR names but does not scope.

## Alternatives considered

| Alternative | Why rejected |
|---|---|
| **Userspace `rustls` only**, no kTLS | One path, tested everywhere, far simpler — and it is kept as the fallback for exactly that reason. Rejected as the *only* mode because it puts a copy and an allocation on the hot path of every message, which is the specific thing this engine exists to avoid |
| **OpenSSL or BoringSSL via FFI** | Mature, and it has its own kTLS support. Rejected because it puts a C toolchain in the shipping path for every user — the same reasoning ADR-0001 used to reject wrapping QuickFIX, and it would be inconsistent to accept it here |
| **Terminate TLS in a sidecar** — stunnel, HAProxy | Zero engine code, and genuinely the right answer for some deployments. Rejected as *the* answer because it adds a process hop. `DESIGN.md` §8 puts the whole kernel-TCP floor at 10–20 µs; a loopback hop is the same order, so this trades a sub-microsecond crypto cost for a multi-microsecond routing cost |
| **No TLS; require a private wire** | Honest for colocation and it is what many production FIX links actually do. Rejected because it excludes every cloud and internet-facing deployment, which is most of the addressable use for an open-source engine |
| **Write the record layer by hand** | Full control of allocation, and no dependency. Rejected without hesitation: hand-rolled TLS is how CVEs are written, and `CLAUDE.md` §2 rule 8 would demand a proof this project cannot supply |

## Open questions

1. **Can `ktls-core` be driven from a plain non-blocking socket with no async runtime?** The
   documented usage is `tokio-rustls`-shaped. If the answer is no, this ADR's central claim
   collapses into "userspace rustls only" and the decision must be revisited. **Verify before
   building anything.**
2. **Which kernel version and which cipher suites are the floor?** kTLS support is narrower
   than `rustls` negotiation, so this becomes a documented deployment requirement in
   `DESIGN.md` §9 alongside `isolcpus` and the governor setting.
3. **What asserts which mode is actually active?** A session that silently negotiates into the
   userspace path has left the fast path, and nothing currently notices. A gate is needed, not
   a log line.
4. **Is mutual TLS in phase 1?** Many venues require client certificates. The answer changes
   the size of this work materially.
5. **Does the acceptor need SNI and multiple certificates**, or is one certificate per listener
   sufficient for the deployments in scope?
6. **How is key update handled under kTLS**, and what test proves a session survives one?

## Sources

- [ktls-core — docs.rs](https://docs.rs/ktls-core)
- [ktls-stream — docs.rs](https://docs.rs/ktls-stream/latest/ktls_stream/)
- [ktls — crates.io](https://crates.io/crates/ktls)
- [ktls now under the rustls org](https://fasterthanli.me/articles/ktls-now-under-rustls-org)
- [Kernel TLS offload — Linux kernel documentation](https://docs.kernel.org/networking/tls-offload.html)
