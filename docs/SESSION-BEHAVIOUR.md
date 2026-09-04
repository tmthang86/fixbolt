# Session Behaviour at the Boundary

What the FIX 4.4 session layer does with a message before your application ever
sees it: which messages it answers itself, which it rejects, and when it drops the
connection. This page is a reference, not a tutorial — for how to embed the engine
see [GUIDE.md](GUIDE.md), and for the vocabulary see [INTRODUCTION.md](INTRODUCTION.md).

**Every row names the `.def` file or the test that holds it.** The `.def` files are
the QuickFIX acceptance definitions, fetched into gitignored `vendor/` by
`scripts/fetch-quickfix-assets.sh`; the 59 the session layer is gated against are its
primary conformance gate. A behaviour with nothing to point at is not on this page —
if you find one missing, it is missing because nothing proves it, which is the point.

The pure session machine lives in `crates/session/src/lib.rs`. It has no socket and
no clock: time arrives as `Input::Tick`, and every decision below is taken without
allocating.

---

## 1. The Logon handshake, and why a connection is dropped

The first message on a connection must be a `Logon` (`35=A`) carrying both `98=`
(EncryptMethod) and `108=` (HeartBtInt). Anything else, and the session drops the
connection rather than replying — a refused Logon gets silence, not a Reject.

Every reason the session drops is a variant of `DropReason` (`crates/session/src/lib.rs`,
`pub enum DropReason`), and each variant names the exact wire condition:

| `DropReason` | Wire condition |
|---|---|
| `WrongBeginString` | `8=` is not the configured `BeginString` |
| `NotALogon` | the first message on the connection was not a `Logon` |
| `LogonIncomplete` | a `Logon` missing `98=` or `108=`, both required by FIX 4.4 |
| `WrongSenderCompId` | `49=` is not the configured counterparty |
| `WrongTargetCompId` | `56=` is not us |
| `SendingTimeOutOfRange` | `52=` is absent, unreadable, or further from the engine's clock than `max_skew_ms` — check NTP; `Session::last_skew_ms` says by how much |
| `SequenceNumberTooLow` | `34=` is absent, unreadable, or already used |
| `OutsideSchedule` | a message arrived while the schedule says the session is shut |
| `CannotSend` | the session could not put a message on the wire; it fails closed rather than send something malformed |
| `HeartbeatTimeout` | nothing arrived for long enough that the session gave up, after an unanswered `TestRequest` |
| `PeerLogout` | the counterparty sent a `Logout` |
| `ScheduleClosed` | the schedule's window closed on a live session — **not a fault** |

`DropReason` is `#[non_exhaustive]`: match it with a `_` arm. It is surfaced to the
operator through the event stream (see [GUIDE.md](GUIDE.md) §8a, *Why a connection
ended*).

---

## 2. The seven messages the session answers itself

Administrative traffic never reaches your application. The session emits these seven
message shapes on its own (`crates/session/src/lib.rs`, `enum Which`):

| Emitted | When |
|---|---|
| `Logon` (`35=A`) | to accept a valid counterparty Logon, or to initiate one |
| `Logout` (`35=5`) | to end the session, and in reply to a peer `Logout` |
| `Heartbeat` (`35=0`) | on the `HeartBtInt` interval, and in reply to a `TestRequest` |
| `TestRequest` (`35=1`) | when the interval passes with nothing heard — one unanswered `TestRequest` then drops (`HeartbeatTimeout`) |
| `Reject` (`35=3`) | a session-level reject; see §3 |
| `ResendRequest` (`35=2`) | when an inbound sequence number is higher than expected — a gap |
| `SequenceReset` gap fill (`35=4`, `123=Y`) | to fill a gap in the outbound stream during a resend |

The `TestRequest` id (`112`) is a fixed constant, because the oracle makes it one:
`6_SendTestRequest.def` writes `112=TEST` and nothing in FIX 4.4 fixes its value.

---

## 3. Session-level rejects — which `373` comes back

A malformed but well-framed message earns a `Reject` (`35=3`) naming
`SessionRejectReason` (`373`). The message is walked once in wire order and the **first
fault wins** — the corpus depends on this ordering. The mapping below is held by the
`.def` files named:

| `373=` | Meaning | Held by |
|---|---|---|
| `0` | invalid tag / garbled | `2d_GarbledMessage.def` |
| `1` | required field missing | `2g_PossDupNoOrigSendingTime.def` (names `122`) |
| `4` | tag specified without a value | `14d` (`56=` empty, which beats the CompID mismatch) |
| `9` | CompID problem | the `14*` family |
| `10` | SendingTime accuracy | `2m_BodyLengthValueNotCorrect.def` (via `122` OrigSendingTime) |
| `11` | invalid MsgType | `2r_UnregisteredMsgType.def` |

Ordering is itself tested: `14d` proves required-field runs before CompID, and the
`14h` family proves MsgType-not-defined runs after required-field. Break the order and
those files change which `373` they see.

**A session reject is not a business reject.** `2r_UnregisteredMsgType.def` sends
`35=8` (an application message of an unsupported type), and the engine answers with a
*business* reject, not a `373`. Unsupported application types are the application's
concern; malformed session plumbing is the session's.

---

## 4. Sequence numbers, gaps, and resend

- **A gap triggers a `ResendRequest`.** An inbound `34=` higher than expected means
  messages were missed; the session asks for them and holds newer messages until the
  gap fills.
- **Gap fill defaults to `123=N`.** A `SequenceReset` with `123=` absent is treated as
  `N` — `11a` and `11b` leave it out and the session must default it.
- **A `SequenceReset-Reset` (`123=N`) versus gap fill (`123=Y`)** reset the expected
  number differently; the table in `crates/session/src/lib.rs` around the resend logic
  records which of the two restarts the counts and which does not.
- **`141=Y` on a Logon resets both sequence counts** before the Logon's own numbers are
  applied — the only thing that restarts a resumed session's counters.
- **`16=0` and a range past what was sent are both clamped** to the last number this end
  actually sent. Guarded by `crates/engine/tests/journal.rs::what_no_longer_fits_in_the_ring_is_filled_over_not_skipped`.
- `[2026-09-04]` **An answer goes out in batches**, `Config::resend_batch` messages per call —
  a replay is one message and a gap fill is one message however many numbers it covers.
  The rest follow on later calls, interleaved with new traffic: a replayed message carries its
  original `34=` with `43=Y`, a new one carries the next new number. Continued by
  `Session::tick_with` and by each judged message; **plain `Session::tick` does not continue
  one**, because it has no journal and would gap-fill the remainder. Guarded by
  `crates/engine/tests/journal.rs::a_long_resend_is_replayed_over_several_ticks_in_order`,
  `::a_replay_stalls_on_the_journal_less_tick_and_says_nothing_wrong` and
  `crates/engine/tests/backpressure.rs::a_resend_larger_than_tx_does_not_end_the_session`.
  [ADR-0046](decisions/ADR-0046-the-ring-is-the-resend-store-and-a-replay-goes-in-batches.md).
- `[2026-09-04]` **A replay in progress is cancelled** by a disconnect, by a schedule boundary
  that restarts the counts, and by a newer `ResendRequest` — which replaces it rather than
  queueing behind it. Guarded by `::a_disconnect_cancels_a_resend_in_progress`,
  `::a_schedule_reset_cancels_a_resend_in_progress` and
  `::a_new_resend_request_replaces_the_one_in_progress`.
- `[2026-09-04]` **Numbers below the journal's floor are gap-filled and counted.** Legal on the
  wire and otherwise invisible; `SessionSnapshot::resend_beyond_journal` and
  `EventKind::ResendBeyondJournal { filled, oldest }` say how many messages, not how many
  times. A fill over an administrative message is **not** counted — none was ever replayable.
  Guarded by `::a_resend_that_reaches_below_the_ring_counts_every_number_it_filled` and
  `::a_fill_over_messages_the_ring_never_held_is_not_counted`.

---

## 5. PossDup, PossResend, and OrigSendingTime

- **`43=Y` PossDup** marks a possibly-duplicate retransmission; it requires `122`
  OrigSendingTime. `2g_PossDupNoOrigSendingTime.def` omits it and earns `373=1`.
- **`97=Y` PossResend** marks a message the sender may have sent before.
  `19b_PossResendMessageThatHasNotBeenSent.def` sends one the receiver has not seen.
- **`122` OrigSendingTime** is checked against SendingTime; `2m_BodyLengthValueNotCorrect.def`
  exercises the `122` path.

---

## 6. What this page does not claim

The 59 `.def` files are a conformance corpus, not an adversarial one — see
[reference/a-conformance-corpus-is-not-an-adversarial-one.md](reference/a-conformance-corpus-is-not-an-adversarial-one.md).
A behaviour with no `.def` file and no test behind it is not documented here as though
it were proven. Where a boundary decision matters to you and you do not find it above,
read the code — `crates/session/src/lib.rs` — rather than assume the behaviour, and if
it is real and untested, that is a gap worth a test before it is worth a doc row.
