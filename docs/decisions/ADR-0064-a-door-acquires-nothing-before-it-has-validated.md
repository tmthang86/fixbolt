# ADR-0064 — A door acquires nothing before it has validated

**Status:** Proposed · **Date:** 2026-09-13 · **Plan:** docs/plans/2026-09-13-what-the-residue-review-found.md, step 2

## Context

ADR-0015 decision 6 reads: *"`ShardPlan::validate()` runs before a single thread is
spawned. Half a runtime that then refuses is worse than no runtime: it leaves threads to
join and sockets to close on an error path nobody exercises."* Its letter names threads.
Its reason names sockets too.

`Shards::start` honours the letter (`crates/engine/src/shard.rs:231`, *"ADR-0015 decision
6: before a single thread exists"*). The public door around it does not honour the reason:
`serve_sharded_hft_with` (`shard.rs:507`) calls `Acceptor::bind(addr)` first, opens one
log file per shard, and only then reaches `Shards::start`, where the plan is validated.
`[measured 2026-09-13]` by the senior review of PR #68, on a Linux desk: a port already
held plus `ShardPlan::new(vec![CoreId(4096)])` answers `Io(Os { code: 98, kind: AddrInUse })`.
The affinity refusal — the one that names what is wrong with the *plan*, which is the
half the operator can fix — is never reached.

`serve_hft_pinned`, added in the same PR (`crates/engine/src/lib.rs:2838-2839`), does
validate → pin → bind, and `crates/engine/tests/hft_pinned.rs` holds that order with a
held port as the witness. The two doors disagree about the same rule.

Prior art was read, not copied (non-negotiable 9). QuickFIX (C++) creates every session
from settings in `Acceptor::initialize()`, from the constructor, and binds only in
`onInitialize()` from `start()`/`block()`: a configuration error throws before any socket
exists. QuickFIX/J `AbstractSocketAcceptor.startAcceptingConnections()` runs
`createSessions(getSettings(), continueInitOnError)` — where settings are validated — and
`startSessionTimer()` before the loop that calls `ioAcceptor.bind(...)`. QuickFIX/n's
`ThreadedSocketAcceptor` constructor creates sessions from settings before any listener is
started in `Start()`. All three validate configuration before binding. None of them
rolls back a socket bound before a *later* bind fails, which is the same gap this ADR
leaves open below.

## Decision

1. **A `fixbolt_engine` entry point acquires no resource — no thread, no socket, no file —
   before every check it can make without acquiring one has passed.** Checks that read
   only the arguments (an empty `Table`) come first; checks that read the machine
   (`ShardPlan::validate()`, which reads `/sys`) come next; acquisitions come last. This is
   ADR-0015 decision 6 with its letter widened to match its reason. ADR-0015 is not
   edited (`CLAUDE.md` §5); this ADR amends it.
2. `serve_sharded_hft_with` moves `plan.validate()` to before `Acceptor::bind`. The
   `NoCounterparties` check stays first: it reads nothing but the arguments.
3. `Shards::start` keeps its own `validate()`. It is public on its own, its tests in
   `crates/engine/tests/shard.rs` guard it on its own, and a second read of `/sys` at
   startup is not on any hot path.
4. The order is held by a test of the same shape as `hft_pinned.rs`: a held port and a
   core the machine does not have, in the same call, and the refusal must be the core's.
   A refusal is only evidence of order when the bind would have failed too.

## Consequences

**Good**

- With a bad plan and a held port, the operator now reads the fault they can fix in the
  plan, not the one they must fix on the machine. A held port is usually the *previous*
  instance still shutting down; a wrong core is a configuration error that would have
  survived the retry.
- The two `hft` doors agree, and the rule is written once, here, rather than inferred from
  two functions that happen to match.
- No socket is bound during the milliseconds `/sys` is read. Small, but it is the exact
  window ADR-0015's reason was about.

**Bad**

- **This is an observable behaviour change on a public door.** A caller matching on
  `ShardError::Io` for the two-faults case now sees `ShardError::Affinity`. Nothing is
  released, so nothing breaks in the field, but `CHANGELOG.md` records it under
  *Changed* because a future reader comparing behaviour across commits deserves the line.
- The order *among acquisitions* is not decided here. Log files are still opened after
  the socket and before the threads; a bad log path leaves a socket bound for the length
  of one `open`. Both faults read `Io`, so no test can tell the orders apart by variant,
  and Rust drops the socket on the error return either way. Left open on purpose: opening a
  file is an acquisition, not a check, and this ADR is about the boundary between the two.
- Decision 1 is a rule in prose over every entry point, and only two doors have a test for
  it. The others (`serve`, `serve_hft`, `serve_tls`, the initiator doors) take no plan to
  validate today, so there is nothing to reorder — but a door that grows a machine-reading
  check later inherits this rule with no lint to remind it. `docs/GUIDE.md` names the rule
  where it lists what each door refuses.
