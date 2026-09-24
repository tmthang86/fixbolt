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
| `transport/uring.rs` | `[2026-09-24]` A second `Transport`, behind `#[cfg(all(feature = "io-uring", target_os = "linux"))]`: `Uring` (the ring, the provided-buffer ring, the per-connection staging slab, `!Send`), `UringTransport`, `UringSpin`/`UringBlock` (the idle strategy that is also the reaper), `UringConfig`, `HftArm`, `UringRefused`, `UringReport`. The module doc names each `unsafe` block (U1–U7) and the test that proves it |
| `frame.rs` | `Framer` — cutting a byte stream into messages by `9=`, nothing parsed |
| `presession.rs` | `Identity`, `PendingSet`, `Registry`, `Table` — who owns a socket before a session exists |
| `conn.rs` | One connection: socket, receive buffer, state machine, unsent bytes; its `Drop` retires the journal (ADR-0153, `tests/retire.rs`) |
| `backpressure.rs` | The queue a connection uses when the counterparty stops reading (D10) |
| `dispatch.rs`, `ring.rs` | `InlineDispatch`, `RingDispatch` over an `AtomicU8` SPSC ring (D4). `[2026-09-24]` `Dispatch::ring_to_app(&self) -> Option<Occupancy>`, a provided method defaulting to `None`; `RingDispatch` answers it from the new `Producer::capacity()` and the existing `Producer::free()`, in bytes; `ring.rs` also keeps the idle rule every writer thread shares — `Idle`, `IDLE_SPINS`, `IDLE_SLEEP`, public since [ADR-0181](../decisions/ADR-0181-a-journal-outside-the-engine-joins-the-engines-writer-bookkeeping-through-three-public-handles.md) so a journal outside this crate can wait the same way |
| `journal.rs` | `MemJournal`, `FileJournal`, `Reader`, `Store` — the resend store; also the two other handles a journal outside this crate joins the engine's shutdown bookkeeping through (ADR-0181): `WriterTicket`/`TicketState` (the retired-writer count `wait_for_retired_writers` waits on, and the separate `writers_retired()` count of retire acts) and `Releaser`/`Released::pair` (the release flag a `Recovery` answers `ready` from). `FileJournal` itself runs on all three, replacing what used to be private |
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
9. `transport/uring.rs` — a third transport, read last for the same reason as `tls.rs`, and
   after `wait.rs` (`NEEDS_REAPER`/`REAPS`, the const it and `Engine::new` check between them)

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
- `tests/writer_hooks.rs` — the three public handles a journal outside this crate joins to the
  engine's bookkeeping ([ADR-0181](../decisions/ADR-0181-a-journal-outside-the-engine-joins-the-engines-writer-bookkeeping-through-three-public-handles.md)),
  one test (or set of tests) per handle:
  - `journal::WriterTicket` / `TicketState` — `a_ticket_retired_twice_is_counted_once`,
    `finish_without_retire_changes_no_count`, `a_retire_after_the_writer_finished_counts_nothing`,
    `the_first_retire_is_counted_even_after_the_writer_finished` (the two counters, *Revision 2*'s
    fix — see [the trap page](../reference/push-then-retire-can-race-a-writers-finish-unless-finish-on-a-running-ticket-is-recorded.md)),
    `wait_for_retired_writers_waits_for_a_ticket_finished_on_another_thread`
  - `journal::Releaser` / `Released::pair` — `released_turns_true_only_when_its_releaser_releases`
  - `ring::Idle` / `IDLE_SPINS` / `IDLE_SLEEP` — `idle_is_reachable_from_outside_the_crate`

  `FileJournal` itself now runs on the same three handles, so the eight pre-existing engine test
  binaries this plan's step 1 left unmodified (`one_appender`, `after_serving`, `retire`,
  `writer_idle`, `journal`, `on_disk`, `engine_recovery`, `secrets_stay_off_disk`) are the proof
  the move changed nothing observable; `crates/store-sqlite`'s `writer.rs` is the worked example
  of a journal outside this crate using all three.
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
- `tests/uring.rs` (`--features io-uring`, Linux) — `transport/uring.rs`: red first (plan
  `docs/plans/2026-09-24-p4-io-uring-transport.md` step 1), byte-exact stress over real sockets,
  an ownership ledger checked after every reap (`Uring::buffers_accounted_for`), a residency
  check on the buffer memory, and a canary written into the buffer range after the ring is
  dropped (`unregistered_buffers_are_not_written_after_the_ring_is_dropped`; under ASan needs
  `ASAN_OPTIONS=quarantine_size_mb=0:thread_local_quarantine_size_kb=0` —
  [the trap](../reference/asans-quarantine-keeps-a-freed-mapping-from-being-re-occupied.md)). A
  real seccomp filter proves the blocked-startup refusal
  (`a_blocked_io_uring_refuses_to_start_and_names_seccomp`); `scripts/check-uring-refused-under-sysctl.sh`
  proves the sysctl half on the desk only. Machine-checked also by
  `scripts/check-no-kernel-sleep.sh`'s and `scripts/check-standard-gives-the-core-back.sh`'s
  `io_uring` arms (ADR-0190, ADR-0191)
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
