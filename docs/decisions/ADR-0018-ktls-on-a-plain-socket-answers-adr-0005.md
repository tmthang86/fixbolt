# ADR-0018 — kTLS drives from a plain socket, and ADR-0005 keeps its central claim

> **Status:** **Accepted — 2026-08-31** · **Supplements** [ADR-0005](ADR-0005-tls.md).
> It supersedes nothing: ADR-0005's decision stands, and this ADR records the
> measurement that decision was accepted without.
>
> **Numbering.** `ADR-0015` is not skipped by accident — it is reserved for the
> approved [threads-and-affinity](../plans/2026-08-30-threads-and-affinity.md)
> plan's step 1, which is not written yet. §5 forbids reusing a number; it does
> not require them to be claimed in order.
>
> **Accepted by standing delegation.** `[2026-08-30]` the owner delegated
> plan-writing and plan approval to the agent working in this repository. Nobody
> read the reasoning below on the owner's behalf.

- **Date**: 2026-08-31
- **Deciders**: Tran Manh Thang
- **Related**: [ADR-0005](ADR-0005-tls.md), [ADR-0013](ADR-0013-two-modes-standard-and-hft.md),
  [DESIGN.md §4 D8 and D11](../DESIGN.md),
  [reference/ktls-on-a-plain-socket.md](../reference/ktls-on-a-plain-socket.md),
  [plans/2026-08-30-ktls-spike.md](../plans/2026-08-30-ktls-spike.md)

## Context

[ADR-0005](ADR-0005-tls.md) was accepted **on reasoning, not measurement**, and
said so. Its open question 1 was load-bearing and it said that too:

> Can `ktls-core` be driven from a plain non-blocking socket with no async
> runtime? The documented usage is `tokio-rustls`-shaped. **Verify before
> building anything.**

If the answer were no, ADR-0005's steady-state mode 2 would collapse into mode 3
— userspace `rustls` for everybody — and with it the two properties the whole
codec design exists to protect: the D8 spin loop and parse-in-place.

`STATUS.md` open item 10 tracked it. `[measured 2026-08-31]` the spike in
`spikes/ktls` answered it on the owner's §9 desktop: **yes, with conditions.**
The full transcript, the syscall trace and each condition's own measurement are in
[reference/ktls-on-a-plain-socket.md](../reference/ktls-on-a-plain-socket.md).

Two facts carry the decision.

**`ktls-core` has no async dependency, and ADR-0005 was reading about a different
crate.** It cited `ktls-core`, `ktls-stream` and `ktls` together. `ktls` 6.0.2, in
the rustls organisation, genuinely is `tokio-rustls`-specific; `ktls-stream`
defaults to a `tokio` feature. `ktls-core` 0.0.5 depends on `bitfield-struct`,
`libc`, `nix` and `zeroize`, exposes no async feature at all, and every entry
point it has is synchronous and generic over `AsFd`.

**The offloaded data path makes no blocking call.** `strace -f` over 1000 round
trips, attributed by tid to the thread driving the socket: `recvfrom` 3033,
`sendto` 1000, and nothing else. The gate's red arm — the identical loop with
`poll(2)` in front of the read — shows `poll` 1000, which is what proves the green
arm could have failed.

## Decision

**1. ADR-0005 stands unchanged, and its decision 2 is now measured rather than
reasoned.** kTLS is the Linux steady-state mode; `recv` and `send` stay ordinary
non-blocking syscalls; D8 and parse-in-place both survive. Nothing about the
shape of that decision needs revising.

**2. Open question 1 is closed: yes, with conditions**, and the conditions bind
the TLS transport when it is built. They are requirements on that plan, not
suggestions:

- Every error from `read`/`write` on an offloaded socket goes to
  `ktls_core::Context::handle_io_error`. The kernel returns `EIO` for TLS control
  records it will not decode, and `[measured]` that happens once per connection
  with rustls's default session-ticket count. A transport that treats an error as
  fatal loses the session moments after the handshake.
- **The transport never reads the socket outside the offload.** `[measured]`
  draining post-handshake ciphertext by hand desynchronises the kernel's receive
  sequence number permanently: `EBADMSG` on the next record, and
  `/proc/net/tls_stat` `TlsDecryptError` moves. This is the trap most likely to be
  written by somebody trying to be helpful about the previous condition.
- The handshake driver hands over with **zero unprocessed bytes in its own
  buffer**, and asserts it rather than hoping.
- `setup_ulp` requires the socket to be `ESTABLISHED`.

**3. The dependency is `ktls-core`, not `ktls` and not `ktls-stream`.** The other
two bring an async runtime, and `CLAUDE.md` §6 makes that an ADR of its own. This
decision is the reason that ADR will not be needed.

**4. `Transport` does not change shape for this.** `Io::Idle` already means
*nothing moved, not an error, come back*, which is exactly what a recovered `EIO`
is. The TLS transport absorbs the control record inside `recv` and reports `Idle`.
**What the TLS plan must still verify** is the `standard`-mode interaction: a
socket with a control record pending is *readable*, so `poll` returns, `recv`
consumes the record and reports `Idle`, and the loop goes round again. That
terminates because `handle_io_error` consumes the record — but "terminates" is
reasoning, and it needs a test.

**5. Nothing from the spike is merged.** `spikes/ktls` is outside the workspace,
excluded in the root `Cargo.toml`, with its own lockfile. `cargo build` at the
root and CI's `--no-default-features` job never see `rustls` or `ring`. The spike
answers a question; the transport is a separate plan with its own ADRs.

**6. `DESIGN.md` §8's TLS row stays empty.** This spike deliberately published no
latency number, and one is not to be inferred from "the syscalls are the same
ones". That row is filled when `tools/w2w` runs the same load three ways — TLS
off, kTLS, userspace `rustls` — on one machine, which is ADR-0005 decision 5 and
is untouched.

## Consequences

**Good**

- The most expensive thing that could have been wrong about ADR-0005 is now
  known not to be, and it cost one afternoon rather than a TLS transport built on
  a false premise.
- **ADR-0005's open question 3 got cheaper without being answered.**
  `/proc/net/tls_stat` gives a kernel-side count of offloaded sockets and of
  decrypt errors — an observation from outside the process, which is what a
  "which mode is actually live" gate needs. The gate still does not exist.
- The five conditions are the kind of thing that is normally discovered during
  implementation, at the price of a day each. They are written down before the
  implementation starts.
- The spike is a committed, runnable gate with a red arm, not a transcript in a
  document. `scripts/check-ktls-on-a-plain-socket.sh` re-answers the question on
  any machine, and skips with exit 2 rather than reporting a green it did not earn.

**Bad, and named**

- **One kernel, one cipher suite, one library version.** 7.0.0-30-generic,
  `TLS13_AES_128_GCM_SHA256`, `ktls-core` 0.0.5, `rustls` 0.23.43. The pinning is
  what makes the result attributable and it is also the limit of what it covers.
  ADR-0005 open question 2 — which kernel and which suites are the floor — is
  **not** answered here, and this ADR must not be cited as though it were.
- **`ktls-core` is a 0.0.x crate from one author**, outside the rustls
  organisation, at version 0.0.5. That is a supply-chain fact ADR-0005 did not
  weigh, and it is a real cost: the API may move under us, and the alternative in
  the rustls org is the one that requires tokio.
- **Rekey is untested and TLS 1.3 rekeys.** ADR-0005 already named this as "the
  expensive kind of bug" — a session that dies after a duration rather than
  immediately. `ktls-core` has `refresh_traffic_keys` and a `tls13-key-update`
  feature; this spike ran neither, and the kernel's own note is that setting kTLS
  parameters more than once wants roughly 6.12 or later.
- **The spike proves a spin loop can drive kTLS. It does not prove this engine's
  spin loop can**, because the engine's loop is not what ran. The gap is small and
  it is a gap.
- **A second lockfile and a second dependency tree** now live in the repository,
  built by nothing that CI runs. It will rot quietly, and the mitigation is only
  that the script rebuilds it on demand.

## Alternatives considered

**Supersede ADR-0005.** Warranted only if the answer had been no. It was yes.

**Fold the conditions into ADR-0005 by editing it.** Refused by `CLAUDE.md` §5:
an accepted ADR's substance is never edited. A supplement is the shape the rules
provide, and it also keeps the honest record that ADR-0005 was accepted without
this evidence.

**Write the TLS transport now, while the material is fresh.** Rejected — the
spike's own plan puts it out of scope, and `CLAUDE.md` §1 wants a plan first.
Everything learned here is written down, which is the point of writing it down.

**Use the `ktls` crate from the rustls organisation** and accept an async runtime.
Rejected: it would need its own ADR under §6, it would put `tokio` in the shipping
path of an engine that has spent four ADRs staying out of one, and the measurement
above says it is not necessary.
