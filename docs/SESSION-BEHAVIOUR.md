# Session Behaviour at the Boundary

What the session layer does with a message before your application sees it: which messages
it answers itself, which it rejects, and when it drops the connection.

This page is a reference. For how to embed the engine see [GUIDE.md](GUIDE.md); for the
vocabulary see [INTRODUCTION.md](INTRODUCTION.md).

**Every row names the `.def` file or the test that holds it.** The `.def` files are the
QuickFIX acceptance definitions, fetched into gitignored `vendor/` by
`scripts/fetch-quickfix-assets.sh`. A behaviour with nothing to point at is deliberately not
on this page: if it is not proven, it is not documented as if it were.

The session machine lives in `crates/session/src/lib.rs`. It has no socket and no clock. Time
arrives as `Input::Tick`, and nothing below allocates.

---

## 1. Logon, and why a connection is dropped

The first message on a connection must be a Logon (`35=A`) carrying `98=` (EncryptMethod)
and `108=` (HeartBtInt). Anything else drops the connection without a reply. **A refused
Logon gets silence, not a Reject.**

Every reason for a drop is a variant of `DropReason` in `crates/session/src/lib.rs`:

| `DropReason` | Wire condition |
|---|---|
| `WrongBeginString` | `8=` is not the configured `BeginString` |
| `NotALogon` | the first message was not a Logon |
| `LogonIncomplete` | a Logon missing `98=` or `108=` |
| `WrongSenderCompId` | `49=` is not the configured counterparty |
| `WrongTargetCompId` | `56=` is not us |
| `SendingTimeOutOfRange` | `52=` is absent, unreadable, or further from the engine's clock than `max_skew_ms`. Check NTP; `Session::last_skew_ms` says by how much |
| `SequenceNumberTooLow` | `34=` is absent, unreadable, or already used |
| `OutsideSchedule` | a message arrived while the schedule says the session is shut |
| `CannotSend` | the session could not put a message on the wire and fails closed rather than send something malformed |
| `HeartbeatTimeout` | nothing arrived for long enough, after an unanswered TestRequest |
| `PeerLogout` | the counterparty sent a Logout |
| `ScheduleClosed` | the schedule's window closed on a live session; **not a fault** |
| `TransportClosed` | the socket closed |
| `DuplicateIdentity` | a second connection claimed an identity that is already logged on ([ADR-0030](decisions/ADR-0030-one-engine-holds-many-counterparties.md)) |
| `SlowApplication` | the ring to the application filled ([ADR-0011](decisions/ADR-0011-a-full-ring-disconnects.md)) |
| `SlowConsumer` | the counterparty stopped reading and the send queue filled (DESIGN.md D10) |
| `EngineShutdown` | an ordered shutdown closed it at the deadline ([ADR-0038](decisions/ADR-0038-an-ordered-shutdown-is-a-state-not-a-flag.md)) |

`DropReason` is `#[non_exhaustive]`; match it with a `_` arm. The engine pushes it to the
operator on the event stream ([GUIDE.md §8a](GUIDE.md), *Why a connection ended*).

---

## 2. The seven messages the session answers itself

Administrative traffic never reaches your application. The session emits these on its own
(`enum Which` in `crates/session/src/lib.rs`):

| Emitted | When |
|---|---|
| Logon (`35=A`) | to accept a valid counterparty Logon, or to open a session as initiator |
| Logout (`35=5`) | to end the session, and in reply to a peer Logout |
| Heartbeat (`35=0`) | every `HeartBtInt`, and in reply to a TestRequest |
| TestRequest (`35=1`) | when the interval passes with nothing heard; one unanswered TestRequest, then `HeartbeatTimeout` |
| Reject (`35=3`) | a session-level reject; see §3 |
| ResendRequest (`35=2`) | when an inbound sequence number is higher than expected |
| SequenceReset gap fill (`35=4`, `123=Y`) | to fill a gap in the outbound stream during a resend |

The TestRequest id (`112=`) is a fixed constant, because the oracle makes it one:
`6_SendTestRequest.def` writes `112=TEST`, and FIX 4.4 does not fix its value.

---

## 3. Session-level rejects: which `373=` comes back

A well-framed but malformed message earns a Reject (`35=3`) naming `SessionRejectReason`
(`373=`). The message is walked once in wire order and the **first fault wins**; the corpus
depends on that order.

| `373=` | Meaning | Held by |
|---|---|---|
| `0` | invalid tag / garbled | `2d_GarbledMessage.def` |
| `1` | required field missing | `2g_PossDupNoOrigSendingTime.def` (names `122`) |
| `4` | tag specified without a value | `14d` (`56=` empty, which wins over the CompID mismatch) |
| `9` | CompID problem | the `14*` family |
| `10` | SendingTime accuracy | `2m_BodyLengthValueNotCorrect.def` (via `122` OrigSendingTime) |
| `11` | invalid MsgType | `2r_UnregisteredMsgType.def` |

The order itself is tested: `14d` proves the required-field check runs before the CompID
check, and the `14h` family proves the MsgType check runs after the required-field check.

**The dictionary half of that table is callable on its own.**
`fixbolt_session::validate(view, msg_type) -> Option<SessionText>`
([ADR-0050](decisions/ADR-0050-the-dictionary-pass-is-public-so-it-can-be-timed.md)) runs the
field scan, the required tags and the group counters in exactly the order above and returns the
first fault, without a session. It therefore answers `373=` 0, 1, 4 and the group-count and
value-format faults; it does **not** answer 9, 10 or 11, which need CompIDs, a clock and a
session state that a bare view does not carry. It was made public so the pass could be timed —
`[measured 2026-09-05]` 897.3 ns on a `NewOrderSingle`, `DESIGN.md` §8 — and it is the same
code path, not a copy of it.

**A session reject is not a business reject.** `2r_UnregisteredMsgType.def` sends `35=8` as
an application message of an unsupported type, and the engine answers with a *business*
reject, not a `373=`. Unsupported application types are the application's concern; malformed
session plumbing is the session's.

---

## 4. Sequence numbers, gaps and resends

- **A gap triggers a ResendRequest.** An inbound `34=` above the expected number means
  messages were missed. The session asks for them and holds newer messages until the gap
  fills.
- **Gap fill defaults to `123=N`.** A SequenceReset with no `123=` is read as `N`; `11a` and
  `11b` leave it out.
- **Reset versus gap fill.** A SequenceReset with `123=N` and one with `123=Y` move the
  expected number differently. The table beside the resend logic in
  `crates/session/src/lib.rs` records which restarts the counts.
- **`141=Y` on a Logon resets both counts** before the Logon's own numbers are applied. It is
  the only thing *on the wire* that restarts a resumed session's counters.
- **`ResetPolicy` restarts them from this end** `[2026-09-05]`, and is the only other thing
  that does. `Config::with_reset(ResetPolicy::new().on_logon() / .on_logout() /
  .on_disconnect())` — QuickFIX's `ResetOnLogon`, `ResetOnLogout` and `ResetOnDisconnect`.
  **The default resets on nothing**, which is the behaviour the 59 definitions prove.
  - `on_logon` restarts both counts in `Session::connect`, **including for a resumed session**,
    which is the only case where it changes anything.
  - `on_logout` and `on_disconnect` restart them in `Session::end` — **after** the message that
    ends the session has been written, so a `Logout` still carries the number it was owed
    rather than spending `34=1` twice.
  - It is not the same choice as `Session::new` versus `Session::resume`: those say what the
    journal still holds, this says what the session wants next time.
  Guarded by `crates/session/tests/logon.rs::reset_on_logon_restarts_a_resumed_sessions_numbers`,
  `::reset_on_disconnect_restarts_the_numbers`,
  `crates/session/tests/goodbye.rs::reset_on_logout_restarts_the_numbers_only_after_the_goodbye_is_numbered`,
  and their three neutral twins — `::the_default_reset_policy_leaves_a_resumed_session_counting`,
  `::a_disconnect_without_the_policy_keeps_the_numbers`,
  `::a_logout_without_the_policy_keeps_the_numbers` — which are what say the flag and not the
  code path is doing the work.
- **`16=0` and a range past what was sent are clamped** to the last number this end actually
  sent. Guarded by
  `crates/engine/tests/journal.rs::what_no_longer_fits_in_the_ring_is_filled_over_not_skipped`.
- **A resend is answered in batches** `[2026-09-04]` of `Config::resend_batch` messages per
  call; a replayed message and a gap fill each count as one, whatever range the gap fill
  covers. The rest follow on later calls, interleaved with new traffic: a replay carries its
  original `34=` with `43=Y`, a new message carries the next new number. `Session::tick_with`
  and each judged message continue a replay; **plain `Session::tick` does not**, because it
  has no journal and would gap-fill the remainder. Guarded by
  `crates/engine/tests/journal.rs::a_long_resend_is_replayed_over_several_ticks_in_order`,
  `::a_replay_stalls_on_the_journal_less_tick_and_says_nothing_wrong` and
  `crates/engine/tests/backpressure.rs::a_resend_larger_than_tx_does_not_end_the_session`
  ([ADR-0046](decisions/ADR-0046-the-ring-is-the-resend-store-and-a-replay-goes-in-batches.md)).
- **A replay in progress is cancelled** `[2026-09-04]` by a disconnect, by a schedule boundary
  that restarts the counts, and by a newer ResendRequest, which replaces it rather than
  queueing behind it. Guarded by `::a_disconnect_cancels_a_resend_in_progress`,
  `::a_schedule_reset_cancels_a_resend_in_progress` and
  `::a_new_resend_request_replaces_the_one_in_progress`.
- **Numbers older than the journal ring are gap-filled and counted** `[2026-09-04]`.
  `SessionSnapshot::resend_beyond_journal` and
  `EventKind::ResendBeyondJournal { filled, oldest }` count messages, not occurrences. A fill
  over an administrative message is not counted, because none was ever replayable. Guarded by
  `::a_resend_that_reaches_below_the_ring_counts_every_number_it_filled` and
  `::a_fill_over_messages_the_ring_never_held_is_not_counted`.
- **The journal always knows the highest outbound number spent** `[2026-09-05]`, including the
  numbers it holds no bytes for — a `Logon`, a `Heartbeat`, a `Logout`, and an application
  message `put` refused. `Journal::mark_out` is a high-water mark, so a `put` that was kept
  raises it and telling it again writes nothing. A resumed session's `next_out` is
  `highest_out() + 1`, **never `highest() + 1`**, which is the highest message held for a
  *replay*. Guarded by `crates/session/tests/numbering.rs` — in particular
  `::three_administrative_messages_and_the_journal_knows_the_count` — and
  `crates/engine/tests/on_disk.rs::the_outbound_count_survives_a_restart_and_is_not_highest`
  ([ADR-0053](decisions/ADR-0053-the-journal-answers-two-questions-and-the-second-is-a-number.md)).
- **The inbound mark covers the message that ended the session** `[2026-09-05]`. The
  counterparty's own `Logout` is judged, consumed, and answered, and it is marked like any
  other — the mark is taken on every path out of `Session::received_with`, not only the ones
  that leave the link up. Before this the number it arrived under was consumed and never
  recorded, so a resumed session expected it again and opened a gap on the counterparty's next
  message. Guarded by
  `crates/session/tests/numbering.rs::the_logout_that_ends_the_session_is_still_a_message_that_was_consumed`
  and by `scripts/interop.sh`'s `interop-reconnect-logout: no_resend`, which is what found it.

---

## 5. PossDup, PossResend and OrigSendingTime

- **`43=Y` (PossDup)** marks a possible duplicate and requires `122=` OrigSendingTime.
  `2g_PossDupNoOrigSendingTime.def` omits it and earns `373=1`.
- **`97=Y` (PossResend)** marks a message the sender may have sent before.
  `19b_PossResendMessageThatHasNotBeenSent.def` sends one the receiver has not seen.
- **`122=` OrigSendingTime** is checked against SendingTime; `2m_BodyLengthValueNotCorrect.def`
  exercises that path.

---

## 5a. A message the application originated

`[added 2026-09-05]` An application can send a message no inbound message asked for, through
`Handler::on_logon` or a `Sender` ([`GUIDE.md` §8d](GUIDE.md), `DESIGN.md` D15,
[ADR-0048](decisions/ADR-0048-an-engine-that-can-speak-first-has-two-doors.md)). What the
session does with it:

| Rule | Behaviour | Guarded by |
|---|---|---|
| The number is the session's | `34=` is the session's `next_out`, and it is spent. Whatever the application wrote is ignored | `crates/engine/tests/originate.rs::what_an_application_writes_into_34_and_52_is_ignored` |
| The clock is the session's | `52=` is stamped from the session's last tick, not from anything the application supplied | the same test |
| The frame is rebuilt | `8=`, `9=` and `10=` are written by the session; body fields are reordered from the generated tables | `Session::send_application`, and non-negotiable 5 |
| It is kept for a resend | an originated message goes into the journal ring like any other outbound application message, and replays with `43=Y` | `scripts/interop.sh` acceptor role, step `resend` |
| Before Logon and after Logout, nothing happens | the session discards it and reports `Link::Up`. **Not an error**: an application that offers a message to a session that is not up has done nothing wrong | `Session::send_application`'s state check |
| A full ring, a dead connection | a `Sender` message for a connection that has gone is dropped and counted (`EventKind::OriginationUndeliverable`); a full queue is refused at the call | `crates/engine/tests/originate.rs::a_message_for_a_connection_that_has_gone_is_dropped_and_counted`, `::a_full_queue_refuses_at_the_call_rather_than_losing_a_message` |
| Ordering within a turn | a message queued from another thread goes out **before** any reply that turn produces, because it has been waiting since the previous turn | `crates/engine/tests/originate.rs::an_origination_and_a_reply_in_one_turn_do_not_corrupt_each_other` |

**None of this is in the acceptance corpus.** Every `.def` file is stimulus-then-response, so a
message needing no stimulus cannot be written in that format — `[measured 2026-09-05]` removing
the door entirely leaves `--test score` at 59 / 59. The gate that holds it is
`scripts/interop.sh`'s acceptor role, judged by `libquickfix`.

---

## 6. What this page does not claim

The 59 `.def` files are a conformance corpus, not an adversarial one
([reference/a-conformance-corpus-is-not-an-adversarial-one.md](reference/a-conformance-corpus-is-not-an-adversarial-one.md)).
A behaviour with no `.def` file and no test behind it is not listed here. If a boundary
decision matters to you and is missing, read `crates/session/src/lib.rs` rather than assume,
and if the behaviour is real and untested, it deserves a test before it deserves a row here.

**To see what actually happened on a connection**, turn on the message log with
`FileLogPath` `[2026-09-04]`. It writes every message the engine saw or sent, one line each,
**including frames refused before the session judged them**, which is exactly the class this
page cannot describe because a refusal that never reaches the session has no session
behaviour. The log is written at the engine's edges; the session neither knows about it nor
changes because of it ([GUIDE.md §6c](GUIDE.md), [DESIGN.md](DESIGN.md) D14).
