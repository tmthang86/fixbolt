# Who owns the outbound header, and why `369` is not a one-line feature

`[researched 2026-09-06]` Written while re-verifying
[seq-resync-789-369](../plans/2026-09-04-seq-resync-789-369.md), wave B's second plan. The plan
had one line — *"when enabled, **every** message out carries `369=<next_in - 1>`, patched like
`34=`"* — that turned out to be impossible in this engine's shape, and the reason is not a
missing line of code. It is a structural difference between this engine and every engine it is
measured against.

Everything below was read out of source, not recalled. Sources are named per row.

---

## The finding

**In every QuickFIX-family engine, the engine owns the header of every outbound message,
including application messages, because the application hands the engine a *message object* and
the engine serialises it.** In fixbolt the application hands over *bytes it wrote itself*, and
the session emits them untouched. That choice is deliberate — it is what removes a rebuild per
reply — and it is also what puts `369` out of reach.

| Engine | The call | What it does to the header |
|---|---|---|
| QuickFIX C++ | `Session::sendRaw(Message&)` → `fill(header)` | sets `8`, `49`, `56`, `34`, `52` on **every** message, then `toString` |
| QuickFIX/J | `Session.sendRaw(Message, int)` → `initializeHeader(header)` | the same five, then `369` if enabled |
| QuickFIX/n (.NET) | the generic header fill before send | the same, then `369` if enabled |
| quickfixgo | `session.fillDefaultHeader(msg, inReplyTo)` | the same, then `369` if enabled |
| **fixbolt** | `Application::on_message(msg, seq, stamp, out)` | **nothing** — the application writes the whole message and `Session` emits `buf[r]` verbatim (`crates/session/src/lib.rs:2982`) |

fixbolt's session does own the header of the seven messages it generates itself (`out.rs`
templates) and of an *originated* application message (`rebuild()`, `lib.rs:3297`). The gap is
exactly the **reply** — which on an acceptor is the common case.

---

## The survey, field by field

`[verified 2026-09-06]` QuickFIX C++ read from the pinned vendor tree at
`386ce46e` (`scripts/fetch-quickfix-assets.sh`, then widened — see *Reproducing* below); QuickFIX/J,
QuickFIX/n, quickfixgo and `nanofix` fetched from their public repositories at `master`/`main` on
the same day; OnixS read from its published guide, so its rows are `[documented]`, not `[verified]`.

### `LastMsgSeqNumProcessed (369)`

| Engine | Sends it? | Config key | Value it writes | Application can override |
|---|---|---|---|---|
| QuickFIX **C++** | **No. Never.** | **none — the string does not exist in `SessionSettings.h`** | — | — |
| QuickFIX/**J** | yes, on every message | `EnableLastMsgSeqNumProcessed`, default `false` | `getExpectedTargetNum() - 1` | **yes** — skipped if the header already has it |
| QuickFIX/**n** | yes, on every message | `EnableLastMsgSeqNumProcessed`, default `false` | `NextTargetMsgSeqNum - 1` | **yes**, same guard |
| quickfix**go** | yes, on every message | `EnableLastMsgSeqNumProcessed` | `inReplyTo`'s own `34=` when replying, else `NextTargetMsgSeqNum - 1` | — |
| **OnixS** (commercial) | yes, on every message | `SpecifyLastMsgSeqNumProcessed` / `SetLastMsgSeqNumProcessed`, default `false` | not published | not published |
| `matthart1983/nanofix` | **no** — the string does not occur in `src/session.rs` | — | — | — |

**QuickFIX C++ knows `369` exists and does nothing with it.** Its only occurrence outside the
generated field tables is `Message.cpp:465`, a `case` in `isHeaderField` — so it parses and
sorts the field correctly and never writes or reads one. There is no key, no `setField`, no
branch.

**Three of the four that do implement it write the same value in the same place**: the generic
send path, `next inbound - 1`, guarded so an application-set value wins. **quickfixgo's is
better**: given the message being replied to, it writes *that* message's `34=` rather than a
count read at send time. Those differ whenever anything was received between the two moments,
and quickfixgo's is the one that answers the question `369` actually asks.

**It is not decoration.** OnixS's own guide says the option exists because **CME iLink requires
tag 369 on every message**. A venue can mandate it, so "nobody uses it" is not a defence.

### `NextExpectedMsgSeqNum (789)`

| Engine | Reads it? | Config key to **send** it |
|---|---|---|
| QuickFIX **C++** | yes | **`SendNextExpectedMsgSeqNum`** |
| QuickFIX/**J** | yes | **`EnableNextExpectedMsgSeqNum`** |
| QuickFIX/**n** | **no — the string does not occur in `Session.cs` at all** | — |
| quickfix**go** | yes | `EnableNextExpectedMsgSeqNum` |
| `nanofix` | no | — |

**The two QuickFIX families spell the key differently, and this repository had written down the
wrong one.** `prior-art.md` and the plan draft both said `EnableNextExpectedMsgSeqNum`, quoting
QuickFIX/J's configuration reference. The engine `scripts/interop.sh` links against is the
**C++** one, and it spells the key `SendNextExpectedMsgSeqNum`
(`vendor/quickfix/src/C++/SessionSettings.h:45`).

### QuickFIX C++'s receive logic, which is the one worth copying

`Session.cpp:198–290`, in order:

1. `141=Y` is applied **first** — the counts restart before anything else looks at them.
2. `789` is compared against `getExpectedSenderNum()`: **lower** sets a flag; **higher** is
   `generateLogout(reason)` then `disconnect()` and return; **equal** does nothing.
3. The `Logon` reply is generated.
4. `onLogon` fires.
5. **Only then** the retransmit runs, `789 .. next_out - 1`.

Two things fall out of that order. **There is no fourth branch for "`789` together with
`141=Y`"** — the reset already moved the number the comparison reads, so `141=Y` with `789=1`
lands in the *equal* branch on its own and anything else is caught by the two branches that
exist. And **the reply goes out before the replay**, which is not a nicety: a counterparty
receiving replayed messages before the `Logon` answer sees them on a session it does not yet
consider up.

**The off-by-one worth writing down.** An initiator sends `789 = getExpectedTargetNum()`
(`Session.cpp:691`). An acceptor's *reply* sends `getExpectedTargetNum() + 1`
(`Session.cpp:713`), with the comment *"+1 because incoming Logon did not increment the target
SeqNum yet"*. The right value depends on whether the inbound `Logon`'s own number has been
counted at the moment the field is written — so it is a property of where in the handler the
field is set, not of the role.

---

## What this costs a zero-copy engine

Three ways to put `369` on a reply, and the middle one is already refuted by this repository's
own numbers:

1. **Widen the application seam** — pass the value down to `on_message` so the application (or
   a `Reply` helper) writes it. Keeps the zero-copy property. Breaks every implementation of the
   trait, which is a compile error rather than a silent one. **This is the closest a
   byte-writing seam gets to what the others do** — and it is still weaker: the others
   *guarantee* the field, this only *offers* it, because an application that ignores the
   argument omits it.
2. **Rewrite the application's bytes after it returns** — this is exactly the per-reply rebuild
   that `[measured 2026-09-05]` costs ~24× the fast path (`STATUS.md` item 34) and that
   [ADR-0044](../decisions/ADR-0044-a-builder-that-is-not-moved-per-field.md) exists to avoid.
   **Refuted by measurement, not by taste.**
3. **Put `369` only on the messages the session already owns** — cheap, and **no engine
   surveyed does this.** It is also the semantically worst of the three: `369` on a Heartbeat
   but not on the ExecutionReport answering an order tells the counterparty about the quiet
   moments and nothing about the busy ones.

**The general shape, for anyone who has never heard of FIX:** *a protocol field that must appear
on every outbound message is not a feature of the field. It is a claim about who owns the
outbound envelope.* An architecture where the caller writes whole messages can offer such a
field; it cannot guarantee one, and the difference is invisible in a design document and obvious
the moment someone tries to add the field.

---

## The testing lesson **`[to testing-skills]`**

**A gate configured with a key the system under test does not recognise runs green and tests
nothing.**

`[measured 2026-09-06]` **This was run, both ways, and the numbers are below rather than the
prediction that used to be here.**

The plan's interop step reads: *run `scripts/interop.sh` again with QuickFIX configured to send
`789`*. Written with the key name this repository had recorded —
`EnableNextExpectedMsgSeqNum` — against the C++ engine, which spells it
`SendNextExpectedMsgSeqNum`, the run goes like this:

1. `[verified 2026-09-06]` QuickFIX C++ reads settings **on demand by name**:
   `SessionFactory.cpp:228` asks `settings.has(SEND_NEXT_EXPECTED_MSG_SEQ_NUM)`. There is **no
   validation pass over the dictionary** — an unrecognised key is never asked for, so it is
   never reported.
2. The C++ side therefore sends no `789`.
3. This engine's `789` branch is never reached.
4. The scenario exchanges its messages and **passes**, having exercised the one thing it was
   written to exercise exactly zero times.

Nothing is red, nothing logs, and the reviewer's evidence — *"interop passes with 789 enabled"* —
is a true sentence about a run in which `789` was never enabled.

**What was actually measured, on 2026-09-06, against `libquickfix` at `386ce46e`:**

| Key written in the C++ config | The seven ordinary steps | `789` on the counterparty's Logon |
|---|---|---|
| `SendNextExpectedMsgSeqNum=Y` (correct) | `interop: PASS 7/7` | **`789=2`** — present |
| `EnableNextExpectedMsgSeqNum=Y` (Java's) | **`interop: PASS 7/7`** | **absent** |

**The two rows are indistinguishable on everything the scenario used to check.** The wrong key
produced a session that logged on, exchanged two application messages, answered a `TestRequest`,
replayed a resend, survived a gap fill and said goodbye — seven green steps over a field that
never crossed the wire. Only the added precondition tells them apart, and under the wrong key it
reads:

```text
interop-next-expected: received    FAIL  the counterparty's Logon carried no 789= — is SendNextExpectedMsgSeqNum the right key?
```

**The other direction stayed green in both runs, and that is worth its own line.** This engine's
own `789=1` reached `libquickfix` and was accepted either way — the `sent` precondition passes in
both rows — because the counterparty's *reading* of the field never depended on the key at all.
Only its *writing* did. So a scenario asserting one direction would have been half a test while
looking like a whole one.

**The shape**: whenever a test's whole purpose depends on a configuration flag reaching the
system under test, *the flag arriving is itself an assertion*, and it is the one nobody writes.
A configuration surface that ignores unknown keys is a silent-failure surface — this repository
made the opposite choice for its own settings
([ADR-0040](../decisions/ADR-0040-a-configuration-file-refuses-what-it-does-not-understand.md):
an unknown key is a line-numbered error) and can still be tripped by somebody else's.

**The fix, in this case**: the scenario must assert that a `789` field was *observed on the
wire* before it asserts anything about the response to it. A test that cannot see its own input
arrive is not testing its subject.

---

## Reproducing the QuickFIX C++ citations

`scripts/fetch-quickfix-assets.sh` deliberately fetches a **narrow** slice — the dictionary, the
definitions, and the generated headers it uses as an ordering oracle. The session sources cited
above are not in it, and were pulled by widening the sparse checkout of the same pinned commit:

```sh
scripts/fetch-quickfix-assets.sh                 # the normal slice, pinned 386ce46e
cd vendor/quickfix
for f in Session.cpp Session.h SessionSettings.h SessionFactory.cpp Message.cpp; do
  echo "/src/C++/$f"
done >> .git/info/sparse-checkout
git read-tree -mu HEAD
```

**The script was not changed to do this by default, on purpose.** Its slice is what the *build and
the gates* need; these files are what one piece of research needed. Widening the default would
enlarge every fetch, and the counts the script verifies (59 definitions, 539 message lines, 244
checksum lines, 95 generated headers) are about the slice it guarantees. `vendor/` stays
gitignored either way — nothing here is committed ([ADR-0001](../decisions/ADR-0001-relationship-to-quickfix.md)).

---

## Sources

- QuickFIX C++, pinned `386ce46e` in gitignored `vendor/`: `src/C++/Session.cpp`,
  `src/C++/SessionSettings.h`, `src/C++/SessionFactory.cpp`, `src/C++/Message.cpp`,
  `spec/FIX44.xml`, `src/C++/fix44/Message.h`, `src/C++/fix44/Logon.h`
- [QuickFIX/J `Session.java`](https://github.com/quickfix-j/quickfixj/blob/master/quickfixj-core/src/main/java/quickfix/Session.java)
- [QuickFIX/n `Session.cs`](https://github.com/connamara/quickfixn/blob/master/QuickFIXn/Session.cs)
- [quickfixgo `session.go`](https://github.com/quickfixgo/quickfix/blob/main/session.go),
  [`config/configuration.go`](https://github.com/quickfixgo/quickfix/blob/main/config/configuration.go)
- [OnixS Java FIX Engine programming guide](https://ref.onixs.biz/java-fix-engine-guide/onixs-fix-engine-java/onixs-fix-engine/index.html),
  [OnixS FIX dictionary, tag 369](https://www.onixs.biz/fix-dictionary/4.4/tagnum_369.html)
- [`matthart1983/nanofix` `src/session.rs`](https://github.com/matthart1983/nanofix/blob/master/src/session.rs)

**Nothing from any of these is copied into this repository** — they are read as an oracle,
per [ADR-0001](../decisions/ADR-0001-relationship-to-quickfix.md).
