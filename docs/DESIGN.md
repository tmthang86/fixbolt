# fixbolt — Design

A FIX 4.4 engine in Rust, **bidirectional** — acceptor and initiator on one session core,
parameterised by role ([ADR-0004](decisions/ADR-0004-bidirectional-engine.md)) — built so that
latency is a property the design guarantees rather than one it hopes for.

**Positioning, stated plainly:** the fastest FIX acceptor that can be built **on kernel TCP**.
The acceptor stays the headline because that is where the gap is: as of 2026-08-27 the Rust
ecosystem has no production-proven FIX acceptor, while it already has two initiators
([reference/prior-art.md](reference/prior-art.md)). The initiator ships in the same phase, held
to the same gates — it is simply not the differentiator.

Not an HFT client, not kernel-bypass. FIX tag=value over the kernel stack has a latency floor
of roughly **10–20 µs** wire-to-wire that no codec can move — the figure §8 derives, and the
only one this repository uses. The job is to make everything *above* that floor disappear, and
to measure the floor honestly.

**The shape that positioning assumes is one session on one isolated polling thread**
([ADR-0012](decisions/ADR-0012-latency-first-and-one-session-per-polling-thread.md)). Latency
beats session density here, and the tie-breaker has teeth: a change trading per-session latency
for sessions-per-core needs its own ADR to reverse it. Many sessions on a thread is supported
and named **`density`**; it carries `[measured 2026-08-31]` **`N × 449 ns`** of polling on a
core set up to §9 — and `N × 670 ns` if that core has `nohz_full`, which
[ADR-0021](decisions/ADR-0021-nohz-full-leaves-section-9.md) took out of §9 for exactly this
reason — and **does not inherit the latency figures on this page**. Every figure here names its `N`.

What must be built and in which phase is [PRD.md](PRD.md); this document is *how*.

> **The name is `fixbolt`**, decided 2026-08-30. It replaced the placeholder
> `nanofixengine`, which collided with `matthart1983/nanofix` — see [STATUS.md](../STATUS.md)
> item 1 for what was checked and why the rest of the shortlist was set aside.

Settled decisions live in [decisions/](decisions/). What the landscape looks like is in
[reference/prior-art.md](reference/prior-art.md). Numbers that were actually measured — and
what they cost the designs that ignored them — are in
[reference/measured-costs.md](reference/measured-costs.md).

---

## 1. The finding this architecture is built around

A mature C++ FIX engine (fix8, 68% faster than QuickFIX) encodes a `NewOrderSingle` in
**2.1 µs** on production hardware, and says of itself that **1.4 µs** of that remains with
the framework stripped out. A Rust flyweight parser measured on an Apple M5 on 2026-08-27
parsed the same message shape in **139 ns**.

The gap is not the bytes. It is the framework: object models, dictionary lookups at runtime,
virtual dispatch, mandatory validation. Confirmation from the other direction: `hffix`
deletes the framework entirely — parse in place, no session layer — and is the fastest thing
in the survey.

**The organising principle: keep the framework off the hot path.** Every layer below is
shaped by it.

A second principle, learned from reviewing the first draft of this document: **the codec is
~1% of the wire-to-wire budget on kernel TCP.** A design that optimises the codec and says
nothing about I/O strategy, outbound encoding, or the OS underneath has optimised the wrong
1%. §4 D8–D10, §8 and §9 exist because of that review.

**A third principle, and it is the second one turning on its author.** `[measured 2026-08-31]`
the I/O strategy §4 D8 chose — one non-blocking `read` per connection per turn — costs a whole
`Engine::turn` of **449 ns per session**, flat from 1 to 16 sessions within 2%. (**~670 ns** on a
core carrying `nohz_full`, which is why §9 no longer asks for it —
[ADR-0021](decisions/ADR-0021-nohz-full-leaves-section-9.md).) **Of the 449, ~420 ns is the syscall and about 30 ns is
everything the engine itself does.** This document's own `parse NewOrderSingle` is **122.6 ns**
on the same machine. *The syscall that discovers there is nothing to parse costs 3.8× the
parse — and on the core §9 recommends, more.* So the second principle was right and
incompletely applied: the codec was priced, the I/O strategy was chosen and **never priced**, and
§8's budget was written for "the user-space path" — the half that was already cheap.
[ADR-0012](decisions/ADR-0012-latency-first-and-one-session-per-polling-thread.md) is the
consequence: **latency wins over session density, the budget is stated end to end including
syscalls, and every figure names its session count `N`.**

## 2. Layers

```
┌─────────────────────────────────────────────────────────────┐
│  Application — implements SessionHandler                    │
└──────────────────────────▲──────────────────────────────────┘
                           │
     ┌─────────────────────┴──────────────────────┐
     │  InlineDispatch — same thread as the       │  ← THE DEFAULT (D4)
     │  session machine, zero hops, the borrowed  │
     │  MessageView handed straight through       │
     │                  ── or ──                  │
     │  RingDispatch — SPSC ring, library on its  │  ← the option: an application
     │  own thread. In-process now, shared        │    that may block cannot stall
     │  memory later, without an API change       │    the session layer
     └─────────────────────┬──────────────────────┘
┌──────────────────────────┴──────────────────────────────────┐
│ L4  library    session ownership, dispatch to business code │
├─────────────────────────────────────────────────────────────┤
│ L3  engine     TCP accept AND connect (ADR-0004), drives    │
│                the session machines, owns the journal       │
├─────────────────────────────────────────────────────────────┤
│ L2  session    FIX session protocol as a PURE state machine │
│                — no sockets, no clock, no I/O.              │
│                Role { Acceptor, Initiator } is a parameter  │
├─────────────────────────────────────────────────────────────┤
│ L1  codec      parse / serialise in place, zero allocation  │
├─────────────────────────────────────────────────────────────┤
│ L0  transport  trait. TCP is the default; TLS is a second   │
│                implementation behind a feature flag (D11)   │
└─────────────────────────────────────────────────────────────┘
```

## 3. Crates

Added one at a time, each behind an approved plan.

| Crate | Layer | Owns | Depends on |
|---|---|---|---|
| `codec` | L1 | Parse and serialise. The hot path. Target: `no_std`-compatible, zero dependencies | — |
| `dict` | build | Code generation from FIX XML: tag constants, message shapes, required-field tables, **field ordering**, group delimiters and members, and the four validation tables — defined tags, message types, per-message tag sets, field types and enum values | `codec` — it implements `codec::Dictionary` |
| `session` | L2 | The FIX session state machine. Pure. No I/O. `Role`-parameterised. Time enters as `Tick`, in **milliseconds since 0000-01-01** — see D13 | `codec`, `dict` |
| | | `[2026-09-02]` module `schedule` — `Schedule`, `Weekday`, `Weekdays`. **When a session is open, and when both ends start again at `34=1`** ([ADR-0033](decisions/ADR-0033-a-schedule-is-utc-arithmetic-and-the-calendar-stays-outside.md)). Pure arithmetic on the millisecond timeline, **expressed in UTC**: no zone name, no IANA database, no daylight saving, because that database is a dependency that allocates in the layer non-negotiable 2 calls pure. `daily` / `weekly` / `always` / `with_weekdays` / `with_utc_offset_ms`, all returning `Option`. The whole type stands on one private `session_start(t) -> Option<u64>`, so the midnight wrap is reasoned about once. `Config::with_schedule`, `Session::resume_at` and `Session::last_active_ms` are the session-layer surface; `Schedule::always()` is the default and is **exactly neutral** — `[measured 2026-09-02]` forcing `same_session` to `true` turns five schedule tests red and leaves **59/59** green, which is what proves the corpus cannot see a schedule at all | |
| `transport` | L0 | `Transport` trait + TCP implementation; TLS behind a feature flag (D11) | — |
| `engine` | L3 | TCP **acceptor and connector**, drives session machines, owns the journal, and — `[2026-09-04]` — the two-directional message log (module `msglog`, D14) | `session`, `transport`, and **`libc` only under the `standard` feature** |
| | | `[2026-08-30]` step 1 of six exists: `Transport`, `TcpTransport`, `Loopback`, `Waiting`. `transport` is a module here rather than its own crate until something needs it to be otherwise | |
| | | `[2026-08-30]` modules `poll`, `block` and `waker` — `poll(2)` and `standard`'s idle turn, behind `#[cfg(all(feature = "standard", unix))]`. **The crate's first external dependency and first `unsafe`, both behind that feature**: `--no-default-features` builds it with neither (ADR-0014) | |
| | | `[2026-08-31]` module `affinity` — `CoreId`, `AffinityError`, `pin_current_thread` (which reads the mask back) and `running_on` (which reads the scheduler's own answer out of `/proc/thread-self/stat`). Behind `#[cfg(all(feature = "affinity", target_os = "linux"))]`, **off by default**, reusing the same optional `libc`. Two `unsafe` blocks, each naming its test ([ADR-0015](decisions/ADR-0015-explicit-cores-pinned-from-inside-and-read-back.md), [ADR-0019](decisions/ADR-0019-two-unsafe-blocks-and-an-error-the-enum-can-hold.md)). Also `Topology` and `ShardPlan`: the engine refuses a core that is absent, offline, duplicated, an SMT sibling of another in the plan, or — for shard cores — outside `isolcpus`, **before any thread is created** | |
| | | `[2026-08-31]` module `shard` — `Shards`, `Shardable`, `serve_sharded_hft`. One pinned thread per shard, each confirming its own pin before any of them serves; the acceptor thread blocks, because it is not an engine thread. `[2026-09-01]` **sound for more than one shard, and the corpus says so**: `Assign`/`RoundRobin` are replaced by `Route`/`HashRoute`, asked **after** the `Logon` rather than at accept, and the acceptance corpus scores **59 through two shards** where it scored 57 ([ADR-0020](decisions/ADR-0020-a-pre-session-stage-owns-the-socket-until-logon.md), [ADR-0022](decisions/ADR-0022-the-pre-session-stage-enforces-two-definitions.md)) | |
| | | `[2026-09-01]` `Route` and `HashRoute` live in `presession`, not `shard` — routing by identity has nothing to do with pinning a core, and `scripts/bench.sh` runs `cargo bench` with **no features**, so a benchmark could not have reached them behind `affinity`. `shard` re-exports both | |
| | | `[2026-09-02]` `Recovery::fresh`, `Journal::mark_active` / `last_active`, `Record::ActivityMark`, and `pump` / `serve_with_recovery` / `serve_hft_with_recovery` **generic over the journal** ([ADR-0039](decisions/ADR-0039-a-fresh-journal-is-the-deployments-to-build.md)). Item 32 (b) and (c), which were one gap: persisting *when a session was last alive* into a `FileJournal` means nothing while the serving loop cannot use one. **The whole of (b) was one bound** — the engine called `J::default()` when recovery answered `None`, and a `FileJournal` has no honest `Default`. `fresh` is **required, not defaulted**: `[measured 2026-09-02]` a `where J: Default` on a default body lands on *callers*, so the bound had merely moved; the reversal is a **compile error**, not a red test, which proves the bound is absent rather than unexercised. (c) is `seq == 0` carrying eight little-endian bytes — `34=0` is not a sequence number FIX has, the mirror of ADR-0017's `len == 0`, so **the format did not change**. Written at logon and at an ordered shutdown, **never per message**; `None` means *this journal does not know*, not *the session was never active*. `TcpAcceptorEngine` gained a **defaulted** `J` so `shard.rs` compiles untouched | |
| | | `[2026-09-02]` `Admin::shutdown`, `Engine::shutdown_finished`, `Shutdown`, `Session::begin_logout`, `State::LoggingOut`, `DropReason::EngineShutdown`; `run`, `serve` and `serve_hft` **return** ([ADR-0038](decisions/ADR-0038-an-ordered-shutdown-is-a-state-not-a-flag.md)). Until this, the only way to stop the engine was to kill the process — a dead line to the counterparty rather than a planned close, with numbered bytes lost out of `tx`. **`LoggingOut` is its own state and that is the load-bearing part**: `AwaitingLogout` reports the link down at once and ignores what follows, which is right for D10 and `[measured 2026-09-02]` made every wait vacuous — *they answered* and *they never answered* became one observable. `logout_now` is untouched. The deadline is the caller's, because a dead counterparty never answers; without it the reversal is **a hang, not a red test**. `Shutdown { sessions, said_goodbye, acked, timed_out }` names what it could not do. Nothing leaves anonymously — sessions closed at the deadline are given `EngineShutdown` and **emit an `Ended` event before the vector is cleared**. `serve_sharded_hft` is Linux-only and **not touched**. `[measured 2026-09-02]` `benches/alloc.rs` case `shutdown` reads **0** | |
| | | `[2026-09-02]` `journal::Reader`, `Records`, `Record`, and `FileJournal::torn_tail_bytes` — **reading the file from outside the process that wrote it** ([ADR-0037](decisions/ADR-0037-reading-a-journal-is-not-recovering-from-one.md)). `FileJournal` reloads into a fixed ring of `N` because its job is the next `ResendRequest`; the operations question — *"we sent order X at 10:32, did you receive it?"* — is about a message the ring dropped long ago. So an `Iterator` over the whole file: no `N`, no `LEN`, no bound. **It allocates on purpose** and says so in its own rustdoc: nothing here runs on the engine thread, and non-negotiable 1 is about the engine. It does **not** interpret FIX — that needs a dictionary, and a file reader has no business pulling one in. `[2026-09-02]` `FileJournal::open` counted torn trailing bytes into `torn` and then did `let _ = torn;` — skipping them is a correct recovery decision, being silent about it was a defect; the count is now readable, and `tools/jrnl` warns on stderr **and exits 2** so a script checking only the status cannot read success from a damaged file | |
| | | `[2026-09-02]` `observe::Command`, `Admin`, `Engine::admin()`, `COMMAND_CAPACITY`, `EventKind::Administered`, and `Session::set_next_out` / `set_next_in` / `send_sequence_reset` ([ADR-0036](decisions/ADR-0036-one-mechanism-two-capabilities.md)). **The 3 a.m. operation, which had no path at all**: `Session::resume` is a *constructor*, so changing one number meant stopping the engine. **One mechanism, two capabilities** — commands live in the same `observe::Shared` behind the same `Arc`, and the engine hands out `Observer` (reads) and `Admin` (writes) over it; what is separated is the capability, not the mechanism. Applied at the **top of `turn()`, before anything is judged or numbered** — applied after, the change misses by exactly one message. **The lock asymmetry is the design**: the operator's thread may block so `submit` takes `lock()`, the engine's may not so the drain takes `try_lock()` — and unlike an event, a refused lock **loses nothing**, because a lost command is an action that silently did not happen and no counter makes that acceptable. A full queue refuses **at the call**. Outcomes ride ADR-0035's event stream, `NoSuchConnection` included, which is why `submit` cannot report one. `SetNextOut` is silent and is a lie until the counterparty is told; `SendSequenceReset` is the honest form. No `_` arm. **One relaxed load while nobody administers** — the first version reached for the mutex every turn and every content assertion stayed green, so `Admin::drains()` exists to make the claim falsifiable, exactly as `Observer::published()` does for snapshots; `[measured 2026-09-02]` removing the check reads `left: 1002, right: 2`. `[measured 2026-09-02]` `admin-idle` and `admin-busy` both **0** | |
| | | `[2026-09-02]` `observe::Event`, `EventKind`, `EVENT_CAPACITY`, `Observer::events` / `events_lost`, and `Session::DropReason` / `last_drop_reason` ([ADR-0035](decisions/ADR-0035-an-event-is-pushed-and-a-loss-is-counted.md)). **Why a connection ended, on a thread that can act on it.** `Link::Dropped` is one bit and the session returns it from eighteen places; six acceptance definitions expect no response at all, so 59/59 is blind to every one of them. `DropReason` is **fieldless**, so D1 holds, and `From<Refusal>` is exhaustive with **no `_` arm** — an unnamed refusal will not compile. Unlike a snapshot, an event is **pushed**: a snapshot not asked for is a stale number, an event not recorded is gone. One `try_lock` per *state change* — logon, logout, disconnect — never per message; a refusal or a full ring bumps `lost` and the turn continues, because an observer may never drop a session (ADR-0011). Three endings the session cannot know are named by the engine: `DuplicateIdentity` (ADR-0030's rule, which `[measured 2026-09-02]` was reporting itself as `TransportClosed` — the socket blamed for a policy decision), `SlowApplication` and `SlowConsumer` (D10). **A cause already known is never replaced.** `[measured 2026-09-02]` `benches/alloc.rs` cases `events-idle` and `events-busy` both read **0**, the second asserting the stream recorded something inside the counted window | |
| | | `[2026-09-02]` module `settings` — `Settings`, `SettingsError`, `Problem` ([ADR-0040](decisions/ADR-0040-a-configuration-file-refuses-what-it-does-not-understand.md)). **Who this acceptor serves comes out of a file, not out of a rebuild** — the last open line of `PRD.md`'s `many counterparties`. `[DEFAULT]` and repeated `[SESSION]` blocks, QuickFIX's shape because every FIX operator has already read one; **no dependency**, because the format is `Key=Value` and `serde` plus `toml` is a tree. Three rules differ from QuickFIX and each has a test: an **unrecognised key is an error** (a mistyped `Starttime` would fall back to `Schedule::always` and hold a session open all night), a file with **no `[SESSION]` is an error** (an empty `Table` is indistinguishable from a firewall), and every error carries its **line number** and quotes what was written. `fixbolt_session` gains `MAX_BEGIN_STRING_LEN` and `MAX_COMP_ID_LEN` so the parser can refuse an over-long name rather than truncate it into one that matches nothing — one rule, one place | |
| | | `[2026-09-02]` module `recovery` — `Recovery`, `Resumed`, `NoRecovery`, `FromFn`, and `serve_with_recovery` / `serve_hft_with_recovery` ([ADR-0034](decisions/ADR-0034-recovery-is-asked-once-the-counterparty-is-known.md)). **The seam that makes recovery reachable without giving up the serving loop.** Asked once per connection, **after** the registry names the counterparty and before the connection reaches the engine, on the acceptor thread — before the `Logon` there is no identity to look a journal up by, and that thread is the one ADR-0020 allows to block. A trait, not a map, for ADR-0026's reason. `serve`/`serve_hft` are unchanged and delegate through `NoRecovery`; **`serve_sharded_hft` has no variant**. `pump` fixed `J = journal::Store` until [ADR-0039](decisions/ADR-0039-a-fresh-journal-is-the-deployments-to-build.md) made it generic | |
| | | `[2026-09-02]` `Engine::add_resumed` — **the only way this type continues a session that outlived the process.** Until it existed every `add` built `Session::new`, so D7's journal, ADR-0010's `resume` and ADR-0017's inbound count were reachable from a `Session` and from nothing else; `crates/engine/tests/recovery.rs` proved all three with **no `Engine` in the file**. It takes the journal with the counts, because correct numbers over an empty journal answer the first `ResendRequest` with a gap fill, and `last_active_ms` beside them, because the numbers cannot imply whether a schedule boundary has passed. **`serve*` still offers no seam to call it** — `STATUS.md` item 31 | |
| | | `[2026-09-01]` module `observe` — `Observer`, `Snapshot`, `SessionSnapshot`, `MAX_SESSIONS`, and `Engine::observer()`. **What an operator can see, from another thread, without touching the hot path** ([ADR-0032](decisions/ADR-0032-observation-is-a-snapshot-taken-on-request.md)). On request only: `Observer::request` raises a flag and the engine builds a snapshot on its next turn, so the cost while nobody is watching is **one relaxed load**, and none at all on an engine whose `observer()` was never called. `try_lock` never `lock` — a refusal leaves the request standing for the next turn. Fixed `[SessionSnapshot; 64]` plus `truncated`, because non-negotiable 1 forbids the `Vec` and `standard` has no session ceiling. `Session::last_skew_ms()` is new and is written **before** the `SendingTime` verdict, since a silent skew refusal is the case it exists to explain. `Snapshot::healthy()` is a pure function on that data — item 30 (f) — so a probe and a `Debug` print cannot disagree. `[measured 2026-09-01]` `benches/alloc.rs` cases `observe-idle` and `observe-asked` both read **0** | |
| | | `[2026-09-01]` module `presession`, part four, and **`Engine` stops being a link** — [ADR-0030](decisions/ADR-0030-one-engine-holds-many-counterparties.md), which supersedes ADR-0026 decision 5. **One engine holds as many counterparties as reach it**: `Engine::add_with_prefix_and_config` builds the connection's `Session` from the `Config` the registry chose, and the single-logon rule **compares identities** (`Config::same_identity_as`) instead of counting logged-on connections — which is what `1b_DuplicateIdentity.def`'s own comment always asked for. `Identity` gains `50=`/`57=`; `HashRoute` deliberately ignores them, because two connections from one counterparty must still share a shard. `serve`, `serve_hft` and `serve_sharded_hft` take a `presession::Table` and refuse an empty one at startup (`ServeError::NoCounterparties`) | |
| | | `[2026-09-01]` module `presession`, part three — `Registry`, `Entry`, `Table`, `One`. **Which counterparty, decided before a session exists** ([ADR-0026](decisions/ADR-0026-a-counterparty-registry-in-the-pre-session-stage.md)). A **trait, not a map**: `lookup(Identity) -> Option<&Entry>`, synchronous, and returning `None` *is* the authentication hook — there is no second `AuthStrategy` beside it. `Table` is the default implementation, a linear scan built once; **empty refuses everything**, because an acceptor that admits an identity nobody configured is an open port. `Table` keys on `Config::serves`, the same comparison the session's `Logon` check makes, so the rule has one home. `PendingSet` gains the registry as a type parameter and `Progress` a fifth count, `unknown`. `[measured 2026-09-01]` the corpus still scores **59 through one shard and through two**, with `1c_InvalidSenderCompID.def` and `1c_InvalidTargetCompID.def` now refused here rather than by the session — CI run 33512983304, [ADR-0029](decisions/ADR-0029-the-pre-session-stage-enforces-four-definitions.md) | |
| | | `[2026-09-01]` module `presession`, part two — `Limits`, `PendingSet`, `Pending`, `Refused`, `Progress`. It owns a socket until the first whole message, on the **acceptor** thread, under two limits with **no defaults** ([ADR-0020](decisions/ADR-0020-a-pre-session-stage-owns-the-socket-until-logon.md) decision 4): a deadline to `Logon` and a ceiling on how many may wait. Full refuses **now** and hands the socket back rather than queueing. Everything is allocated once, to the ceiling, and `benches/alloc.rs` has three cases proving it — the third exists because the first two **could not fail** | |
| | | `[2026-09-01]` module `presession` — `Identity`, `identity_of`, `is_logon`. **Unconditional, and it reads bytes only**: `49=`/`56=`/`35=` by field scan, no dictionary, no parse, nothing from `session` but `Config` ([ADR-0020](decisions/ADR-0020-a-pre-session-stage-owns-the-socket-until-logon.md) decision 2). It does **not** frame — `frame::Framer` already carries that rule and two copies of it would be two rules that disagree. `Engine`'s own `Logon` check now calls it, so the rule has one home | |
| | | `[2026-08-31]` `affinity::spawn_pinned` and `journal::FileJournal::open_pinned` — the one thread this crate spawns has a home, and the confirmation reaches the caller who can stop startup. The `RingDispatch` consumer is the caller's own thread and is validated rather than pinned | |
| `library` | L4 | The application-facing API. Package name **`fixbolt`** — the one a user names in their own `Cargo.toml` | `engine` |
| | | `[2026-09-02]` **built.** `Handler`, `Incoming`, `Reply`, `Message`, `Answer`, `ReplyError`, `App`, `app()`, plus a curated re-export surface — `serve`, `Config`, `Table`, `Limits`, `Settings`, `Observer`, `Admin`, `Recovery`, `FileJournal` and the rest of what an application needs, and **nothing else**: `Engine`, `Dispatch`, `Transport`, `wait`, `shard`, `affinity`, `frame` and `ring` are deliberately absent, so reaching for one means naming `fixbolt-engine` yourself. `Reply` writes the seven fields an application does not own — `8`, `9`, `10` from the frame, `34`, `49`, `52`, `56` from the session, with `49`/`56` **reversed** — and every field a handler names is sorted by `TemplateBuilder::build::<Fix44>()`, which is non-negotiable 5 seen from the application side. Nothing in `session` or `engine` changed for it: `App` cabinets onto the existing `Application` seam. **It is a convenience layer and not the `hft` path** — `[measured 2026-09-02, Intel Xeon @ 2.80GHz, a machine that fails §9]` a twelve-field reply cost **~2.1 µs** against **40 ns** to encode a template built once (D9's shape) — ~50×, published rather than discovered ([ADR-0041](decisions/ADR-0041-the-library-layer-buys-an-api-with-a-template-per-message.md)). `[measured 2026-09-02, same machine]` **half of that is gone**: `TemplateBuilder` no longer moves an `S`-byte struct once per field, and `library, reply only` reads **766 ns** against 1 549, `on_message` **956** against 1 594, with `parse only` unmoved at 146 as the control ([ADR-0044](decisions/ADR-0044-a-builder-that-is-not-moved-per-field.md)). **~24×, and the rest is the `Template` still being materialised per message** — `STATUS.md` item 34, with 766 as the number to beat. `crates/library/benches/cost.rs`. Defaults `P = 64, S = 1024` were chosen off a sweep, not off taste. `benches/alloc.rs` reads **0** on all three paths with an injected control reading 1000 in the same run | |
| `conformance` | dev | The `.def` acceptance runner, both roles. Also owns the corpus loader and the echo application the corpus assumes — **built before `session`**, so the gate exists before the thing it gates | `codec`, `dict` |
| `tools/w2w` | tool | The wire-to-wire harness, and the concrete binary `check-no-kernel-sleep.sh` and `check-standard-gives-the-core-back.sh` trace. **Has its own `[features]` block**, because a `cfg` never reaches into a dependency's features — `[measured 2026-08-30]` without it `--mode standard` printed its banner and ran nothing. `[2026-09-02]` **it is the instrument phase 1 exit criterion 6 is read off**, and closing that criterion took three additions to it: `--path app` (`NewOrderSingle` → `ExecutionReport` through an application holding a template built once, which is what §8's table is about), `--engine-core` / `--client-core` behind an `affinity` feature that **refuses** rather than shrugs when the build cannot pin, and p99.9 in the output. It counts its own allocations on both threads over the timed window and asserts zero, because a bench target in a library crate cannot see a binary. `scripts/w2w-baseline.sh` is the committed 20-run procedure | `engine`, `session`, `codec`, `dict` |
| `tools/jrnl` | tool | `[2026-09-02]` Reads a journal file **from outside the process that wrote it** — `STATUS.md` item 30 (e). Prints records, filters by sequence number, counts, and **warns on a torn tail with exit code 2**. Takes `engine` with `default-features = false`, so `check-no-optional-deps.sh` cannot be quietly defeated by a sibling crate the way `[measured 2026-08-30]` it once was. **No `[features]` block, deliberately**: nothing here branches on one, and declaring one would be a lie the compiler cannot catch | `engine` |

## 4. The decisions that shape it

### D1 — The session layer is a pure state machine with no I/O

```rust
pub enum Input<'a> { Message(MessageView<'a>), Tick(Timestamp), Disconnected }

/// Owns a scratch buffer. A generated Reject or Heartbeat is *written into it*, not borrowed
/// from the input — the first draft of this sketch had `Send(&'a [u8])` and could not have
/// compiled for any message the session itself originates.
pub struct ActionBuf { scratch: [u8; MAX_OUT], actions: [Action; MAX_ACTIONS], len: u8 }
pub enum Action { Send { range: Range<u16> }, Deliver, Store { seq: SeqNum, range: Range<u16> }, Disconnect }

fn step(&mut self, input: Input<'_>, out: &mut ActionBuf) -> Result<(), SessionError>;
```

```rust
pub trait Role: sealed::Sealed { const SPEAKS_FIRST: bool; }
pub struct Acceptor;   // SPEAKS_FIRST = false
pub struct Initiator;  // SPEAKS_FIRST = true
```

**Written, and it came out narrower than the sketch.** `[measured 2026-08-29]` `Role` is a
sealed trait with two marker types rather than an enum, so the branch is resolved at compile
time and costs nothing at run time. `ActionBuf` does not exist: the four inputs are four
methods — `connect`, `disconnect`, `tick`, `received` — each taking an `emit` closure the
caller supplies, and each answering `Link::{Up, Dropped}`. One input may call `emit` up to five
times; `[measured]` two files in the corpus need five. The buffer the messages are written into
is the session's own, so nothing is borrowed from the input. The rest of this section stands.

**`Deliver` became a trait, and it is the only other public API.** `[measured 2026-08-29]` the
session owns the seven administrative message types (`0 1 2 3 4 5 A`) and hands everything else
to the application:

```rust
pub trait Application {
    fn on_message(&mut self, msg: &[u8], seq: u32, stamp: &[u8], out: &mut [u8])
        -> Option<Range<usize>>;
}
```

It is given the two things an application does not own — the outbound sequence number and the
clock — writes its reply into a buffer the session lends it, and returns the range it used, or
`None` to say nothing. **`None` spends no sequence number.** `received` keeps its signature and
calls `received_with` with an application that never answers, so a session used as a pure
protocol machine is unchanged.

**The initiator needs one thing a pure machine cannot give it, and it is not a state.**
`[measured 2026-08-30]` 46 of the 50 mirrorable acceptance definitions require this end to send
a message nothing on the wire asks for and no clock produces — 42 of them a `Logout`. Neither a
`tick` nor an inbound message can produce one. So the layer grew **six functions that take an
operator's intent**, three of which arrived on 2026-09-02:

```rust
send_heartbeat(emit)                  // 35=0, carrying no 112=
send_test_request(id, emit)           // 35=1, the caller's 112=
send_resend_request(from, to, emit)   // 35=2, the caller's 7= and 16=
send_sequence_reset(n, emit)          // 35=4 with 123=N, and become n
begin_logout(text, emit)              // 35=5, then wait for theirs
send_application(msg, journal, emit)  // anything the session does not own
```

**None of them takes whole message bytes**, and that boundary is a decision rather than a
convenience — [ADR-0042](decisions/ADR-0042-a-second-implementation-is-the-only-independent-opinion.md).
A caller supplies the fields it owns; the session builds the message from its own `Template` and
keeps `8`, `9`, `34`, `49`, `52`, `56` and `10`. The purity above is untouched: every one is a
patch through the single code path that writes `34=` and `52=`, and `benches/alloc.rs` reads
`ordered 0`.

**And the two roles are not symmetric in one place the corpus could never show.**
`[measured 2026-09-02]` the inbound-`Logon` handler answered with a `Logon` for **both** roles.
For an acceptor that is the handshake; for an initiator, which sent the first one, it starts a
second handshake on a session that already has one. The reply is now behind `!R::SPEAKS_FIRST`.
Nothing in this repository could see it — `tests/score.rs` was 59 / 59 because for an acceptor
the line is *correct* — and it was found on the first run of `scripts/interop.sh`, by
`libquickfix` dropping the connection without a word.
[reference/a-role-can-be-wrong-in-a-direction-no-gate-runs.md](reference/a-role-can-be-wrong-in-a-direction-no-gate-runs.md).

**A connection and a session became different things.** `[2026-08-31]`
[ADR-0010](decisions/ADR-0010-a-reconnect-is-not-a-restart.md). `connect` used to reset both
sequence numbers unconditionally, and that is wrong for a real deployment: FIX 4.4 numbers a
**session**, not a connection, so a session that outlives its process must keep counting.
`Session::resume(cfg, next_out, next_in)` builds one that already carries numbers, and
`connect` leaves those alone; a session from `Session::new` has persisted nothing, so it still
resets on every connection.

**The acceptance corpus keeps its meaning by construction, not by exemption.** All seven
`iCONNECT`s across the three files that reconnect expect `34=1` back, because the runner builds
a session per scenario with `new` and a second connection is therefore a second connection to a
session that never persisted anything. **No file is exempted and none needed to be** — the
score is 59/59 unchanged. `[measured 2026-08-31]` forcing `connect` to *never* reset drops it
to **56/59**, which is what proves the corpus exercises that branch rather than tolerating it.

Recovering the numbers is the engine's job; this layer does no I/O and takes them as arguments.
`Session::next_out()` and `next_in()` exist so the engine can persist them, and are not
hot-path accessors.

**Both counts now survive, and the inbound one is written after delivery.** `[2026-08-31]`
[ADR-0017](decisions/ADR-0017-the-inbound-count-is-persisted-after-delivery.md). `Journal` gains
`mark_in(seq)` and `highest_in()`, so one file carries both directions: outbound from the
highest record, inbound from the highest mark. The session calls `mark_in` at the end of
`received_with`, after `judge` and after the held-message drain, so it covers a message
delivered directly and one released when a gap closed.

**The ordering is the decision, not an implementation detail.** Writing the mark *before*
delivery would mean an ill-timed crash loses the message — this end has counted it and will
never ask for a resend, while the counterparty believes it arrived. Writing it *after* means the
message is delivered twice, and the second copy carries `43=Y` because it comes from a
`ResendRequest` this end issued. **FIX has a flag for that failure and nothing for the other**,
and QuickFIX advances its target sequence number after delivery too. The cost is named in
ADR-0017: under `Durability::Fsync` the inbound path now pays a `sync_data` per message, and an
application behind this engine must be **idempotent per sequence number** — `GUIDE.md` §6
carries that, because the type system cannot.

**`Store` became a trait, and the debt is paid.** `[measured 2026-08-30]` a resend has to
replay application messages this end already sent. The session no longer keeps them: it is
handed a `journal::Journal` — `put(seq, bytes)` and `get(seq)` — exactly as it is handed an
`Application`, and `Session::received` supplies `NoJournal` so a pure protocol machine is
unchanged. An emitted `Action::Store` could not have worked, because a resend has to *read*
and an action is one-way; [ADR-0008](decisions/ADR-0008-journal-is-a-trait.md) records that
and what else differs from the sketch. The three D7 tiers are three types in `engine`.

**One machine, both roles.** The acceptor waits for `Logon` and answers; the initiator sends
`Logon` and waits. Sequence handling, resend, heartbeat, test-request and logout are the same
protocol read from the other end. [ADR-0004](decisions/ADR-0004-bidirectional-engine.md)
measured how much differs — 51 of the 59 acceptance definitions mirror unchanged — and
concluded that a session core which cannot invert is a rewrite later, not an extension.

No socket, no clock, no allocation inside. Time arrives as `Tick`. `SessionError` is a
fieldless enum — no `String`, no `format!`, nothing that allocates on an error path.

**Why this is the highest-leverage decision in the design:** the 59 QuickFIX acceptance
definitions become *unit tests*. No listening socket, no timing window, no flake, and they
run in CI in milliseconds. A session layer that is entangled with I/O can only be tested
through a socket, and socket tests are the ones that get muted.

It also makes the engine replaceable without touching protocol correctness.

### D2 — The field index is separate from the message view

Measured, not assumed. Full detail in
[ADR-0003](decisions/ADR-0003-message-representation.md) and
[reference/measured-costs.md](reference/measured-costs.md).

```rust
#[repr(C)]                       // 12 bytes, natural alignment 4. NOT align(16)
pub struct FieldEntry { tag: u32, offset: u32, length: u16, _pad: u16 }

pub struct FieldIndex<const N: usize> { count: u16, fields: [FieldEntry; N] }  // reusable, no lifetime
pub struct MessageView<'a, const N: usize> { buf: &'a [u8], idx: &'a FieldIndex<N> }

/// Incomplete is Ok, not Err: TCP delivers bytes, not messages. Folding "wait for more"
/// into the error branch makes every call site pay to tell it apart from "session is broken".
pub enum Parsed { Complete { consumed: usize }, Incomplete }

pub fn parse_into<D: Dictionary, const N: usize>(
    buf: &[u8], idx: &mut FieldIndex<N>, v: Validation,
) -> Result<Parsed, ParseError>;
```

The caller owns one `FieldIndex` and reuses it for every message on that connection. The
parser never constructs or returns a large struct.

**`MessageView` is 24 bytes, not 16.** `&[u8]` is a fat pointer — 16 bytes — plus 8 for the
index reference. `[measured]` verified with `rustc -O` on 2026-08-27; the earlier "two words"
claim in the first draft of this document and in ADR-0003 was wrong, and both are corrected in
place. The consequence is an ABI one: on x86-64 SysV and AArch64 a struct over 16 bytes is
passed **indirectly**, so any function taking a `MessageView` by value on the hot path carries
`#[inline]` and `crates/codec/src/index.rs` carries
`const _: () = assert!(size_of::<MessageView<64>>() == 24);` so that growing it fails to
compile rather than silently costing a spill.

`N` is a const generic, so the caller chooses: `FieldIndex<64>` for order flow,
`FieldIndex<512>` for a market-data snapshot, same code, no runtime cost. Overflow is
`ParseError::TooManyFields`, never silent truncation. The 64 default is a measurement, not a
preference — see ADR-0003.

**Repeating groups do not change the index, and that is the decision.** The index stays flat:
`parse_into` records tags in wire order and knows nothing about groups. A group is resolved
only when asked for, by `MessageView::group(msg_type, counter)`, which walks the flat entries
and returns a pair of positions.

```rust
pub struct GroupIter<'a, D: Dictionary, const N: usize>;  // Iterator<Item = GroupEntry<'a, N>>
pub struct GroupEntry<'a, const N: usize>;                // get(tag), group::<D>(msg_type, counter)
```

Three consequences, each of which is why it is shaped this way:

- **A message with no group pays nothing**, because none of this runs unless it is called.
  `[measured]` parse is unchanged at 77 ns; walking a group is a separate 29–145 ns depending
  on depth (`benches/groups.rs`).
- **Nothing is built and nothing is allocated.** `benches/alloc.rs` walks four nesting levels
  and reports 0.
- **The scan steps over nested regions.** A group ends at the first tag outside its member
  set, and a nested group's members are not members of the group around it, so a scanner that
  does not skip stops inside the first nested group. `[measured]` 235 of the 731 group
  positions in FIX 4.4 contain a nested group — 32%.

`declared()` (what the counter field says) and `counted()` (what is on the wire) are reported
separately and never reconciled by the codec. Whether a mismatch is a `Reject 373=16` is the
session layer's decision, and it needs the counter tag for `371=`.

### D3 — Field ordering comes from generated tables, never from hand-written code

The QuickFIX acceptance comparator compares fields **positionally**: a correct FIX message
whose fields are in a different order fails. This is recorded as a trap in
[reference/quickfix-acceptance-def-format.md](reference/quickfix-acceptance-def-format.md).

So the serialiser emits in an order derived from the dictionary at build time. Ordering is
never a judgement made at a call site.

**Inside a repeating group the ascending-tag rule does not apply**, and this is the place D3
actually bites. `MsgType` first, then header tags ascending, then body tags ascending governs
the message; a group entry is written in the dictionary's **declaration** order, delimiter
first — `269` before `270`, `279` before `285`. The counter tag itself sorts among the body
tags like any other field; nothing after it sorts at all.

`Template::encode_with::<D>` therefore walks `D::group_order(msg_type, counter)` and never the
order the caller supplied. `[measured]` `crates/codec/tests/group_roundtrip.rs` hands every
entry over in reverse and round-trips 357 top-level positions byte-for-byte.

**A round-trip against your own table proves stability, not correctness**, so the order is
checked against a second implementation: QuickFIX's generated C++ for FIX 4.4, which carries
each group as `FIX::Group(counter, delimiter, message_order(...))`. `[measured]` delimiter
agrees on 730/730 groups and QuickFIX's order is an exact subsequence of this crate's on
730/730 (`crates/dict/tests/interop_quickfix_order.rs`). Swapping two adjacent members in
every group leaves the round-trip green and turns that test red — which is why it exists.

**A DATA field is written immediately behind its length field, and the encoder writes that
length.** A DATA value may legally contain `0x01`, so a reader takes its length from the field
in front and from nowhere else. Two rules follow, and both are refusals rather than advice:
a DATA field declared without its length field fails at `TemplateBuilder::build` — once, at
startup — with `EncodeError::DataWithoutLength`, and inside a repeating group the same case
fails in `encode_with` before a byte is written; and the length's value is computed from the
data rather than taken from the caller, because a caller who can state it can state it wrongly.
`[measured 2026-08-30]` fifteen of FIX 4.4's sixteen DATA pairs have `length == data - 1`, so
ordering by ascending tag was right by accident; `Signature(89)` takes `SignatureLength(93)`
and was emitted before its length. Held by `crates/codec/tests/data_encode.rs`, and by
`group_roundtrip.rs`, which writes **508 DATA members** with a separator inside every value.

### D4 — Dispatch is a trait; inline is the default, the ring buffer is the option

Taken from [Artio](https://github.com/artiofix/artio), which separates `FixEngine` (owns TCP
connections and session lifecycle) from `FixLibrary` (runs business logic) — **but not
adopted as the default.** Full reasoning and the reversal in
[ADR-0002](decisions/ADR-0002-engine-library-split.md).

```rust
pub trait Dispatch { fn deliver<const N: usize>(&mut self, session: SessionId, msg: MessageView<'_, N>) -> Flow; }

pub struct InlineDispatch<H: SessionHandler>(H);   // same thread, zero hops — the default
pub struct RingDispatch { ring: Spsc<Frame> }      // library on its own thread
```

- **`InlineDispatch`** — the handler runs on the engine thread, directly after the session
  machine. Zero handoff, zero copy, the borrowed `MessageView` is handed straight through.
  This is the HFT-standard shape: `recv → parse → decide → encode → send` on one core.
- **`RingDispatch`** — bytes are copied into an SPSC ring and a library thread consumes them.
  Costs a hop (measured, not estimated — see §6) and ends zero-copy at the boundary. Buys the
  one thing inline cannot: **an application that blocks does not stall the session layer.**

The application picks. A simulator serving a QA app that may stall for 40 ms wants the ring.
A gateway that answers in nanoseconds wants inline. Both are the same engine.

**Why the first draft got this wrong:** Artio's split is justified largely by the JVM —
process isolation contains GC pauses. Rust has no GC, so half the motivation does not
transfer, and what remains is a property some applications need and others pay for.

**As built.** `Dispatch` carries a `const OUT_OF_BAND: bool`, and it is `false` for
`InlineDispatch` — so the engine's "collect what the other thread produced" block is behind a
constant and compiles away entirely on the default engine. A reply from the ring comes back
through `Session::send_application`, which means the sequence number and `SendingTime` are the
session's own: an application on another thread cannot get either wrong, because it is never
told them.

A reply is routed by a **connection id, never an index** — the engine drops a dead connection
with `swap_remove`, so an index is stale the moment anything hangs up, and a reply for a
connection that has gone is dropped rather than delivered to whoever took its slot.
`crates/engine/tests/dispatch.rs` asserts that, and asserts the thing that makes the whole
trait worth having: **the same message produces the same bytes on the wire under either
dispatch.** The dispatch chooses a thread, not a protocol.

The ring itself is `Box<[AtomicU8]>` — safe Rust, no dependency, and a byte-at-a-time copy
whose price is published rather than hidden. [ADR-0007](decisions/ADR-0007-spsc-ring-without-unsafe.md).

### D5 — Transport is a trait; TCP is the only implementation that ships by default

```rust
pub trait Transport { fn recv(&mut self, buf: &mut [u8]) -> io::Result<usize>; fn send(&mut self, buf: &[u8]) -> io::Result<usize>; }
```

Two rules, both learned from reading `matthart1983/nanofix`:

1. **A feature flag must gate the module declaration itself** — `#[cfg(feature = "aeron")] mod aeron;`. In that project the flag exists in `Cargo.toml` while `src/lib.rs:1` reads `mod aeron_c;` unconditionally, so `cargo test --no-default-features` fails to link for everyone who does not have Aeron installed.
2. **`build.rs` must not invoke an external toolchain unless that feature is on.** In that project it panics regardless, with the author's own home directory as a fallback search path.

Together those two make the crate unbuildable for anyone but its author. That is the failure
mode to design against, and it costs nothing to avoid.

### D6 — No `panic!`, `unwrap()` or `expect()` in any library crate

Enforced by a workspace clippy lint, not by discipline. The reference implementation carries
**276** of them in `src/`; discipline alone demonstrably does not hold this line.

### D7 — Persistence is a policy, and it is off the hot path

QuickFIX's `FileStore` calls `Sync()` on every write across three files. That single choice
is the dominant latency source in its default configuration.

The session machine emits `Action::Store(seq, bytes)` and knows nothing about how it is
satisfied. The engine appends to a memory-mapped journal under a policy the user chooses:

| Policy | Meaning | Default for |
|---|---|---|
| `None` | Nothing persisted. Resend is impossible | Tests, simulators |
| `Async` | Appended and flushed by a background thread | **The default** |
| `Fsync` | `fsync` before the message is acknowledged | Regulated deployments that require it |

**As built.** Three types rather than three branches:
`fixbolt_session::journal::NoJournal` is `None`, `engine::journal::MemJournal` is a ring that
keeps but does not persist, and `engine::journal::FileJournal` carries
`Durability::{Async, Fsync}`. A `FileJournal` **also** keeps the ring and answers `get` from
it: reading a replay back off disk would be a blocking `read` on the thread non-negotiable 4
protects.

**As built `[2026-09-04]`, and this is what a resend actually costs.**
[ADR-0046](decisions/ADR-0046-the-ring-is-the-resend-store-and-a-replay-goes-in-batches.md)
made the sentence above a decision rather than an implementation detail, and fixed what it was
hiding:

- **The ring is the whole resend store.** Anything older is gap-filled — legal, invisible to the
  counterparty's engine, and now **counted and emitted**:
  `EventKind::ResendBeyondJournal { filled, oldest }`, in messages rather than occurrences. The
  other silence gets one too: `JournalRefused { count }`, for a reply longer than a slot.
- **`SLOTS` is 4096, not 8.** Eight was the smallest power of two above what the corpus asks
  for; an acceptor that had sent a hundred `ExecutionReport`s replayed eight of them.
  `N × (LEN + 8)` ≈ 2 MiB per session — `[measured 2026-09-04]` `tools/w2w` reads
  **+2 195 456 bytes** of RSS against `SLOTS = 8`. `GUIDE.md` §6 has the formula for choosing.
- **`get` is one index and one comparison**, addressed by `seq % N`. The scan it replaced
  returned the *first* slot carrying a number, which after `Admin::SetNextOut` wound the count
  back was the **stale** copy — correctly numbered, correctly checksummed, wrong message.
- **A replay goes out in batches** of `Config::resend_batch` messages, default 8, continued from
  `Session::tick_with` and from each judged message. Before this the whole range went out in one
  call, `Out::push` refused what did not fit, and D10 answered a `ResendRequest` with
  `Logout 58=slow consumer` — **the corpus cannot see it**, because no definition asks for more
  than three messages. `crates/engine/tests/backpressure.rs` is what does.

Two deviations from the paragraph above, both in
[ADR-0008](decisions/ADR-0008-journal-is-a-trait.md). The journal is **a trait the session is
handed**, not an action it emits — a resend has to *read*, and an action cannot answer. And
the file is **appended, not memory-mapped**: `mmap` means a dependency or `unsafe`, and the
engine plan authorises neither, the same fork [ADR-0007](decisions/ADR-0007-spsc-ring-without-unsafe.md)
documents for the dispatch ring.

**A journal is written and never read back.** Nothing recovers a session from the log on
startup, so `Fsync` today is an audit trail rather than a recovery mechanism. That needs a
session constructible *from* a journal, which is its own plan — STATUS open item 16.

**A journal reads back, and until 2026-08-30 it did not.** `Journal::highest()` reports the
highest sequence number held, `FileJournal::open` reads the file before appending, and a record
torn by a process killed mid-write is dropped rather than half-read. The on-disk record carries
its own length — `seq(4) || len(4) || bytes` — because without it records cannot be separated
and the file is append-only by construction, which is what `Durability::Fsync` was paying for
before this. **Two values of the header are not messages and never were**: `len == 0` is
ADR-0017's inbound mark, and `[2026-09-02]` `seq == 0` is ADR-0039's activity mark, eight
little-endian milliseconds saying when the session was last alive. `34=0` is not a sequence
number FIX has, so neither costs a format change, and a file written before either existed
reads exactly as it did. Held by `crates/engine/tests/recovery.rs`, which drops the journal between the
write and the read; a `MemJournal` there would prove nothing.

**What still does not resume is the session.** `Session::connect` resets both counters
unconditionally, so recovered numbers are wiped before anything can use them.
[ADR-0010](decisions/ADR-0010-a-reconnect-is-not-a-restart.md) is `Proposed` and says why that
is a decision rather than an oversight: the acceptance corpus resets on every connect, FIX
numbers a session rather than a connection, and one entry point cannot serve both.

`[2026-09-03]` **The ring is the whole resend store, and eight is its default — read, not yet
fixed.** `Store = MemJournal<8, 512>`, and `FileJournal` answers `get` from its own ring only
(`journal.rs:440–470`), which is the right choice for non-negotiable 4 and the wrong default for
an acceptor: a `ResendRequest` reaching past the ring is answered with a gap fill over
application messages, legally and silently. The same loop emits a whole resend in one call, so a
resend larger than `TX` trips D10 and ends the session as a *slow consumer*. `STATUS.md` item 43,
[plan](plans/2026-09-03-resend-from-the-journal.md), and ADR-0046 is that plan's step 0. Nothing
in this section is changed by the finding yet; it is recorded here so this page does not describe
a default as though it were sized for a deployment.

**And the journal is not a message log.** It keeps outbound application messages for resend and
one inbound number (ADR-0017). Neither direction's administrative traffic, and no refused frame,
is written anywhere — `STATUS.md` item 44, [plan](plans/2026-09-03-message-log.md).

### D8 — In `hft` the engine thread busy-polls; in `standard` it blocks

`[amended 2026-08-30, ADR-0013]` **This decision is mode-scoped, and `standard` is the
default.**

| | `standard` — the default | `hft` — opt-in, Linux only |
|---|---|---|
| Idle behaviour | **blocks on readiness** with a timeout, and gives the core back | spins on non-blocking sockets, never enters the kernel |
| Cost of a wakeup | `epoll`-class, 2–5 µs | `[measured 2026-08-31]` one turn at **449 ns per session** on a §9 core — ~670 on one carrying `nohz_full`, which [ADR-0021](decisions/ADR-0021-nohz-full-leaves-section-9.md) removed from §9 |
| Pinning | none | the polling thread is pinned to an isolated core |
| Runs on | any OS, any hardware, a container, a laptop | a machine that satisfies §9 |
| Rule 4 says | it **must** block | it must **not** sleep |

`[2026-08-31]` **"pinned" is now something the code can do rather than something this
paragraph asserts.** `fixbolt_engine::affinity::pin_current_thread` pins the calling thread and
confirms it with `sched_getaffinity`, and `tests/affinity.rs` watches the scheduler's own
`processor` field while the thread works — reversal: with the pin removed the same thread was
observed on **cpu0, cpu4 and cpu5** in one run.

`[2026-08-31]` **the rest of that plan landed, and it closed all six steps.** `Topology` and
`ShardPlan::validate()` refuse a core that is absent, offline, named twice, an SMT sibling of
another in the plan, or — for shard cores — outside `isolcpus`, **before any thread exists**;
`affinity::spawn_pinned` and `journal::FileJournal::open_pinned` give the threads this crate
starts a home and report back the core they landed on. **And the engine shards**:
`shard::Shards` runs one pinned engine per core and `[2026-09-01]` routes each socket by the
identity in its `Logon` rather than by accept order
([ADR-0020](decisions/ADR-0020-a-pre-session-stage-owns-the-socket-until-logon.md)), which is
what makes the acceptance corpus score 59 through two shards.

**What is left of open item 21 is one entry point**: `serve_hft` pins nothing. It is the
single-threaded convenience and it spawns no engine thread of its own, so the thread that calls
it is the caller's to pin — stated in `GUIDE.md` §9 rather than left for a reader to discover.

**In `hft`,** the engine thread is pinned to an isolated core and spins on non-blocking
sockets. No `epoll_wait`, no condition variables, no futex on the hot path.

**Why:** an `epoll` wakeup costs 2–5 µs *and* brings scheduler jitter with it. On a design
whose entire user-space path is under 1 µs, a blocking wait is the single largest cost the
engine controls. It burns a core — that is the price, and in `hft` it is worth paying.

**Why `standard` exists, and why it is the default:** an engine whose out-of-the-box
configuration pins a core at 100% is one most people cannot evaluate — it looks broken. And
`[measured 2026-08-30]` the spin is not free even in `hft`: the poll it replaces `epoll` with
costs `[measured 2026-08-31]` **449 ns per session per turn** on a core set up to §9,
so the trade **wins at N = 1 and loses by N = 8** — a conclusion the re-measurement did not
move, because 8 × 449 ns is 3.6 µs and still clears the top of `epoll`'s range.
`standard` is the honest default for everything that is not one session on an isolated core.

`Waiting` is a trait and is the right seam, but **`wait::Yield` is not `standard`**: it is
`std::thread::yield_now()`, which yields the scheduler and **does not block**, so it still
burns its core. It is `[renamed 2026-08-30]` from `Park` for exactly that reason, and its
rustdoc now says it fails **both** gates rather than sitting beside `Spin` as a peer.

A real `standard` mode blocks on readiness, which needs the sockets — so
[ADR-0014](decisions/ADR-0014-standard-mode-blocks-on-poll.md) hands the source list to
`Waiting::idle` and has `Transport` name its own descriptor, rather than splitting idling
across two traits. The mechanism is **`poll(2)` through `libc`, behind a default-on `standard`
feature**; `epoll` is O(1) where this is O(N) and is a later ADR **with numbers**, because the
difference at `standard`'s shape is unmeasured. On a target with no poller `wait::Block` does
not exist, so the refusal is a compile error rather than a startup one.

`[2026-08-30]` **The seam and the blocking strategy are built; the wiring is not.** `Source`,
`Interest`, `Transport::POLLABLE`/`source()`, `poll::Poller` and `block::Block` exist and are
tested. `Block` blocks on readiness at a **100 ms** timeout — which is a correctness parameter,
not a knob, because a session with no clock sees time only through `Input::Tick` and in
`standard` that timeout is what delivers it.

**The source list is built too.** One interest per connection — readable always, writable only
while that connection still has bytes queued, because a socket is almost always ready to accept
bytes and asking unconditionally would wake the engine continuously. It is rebuilt every turn,
never cached: a `Source` borrows a descriptor, and one kept across a turn can name a socket that
has since closed and been reissued. `serve` hands the listener over, so a connection is accepted
on the connect rather than on the next timeout. Pairing a blocking strategy with a transport
that cannot name a source — `Loopback` — **does not compile**.

**And the waker.** `poll` wakes for descriptors and not for a ring buffer, so a reply produced
on the application's thread would wait out the whole timeout; a self-pipe closes that. The
engine holds the read end itself and **drains it after every wait**, because a pipe with an
unread byte stays readable and an undrained one makes every subsequent `poll` return instantly
— a working engine, burning a core, which is the single thing this mode exists to avoid.

**Both modes are now reachable end to end.** `serve` is `standard` and `serve_hft` spins, both
over one shared loop; `tools/w2w --mode hft|standard|yield` prints the mode it ran and
`scripts/check-no-kernel-sleep.sh` reads that back rather than trusting the flag it passed. Its
red half is now `standard`'s real `poll`, because nobody writes `sched_yield` into an engine by
accident and somebody might well reach for `poll`.

`[measured 2026-08-30]` first figures, on a **shared 4-vCPU container that is not a §9 machine**:
`hft` p50 17.7 µs, `standard` p50 29.0 µs, `yield` p50 18.2 µs. Not publishable, and one thing in
them is worth reading anyway — **`standard`'s p50 is three orders of magnitude below its 100 ms
timeout**, so it is woken by the data and not by its own clock. The §8 row below keeps its "from
the literature" label until a §9 machine says otherwise.

**And rule 4's second half is now machine-checked.**
`scripts/check-standard-gives-the-core-back.sh` asserts four things at once, because CPU near
zero is passable by three different broken engines: the mode the binary reports, its CPU over a
wall-clock window, its scheduler state sampled to tell sleeping from dead, and the round-trip p50
against the poll timeout. `[measured 2026-08-30]` an engine made to ignore readiness reads **0%
CPU**, is found sleeping **20 of 20 samples**, and has a p50 of **99 046 599 ns** — one whole
timeout. Only the fourth assertion sees it.

`[measured 2026-08-30]` **The 59 definitions pass in `standard` too**, with the engine blocking
between steps — `cargo test -p fixbolt-engine --test wire`, second case.

**What is left needs a machine this repository does not have running.** The `standard` wakeup
cost has to be measured where `scripts/check-machine.sh` says `pass 10 fail 0`, and the row in §8
below keeps its *from the literature* label until then. It joins open items 6, 11 and 13, which
are waiting on the same desk.

**As built.** `Engine::turn` is one non-blocking pass over every connection — flush what is
queued, **tick the clock**, read once, cut whole messages out, judge them, flush again — and
`Engine::run` is `loop { if !turn() { wait.idle() } }` and nothing else. Reading *once* per
turn rather than until the socket is empty is deliberate: a counterparty that writes faster
than this end processes must not be able to starve the other connections on the thread.

**The tick comes before the read, and that is a correctness ordering rather than a taste.**
`Session::received_with` takes no clock — D1 — so it judges `SendingTime` against the last
instant a `tick` gave it, and a session that has never ticked holds zero. Reading first means
the very first message on a connection is judged against 0000-01-01 and refused for skew.
`[measured 2026-08-30]` moving the tick left the wire gate at 59 / 59.

Keeping the pass separate from the loop is what lets the 59 acceptance definitions run
**through a real socket** with no background thread, no sleep and no timing window —
`crates/engine/tests/wire.rs` drives `turn` by hand and is as deterministic as the in-process
gate.

**Non-negotiable 4 has no machine check yet, and that is stated rather than glossed.**
`dtruss` is refused by macOS SIP, and the substitute — reading undefined symbols out of the
compiled rlib — fails its own reversal, because `Engine` and `serve` are generic and are
therefore never code-generated into the library at all. **Closed 2026-08-30**:
`scripts/check-no-kernel-sleep.sh` traces `tools/w2w` — a concrete binary, on Linux, with the
syscalls attributed to the engine thread by tid — and the script's own second run swaps
`wait::Spin` for `wait::Park` and fails if that does not trip it. §6 has the row.

### D9 — Outbound messages are templates: a pre-sorted parts list, patched, not built

An `ExecutionReport` from a given session has a fixed skeleton: `BeginString`, `SenderCompID`,
`TargetCompID`, `MsgType`, and the field order (D3). That skeleton is encoded **once per
session per message type**, into a scratch buffer **the template owns**. It cannot borrow one:
the bytes are per-session and must outlive any single send.

```rust
enum Part { Static(Range<u16>), Slot(u32) }        // ranges into the template's own scratch
pub struct Template<const P: usize, const S: usize> {
    scratch: [u8; S], parts: [Part; P], len: u8,
}
pub fn encode(&self, out: &mut [u8], slots: &[(u32, &[u8])]) -> Result<Range<usize>, EncodeError>;
```

Three properties that are not obvious, and that the first sketch of this decision got wrong:

- **The parts are sorted at build time** (D3), so `encode` walks them in order and never makes
  an ordering judgement. A slot the caller does not supply is skipped, so one template serves
  messages that differ in their optional fields.
- **The body is written first; the prefix is then written right-aligned in front of it.**
  `BodyLength` is variable-width, so writing the prefix first would mean shifting the whole
  body once its width is known. That is why `encode` returns a `Range` and not a length — the
  message does not begin at `out[0]`.
- **`SendingTime` is the hidden cost.** Naive formatting is 50–100 ns, as much as a whole
  parse. The `YYYYMMDD-HH:MM` prefix is cached and re-derived once a minute; only `SS.sss` is
  formatted per message.

This is how the fastest commercial engines are reported to reach tens of nanoseconds per
serialise. **That figure was once §6's published serialise target, 60 ns, and ADR-0016
withdrew it** — it described other people's software, no machine here came within 1.5× of it,
and `[measured 2026-08-31]` the measured floor of this `Part` shape is ~116 ns even with the
slot scan removed entirely. The cached-timestamp design above is still right and still cheap;
what was wrong was borrowing somebody else's number to grade it against.

### D10 — TCP send backpressure has a stated policy

**Two ends fall behind, and they are different questions with different answers.** D10 is about
*the counterparty on the wire*; **D10b below is about our own application behind the ring**, and
conflating them is what left the second one unanswered from ADR-0002 until ADR-0011. The tell is
whose fault it is: on the wire, a counterparty that cannot keep up is broken; behind the ring,
the counterparty is faultless and we are the ones who stopped reading.

A slow counterparty fills the socket send buffer, and `send` returns `EAGAIN`. At 50,000
`ExecutionReport`/s against a QA application this **will** happen. The engine must not block
the session machine and must not drop protocol messages silently.

Policy, per session, chosen in configuration:

| Policy | Behaviour |
|---|---|
| `Queue { max_bytes }` | Buffer in a per-session outbound ring up to a bound, then… |
| `Disconnect` | …drop the session with a `Logout(text="slow consumer")`. **The default** — a FIX counterparty that cannot keep up is a broken counterparty |
| `Block` | …spin until the socket drains. Available for tests; never the default |

The queue is its own storage. It is tempting to say the queued bytes are the ones the journal
(D7) already holds — but under `JournalPolicy::None` the journal holds nothing, and that is the
policy simulators and tests run. The queue owns a per-session buffer, sized at startup.

**As built.** `Backpressure` on `Engine` or on a single `Connection`; the queue is the
connection's `TX` buffer and `Queue { max_bytes }` only tightens the bound. Three rules the
code makes explicit and `crates/engine/tests/backpressure.rs` holds:

- **A message goes in whole or not at all.** The session emits one message per `emit` call, so
  a refusal is always at a message boundary; a queue that wrote as much as would fit would put
  a frame on the wire that the counterparty cannot recover from.
- **The Logout that says `58=slow consumer` is not subject to `max_bytes`.** It is written into
  the whole `TX` buffer after the queue is discarded — the queued messages are for a
  counterparty that stopped reading, and the one message that matters must not be the one that
  cannot be sent.
- **A socket that has died ends the connection even with bytes queued.** `[measured 2026-08-30]`
  before this, killing the socket mid-write left the connection `Up` for as long as it was
  turned, because "finished" was defined as *closing and the queue is empty*.

`Block` spins rather than sleeping, so D8 still holds for the thread — but one slow
counterparty then stops every other session on it, which is why it is never a default.

### D10b — a full ring to the application ends the connection

The other end of D10, and
[ADR-0011](decisions/ADR-0011-a-full-ring-disconnects.md) is the decision.

Under `RingDispatch` (D4) the application is on another thread. If it stops draining, the ring
fills, and until 2026-08-31 the answer was a counter: `RingDispatch::refused()` went up and
nothing read it. **A message counted there is one the session accepted, numbered, journalled and
acknowledged by sequence number, that the application never saw** — for order flow that is not
backpressure, it is silent loss.

**As built**, and it is D10's shape with one deliberate difference in each direction:

| | D10, the wire | D10b, the ring |
|---|---|---|
| Whose fault | the counterparty's | ours |
| `58=` text | `slow consumer` | **`slow application`** — a different constant on purpose, so neither the counterparty nor an operator reading two logs is told the wrong side is at fault |
| The queue | **discarded first**, so the Logout has room | **kept**, because the socket is draining perfectly and those messages will go out |
| `Block` offered | yes, for tests | **no** — see below |
| Default capacity | `TX`, the caller's | `ring::DEFAULT_CAPACITY`, **4 MiB** |

**`Block` is not offered on this side.** On the socket, spinning until there is room is
defensible because the peer is draining. Spinning until an *application thread* drains makes the
engine thread's progress depend on code the engine does not control, and non-negotiable 4's gate
(`scripts/check-no-kernel-sleep.sh`) cannot tell a spin that finishes from one that does not.

**How the dispatch says so, since the session layer's `Application` trait cannot.** `deliver` is
reached through `fixbolt_session::Application::on_message`, which returns
`Option<Range<usize>>` and belongs to the pure session layer (D1, non-negotiable 2). So the
signal is a separate, defaulted method on `Dispatch`:

```rust
fn take_refusal(&mut self) -> bool { false }
```

The engine asks **immediately after one connection's turn**, and a `true` belongs to that
connection because the adapter that called `deliver` was built for its id and nothing else ran
in between. No id is carried and nothing is stored. `InlineDispatch` takes the default, so the
branch folds away like `Dispatch::OUT_OF_BAND` already does — the commonest engine pays nothing.

**Two costs, stated rather than discovered later.** 4 MiB resident per ring, which multiplies if
a deployment ever gives each connection its own; and an application that pauses longer than the
ring holds now drops the session rather than lagging. `[measured 2026-08-31]` 4 MiB gives **5.05–5.36 ms**
of slack over four runs on the §9 desktop — 22 550 messages — against 47.7 µs at the old 64 KiB.

**That figure replaces a multiplication, and the two disagree by 48%.** ADR-0011 derived
"roughly 3.6 ms" by scaling the 64 KiB rate and said in its own revision note that the true
value lay somewhere in 1.6–3.6 ms and should be read as an order of magnitude. Measuring it
puts it **above** that whole range, because the per-message cost goes **135 → ~230 ns** once the
buffer stops fitting in cache: a ring that fills more slowly gives the application *more* time,
not less. The decision is unaffected and its margin is larger than it claimed —
[reference/measured-costs.md](reference/measured-costs.md).

Still true: **no real application has ever stalled against this ring**, so both the policy and
the capacity come from one synthetic saturation run plus reasoning about order flow. ADR-0011's
open questions 1 and 3 — whether the ring should be per-connection, and whether the slack is
enough — are still open and need an application nobody has yet.

### D11 — TLS is a transport implementation, and the guarantee is stated per mode

Decided in [ADR-0005](decisions/ADR-0005-tls.md). It needs a decision at all because of one
collision: the codec parses in place at the I/O buffer, and **encrypted bytes cannot be parsed
in place.** Userspace TLS reintroduces exactly the copy
[ADR-0003](decisions/ADR-0003-message-representation.md) spent its length removing.

| Mode | When | Hot-path guarantee |
|---|---|---|
| Handshake — `rustls`, userspace | Once per session, before any message flows | **Allocation permitted.** A named, bounded carve-out from non-negotiable 1 — to the handshake, not to the connection |
| Steady state — **kTLS** | Linux, and a cipher suite the kernel carries | **Met.** The kernel delivers plaintext into the read buffer; the D8 spin loop and parse-in-place both survive unchanged |
| Steady state — userspace `rustls` | macOS, older kernels, unsupported suites | **Not met, and the documentation says so in those words.** One copy each way, and it allocates. A number measured in this mode is never quoted as the engine's number |

`cargo build --no-default-features` produces a binary with no TLS code and no crypto
dependency at all (D5, and CI proves it on a machine with neither installed).

**Verified 2026-08-31, and it was load-bearing.** `ktls-core` *can* be driven from a plain
non-blocking socket with no async runtime, so the kTLS row above is measured rather than
reasoned. The `tokio`-shaped documentation belongs to `ktls`, a different crate; `ktls-core`
0.0.5 has no async feature and every entry point is generic over `AsFd`. `strace -f` over 1000
round trips, attributed to the thread driving the socket, shows `recvfrom` and `sendto` and
nothing else. **It costs four conditions** — every read error goes to
`ktls_core::Context::handle_io_error`, the transport never reads the socket outside the offload,
the handshake hands over with an empty buffer, and `setup_ulp` needs an `ESTABLISHED` socket.
Written up in [reference/ktls-on-a-plain-socket.md](reference/ktls-on-a-plain-socket.md),
decided in [ADR-0018](decisions/ADR-0018-ktls-on-a-plain-socket-answers-adr-0005.md), gated by
`scripts/check-ktls-on-a-plain-socket.sh` with a red arm. **A TLS plan is now unblocked and is
still not written.**

**Still unverified:** which kernel version and which cipher suites are the floor (ADR-0005 open
question 2), whether a session survives a TLS 1.3 key update under kTLS (open question 6), and
what asserts which of the three modes above is actually live (open question 3). The spike pinned
one kernel and one suite *so that* its answer would be attributable, and that pinning is exactly
the limit of what it covers. **The §8 TLS row stays empty**: the spike published no latency
number, and one is not to be inferred from the syscalls being the same ones.

### D13 — `Tick` counts milliseconds from year zero, not from the Unix epoch

D1 says time reaches the session only as `Input::Tick`. What that number *means* had been left
open, and the obvious answer is wrong.

`SendingTime` is `YYYYMMDD-HH:MM:SS[.sss]` — four year digits, so the wire can name any instant
from 0000 to 9999. **Counted from 1970 in a `u64`, more than a fifth of that range does not
exist.** A counterparty sending `52=19600101-00:00:00` would wrap the skew subtraction into a
difference of half a billion years: it fails no check, but it crosses one, and the failure is
silent.

So `Tick` and every parsed `SendingTime` are **milliseconds since 0000-01-01T00:00:00Z**,
proleptic Gregorian. Every timestamp FIX can express is then a non-negative `u64`, the skew is
a plain `abs_diff` that cannot wrap, and the session needs no signed arithmetic. The engine
converts once at the edge: `tick = unix_millis + clock::MILLIS_YEAR_ZERO_TO_EPOCH`.

The cost is one added constant at the edge, and one asymmetry to remember:
`codec::TimestampCache` still takes Unix milliseconds, because it is `no_std` and shared with
callers that have no session. Bridging the two is the session's job, not the codec's.

### D14 — the message log is a second file, written by the journal's pattern, and it records refusals

`[2026-09-04]` The journal answers *"what did we send, numbered `seq`"*, which is what a
`ResendRequest` needs and is D7's whole job. It cannot answer the first question a desk asks in
a dispute: *"at 10:32:07, what did we receive, and what did we turn away?"* Inbound frames are
kept as a number and no bytes (ADR-0017), and a frame refused before the session saw it —
a wrong `56=`, a duplicate identity, a `Cut::Garbage` — disappears the moment the connection
ends.

**It is a second file and not an extension of the journal, because the journal's key is
`seq`.** Every method of the session-layer `Journal` trait takes or returns one, `MemJournal`
addresses `slots[seq % N]`, and the three things this file exists for have no sequence number
at all. `Journal` is also a `session` trait, and a pre-session refusal is by construction bytes
the session never sees, so merging would put in `session` something D1 forbids it to know.
`Durability::Fsync` blocks the engine thread deliberately and a diagnostic must never; one file
cannot serve two durability policies without branching on record kind inside the loop that is
not allowed to branch. The full argument, with the cost counted both ways, is
`reference/why-the-message-log-is-not-the-journal.md`.

**The mechanism is ADR-0007's, unchanged**: one `Producer::push` per message per direction into
a ring, a writer thread that formats and appends, and losses dropped and counted rather than
waited for — ADR-0011's rule, pointed the other way, because a log that blocks the engine is
worse than a log with a hole. The writer is allowed to allocate, for the reason
`journal::Reader` is (ADR-0037).

**`NoLog` is the default and it compiles away.** `MessageLog::LOGS` is an associated constant,
so an engine that was never given a log carries no branch, no field and no cost — the same way
`Dispatch::OUT_OF_BAND` folds out the out-of-band block.

**What it costs the engine thread, and it is not measured yet**: one ring copy per message per
**direction**, ~1.7 ns/byte, so ~340 ns for a 200-byte message and ~680 ns for a request/reply
pair. `[unproven]` — arithmetic from §6's dispatch row, not a measurement of this module. §8
carries the same caveat. What **is** measured is that it allocates nothing on that thread:
`benches/alloc.rs` cases `log-record` (0 over a thousand records), `log-idle` and `log-busy`.

**`OUT` means queued, not sent, and the gap is counted.** The line is written when a message
reaches the outbound buffer, which is the only moment the engine can name it; a dying socket
discards that buffer. `EventKind::MessageLogUnsent { bytes }` says how much of that
connection's tail the file is wrong about — the line cannot be un-written, and a dispute
artefact that overclaims sends is wrong in the worst direction.

## 5. Non-goals for v1

Stated so that scope creep has to argue with a document. The full list with phases is
[PRD.md §5](PRD.md); this is the subset that shapes the architecture.

- FIX 5.0 / FIXT 1.1 — phase 2, and it arrives together with SBE, because SBE messages are
  versioned by `ApplVerID`.
- SBE, FAST, FIXML — phase 2, and **an encoding ADR comes first**: `MessageView` presupposes
  tags on the wire and SBE has none, so the question is whether there is one view type or
  several (PRD §2).
- Kernel bypass — DPDK, OpenOnload, `ef_vi`. Not before an ordinary TCP path has been measured
  and found to be the limit. §8 puts that limit at 10–20 µs. The path is decided even though
  the work is not: Onload first, because D8's spin loop and the socket API survive unchanged;
  `ef_vi` second, as an `impl Transport` behind a D5-style flag; DPDK never, because it has
  no TCP stack. It is plaintext only, so it and D11 exclude each other.
  [STATUS.md](../STATUS.md) open item 14.
- SIMD delimiter scan and checksum. Not until `benches/parse.rs` on Linux shows the parse on
  the critical path — `matthart1983/nanofix` has SIMD and parses 4–6× slower, because layout
  beat it ([reference/measured-costs.md](reference/measured-costs.md)).
  [STATUS.md](../STATUS.md) open item 12.
- Clustering, HA, replication.
- Metrics dashboards and web UIs.
- Matching engine, order book, risk. This is a protocol engine.

**No longer a non-goal:** the initiator side.
[ADR-0004](decisions/ADR-0004-bidirectional-engine.md) moved it into phase 1 on the finding
that the two roles differ by about one enum's worth of behaviour, and that a session core
which cannot invert is a rewrite later rather than an extension.

## 6. Gates

Each is a committed benchmark or test, named. **A target without a runnable gate is a wish.**

| Gate | Target | Proven by |
|---|---|---|
| Parse `NewOrderSingle` | **no regression past this machine's baseline** (ADR-0016). `[measured 2026-08-31]` §9 desktop: **122.6 ns** validated, **117.0 ns** raw, **57.3 ns** `Heartbeat`, medians of 24 qualifying runs | `benches/parse.rs`, against `benches/baselines.tsv` |
| Serialise `ExecutionReport` (template, D9) | **no regression past this machine's baseline** (ADR-0016). `[measured 2026-08-31]` §9 desktop **239.1 ns**, median of 24 qualifying runs. **The 60 ns absolute target is withdrawn** — it was never a measurement of this engine, only of what the fastest commercial engines are reported to reach (§4 D9), and no machine ever came close: 93.8 (M5) · 177.6–199.4 (container) · 239.1 (§9 desktop) | `benches/serialize.rs`, against `benches/baselines.tsv` |
| `RingDispatch` hop vs `InlineDispatch` | measured and published, whatever it is, per machine (ADR-0016). `[measured 2026-09-01]` §9 desktop: inline **8.5 ns** (median of 22 qualifying runs), ring **267.4 ns** one way and **515.7 ns** round trip (24 runs), on a 163-byte `NewOrderSingle` — **the ring hop is ~31x the inline call**, and ~1.7 ns of every byte of it is the `AtomicU8` copy ([ADR-0007](decisions/ADR-0007-spsc-ring-without-unsafe.md)). **The inline figure was published as 1.3 ns for a day and that number was the optimiser deleting the 163-byte copy** — `out` was written every iteration and read by nobody. 163 bytes in 1.3 ns is 125 GB/s from one core, which is the arithmetic that found it; the earlier reading of 6.3 ns was the honest one all along, and the harness change that "sped it up" had removed the indirect call that was keeping the stores alive. [a-benchmark-can-delete-its-own-work.md](reference/a-benchmark-can-delete-its-own-work.md) | `crates/engine/benches/dispatch.rs`, against `benches/baselines.tsv` |
| Allocations on the hot path — codec | **0** | `crates/codec/benches/alloc.rs`, counting allocator |
| Allocations on the hot path — session | **0**, counted separately on **sixteen** paths: accept, refuse, tick, beat, answer, gap, fill, deliver, resend, logon_out, originate, ordered, clock, text, schedule-open, schedule-shut | `crates/session/benches/alloc.rs`. The refusal path is counted apart because it is the one a hostile counterparty controls, and it is where a `format!` is easiest to reach for. `beat` and `answer` are the two the session decides on by itself — a heartbeat that came due, and a reply to a `TestRequest`. `ordered` is the three an **operator** orders: `send_heartbeat`, `send_test_request` and `send_resend_request`, all three taking a value from outside the session, which is the shape that tempts a `to_vec()`. `[measured 2026-09-02]` injecting one reads `ordered 10000` |
| Every `373` code the corpus asks for is actually produced | **12 / 12**, read out of the corpus's own `E` lines | `crates/session/tests/score.rs`. The file count cannot say this: `14a_BadField.def` holds four cases and a session answering all four with the same code still passes the file |
| The session rules the corpus cannot tell apart | each has a test of its own | `crates/session/tests/logon.rs`, `tests/reject.rs` and `tests/heartbeat.rs`. `[measured]` seven so far. Three from steps 1–3: deleting the "first message must be a Logon" check leaves the score unchanged, because `1e_NotLogonMessage.def` also carries a wrong `56=`; stamping `52=` from a constant leaves it unchanged, because `52` is one of the five tags `fields.fmt` matches by shape; a Reject that gives the inbound sequence number back leaves it unchanged, because the *too high* branch does not exist yet. Four from step 4: all three heartbeat thresholds, which the harness's whole-interval ticks cannot see; and that a garbled frame is fatal only when it claims to be a Logon, which the corpus states once from each side in different files. Five from step 5, in `tests/resend.rs`: every file that opens a gap ends before opening a second one, so closing a filled gap, replaying held messages in sequence order, and what happens when there is no room to hold one are all invisible to the score |
| Session conformance, acceptor | **59 / 59** | `cargo test -p fixbolt-session --test score`, in-process, no socket. `[measured 2026-08-29]` **59 / 59** — the session plan is closed |
| The journal keeps what a resend needs, under each D7 policy | `None` fills over everything; `MemJournal` and `FileJournal` replay; a message longer than a slot is refused rather than truncated | `crates/engine/tests/journal.rs`, seven tests. Reversal: making `put` keep nothing turns four of them red **and drops the acceptance score**, which is what proves the score depends on the journal |
| **A resend larger than the transmit buffer does not end the session** | 100 messages of ~200 bytes, `TX` 8 KiB, `Backpressure::Disconnect`: all 100 come back with `43=Y`, in order, and no `58=slow consumer` | `crates/engine/tests/backpressure.rs::a_resend_larger_than_tx_does_not_end_the_session`, ADR-0046. Reversal: `with_resend_batch(10_000)` reads *"the session ended on turn 1 while answering a resend"* — **and `--test score` stays 59 / 59 through it**, which is what says the corpus is blind to this |
| **A resend past the ring is counted, and reaches an operator** | 20 orders through an 8-slot ring, `7=2 16=0`: one `ResendBeyondJournal { filled: 12, oldest: Some(14) }`, and `events_lost() == 0` | `crates/engine/tests/events.rs::a_resend_past_the_ring_is_an_event_with_the_numbers`. One event per turn that changed, not one per message — ADR-0035 |
| Session conformance, acceptor, **through a real socket** | **59 / 59, on every machine** | `cargo test -p fixbolt-engine --test wire`. The same files over TCP: kernel sockets, the real framer, the real session, the real application. The only injected part is the clock, because every `I` line in the corpus carries a fixed instant. `[measured 2026-08-30]` **59 / 59 on the M5 and on Linux x86_64** — **met**. It read 39 / 59 on Linux until the harness's client socket was given `TCP_NODELAY`, which the engine's own sockets have always had; the gate is now flat across a 20× span of its timing bounds, which is what makes the figure mean something |
| **A `standard` engine gives the core back** | engine-thread CPU under 5% over a wall-clock window, found sleeping rather than running, **and** a round-trip p50 far below the poll timeout | `scripts/check-standard-gives-the-core-back.sh`, non-negotiable 4's second half. Four assertions, because CPU near zero is also what a dead thread, a run that never reached the mode, and an engine woken by its own timeout all report. `[measured 2026-08-30]` a `Block` made to ignore readiness reads **0% CPU**, sleeping **20/20**, p50 **99 046 599 ns** — only the p50 catches it. Requires **`hft` and `yield`** to trip it, and separates *failed the policy* from *could not be measured* so a broken harness cannot pass as a red half |
| Session conformance, acceptor, **in `standard` mode** | **59 / 59** with the engine blocking between steps | `cargo test -p fixbolt-engine --test wire`, second case. ADR-0013's stated cost — *two modes is two things to test, for ever* — and the only place the corpus meets `standard`, since every other line of that file drives `turn` by hand where the idle strategy is never reached. **It proves the protocol, not the wiring**: `[measured 2026-08-30]` with `Block` made to ignore readiness, and again with the listener removed from the poll set, the run took 3.30 s and 3.34 s against a 3.28 s baseline — the settle criterion is 1 ms and the timeout 5 ms, so one block satisfies it either way |
| **The engine thread never sleeps in the kernel** | no blocking syscall on that thread | `scripts/check-no-kernel-sleep.sh`. Traces `tools/w2w` with `strace -f` and attributes calls to the engine thread by tid — the client blocks on purpose and would mask everything. `[measured 2026-08-30]` Linux 6.18 x86_64: `accept4`, `recvfrom`, `sendto` and **zero** of `epoll_wait`/`poll`/`select`/`futex`/`nanosleep`/`sched_yield`. **The script runs the binary again with `wait::Park` and fails if that run does *not* trip it** — non-negotiable 4 had two machine checks before this one and both were green with a sleep present |
| Session conformance, acceptor, **through the shard runtime** | **59 / 59 through one shard and through two** | `cargo test -p fixbolt-engine --features affinity --test shard_wire`. Two pinned threads, two engines, connections routed by the identity in their `Logon`. `[measured 2026-08-31]` it read **57 through two** and the two failures were named and pinned; `[measured 2026-09-01]` **59 / 59** — [ADR-0020](decisions/ADR-0020-a-pre-session-stage-owns-the-socket-until-logon.md). **And the score is not the whole gate**: the test also counts how the pre-session stage disposed of every socket, because a connection it dropped is indistinguishable from a duplicate the session refused. Exactly two are, by name — [ADR-0022](decisions/ADR-0022-the-pre-session-stage-enforces-two-definitions.md) |
| Allocations on the hot path — engine | **0**, counted separately on **twenty-four** paths: idle, send, recv, frame, turn, shard-turn, busy, ring, interests, pending-idle, pending-busy, pending-cycle, registry-lookup, observe-idle, observe-asked, events-idle, events-busy, admin-idle, admin-busy, shutdown, reconnect, log-record, log-idle, log-busy `[added 2026-09-04]` | `crates/engine/benches/alloc.rs`, counting allocator. `busy` is a whole turn carrying a message in and a reply out, and it asserts the session is still logged on at the end of the count — `[cost]` an earlier version measured a connection that had been dropped at message two and reported the test double's queue growth as the engine's. **`log-record` is the targeted one**: it calls `MessageLog::record` a thousand times with no engine and no writer thread in the window, so a number there cannot be blamed on the harness or on the writer — `[measured 2026-09-04]` making `record` allocate once reads **1000**. `log-busy` drives a whole turn with a real `FileLog` attached, and its consumer is held rather than drained so every allocation it counts is an engine-thread one by construction; the writer thread is allowed to allocate (ADR-0037) and the counting allocator is global. **What no bench here proves** is that the writer allocates nothing while the engine runs — `tools/w2w` is where a both-threads number belongs |
| **A journal on disk says when its session was last alive** | the instant survives the process that wrote it, the **latest** mark answers, and a file with no mark reads exactly as it did | `crates/engine/tests/on_disk.rs`, six tests, one of them through a **real socket** and the serving loop. Reversal: `mark_active` writing nothing turns **three** red. The sentinel is read by two decoders and reversing only one reads `5 passed; 1 failed` — flipping both reads `3 passed; 3 failed`, and the no-mark control stays green either way ([a-reversal-that-must-not-compile](reference/a-reversal-that-must-not-compile.md)) |
| **The serving loop does not require the journal to have a `Default`** | putting the bound back **must not compile** | `crates/engine/tests/on_disk.rs::serving`, which uses a `FileJournal` — a type that has no honest `Default`. `[measured 2026-09-02]` restoring `J::default()` in `pump` gives `error[E0599]`. **No runnable test can hold this claim**: both versions behave identically for every type a test can name |
| **Counterparties come out of a configuration file** | two named only in a file both log on **through a real socket**; one the file does not name gets nothing; one whose window closed two hours ago is refused while one open now is served | `crates/engine/tests/settings.rs` (30 tests) and `tests/settings_wire.rs` (4, one of them a `#[should_panic]` control proving the harness can tell a **closed** socket from a **hung** one — without it a hung acceptor reads as a refusal, which is the same defect the row below records). Reversals: ignoring an unknown key turns **1** red, accepting a file with no `[SESSION]` turns **2**, letting `[DEFAULT]` win over `[SESSION]` turns **1**, and dropping the parsed schedule turns **5** with the no-hours control green. `[measured 2026-09-02]` a reversal *not* in the plan — keeping only the file's first counterparty — left all three wire tests green, because an unserved identity and a closed window are the same silence; the fix asserts the **registry's** length at the moment it is handed to the acceptor, and two of the three then go red ([two-time-rules-share-one-observable](reference/two-time-rules-share-one-observable.md)) |
| **An initiator comes back after its counterparty hangs up** | the loop dials again, and a policy that says stop opens no socket at all | `crates/engine/tests/reconnect_wire.rs`, over a real `TcpListener` that answers one `Logon` and closes. Two reversals, **orthogonal**: ignoring the policy reddens the stop control and leaves the reconnect test green; never coming back reddens the reconnect test and leaves the control green. `[measured 2026-09-02]` the first reversal originally made the suite **hang** — a loop that never sees `Stop` never returns, and `cargo test` was killed at ten minutes — so the control now runs on a thread with a deadline and fails in 10 s with a sentence ([a-reversal-can-fail-by-hanging](reference/a-reversal-can-fail-by-hanging.md), second instance) |
| The reconnect ladder, with no I/O and no clock | doubling to a ceiling that holds; `logged_on` resets it; a shut venue outranks it | `crates/engine/tests/reconnect.rs`, 8 cases, [ADR-0043](decisions/ADR-0043-backoff-without-jitter-and-a-reconnect-asks-recovery-every-time.md). **Every case here is invented** — no corpus in this repository covers reconnect, so unlike `tests/score.rs` it measures this engine against this project's own reading. Same weakness `crates/dict/tests/field_types.rs` carries and says so. The ordering reversal was a **no-op** until the assertion moved to an instant where the two orderings disagree ([a-reversal-needs-an-input-where-the-answers-differ](reference/a-reversal-needs-an-input-where-the-answers-differ.md)) |
| The conformance runner can tell right from wrong | a fake that replays each file's own expected output scores **59 / 59** | `crates/conformance/tests/fix44.rs`. Without it `0 / 59` would also be what a broken runner reports |
| **The initiator, against a real `libquickfix`** — this engine dials out | **7 / 7** — logon · application messages in · an unprompted heartbeat · a `TestRequest` with this end's own `112=` · a `ResendRequest` answered by replay at the numbers asked for · a gap this end opens and gap-fills · logout | `scripts/interop.sh` and the blocking `interop` CI job. **Phase 1 exit criterion 4**, [ADR-0042](decisions/ADR-0042-a-second-implementation-is-the-only-independent-opinion.md). Builds QuickFIX from source at the same commit `fetch-quickfix-assets.sh` pins, and refuses to run if the two pins have drifted. **It reads the transcript, not the exit code** — a binary that dies before printing and one that prints seven failures both exit non-zero. `[measured 2026-09-02]` its **first** run found that this engine's initiator answered a `Logon` with a `Logon`, a defect green in `--test score` at 59/59, in `--test mirror` at 0/50 as asserted, and in 430 other tests ([a-role-can-be-wrong-in-a-direction-no-gate-runs](reference/a-role-can-be-wrong-in-a-direction-no-gate-runs.md)). Reversal 1 red at 2/7; **reversal 2 was a no-op** until the resend step stopped accepting any `43=Y` and started naming the sequence numbers it wanted back ([a-resend-answer-has-two-legal-shapes](reference/a-resend-answer-has-two-legal-shapes.md)) |
| **The acceptor, against a real `libquickfix`** — this engine listens | **7 / 7** — logon with `141=Y` echoed · two `35=D` answered by two `35=8` paired on `11=` · an unprompted heartbeat carrying **no** `112=` · a `TestRequest` · a `ResendRequest` answered by replay **at the two numbers asked for** · a gap the counterparty opens · logout | `scripts/interop.sh` and the same blocking `interop` CI job — one script, both directions, because two scripts are two `PINNED_SHA`s. **This is the differentiator's first independent opinion**: until 2026-09-04 the acceptor's whole evidence was 59 `.def` files read by this repository's own runner, and ADR-0042's sentence applied to the half of the engine that is not the product (`STATUS.md` item 42). Unlike the initiator row, what is under test here is the **whole stack** — `fixbolt::serve`, the poller, the pre-session table, the settings file, the library `Handler`. `[measured 2026-09-04]` its **first** run was red on `gapfill`, and the red was the test's: it waited for an answer to the `TestRequest` that opened the gap, and QuickFIX's own gap fill covered that sequence number ([a-gap-fill-can-swallow-the-question](reference/a-gap-fill-can-swallow-the-question.md)). Three reversals run and each red on its own step; the inverted-range one that was a **no-op** in the initiator direction bites here |
| Session conformance, initiator, mirrored corpus | **10 / 50** `[measured 2026-09-02]`, and **ADR-0006's ceiling of 45 is in doubt** — at least six of the rest need `SequenceReset` shapes no operator API should expose, and the other 34 are unclassified, **and the harness's own drive count asserted beside it** | `cargo test -p fixbolt-session --test mirror`. **The secondary gate, and it cannot check its own reading** — mirroring is this project's interpretation of a suite written for the other direction, and its ceiling moved 51 → 50 → 45 across two readings. It asserted `passed == 0` for three days, and a gate pinned at a constant reports nothing about the code under it: an initiator that answered a `Logon` with a `Logon` went through it. It went 0 → 2 → **10** in one session, and the last jump was two real defects it found: a session that said goodbye first answered the acknowledgement with a *third* `Logout`, and `begin_logout(b"")` wrote an empty `58=`. Neither was visible to the acceptor corpus. The score is asserted **by file name**, and next to it `Report::driven` — every time the harness played the operator, by `MsgType`, as exact numbers, because a score a harness can raise by driving harder is not a score. Reversals: `make_receivable` neutered takes it to 0 / 50 and the drives from 141 to 1; the origination removed takes it to 1; and letting the **acceptor** corpus be driven makes `tests/score.rs` panic, which is the guard that keeps a harness from making a broken session look correct |
| Repeating groups — read | every group **found**, to the full nesting depth of 4, at all **731** positions the dictionary declares | `crates/codec/tests/groups.rs` — reading is done; writing is not |
| Repeating groups — written | parse → encode **byte-identical** at all **357** top-level positions, exercising all **59** counters and nesting to depth 4 | `crates/codec/tests/group_roundtrip.rs` |
| Every tag number matches another implementation | **912 / 912** against QuickFIX's `FixFieldNumbers.h`, **and** 5 168 field names whose tag FIX 4.4 does not define are refused | `crates/dict/tests/interop_quickfix_fields.rs`. The negative half is what stops `is_defined_tag` being `true` for everything |
| Every field type matches another implementation | **898 / 912** exact, **14** differences each named by tag with both spellings | same test. QuickFIX's `FixFields.h` is shared across versions and carries later refinements; the XML is the source of truth (ADR-0001) |
| Every (message, tag) pair matches another implementation | **12 524 / 12 524** body pairs, checked as **84 816** answers — every message against every tag, both directions | `crates/dict/tests/interop_quickfix_messages.rs`. One acceptance definition covers this table; 84 816 answers do |
| Every enum value is one QuickFIX also knows | **245 / 245** fields, **1 708 / 1 708** values, zero exceptions | `crates/dict/tests/enums.rs`. One-directional by construction: QuickFIX lists every version's values, so it can only confirm, never forbid |
| What each of the 23 field types accepts | at least one accepted and one refused value per type | `crates/dict/tests/field_types.rs`. **These cases are invented** — the corpus supplies two, and 23 types with 2 real cases is not coverage |
| In-group field order matches another implementation | delimiter exact on all **730** groups, and QuickFIX's `message_order` an exact subsequence of this crate's member list on all 730 | `crates/dict/tests/interop_quickfix_order.rs`, read out of QuickFIX's generated C++. Exists because the round-trip test reads the same table the encoder does and is blind to a wrong order |
| **Wire-to-wire, over loopback, on a §9 box** | p50 / p99 / p99.9 published with the machine and its settings. `[measured 2026-09-02]` **MET** — §9 desktop, `pass 12  fail 0  unknown 1`, engine pinned to isolated `cpu6` and client to `cpu7`, medians of 20 whole runs of 20 000 round trips: `hft` **16 010 / 20 589 / 22 127 ns** administrative, **19 908 / 24 657 / 26 150** application; `standard` **19 447 / 24 106 / 25 609** and **20 920 / 25 618 / 27 092**. **p99 ≤ 50 µs holds in all four arms.** Allocations in the timed window **0**, asserted by the binary on both threads | `tools/w2w --features affinity`, driven by `scripts/w2w-baseline.sh`. Phase 1 exit criterion 6 |
| **Wire-to-wire, NIC to NIC** | the same three percentiles with a **real NIC** in the path | `tools/w2w` — `SO_TIMESTAMPING`, HdrHistogram, load generator on a **separate machine**. **NOT met, and the row above does not close it**: loopback has no driver, no IRQ, and no wire, which is why §9's NIC IRQ affinity row reads `unknown` for every figure quoted above. `STATUS.md` open item **40** |
| Which TLS mode is actually in force | a session that fell back to the userspace path is **detected**, not assumed | ADR-0005 open question 3 — **no gate exists yet, and that is a known hole** |
| `parse_into` never panics on hostile input | `[measured]` 304,230,294 executions, 0 crashes, 2026-08-28 | `fuzz/fuzz_targets/parse.rs`, `cargo +nightly fuzz run parse` |
| **kTLS can be driven from a plain non-blocking socket** | 15 assertions green, **and** the offloaded data path makes no blocking syscall | `scripts/check-ktls-on-a-plain-socket.sh`, D11 and [ADR-0018](decisions/ADR-0018-ktls-on-a-plain-socket-answers-adr-0005.md). Runs `spikes/ktls`, then traces it and attributes syscalls to the thread that drove the socket by the tid that wrote the marker. `[measured 2026-08-31]` `recvfrom` 3033 + `sendto` 1000 over 1000 round trips and **nothing else**. **Runs a second time with `poll(2)` in the same loop and fails if that run does not trip it.** Skips with exit 2, not a pass, on a kernel that cannot offload TLS |
| The lint config denies `unwrap` / `expect` / `panic` | red on a crate carrying all three, green once they are gone | `scripts/check-lint-config.sh`, run in CI on every push |
| Builds with nothing optional installed | `--no-default-features` on a clean runner (non-negotiable 6) | `.github/workflows/ci.yml`, its own job. **`[measured 2026-08-30]` the workspace-wide command alone is not enough**: `cargo test --all --no-default-features` still built `libc`, because `tools/w2w` depends on `fixbolt-engine` with defaults and cargo unifies features across one invocation — the flag under test was switched back on by a sibling crate. See [reference/feature-flags-unify-across-a-workspace.md](reference/feature-flags-unify-across-a-workspace.md) |
| An optional dependency is really optional | absent from the crate's graph with no features on, **and** the crate still builds and tests that way | `scripts/check-no-optional-deps.sh`, run by the same CI job, **per crate** — the only scope where `--no-default-features` means what it reads as. Reversal: removing `optional = true` from `libc` turns it red with the graph printed |
| No documentation link points at a missing file | `[measured 2026-09-02]` **971 internal links** across 244 files resolve | `scripts/check-links.py`, run in CI |
| `unsafe` blocks | each names what proves it sound | code review + Miri |

The wire-to-wire row is the only one that measures what a counterparty experiences. Every
other row is an internal number; without this one they are unfalsifiable.

**Most of these rows run today.** `[2026-09-04]` **both interop rows do** — one blocking CI
job, both directions, and they are the only two rows in this table whose opinion did not come
from this repository. The ones that still do not are the wire-to-wire figures, which need a machine
matching §9, and the TLS-mode gate, which does not exist.

`[2026-09-01]` **and one thing in CI is worth reading as a hazard rather than as a feature.**
Every crate-dependent job in `.github/workflows/ci.yml` is still guarded by
`if: steps.ws.outputs.count != '0'`, with a warning annotation for the case where the workspace
holds no crates. That guard was written when it held none. **It now holds six**, so the only
way that branch can be taken is a broken checkout or a broken manifest — and if it ever is, the
job goes *green* having run `fmt`, `clippy` and `test` on nothing, behind an annotation nobody
reads. That is the shape `STATUS.md` open item 25 names — **a check that passes because it
measured nothing** — and it is the fourth instance, after a benchmark whose work the optimiser
deleted, an allocation guard whose window excluded the operation, and a machine setting outside
§9 that made every number faster. It has not fired and it is **not** fixed here; it is item 26,
written down so the next person to touch that file makes `count == 0` fatal instead of
advisory, and proves it by emptying the workspace members and watching CI go red.

### Timing gates are per machine, not absolute — ADR-0016

**There is no single published nanosecond target any more, and that is a decision rather than
an omission.** Every timing row above is judged against the figure *this project measured for
that case on the CPU it is running on*, times that case's own margin. Both live in
[`benches/baselines.tsv`](../benches/baselines.tsv), each line carrying its sample size, its
date, and the `scripts/check-machine.sh` verdict of the run that produced it.

Two findings retired the absolute column.

**The 60 ns serialise target was never a measurement of this engine.** §4 D9 says where it
came from: it is how the fastest commercial engines are *reported* to perform. That is a
different kind of number from the 150 ns parse target, which was anchored to 139 ns measured
here on an Apple M5 on 2026-08-27. One column held both, and only the second kind can gate
anything. No machine came close to 60: **93.8** (M5) · **177.6–199.4** (shared Xeon
container) · **239.1** (§9 desktop). The plan that went looking for the missing time found
that ~31 ns is spent before the first variable field is written — 51% of the whole target on
a message carrying nothing — and that removing the slot scan *entirely* still leaves ~116 ns.

**The ceilings had the same disease from the other side.** `STATUS.md` open item 20 measured
`ring, one way` at 260.9 ns on a Ryzen 7 3700X, 270.7–272.9 on an EPYC 9V74 and 327.2–331.1 on
an EPYC 7763 — 21% between two machines of one vendor, against ~1% within either. The single
260 ns ceiling sat 0.3% *below the fastest of the three*: a ceiling no machine passes is a
ceiling somebody switches off.

**Recorded baselines**, medians of **24 qualifying `scripts/bench.sh` runs**, `[measured 2026-08-31]` — measured through the whole invocation that judges them, on a box reading `pass 10 fail 0` for every run counted:

| Case | AMD Ryzen 7 3700X (§9, `pass 11 fail 0` — ADR-0021) | margin |
|---|---|---|
| parse `NewOrderSingle` (validated) | 122.6 ns | 1.10 |
| parse `NewOrderSingle` (no checks) | 117.0 ns | 1.15 |
| parse `Heartbeat` (validated) | 57.3 ns | 1.10 |
| encode `ExecutionReport` (template) | 239.1 ns | 1.10 |
| `SendingTime` from the cache | 4.9 ns | 1.10 |
| walk 1 group, 2 entries | 58.7 ns | 1.10 |
| walk 4 levels, 61-tag member list | 352.9 ns | 1.10 |
| `group_members` contains, 61 tags | 9.7 ns | 1.10 |
| encode 1 group, 2 entries | 108.4 ns | 1.10 |
| inline deliver + reply | 8.5 ns | 1.10 |
| ring, one way | 267.4 ns | 1.30 |
| ring, round trip | 515.7 ns | 1.20 |
| recv on a quiet socket | 420.5 ns | 1.10 |
| engine turn, 1 idle sessions | 448.9 ns | 1.10 |
| engine turn, 4 idle sessions | 1807.1 ns | 1.10 |
| engine turn, 16 idle sessions | 7333.5 ns | 1.10 |
| presession sweep, 1 quiet sockets | 435.9 ns | 1.10 |
| presession sweep, 16 quiet sockets | 6819.5 ns | 1.10 |
| presession, read and route an identity | 84.0 ns | 1.10 |

**No other machine has a baseline, and none is invented for it.** The Apple M5 and the two CI
EPYCs have figures scattered through this file and `STATUS.md`, but none was taken by the
procedure above — N ≥ 20 whole runs on a box reading `pass 10 fail 0` — so none is written
into the file. Those machines report `NO BASELINE`, which is counted on its own summary row
and is **not** a pass.

**The margin is per case because one margin cannot work.** `[measured 2026-08-31]` nine of the
twelve cases hold inside 7.6% of their own median across a run set, while `ring, one way`
draws a second mode at **+24%** on roughly 1 run in 15 — the mode open item 20 could not
explain after refuting five hypotheses, and which appeared here on the *quietest* runs of the
set rather than the loaded ones. A single margin wide enough for that case would let `encode
ExecutionReport` drift 236 → 319 ns unnoticed.

**Stretch, and it is NOT a gate:** serialise at **~116 ns** on the §9 desktop. That is the
measured floor of the current `Part` shape — what remains after removing the slot scan
completely — and reaching it needs the ~31 ns fixed prefix cost and the ~7 ns per field in
`put` attacked, not the scan. It is written here as an ambition with a measurement behind it,
in place of a target with somebody else's product behind it.

### How the benchmarks are run

Nothing ran them until 2026-08-30. `cargo test --all` does not run a `harness = false` bench
target — measured: 43 test binaries, not one of them a bench — and no CI job called
`cargo bench`. Every ceiling in the rows above, and every allocation count, was an assertion
no machine executed. `benches/alloc.rs` is what `CLAUDE.md` §2 names as the machine check for
non-negotiable 1, so that entry named a check nothing invoked.

`scripts/bench.sh` runs every bench target and the `bench` CI job runs the script on every
push. **It splits the two kinds of benchmark by what decides their result**, because they
cannot share an exit code:

| | What it measures | On a failure |
|---|---|---|
| **Invariant** — `alloc` × 3, `ring_full` | allocation counts, message counts | **CI red.** The answer is the same on every machine, so a failure is a defect |
| **Timing** — `parse`, `serialize`, `groups`, `dispatch`, `turn`, `presession` | ns/op against **this machine's own band**, `[baseline / margin, baseline * margin]` ([ADR-0016](decisions/ADR-0016-per-machine-baselines-replace-absolute-targets.md), [ADR-0031](decisions/ADR-0031-a-baseline-is-a-band.md)) | **Reported, never red on a shared runner.** `bench.sh --strict`, which a §9 machine runs, is fatal on a case with no baseline for its CPU **and on one that came in under its floor** — a benchmark that stops measuring reads far under its limit, and a ceiling alone passes it forever |

`--strict` makes a timing failure fatal too; that is what a §9 machine should use. The script
also fails when a target produces **no** measurement, which is not hypothetical: Cargo had
auto-discovered `benches/harness.rs` — a module, containing no case — as a ninth bench target
that reported `0 measured` and exited 0.

**Timing ceilings are not enforced on a shared runner because they cannot be.**
`[measured 2026-08-30]` Five runs of the twelve timing cases on a 4 vCPU Xeon container:
run-to-run spread 5–232%, three cases flip colour between runs, and **not one case exceeds its
ceiling in all five**. The same commit on the CI runner — AMD EPYC 7763, **2 cores**, run
33304774414 — puts **six of the twelve over**, `ring, one way` at 328.3 ns against 188.5–233.2
on the container. Across four CI runs that case ranges **270.7–331.1 ns**, while the
single-threaded cases hold to ~3%: the spread follows **whether a case crosses threads**, not
just which machine it ran on. A gate that goes red at random gets switched off. Re-tuning waits for §9 —
STATUS.md open item 20.

**Every case is measured and printed before any is allowed to fail.** The harness used to
assert inside each case, so the first one over its ceiling ended the process: a single 17.8 ns
outlier on a 3.7 ns baseline threw away both `ring` figures on that run, and `groups` had a
fourth case — `encode 1 group, 2 entries`, over its ceiling — that **nobody had ever seen**,
because the process died two cases earlier.

**There is no published target distinct from the asserted limit any more** — ADR-0016
collapsed the two into one per-machine number, for the reason this paragraph used to describe
from the other end. The old arrangement existed because 139 ns sat 8% under a 150 ns target
while the laptop it ran on varied by more than 8%, so asserting the target would have gone red
at random; the fix was to assert something looser and publish the target anyway. That kept a
number in the table that nothing checked. Now the limit is the band `baseline / margin` to
`baseline × margin` for the CPU at
hand, both recorded from measurement, and the table publishes exactly what the benchmark
asserts.

`[measured 2026-08-31]` **A baseline must be taken through the path that will judge it.** The
first attempt recorded medians from running the four timing targets directly; `encode
ExecutionReport` then went over its 1.10 limit on the first `scripts/bench.sh` run, because
`bench.sh` runs eight targets and the case is not measured in the same state. The recorded
figures come from 20 full `bench.sh` runs. A related trap sits next to it: back-to-back runs
leave the previous suite inside `check-machine.sh`'s own one-second window, so an unspaced loop
reads 25–36% busy and disqualifies its own measurements.

## 7. Build order

Each step is a plan, a branch, and a merge. Nothing starts before its predecessor is green.

1. **`codec` + `dict`** — parse, serialise, generated tables. Gated by the parse/serialise and
   allocation benchmarks. [Plan](plans/2026-08-27-codec-dict.md), approved.
2. **Repeating groups** — `GroupIter` over the flat index, plus `<component>` recursion in
   `dict`. [Plan](plans/2026-08-27-repeating-groups.md), approved. Immediately after step 1,
   because without it the codec cannot read an application message.
3. **`conformance`** — the `.def` runner. 1–2 days; needed regardless of anything else.
4. **`session`, acceptor role** — the pure state machine, driven to **59/59**.
5. **`session`, initiator role** — `Role::Initiator` against the mirrorable definitions, then
   interop against `libquickfix` in CI. A separate step because the *oracle* differs, not
   because the code does (ADR-0004). **Paused after its own step 2, 2026-08-30**, and step 6
   taken first: the mirrored gate turned out to top out at 45 of 50 and to measure framing
   rather than protocol wherever the harness has to play the operator. The initiator speaks
   first and can originate; the rest waits. See
   [the plan](plans/2026-08-29-session-initiator.md) Sửa 2 and `STATUS.md`.
6. **`engine`** — busy-poll TCP acceptor **and connector** (D8), journal, both dispatchers
   (D4), backpressure (D10).
7. **`tools/w2w`** — the wire-to-wire harness, run against step 6 on Linux **before** step 8. `tools/jrnl` is beside it, `[2026-09-02]`, and needs no machine to run.
8. **`library`** — the public API and the first end-to-end example. `[2026-09-02]` **done**,
   as `crates/library`, package `fixbolt`. `examples/acceptor.rs` is the example and
   `examples/shared/order_handler.rs` is the handler it runs; `tests/end_to_end.rs` pulls in
   **that same file** with `#[path]` and drives it through a kernel socket, so the example is
   run rather than merely compiled. The example names nothing from `fixbolt_engine` or
   `fixbolt_session`, which is the facade's own test. What it cost is
   [ADR-0041](decisions/ADR-0041-the-library-layer-buys-an-api-with-a-template-per-message.md).

Step 3 before step 4 is deliberate: the gate exists before the thing it gates. Step 7 before
step 8 for the same reason.

**TLS (D11) has no step here, because it has no plan** — it is blocked on ADR-0005 open
question 1. When it lands it belongs beside step 6, in `transport`.

## 8. Latency budget on kernel TCP

Where the time goes for one inbound `NewOrderSingle` → outbound `ExecutionReport`, Linux,
kernel TCP, no bypass.

`[measured 2026-09-02]` **The bottom line is no longer arithmetic over a borrowed
denominator.** `tools/w2w` has run on a machine matching §9 and the round trip is measured;
what is still from the literature is labelled row by row, and the four rows the kernel owns are
the ones that still are.

| Round trip, measured | `hft` | `standard` |
|---|---|---|
| `TestRequest` → `Heartbeat` (no application) | p50 **16 010** · p99 **20 589** · p99.9 **22 127** ns | p50 **19 447** · p99 **24 106** · p99.9 **25 609** ns |
| `NewOrderSingle` → `ExecutionReport` (through an application) | p50 **19 908** · p99 **24 657** · p99.9 **26 150** ns | p50 **20 920** · p99 **25 618** · p99.9 **27 092** ns |
| **if `FileLog` is on**, added per message **per direction** — a request/reply pair pays twice | ~340 ns *`[unmeasured]`* | ~340 ns *`[unmeasured]`* |

§9 desktop, `check-machine.sh` `pass 12  fail 0  unknown 1`, engine pinned to isolated `cpu6`
and the client to `cpu7`, medians of 20 whole runs of 20 000 timed round trips each, **over
loopback**. `scripts/w2w-baseline.sh` is the procedure and
[measured-costs.md](reference/measured-costs.md) carries the whole reading. **Loopback is not a
NIC**: §6's NIC-to-NIC row stays open, and these figures contain no driver and no interrupt.

**`hft` is worth 3 437 ns — 17.7% — against `standard` on the identical path**, and that
difference is D8's entire case. It also prices the wakeup row below at **~3.9 µs** (the delta
plus the ~449 ns sweep it replaces), which lands inside the 2–5 µs this table has carried from
the literature since it was written.

Stage by stage:

| Stage | Typical | Who controls it |
|---|---|---|
| NIC → kernel → socket buffer | 3–8 µs | Kernel, IRQ affinity, driver |
| TLS record decrypt, **if enabled** — kTLS **vs** userspace (D11) | in-kernel with AES-NI, no extra copy **vs** one copy each way plus allocation | **This design**, and the kernel |
| Wakeup — **`standard`** blocks on readiness | 2–5 µs, `epoll`-class, **and the core is given back** | **This design**, D8 |
| Wakeup — **`hft`** busy-polls | `[measured 2026-08-31]` **`Engine::turn` itself: ~449 ns × N**, N = sockets on the thread, **and a core is burned**. The 2026-08-30 figure of 703 ns was a C program's bare `read` on a `nohz_full` core; matched for placement the two agree to 4%. **`nohz_full` — and only `nohz_full`, not `isolcpus` or `rcu_nocbs` — adds ~200 ns to every kernel entry on the core that has it and ~45 ns on every core that does not, taking this row to ~670 ns**, which is why §9 no longer asks for it ([ADR-0021](decisions/ADR-0021-nohz-full-leaves-section-9.md), [measured-costs.md](reference/measured-costs.md)) | **This design**, D8, `benches/turn.rs` |
| Parse (D2) | `[measured 2026-08-31]` **0.12 µs** (§9 desktop, 122.6 ns) — **framing, field indexing, `9=` and `10=` only.** `benches/parse.rs` parses with **`NoDict`**, so this row does **not** include the dictionary pass the session runs on every inbound message: required fields, field types, enum values, unknown tags, group structure, once per field and once per required tag. **That pass has no benchmark** — `STATUS.md` open item **39**, found by the wire measurement below | This design |
| Session machine (D1) | ~0.1 µs | This design |
| Dispatch — inline **vs** ring (D4) | `[measured 2026-09-01]` **0.0085 µs** inline **vs** **0.27 µs** ring one way (§9 desktop) — **was published as 0.0013 µs until the benchmark was found to be deleting a 163-byte copy** ([a-benchmark-can-delete-its-own-work.md](reference/a-benchmark-can-delete-its-own-work.md)); D4 is unaffected, 31× is still an enormous gap | Application's choice |
| Serialise — template (D9) | `[measured 2026-08-31]` **0.24 µs** (§9 desktop, 239.1 ns) — **was ~0.05 µs from the literature; see below** | This design |
| `send` syscall → NIC | 3–10 µs | Kernel |
| **Floor** | **~10–20 µs** | Kernel |
| **User-space work only** | `[measured 2026-08-31]` **~0.46 µs**, inline dispatch, N = 1 | The half that was always cheap |
| **Everything this design controls, N = 1** | **~0.91 µs** — the row above plus one turn (**~1.13 µs** if the core carries `nohz_full`) | |
| **Everything this design controls, N sessions** | **`~0.46 µs + N × 449 ns`** (`+ N × 670 ns` with `nohz_full`) | |

`[measured 2026-08-31]` **The poll row is now `Engine::turn` rather than a floor, and it
carries a number nobody expected.** `crates/engine/benches/turn.rs` measures the real sweep over
real sockets: **449 ns per session**, flat from 1 to 16 to within 2%, of which **~420 ns is the
`recv` syscall and ~30 ns is everything else the engine does** — measured in the same run, so
the subtraction is not across programs.

**And the same benchmark under `taskset` reversed a piece of §9's advice.** The core this
document used to recommend was **36% slower** at that syscall: 680 ns against 498 ns, with
`cpu5` and `cpu6` in the *same* L3 domain to show it is not cache placement. `[measured
2026-08-31]` a second boot then separated the three isolation options and found **`nohz_full`
carries all of it** — `isolcpus` 494.8 ns, `rcu_nocbs` 498.2 ns, untouched 501.8 ns,
`nohz_full` 670.7 ns — and measured what it buys: it is worse at p50, p99 **and p99.9**, and
ahead only from p99.99. §9 now keeps the two that are free and prices the one that is not
([ADR-0021](decisions/ADR-0021-nohz-full-leaves-section-9.md)). **Rebooting without it then
moved the untouched cores too**: naming any CPU in `nohz_full` costs ~45 ns per kernel entry on
*every* CPU, so the four figures above each carry that as well and the row settled at 449 ns
with 24 fresh baseline runs — `[measured 2026-08-31]`, [measured-costs.md](reference/measured-costs.md).
It was a trade that is stated
rather than assumed. [measured-costs.md](reference/measured-costs.md) carries the four arms.

`[measured 2026-08-31]` **Four rows stopped being literature figures, and one of them got
4.8× worse in the process.** Serialise was carried here as **~0.05 µs**, which was the 60 ns
target of §6 — a figure describing how the fastest commercial engines are *reported* to
perform, not this engine.
[ADR-0016](decisions/ADR-0016-per-machine-baselines-replace-absolute-targets.md) withdrew it,
and the measured number on the §9 desktop is **239.1 ns**, the median of 24 qualifying runs.
Parse and inline dispatch moved the other way or barely at all. The user-space total below is
recomputed from the measured rows rather than the borrowed ones.

**What this does to the bottom line: nothing that matters, and that is the point.** The
user-space rows total **~0.46 µs** at N = 1 with the inline dispatcher — parse 0.123 + session
~0.1 + dispatch 0.0085 + serialise 0.239 — against a kernel floor of **10–20 µs**. So serialise
costing 239 ns rather than 60 ns moves the wire-to-wire figure by under **2%** of the floor,
which is why §6 was able to withdraw the target without the design changing.

`[measured 2026-09-02]` **and that division now has a measured denominator: 0.46 µs against a
16.0 µs `hft` round trip is 2.9%.** The sentence above was arithmetic over a literature floor
for as long as this document has existed, and the measurement agrees with it. **It is also the
whole of [ADR-0045](decisions/ADR-0045-parse-is-under-one-percent-of-the-wire-and-simd-is-declined.md)**,
which declines SIMD/SWAR on it: parse is **0.62%** of the application round trip, and the 20–40 ns
such work was estimated to save is **0.10–0.20%** — smaller than the 0.5% run-to-run spread of the
instrument that would have to see it. **The measured app round trip is 3 898 ns above the
administrative one and the committed benchmarks account for ~320 ns of that**; the gap is
recorded as a gap, with the dictionary pass named as the largest untested candidate — open
item 39. **The ring
dispatcher is the row worth reading**: at 0.27 µs one way it is more than parse and serialise
put together, and it is the application's choice rather than this design's.

**Which mode the stage-by-stage table is about: `hft`.** `[amended 2026-08-30, ADR-0013]`
`standard` is the default and its wakeup row is the 2–5 µs one, so **its bottom line is
`epoll`-class and the stage table does not describe it**. A `standard` figure and an `hft`
figure are not comparable and must not be quoted as if they were — ADR-0013 decision 4.
`[measured 2026-09-02]` **`standard`'s wire-to-wire round trip is measured and is in the table
at the top of this section**; what is still unmeasured is its *stage breakdown*, because the
wakeup is one opaque term and nothing here decomposes it. The two modes **are** comparable as a
difference, which is the one thing a difference is for: **3 437 ns, 17.7%**, for a whole core
burned permanently per polling thread.

`[measured 2026-08-30]` **the last two rows are the honest bottom line and the "< 1 µs" line
alone was not.** It counted user-space work and excluded the syscall that reaches the socket —
which this design chose, and can change by batching it, removing it, or carrying fewer sockets.
**At N = 2 the polling sweep alone exceeds the whole user-space budget.**
[ADR-0012](decisions/ADR-0012-latency-first-and-one-session-per-polling-thread.md) settles what
follows from that: one session per polling thread is the shape this table describes, `density`
is a labelled mode carrying the `N × 449 ns` term, and **no latency figure is published without
its `N`**.

The TLS row has no number in it on purpose: none has been measured here, and none is quoted
from elsewhere either. It gets filled in when `tools/w2w` runs the same load three ways — TLS
off, kTLS, userspace `rustls` — on the same Linux box (ADR-0005 decision 5).

Two readings of this table:

1. On kernel TCP, this engine's user-space path is **2.9% of the total** — `[measured
   2026-09-02]` 0.46 µs of a 16.0 µs `hft` round trip, and until that day this line said
   "under 5%" on a literature floor. The design makes that 2.9% as small as it can be, and —
   through D8 — trades `epoll`'s wakeup for a 449 ns poll, **which is measured end to end at
   3 437 ns, 17.7%**. `[measured 2026-08-31]` **that trade wins at N = 1 and loses by N = 11**, which
   is the sentence this table did not contain until the poll was measured — and which survived
   the poll being re-measured against the engine rather than against a C floor.
2. Going below the floor means kernel bypass (OpenOnload, DPDK, `ef_vi`). That is L0's
   job, behind a feature flag that actually gates (D5), and it is **not v1**.

## 9. Deployment — the OS is part of the design

"Rust has no GC" does not mean "no jitter". p99.9 on a correct engine is usually lost to
the machine, not the code. None of this is optional for a latency measurement to mean
anything:

| Setting | Why |
|---|---|
| **The machine is not a guest** | Four rows below — governor, turbo, C-states, SMT — plus NIC IRQ affinity are **host** properties. A VM cannot set them, and does not fail them loudly: the `/sys` files are simply absent, so a guest collects `unknown` and reads as under-configured rather than as structurally unable to comply. So this is a row of its own, and it decides whether the rest can mean anything. `check-machine.sh` reports `systemd-detect-virt` and steal time over the same window as the row below; a guest is a **FAIL**, and steal on bare metal is reported as unexplained rather than resolved either way. **Development may move to a cloud VM; measurement cannot.** Bare metal, or nothing but counts and same-machine A/B |
| **Nothing else is running on the machine** | `[measured 2026-08-30]` **the row that was missing, and it dominates every other row here.** On the project's Ryzen 7 3700X, all six tuning rows below move the `ring, one way` median by **0.8%** — 260.6 ns untuned to 259.7 ns tuned, both on a quiet box. Competing CPU load moves it by **71%**, 262 ns to 449 ns, and takes the rate of a second mode near 324 ns from ~5% to **92%**. A machine can satisfy every other line in this table and still be useless to measure on. `scripts/check-machine.sh` now reads CPU busy over a one-second window and **FAILs above 3%**, naming the processes by their delta in that window |
| `isolcpus` + `rcu_nocbs` for the engine core, **and the engine thread pinned to it** | No other tenants, and no RCU callbacks on the engine core. `[measured 2026-08-31]` **free**: `Engine::turn` reads 494.8 ns on an `isolcpus` core and 498.2 ns on an `rcu_nocbs` core against 501.8 ns on an untouched one. `[measured 2026-09-02]` **and no longer free of a benefit either: it is worth 11× at p99.9 and nothing at p50.** Wire-to-wire, `hft`, application path, one variable — whether `isolcpus` names the core, with both arms inside one CCD so L3 placement is held: p50 **19 968 against 19 407** (the isolated core is 2.9% *slower*), p99.9 **26 300 against 266 887**, and **293 749** with no pinning at all. This row used to say the benefit was unmeasured; what could not see it was the instrument — a 20 000-sample benchmark of a 500 ns operation has no readable p99.9, and the excursion is 250 µs long. [measured-costs.md](reference/measured-costs.md), and **the opposite verdict to `nohz_full` below** |
| **CPU speculation mitigations IN FORCE** | `[measured 2026-09-01]` **the single largest term in this design's budget, and it is not the syscall.** Turning them off makes every syscall this engine performs **59–63%** cheaper: `Engine::turn` goes from **448.9 ns to 175.2**, `recv` from 420.5 to 156.9, while thirteen pure user-space benchmarks move −4.1% to +4.1% with no direction. All of it is `retbleed`'s untrained return thunk plus `spec_rstack_overflow`'s Safe RET — `vmscape`, the mechanism `STATUS.md` had named for two days, costs **nothing**. **This row requires them ON**, because that is what `benches/baselines.tsv` was recorded with and a machine without them is not comparable; it is **not** advice to disable them. [ADR-0023](decisions/ADR-0023-section-9-records-the-cpu-mitigations.md), [measured-costs.md](reference/measured-costs.md) |
| `nohz_full` — **NOT recommended**, and this row is the price rather than the instruction | `[measured 2026-08-31]` **it costs 160 ns on every kernel entry and it is the whole of the 36%** the row above used to carry: 670.7 ns per turn against 494.8. What it buys is the far tail, and only the far tail — p50 376 against 216, p99 376 against 224, **p99.9 384 against 224**, and it wins from p99.99 outward (504 against 2848). A busy `hft` engine makes ~2 000 000 kernel entries per second and this removes ~1100 excursions of 3 µs: **0.32 s of tax against 0.0033 s of tail, a hundred to one against**. Take it only for a p99.99 objective, which §6 does not have. [ADR-0021](decisions/ADR-0021-nohz-full-leaves-section-9.md), [measured-costs.md](reference/measured-costs.md) |
| IRQ affinity: NIC queue → a core that is *not* the engine core | The engine never takes an interrupt |
| `mlockall` + pre-faulted buffers | No page fault on the hot path. The reference project's `pool.rs` touches every page at startup — copy that |
| Transparent huge pages **off** | THP compaction stalls are multi-millisecond |
| CPU frequency governor `performance`, C-states off | A core waking from C6 costs ~100 µs |
| `SO_BUSY_POLL` / `net.core.busy_poll` | Lets the kernel's own receive path spin instead of sleeping |
| **If TLS is on:** a kernel that carries the negotiated cipher suite in kTLS | kTLS support is narrower than what `rustls` will happily negotiate. A session that negotiates outside it drops silently to the userspace path and off the hot-path guarantee (D11). Which kernel version and which suites is ADR-0005 open question 2 — **unanswered** |

A latency number published without stating which of these were set is not a number.

### Checking, and the difference between a tuned box and an untuned one

The table above was a list of things somebody was supposed to have done. There was no way to
tell a tuned machine from an untuned one except by asking the person who set it up — which is
prose holding a constraint, and `CLAUDE.md` §4 says prose does not.

**`scripts/check-machine.sh` reads every row off the running machine** and prints `PASS`,
`FAIL` or `? ? ?` for each, with the command that fixes a failing one. It reads only: applying
these is root, machine-specific, and belongs to the person at the box.

`unknown` is deliberately **not** a pass. A container that cannot read `/sys` must not be able
to look like a tuned host — that is the shape of a green result nobody checked.

```
scripts/check-machine.sh          # what is in force, and how to fix what is not
scripts/bench.sh                  # counts and A/B comparisons, on any machine
scripts/bench.sh --strict         # refuses unless check-machine.sh is clean
```

`--strict` is the gate that makes non-negotiable 10 real: it fails **before it looks at a
single ceiling** if the machine is not set up, because a latency figure from an untuned box is
exactly the number that rule forbids publishing. Without `--strict` the run is still useful —
allocation counts are machine-independent, and an A/B comparison against the same box is valid
whatever that box is.

`[measured 2026-08-30]` On the shared container this repository was developed in, the script
reports `pass 1  fail 5  unknown 3` and exits 1, and `scripts/bench.sh --strict` refuses. That
is the correct answer for that machine, and it is why every timing figure in §6 above is
labelled with the box it came from.
