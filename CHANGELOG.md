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

**Nothing has been released.** Six crates now exist and none is published; the entries
below describe what a first release would contain.

### Added

- **Two deadlines a caller can state, and two reasons for them.**
  **`Config::with_logon_timeout_ms`** and **`Config::with_logout_timeout_ms`** — QuickFIX's
  `LogonTimeout` and `LogoutTimeout` — with **`DropReason::LogonTimedOut`** and
  **`DropReason::LogoutTimedOut`** to say which one fired. Step 2 of `STATUS.md` item 45,
  wave B. Both are **off by default**, so no existing session changes.

  The logon deadline is the **initiator's**: an acceptor has `presession::Limits` holding the
  socket before a `Session` exists, and an initiator dialling a venue that accepts the
  connection and then says nothing has no such stage. The logout deadline bounds a wait that
  nothing bounded: `begin_logout` leaves the link up so the caller can hear the answer, and
  until now the only limit was 2.4 × `HeartBtInt` — 72 seconds on a default session, during
  which a venue that took the goodbye and died holds the socket open.

  Both are measured from the first `Session::tick` after the event they bound, because a pure
  session layer is given time in no other way. Both end the connection **without sending
  anything**: there is no agreed session to speak on before a Logon, and the goodbye has
  already gone out after one.

- **A session can be told to restart its numbering.** **`fixbolt::ResetPolicy`** — QuickFIX's
  `ResetOnLogon`, `ResetOnLogout` and `ResetOnDisconnect`, under those names — reached through
  **`Config::with_reset`** and read back through `Config::reset`. Step 1 of `STATUS.md` item 45,
  wave B.

  Until now the only thing that could restart a resumed session's counts was the counterparty
  sending `141=Y`. `Session::new` versus `Session::resume` looks like the same choice and is
  not: those say **what the journal still holds**, and a reset policy says **what this session
  wants next time**. Collapsing them would leave `ResetOnLogon=Y` unrepresentable for exactly
  the session that needs it — one that was resumed from a journal holding 500 messages, whose
  counterparty starts each morning at `34=1`.

  **The default resets on nothing**, which is the behaviour the 59 acceptance definitions
  prove, so no existing session changes. `on_logon` restarts both counts in `Session::connect`;
  `on_logout` and `on_disconnect` restart them in the one place that knows which ending
  happened, **after** the message that ends the session has been written — a `Logout` still
  carries the number it was owed rather than spending `34=1` twice.

  The file keys that set these arrive with step 4; this is the API underneath them.

- **The handles reach the front door.** **`fixbolt::Handles`** — one cell, made by the caller
  **before** the engine, with `observer()`, `admin()` and `sender()` on it — plus
  **`Engine::adopt(&Handles) -> bool`** and **`Engine::logons() -> u64`**
  ([ADR-0054](docs/decisions/ADR-0054-the-handles-are-made-before-the-engine-and-the-engine-adopts-them.md),
  `STATUS.md` item 47).

  **What it fixes.** `Engine::observer`, `admin` and `sender` all need a `&mut Engine`, and a
  caller who came through `serve` never holds one: the engine is built inside the function and
  only a `Shutdown` comes back, after everything has ended. So an engine reached through the
  front door could not be watched, administered, sent through, **or stopped** — and
  `docs/GETTING-STARTED.md` had promised since 2026-09-03 that *"`serve` returns when an
  operator stops the engine through `Admin::shutdown`"*. `[measured 2026-09-05]` that sentence
  now executes:
  `crates/library/tests/end_to_end.rs::an_operator_stops_the_front_door_and_serve_comes_back`
  reads `sessions 1, said_goodbye 1, acked 1`, and `scripts/interop.sh` stops its acceptor role
  by asking rather than by `kill`.

- **A journal that knows how far it has counted.** `Journal` gains **`mark_out(seq)`** — a
  high-water mark, empty default body — and **`highest_out() -> Option<u32>`**, no default;
  `Resumed::from_journal(journal) -> Option<Resumed<J>>` computes `next_out`, `next_in` and
  `last_active_ms` from them; `Record::OutboundMark { seq }` is the fourth record shape a
  journal file can hold; and `Session` gains `logout_now_with`, `begin_logout_with` and
  `send_sequence_reset_with`, which are the existing three with a journal to tell.
  ([ADR-0053](docs/decisions/ADR-0053-the-journal-answers-two-questions-and-the-second-is-a-number.md),
  `STATUS.md` item 48.)

  **What it fixes.** `Journal::highest()` answers *what can be replayed*, and every worked
  example in this repository read it as *how far the outbound count has got*. The two differ by
  every administrative message sent since the last application one — a `Logon`, a `Heartbeat`
  and a `Logout` each spend a `34=` no journal holds bytes for. `[measured 2026-09-05]` one
  clean logout was enough: a real `libquickfix` refused the resumed session with `MsgSeqNum too
  low, expecting 4 but received 3`.

  **`FileJournal` writes one more record**, `seq == 0 && len == 4`, and **the format stays
  v1** — `34=0` is not a sequence number FIX can produce, which is the same escape the inbound
  and activity marks use. The ADR records that this is the last shape that gets it. A file
  written before this reads exactly as it did, and is short by exactly as much as it was.

- **`fixbolt_session::validate(view, msg_type) -> Option<SessionText>`** — the dictionary pass
  the session runs on every inbound message, callable without a session
  ([ADR-0050](docs/decisions/ADR-0050-the-dictionary-pass-is-public-so-it-can-be-timed.md),
  `DESIGN.md` §6 and §8). Answers "what would the session fault this message for": the
  wire-order field scan, then the required tags, then the group counters, first fault wins.
  It looks at **none** of sequence numbers, CompIDs, `SendingTime` or session state, and it
  does not return the `371=` tag reference. It exists because `STATUS.md` open item 39 could
  not be measured without it — `[measured 2026-09-05]` **897.3 ns** on a `NewOrderSingle` and
  **218.4 ns** on a `TestRequest`, §9 desktop, about seven times the parse that precedes it.

- **An application can speak first.** Two new doors, and before them every application message
  this engine could send was a *reply*
  ([ADR-0048](docs/decisions/ADR-0048-an-engine-that-can-speak-first-has-two-doors.md),
  `DESIGN.md` D15, `GUIDE.md` §8d).
  - `fixbolt::Handler::on_logon(&mut self, who: Peer<'_>, nth: u32, reply: Reply<'_, P, S>)
    -> Answer` — **a default body that says nothing**, so every existing handler compiles
    unchanged. Asked `nth = 0, 1, 2, …` on the engine thread until it answers
    `reply.silent()`, bounded by `fixbolt::MAX_ON_LOGON` (16).
  - `fixbolt_engine::Engine::sender() -> fixbolt::Sender` — `Send + Sync + Clone`, rides the
    `Arc` that `Observer` and `Admin` already ride. `send(id, msg) -> bool`; `false` means
    nothing was taken, because the queue was full (`ORIGIN_CAPACITY`, 64) or the message was
    empty or over `ORIGIN_LEN` (512).
  - `fixbolt::Reply::originate(begin_string, sender, target, out)` — a `Reply` with no
    sequence number and no stamp, because the session writes both on the way out.
  - `fixbolt_session::Peer<'_>` — `begin_string`, `sender`, `target` for the session being
    spoken on; re-exported as `fixbolt::Peer`.
  - New `EventKind` variants `SpokeFirstToTheBound { sent }` and
    `OriginationUndeliverable { count }`, and `Engine::speak_first_sends()`.
- **`fixbolt_session::Application::on_logon`**, with a default body returning `None`.
  Nothing in `fixbolt-session` calls it — the engine does, at the turn a session comes up —
  and it is declared there because that is where the application seam is declared. Adding it
  breaks no existing implementation.
- **`fixbolt_session::Config::begin_string()`, `sender_comp_id()`, `target_comp_id()`** — the
  first way to *read* a configured identity rather than test one.
- **The four buffer sizes are the caller's, through the front door.** Every serving entry point
  has a `*_with` twin taking `N`, `RX`, `TX` and `APP` as const parameters —
  `serve_with`, `serve_hft_with`, `serve_with_recovery_with`, `serve_hft_with_recovery_with`,
  `connect_and_serve_with`, `shard::serve_sharded_hft_with`. The originals keep their exact
  signatures and delegate with `256, 4096, 8192, 1024`, so **no existing call changes**.

  Before this the sizes were literals inside type aliases and `Engine` was not re-exported from
  `fixbolt`, so a deployment whose counterparty sends messages larger than 4 KiB had to fork the
  serving loop. `docs/CONFIGURATION.md` had been telling readers to *"instantiate `Engine<...>`
  directly"*, which that audience could not do.

  `APP` is new as a name: the scratch an `Application` lays one reply out in was a hard-coded
  `[u8; 1024]` in the session layer — the tightest ceiling in the engine, and the only one that
  failed as **silence**, because an application that cannot lay out its reply returns `None` and
  that is a legal answer. `fixbolt_session::DEFAULT_APP_SCRATCH` names the default.

- **A two-directional message log.** `msglog::{MessageLog, NoLog, FileLog, MaybeLog, Direction}`
  and the `FileLogPath` settings key. One text file, one line per message, **both directions
  including frames refused before the session saw them** — the class the journal cannot hold,
  because its key is `seq` and those frames never got one. Written by a thread that is not the
  engine's, through the ring `journal` already uses; a full ring drops and counts rather than
  blocking the engine. `DESIGN.md` D14, `GUIDE.md` §6c.
- **`Snapshot::log_lost`, `EventKind::MessageLogLost` and `EventKind::MessageLogUnsent`.** The
  first two are records the log never wrote; the third is bytes the log called `OUT` that the
  socket never took, because a line is written when a message reaches the outbound queue and a
  dying socket discards that queue.
- **The journal's on-disk format is version 1: a `FXBJ\x01` header and a CRC32 after every
  record.** A record whose checksum does not match is treated exactly as a torn tail — the read
  stops there, everything before it stands, and `FileJournal::corrupt_records` /
  `Reader::corrupt_records` say so; `jrnl` warns and exits 2. **A file without the header is
  version 0, read exactly as before, and stays version 0 as it is appended to.** Note at the
  end of [ADR-0008](docs/decisions/ADR-0008-journal-is-a-trait.md).
- **`Settings::log`**, and `Problem::SessionOnly` for a `FileLogPath` found in a `[SESSION]`
  block. One engine writes one file.
- **`Engine::with_log`, `Engine::with_shard`, `Engine::shard`, `Engine::log_mut`** and
  `Connection::with_shard`, `Connection::unsent_bytes`.

### Changed

- **BREAKING — all ten `serve*` / `connect_and_serve*` functions take `handles: Handles` as
  their last parameter.** `serve`, `serve_with`, `serve_with_recovery`,
  `serve_with_recovery_with`, `serve_hft`, `serve_hft_with`, `serve_hft_with_recovery`,
  `serve_hft_with_recovery_with`, `connect_and_serve`, `connect_and_serve_with`. This
  contradicts [ADR-0047](docs/decisions/ADR-0047-the-four-buffer-sizes-are-the-callers-through-a-second-function.md)
  decision 2 — *"the originals keep their exact signatures"* — deliberately and in part; its
  other four decisions stand and it is **not** superseded. The alternative was six more twin
  functions, sixteen for one idea. `shard::serve_sharded_hft` is **not** in this list and is
  named as an open question in ADR-0054: N shards are N cells, and a `ConnId` is unique only
  within one.
- **`connect_and_serve` no longer drains the caller's event ring.** `Observer::events` drains,
  so two readers on one cell share events rather than each seeing them — and the reconnect loop
  was one of those readers, taking every `LoggedOn` for its own backoff ladder. It compares
  `Engine::logons()` across a turn instead. Nothing changed on a turn: the counter is
  incremented in the branch that already tests whether a session came up.
- **BREAKING — `Journal` gains two methods, and one has no default body.** Anyone implementing
  the trait must add `highest_out()`; `mark_out()` defaults to doing nothing, which is right for
  a journal that does not survive a restart. Nothing is published, so nothing in the wild breaks.
- **BREAKING — `Session::tick_with` takes `&mut J` where it took `&J`.** A tick is where a
  `Heartbeat` and a `TestRequest` are born, and telling the journal their number is a write.
- **`Session::received_with` marks the inbound number on every path out, including the one that
  ends the session.** `[measured 2026-09-05]` it used to sit after the drain, which the
  counterparty's own `Logout` returns before — so the number that `Logout` arrived under was
  consumed and never recorded, a resumed session expected it again, and this end sent a
  `ResendRequest` for a message it already had. ADR-0017's ordering is unchanged: the mark is
  still taken after the application has seen the message, just later. Found by the interop gate.
- **Under `Durability::Fsync` an administrative message now costs a `sync_data`** — the outbound
  count is written when it moves. This is the price ADR-0017 already accepted for the inbound
  direction arriving on the outbound one; `Async` remains the default and keeps it off the
  engine thread.

- **Every entry point takes a message log.** *(breaking)* `serve`, `serve_hft`,
  `connect_and_serve`, `serve_with_recovery` and `serve_hft_with_recovery` each gained a
  trailing `log: L` parameter; pass `NoLog` for none, which compiles away entirely.
  `serve_sharded_hft` gained `log_path: Option<&Path>` and opens one file per shard,
  `messages.log.0`, `.1`, … — every engine numbers its connections from zero, so a shared file
  would write `conn=0` for several different sockets.
- **`Engine` has a tenth type parameter, `L = NoLog`.** *(breaking for anyone naming the type
  in full; the aliases and `Engine::new` are unchanged.)*
- **`ServeError::LogPath`.** *(breaking: the enum is `#[non_exhaustive]`, but a `match` naming
  every variant will need it.)* A `FileLogPath` that cannot be opened is its own startup error,
  not `Io` — a missing directory and a busy port send an operator to two different places.
- **`Reader::is_empty` means *no records and no torn tail*, not *zero bytes*.** A version-1
  journal opened and never written to is five bytes of header, and a session that has sent
  nothing must not read as a file with something in it.
- **`Journal::put` returns `bool`, and `Journal::oldest` is new.** *(breaking)*
  [ADR-0046](docs/decisions/ADR-0046-the-ring-is-the-resend-store-and-a-replay-goes-in-batches.md).
  Neither gets a default implementation, for the reason `highest` has none: a default that lies
  is worse than a compile error. `put` reporting a refusal is what lets the session count it;
  `oldest` is the floor that separates *"never sent"* from *"fell out of the ring"*.
- **`journal::SLOTS` is 4096, was 8.** ≈ 2 MiB per session at `SLOT_LEN` 512
  (`[measured 2026-09-04, Apple M5]` `tools/w2w` reads +2 195 456 bytes of RSS). Eight was
  chosen for the acceptance corpus; an acceptor that had sent a hundred `ExecutionReport`s
  replayed eight of them and gap-filled the rest, with nothing on this side saying so.
  `docs/CONFIGURATION.md` has the formula for choosing your own.
- **`MemJournal` stores its slots in a `Box<[Slot]>` and `new` is no longer `const fn`.**
  An inline `MemJournal<4096, 512>` builds 2 MiB on the stack; at 65 536 slots that is a
  SIGSEGV rather than a failing test. `Engine::add` therefore allocates once per connection —
  use `add_with_journal` with pre-built journals in `hft`.
- **`MemJournal::get` is addressed by `seq % N`.** *(bug fix)* The scan it replaced returned
  the **first** slot carrying a number, so after `Admin::SetNextOut` wound the outbound count
  back, a replay returned the **stale** message: correctly numbered, correctly checksummed,
  wrong content.
- **A `ResendRequest` is answered in batches** of `Config::resend_batch` messages, default 8.
  *(behaviour)* The whole range used to go out in one call; more than `TX` would fit set
  `overflow`, and backpressure answered a resend with `Logout 58=slow consumer`. A replay now
  continues across calls, and **`Session::tick_with` is the tick that advances one** — plain
  `Session::tick` deliberately does not, because it has no journal and would gap-fill the
  remainder.

### Added

- **`Config::with_resend_batch` / `Config::resend_batch`**, and `DEFAULT_RESEND_BATCH` = 8.
  Zero is read as one. The constraint is `resend_batch × SLOT_LEN < TX`.
- **`Session::tick_with`**, `Session::puts_refused`, `Session::resend_beyond_journal`.
- **`observe::EventKind::ResendBeyondJournal { filled, oldest }`** and
  **`JournalRefused { count }`**, plus the matching `SessionSnapshot` accessors. Both count
  **messages**, not occurrences, and a fill over an administrative message is not counted —
  none was ever replayable.

- **`fixbolt_engine::connect_and_serve` — an initiator that comes back.**
  [ADR-0043](docs/decisions/ADR-0043-backoff-without-jitter-and-a-reconnect-asks-recovery-every-time.md).
  `connect` opened a socket and nothing decided when to open it again. One session, one
  socket, `standard` mode.
  - `reconnect::Policy` — exponential backoff to a ceiling, reset by a `Logon` and not by a
    socket, an optional `Schedule` that outranks it, and `stop()`. It **answers a question and
    never sleeps**: `Next::At(instant)`, with the caller's own wait strategy doing the waiting.
  - `recovery` is asked on **every** attempt, not only the first — a reconnect is not a restart
    ([ADR-0010](docs/decisions/ADR-0010-a-reconnect-is-not-a-restart.md)). **With `NoRecovery`
    every reconnect starts at `34=1`**; a deployment that needs continuity passes a `Recovery`
    backed by a journal on disk. `GUIDE.md` §8c.
  - No jitter, and no `hft` entry point. Both named in the ADR's consequences.
  - `TcpInitiatorEngine<A, W, J>`, the dialling counterpart of `TcpAcceptorEngine`.

- **Three things an operator can order a session to say**, on `fixbolt_session::Session<R, N>`,
  alongside the three that already existed:
  - `send_heartbeat(emit) -> bool` — `35=0`, carrying no `112=`. Not the heartbeat rule; this
    is a keepalive through a device that times a connection out faster than the session does.
  - `send_test_request(id, emit) -> bool` — `35=1` with **the caller's** `112=`. Nothing is
    remembered: matching the answer is the caller's, because waiting for it would need a clock
    this layer does not own.
  - `send_resend_request(from, to, emit) -> bool` — `35=2` with the caller's `7=` and `16=`.
    `to == 0` is *"and everything after"* and is passed through, not refused.

  All three are silent — `false`, and nothing on the wire — unless the session is logged on.
  **None of them takes whole message bytes**: the session builds the message from its own
  `Template` and keeps `8`, `9`, `34`, `49`, `52`, `56` and `10`
  ([ADR-0042](docs/decisions/ADR-0042-a-second-implementation-is-the-only-independent-opinion.md)).
  Zero allocations, proven by injection.

- **A new crate, `fixbolt` (`crates/library`) — the application-facing API.**
  `DESIGN.md` §3 L4 and §7 step 8. It adds no capability: every byte still goes through
  `fixbolt-engine` and `fixbolt-session`. What it adds is one crate to depend on and a
  handler that does not have to know the session's job.
  - `Handler<N = 256, P = 64, S = 1024>` — `on_message(&Incoming<'_, N>, Reply<'_, P, S>)
    -> Answer`. Runs **on the engine thread**, inline, like any other application.
  - `Incoming` — the message already parsed: `msg_type`, `get`, `seq`, `sender`, `target`,
    `view`.
  - `Reply` / `Message` / `Answer` / `ReplyError` — a reply that writes `8`, `9`, `10`, `34`,
    `49`, `52` and `56` itself, with `49`/`56` **reversed**, and sorts every field the handler
    names through the generated tables. A handler naming one of the four session-owned tags is
    ignored rather than merged. `Answer::Failed` when the reply does not fit — never a partial
    message.
  - `App<H>` and `app(h)` — adapt a `Handler` onto the existing `fixbolt_session::Application`
    seam, plus `App::unparsable()` and `App::failed_replies()`, because an uncounted silence
    is one nobody can explain.
  - A curated re-export surface: `serve`, `serve_hft`, `serve_with_recovery`,
    `serve_hft_with_recovery`, `ServeError`, `Shutdown`, `Config`, `Role`, `Link`,
    `DropReason`, `Schedule`, `Table`, `Limits`, `LimitError`, `Entry`, `Identity`,
    `Registry`, `Settings`, `Observer`, `Admin`, `Event`, `Snapshot`, `Recovery`,
    `FileJournal`, `MessageView` and the rest of what an application needs — and **nothing
    else**. `Engine`, `Dispatch`, `Transport`, `wait`, `shard`, `affinity`, `frame` and `ring`
    are deliberately absent.
  - Feature `standard`, on by default, forwarding to `fixbolt-engine/standard`. The `#[cfg]`
    is on the `pub use serve` itself and not only in the manifest, and
    `scripts/check-no-optional-deps.sh` has a `fixbolt:libc` case.
  - **What it costs, published rather than discovered:** `[measured 2026-09-02, Intel Xeon @
    2.80GHz, a machine that does NOT meet `DESIGN.md` §9]` a twelve-field reply through
    `App::on_message` is **~2.1 µs**, against **40 ns** to encode a `Template` built once —
    about **50×**. The second parse is ~190 ns of it; building a template per message is the
    rest. `fixbolt_session::Application` is untouched and remains the way to write a handler
    that cares.
    [ADR-0041](docs/decisions/ADR-0041-the-library-layer-buys-an-api-with-a-template-per-message.md).
- **`presession::LimitError` implements `Display` and `std::error::Error`.** `ServeError` and
  `SettingsError` already did; this one did not, so `Limits::new(64, 30_000)?` into a
  `Box<dyn Error>` — the first line of the new crate's own worked example — did not compile.
  Additive; no behaviour change.

### Fixed

- **A session that says goodbye first no longer answers the acknowledgement.** A `Logout`
  exchange is one message each way; this engine sent a third. QuickFIX's `nextLogout` replies
  only when it did not begin the exchange, and now so does this. Nothing could see it: the 59
  acceptance definitions never have the acceptor start a logout, `their_answer_ends_the_session`
  passed an `emit` that counted nothing, and `scripts/interop.sh` stops reading once it has seen
  the counterparty's `35=5`. The **mirrored** corpus found it. A `Logout` this end did not start
  is still answered — both halves are in `crates/session/tests/goodbye.rs`.
- **`Session::begin_logout(b"")` no longer writes an empty `58=`.** No words means no field, not
  a field with nothing in it. Found the same way, on the same file.

- **An initiator no longer answers a `Logon` with a `Logon`.** The inbound-Logon handler
  replied for **both** roles; for an acceptor that is the handshake, for an initiator — which
  sent the first one — it starts a second handshake on a session that already has one. A real
  counterparty drops the connection for it without a word, so the whole role was unusable
  against anything but this repository's own tests.
  `[measured 2026-09-02]` the defect was green in the 59 / 59 acceptance score (for an
  *acceptor* the reply is correct), in the mirrored corpus at its asserted 0 / 50, and in 430
  other tests. It was found on the first run of `scripts/interop.sh` against `libquickfix` —
  [reference](docs/reference/a-role-can-be-wrong-in-a-direction-no-gate-runs.md). The reply is
  now behind `!R::SPEAKS_FIRST`; acceptor behaviour is unchanged.

### Changed

- **`fixbolt_codec::TemplateBuilder`'s `field`, `slot`, `group` and `build` take `&mut self`.**
  [ADR-0044](docs/decisions/ADR-0044-a-builder-that-is-not-moved-per-field.md). They took `self`
  by value, so an `S`-byte struct was copied **once per field** — with `S = 1024` that is
  kilobytes of memcpy to add a few bytes. `[measured 2026-09-02, on a machine that fails §9]`
  `library, reply only` **1 549 → 766 ns/op (−51%)**, `library, on_message` **1 594 → 956 ns
  (−40%)**, with `library, parse only` unmoved at 144 → 146 ns as the control.
  **Chaining is unchanged where the chain starts from a temporary** — Rust auto-refs it — so
  `crates/session/src/out.rs` and its ~70 chained calls did not change at all. A call site that
  binds the chain to a variable now binds first and mutates after.
- **`fixbolt::Message`'s `field`, `group`, `send` and `send_with_groups` follow**, which keeps
  the handler shape identical: `reply.message(b"8").field(37, id).send()`.
  ADR-0041's published ratio moves from ~50× the 40 ns template path to **~24×**; the rest is
  the `Template` still being materialised per message, and `STATUS.md` item 34 stays open with
  766 ns as the number to beat.

- **`Engine::run`, `serve`, `serve_hft` and `serve_with_recovery` return instead of never
  returning.** `run()` was `-> !` and the `serve*` family was
  `Result<core::convert::Infallible, ServeError>`; they now hand back a
  [`Shutdown`](docs/decisions/ADR-0038-an-ordered-shutdown-is-a-state-not-a-flag.md). Nothing
  in the repository called `run()`, so the break costs no caller here — it costs an embedder
  a `let _ =` or a match, and buys the ability to stop at all.
- **`serve_sharded_hft` is unchanged and still cannot be stopped.** It is Linux-only; see
  `STATUS.md` *Not proven*.
- **`Recovery` gained a required method, `fresh(&Config) -> J`**, and `pump`,
  `serve_with_recovery` and `serve_hft_with_recovery` became **generic over the journal**.
  [ADR-0039](docs/decisions/ADR-0039-a-fresh-journal-is-the-deployments-to-build.md). The
  engine used to build an empty journal with `J::default()`, which put `J: Default` on the
  whole serving loop — and a `FileJournal` has no honest `Default`, so no deployment could
  use a journal on disk. Implementors of `Recovery` must now supply `fresh`; `NoRecovery` and
  `FromFn` provide it for any `J: Default`, so a `journal::Store` deployment is unaffected.
- **`journal::Record` gained `ActivityMark`.** It is `#[non_exhaustive]`-free on purpose, so
  a `match` over it will not compile until the new shape is handled — which is how three
  places, `tools/jrnl` included, were found rather than silently skipped.
- **`TcpAcceptorEngine` takes a third type parameter, `J`, defaulting to `journal::Store`.**
  Existing uses compile unchanged.

### Added

- **`fixbolt-engine::settings` — who this acceptor serves, read from a file.**
  [ADR-0040](docs/decisions/ADR-0040-a-configuration-file-refuses-what-it-does-not-understand.md).
  `Settings::load` / `Settings::parse` read a `[DEFAULT]` plus `[SESSION]` INI shaped like
  QuickFIX's and `Settings::into_table` builds a `presession::Table`, so adding a counterparty
  is an edit and a restart rather than a release. **No new dependency.** Keys:
  `BeginString`, `SenderCompID`, `TargetCompID`, `HeartBtInt`, `MaxSkewMillis`, `StartTime`,
  `EndTime`, `StartDay`, `EndDay`, `Weekdays`. Three behaviours differ from QuickFIX on
  purpose — an **unrecognised key is an error**, a file with **no `[SESSION]` is an error**,
  and every `SettingsError` carries its **line number** and quotes what was written.
- **`fixbolt-session::MAX_BEGIN_STRING_LEN` and `MAX_COMP_ID_LEN`.** The sizes `Config` stores
  names in, published so a caller building a `Config` from text can refuse an over-long value
  instead of truncating it into one that matches nothing.

- **`fixbolt-session`** — the FIX session state machine. Pure: no socket, no clock, no
  allocation, no `format!` on any path. Depends on `codec` and `dict`.
  - **`schedule` — when a session is open, and when both ends start again at `34=1`.**
    New module, no dependency, no feature flag.
    [ADR-0033](docs/decisions/ADR-0033-a-schedule-is-utc-arithmetic-and-the-calendar-stays-outside.md).
    - `Schedule` — `always()`, `daily(open_sod, close_sod)`, `weekly(day, sod, day, sod)`,
      `with_weekdays(Weekdays)`, `with_utc_offset_ms(i64)`; `contains(t)` and
      `same_session(a, b)`. `Copy`, and **every constructor returns `Option`**: a schedule
      that cannot be honoured is refused where it is written, not discovered at 3 a.m.
    - `Weekday` and `Weekdays` — a seven-bit mask, so nothing allocates and `Schedule` stays
      `Copy`. `ALL`, `WEEKDAYS`, `WEEKEND`, `NONE`, `only`, `and`.
    - **Times are UTC. There is no timezone database and no daylight saving.** Resolving
      *"17:00 America/New_York"* is the caller's job, with their own zone library, rebuilding
      the `Schedule` when the offset changes. `with_utc_offset_ms` is a **fixed** offset and
      is not DST support — `GUIDE.md` §5a carries the warning.
    - `open > close` wraps: 22:00–06:00 is **one** session, and every instant in it belongs to
      the same one.
    - `Config::with_schedule` / `Config::schedule`. The default is `Schedule::always()` and it
      is exactly neutral — `[measured 2026-09-02]` 59/59 unmoved, in process, through a real
      socket, and in `standard` mode.
    - **`Session::resume_at(cfg, next_out, next_in, last_active_ms)`**, and
      `Session::last_active_ms()`. `Session::resume` carries the numbers and asserts nothing
      about the calendar, so it **never** resets on a boundary — ADR-0010 unchanged. A reset
      cannot be decided from the numbers alone: `next_out = 41` says nothing about whether a
      boundary has passed since 41 was reached, so the instant is a separate input.
    - Behaviour: a message arriving while the schedule says shut is **refused in silence**,
      outranking every identity and sequence check; the window closing on a live session sends
      a `Logout` **with no `58=`** and gives up the link; a boundary crossed since the session
      was last active **restarts both counts at the top of the tick**, ahead of the numbering.
    - `[measured 2026-09-02]` `crates/session/benches/alloc.rs` cases `schedule-open` and
      `schedule-shut` both read **0**.
  - **`Session::set_next_out`, `set_next_in`, `send_sequence_reset`** — moving a sequence
    number by hand, on a session that is already running.
    [ADR-0036](docs/decisions/ADR-0036-one-mechanism-two-capabilities.md).
    - `set_next_out(n)` and `set_next_in(n)` are **local and silent**. The first is a lie
      until the counterparty is told, and is named after QuickFIX's
      `setNextSenderMsgSeqNum` rather than improved on: an operator who knows that name
      knows what it does. The second is not a lie — what you expect is your own business.
    - `send_sequence_reset(n, emit)` is the **honest** form: `35=4` with `123=N` and `36=n`,
      sent at the current number, and `next_out` becomes `n` only after it.
    - All three refuse `n == 0` and **change nothing when they do** — there is no `34=0`.
      A reset that cannot be built does not move the number either.
    - A reset **downwards** is permitted. It is a last resort and a test asserts the
      permission, so it reads as deliberate rather than as an oversight.
  - **`Journal::mark_active(at_ms)` and `Journal::last_active()`** — when the session was
    last alive, on a journal that survives a restart.
    [ADR-0039](docs/decisions/ADR-0039-a-fresh-journal-is-the-deployments-to-build.md).
    - **Default bodies are empty**, so a journal that cannot outlive the process is not
      obliged to pretend. `None` means *"this journal does not know"* — **not** *"the session
      was never active"*, and a caller that confuses the two silently restarts its numbering.
    - The sequence numbers cannot imply it: `next_out = 9` says nothing about whether a
      trading day has ended since 9 was reached, which is what
      [ADR-0033](docs/decisions/ADR-0033-a-schedule-is-utc-arithmetic-and-the-calendar-stays-outside.md)'s
      boundary reset needs after a restart.
  - **`Session::begin_logout`, `State::LoggingOut`, `DropReason::EngineShutdown`** — saying
    goodbye and **waiting to be answered**.
    [ADR-0038](docs/decisions/ADR-0038-an-ordered-shutdown-is-a-state-not-a-flag.md).
    - `begin_logout` sends the `Logout` and returns `Link::Up`, so the caller keeps turning
      until the counterparty's own `Logout` arrives — reported as `DropReason::PeerLogout` —
      or until the caller gives up. **`logout_now` is unchanged**: it is D10's path, where
      cutting immediately is right, and one function serving both is how both go wrong.
    - `State::LoggingOut` is separate from `AwaitingLogout` on purpose. The latter reports the
      link **down at once** and ignores what follows; `[measured 2026-09-02]` reusing it made
      every wait vacuous — *they answered* and *they never answered* became one observable.
    - Only a logged-on session is told. FIX has no `Logout` before a `Logon`, so a connection
      that never got that far is **ended with `EngineShutdown`** rather than sent a message it
      must not receive — a reason, because an anonymous close reads as `EndedWithoutReason`.
  - **`DropReason`, `Session::last_drop_reason()`, `disconnect_with`, `note_drop_reason`** —
    why a connection ended, instead of one bit.
    [ADR-0035](docs/decisions/ADR-0035-an-event-is-pushed-and-a-loss-is-counted.md).
    - `Link::Dropped` is returned from **eighteen** places, and nothing at the other end told
      them apart. On the wire a bad clock and a shut venue are the same observable — silence —
      and six acceptance definitions expect no response at all, so 59/59 is blind to the
      difference. Different people fix them, on different days.
    - `DropReason` is a **fieldless** `#[non_exhaustive]` enum, so the session stays pure: no
      clock, no allocation, no `format!`. `Link`'s signature is unchanged.
    - Recorded at every refusal **and** on the paths a refusal never reaches — a heartbeat
      that timed out, the counterparty's own `Logout`, a window that closed. `From<Refusal>`
      is exhaustive with **no `_` arm**, so a new refusal that is not named will not compile.
    - `connect()` clears it: a live session has nothing to explain, and a stale reason read as
      a current one is worse than none.
    - `disconnect_with(why, emit)` and `note_drop_reason(why)` let the **engine** name a cause
      the session cannot know. **A cause already known is never replaced** — before that rule,
      `disconnect()` overwrote every specific reason with the transport's.
  - **`Session::last_skew_ms() -> Option<i64>`** — the engine's clock minus the
    `SendingTime` of the last inbound message whose `52=` could be read, in milliseconds.
    Recorded **whether that message was accepted or refused**, because a `max_skew_ms`
    refusal is silent by protocol and is exactly the case this number exists to explain.
    Still pure: an `Option<i64>` on the struct, computed from the `now_ms` that already
    arrives as a tick.
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
  - **`recovery` — what a counterparty left behind, asked when its identity is known.** New
    module. [ADR-0034](docs/decisions/ADR-0034-recovery-is-asked-once-the-counterparty-is-known.md).
    - `Recovery<J>` with `recover(&mut self, &Config) -> Option<Resumed<J>>`, `Resumed<J>`
      (`journal`, `next_out`, `next_in`, `last_active_ms`), `NoRecovery`, and `FromFn` for a
      closure.
    - **`serve_with_recovery` and `serve_hft_with_recovery`.** `serve` and `serve_hft` are
      unchanged and delegate through `NoRecovery`. New functions rather than new parameters,
      so `tools/w2w` and the Linux-only shard path are untouched.
    - Asked **once per connection, after the registry names the counterparty**, on the
      acceptor thread — before the `Logon` there is no identity to look a journal up by
      (ADR-0020, ADR-0026), and that thread is the one allowed to block, so an implementation
      may read a file.
    - `[measured 2026-09-02]` proven by two reversals that fail on **opposite** tests: making
      `pump` discard the answer reddens only the resuming test, and making `NoRecovery`
      fabricate a session reddens only the plain-`serve` control. 59/59 green through both.
    - **`serve_sharded_hft` has no recovery variant**, and `pump` fixes `J = journal::Store`,
      so a per-counterparty `FileJournal` through the serving loop is not yet possible.
  - **`Engine::add_with_prefix_config_and_state`** — as `add_with_prefix_and_config`, taking
    what `Recovery` answered. The seam the serving loop needs.
  - **`Engine::add_resumed(transport, cfg, journal, next_out, next_in, last_active_ms)`** —
    **the only way an `Engine` continues a session that outlived the process.**
    `[verified 2026-09-02]` before it existed every `add` built `Session::new`, which resets,
    so `Journal::highest`, `Session::resume`, ADR-0010, ADR-0017 and `Durability::Fsync` were
    all real, all tested, and all unreachable through this type (`STATUS.md` item 31).
    - **The journal is taken as well as the counts.** Correct numbers over an empty journal
      answer the first `ResendRequest` with a `SequenceReset` gap fill — legal, and a silent
      loss of everything the counterparty asked for. Two tests tell the outcomes apart.
    - **`last_active_ms` is what makes a schedule boundary decidable**, and it is a separate
      argument because the numbers cannot imply it: `next_out = 9` says nothing about whether
      a trading day has ended since 9 was reached. Supplied, ADR-0033's reset is reachable
      from an engine; `None` means no boundary is ever noticed, which is right under
      `Schedule::always` and wrong under anything else.
    - The engine still does not read the journal for you and does not guess. ADR-0010's point
      is that choosing between a restart and a continuation is the caller's.
  - **`journal::Reader`, `Records`, `Record`, and `FileJournal::torn_tail_bytes()`** —
    reading the file from outside the process that wrote it.
    [ADR-0037](docs/decisions/ADR-0037-reading-a-journal-is-not-recovering-from-one.md).
    - `FileJournal` reloads into a fixed ring of `N`, because its job is the next
      `ResendRequest`. The operations question — *"we sent order X at 10:32, did you receive
      it?"* — is about a message the ring dropped long ago. `Reader` is an `Iterator` over
      the whole file: no `N`, no `LEN`, no bound.
    - `Record::{Message, InboundMark}`. The mark is [ADR-0017](docs/decisions/ADR-0017-the-inbound-count-is-persisted-after-delivery.md)'s
      zero-length record and comes back **as a mark**, not as an empty message.
    - **It allocates**, and the rustdoc says why that is allowed: nothing here runs on the
      engine thread. A file too large to hold in memory is a real limit, named in `GUIDE.md`.
    - **It does not interpret FIX.** That needs a dictionary, and a file reader has no
      business pulling one in.
    - `FileJournal::torn_tail_bytes()` reports bytes at the end that did not form a whole
      record — a process killed mid-write. Those bytes are **not** replayed, which is
      correct; `[2026-09-02]` they were also **not reported**, which was not.
  - **`journal::Record::ActivityMark` and `FileJournal`'s activity marks** — a record whose
    **sequence number** is zero, carrying eight little-endian bytes of milliseconds.
    [ADR-0039](docs/decisions/ADR-0039-a-fresh-journal-is-the-deployments-to-build.md).
    - `34=0` is not a sequence number FIX has, so it cannot be confused with a message —
      the mirror of the `len == 0` inbound mark. **The file format did not change**: every
      file written before this parses exactly as it did, and the reader is one branch longer.
    - Written when a session logs on and when an ordered shutdown says goodbye. **Never per
      message** — that is a disk write on the hot path, which D8 forbids.
    - `tools/jrnl --count` reports `last-alive`, and a full dump shows `live <ms>`.
  - **`observe` — what an operator can see, from another thread, while the engine runs.** New
    module, no feature flag, no dependency.
    [ADR-0032](docs/decisions/ADR-0032-observation-is-a-snapshot-taken-on-request.md).
    - `Engine::observer() -> Observer` — hands out a `Send + Sync` handle. **Calling it is
      what makes the engine observable at all**: until then the engine does nothing about it,
      and afterwards a turn does one relaxed load. One allocation, here, never on a turn.
    - `Observer::request() -> Option<Snapshot>` — takes the most recent snapshot the engine
      published and **asks for a fresh one**. It does not wait, in either direction; `None`
      before the engine has published anything. `Observer::published() -> u64` counts the
      snapshots built, which is what keeps *"on request"* falsifiable.
    - `Snapshot` — `sessions()`, `truncated()`, `connections()`, `refused_connections()`,
      `sources_missing()`, and `healthy()`. Plain `Copy` data, a fixed `[SessionSnapshot;
      MAX_SESSIONS]` with `MAX_SESSIONS = 64`, because non-negotiable 1 forbids the `Vec` and
      `standard` has no session ceiling — beyond it `truncated()` is set and the fact is
      reported rather than failed on.
    - `SessionSnapshot` — `id()`, `logged_on()`, `next_out()`, `next_in()`,
      `last_skew_ms()`, `has_pending_output()`.
    - `Snapshot::healthy()` is a **pure function on that data** — at least one session, all
      logged on, neither should-be-zero counter non-zero — so a health probe and an
      operator's print cannot disagree. Truncation is not unhealthy.
    - `[measured 2026-09-01]` `benches/alloc.rs` cases `observe-idle` and `observe-asked`
      both read **0**: being watched allocates nothing, and being watched on every single turn
      allocates nothing. The **nanosecond** cost of a turn that publishes is **not measured** —
      it needs the §9 machine.
    - **`Event`, `EventKind`, `EVENT_CAPACITY`, `Observer::events(&mut Vec<Event>) -> usize`,
      `Observer::events_lost() -> u64`** — endings, **pushed** rather than asked for.
      [ADR-0035](docs/decisions/ADR-0035-an-event-is-pushed-and-a-loss-is-counted.md).
      - A snapshot tells you what **is**; by the time you ask, a session that ended is gone
        from it. So the engine records an `Event` when a session's state changes, whether or
        not anybody is reading.
      - `EventKind` is `LoggedOn`, `Ended(DropReason)` or `EndedWithoutReason`. The third is a
        variant and not a guess: a diagnostic that invents the most likely cause is worse than
        one that admits it does not know.
      - One `try_lock` per **state change** — logon, logout, disconnect — and never per
        message; D8 forbids anything message-rate on the hot path. A refused lock, or a ring
        with no room, bumps `events_lost()` and the turn continues: an observer may never drop
        a session ([ADR-0011](docs/decisions/ADR-0011-a-full-ring-disconnects.md)).
      - `EVENT_CAPACITY` is 256 and **losses are counted, not swallowed** — a stream that
        loses silently is a source an operator would keep trusting. The counter has its own
        test, which drives the ring past full on purpose.
      - The engine names three endings the session cannot see: `DuplicateIdentity`
        (ADR-0030's single-logon rule, which `[measured 2026-09-02]` reported itself as
        `TransportClosed` — the network blamed for a policy decision), `SlowApplication` and
        `SlowConsumer` (D10).
      - `[measured 2026-09-02]` `benches/alloc.rs` cases `events-idle` and `events-busy` both
        read **0**, the second asserting the stream recorded something **inside** the counted
        window — three earlier readings of that case measured its own fixture
        ([reference](docs/reference/a-benchmark-measured-its-own-fixture.md)).
    - **`Command`, `Admin`, `Engine::admin()`, `COMMAND_CAPACITY`, `Change`, `Outcome`, and
      `EventKind::Administered`** — changing a running engine, not only watching it.
      [ADR-0036](docs/decisions/ADR-0036-one-mechanism-two-capabilities.md).
      - **One mechanism, two capabilities.** Commands share `observe`'s `Arc` and its fixed
        shapes; the engine hands out `Observer` (reads) and `Admin` (writes). Give an
        `Observer` to everything that watches and an `Admin` only to what administers.
      - `Command::{SetNextOut, SetNextIn, SendSequenceReset}`, applied at the **top of a
        turn, before anything is judged or numbered**. Applied afterwards, an operator's
        change misses by exactly one message.
      - **The lock asymmetry is the design.** `Admin::submit` takes `lock()` because the
        operator's thread may block; the engine's drain takes `try_lock()` because its own
        may not. A refused lock takes nothing and **loses nothing** — unlike an event, a
        lost command is an action that silently did not happen.
      - A full queue (`COMMAND_CAPACITY` = 32) **refuses at the call**, so a command is
        never silently swallowed.
      - `submit` answers *queued or not*; the outcome arrives as
        `EventKind::Administered { change, to, outcome }`. It cannot be otherwise:
        `Outcome::NoSuchConnection` is the ordinary answer for a command that raced a
        disconnect, and is unknowable at submit time.
      - `Admin::drains()` counts how often the engine reached for the queue. **A turn on an
        engine nobody is administering costs one relaxed load and does not touch the mutex**,
        and this counter is what keeps that falsifiable — the first version attempted the
        lock every turn and every content assertion stayed green.
      - `[measured 2026-09-02]` `benches/alloc.rs` cases `admin-idle` and `admin-busy` both
        read **0**, the second asserting the stream recorded something inside the window.
    - **`Admin::shutdown(grace_ms)`, `Engine::shutdown_finished()`, `Shutdown`** — stopping
      the engine without lying to the counterparty.
      [ADR-0038](docs/decisions/ADR-0038-an-ordered-shutdown-is-a-state-not-a-flag.md).
      - Not a `Command`: every command is about one connection, and this is the engine's own
        life. Same `Arc`, same capability split — an `Observer` cannot stop it, an `Admin`
        can. Asking twice is harmless and **the first grace period stands**.
      - `Shutdown` reports `sessions`, `said_goodbye`, `acked`, `timed_out` and `clean()`.
        *"We stopped"* and *"we stopped while two counterparties never answered"* are
        different facts, and only the second needs a human before restarting.
      - Sessions still present at the deadline are closed, given `EngineShutdown`, and
        **emit an `Ended` event first** — clearing the vector would take them away without a
        word.
      - `[measured 2026-09-02]` `benches/alloc.rs` case `shutdown` reads **0**, with a
        control asserting every session was told and that the goodbye reached the wire.
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

  - **`presession`, part two — the socket has a deadline and the table has a ceiling.**
    `Limits` (a named struct, because two `usize`-shaped limits as positional arguments
    would transpose silently), `LimitError`, `PendingSet`, `Pending`, `Refused`,
    `Progress`. **Neither limit has a default and zero is refused for both**
    ([ADR-0020](docs/decisions/ADR-0020-a-pre-session-stage-owns-the-socket-until-logon.md)
    decision 4) — a connection that opens and says nothing costs a slot forever, and a
    table with no ceiling costs memory forever.
    - A full table refuses **immediately** and hands the socket back in `Refused::Full`,
      so closing it is something the caller does on purpose.
    - Everything read off the socket is handed on, `Logon` **and anything pipelined
      behind it** — `Framer` gains `all()` for it. A stage that passed on only the
      message it routed by would lose the rest in silence.
    - `[measured 2026-09-01]` 12 tests, four reversals — no timeout, no ceiling, hand on
      only the message, let a non-`Logon` through — each red on the case that names it.
      Allocation: three cases in `benches/alloc.rs`, all 0, and the third proven to go
      red at 7 allocations when the one-time reservation is removed.

  - **`presession`, part four — a counterparty registry, and it is BREAKING.** `Registry`,
    `Entry`, `Table` and `One` are new; `PendingSet<T, PRE>` becomes
    `PendingSet<T, R, PRE>` and `PendingSet::new` takes the registry; `Progress` gains
    `unknown`; `Pending` gains `config()`. On `fixbolt-session`, `Config` gains
    `serves`, `inbound_sender_matches` and `inbound_target_matches`.
    [ADR-0026](docs/decisions/ADR-0026-a-counterparty-registry-in-the-pre-session-stage.md).
    - **A trait, not a map.** `lookup(Identity) -> Option<&Entry>`, and `Table` is one
      implementation of it. `Option` rather than `Result`, so there is nothing to
      `unwrap`; **synchronous**, so an accept path cannot await a network call, which is
      where this deliberately parts from Artio's `authenticateAsync`; and returning `None`
      **is** the authentication hook — there will be no second `AuthStrategy` beside it.
    - **An empty registry refuses every connection**, and there is no wildcard. An
      acceptor that admits an identity nobody configured is an open port.
    - **`Config::serves` is the only comparison.** The registry and the session do not
      each own a copy — the session's `Logon` check calls the same two predicates
      `serves` is composed from.
    - `[measured 2026-09-01]` the refusal for an unknown identity moves one stage
      earlier, so `1c_InvalidSenderCompID.def` and `1c_InvalidTargetCompID.def` are now
      scored by the pre-session stage. **The corpus is unmoved: 59 through one shard and
      through two**, CI run
      [33512983304](https://github.com/tmthang86/fixbolt/actions/runs/33512983304) —
      [ADR-0029](docs/decisions/ADR-0029-the-pre-session-stage-enforces-four-definitions.md),
      which amends ADR-0022's count of two to four.
    - `[measured 2026-09-01]` 8 tests, two reversals — `lookup` ignoring the identity, and
      an unknown identity held rather than dropped — each red on the assertion naming it.
      Allocation: `benches/alloc.rs` still reports 0 for all three pre-session cases with
      `lookup` on the path.
  - **`presession`, part five — one engine holds many counterparties, and it is BREAKING.**
    [ADR-0030](docs/decisions/ADR-0030-one-engine-holds-many-counterparties.md), superseding
    ADR-0026 decision 5.
    - **`serve`, `serve_hft` and `serve_sharded_hft` take a `presession::Table` and a
      `presession::Limits`** instead of a `Config`, and return `ServeError` /
      `ShardError::NoCounterparties`. **An empty registry is refused at startup**, not at
      every connection for as long as the process lives.
    - `Engine::add_with_prefix_and_config`, `Session::config()`,
      `Config::same_identity_as`, `Table::first()` are new. `Shardable::add` gains the
      `Config`. `Engine::new`'s `Config` is now only the default for `Engine::add`.
    - **The single-logon rule compares identities.** It counted logged-on connections,
      which was the same answer only while an engine held one identity.
      `1b_DuplicateIdentity.def`'s own comment is the specification: *"If two logons with
      the same SenderCompID/TargetCompID combination logon the second one must be
      disconnected."*
    - **`Identity` gains `sender_sub` (`50=`) and `target_sub` (`57=`)**, and
      `Identity::comp_ids` builds one without them. `HashRoute` ignores them on purpose:
      two connections from one counterparty that differ in `50=` must still land on one
      shard. `Table` ignores them too — a deployment told apart by sub-ID writes its own
      `Registry`, which is what the trait is for.
    - `[measured 2026-09-01]` a new test puts two counterparties on one engine and a
      duplicate of one behind them. Reverting the rule to a count makes it red; **deleting
      the rule entirely also makes it red, while the corpus alone would not notice** —
      `tests/wire.rs` catches deletion and only this test catches the failure to compare.


  - **`presession`, part three, and it is a BREAKING change to `shard`.** `Assign`,
    `RoundRobin` and `Shards::with_assign` are **removed**, replaced by `Route`,
    `HashRoute` and `Shards::with_route`; `ShardError::BadAssignment` becomes `BadRoute`
    and `NoIdentity` joins it; `Shards` gains a `PRE` const parameter; `Shards::hand`
    takes a `Pending` rather than a `TcpTransport`; `Shardable::add` takes the bytes
    already read and returns whether they fit; `serve_sharded_hft` takes a `Limits`.
    `Engine::add_with_prefix` and `Connection::prime` are new, as is `Framer::all`.
    - **`Assign` could not have worked however it was written.** It was asked at accept
      time, when the `Logon` that names the counterparty had not arrived. `Route` is
      asked after it, which is the only moment the question has an answer.
    - **`RoundRobin` is deleted with no shim.** It is the policy that produced the
      defect; keeping it would leave a documented trap in a public API.
    - `[measured 2026-09-01]` **the acceptance corpus scores 59 through two shards**,
      where `[measured 2026-08-31]` it scored 57 and failed exactly
      `1b_DuplicateIdentity.def` and `AlreadyLoggedOn.def`. The characterisation test
      that pinned the defect went red first, as it was written to.
    - `[measured 2026-09-01]` and it passes for the right reason: the test counts how
      the pre-session stage disposed of every socket, because a connection it threw away
      is indistinguishable from a duplicate the session refused. Exactly two are
      disposed of — `1e_NotLogonMessage.def` and `1d_InvalidLogonLengthInvalid.def`,
      both definitions whose subject is that the link must be dropped
      ([ADR-0022](docs/decisions/ADR-0022-the-pre-session-stage-enforces-two-definitions.md)).
    - `Route` and `HashRoute` are exported from `presession`, and re-exported from `shard`.
      Routing by identity has nothing to do with pinning a core, and `scripts/bench.sh` runs
      `cargo bench` with no features — a benchmark could not have reached them behind
      `affinity`.
    - `[measured 2026-09-01]` what it costs, 20 qualifying runs on the §9 machine: the
      stage's sweep is **426.2 ns per socket** against `Engine::turn`'s 458.3, its own work
      over the bare `recv` is **~15 ns** against the engine's ~28, and reading both comp IDs
      and choosing a shard is **84.0 ns once per connection**. `DESIGN.md` §8 does not move:
      none of it is on the message path.

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
