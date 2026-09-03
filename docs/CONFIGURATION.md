# Configuration Reference

A complete, single-source lookup table for all configuration settings in `fixbolt`.

This document complements [`docs/GUIDE.md`](GUIDE.md) §1a0 by providing an exhaustive
reference of configuration file keys, runtime constants, const generics, and code-level
tuning parameters with their source locations.

---

## 1. Session Configuration File (`settings.rs`)

Counterparties and session schedules are loaded via [`Settings::load`](../crates/engine/src/settings.rs#L344)
from a QuickFIX-style INI file. Every recognised setting is validated strictly:
an unrecognised key, misspelled value, or impossible schedule will fail startup immediately.

| Setting | Meaning | Valid Values | Default | Where Set | Source Location |
|---|---|---|---|---|---|
| `BeginString` | FIX protocol version for the session | ASCII string (e.g. `FIX.4.4`), max 32 bytes | *None (Required)* | `[DEFAULT]` or `[SESSION]` | [`crates/engine/src/settings.rs:95`](../crates/engine/src/settings.rs#L95) |
| `SenderCompID` | Identifier of this acceptor engine | ASCII string, max 32 bytes | *None (Required)* | `[DEFAULT]` or `[SESSION]` | [`crates/engine/src/settings.rs:96`](../crates/engine/src/settings.rs#L96) |
| `TargetCompID` | Identifier of the remote counterparty | ASCII string, max 32 bytes | *None (Required in `[SESSION]`)* | `[SESSION]` (or `[DEFAULT]`) | [`crates/engine/src/settings.rs:97`](../crates/engine/src/settings.rs#L97) |
| `HeartBtInt` | Heartbeat interval in seconds | Positive integer (`u32`) | `30` | `[DEFAULT]` or `[SESSION]` | [`crates/engine/src/settings.rs:98`](../crates/engine/src/settings.rs#L98), [`crates/session/src/lib.rs:266`](../crates/session/src/lib.rs#L266) |
| `MaxSkewMillis` | Maximum allowed clock skew for incoming `SendingTime (52)` in milliseconds | Integer (`u64`) | `120000` (2 minutes) | `[DEFAULT]` or `[SESSION]` | [`crates/engine/src/settings.rs:99`](../crates/engine/src/settings.rs#L99), [`crates/session/src/lib.rs:260`](../crates/session/src/lib.rs#L260) |
| `StartTime` | Session activation start time (UTC) | `HH:MM:SS` (24-hour format) | *None* (omitted with `EndTime` = continuous session) | `[DEFAULT]` or `[SESSION]` | [`crates/engine/src/settings.rs:100`](../crates/engine/src/settings.rs#L100), [`crates/engine/src/settings.rs:567`](../crates/engine/src/settings.rs#L567) |
| `EndTime` | Session deactivation end time (UTC) | `HH:MM:SS` (24-hour format) | *None* | `[DEFAULT]` or `[SESSION]` | [`crates/engine/src/settings.rs:101`](../crates/engine/src/settings.rs#L101), [`crates/engine/src/settings.rs:567`](../crates/engine/src/settings.rs#L567) |
| `StartDay` | Start day of the week for weekly sessions | `Monday`/`Mon` .. `Sunday`/`Sun` | *None* | `[DEFAULT]` or `[SESSION]` | [`crates/engine/src/settings.rs:102`](../crates/engine/src/settings.rs#L102), [`crates/engine/src/settings.rs:590`](../crates/engine/src/settings.rs#L590) |
| `EndDay` | End day of the week for weekly sessions | `Monday`/`Mon` .. `Sunday`/`Sun` | *None* | `[DEFAULT]` or `[SESSION]` | [`crates/engine/src/settings.rs:103`](../crates/engine/src/settings.rs#L103), [`crates/engine/src/settings.rs:590`](../crates/engine/src/settings.rs#L590) |
| `Weekdays` | Active weekdays for daily schedule | Comma-separated weekdays (e.g. `Mon,Tue,Wed,Thu,Fri`) | *None* (all 7 days if daily schedule) | `[DEFAULT]` or `[SESSION]` | [`crates/engine/src/settings.rs:104`](../crates/engine/src/settings.rs#L104), [`crates/engine/src/settings.rs:629`](../crates/engine/src/settings.rs#L629) |

---

## 2. Code-Level Configuration & Limits

Parameters that are configured programmatically when instantiating or starting the engine.

| Parameter | Meaning | Valid Values | Default | Where Set | Source Location |
|---|---|---|---|---|---|
| `Limits` | Pre-session handshake limits protecting against slow-loris attacks | `pending > 0`, `logon_ms > 0` | **No default** ([ADR-0020](decisions/ADR-0020-a-pre-session-stage-owns-the-socket-until-logon.md)) | Argument to `serve` / `serve_hft` via `Limits::new(pending, logon_ms)` | [`crates/engine/src/presession.rs:372`](../crates/engine/src/presession.rs#L372), [`crates/engine/src/presession.rs:412`](../crates/engine/src/presession.rs#L412) |
| `DEFAULT_CAPACITY` | Size of the outbound dispatch ring buffer in bytes | Power of two (`usize`) | `4194304` (4 MiB = `1 << 22`) | `RingDispatch::new(capacity)` | [`crates/engine/src/ring.rs:113`](../crates/engine/src/ring.rs#L113) |
| `DEFAULT_TIMEOUT_MS` | Idle poll timeout for `standard` mode in milliseconds | Whole milliseconds (`u32`) | `100` | `Block::new(capacity)` or `Block::with_timeout_ms` | [`crates/engine/src/block.rs:33`](../crates/engine/src/block.rs#L33) |
| `MIN_TIMEOUT_MS` | Minimum poll timeout enforced by `Block` | Whole milliseconds (`u32`) | `5` | Enforced internally by `Block::with_timeout_ms` | [`crates/engine/src/block.rs:41`](../crates/engine/src/block.rs#L41) |
| `SLOTS` | Outbound messages retained in journal for resend replies | Power of two (`usize`) | `8` | `MemJournal<SLOTS, SLOT_LEN>` / `Store` | [`crates/engine/src/journal.rs:40`](../crates/engine/src/journal.rs#L40) |
| `SLOT_LEN` | Maximum message size stored in journal slot | Bytes (`usize`) | `512` | `MemJournal<SLOTS, SLOT_LEN>` / `Store` | [`crates/engine/src/journal.rs:45`](../crates/engine/src/journal.rs#L45) |
| `Durability` | Persistence guarantee for on-disk file journaling | `Durability::Async` (non-blocking writer), `Durability::Fsync` (blocks engine thread) | `Durability::Async` | `FileJournal::open(path, durability)` | [`crates/engine/src/journal.rs:144`](../crates/engine/src/journal.rs#L144) |

---

## 3. Const Generics and Aliases

The type alias [`TcpAcceptorEngine`](../crates/engine/src/lib.rs#L970) fixes buffer and indexing
capacities for standard deployments:

```rust
pub type TcpAcceptorEngine<A, W, J = crate::journal::Store> = Engine<
    TcpTransport,
    fixbolt_session::Acceptor,
    InlineDispatch<A>,
    crate::clock::SystemClock,
    W,
    J,
    256,  // N: Maximum indexed fields in MessageView
    4096, // RX: Read buffer capacity in bytes
    8192, // TX: Write buffer capacity in bytes
>;
```

| Const Parameter | Meaning | Alias Default | Customization |
|---|---|---|---|
| `N` | Maximum number of fields indexed in `MessageView<N>` | `256` | Instantiate `Engine<..., N, RX, TX>` directly |
| `RX` | Per-connection read buffer capacity | `4096` bytes (4 KiB) | Instantiate `Engine<..., N, RX, TX>` directly |
| `TX` | Per-connection write buffer capacity | `8192` bytes (8 KiB) | Instantiate `Engine<..., N, RX, TX>` directly |

---

## 4. Cargo Feature Flags

| Feature | Scope | Description | Default |
|---|---|---|---|
| `standard` | `engine`, `library` | Enables blocking engine poller via `kqueue`/`epoll` (`block.rs`, `serve`, `StandardAcceptorEngine`). | **Yes** |
| `affinity` | `engine` | Enables CPU core pinning and thread affinity checks via `libc` on Linux. | No |
