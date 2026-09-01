# Changelog

Notable changes to the published crates. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versioning will follow
[Semantic Versioning](https://semver.org/spec/v2.0.0.html) from the first release.

**Scope, stated so this file does not drift into being a second `STATUS.md`:** it records
changes to the **public API and observable behaviour of released crates**, and nothing else.
Decisions live in [docs/decisions/](docs/decisions/); where the work stands lives in
[STATUS.md](STATUS.md); what is about to be built lives in [docs/plans/](docs/plans/). A thing
that has not shipped does not belong here — `CLAUDE.md` §4: one rule, one place.

## [Unreleased]

**Nothing has been released.** Four crates now exist and none is published; the entries
below describe what a first release would contain.

### Added

- **`fixbolt-session`** — the FIX session state machine. Pure: no socket, no clock, no
  allocation, no `format!` on any path. Depends on `codec` and `dict`.
  - **`Session::resume(cfg, next_out, next_in)`, and `connect` no longer resets
    unconditionally** — [ADR-0010](docs/decisions/ADR-0010-a-reconnect-is-not-a-restart.md).
    FIX 4.4 numbers a session, not a connection, so a session that outlived its process keeps
    counting across the connection that follows; one built with `Session::new` has persisted
    nothing and still resets, which is what every `iCONNECT` in the acceptance corpus expects.
    `Session::next_out()` and `next_in()` are new, for an engine that must persist them. This
    layer still does no I/O: recovery is the engine's job and the numbers arrive as arguments.
    `[measured 2026-08-31]` **59/59 unchanged, with no corpus file exempted**; forcing `connect`
    to never reset drops the score to **56/59**, which is what proves the corpus exercises the
    branch.
  - **`Journal::mark_in(seq)` and `Journal::highest_in()`** —
    [ADR-0017](docs/decisions/ADR-0017-the-inbound-count-is-persisted-after-delivery.md). One
    file now carries both directions, so a resumed session takes `next_out` from the highest
    record and `next_in` from the highest mark. **Neither has a default implementation**, for
    the reason `highest` has none: a default would let a journal holding state report that it
    holds none, and a session resumed from it would silently start again at 1. `FileJournal`
    encodes a mark as a record of length zero — a FIX message is never zero bytes, so the format
    is unchanged and the reader is one branch longer.

    **The session writes the mark after the application has seen the message, not before**, and
    that ordering is the decision rather than a detail. Writing first loses the message on an
    ill-timed crash; writing after repeats it, and the repeat arrives with `43=Y`. **Your
    application must be idempotent per sequence number** — `GUIDE.md` §6a. Under
    `Durability::Fsync` the inbound path now pays a `sync_data` per message, which it did not
    before.
  - `Session<R: Role, N>` with `connect` / `disconnect` / `received` / `tick`, each taking an
    `emit` closure and returning `Link`. `Role` is a sealed marker — `Acceptor` and
    `Initiator` — so the two ends differ at compile time rather than on a branch per message.
  - `Config::acceptor(begin_string, sender_comp_id, target_comp_id)`. CompIDs are held inline;
    one too long for its buffer **fails closed** and matches nothing, rather than being
    compared on its first 32 bytes.
  - `clock::parse_utc` and `clock::MILLIS_YEAR_ZERO_TO_EPOCH` — `Tick` counts milliseconds
    from **0000-01-01**, not from 1970, so every year `SendingTime` can name is a non-negative
    `u64` and the skew cannot wrap. See `DESIGN.md` D13.
  - `journal::{Journal, NoJournal}` — the session no longer keeps the messages it has sent. It
    is handed a journal, exactly as it is handed an `Application`, and asks two questions:
    *keep `34=n`, these are its bytes*, and *do you still have `34=n`?*
    **`received_with` and `send_application` therefore take one more argument**, and `received`
    supplies `NoJournal` so a pure protocol machine is unchanged.
    [ADR-0008](docs/decisions/ADR-0008-journal-is-a-trait.md).
  - `text::SessionText` — the 17 expected `58=` values and their `373=` codes, rendered with
    no `format!` and no allocation. **Moved here from `fixbolt-conformance`**, where it lived
    only because the session did not exist yet.
  - Answers a Logon by echoing `98=` and `108=`, answers a Logout, and tracks sequence numbers
    in both directions. A message with `43=Y` and a sequence number already seen is dropped in
    silence; one without it ends the session with a Logout saying so.
  - `Reject (35=3)` with all twelve `SessionRejectReason` codes, driven by `fixbolt-dict`'s
    validation tables. Routing tags are reversed on the way back — `115` in becomes `128` out and
    the other way round. A CompID or SendingTime fault answers with a Reject **and** a Logout;
    the other ten leave the session running.
  - `Heartbeat (35=0)` and `TestRequest (35=1)`, on the session's own clock. Three thresholds,
    QuickFIX's: a heartbeat one `HeartBtInt` after this end last spoke, a test request 1.2
    intervals after the counterparty last did, the link at 2.4. A `TestRequest` is answered with
    the `112=` it carried; the one this session invents is the literal `TEST`, because the
    acceptance comparator reads tag 112 byte for byte.
  - Inbound `SequenceReset (35=4)`, gap fill and plain, and `ResetSeqNumFlag` on a Logon.
    **Whether a message's sequence number is checked, and whether it advances the count, is per
    `MsgType`** — a Logout is never checked and a `SequenceReset` never advances.
  - A frame the codec cannot read is now ignored rather than fatal, **unless it identifies
    itself as a Logon**. `MsgType` must be the third field; a message that puts it elsewhere is
    treated the same way.
  - `ResendRequest (35=2)` when a message runs ahead of the count. The message is **held**, not
    refused, and replayed in sequence order once the gap closes; the gap is asked for **once**,
    and a Logon that runs ahead is answered before it is asked for. Four held messages per
    connection, 512 bytes each — one that does not fit is dropped, which costs a round trip and
    never an allocation.
  - An inbound `ResendRequest` is answered with one `SequenceReset` gap fill. Every message this
    session has sent so far is administrative and QuickFIX never replays those. A store of
    application messages, and a real replay, are still to come.
  - An `Application` trait, and `received_with`. The session owns the seven administrative
    message types (`0 1 2 3 4 5 A`) and hands everything else over, lending the application the
    outbound sequence number, the clock, and a buffer to answer in. **Returning nothing spends
    no sequence number.** `received` keeps its signature and delivers to an application that
    never answers.
  - `PossDup (43=Y)` on a message behind the count is asked for `122=` **OrigSendingTime**, and
    refused if it claims to have been sent after it was resent. A `SequenceReset` is exempt from
    both.
  - A tag is read as a **signed** integer, as QuickFIX's tokeniser does: `-1=x` is a field and
    is Rejected, `4garbled9=x` is not a field and the message is ignored.
  - A journal of outbound application messages, and a real resend. A `ResendRequest` is answered
    by replaying each kept message at the number it was sent with — `43=Y`, a fresh `52=`, and
    the original `52=` carried as `122=` — and by covering each contiguous run that cannot be
    replayed with **one** `SequenceReset` gap fill. A replay spends no sequence number.
  - Scores **59 / 59** on the acceptance definitions. The session layer's gate is met.
  - `Config::initiator` and `Config::with_heart_bt_int`, and an initiator that **speaks
    first**: `connect` records whose turn it is and the next `tick` sends the Logon, because
    `connect` has no clock and a Logon carries a `52=`.
  - `Session::send_application` — an initiator has to be able to originate, and nothing on the
    wire asks it to. The session takes over the sequence number and the clock, keeps a copy for
    a later resend, and lets `Fix44` order the fields.
  - `[measured 2026-08-30]` **0 / 50** on the mirrored corpus. Every mirrored Logon is
    accepted; what the files ask for next is a message only an operator can order.

- **`fixbolt-engine`** — the crate that touches the socket. **All six steps.**
  - **`affinity` — pinning a thread to a core, and proving it happened.** New module, new
    optional feature of the same name, **off by default**, Linux only.
    [ADR-0015](docs/decisions/ADR-0015-explicit-cores-pinned-from-inside-and-read-back.md) and
    [ADR-0019](docs/decisions/ADR-0019-two-unsafe-blocks-and-an-error-the-enum-can-hold.md).
    - `CoreId(pub usize)` — a logical CPU as the kernel numbers it. **The caller names it; the
      engine never picks one**, because the OS's idea of a free core knows nothing about
      `isolcpus`, NIC interrupt placement or SMT siblings.
    - `pin_current_thread(CoreId) -> Result<(), AffinityError>` — call it **from inside the
      thread, as its first act**. It sets the affinity and then asks the kernel back with
      `sched_getaffinity`, returning `ReadbackMismatch` when the two disagree: a call returning
      `Ok` is not evidence.
    - `current_mask() -> Result<Vec<CoreId>, AffinityError>` and
      `running_on() -> Result<CoreId, AffinityError>`. The second reads the `processor` field of
      `/proc/thread-self/stat`, which the scheduler writes and this crate does not.
      `scaling_cur_freq` is deliberately not used — `[measured 2026-08-30]` it freezes on a
      `nohz_full` core, so a check built on it cannot fail.
    - `AffinityError` **carries the offending core**. Elsewhere in this workspace errors are
      fieldless; that rule is about hot paths, and this one is raised once at startup where
      `NotIsolated(cpu3)` tells an operator what to change and `NotIsolated` does not. It is
      `#[non_exhaustive]`: the topology rejections are not implemented yet.
    - `[measured 2026-08-31]` proven by reversal rather than by passing: with the
      `sched_setaffinity` call removed **and** the read-back disabled, the same thread was
      observed on **cpu0, cpu4 and cpu5** during one run of the residency test — so the test is
      not vacuous, and pinning is what stops the movement.
    - **The feature adds no dependency**: it reuses the `libc` that `standard` already made
      optional. `--no-default-features` still builds with no dependency and no `unsafe` at all,
      and that build is what proves the `#[cfg]` gates the `mod` and not just the manifest.
    - `Topology` and `ShardPlan` — **the engine refuses a plan it knows is wrong, before a
      single thread exists.** `ShardPlan::new(vec![CoreId(6), CoreId(7)])`, optionally
      `.with_journal_core(..)`, `.with_consumer_cores(..)`, `.allow_unisolated()`, then
      `validate()`. Four refusals, each naming the core: `NoSuchCore`, `NotOnline`,
      `SmtSiblingOf`, `DuplicateCore`, plus `NotIsolated` for shard cores unless waived, and
      `EmptyPlan`.
      - **Isolation is required of shard cores only.** A journal writer or ring consumer on an
        isolated core would be taking back the very core this design isolates. They are still
        checked for existence and for SMT contention with a shard.
      - `allow_unisolated()` **lifts exactly one rule.** An absent or offline core is still
        refused, and there is a test that says so, because an escape hatch that quietly becomes
        allow-anything is worse than no rule.
      - `Topology::from_sysfs(present, online, isolated, siblings)` is public so the refusals can
        be tested against a machine this one is not. `[measured 2026-08-31]` the §9 desktop reads
        `present 0-15`, `online 0-7`, `isolated 6-7,14-15` — **`isolated` names two cores that
        are offline**, because §9 turns SMT off. A validator reading `isolated` alone would
        accept a core that cannot run anything; that exact reading is committed as a fixture.
  - **`presession` — who is on the other end, before there is a session to ask.** New
    module, no feature gate: `Identity<'a>` (borrowed `49=` and `56=`, in wire order),
    `identity_of` and `is_logon`. It reads bytes by field scan and nothing else — no
    dictionary, no parse, and nothing from `fixbolt-session` but `Config`
    ([ADR-0020](docs/decisions/ADR-0020-a-pre-session-stage-owns-the-socket-until-logon.md)).
    Framing stays in `frame::Framer`; `Engine`'s own private `Logon` check is gone and calls
    this instead, so the rule has one home.
    - `[measured 2026-09-01]` proven against **289 real messages** from the acceptance
      corpus, asserting the identities it actually carries — including `49=WT`, `56=DLSI`
      and an empty `56=`, all of which the corpus sends on purpose — and the exact **five**
      that name no identity. The first draft of that test asserted every corpus `Logon` is
      `TW44`/`ISLD` and went red on the real bytes.
    - `[measured 2026-09-01]` and the corpus was **not enough**: two leniency reversals —
      matching the tag anywhere inside a field, and ignoring field boundaries entirely —
      left all 289 green, and were caught only by one hand-built message with `49=` inside
      a `Text` value.
      [reference/a-conformance-corpus-is-not-an-adversarial-one.md](docs/reference/a-conformance-corpus-is-not-an-adversarial-one.md).

  - **`shard` — many engines, one per pinned core.** New module behind the same `affinity`
    feature. `Shards::start(&plan, make)` validates the plan, starts one thread per shard, and
    **waits for every thread to confirm its own pin before any of them serves** — so a plan that
    fails on shard 3 does not leave shards 0 to 2 already taking connections. `make` runs on the
    pinned thread, so a connection's buffers are allocated by the core that will touch them.
    - `Shardable` — the three methods the runtime needs from an engine (`add`, `turn`, `idle`),
      so `Shards` carries none of `Engine`'s nine type parameters and a test can hand it
      something that is not an engine at all.
    - `Assign` and `RoundRobin` — which shard takes the next connection. An index outside the
      range is **refused**, not taken modulo: silently rewriting a caller's answer hides the bug.
    - `serve_sharded_hft` — bind, start, and hand connections over. The acceptor thread uses a
      **blocking** `accept` (new: `Acceptor::bind_blocking`, `Acceptor::accept_blocking`),
      because it is not an engine thread and spinning would burn a core to wait for something
      that happens once per session.
    - Dropping `Shards` ends every thread; there is no other shutdown.
    - `[measured 2026-08-31]` the channel makes **no syscall** (`try_recv`, two million calls)
      and **no allocation** (`benches/alloc.rs`, new `shard-turn` case, 0).
    - **Known defect, and it is why this is documented rather than recommended.** With more than
      one shard the single-logon rule stops working: an `Engine` serves one identity and answers
      *"already logged on"* by counting the other connections it holds, and sharding splits
      those across engines. The acceptance corpus scores **59 through one shard and 57 through
      two**. `STATUS.md` open item 24.
  - **The threads that are not engine threads now have homes too.**
    - `affinity::spawn_pinned(name, core, work)` — starts a thread that pins itself before doing
      anything and **does not return until that thread has confirmed it**, so a failed pin
      reaches the caller who can stop startup rather than dying quietly on the new thread. It
      returns the core the thread was *observed* on.
    - `FileJournal::open_pinned(path, Durability::Async, core)` and `writer_core()`. Only
      `Async` has a writer thread; asking to pin a `Fsync` journal is **refused** rather than
      accepted and ignored, because a constructor that silently drops an argument is how a
      deployment ends up believing it pinned something.
    - `Topology::siblings_of` is public: the engine never picks a core, so a caller has to, and
      `CoreId(0), CoreId(1)` is wrong on any machine with SMT on.
    - **The ring consumer is still yours to pin** — it is whatever thread calls `RingApp::pump`
      and this crate never spawns one. `ShardPlan::with_consumer_cores` validates the choice
      (a consumer sharing a physical core with a shard is refused before startup) and
      `spawn_pinned` is how you put it there.
  - **A full ring to the application ends the connection** —
    [ADR-0011](docs/decisions/ADR-0011-a-full-ring-disconnects.md), `DESIGN.md` D10b. Under
    `RingDispatch`, a message the ring will not take is one the session already accepted,
    numbered, journalled and acknowledged, that the application never saw. The engine now sends
    the counterparty a `Logout` reading **`58=slow application`** and drops the session, rather
    than incrementing a counter nobody reads. Three additions to the public API:
    - `Dispatch::take_refusal() -> bool`, **defaulted to `false`**, so every existing `Dispatch`
      implementation keeps compiling and `InlineDispatch` — which has nothing to refuse, its
      handler being on the engine thread — pays nothing: the branch folds away like
      `Dispatch::OUT_OF_BAND`. It carries no connection id because it needs none; the engine
      asks immediately after one connection's turn, and the adapter that reached `deliver` was
      built for that connection and nothing else ran in between.
    - `Engine::refused_connections()`, the same events counted from the engine's side.
    - `ring::DEFAULT_CAPACITY`, **4 MiB**, and `backpressure::SLOW_APPLICATION`. The text is
      deliberately not D10's `slow consumer`: on the wire the counterparty is at fault, here it
      is faultless and we are the ones who stopped reading. **The benchmarks stay at `1 << 16`**
      so that `DESIGN.md` §6's recorded baselines still compare against what produced them.

    Two costs, stated rather than found later: 4 MiB resident per ring, and an application that
    pauses longer than the ring holds now drops the session instead of lagging.
    `[measured 2026-08-31]` 4 MiB gives **5.05–5.36 ms** of slack against **47.7 µs** at the old
    64 KiB — but **no real application has ever stalled against this ring**. That figure is
    measured at 4 MiB rather than extrapolated from 64 KiB, and the two differ by 48%: see
    `docs/reference/measured-costs.md`.

    One behaviour an embedder must know and the compiler cannot enforce: **the `Logout` is
    queued on the turn the refusal happens and goes out on the next flush**, exactly as D10's
    path does. Stop turning the engine when a connection looks doomed and it is never sent.
    `GUIDE.md` §4.
  - `Transport`, with `TcpTransport` (non-blocking, `TCP_NODELAY`) and `Loopback` (in memory,
    for tests that must not depend on a free port).
  - **`Io::{Ready, Idle, Closed, Failed}` rather than `io::Result<usize>`.** On a stream socket
    `Ok(0)` already means end-of-stream, so reporting `WouldBlock` the same way hands the caller
    one value for two opposite facts — a session dropped because the counterparty was quiet, or
    a loop spinning forever on a socket that closed. Same answer the codec reached for
    `Parsed::Incomplete`.
  - `Waiting`, with `Spin` (`hft`, D8) and `Yield` (every test in this repository).
    **`Waiting::idle` takes the source list**, and `Waiting::NEEDS_SOURCES` says whether a
    strategy actually needs it — blocking on readiness requires knowing the sockets, and
    splitting idling across two traits to express that would have been worse than showing the
    sources to a waiter that ignores them. `Spin` drops the slice and the call disappears at
    `-O2`. [ADR-0014](docs/decisions/ADR-0014-standard-mode-blocks-on-poll.md) decision 3.
  - `Transport::POLLABLE` and `Transport::source()`, with `Source` and `Interest`.
    `source()` has a default body returning `None`, so a transport written elsewhere keeps
    compiling; what it cannot do is join a `standard` engine. `Source::from_raw_fd` is public
    because without it no transport outside this crate could ever be pollable — **and it
    borrows rather than owns**: a `Source` outliving its socket silently starts naming whichever
    socket the kernel gave that number to next.
  - `poll::{Poller, PollError, Ready}`, behind the **`standard` feature (on by default)** and
    `cfg(unix)`. The crate's first external dependency (`libc`) and first `unsafe`, both
    arriving behind that feature: `--no-default-features` still builds an engine with **no
    dependency and no `unsafe` at all**. `POLLNVAL` is `PollError::BadSource`, never counted as
    readiness — `poll(2)` includes it in its return value, so trusting that number would report
    an unknown descriptor as a ready one.
  - `block::Block` — `standard` mode's idle turn, behind the same feature and `cfg(unix)`.
    Blocks on readiness with a **100 ms default timeout**, floored at 5 ms because a timeout
    short enough to be indistinguishable from a spin defeats the mode. The timeout is a
    correctness parameter rather than a knob: `Session` takes no clock, so in `standard` it is
    the coarsest grain of time the session can see. `EINTR` goes back and waits out **what is
    left**, never the full timeout again. A `poll` that fails is recorded in `Block::last_error`
    **and still gives the core back** — an error `idle` cannot return must at least be
    observable.
  - `Engine::refresh_interests`, `refresh_interests_with` and `idle_with` — the list of sources
    an idle turn waits on. **Readable always; writable only while that connection still has
    bytes queued**, because a socket is almost always ready to accept bytes and asking
    unconditionally would wake the engine continuously and turn `standard` back into a spin.
    Rebuilt every turn rather than cached: a `Source` borrows a descriptor, and one kept across
    a turn can name a socket that has since closed and been reissued. For a strategy that does
    not need the sources the whole rebuild compiles away.
  - `Acceptor::source()`, and `serve` now hands the listener to `idle_with`. Without it a new
    connection is accepted on the next timeout rather than on the connect.
  - `Engine::sources_missing()` — connections that claimed to be pollable and produced no
    source. Zero on a healthy engine; anything else is traffic arriving one timeout late, and
    the count is the only thing that would say so.
  - `waker::{Waker, WakeHandle}`, `Engine::with_waker` and `RingApp::with_waker` — a self-pipe,
    so a thread that is not the engine can say *look again*. `poll` wakes for descriptors, not
    for a ring buffer, so without it a reply produced by `RingApp` waits out the engine's whole
    timeout. The engine puts the read end in its **own** poll set and **drains it after every
    wait**: a pipe holding an unread byte stays readable, so an undrained one makes every
    subsequent `poll` return instantly and turns `standard` back into a spin. `wake()` never
    blocks, and a write refused because the pipe is full is not lost work — a full pipe is
    already readable, which is the entire signal.
  - **A `WakeHandle` outliving its `Waker` is safe.** Both pipe ends are held jointly, so
    dropping the engine while an application thread still holds a handle cannot leave a write
    without a reader. `[measured 2026-08-30]` before this it raised `SIGPIPE` and killed the
    process — invisible from an ordinary Rust binary, because the runtime sets `SIG_IGN` before
    `main`, and a library cannot assume its host does the same.
  - **Pairing `Block` with a transport that cannot be waited on does not compile.** `Loopback`
    is the case that matters: an engine there would answer every message, pass all 59
    definitions, read 0% CPU, and be 100 ms slower per message. A `compile_fail,E0080` doctest
    on `Block` keeps it refused.
  - `Park` is renamed **`Yield`**. It is neither mode and its rustdoc says so: it fails the
    `hft` gate (`sched_yield`) and fails the `standard` gate (it burns the core). Nothing about
    its behaviour changed.
  - `Framer<N>` — one fixed buffer per connection, no allocation, no parsing. It reads one
    field, `9=`, and answers `Cut::{Message, Garbage, Need}`. **Rubbish is handed to the session
    once** rather than dropped, so "a bad frame is fatal only if it claims to be a Logon" stays
    in `fixbolt-session` and is not duplicated. A message bigger than the buffer is `Garbage`
    too: a buffer that fills and never empties is a connection wedged by a number the
    counterparty chose.
  - `crates/session/tests/score.rs` now calls that framer instead of keeping its own copy, and
    still scores **59 / 59**.
  - `Clock`, with `SystemClock` and `ManualClock`. A seam that exists for the corpus rather
    than for tidiness: every `I` line carries a fixed instant, so an engine wired to the wall
    clock cannot be driven by the acceptance files at all.
  - `Connection<T, R, N, RX, TX>` — one socket, one framer, one session, and an outbound
    queue of `TX` bytes. `Turn::{Up(bool), Gone}` says whether anything moved.
  - `Engine<T, R, A, C, W, N, RX, TX>` — `turn()` is one non-blocking pass over every
    connection; `run()` is `loop { if !turn() { wait.idle() } }` and nothing else. It reads
    **once** per connection per turn, so a counterparty that writes faster than this end
    processes cannot starve the others.
  - `Acceptor::{bind, local_addr, accept}`, `connect(addr)`, and `serve(addr, cfg, app,
    capacity)` — the whole loop written once.
  - **`serve` is `standard` and blocks; `serve_hft` spins.** `[2026-08-30]` `serve` used to
    spin. An engine whose out-of-the-box configuration pins a core at 100% is one most people
    cannot evaluate — it looks broken — so ADR-0013 reversed the default and this is where the
    reversal lands. `TcpAcceptorEngine<A, W>` is now parameterised by the mode, with
    `HftAcceptorEngine` and `StandardAcceptorEngine` naming the two, and both `serve` functions
    run **one** shared loop so they can differ in exactly one type and nothing else.
  - **The 59 acceptance definitions pass in both modes**: the same corpus, once with the engine
    yielding and once with it blocking between steps, 59 / 59 each way. ADR-0013 named this cost
    when it accepted two modes.
  - **The 59 acceptance definitions now pass through a real socket**: `cargo test -p
    fixbolt-engine --test wire` → **59 / 59**, kernel TCP, no background thread and no sleep.
  - `Dispatch`, with `InlineDispatch<H>` (the default, D4 / ADR-0002) and `RingDispatch<M>`
    plus its `RingApp<M>` on the far side. The trait carries `const OUT_OF_BAND: bool`, so the
    engine's out-of-band collection compiles away entirely on the inline engine.
  - **A reply is routed by `ConnId`, never by index.** `swap_remove` reuses indices the moment
    anything hangs up; a reply for a connection that has gone is dropped.
  - `ring::pair(capacity)` — a single-producer single-consumer **record** queue built from
    `Box<[AtomicU8]>`: no `unsafe`, no dependency, allocated once. A push is one whole message
    or none, and a record too long for the reader is dropped rather than wedging the queue.
    [ADR-0007](docs/decisions/ADR-0007-spsc-ring-without-unsafe.md) carries the price.
  - `Engine::new` now takes a `Dispatch` rather than an `Application`; `serve` still takes an
    application and wraps it inline, so the deployment shape is unchanged. `application()`
    became `dispatch_mut()`.
  - `benches/dispatch.rs`: inline **2.7 ns**, ring **128.0 ns** one way, **242.5 ns** round
    trip, each asserting its own ceiling.
  - `Backpressure` (D10), on an engine or on one connection: `Disconnect` (the default),
    `Queue { max_bytes }`, `Block`. A message is queued whole or refused whole; a refusal ends
    the session with `Logout(58=slow consumer)`, written after the queue is discarded so that
    a bound smaller than one Logout cannot end a session in silence.
  - **`Connection::turn` now ticks before it reads.** `Session::received_with` has no clock
    (D1) and judges `SendingTime` against the last tick, so a session that has never ticked
    refused the first message on every connection. The wire gate is unchanged at 59 / 59.
  - A connection whose socket has died is finished even with bytes queued. It previously
    stayed up for as long as it was turned.
  - `fixbolt-session`: `Session::logout_now(text, emit)` — send a `Logout` carrying `58=text`
    and give up the link. Additive, for the engine's D10 policy; the session cannot see a full
    queue and the engine cannot build a message.
  - `journal::{MemJournal, FileJournal, Durability, Store}` (D7). `FileJournal` appends to a
    file — `Fsync` writes and syncs inline, `Async` hands the bytes to a writer thread over the
    ADR-0007 ring — and answers a resend from its in-memory ring rather than from disk, because
    reading it back would be a blocking `read` on the engine thread.
    [ADR-0008](docs/decisions/ADR-0008-journal-is-a-trait.md).
  - `Engine::add_with_journal`, for a journal the caller built. `Engine::add` uses
    `J::default()`.
  - `benches/alloc.rs`: **0** on seven paths — idle, send, receive, framing, an idle turn, a
    turn carrying a message in and a reply out, and a full ring round trip.

- **`fixbolt-conformance`** — the mirrored corpus, for the initiator role.
  - `script::scenarios_mirrored()` reads the same 59 files from the other side: `I` lines
    become what this engine must send, `E` lines become what arrives, and `iDISCONNECT` /
    `eDISCONNECT` swap. An `I` line's `<TIME>` grows to 21 bytes with it, because mirrored it
    is **this engine** that writes it and this engine writes milliseconds.
  - `script::mirrors()` applies `ADR-0004` decision 6 as amended by `ADR-0006` rather than
    quoting it, and a test asserts the set it computes equals the nine names. **50 of the 59
    mirror**, not the 51 the ADR first said: `1b_DuplicateIdentity.def` mirrors into this
    engine hanging up a connection nothing told it to.
  - `runner::run_mirrored()` scores out of 50. `NullSession` scores **0 / 50** and `Replay`
    scores **50 / 50**, which is what makes a real score mean something.

- **`fixbolt-dict`** — four validation tables, generated from `FIX44.xml`, answering the
  dictionary half of `Reject (35=3)`.
  - `Fix44::is_defined_tag(tag)` — a bitset over 0..=956. **No user-defined range**: QuickFIX's
    own header calls 5000..=9999 user-defined and the acceptance corpus expects `5000=HI`
    refused anyway. Answers `373=0`.
  - `Fix44::is_msg_type(msg_type)` — the 93 FIX 4.4 message types. `required()` could not answer
    this: it gives `&[]` for an unknown type and for a known one with no required fields alike.
    Answers `373=11`.
  - `Fix44::allows(msg_type, tag)` — one 120-byte bitset per message, header and trailer folded
    in so a caller asks once rather than three times. 12 524 (message, tag) pairs. Answers
    `373=2`.
  - `Fix44::field_type(tag)` and `FieldType::accepts(value)` — the 23 FIX 4.4 types and what each
    takes on the wire. `FieldType` is the one place the XML type names map to behaviour;
    `build.rs` includes the same file by path rather than restating it. Answers `373=6`.
  - `Fix44::required_header()` — the 7 header fields every message must carry. `required()`
    answers for a message body and says so in its own doc; this is the other half, and
    `14b_RequiredFieldMissing.def` needs both.
  - `Fix44::enum_allows(tag, value)` — 245 enumerated fields, 1 708 values, 98 distinct lists
    after deduplication. `None` means *not enumerated*, never *fine*. Answers `373=5`.
  - `SEQNUM` accepts `0`. It did not, on a rule this project invented and an invented test
    that agreed with it. `11a`, `11b` and `11c` all send `34=0` and QuickFIX processes them —
    the restored rule costs three files. See `docs/reference/fix44-dictionary-traps.md`.
  - Roughly **33 KB** of static data; the build script's run time is unchanged at under a second.
  - **A field type the enum does not know stops the build.** Falling through to `STRING` would
    make `373=6` blind to a whole type, and no acceptance definition would notice.

- **`fixbolt-conformance`** — the 59 QuickFIX FIX 4.4 acceptance definitions, run in process
  with no socket. Zero runtime dependencies. Not published: it is a measuring instrument.
  - `script` — the corpus as 669 typed steps. Refuses to skip a directive it cannot read.
  - `compare` — `Comparator.rb`'s positional rules, with the five loosely-matched tags read
    out of `fields.fmt` rather than hard-coded.
  - `runner` — `SessionUnderTest`, keyed by connection so `1b_DuplicateIdentity` is
    expressible; `NullSession` scores **0 / 59** and `Replay` scores **59 / 59**.
  - The harness clock moves forward, and only when the file is waiting: before matching an `E`
    line the session has not answered, it advances one `HeartBtInt` — the file's own, from its
    Logon — and retries, at most three times. `[measured]` 33 of the 250 `E` lines have no `I`
    line in front of them, and that absence is the only "wait" the `.def` grammar has.
  - `echo` — the echo application the corpus assumes. All 22 application pairs reproduced.

- **`fixbolt-codec`** — FIX 4.4 read and write, `no_std`, zero runtime dependencies.
  - `parse_into::<D, N>(buf, &mut idx, validation) -> Result<Parsed, ParseError>`. `Incomplete`
    is an `Ok`, because TCP delivers bytes and not messages.
  - `FieldIndex<N>` and `MessageView<'_, N>` — the caller owns the index and reuses it;
    the view is 24 bytes and `Copy`.
  - `Template<P, S>` and `TemplateBuilder` — a message skeleton that sorts its fields once, at
    build time, and fills holes at send time. Optional slots may be omitted.
  - `TimestampCache` — `SendingTime` with the minute prefix cached.
  - `MessageView::group` / `GroupIter` / `GroupEntry` — repeating groups **read** off the
    flat index, nested to the 4 levels FIX 4.4 reaches. No allocation: an iterator is a pair
    of positions into the index the parser already filled. `GroupIter` is an `Iterator`.
    `declared()` (what the counter says, `Option` — a non-numeric count is not a count) and
    `counted()` (what is on the wire) are reported separately and never reconciled here.
    `GroupIter` yields `GroupEntry`, which can itself be descended into.
  - `TemplateBuilder::group(counter)` and `Template::encode_with::<D>` — repeating groups
    **written**. `GroupData` / `GroupEntryData` are borrowed and recursive, so nesting costs
    no allocation. Field order inside an entry comes from `D::group_order`, never from the
    order the caller supplied: inside a group the order is not ascending by tag, so the rule
    that governs the body cannot catch a mistake there. The counter's value is
    `entries.len()`, so the count and the entries cannot disagree.
  - `EncodeError` gains `UnknownGroup`, `NotAGroupMember`, `MissingDelimiter`,
    `MsgTypeMissing` and `GroupTooDeep`.
  - `Dictionary` trait. `is_header` and `data_length_tag` are answered by `fixbolt-dict`;
    of the three repeating-group methods, all three are now implemented there — reading and
    writing groups is not, and lands with the rest of the repeating-groups plan.
- **`fixbolt-dict`** — 912 tag constants, 93 message types, `is_header`, `data_length_tag`,
  `required` and the group tables, all generated from `FIX44.xml` at build time.
  - `group_members(msg_type, counter)` — one table serving `group_delimiter` (its head) and
    `group_order` (itself), so the three cannot disagree. Keyed by **`(msg_type, counter)`**:
    four counters take a different delimiter in different messages.
  - `GROUP_COUNTERS = 59`, `GROUP_POSITIONS = 731`, and `GROUP_KEYS` — every declared
    `(msg_type, counter)` pair, so a caller can enumerate the groups rather than name them.

### Changed

- **`fixbolt-conformance::script`** — `<TIME>` now substitutes a **real instant**
  (`20260828-12:00:00`, and `…​.000` on `E` lines) instead of `00000000-00:00:00`. The old
  value is the corpus's placeholder for output the comparator never reads by value, and it is
  not a date at all — month 00, day 00 — so no `SendingTime` check could be written against it.
  `FIXED_TIME_IN` and `FIXED_TIME_OUT` keep their names and their two widths; `FIXED_TIME_MILLIS`
  is new, and is what the runner ticks with.
- **`<TIME±N>` is now real arithmetic.** With the base at midnight of year zero there was
  nowhere to go backwards to, so the offset wrapped: `<TIME-121>` came out 86 279 seconds
  *forward*, in the one file that exists to test `SendingTime` accuracy.
- **`fixbolt-conformance::text` moved to `fixbolt-session::text`.** The table describes what a
  session says, and it lived in `conformance` only because no session crate existed. `codec`'s
  allocation bench loses its `text` case, which reappears in `session`'s.
- **`scripts/fetch-quickfix-assets.sh` fetches four more QuickFIX headers** — `FixFieldNumbers.h`,
  `FixFields.h`, `FixCommonFields.h` and `FixValues.h` — read as oracles, never copied. `vendor/`
  stays gitignored (ADR-0001).
- **`Journal` gains `highest()`, and it has no default implementation.** Every implementation
  must answer, because a default `None` would let a journal that holds messages report that it
  holds none and a resumed session would silently start again at 1.
- **`FileJournal`'s on-disk record gains a length**: `seq(4) || len(4) || bytes` instead of
  `seq(4) || bytes`. Without it records could not be separated on read, so the file was
  append-only by construction. `FileJournal::open` now reads the file back before appending,
  and drops a torn trailing record rather than half-reading it.
- **`TemplateBuilder::build` enforces the DATA invariant, so it can now fail where it did not.**
  A DATA field declared without the length field that must sit immediately in front of it
  returns the new `EncodeError::DataWithoutLength(tag)` — at build, once, rather than emitting
  bytes no reader can frame on every message. Inside a repeating group the same refusal comes
  from `encode_with`, before anything is written.
- **The encoder writes a DATA field's length itself.** A value supplied by the caller for a
  length field is ignored, and the byte count of the data — embedded `0x01` included — is
  written instead. If the caller could state it, the invariant would be advice: one wrong
  number and every reader mis-frames the message after it.
- **Field order inside a message now places a DATA field immediately behind its length field**
  rather than at its own tag's ascending position. `[measured 2026-08-30]` fifteen of FIX 4.4's
  sixteen DATA pairs have `length == data - 1`, so ascending order was right by luck;
  `Signature(89)` takes `SignatureLength(93)` and was emitted **before** its length.
- **`runner::run_scenario` seeds the clock**, sending `Input::Tick` before the connect and
  before every message. A session has no clock, so the harness is its clock. The value is fixed;
  advancing it is the heartbeat rule and belongs to a later step.

### Known limitations, stated rather than discovered

- `required()` does not descend into `<component>`, so it is wrong for 21 of the 93 message
  types. Nothing calls it. Component recursion arrives with the repeating-groups plan.
- The trailer tags — `Signature(89)`, `SignatureLength(93)`, `CheckSum(10)` — are classified as
  neither header nor body, so a written `Signature` sorts into the body. It is now at least
  emitted **after** its length field, which it was not before 2026-08-30.
- **Nothing on the DATA write path is backed by real data.** No `.def` file in the corpus
  carries a DATA message, so every frame in `tests/data_encode.rs` is built to the FIX 4.4
  specification. Two implementations agreeing about a spec is not a counterparty accepting the
  bytes.
- `[measured 2026-08-31]` Encoding an `ExecutionReport` costs **239.1 ns** on the §9 desktop
  (AMD Ryzen 7 3700X, median of 24 qualifying `scripts/bench.sh` runs), 93.8 ns on an Apple M5, and
  177.6–199.4 ns on a shared Xeon container. **There is no absolute published target any
  more**: [ADR-0016](docs/decisions/ADR-0016-per-machine-baselines-replace-absolute-targets.md)
  withdrew the 60 ns figure — it described other engines rather than this one — and the gate
  is now a per-machine baseline in `benches/baselines.tsv`. Asking how fast this is now
  requires naming a machine, which is the cost that decision accepted.

Crate names settled before any publish: they are `fixbolt-*` as of 2026-08-30.

**The crate names are settled.** `nanofix-*` became `fixbolt-*` on 2026-08-30, before any
publish, because the old prefix sat one word from `matthart1983/nanofix` — the reference
project this repository measures itself against. `fixbolt` was checked free on crates.io
**and** GitHub; the rejected candidates and the reasons are in [STATUS.md](STATUS.md) item 1.
Renaming after a crates.io publish is not possible, so it happened before.
