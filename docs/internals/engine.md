# `engine` — internals

Layer L3 in [DESIGN.md §3](../DESIGN.md#3-crates): the TCP acceptor and connector, the
thread that drives the session machines, the journal and the message log. The largest crate
— [DESIGN.md's *What `engine` contains* table](../DESIGN.md#what-engine-contains) already names each module's
decision (D1–D15, ADR numbers); this page adds the file names, a read order and the guarding
tests, and does not repeat that table's *Decision* column.

## Files, and what each keeps

| File | Keeps |
|---|---|
| `lib.rs` | `Engine`, `turn`, `run`, `serve*` entry points — one non-blocking pass, and the loop around it |
| `transport.rs` | `Transport`, `TcpTransport`, `Loopback`, `Waiting`, the `Io` result type |
| `frame.rs` | `Framer` — cutting a byte stream into messages by `9=`, nothing parsed |
| `presession.rs` | `Identity`, `PendingSet`, `Registry`, `Table` — who owns a socket before a session exists |
| `conn.rs` | One connection: socket, receive buffer, state machine, unsent bytes |
| `backpressure.rs` | The queue a connection uses when the counterparty stops reading (D10) |
| `dispatch.rs`, `ring.rs` | `InlineDispatch`, `RingDispatch` over an `AtomicU8` SPSC ring (D4) |
| `journal.rs` | `MemJournal`, `FileJournal`, `Reader`, `Store` — the resend store |
| `recovery.rs` | `Recovery`, `Resumed`, `NoRecovery`, `FromFn` — asked once the counterparty is known |
| `msglog.rs` | `MessageLog`, `FileLog` — every message seen or sent, both directions, one line each |
| `observe.rs` | `Handles`, `Observer`, `Event`, `Admin` — the operator's on-request view |
| `origin.rs` | `Sender`, the fixed origination queue — a message an application starts from another thread |
| `settings.rs` | `Settings`, `Problem` — the QuickFIX-shaped configuration file |
| `reconnect.rs` | `Policy` — doubling backoff, and `connect_and_serve` |
| `clock.rs` | Where the engine gets the time, as a trait — the seam the acceptance corpus drives |
| `poll.rs`, `block.rs`, `wait.rs`, `waker.rs` | `standard` mode's idle turn: the `poll(2)` call, the policy around it, the mode split, and how another thread wakes a blocked engine |
| `affinity.rs`, `shard.rs` | Pinning a thread to a core and proving it, and many engines each on one pinned core |
| `tls.rs` | TLS as a second `Transport`: `rustls` handshake, kTLS steady state on Linux |

## Read in this order

1. `lib.rs`, `transport.rs`, `frame.rs` — the shape of one turn
2. `presession.rs`, `conn.rs` — how a socket becomes a session
3. `backpressure.rs`, `dispatch.rs`/`ring.rs` — what happens to an outbound message
4. `journal.rs`, `recovery.rs`, `msglog.rs` — persistence and what a reconnect sees
5. `observe.rs`, `origin.rs`, `settings.rs`, `reconnect.rs`, `clock.rs` — operator-facing and
   configuration seams
6. `poll.rs` → `block.rs` → `wait.rs` → `waker.rs` — `standard` mode's idle turn, in the order
   one syscall builds on the next
7. `affinity.rs` → `shard.rs` — cores, then many engines across them
8. `tls.rs` — a second transport, read last since it assumes 1–3

## Tests that guard it

- `tests/wire.rs` — the 59 acceptance definitions over a real socket
- `tests/presession.rs`, `tests/registry.rs`, `tests/pending.rs`, `tests/connection_size.rs`,
  `tests/buffer_size.rs` — `presession.rs`, `conn.rs`
- `tests/backpressure.rs`, `tests/dispatch.rs`, `tests/frame.rs` — one module each
- `tests/journal.rs`, `tests/journal_reader.rs`, `tests/on_disk.rs`, `tests/recovery.rs`,
  `tests/engine_recovery.rs`, `tests/msglog.rs` — persistence and recovery
- `tests/observe.rs`, `tests/events.rs`, `tests/admin.rs`, `tests/originate.rs`,
  `tests/settings.rs`, `tests/settings_roles.rs`, `tests/reconnect.rs`,
  `tests/reconnect_wire.rs`, `tests/shutdown.rs` — operator and config seams
- `tests/standard.rs`, `tests/hft_wire.rs`, `tests/hft_pinned.rs`, `tests/waker_sigpipe.rs`,
  `tests/listener_cadence.rs` — the mode split, machine-checked also by
  `scripts/check-no-kernel-sleep.sh` and `scripts/check-standard-gives-the-core-back.sh`
- `tests/affinity.rs`, `tests/shard.rs`, `tests/shard_hft.rs`, `tests/shard_wire.rs` —
  `affinity.rs`, `shard.rs`
- `tests/tls*.rs` — `tls.rs`
- `benches/alloc.rs` — non-negotiable 1; `benches/turn.rs`, `benches/dispatch.rs` — the
  per-turn and dispatch cost
