# `engine` — internals

Layer L3 in [DESIGN.md §3](../DESIGN.md#3-crates): the TCP acceptor and connector, the
thread that drives the session machines, the journal and the message log. The largest crate
— [DESIGN.md's *What `engine` contains* table](../DESIGN.md#what-engine-contains) already names each module's
decision (D1–D15, ADR numbers); this page adds the file names, a read order and the guarding
tests, and does not repeat that table's *Decision* column. `fix50sp2` is a pass-through feature
only (`fixbolt-dict/fix50sp2`, `fixbolt-session/fix50sp2`) — no module here is gated on it.

## Files, and what each keeps

| File | Keeps |
|---|---|
| `lib.rs` | `Engine`, `turn`, `run`, `serve*` entry points — one non-blocking pass, and the loop around it |
| `transport.rs` | `Transport`, `TcpTransport`, `Loopback`, `Waiting`, the `Io` result type |
| `frame.rs` | `Framer` — cutting a byte stream into messages by `9=`, nothing parsed |
| `presession.rs` | `Identity`, `PendingSet`, `Registry`, `Table` — who owns a socket before a session exists |
| `conn.rs` | One connection: socket, receive buffer, state machine, unsent bytes; its `Drop` retires the journal (ADR-0153, `tests/retire.rs`) |
| `backpressure.rs` | The queue a connection uses when the counterparty stops reading (D10) |
| `dispatch.rs`, `ring.rs` | `InlineDispatch`, `RingDispatch` over an `AtomicU8` SPSC ring (D4). `[2026-09-24]` `Dispatch::ring_to_app(&self) -> Option<Occupancy>`, a provided method defaulting to `None`; `RingDispatch` answers it from the new `Producer::capacity()` and the existing `Producer::free()`, in bytes |
| `journal.rs` | `MemJournal`, `FileJournal`, `Reader`, `Store` — the resend store |
| `recovery.rs` | `Recovery`, `Resumed`, `NoRecovery`, `FromFn` — asked once the counterparty is known |
| `msglog.rs` | `MessageLog`, `FileLog` — every message seen or sent, both directions, one line each |
| `redact.rs` | `MASKED`, `mask`, `carries_secret` — what never reaches disk in clear, and the SOH-splitting scan that finds it, called from `msglog.rs` and `journal.rs`. `[2026-09-23]` |
| `observe.rs` | `Handles`, `Observer`, `Event`, `Admin` — the operator's on-request view. `[2026-09-24]` `Occupancy { used, capacity }`; `Snapshot::ring_to_app()`/`presession_slots() -> Option<Occupancy>` (`None` means "nothing reported," not zero); `Observer::latest()`, which reads the published cell without raising the request flag — added for `fixbolt-metrics` (ADR-0170) and additive under `cargo-semver-checks`. `[2026-09-24, plan *Sửa 1*, review of PR #108 F5]` `Observer::ask()`, which raises the request flag and locks nothing; `request()` is now `ask()` then `latest()`, so a caller who only wanted the flag raised no longer pays for a copy it throws away |
| `origin.rs` | `Sender`, the fixed origination queue — a message an application starts from another thread |
| `settings.rs` | `Settings`, `Problem` — the QuickFIX-shaped configuration file |
| `reconnect.rs` | `Policy` — doubling backoff, and `connect_and_serve` |
| `clock.rs` | Where the engine gets the time, as a trait — the seam the acceptance corpus drives |
| `poll.rs`, `block.rs`, `wait.rs`, `waker.rs` | `standard` mode's idle turn: the `poll(2)` call, the policy around it, the mode split, and how another thread wakes a blocked engine |
| `affinity.rs`, `shard.rs` | Pinning a thread to a core and proving it, and many engines each on one pinned core |
| `tls.rs` | TLS as a second `Transport`: `rustls` handshake, kTLS steady state on Linux; a handshake TLS refuses sends its alert and ends as `Step::Refused`, apart from a peer that left (ADR-0151) |

## Read in this order

1. `lib.rs`, `transport.rs`, `frame.rs` — the shape of one turn
2. `presession.rs`, `conn.rs` — how a socket becomes a session
3. `backpressure.rs`, `dispatch.rs`/`ring.rs` — what happens to an outbound message
4. `journal.rs`, `recovery.rs`, `msglog.rs`, `redact.rs` — persistence, what a reconnect sees,
   and what neither file may hold
5. `observe.rs`, `origin.rs`, `settings.rs`, `reconnect.rs`, `clock.rs` — operator-facing and
   configuration seams
6. `poll.rs` → `block.rs` → `wait.rs` → `waker.rs` — `standard` mode's idle turn, in the order
   one syscall builds on the next
7. `affinity.rs` → `shard.rs` — cores, then many engines across them
8. `tls.rs` — a second transport, read last since it assumes 1–3

## Tests that guard it

- `tests/wire.rs` — the 59 acceptance definitions over a real socket; `tests/wire_fixt.rs`
  (`fix50sp2`) the FIXT 60. `[changed 2026-09-20]` Both settle a step on counted framed-message
  facts — every message the engine consumed, every one the harness read — rather than on a
  wall-clock quiet, so a slow answer can no longer be misread as "finished"
  ([ADR-0087](../decisions/ADR-0087-a-socket-harness-settles-on-counted-records-and-the-clock-waits-for-the-engine.md),
  [ADR-0091](../decisions/ADR-0091-the-socket-harness-race-is-the-loopback-stacks-not-the-schedulers-and-its-reversal-runs-on-macos.md)).
  The old quiet becomes a 5 s lifeline, reported and asserted hit zero times.
  `scripts/check-socket-corpus-under-contention.sh` runs both under concurrent copies in the
  `gates` job and counts red/green — a count, not a §9 measurement.
- `tests/presession.rs`, `tests/registry.rs`, `tests/pending.rs`, `tests/connection_size.rs`,
  `tests/buffer_size.rs` — `presession.rs`, `conn.rs`
- `tests/backpressure.rs`, `tests/dispatch.rs`, `tests/frame.rs` — one module each
- `tests/journal.rs`, `tests/journal_reader.rs`, `tests/on_disk.rs`, `tests/recovery.rs`,
  `tests/engine_recovery.rs`, `tests/msglog.rs` — persistence and recovery
- `tests/redact.rs`, `tests/secrets_stay_off_disk.rs` — `redact.rs`, and the two write paths that
  call it (`msglog.rs`, `journal.rs` under both `Durability`); `benches/alloc.rs` cases
  `redact-mask`/`redact-scan` hold non-negotiable 1 for it. `[2026-09-23]`, ADR-0110
- `tests/observe.rs`, `tests/events.rs`, `tests/admin.rs`, `tests/originate.rs`,
  `tests/settings.rs`, `tests/settings_roles.rs`, `tests/reconnect.rs`,
  `tests/reconnect_wire.rs`, `tests/shutdown.rs` — operator and config seams
- `tests/observe_occupancy.rs` — `[2026-09-24]` `Occupancy`, `ring_to_app`, `presession_slots`
  and `latest`: a `RingDispatch` reports its ring's fullness, `InlineDispatch` reports none, a
  hand-built engine and one behind `serve*`'s front door differ on `presession_slots`
  (`Engine::note_presession_slots`, called beside every `note_unframeable`), and `latest()` does
  not itself raise the request flag. `benches/alloc.rs` case `observe-asked-ring` holds
  non-negotiable 1 for the `RingDispatch` read. `[plan *Sửa 1*]` `ask_raises_the_flag_and_reads_nothing`:
  a `turn()` after `ask()` moves `published()` by exactly one, and `ask()` before the first
  publish neither panics nor returns anything
- `tests/standard.rs`, `tests/hft_wire.rs`, `tests/hft_pinned.rs`, `tests/waker_sigpipe.rs`,
  `tests/listener_cadence.rs` — the mode split, machine-checked also by
  `scripts/check-no-kernel-sleep.sh` and `scripts/check-standard-gives-the-core-back.sh`
- `tests/affinity.rs`, `tests/shard.rs`, `tests/shard_hft.rs`, `tests/shard_wire.rs`,
  `tests/shard_recovery.rs` — `affinity.rs`, `shard.rs`. `shard_recovery.rs`
  (`[2026-09-20]`, [ADR-0088](../decisions/ADR-0088-recovery-reaches-the-sharded-runtime-and-the-journal-crosses-the-channel-with-the-connection.md))
  guards `serve_sharded_hft_with_recovery`/`_with`: a `Recovery` answering on the acceptor
  thread hands `Start<J>` across the shard channel, and the resulting Logon reply carries the
  sequence number recovery gave it, not `1` — **and asks for no resend**, which is the assertion
  that carries the *inbound* number: without it `next_in` could be replaced by a literal and no
  test in the workspace would go red. `serve_sharded_hft_serves_a_session` in
  `tests/shard_hft.rs` stays green unmodified, which is the proof the fresh (`NoRecovery`) path
  did not move — `tests/shard.rs` is **not** that file and did change, because `Counter`
  implements `Shardable` and gained the required `add_started`
- `tests/tls*.rs` — `tls.rs`
- `benches/alloc.rs` — non-negotiable 1; `benches/turn.rs`, `benches/dispatch.rs` — the
  per-turn and dispatch cost
- `benches/wakeup.rs` — cross-thread wake latency, `epoll_wait` and `poll` arms, 20 000
  samples each (200 under `-- --test`). The sampling is its own, because closure timing on one
  thread cannot see a wait that spans two; each arm's **p50** is handed to
  `codec/benches/harness.rs`'s `Suite::figure` (pulled in by `#[path]`, as `turn.rs` and
  `density.rs` do), so it meets the per-machine band in `benches/baselines.tsv` (`wakeup epoll
  p50`, `wakeup poll p50`, recorded unpinned) and an over-band p50 fails the run like any other
  case ([ADR-0096](../decisions/ADR-0096-a-figure-measured-elsewhere-meets-the-same-band-the-baseline-file-stays-out-of-the-heap-and-a-boot-may-rebuild-on-its-housekeeping-cores.md) decision 1). min, p99,
  p99.9 and max are printed, not banded. The seam is guarded by the `figure_*` tests in
  `crates/codec/tests/bench_verdict.rs`
