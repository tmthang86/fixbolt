# A transmit timestamp wakes a blocking engine

`[2026-09-14]` found while building step A3b of
[the second Linux desk plan](../plans/2026-09-04-the-second-linux-desk.md) (`tools/w2w
--wire-timestamps`), written up in that plan's Sửa 2, Điều 3.

**Put `SO_TIMESTAMPING` with a TX flag on a `standard` engine's socket, and the engine spins
in bursts until someone reads the socket's error queue.** Nothing in the engine is wrong; the
instrument made it spin. A `standard` engine that spins is non-negotiable 4's second half
broken, so a latency figure taken this way is a figure about an engine that spins, not about
`standard`.

## The mechanism, from the kernel

1. **A TX timestamp is queued on the socket's error queue.** With `SOF_TIMESTAMPING_TX_*` set,
   the stack (or the driver, for a hardware stamp) hands a clone of the sent skb to
   `sock_queue_err_skb`, which calls `sk_error_report` → `sock_def_error_report` →
   `wake_up_interruptible_poll(…, EPOLLERR)` (`net/core/sock.c`). Every waiter on that socket is
   woken.
2. **`tcp_poll` reports `EPOLLERR` whenever that queue is not empty**: `if (READ_ONCE(sk->sk_err)
   || !skb_queue_empty_lockless(&sk->sk_error_queue)) mask |= EPOLLERR;` (`net/ipv4/tcp.c`,
   `tcp_poll`).
3. **`poll(2)` sets `POLLERR` in `revents` whatever `events` asked for** — "will be set in the
   revents field whenever the corresponding condition is true" (the man page). There is no mask
   that turns it off.

So a `standard` engine asleep in `poll` wakes, `recv` returns `EAGAIN` (no data), it goes back
to `poll`, and `poll` returns **at once**, because the error queue is still not empty. It loops
until a reader calls `recvmsg(MSG_ERRQUEUE)`. The engine never reads the error queue — it knows
nothing about timestamps, by design — so how long each burst lasts is decided entirely by
whoever does (in `w2w`, the observer thread).

**`hft` is unaffected**: its engine thread never waits in `poll`; it calls a non-blocking `recv`
every turn, so there is nobody for `POLLERR` to wake. The engine thread's syscall set with and
without `--wire-timestamps` is the same. **`lo` never shows it**: loopback has no hardware clock,
so a hardware-only TX request queues nothing, and the error queue stays empty.

## What was seen

Probe, not a figure — senior developer, 2026-09-14, the Linux desk (Ryzen 7 3700X, kernel
7.0.0-31-generic), shared and not in `DESIGN.md` §9 tuning. A throwaway build of `w2w` asked for
**software** TX stamps as a stand-in for hardware ones, so that `lo` would queue something; the
build was deleted after.

- `w2w --mode standard --messages 1000 --warmup 50 --interval 2000 --client-core 7
  --wire-timestamps --nic lo --observer-core 2`, under `sudo -n strace -f -u tmt`: **111 of
  1 051 replies** woke the engine thread with `revents=POLLERR`, and the longest run was **67
  consecutive** `poll → recvfrom EAGAIN` iterations before the observer read the stamp. The
  same command without the flag: 0 `POLLERR`. Under strace the observer is slowed too, so 67 is
  larger than it would be without it.
- Without strace, 1 000 messages a second: the engine thread's CPU over 2 s read **2.48 %
  without the flag and 3.48 % with it** — one run each, at `CLK_TCK = 100` a 0.5 % resolution.
  A direction, not a number.

## What guards it

- **`tools/w2w` refuses `--mode standard --wire-timestamps` on any NIC that is not loopback**,
  checked before it needs a capability, with a sentence that names `POLLERR`. Test:
  `standard_is_refused_on_a_hardware_nic_and_not_on_loopback` in `tools/w2w/src/main.rs`, and the
  run of a binary with no capability against a real NIC (step A3b's gate).
- **`scripts/check-standard-gives-the-core-back.sh` cannot catch it**, and its header says so: it
  measures an idle window, where no reply is being sent, and on `lo` no stamp is ever queued.
- `standard` on a real NIC is measured **without** the flag: only the generator's table, *as the
  counterparty sees it*.

## What would let `standard` be stamped

Recorded for later; none of it is built.

- **The engine reads and discards its own error queue** on `POLLERR` with `EAGAIN`. A change to
  `crates/engine`, a syscall on `standard`'s path, and an engine that knows about timestamps —
  it would need its own plan and an ADR, and it would measure a different engine.
- **BPF sock_ops TX timestamping** (Jason Xing's series, in the kernel since 6.15): a cgroup BPF
  program enables `SK_BPF_CB_TX_TIMESTAMPING` and receives `BPF_SOCK_OPS_TSTAMP_SND_HW_CB`, with
  the application's socket untouched. `SKBTX_HW_TSTAMP = SKBTX_HW_TSTAMP_NOBPF | SKBTX_BPF`
  (`include/linux/skbuff.h`) suggests a BPF-only request queues nothing on the error queue —
  verified 2026-09-18, [ADR-0073](../decisions/ADR-0073-the-standard-wire-figure-goes-through-bpf-sock-ops-and-not-before-phase-2.md),
  quoted: `__skb_tstamp_tx` is read and the route is fixed — `tools/w2w-bpf/`, `clang -target bpf`
  via `scripts/build-bpf-tstamp.sh`, loaded by `aya`, behind an off-by-default `bpf-tstamp`
  feature — but it is not built before phase 2's first §9 boot (ADR-0073 decision 2). Needs a
  BPF loader (a new dependency and an ADR), `CAP_BPF` and `CAP_NET_ADMIN`.
  Sources: <https://lwn.net/Articles/996139/>,
  <https://github.com/torvalds/linux/commit/59422464266f8baa091edcb3779f0955a21abf00>.
- **Stamping outside the host** — a passive tap into a timestamping capture appliance — which
  touches nothing in the host at all, and is outside this project's hardware.

## Sources

- `poll(2)`: <https://man7.org/linux/man-pages/man2/poll.2.html>
- Kernel timestamping documentation (error queue, `OPT_ID_TCP`, `OPT_TSONLY`):
  <https://docs.kernel.org/networking/timestamping.html>
- `net/ipv4/tcp.c` (`tcp_poll`) and `net/core/sock.c` (`sock_def_error_report`), read at tag
  `v7.0` from <https://github.com/torvalds/linux>.
