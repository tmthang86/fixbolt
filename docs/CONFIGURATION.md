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

**Thirty keys** are recognised `[changed 2026-09-12, was twenty-six]`.

**What in these tables a machine checks, and what is a promise** `[added 2026-09-12]`. The
`doc_table` tests in [`settings.rs`](../crates/engine/src/settings.rs) read this section and
run the parser against four things: **the count sentence above, inside this section**; **a
*Default* cell that begins with a backticked literal** (the key is written into a minimal
file with that value and must change nothing); **a *Values* cell that is two or more
backticked literals joined by nothing but `or` and commas** (every listed literal must
survive the parser, and one sampled value it does not list must not — a sample of three fixed
candidates, not a check of everything the cell omits); and **a *Where* cell claiming either
`[DEFAULT]` only or `[DEFAULT]` or `[SESSION]`** (the first must be refused in a `[SESSION]`,
the second must not).
`[measured 2026-09-12]` those three reached 15, 10 and 18 of the thirty rows — 16 and 11 for
the first two in a build with the `tls` feature, which is the only build where the fourth TLS
key can be written into a file at all.

**Every other sentence here is a hand-checked promise** — each *Meaning* cell, every note,
and each paragraph between the tables. So is a *Default* cell written as prose (`required`,
`none`, `16 × …`) and a *Values* cell that describes a shape rather than listing values
(`ASCII, max 32 bytes`, `HH:MM:SS`): **writing a cell that way costs it its machine check**,
which is why each probe reports how many rows it skipped and carries a floor on how many it
reached that may only be raised. `[measured 2026-09-12]` before those probes existed, a
review rewrote the meaning, the values and the default of two TLS rows to say the opposite of
the code and changed this count sentence to *"Four hundred keys"*, and the test suite read
`3 passed; 0 failed`.

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

**Which role the file describes, and where to dial** `[added 2026-09-05]`:

| Key | Meaning | Values | Default | Where | Source |
|---|---|---|---|---|---|
| `ConnectionType` | Which role this whole file configures | `acceptor` or `initiator` | `acceptor` — every file written before 2026-09-05 | `[DEFAULT]` **only**; a file names one role | [`settings.rs`](../crates/engine/src/settings.rs) |
| `SocketConnectHost` | Where to dial. **Kept as written and resolved on every dial**, so a venue whose DNS fails over keeps working | a hostname or an address | required when `ConnectionType=initiator`; **refused otherwise, by line** | `[DEFAULT]` or `[SESSION]` | [`settings.rs`](../crates/engine/src/settings.rs) |
| `SocketConnectPort` | Which port | `0`–`65535` | required when `ConnectionType=initiator`; refused otherwise | `[DEFAULT]` or `[SESSION]` | [`settings.rs`](../crates/engine/src/settings.rs) |
| `ReconnectInterval` | First backoff delay after a connection ends | integer, **seconds** | `30` (QuickFIX's own) | initiator only | [`reconnect.rs`](../crates/engine/src/reconnect.rs) |
| `ReconnectCeiling` | Largest backoff delay. **No QuickFIX equivalent** — without it the ladder doubles for ever | integer, **seconds**, not below `ReconnectInterval` | 16 × `ReconnectInterval` | initiator only | [`reconnect.rs`](../crates/engine/src/reconnect.rs) |

**The session's own behaviour** `[added 2026-09-05]`. Each sets the `Config` knob of the same
name in [§2](#2-programmatic-limits-and-defaults); all are `[DEFAULT]` or `[SESSION]`:

| Key | Meaning | Values | Default |
|---|---|---|---|
| `ResetOnLogon` | Restart both counts as the connection is made, **including for a resumed session** | `Y` or `N` | `N` |
| `ResetOnLogout` | Restart both counts once the `Logout` exchange is over | `Y` or `N` | `N` |
| `ResetOnDisconnect` | Restart both counts when the link drops for any other reason | `Y` or `N` | `N` |
| `LogonTimeout` | How long a connection may sit without completing its `Logon`. **The initiator's** — an acceptor has `Limits.logon_ms` in front of it | integer, **seconds**; `0` is off | `0` |
| `LogoutTimeout` | How long to wait for the `Logout` this end asked for | integer, **seconds**; `0` is off | `0` |
| `AllowUnknownMsgFields` | Do not refuse a **defined** tag that this `MsgType` does not carry (`373=2`). A tag the dictionary has never heard of is still refused | `Y` or `N` | `N` |
| `ValidateUserDefinedFields` | Ask the dictionary about tags at or above 5000. **The one key whose `Y` means *keep working*** | `Y` or `N` | `Y` |
| `SendNextExpectedMsgSeqNum` | `[added 2026-09-06]` Put `789=NextExpectedMsgSeqNum` on every `Logon` this end sends, naming the number it is waiting to receive. **Reading an inbound `789` does not depend on this key** — that always happens | `Y` or `N` | `N` |
| `EnableLastMsgSeqNumProcessed` | `[added 2026-09-06]` Report `369=LastMsgSeqNumProcessed` — *the last of your messages I have processed* — on messages this end sends. **What it reaches is not everything**: see the note below | `Y` or `N` | `N` |
| `TimestampPrecision` | `[added 2026-09-09]` Fractional digits on `52=SendingTime` **going out**: `3` → 21 bytes, `6` → 24, `9` → 27. **A ceiling, not a promise** — see the note below. Reading a wider stamp never depended on this key and never has since 2026-09-08 | `3`, `6` or `9` | `3` |

**Both resync keys default to `N`, and the default is the safe one rather than a shy one.**
`[researched 2026-09-06]` every FIX engine surveyed defaults them off, because a counterparty
that does not expect an optional field can answer a `Reject`. Some venues require `369` — CME
iLink is the one OnixS names — so turning it on is a decision about the counterparty, not about
this engine.

**The two key names come from different QuickFIX families, and that is deliberate.** QuickFIX
**C++** spells the first `SendNextExpectedMsgSeqNum`; QuickFIX/J and quickfixgo call it
`EnableNextExpectedMsgSeqNum`. The C++ name is taken because that is the engine
`scripts/interop.sh` runs against. The second has **no C++ equivalent at all** — that engine
never sends `369` — so it takes the QuickFIX/J and QuickFIX/n spelling. Writing the other
family's name gets *"unknown key"* with its line number, which is this engine's answer to a
setting it does not understand ([ADR-0040](decisions/ADR-0040-a-configuration-file-refuses-what-it-does-not-understand.md))
and is worth more than it sounds: QuickFIX C++ ignores a key it does not recognise **in
silence**. `docs/reference/who-owns-the-outbound-header.md`.

**What `EnableLastMsgSeqNumProcessed=Y` reaches, and what it cannot.** The seven messages the
session generates itself carry `369`, and so does an application message this end originates.
An application **reply** carries it only if the application writes it, because on that path the
application writes the whole message and the session emits those bytes untouched. Through the
`fixbolt` library that happens for you — `Reply` writes the field — but an application
implementing `fixbolt_session::Application` directly must write
`Header::last_processed` itself, and **nothing in this engine can detect one that does not**.
[ADR-0056](decisions/ADR-0056-the-application-is-told-what-the-session-owns.md), and
[GUIDE.md](GUIDE.md) carries it as a constraint the type system cannot enforce.

**`ValidateFieldsOutOfOrder` is not recognised, and it is not an oversight.** QuickFIX's third
setting of that family switches off `373=14`, *tag specified out of required order*. This engine
builds a **flat index** of tag positions (DESIGN.md D2) and reads header-versus-body order out
of it with one comparison in the same scan that checks everything else. There is no separate
pass to skip; turning it off would mean deleting the comparison, which is a different engine
rather than a setting. Writing the key gets *"unknown key"* with its line, like any other.

**Y and N, and nothing else.** `true`, `yes` and `1` are refused with their line. Reading `true`
as `Y` today is reading `1` as `N` tomorrow, and a flag guessed wrongly is a session that
silently keeps or drops its numbering.

**A file names one role, and the wrong door says so with a line number.** `Settings::into_table`
refuses an initiator file and `Settings::into_initiator` refuses an acceptor file, each naming
the `ConnectionType=` line and the other door. The mistake worth catching is the first: a table
built from an initiator file is perfectly well formed, and the acceptor would sit waiting for
the venue it was told to dial, with nothing on the wire to say so because nothing would happen
on the wire. The sharded entry point takes a `Table` and nothing else, so it meets the same
refusal — one mechanism, not a second check to disagree with the first.

**An initiator file has exactly one `[SESSION]`**, because an initiator holds one session and
`connect_and_serve` takes one `Config`. A second block is refused on its own line.

Three rules the file enforces:

- `StartTime` and `EndTime` go together. One without the other is an error.
- `StartDay` and `EndDay` go together, and they need `StartTime`/`EndTime` as well. A
  `StartDay` with no hours is a key that is spelled correctly and has no effect, so it is
  refused.
- A value longer than 32 bytes is refused rather than truncated. A truncated name would match
  no counterparty and the acceptor would start cleanly and serve nobody.

**The certificate this acceptor presents** `[added 2026-09-12]`:

| Key | Meaning | Values | Default | Where | Source |
|---|---|---|---|---|---|
| `SocketUseSSL` | Turn TLS on for this listener | `Y` or `N` | `N` | `[DEFAULT]` **only**; acceptor only — refused on an initiator file, by line | [`settings.rs`](../crates/engine/src/settings.rs) |
| `ServerCertificateFile` | The PEM certificate chain this acceptor presents | a path to a PEM file | required when `SocketUseSSL=Y`; refused otherwise | `[DEFAULT]` only | [`settings.rs`](../crates/engine/src/settings.rs) |
| `ServerCertificateKeyFile` | The private key for that chain | a path to a PEM file | required when `SocketUseSSL=Y`; refused otherwise | `[DEFAULT]` only | [`settings.rs`](../crates/engine/src/settings.rs) |
| `TlsRequireKernel` | Refuse this deployment rather than serve TLS from userspace, if the kernel cannot take the keys ([ADR-0060](decisions/ADR-0060-a-deployment-that-requires-the-kernel-is-refused-twice.md)) | `Y` or `N` | `N` | `[DEFAULT]` only; **refused**, not ignored, without `SocketUseSSL=Y` | [`settings.rs`](../crates/engine/src/settings.rs) |

**All four are `[DEFAULT]`-only and acceptor-only, for the same reason a dialling key is refused
on an acceptor file.** These four describe the certificate a *server* presents; an initiator file
carrying one gets `WrongRole`, by line, exactly like `SocketConnectHost` on an acceptor file.
There is no `ClientCertificateFile` yet, so an initiator has nothing to configure here. A
`[SESSION]` block carrying one of the four is `DefaultOnly`, for the reason `FileLogPath` already
is: one listener presents one certificate ([ADR-0005](decisions/ADR-0005-tls.md) open question 5)
— SNI and a certificate per counterparty are out of scope, not merely unbuilt.

**The switch and the certificate are checked against each other, not read independently.**
`SocketUseSSL=Y` with no `ServerCertificateFile` or no `ServerCertificateKeyFile` is
`MissingKey`, blamed on the `SocketUseSSL=` line; a certificate, a key or `TlsRequireKernel`
written down with `SocketUseSSL=N` or no `SocketUseSSL` line at all is also `MissingKey`, blamed
on the key that would do nothing — refused rather than silently ignored, because an operator who
wrote a certificate and forgot the switch would otherwise get a plaintext acceptor with no
sentence about it anywhere.

**A build without the `tls` feature refuses `SocketUseSSL=Y` at parse time** (`Problem::NeedsFeature`)
rather than serve the port in plaintext — non-negotiable 6: such a build has no `rustls` in it at
all, so parsing and ignoring the key is not an honest option. This check runs *last*, so a file
with a missing certificate is still told that first, in every build.

**`Settings::into_table` refuses a file that asks for TLS** (`Problem::NeedsTlsDoor`, naming the
`SocketUseSSL=` line), the same expensive mistake `WrongRole` already guards: the table would be
perfectly well formed and the acceptor would serve **plaintext**, certificate unread on disk,
nothing on the wire to say so. [`Settings::into_tls_table`](../crates/engine/src/settings.rs) is
the door that returns the `Table` and the certificate together; reading the PEM off disk is the
caller's next step, at [`tls::load_pem`](../crates/engine/src/tls.rs).

---


### `TimestampPrecision` — a ceiling on the way out, and nothing on the way in

`[added 2026-09-09]` [ADR-0057](decisions/ADR-0057-sub-millisecond-time-arrives-beside-the-tick.md).

**The key is an integer, and that is deliberate.** QuickFIX C++ — the engine `scripts/interop.sh`
runs against — reads `TimestampPrecision` with `getInt` and accepts 0–9
(`Session.h:167–174`, read at the pinned commit), defaulting to 3. A `.cfg` shared between the
two ends therefore carries a number, not a name.

**This engine writes three of those widths and refuses the other seven by line number.**
`TimestampPrecision=4` is a settings error naming the key, not a value rounded up to 6: answering
with a width the operator did not choose would put a resolution on the wire that nobody asked
for and nothing downstream would say so.

**It is a ceiling, and the floor is what your clock actually gave the session.** The stamp is
written at the *coarser* of this key and the resolution the caller supplied, so:

| How the session is driven | `TimestampPrecision=6` writes |
|---|---|
| `fixbolt::serve` and friends — the engine reads a real clock | **24 bytes** |
| `Session::tick(now_ms)` by hand — a millisecond and nothing finer | **21 bytes** |

Padding the three extra digits with zeroes would claim microsecond resolution this end does not
have; `.123000` to a venue *measuring* clock divergence under MiFID II RTS 25 is worse than
`.123`, which claims millisecond resolution truthfully. `Session::tick_at` is the door that
carries the finer number, and the serving loop uses it.
`crates/session/tests/timestamp_precision.rs::a_configured_width_is_a_ceiling_and_not_a_promise`
holds it.

**Receiving is not configurable and never was.** A counterparty's `52=` is read at 17, 21, 24 or
27 bytes regardless of this key — a valid timestamp is not something an operator should have to
switch on.

## 2. Programmatic limits and defaults

Set in code when the engine is built or started.

| Parameter | Meaning | Values | Default | Where set | Source |
|---|---|---|---|---|---|
| `Limits` | Pre-session bounds: how many sockets may wait for their Logon, and for how long | `pending > 0`, `logon_ms > 0` | **none** ([ADR-0020](decisions/ADR-0020-a-pre-session-stage-owns-the-socket-until-logon.md)) | `Limits::new(pending, logon_ms)`, passed to `serve` / `serve_hft` | [`presession.rs:372`](../crates/engine/src/presession.rs#L372), [`presession.rs:420`](../crates/engine/src/presession.rs#L420) |
| `DEFAULT_CAPACITY` | Size of the ring between the engine and an out-of-band application | power of two, bytes | `4194304` (4 MiB) | `RingDispatch::new(capacity)` | [`ring.rs:113`](../crates/engine/src/ring.rs#L113) |
| `DEFAULT_TIMEOUT_MS` | How long a `standard` engine blocks before waking to check timers | milliseconds | `100` | `Block::new(capacity)` or `Block::with_timeout_ms` | [`block.rs:33`](../crates/engine/src/block.rs#L33) |
| `MIN_TIMEOUT_MS` | Smallest timeout `Block` accepts; a smaller one is raised to it | milliseconds | `5` | enforced by `Block::with_timeout_ms` | [`block.rs:41`](../crates/engine/src/block.rs#L41) |
| `SLOTS` | Outbound messages the journal ring keeps for resends | power of two | `4096` `[changed 2026-09-04, was 8]` | `MemJournal<SLOTS, SLOT_LEN>` / `Store` | [`journal.rs:51`](../crates/engine/src/journal.rs#L51) |
| `SLOT_LEN` | Largest message the journal ring can keep | bytes | `512` | `MemJournal<SLOTS, SLOT_LEN>` / `Store` | [`journal.rs:56`](../crates/engine/src/journal.rs#L56) |
| `resend_batch` | Messages put on the wire per call when answering a `ResendRequest` | `u16`; zero is read as one | `8` | `Config::with_resend_batch` | [`session/src/lib.rs`](../crates/session/src/lib.rs) |
| `validation` | `[added 2026-09-05]` Which of the dictionary's questions the session asks — QuickFIX's `AllowUnknownMsgFields` and `ValidateUserDefinedFields`. **`ValidateFieldsOutOfOrder` is not supported and will not be**: the parser builds a flat index (D2) and header-versus-body order is one comparison inside the same scan, not a pass that can be skipped | `DictionaryChecks::new()` plus `.allowing_unknown_msg_fields()` and/or `.skipping_user_defined_fields()` | every check **on** — what the 59 acceptance definitions prove | `Config::with_validation` | [`session/src/lib.rs`](../crates/session/src/lib.rs), [SESSION-BEHAVIOUR §3](SESSION-BEHAVIOUR.md) |
| `logon_timeout_ms` | `[added 2026-09-05]` How long a connection may sit without completing its `Logon`. **The initiator's** — an acceptor has `Limits.logon_ms` in front of it, before a `Session` exists | milliseconds; **zero is off** | `0` | `Config::with_logon_timeout_ms` | [`session/src/lib.rs`](../crates/session/src/lib.rs), [SESSION-BEHAVIOUR §1](SESSION-BEHAVIOUR.md) |
| `logout_timeout_ms` | `[added 2026-09-05]` How long this end waits for the `Logout` it asked for. Without it the only bound is 2.4 × `HeartBtInt` | milliseconds; **zero is off** | `0` | `Config::with_logout_timeout_ms` | [`session/src/lib.rs`](../crates/session/src/lib.rs), [SESSION-BEHAVIOUR §1](SESSION-BEHAVIOUR.md) |
| `reset` | `[added 2026-09-05]` When this session restarts both counts at 1 of its own accord — QuickFIX's `ResetOnLogon`, `ResetOnLogout`, `ResetOnDisconnect`. **Not the same choice as `Session::new` versus `resume`**, which says what the journal holds | `ResetPolicy::new()` plus any of `.on_logon()`, `.on_logout()`, `.on_disconnect()` | resets on **nothing** — the behaviour the 59 acceptance definitions prove | `Config::with_reset` | [`session/src/lib.rs`](../crates/session/src/lib.rs), [SESSION-BEHAVIOUR §4](SESSION-BEHAVIOUR.md) |
| `Durability` | What a `FileJournal` guarantees. **Under `Fsync` an *administrative* message costs a `sync_data` too**, since 2026-09-05: the outbound count is written when it moves, so a `Heartbeat` every second is a disk sync every second (ADR-0053, the price ADR-0017 already accepted inbound). `Async` — the default — keeps it off the engine thread | `Async` (background writer), `Fsync` (blocks the engine thread) | `Async` | `FileJournal::open(path, durability)` | [`journal.rs`](../crates/engine/src/journal.rs) |
| `MAX_ON_LOGON` | Most messages one session may originate from `Handler::on_logon` | `u32`, not configurable | `16` | compile-time constant | [`engine/src/lib.rs`](../crates/engine/src/lib.rs) |
| `ORIGIN_CAPACITY` | Originated messages a `Sender` may have waiting for the engine's next turn | `usize`, not configurable | `64` | compile-time constant | [`origin.rs`](../crates/engine/src/origin.rs) |
| `ORIGIN_LEN` | Largest message `Sender::send` will take | bytes, not configurable | `512` | compile-time constant | [`origin.rs`](../crates/engine/src/origin.rs) |

### The three origination numbers

`[added 2026-09-05]` All three come with [ADR-0048](decisions/ADR-0048-an-engine-that-can-speak-first-has-two-doors.md)
and **none of them is a knob today** — they are constants, not const generics, and making them
the caller's is work nobody has asked for yet.

- **`MAX_ON_LOGON` is a guard, not a quota.** It stops a handler that never answers
  `reply.silent()` from holding the engine thread. There is no measurement behind 16 and the
  ADR says so. Reaching it emits `EventKind::SpokeFirstToTheBound`, so an application that
  genuinely needs more will say so on the event stream rather than quietly opening a session a
  few messages short.
- **`ORIGIN_CAPACITY × ORIGIN_LEN` is 32 KiB, allocated once** when the engine is built, per
  engine and not per session.
- **`ORIGIN_LEN` matches `SLOT_LEN`** on purpose: a message too long for the journal to keep
  for a resend is a message that should not go out. A message over it is refused at
  `Sender::send`, which answers `false` — unlike the reply scratch, this ceiling does not fail
  as silence.

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

**Four sizes, and every entry point takes them.** `[changed 2026-09-05]` these used to be
literals inside a type alias, and the only advice this page could give was *"instantiate
`Engine<...>` directly"* — which a user of the `fixbolt` crate could not do, because `Engine` is
not re-exported. Each `serve*` function now has a `*_with` twin that takes all four:

```rust
// the defaults, spelled out — this is exactly what `serve` calls
serve_with::<256, 4096, 8192, 1024, _, _>(addr, table, app, capacity, limits, log, handles)?;

// a counterparty that sends 16 KiB messages and expects 8 KiB answers
serve_with::<256, 16_384, 16_384, 8_192, _, _>(addr, table, app, capacity, limits, log, handles)?;
```

The two `_` are the language's, not this API's: a turbofish must supply **every** generic
argument, and `A` and `L` are inferred from the arguments you pass. Const parameters cannot
carry defaults on a function, which is why `serve_with` is a second function rather than four
more parameters on `serve`.

| Parameter | Meaning | Default | Raise it when |
|---|---|---|---|
| `N` | Maximum fields in one `MessageView<N>`; more is `ParseError::TooManyFields` | `256` | a counterparty sends messages with many repeating-group members |
| `RX` | Read buffer per connection — **the largest message this end can frame**. Also sizes the pre-session buffer | 4 KiB | a counterparty sends messages larger than 4 KiB |
| `TX` | Write buffer per connection; also the queue for a slow counterparty | 8 KiB | replies are large, or a counterparty reads slowly |
| `APP` | Scratch an `Application` lays one reply out in | 1 KiB | **this end must answer with more than ~1 KiB** |

**`APP` must not exceed `TX`**, which is the queue the reply is copied into after it is laid
out. Nothing checks this: an `APP` larger than `TX` simply wastes the difference.

**A message too large for `RX` is not a silent drop.** The framer reads `9=` and, when the
message cannot fit, reports the bytes as garbage: after logon the session decides what an
unreadable frame means, and before logon the connection is closed. A prefix longer than `RX`
handed over by the pre-session stage is refused with `PrefixTooLong`.

**`APP` is the ceiling most likely to surprise you, and it fails as silence.** `[measured
2026-09-05]` an `Application` that cannot lay out its reply returns `None`, which is a legal
answer meaning *"nothing to say"* — so exceeding `APP` looks exactly like an application that
chose not to reply. A size sweep against a real acceptor found the wall between 200 and 1000
bytes while `RX` was 16 KiB:
[a-ceiling-has-more-than-one-floor](reference/a-ceiling-has-more-than-one-floor.md).

**What they cost.** `[measured 2026-09-05]` one `Connection` is **23 760 bytes** at `RX = 4096`
and **36 048** at `RX = 16 384` — the difference is exactly the buffer, which
`crates/engine/tests/connection_size.rs` asserts rather than states. Against that, each session
carries a **~2 MiB** journal ring on the heap (`SLOTS × (SLOT_LEN + 8)`), so quadrupling the
receive buffer is **+0.57%** per connection. Memory is rarely the reason not to raise these —
but a larger `RX` buys **capacity, not speed**, and `RX = 4096` is an unmeasured default:
[ADR-0055](decisions/ADR-0055-max-message-size-is-not-a-key-and-rx-is-the-answer.md).

**There is no `MaxMessageSize` key, and there will not be one.** No engine surveyed has one —
`MaxMessageSize` is FIX **tag 383**, an optional `Logon` field by which the two ends tell each
other their limit, not a setting. `RX` is where this engine's ceiling is set, and it is set at
compile time. ADR-0055.

The library's `Handler<N, P, S>` has its own three: `N = 256` fields in the inbound index,
`P = 64` fields in a reply, `S = 1024` bytes for them. A reply that does not fit is
`Answer::Failed`, counted by `App::failed_replies()` ([GUIDE.md §1b](GUIDE.md)).

---

## 4. Cargo features

| Feature | Crates | What it enables | Default |
|---|---|---|---|
| `standard` | `engine`, `library` | The blocking poller (`block.rs`, `serve`, `StandardAcceptorEngine`), through `poll(2)` via `libc` | **on** |
| `affinity` | `engine` | Core pinning and topology checks via `libc`, Linux only. Naming a core in a build without it is a hard error | off |
| `tls` | `engine` | `mod tls`: the userspace `rustls` handshake, the kTLS handover, `serve_tls`/`serve_tls_with`/`serve_tls_requiring`, `tls::load_pem`, and the four `SocketUseSSL`-family settings keys (§1). Pulls in `rustls`, `ktls-core` and `libc` — the first dependencies in this crate that bring a tree of their own | off |

`cargo build --no-default-features` builds with none of the three, and CI proves that on a
runner with nothing optional installed. **`tls` is Linux-only in practice**: `mod tls` itself is
gated only on the feature, but the handshake, `load_pem` and every `serve_tls*` entry point
inside it are additionally `#[cfg(target_os = "linux")]`, so a `--features tls` build on another
target compiles the crate but exposes no way to bring a TLS listener up — [D11 in
`DESIGN.md`](DESIGN.md) has the full split by mode. The four `SocketUseSSL`-family settings keys
(§1) are unaffected by any of this: `TlsSettings` and `Settings::into_tls_table` are compiled in
every feature set, which is what lets a build **without** `tls` refuse `SocketUseSSL=Y` with a
sentence (`Problem::NeedsFeature`) instead of failing to compile at all.
