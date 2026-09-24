# ADR-0190 — The `io_uring` transport is reaped by the idle strategy, and an `hft` turn enters the kernel once without waiting

- **Status**: Proposed — 2026-09-24; **revised in place 2026-09-24** (Revision 1, R1–R4, at the
  end of this ADR — decisions 4, 5 and 7 changed while it was still Proposed, after steps 1–4 were
  built). Written by the architect (Opus) for phase 4 row 5
  ([plan](../plans/2026-09-24-p4-io-uring-transport.md)). Accepted with that plan, by the owner or
  by the manager under the owner's standing mandate.
- **Date**: 2026-09-24
- **Deciders**: proposed by the architect; accepted by the owner or the manager.
- **Related**: [ADR-0098](ADR-0098-phase-4-is-the-owners-five-items-each-entering-behind-a-measurement-that-can-kill-it.md)
  decision 1 (the item, its kill line) and the owner's Q8 (SQPOLL in `hft` is an arm, never a
  default); [ADR-0074](ADR-0074-kernel-bypass-io-uring-and-the-logon-hop-stay-unmeasured-by-decision.md)
  decision 2 (its trigger has fired); [ADR-0013](ADR-0013-two-modes-standard-and-hft.md),
  [ADR-0014](ADR-0014-standard-mode-blocks-on-poll.md) (the `Waiting` / `Transport` split and the
  compile-time pairing refusal this copies); [ADR-0060](ADR-0060-a-deployment-that-requires-the-kernel-is-refused-twice.md)
  (refuse at startup, never fall back); [ADR-0068](ADR-0068-a-published-figure-is-two-procedures-shown-side-by-side.md),
  [ADR-0090](ADR-0090-a-measurement-boot-is-pre-built-shares-one-control-arm-and-a-five-step-drift-is-closed-one-named-segment-at-a-time.md)
  (how row 7 measures); [ADR-0191](ADR-0191-the-hft-sleeper-list-reads-io-uring-enter-by-its-min-complete.md)
  (the gate change this transport needs); `DESIGN.md` D5, D8, §6, §9.

## Context

ADR-0098 decision 1 admits `io_uring` as a second `Transport` on kernel TCP, behind a feature, with
a kill line: the `hft` NIC wire p50 ≥ 3 % better in both procedures, **or** the idle loop at
N = 16 ≥ 25 % better with the single-session wire p50 no worse than the band. It fixed the shape in
one sentence — *"`hft` submits and peeks the CQ, never `io_uring_enter` with a wait; `standard`
waits in `io_uring_enter(min_complete = 1)`"* — and left the mechanism to the row plan. Reading the
engine and the kernel's documentation turned that sentence into four questions the ADR has to
answer before a line is written:

1. **Where are completions reaped?** The engine is generic over one `T: Transport` per connection
   and one `W: Waiting` per engine (`crates/engine/src/transport.rs:168`, `wait.rs:34`).
   `Engine::turn` calls `recv` once per connection; `Engine::run` calls `idle` only when a turn
   moved nothing (`lib.rs:1721`). A ring is one per engine thread, not one per connection.
2. **Does "peek the CQ" alone ever see a network completion?** A socket `recv` on `io_uring`
   completes through *task work* that runs in the submitting task's context. By default the kernel
   interrupts a task running in user space to run it (`io_uring_setup(2)`,
   `IORING_SETUP_COOP_TASKRUN`: *"By default, io_uring will interrupt a task running in userspace
   when a completion event comes in"*). With `IORING_SETUP_DEFER_TASKRUN` the work is deferred
   *"until an io_uring_enter(2) call with the IORING_ENTER_GETEVENTS flag set"*. So a loop that only
   reads the CQ ring from user space either takes an interrupt per completion on the engine core,
   or never sees a completion at all.
3. **What does the `hft` gate say?** `scripts/check-no-kernel-sleep.sh:116` lists
   `io_uring_enter` among its sleepers, with no regard to its arguments. Any `hft` design that is
   not SQPOLL enters the kernel through `io_uring_enter`.
4. **What does "blocked" look like?** Docker ≥ 25.0 removes the three `io_uring_*` syscalls from
   its default seccomp allowlist, and that profile's default action is `SCMP_ACT_ERRNO` with
   `defaultErrnoRet: 1` — **`EPERM`**. `kernel.io_uring_disabled` (Linux 6.6+) makes
   `io_uring_setup` fail with **`EPERM`** too. A kernel built without `CONFIG_IO_URING` answers
   `ENOSYS`; a kernel too old for a flag answers `EINVAL`.

## Research

| Source | What it says | Bearing |
|---|---|---|
| [`io_uring_setup(2)`](https://man7.org/linux/man-pages/man2/io_uring_setup.2.html) | `COOP_TASKRUN` (5.19): by default user space is interrupted per completion. `DEFER_TASKRUN` (6.1): work deferred until `io_uring_enter` with `GETEVENTS`; requires `SINGLE_ISSUER`; *"not compatible with IORING_SETUP_SQPOLL"*. `SINGLE_ISSUER` (6.0): kernel enforces one submitter, `-EEXIST` otherwise. SQPOLL: thread sleeps after `sq_thread_idle` ms and raises `IORING_SQ_NEED_WAKEUP`; `SQ_AFF` binds it to `sq_thread_cpu`. `EPERM` when `io_uring_disabled` is 2, or 1 without `CAP_SYS_ADMIN` | Decisions 2, 3, 4, 7 |
| [`io_uring_enter(2)`](https://man7.org/linux/man-pages/man2/io_uring_enter.2.html) | `GETEVENTS` waits for `min_complete` events; with `min_complete` 0 it returns without waiting. `IORING_ENTER_EXT_ARG` carries a `struct io_uring_getevents_arg` with a timeout. `EINTR` possible *"while waiting for events"* | Decisions 3, 4 |
| [LWN 906470](https://lwn.net/Articles/906470/) , [liburing wiki, *io_uring and networking in 2023*](https://github.com/axboe/liburing/wiki/io_uring-and-networking-in-2023) | Axboe: multishot `recv`, ring-mapped provided buffers (*"vastly more efficient"*), `DEFER_TASKRUN` + `SINGLE_ISSUER` (*"gives the application full control over the batching on the completion side"*) | Decision 2's flags and receive shape |
| [`io_uring_multishot(7)`](https://man7.org/linux/man-pages/man7/io_uring_multishot.7.html) , [`io_uring_provided_buffers(7)`](https://www.man7.org/linux/man-pages/man7/io_uring_provided_buffers.7.html) | A multishot `recv` ends on error, on close, or when the buffer ring runs dry (`-ENOBUFS`, `IORING_CQE_F_MORE` absent); it must then be re-armed. Cancelled, it posts a final `-ECANCELED` | Decision 2's re-arm; trap list |
| [liburing #568](https://github.com/axboe/liburing/issues/568) , [`io_uring_cancelation(7)`](https://manpages.debian.org/testing/liburing-dev/io_uring_cancelation.7.en.html) | Closing a socket does **not** cancel requests pending on it: each holds its own file reference, so the socket stays open and the peer sees no FIN. `IORING_ASYNC_CANCEL_ALL` / `_FD` from 5.19 | Decision 5 (teardown) |
| [tokio-rs/io-uring](https://github.com/tokio-rs/io-uring) 0.7.15 (2026-09-07; [Cargo.toml](https://docs.rs/crate/io-uring/latest/source/Cargo.toml.orig)) | Pure Rust, prebuilt bindings (bindgen only behind its `overwrite` feature), `rust-version = "1.63"`, dependencies `bitflags 2`, `cfg-if 1`, `libc 0.2.98`; `RecvMulti` (6.0), `register_buf_ring_with_flags` (`unsafe`: the ring memory must stay valid until unregistered or the ring is dropped), `Submitter::enter` (`unsafe`, raw), `SubmissionQueue::push` (`unsafe`: referenced memory valid until completion), `register_napi` (6.9) | Decision 6 |
| [tokio-rs/io-uring #408](https://github.com/tokio-rs/io-uring/pull/408) (merged 2026-09-06) | With `DEFER_TASKRUN`, the crate's zero-wait `submit` entered **without** `GETEVENTS`, so deferred completions never ran | Decision 3 calls `enter` with `GETEVENTS` explicitly, whatever the crate version |
| [`rustix::io_uring`](https://docs.rs/rustix/latest/rustix/io_uring/index.html) , [rustix-uring](https://github.com/jordanisaacs/rustix-uring) | Raw bindings; *"rustix does not attempt to provide a safe API"*; rustix-uring is a fork of io-uring over rustix | Rejected in decision 6: a second syscall tree beside `libc` for no safer surface |
| [liburing #536](https://github.com/axboe/liburing/issues/536) | Echo, one connection, 64 B: epoll 1 565 K QPS vs io_uring 506 K; the gap narrows with size — *"network fd … IO depth = 1"* | The single-session wire arm may well lose; the idle arm is the likelier keep |
| [arXiv 2512.04859](https://arxiv.org/html/2512.04859v1) | 8-byte TCP/UDP ping-pong, ConnectX-7, Linux 6.17: SQPOLL beats DeferTR without NAPI, the advantage disappears with NAPI; DeferTR + NAPI best; registered buffers negligible or slightly worse for small messages | No registered buffers (decision 2); SQPOLL is an arm, not a promise (decision 4) |
| [LWN 961189](https://lwn.net/Articles/961189/) , [LWN 930494](https://lwn.net/Articles/930494/) | NAPI busy poll for `io_uring`: UDP ping RTT 37.0 → 29.8 µs; with SQPOLL 44.4 → 37.3 µs. The busy loop sits in `io_cqring_wait` and in the SQPOLL thread | Decision 4: NAPI is not reachable from an `hft` turn that never waits |
| [moby #46762](https://github.com/moby/moby/pull/46762) , [moby default profile](https://raw.githubusercontent.com/moby/profiles/main/seccomp/default.json) , [containerd #9320](https://github.com/containerd/containerd/pull/9320) | Docker 25.0 drops `io_uring_enter`, `io_uring_register`, `io_uring_setup` from the default allowlist; default action `ERRNO`, `defaultErrnoRet: 1` (EPERM); containerd's `RuntimeDefault` the same | Decision 7 |
| [kernel sysctl docs](https://docs.kernel.org/admin-guide/sysctl/kernel.html) , [Phoronix, 6.6](https://www.phoronix.com/news/Linux-6.6-sysctl-IO_uring) | `kernel.io_uring_disabled` 0/1/2; 1 admits `CAP_SYS_ADMIN` or members of `kernel.io_uring_group`; `io_uring_setup` fails `-EPERM` | Decision 7 |

**Searched, found nothing:** a published latency figure for any FIX engine on `io_uring` (as
ADR-0098 found); a statement from the kernel documentation that a pure user-space CQ peek is
sufficient without SQPOLL — every source says the opposite (task work runs on a kernel entry).

## Decision

### 1. One ring per engine thread, split into a transport and an idle strategy

`crates/engine/src/transport/uring.rs`, declared in `transport.rs` as
`#[cfg(all(feature = "io-uring", target_os = "linux"))] pub mod uring;`. A `Uring` owns the ring,
the buffer ring and a fixed slab of per-connection staging lists, behind an `Rc` (the engine
thread is the only user; the type is `!Send`, which is what `SINGLE_ISSUER` demands anyway).
From it come two kinds of value:

- **`UringTransport: Transport`** — one per connection, made from a `TcpTransport` by
  `Uring::register`. `recv` copies from the connection's staged completions into the caller's
  buffer and returns the provided buffer to the ring; it **makes no system call**. `send` is
  unchanged `write(2)` on the non-blocking socket (decision 2). `POLLABLE = true`, `source()`
  names the descriptor.
- **`UringSpin: Waiting`** (`hft`) and **`UringBlock: Waiting`** (`standard`, behind the
  `standard` feature too) — the idle strategy **is the reaper**: `idle` submits what is queued,
  enters the kernel as decision 3 says, and moves every completion into its connection's staging
  list.

Reaping lives in `idle` because `idle` is already the one call the engine makes exactly when a
turn found nothing: after `idle`, the next `turn`'s `recv` calls find the staged bytes without a
system call. Nothing in `Engine::turn`, `pump` or `presession` changes; `pump`'s existing `wrap`
parameter (`lib.rs:3469`) is how accepted sockets become `UringTransport`s, the shape `serve_tls`
already uses.

**The wrong pairing is a compile error**, ADR-0014 decision 4's shape: `Transport` gains
`const NEEDS_REAPER: bool = false` and `Waiting` gains `const REAPS: bool = false`, both
defaulted so no implementation outside this crate changes; `Engine::new` asserts
`!T::NEEDS_REAPER || W::REAPS` in a `const` block. A `UringTransport` under `Spin` or `Block`
would compile today and receive nothing, ever — the 59 definitions would time out rather than
fail — so it is refused before it runs. A `compile_fail` doctest holds it.

### 2. Receive is multishot into a provided buffer ring; send stays `write(2)`

- One `RecvMulti` per connection, buffer-selected from one ring-mapped provided-buffer group per
  `Uring` (`IORING_REGISTER_PBUF_RING`, 5.19). Buffer count (power of two) and length are
  `UringConfig` fields, validated at construction; the memory is allocated page-aligned **and
  written once per page** at construction, so no first touch happens on the hot path
  (non-negotiable 1; `alloc_zeroed` alone may hand back untouched zero pages).
- A completion without `IORING_CQE_F_MORE` ends the multishot: `-ENOBUFS` re-arms it as soon as
  a buffer is returned; `0` is end of stream (`Io::Closed`); another error is `Io::Failed`. Data
  the kernel could not place stays in the socket's own receive buffer, so running dry is TCP
  backpressure, never loss.
- Every `user_data` carries the connection's slab index **and a generation**, so a completion
  that arrives for a connection already dropped — its slot reused — is discarded, not delivered to
  the new tenant.
- **No registered buffers, no registered files, no zero-copy receive.** The first measured
  slightly worse for small messages (arXiv 2512.04859); the second is a follow-up only if the
  kept arm shows `fget` in a profile; the third needs header/data split hardware the I211 lacks
  (ADR-0098 research).
- **Send is not moved onto the ring.** Without SQPOLL a submitted send still costs an
  `io_uring_enter`, which is no cheaper than `write(2)`, and deferring it to the next `idle`
  would add a turn to every reply. The wire arm therefore compares `read` against a reaped
  completion with everything else equal.

### 3. `hft`: every idle turn is one `io_uring_enter(to_submit, 0, GETEVENTS)` — never a wait

The ring is set up with `SINGLE_ISSUER | DEFER_TASKRUN` (kernel 6.1). `UringSpin::idle` calls
`Submitter::enter(to_submit, min_complete = 0, IORING_ENTER_GETEVENTS)` directly — not the crate's
`submit()`, whose zero-wait path omitted `GETEVENTS` until tokio-rs/io-uring #408 — then reaps the
CQ from user space. `min_complete = 0` returns without waiting; `DEFER_TASKRUN` means the kernel
never interrupts the engine core to run completion work, because that work runs inside this
call. **N idle sessions cost one kernel entry per idle turn instead of N `read`s** — the term the
N = 16 kill arm measures.

**The answer to *"does peeking the CQ ring count as never sleeping?"*:** a peek alone never enters
the kernel and so never sleeps — but without SQPOLL it is also not enough to receive. Under
`DEFER_TASKRUN` a peek sees nothing until an `enter` with `GETEVENTS`; without it (and without
`COOP_TASKRUN`), each completion arrives by **interrupting the engine core** to run task work.
That second arm is rejected (decision 8). What `hft` does is enter the kernel once per idle
turn and never wait there: zero voluntary context switches, which
`scripts/check-no-kernel-sleep-by-ctxt.sh` reads without a tracer, and an `io_uring_enter` whose
third argument is 0, which ADR-0191 teaches `scripts/check-no-kernel-sleep.sh` to tell apart
from a waiting one.

### 4. SQPOLL is an `hft` arm only, pinned; NAPI is not built in row 5

- **`HftArm::Sqpoll { pin: CorePin }`** *(R1)* sets up the ring with `SQPOLL | SQ_AFF`,
  `sq_thread_cpu = pin.core()`, validated by `CorePin::validate()` before any socket exists — the
  rules `serve_hft_pinned` applies: absent or offline is refused, and outside `isolcpus` is refused
  unless the caller wrote `CorePin::allow_unisolated()`. The waiver is reported, never implied:
  `UringReport` says whether it was taken. `DEFER_TASKRUN` is
  incompatible with it, so completions are posted by the SQ thread and `UringSpin::idle` only
  peeks — this is the one arm in which the engine thread does not enter the kernel to receive. If
  the kernel raises `IORING_SQ_NEED_WAKEUP`, `idle` calls `enter` with `IORING_ENTER_SQ_WAKEUP` and
  `min_complete = 0`, which does not wait. It **burns a second core**, and every figure from it
  says so beside the number.
- **Never a default** (owner's Q8): `HftArm::Enter` is what `serve_hft_uring` uses unless the
  caller names `Sqpoll`, and `tools/w2w` needs `--uring-arm sqpoll --sqpoll-core <cpu>` spelled out.
- **Never in `standard`**, by type: `UringBlock` is built from a configuration that has no SQPOLL
  field, so *standard + SQPOLL* cannot be written; a `compile_fail` doctest holds it. A spinning
  kernel thread under a `standard` engine is the defect non-negotiable 4 names.
- **NAPI busy poll is not built here.** Its busy loop runs inside a *waiting* `io_uring_enter`
  (`io_cqring_wait`) or inside the SQPOLL thread (LWN 930494). The first is forbidden in `hft`;
  the second is a variant of the SQPOLL arm. It is reopened as *SQPOLL + NAPI* by its own plan only
  if row 7 shows the SQPOLL arm within reach of the kill line. `[unverified on this kernel]` that
  a `min_complete = 0` enter skips the NAPI loop — read from the patch, not measured.

### 5. `standard`: `io_uring_enter(to_submit, 1, GETEVENTS | EXT_ARG, timeout 100 ms)`

- Same `SINGLE_ISSUER | DEFER_TASKRUN` ring. `UringBlock::idle` waits for one completion with the
  `Block` timeout (100 ms by default, `with_timeout_ms` for tests) passed through
  `IORING_ENTER_EXT_ARG` — the timeout is a correctness parameter, as it is for `Block`: it is what
  delivers `Input::Tick`. `EINTR` is a wake, not an error.
- `UringBlock::NEEDS_SOURCES = true`: it is shown the interest list. *(R2)* **A poll lives for
  exactly one wait**: before each wait it arms a one-shot `POLL_ADD` for every readable source the
  ring does not already cover (the listener, the dispatch waker's pipe) and for every `writable`
  interest, each `user_data` carrying the wait's generation; right after the wait it queues an
  `ASYNC_CANCEL` by `user_data` for every one that did not fire, and a completion from an older
  generation is discarded. Sources that are registered connections are covered by their multishot
  `recv` and skipped.
- *(R3)* **It does not wait while the engine has bytes to read that the kernel can no longer
  see.** If a source **in this turn's readable interest list** has reaped bytes staged and unread,
  the enter uses `min_complete = 0`; otherwise `1`. Staged bytes of a connection **not** in the
  list (a connection parked by `Recovery::ready`, for instance) do not count — exactly as `poll(2)`
  ignores a descriptor it was not given; a ring-wide count would make `standard` spin for as long
  as such a connection stays parked.
- *(R3)* **A source that cannot be armed is counted, not hidden**: `UringReport::unarmed`. Such a
  source is woken only by the timeout. The SQ and CQ are sized at setup so it cannot happen in a
  configuration the entry points accept (R4); `unarmed > 0` in any test or script run is a failure.
  `idle_with`'s drain of the waker after the wait (`lib.rs:1689-1698`) applies unchanged.

### 6. The dependency is the `io-uring` crate, pinned `>= 0.7.15`, Linux only

`[target.'cfg(target_os = "linux")'.dependencies] io-uring = { version = "0.7.15", optional = true }`,
turned on by the engine's `io-uring` feature together with the optional `libc` it already has.
It adds `bitflags` and `cfg-if` (MIT OR Apache-2.0) and no async runtime, so `CLAUDE.md` §6 needs
no ADR for it; this paragraph is the plan's justification. **Rejected**: `rustix::io_uring`
(raw, no safe surface, and a second syscall crate tree beside `libc`); `rustix-uring` (the same
API as `io-uring` over that second tree); liburing bindings (a C toolchain in `build.rs`, which D5
rule 2 exists to keep out); hand-written bindings (all the `unsafe` of the crate, none of its
review). The crate's `unsafe` calls this transport makes are listed, each with what proves it,
in the plan's *Bất biến* section (non-negotiable 8).

### 7. Blocked means refused at startup, named, never a fallback

`Uring::hft` and `Uring::standard` return `Result<_, UringRefused>`; `serve_uring` and
`serve_hft_uring` turn it into `ServeError::Uring` **before binding the listener**. The variants
say where the operator goes next:

| `io_uring_setup` / probe says | Variant | The message names |
|---|---|---|
| `EPERM`, `/proc/sys/kernel/io_uring_disabled` reads 1 or 2 | `Disabled { sysctl }` | the sysctl and its value; `kernel.io_uring_group` for 1 |
| `EPERM`, sysctl reads 0 or cannot be read | `Blocked` | a seccomp filter — Docker ≥ 25 / containerd `RuntimeDefault` / systemd `SystemCallFilter` |
| `ENOSYS` | `NotInKernel` | a kernel without `CONFIG_IO_URING`, or a sandbox answering `ENOSYS` |
| `EINVAL` on setup, or the probe lacks `RECV`/`POLL_ADD`/`ASYNC_CANCEL`, or buffer-ring registration fails `EINVAL` | `KernelTooOld` | 6.1 as the floor (`DEFER_TASKRUN`) |
| anything else | `Other(io::ErrorKind)` | the kind |
| *(R4)* `UringConfig::connections` < capacity + pending | `TooSmall { have, need }` | both numbers — a configuration error, not the kernel's |

There is **no code path** from a `serve_*uring*` function to a `TcpTransport` read loop. A
refusal is proven by a test that installs a real seccomp filter answering `EPERM` on the test's
own thread (Docker's mechanism, no privilege needed) and by a desk-only test under the sysctl.

- *(R4)* **The ring holds every socket the serving loop can hold.** Sockets are registered at
  accept, before their `Logon`, so `UringConfig::connections` must be at least the engine's
  capacity plus `presession::Limits::pending()` (the pending and parked sets together are bounded
  by it). `serve_uring` and `serve_hft_uring` check it **at startup** and refuse with
  `UringRefused::TooSmall { have, need }` before binding; a caller driving an `Engine` by hand is
  told in the rustdoc, and an over-full ring makes `register` return `None`, which closes the
  socket like any connection there is no room for.

### 8. Teardown: `shutdown(2)` before close, cancel by `user_data`

Dropping a `UringTransport` calls `shutdown(SHUT_RDWR)` — which ends its multishot `recv` and sends
the peer its FIN even though the pending request still holds the file — queues an
`ASYNC_CANCEL` keyed by that connection's `user_data` for the next `idle`, frees its staging list
back to the buffer ring, and bumps the slot's generation. Dropping the `Uring` unregisters the
buffer ring before its memory is freed (field order and an explicit `Drop`), and closing the ring
cancels whatever is still pending.

### 9. Rejected: an `hft` arm that only peeks and takes an interrupt per completion

A ring set up with neither `DEFER_TASKRUN` nor `COOP_TASKRUN` delivers completions to a spinning
thread by interrupting it. It would need no `io_uring_enter` at all, and it might even measure
fast — but `DESIGN.md` §9's IRQ-affinity row exists so that **the engine never takes an
interrupt**, and this arm takes one per message by construction. Reopened only by a new ADR that
also amends that §9 row.

### 10. What row 7 judges, and how the kill line reads for this transport

Row 7 measures on one §9 boot, both binaries pre-built (ADR-0090), `check-machine.sh` beside each
procedure, two procedures ≥ 30 min apart with the arm order reversed in the second (ADR-0068):

| Arm | Command | Judged |
|---|---|---|
| **K** | `scripts/w2w-baseline.sh`, `ARMS="hft:admin"`, `W2W_EXTRA="--wire-timestamps --nic enp9s0 --observer-core <c>"`, Mac mini generator, interval 0, 20 000 requests × 10 runs | control |
| **U** | the same with `--transport uring` added to `W2W_EXTRA` | **yes** |
| **S** | the same with `--transport uring --uring-arm sqpoll --sqpoll-core <isolated core>` | reported beside, **cannot keep the item** |
| **idle** | `cargo bench -p fixbolt-engine --features io-uring --bench turn` pinned to the engine core, cases *idle loop, 16 idle sessions, kernel* and *…, uring* (N = 1, 16, 64 printed) | **yes**, at N = 16 |
| **std** | engine `--listen --mode standard [--transport uring]`, generator table *as the counterparty sees it* from the Mac | decides the `standard` half only |
| **density** | `cargo bench -p fixbolt-engine --features io-uring --bench density` | recorded, not judged |

- **Kept** if either: (a) U's admin wire p50 ≤ 0.97 × K's in **both** procedures and U's p99 ≤
  1.05 × K's in both; or (b) the idle loop at N = 16 is ≤ 0.75 × K's in both procedures **and** U's
  admin wire p50 ≤ 1.05 × K's in both (ADR-0068 decision 2's 5 % is *the band*).
- **S cannot keep the item alone.** If only S passes, the item is killed, S's pair is recorded with
  its second core named, and the owner is told; making SQPOLL a default would need a new ADR
  superseding Q8.
- **The `standard` half is kept only if it is not worse**: if the std arm's generator-side p50 with
  `uring` is more than 5 % worse than without it in both procedures, `UringBlock` and
  `serve_uring` are removed and the transport stays `hft`-only. `standard` on a hardware NIC has
  no wire column (`w2w` refuses `--mode standard --wire-timestamps` there), which is why this half
  is judged from the generator's side.
- **Killed** means ADR-0098's meaning: feature, module, flags and script arms removed on row 7's
  branch, every pair recorded in `measured-costs.md`, this ADR marked with the result.

## Consequences

**Good**

- The engine loop, the pre-session stage and every existing transport are unchanged; the only
  edits outside the new module are two defaulted associated constants, one defaulted
  `Transport::carrier` method with its `Engine::carrier` reader (so `tools/w2w` prints the
  transport the engine reports, the way it prints `tls:` — row 7's interface), one `const`
  assertion, one `ServeError` variant and two entry points.
- The idle cost becomes one kernel entry per turn whatever N is — the lever the 703 ns-per-socket
  finding pointed at since 2026-08-30.
- `hft` never waits and the engine core takes no interrupt from the ring; SQPOLL exists for the
  measurement Q8 asked for and cannot become a default by accident.
- A container that blocks `io_uring` gets a sentence naming seccomp, not a latency figure about a
  different code path.

**Bad**

- **A received byte waits for the next idle turn.** After a turn that moved something, the next
  turn runs before anything new is reaped. At N = 1 that is one extra user-space pass; under a
  steady flood it is batching the kernel arm does not do. The wire arm will show it or not.
- **One more syscall shape on the hot path.** `hft`'s gate must now read an argument, not a name
  (ADR-0191), and a gate that reads arguments can be fooled by a format change in `strace`.
- **`unsafe` enters a crate whose default build has none**: seven sites, each named with its
  proof in the plan. Miri cannot run `io_uring`; the kernel's writes into the buffer ring are
  invisible to every sanitizer, so the proof of the buffer-ownership invariant is a ledger checked
  in tests and a byte-exact stress run, not a tool.
- **An attack surface the rest of the engine does not have** (ADR-0098: 60 % of 2022 kernel bug
  bounty exploits). Off by default, refused when blocked, and `GUIDE.md` says so.
- **Kernel ≥ 6.1** for this feature, above anything the default build asks.
- **The sharded runtime, the initiator, TLS and the settings file do not get it in row 5.** Each is
  a follow-up only if row 7 keeps the item.
- **The literature expects the single-session wire arm to lose** (liburing #536). The item may well
  survive on the idle arm alone, which is a statement about many sessions per core, not about the
  published single-session figure.

## Revision 1 — 2026-09-24 (while Proposed, after steps 1–4 were built)

Asked by the senior developer before step 5; decided by the architect; the decision text above is
edited in place and marked *(R1)*–*(R4)*.

- **R1 — the SQPOLL core is a `CorePin`, not a bare `CoreId`.** Was: `Sqpoll { core }`, validated
  as isolated with no waiver. `CorePin` (`crates/engine/src/affinity.rs:588-650`) already carries
  `allow_unisolated`, reports it (`is_unisolated_allowed`, ADR-0015 decision 5) and validates through
  the same `ShardPlan` rules, so a separate flag would be a second copy of one rule. `tools/w2w`'s
  existing `--allow-unisolated` applies to the SQ core as it does to the engine core, and its
  `transport:` line prints `unisolated=yes|no` for the SQPOLL arm. Row 7 runs on a §9 boot without
  the waiver; the plan's step 7 desktop run uses it and says so.
- **R2 — a `POLL_ADD` is armed per wait and cancelled after it**, replacing *"arm once, track in a
  table, cancel when the source leaves the list"*. The table was keyed by descriptor number, and a
  listener closed and replaced by one that received the same number **between two turns** never
  leaves the list, so the old poll — holding the old file — would stand in for the new one and the
  new listener would be woken only by the timeout. Arming per wait binds each poll to the file of
  that turn. No wake is lost at the boundary: a poll armed on an already-readable source completes
  at once. Cost: two SQEs per extra source per idle turn (normally two sources), carried by the
  same `io_uring_enter`, no extra syscall. Accepted.
- **R3 — `standard` does not wait while listed sources have staged bytes**, and unarmed sources are
  counted. Accepted, **narrowed**: the built version keys the decision on a ring-wide
  `staged_slots` count; it must key it on the staged bytes of sources in this turn's readable
  interest list (the built fd hash of covered sources makes that a lookup per listed source).
  Reason: a connection parked by `Recovery::ready` is neither read nor listed
  (`lib.rs` `pump_loop`, *"Parking is not progress"*), and a peer that writes while parked would
  otherwise keep `standard` at `min_complete = 0` — spinning — until the park ends, up to its
  `LogonTimeout`. The kernel arm does not spin there, because `poll` is never given that
  descriptor. Guard: test `standard_does_not_spin_on_bytes_nobody_asked_for` (a registered,
  unlisted connection with staged bytes; `idle` with a 50 ms timeout must take ≥ 40 ms; red with
  the ring-wide count). `UringReport::unarmed` and the enlarged SQ/CQ are accepted with R4's
  sizing rule; `unarmed` is printed by `tools/w2w` and asserted 0 by
  `scripts/check-standard-gives-the-core-back.sh`.
- **R4 — ring sizing is checked at startup.** `connections ≥ capacity + pending`, refused otherwise
  (decision 7's new row). The SQ holds, for one idle turn, every re-arm and cancel the ring can owe:
  one `RecvMulti` re-arm and one teardown cancel per connection, plus two SQEs per extra source and
  per writable interest; the CQ holds every buffer the ring can fill plus one SQ's worth of
  completions. Both are derived from `UringConfig` at construction, not tuned by hand; a CQ
  overflow is counted in `UringReport` and asserted 0 in `tests/uring.rs`.
