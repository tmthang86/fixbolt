# Configuration Reference

Every setting in `fixbolt`, in one place: configuration-file keys, programmatic limits, const
generics and Cargo features, each with its default and where it lives in the source.

[GUIDE.md §1c](GUIDE.md) explains how the configuration file is used; this page only lists
what it can say.

---

## 1. Configuration file keys

Counterparties and schedules are loaded by [`Settings::load`](../crates/engine/src/settings.rs)
from a QuickFIX-style INI file: one `[DEFAULT]` block, then one `[SESSION]` block per
counterparty. Values in `[DEFAULT]` apply to every session; a `[SESSION]` may override them.

Validation is strict. An unknown key, a malformed value or an impossible schedule stops
startup with the line number and the text that was written
([ADR-0040](decisions/ADR-0040-a-configuration-file-refuses-what-it-does-not-understand.md)).

**Eleven keys** are recognised `[changed 2026-09-04, was ten]`:

| Key | Meaning | Values | Default | Where | Source |
|---|---|---|---|---|---|
| `BeginString` | FIX version of the session | ASCII, max 32 bytes, e.g. `FIX.4.4` | required | `[DEFAULT]` or `[SESSION]` | [`settings.rs:95`](../crates/engine/src/settings.rs#L95) |
| `SenderCompID` | This engine's identity | ASCII, max 32 bytes | required | `[DEFAULT]` or `[SESSION]` | [`settings.rs:96`](../crates/engine/src/settings.rs#L96) |
| `TargetCompID` | The counterparty's identity | ASCII, max 32 bytes | required per `[SESSION]` | `[SESSION]` (or `[DEFAULT]`) | [`settings.rs:97`](../crates/engine/src/settings.rs#L97) |
| `HeartBtInt` | Heartbeat interval | positive integer, seconds | `30` | `[DEFAULT]` or `[SESSION]` | [`settings.rs:98`](../crates/engine/src/settings.rs#L98), [`session/src/lib.rs:266`](../crates/session/src/lib.rs#L266) |
| `MaxSkewMillis` | How far an inbound `SendingTime (52)` may differ from this engine's clock | integer, milliseconds | `120000` (2 minutes) | `[DEFAULT]` or `[SESSION]` | [`settings.rs:99`](../crates/engine/src/settings.rs#L99), [`session/src/lib.rs:260`](../crates/session/src/lib.rs#L260) |
| `StartTime` | When the session opens each day, UTC | `HH:MM:SS` | none; with no `EndTime` the session is always open | `[DEFAULT]` or `[SESSION]` | [`settings.rs:100`](../crates/engine/src/settings.rs#L100), [`settings.rs:567`](../crates/engine/src/settings.rs#L567) |
| `EndTime` | When the session closes each day, UTC | `HH:MM:SS` | none | `[DEFAULT]` or `[SESSION]` | [`settings.rs:101`](../crates/engine/src/settings.rs#L101), [`settings.rs:567`](../crates/engine/src/settings.rs#L567) |
| `StartDay` | First day of a weekly session | `Monday`/`Mon` … `Sunday`/`Sun` | none | `[DEFAULT]` or `[SESSION]` | [`settings.rs:102`](../crates/engine/src/settings.rs#L102), [`settings.rs:590`](../crates/engine/src/settings.rs#L590) |
| `EndDay` | Last day of a weekly session | `Monday`/`Mon` … `Sunday`/`Sun` | none | `[DEFAULT]` or `[SESSION]` | [`settings.rs:103`](../crates/engine/src/settings.rs#L103), [`settings.rs:590`](../crates/engine/src/settings.rs#L590) |
| `Weekdays` | Days a daily session opens on | comma-separated, e.g. `Mon,Tue,Wed,Thu,Fri` | all seven days | `[DEFAULT]` or `[SESSION]` | [`settings.rs:104`](../crates/engine/src/settings.rs#L104), [`settings.rs:629`](../crates/engine/src/settings.rs#L629) |
| `FileLogPath` | Path of the message log (both directions, one line per message). One engine writes one file; `conn=` and `shard=` tell counterparties apart inside it | any path the process can append to | none (no log) | `[DEFAULT]` **only**; a `[SESSION]` carrying it is refused | [`settings.rs`](../crates/engine/src/settings.rs), [`msglog.rs`](../crates/engine/src/msglog.rs) |

Three rules the file enforces:

- `StartTime` and `EndTime` go together. One without the other is an error.
- `StartDay` and `EndDay` go together, and they need `StartTime`/`EndTime` as well. A
  `StartDay` with no hours is a key that is spelled correctly and has no effect, so it is
  refused.
- A value longer than 32 bytes is refused rather than truncated. A truncated name would match
  no counterparty and the acceptor would start cleanly and serve nobody.

---

## 2. Programmatic limits and defaults

Set in code when the engine is built or started.

| Parameter | Meaning | Values | Default | Where set | Source |
|---|---|---|---|---|---|
| `Limits` | Pre-session bounds: how many sockets may wait for their Logon, and for how long | `pending > 0`, `logon_ms > 0` | **none** ([ADR-0020](decisions/ADR-0020-a-pre-session-stage-owns-the-socket-until-logon.md)) | `Limits::new(pending, logon_ms)`, passed to `serve` / `serve_hft` | [`presession.rs:372`](../crates/engine/src/presession.rs#L372), [`presession.rs:412`](../crates/engine/src/presession.rs#L412) |
| `DEFAULT_CAPACITY` | Size of the ring between the engine and an out-of-band application | power of two, bytes | `4194304` (4 MiB) | `RingDispatch::new(capacity)` | [`ring.rs:113`](../crates/engine/src/ring.rs#L113) |
| `DEFAULT_TIMEOUT_MS` | How long a `standard` engine blocks before waking to check timers | milliseconds | `100` | `Block::new(capacity)` or `Block::with_timeout_ms` | [`block.rs:33`](../crates/engine/src/block.rs#L33) |
| `MIN_TIMEOUT_MS` | Smallest timeout `Block` accepts; a smaller one is raised to it | milliseconds | `5` | enforced by `Block::with_timeout_ms` | [`block.rs:41`](../crates/engine/src/block.rs#L41) |
| `SLOTS` | Outbound messages the journal ring keeps for resends | power of two | `4096` `[changed 2026-09-04, was 8]` | `MemJournal<SLOTS, SLOT_LEN>` / `Store` | [`journal.rs:51`](../crates/engine/src/journal.rs#L51) |
| `SLOT_LEN` | Largest message the journal ring can keep | bytes | `512` | `MemJournal<SLOTS, SLOT_LEN>` / `Store` | [`journal.rs:56`](../crates/engine/src/journal.rs#L56) |
| `resend_batch` | Messages put on the wire per call when answering a `ResendRequest` | `u16`; zero is read as one | `8` | `Config::with_resend_batch` | [`session/src/lib.rs`](../crates/session/src/lib.rs) |
| `Durability` | What a `FileJournal` guarantees | `Async` (background writer), `Fsync` (blocks the engine thread) | `Async` | `FileJournal::open(path, durability)` | [`journal.rs`](../crates/engine/src/journal.rs) |

### Sizing the resend ring

`SLOTS` used to be 8 — enough for the acceptance corpus and far too small for an acceptor. An
acceptor that had sent a hundred ExecutionReports answered a resend request for all of them
by replaying eight and gap-filling ninety-two, which is legal on the wire and lost ninety-two
messages ([ADR-0046](decisions/ADR-0046-the-ring-is-the-resend-store-and-a-replay-goes-in-batches.md)).

Choose `N` by this rule:

> `N` ≥ the number of application messages you send during the longest disconnection you
> are willing to replay across. For most desks that is one trading day.

Memory per session is `N × (SLOT_LEN + 8)`, about **2 MiB** at the defaults.
`[measured 2026-09-04, Apple M5]` `tools/w2w` reads **+2 195 456 bytes** of maximum resident
set against the old `SLOTS = 8`. A gateway with hundreds of sessions should pick a smaller `N`
through the const generic ([GUIDE.md §1a](GUIDE.md)).

Two constraints go with it:

- **`resend_batch × SLOT_LEN` must stay under `TX`.** The defaults are 8 × 512 = 4 KiB against
  an 8 KiB `TX`. If you raise `SLOT_LEN` or lower `TX`, re-check this.
- **Two counters tell you when either is wrong.** `resend_beyond_journal` on
  `SessionSnapshot` (the ring was too small; messages were gap-filled instead of replayed)
  and `puts_refused` (a reply was longer than `SLOT_LEN` and can never be replayed). Both also
  arrive as events.

---

## 3. Const generics

The alias [`TcpAcceptorEngine`](../crates/engine/src/lib.rs#L970) fixes three capacities:

```rust
pub type TcpAcceptorEngine<A, W, J = crate::journal::Store> = Engine<
    TcpTransport,
    fixbolt_session::Acceptor,
    InlineDispatch<A>,
    crate::clock::SystemClock,
    W,
    J,
    256,  // N:  fields indexed per message
    4096, // RX: read buffer per connection, bytes
    8192, // TX: write buffer per connection, bytes
>;
```

| Parameter | Meaning | Alias default | To change it |
|---|---|---|---|
| `N` | Maximum fields in one `MessageView<N>`; a message with more is `ParseError::TooManyFields` | `256` | Instantiate `Engine<..., N, RX, TX>` directly |
| `RX` | Read buffer per connection | 4 KiB | same |
| `TX` | Write buffer per connection; also the queue for a slow counterparty | 8 KiB | same |

The library's `Handler<N, P, S>` has its own three: `N = 256` fields in the inbound index,
`P = 64` fields in a reply, `S = 1024` bytes for them. A reply that does not fit is
`Answer::Failed`, counted by `App::failed_replies()` ([GUIDE.md §1b](GUIDE.md)).

---

## 4. Cargo features

| Feature | Crates | What it enables | Default |
|---|---|---|---|
| `standard` | `engine`, `library` | The blocking poller (`block.rs`, `serve`, `StandardAcceptorEngine`), through `poll(2)` via `libc` | **on** |
| `affinity` | `engine` | Core pinning and topology checks via `libc`, Linux only. Naming a core in a build without it is a hard error | off |

`cargo build --no-default-features` builds with neither, and CI proves that on a runner with
nothing optional installed.
