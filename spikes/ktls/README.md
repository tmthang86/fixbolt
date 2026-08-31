# `ktls` spike — can the kernel do TLS for a socket we spin on?

This answers one question and then stops. It is **not** a TLS transport, nothing
in the engine depends on it, and no code from it is merged — see
[the plan](../../docs/plans/2026-08-30-ktls-spike.md), which puts that out of
scope in writing.

- **The question:** [ADR-0005](../../docs/decisions/ADR-0005-tls.md) open question 1.
- **The answer:** [reference/ktls-on-a-plain-socket.md](../../docs/reference/ktls-on-a-plain-socket.md)
  and [ADR-0018](../../docs/decisions/ADR-0018-ktls-on-a-plain-socket-answers-adr-0005.md).

## Running it

```sh
scripts/check-ktls-on-a-plain-socket.sh     # from the repository root
```

That builds this crate, runs its 15 assertions, then traces it twice — once as
the engine's spin loop, once with `poll(2)` in front of the read, and **requires
the second run to trip the check**. It skips with exit 2, rather than passing, on
a kernel that cannot offload TLS.

To run the program alone: `cargo run --release` in this directory. Every line it
prints starting `PASS`, `FAIL` or `NOTE` is one observation; `SPIKE pass N fail N`
is the summary. `KTLS_SPIKE_WAIT=poll` selects the red arm.

## Why it is outside the workspace

An empty `[workspace]` table in `Cargo.toml` detaches it, and the root manifest
excludes it as well. `CLAUDE.md` §2 rule 6 requires the `--no-default-features`
job to build on a machine with nothing optional installed; cargo unifies features
across one invocation, and this repository has already been burned by exactly
that — see
[feature-flags-unify-across-a-workspace.md](../../docs/reference/feature-flags-unify-across-a-workspace.md).
So `rustls`, `ring` and `ktls-core` stay out of the engine's lockfile entirely.

**Its `Cargo.lock` is committed**, unlike `fuzz/`'s. The answer is about specific
versions — `ktls-core` 0.0.5, `rustls` 0.23.43, `ring` — and a re-run that
silently resolves different ones is answering a different question.

## What it does

| Phase | What it proves |
|---|---|
| 1 · full duplex | rustls handshake on a non-blocking socket with no runtime, keys handed to the kernel both ways, 1000 plaintext round trips, `EAGAIN` unchanged, and the kernel's `EIO` for control records recovered by `ktls_core::Context` |
| 2 · wire | the receiver never offloads and reads the socket raw: the bytes are a TLS record of exactly the size the payload implies, and the plaintext is absent |
| 3 · reversal | the sender does **not** hand its keys over — the offloaded receiver gets `EMSGSIZE` and never sees the plaintext |
| 4 · drain | draining the socket by hand before the handover desynchronises the kernel: `EBADMSG`, and `/proc/net/tls_stat` `TlsDecryptError` moves |

Phase 2 was a false green before it asserted the record's *size* rather than its
*shape* — it passed while reading a session-ticket record the test had not sent.
That is written up in the reference page, and it is why phases 2 and 3 turn
session tickets off while phase 1 leaves them on.
