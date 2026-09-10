# Conformance Results

What this engine has been measured to do, with the command, the machine and, where it ran in
CI, the run id. **Nothing appears here that is not also listed under *Proven* in
[STATUS.md](../STATUS.md).** A figure without a command that was run is not a result.

Correctness figures need a command and a machine, not OS tuning, because they do not depend
on how the box was set up. Latency is a different axis and lives in [DESIGN.md §8](DESIGN.md)
and the [HFT playbook](hft-playbook.md), never on this page.

---

## 1. The 59 QuickFIX acceptance definitions

The 59 definitions are the session layer's primary gate. They pass through four independent
paths, so the score measures four code paths rather than one path four times:

| Path | Command | Result |
|---|---|---|
| In process, pure session machine | `cargo test -p fixbolt-session --test score` | **59 / 59** |
| Through a real kernel-TCP socket | `cargo test -p fixbolt-engine --test wire` | **59 / 59** |
| In `standard` mode, engine blocking between steps | the same wire test, second case | **59 / 59** |
| Through two shards, routed by counterparty | `cargo test -p fixbolt-engine --features affinity --test shard_wire` | **59 / 59** |

Measured on an Apple M5 **and** on Linux x86_64 (AMD Ryzen 7 3700X). The control that gives
the score meaning: a fake that replays each file's own expected output also scores 59 / 59,
so the runner is not scoring itself.

`[CI]` run [`33623429649`](https://github.com/tmthang86/fixbolt/actions/runs/33623429649) on
commit `cdd6fba`, 10 checks of 10.

---

## 2. Dictionary agreement with QuickFIX

The generated tables were checked against QuickFIX's own generated C++ `[measured 2026-08-28]`:

| What | Result |
|---|---|
| Tag numbers | **912 / 912** |
| Field types | **898 / 912**; the 14 differences are each named by tag |
| (message, tag) pairs | **12 524 / 12 524**, checked as 84 816 answers in both directions |
| Enum values | **1 708 / 1 708** |
| Message types | **93** |

Eight reversals were run against these tables and all eight went red, so the tables are
proven able to disagree, not merely observed to agree.

---

## 3. Repeating groups

`[measured]` Field order inside groups agrees with QuickFIX's generated C++ on **730 / 730**
groups (731 with `NoHops(627)`, which lives in the header). Groups are read and written nested
to depth 4 with **0 allocations** walking all four levels.

---

## 4. Zero allocation on the hot path

`benches/alloc.rs` runs a counting allocator over every hot path and asserts zero:

| Crate | Paths counted | Allocations |
|---|---|---|
| `codec` | 6 | **0** |
| `session` | 16 | **0** |
| `engine` | 24 | **0** |

Each case asserts that its own path is live, so a case that stopped exercising its path fails
instead of passing silently. `[measured]` The guard is proven by reversal: one `to_vec()` in
the timed loop takes the counter from 0 to 2 000 over 2 000 messages.

---

## 5. Codec micro-figures

`[measured 2026-08-27, Apple M5, a development laptop and not a tuned machine]` parse
**77.0 ns**, encode **93.8 ns**, **0 allocations**, and **304 million** fuzz executions with no
failure. These say the code is fast on that laptop, not that it meets a budget on a tuned box.
The tuned-box figures are in [DESIGN.md §6](DESIGN.md).

---

## 6. What is not proven here

- **No latency figure is on this page.** Round-trip numbers belong to [DESIGN.md §8](DESIGN.md)
  and the playbook, each with its machine and settings.
- **The corpus is not adversarial.** 59 / 59 is agreement with a known oracle, not proof
  against inputs nobody wrote down
  ([reference/a-conformance-corpus-is-not-an-adversarial-one.md](reference/a-conformance-corpus-is-not-an-adversarial-one.md)).
- **The 14 field-type differences are real.** Agreement is 898 / 912, not 912 / 912.
- **The independent check is narrow.** Both roles are interop-green against a real
  `libquickfix` (§7), but that is **7 cases each, not 59**. Everything else on this page is
  this repository's own runner reading QuickFIX's definitions.

- **TLS is gated for behaviour, not for mode.** `[2026-09-10]` the `tls` CI job runs
  `clippy --features tls` and the three TLS test files on a runner that is asserted to be able to
  offload TLS. What it proves: the handshake completes, the kernel takes the keys, the userspace
  fallback carries the same bytes and says it is not the kernel, a session comes up through
  `serve_tls`, and `TlsRequireKernel` refuses at both layers. **What it does not prove: anything
  about the engine thread under TLS** — `scripts/check-no-kernel-sleep.sh` has no TLS arm, so no
  claim here touches non-negotiable 4. **And no latency figure comes from it**;
  [DESIGN.md](DESIGN.md) §8's TLS row is still empty.

---

## 7. Interop against a real `libquickfix`, both directions

`[measured 2026-09-04]` **The only evidence on this page that this repository did not write.**
Another engine sits at the other end of a kernel socket and either agrees or does not.

Both directions run in **one script and one CI job**, against `libquickfix` built from source
at commit `386ce46e917ae494ab6e90b1be90fd421cdbe3f9`. That is the same commit
`scripts/fetch-quickfix-assets.sh` pins for the acceptance corpus, and the script checks the
two pins agree before running anything, so a disagreement between corpus and counterparty is
always attributable.

```
scripts/interop.sh
```

| Direction | Driver | Under test | Score |
|---|---|---|---|
| `interop:` | this engine's `Session<Initiator, 256>` over a blocking socket | a `libquickfix` `SocketAcceptor` | **7 / 7** |
| `interop-acceptor:` | a `libquickfix` `SocketInitiator` | `fixbolt::serve` in `standard` mode: the poller, the pre-session table, the settings file, the library `Handler` and the session layer under all of it — **and, since 2026-09-05, `Admin::shutdown` through the front door**: the process is asked to stop rather than killed, and `serve` returns a `Shutdown` (ADR-0054, `STATUS.md` item 47) | **8 / 8** |
| `interop-reconnect:` | a `libquickfix` `SocketAcceptor` **killed with `SIGKILL` and restarted on the same `FileStore`** | `fixbolt_engine::connect_and_serve`: the reconnect ladder, the `Recovery` seam, `add_resumed`, the engine turn, and — since ADR-0054 — the durable count against the live one read through an `Observer` | **6 / 6** |
| `interop-reconnect-logout:` | the same, **stopped with `SIGTERM`** so it says goodbye first | the same, after a clean logout — ADR-0043 decision 5 | **6 / 6** |
| `interop-reconnect-beat:` | the same as the `SIGKILL` row, at **`HeartBtInt=1` with a pause before the kill**, so a `Heartbeat` is guaranteed between the last application message and the death | the same, with the last number spent belonging to a message no journal holds bytes for — ADR-0053 | **6 / 6** |
| `interop-next-expected:` | a `libquickfix` `SocketAcceptor` with **`SendNextExpectedMsgSeqNum=Y`** | this engine's initiator with `Config::with_next_expected(true)`: `789` written on the way out and read on the way in | **9 / 9** |
| `interop-micros:` | a `libquickfix` `SocketInitiator` with **`TimestampPrecision=6`** | `fixbolt::serve` with `TimestampPrecision=6`: `52=` written at 24 bytes and a counterparty's 24-byte `52=` read — **judged on the bytes of QuickFIX's own transcript, not on a step line** | **5 / 5** |
| `interop-odd:` | a `libquickfix` `SocketInitiator` with **`TimestampPrecision=2`** — a width this engine **cannot be configured to send** | `fixbolt::serve` at its default: a 20-byte `52=` read on the way in, on both readers, when nothing here can produce those bytes | **3 / 3** |
| `interop-reset:` | a `libquickfix` `SocketInitiator` at `ResetOn*=N` with a `FileStore` that survives, run **twice** against **this engine stopped and restarted on the same `FileJournal`** | `fixbolt::serve_with_recovery` with `ResetOnLogon` at **`Y` and at `N`** — the resumed session's Logon reply read off the counterparty's transcript. **The knob is the only line that differs between the two arms**; before this row existed the two values produced identical wire traffic, because `serve` has no `Recovery` seam and the branch was unreachable (`STATUS.md` item 53) | **4 / 4** |

**`[measured 2026-09-09]` the `interop-micros:` row is the only one here that is judged on field
widths rather than on steps, and it is why the row exists.** QuickFIX C++ accepts a
`UTCTimestamp` of any length from 17 to 27 bytes, so a run in which this engine silently stayed
at milliseconds would log on, exchange orders, resend, gap-fill and log out — every step gate
green. What separates the two outcomes is a count of what is **not** there. The scenario reads
QuickFIX's own view of the wire and asserts: at least one 24-byte `52=` from this engine, at
least one from `libquickfix`, **zero** 21-byte stamps, and **zero** `35=3` naming tag 52. It
reads `12 from fixbolt, 10 from libquickfix, 0, 0`. Proven by reversal: with this engine put
back to `TimestampPrecision=3` it reads `0 from fixbolt` and `12` millisecond stamps, and fails.

**`[measured 2026-09-09]` the `interop-odd:` row is the one no fixture in this repository could
fake.** `settings.rs` refuses `TimestampPrecision=2` — this engine writes 3, 6 or 9 digits and no
others (ADR-0057 decision 1) — so the 20-byte stamps under test can only come from the other end,
and the scenario exists because `STATUS.md` open item 59 was about widths **fixbolt never sends**.
The three assertions are: at least one 20-byte `52=` from `libquickfix` (without it the run proves
nothing, because an oracle that ignored the key would look identical to a pass), the session
**logs on**, and **zero** `35=3` naming tag 52. It reads `10, logon ok, 0`.

**Proven by reversal, and the reversal is the item's own defect.** With `session::clock::parse_utc`
put back to the four widths it had before
[ADR-0058](decisions/ADR-0058-a-timestamp-is-read-at-every-precision-and-written-at-three.md), the
scenario fails on **`the odd-precision session never logged on — a valid 52= was refused before
the Logon, in silence`** — and on that assertion alone: the other two still pass, so the setup is
proven valid at the moment the gate goes red. The stamp count moves **10 → 97** in the failing
run, which is the silence made visible: the C++ end reconnects over and over because it is being
hung up on without a byte of explanation.

**It found a defect on its first run**, and the defect was in the *receive* direction that a
plan had closed the day before: `dict`'s `UTCTIMESTAMP` reader still refused six- and nine-digit
stamps, so every message after the Logon came back `373=6`.
[one-field-two-readers](reference/one-field-two-readers.md).

`[measured 2026-09-06]` The table above is from run
[`34034849824`](https://github.com/tmthang86/fixbolt/actions/runs/34034849824) on `main`,
commit `06a6509`, **11 jobs of 11** — the run that tests what actually landed, not a pull
request's run against GitHub's merge ref. The interop job's log was read line by line, and doing
so found a shell error inside the passing job: two backticks in a comment inside an unquoted
heredoc made the fixture print `command not found` while the scenario went on to pass. Fixed,
with the config generation now asserting its own stderr is empty
([reading-the-output-you-grepped-for](reference/reading-the-output-you-grepped-for.md)). The
scores are unaffected — the mangled text was a comment — but the run that produced them carried
an error, and saying so is the point of naming a run by id at all.

The seven steps of the acceptor direction: `logon` (with `141=Y` echoed); `order` (two
`35=D`, two `35=8`, paired by `11=`); `heartbeat` (an unprompted `35=0` with **no** `112=`,
within a deadline read from the `108=` on the wire); `testrequest`; `resend` (**the two
original sequence numbers replayed with `43=Y`**, not merely something carrying `43=Y`);
`gapfill` (the acceptor asks `35=2 7=n 16=0`, and a fresh TestRequest after the gap fill is
answered); `logout`. The eighth is the script's own, not the C++ initiator's: `shutdown` — a
line on stdin reaches `Admin::shutdown` through `fixbolt::serve`, and the process **returns**
and prints its `Shutdown` instead of being killed. `[measured 2026-09-05]` removing the
`admin.shutdown` call makes it read *"this engine's acceptor did not return from serve within
10s"* and fail in 29 seconds — the bound is there because a reversal that hangs proves nothing
([a-reversal-can-fail-by-hanging](reference/a-reversal-can-fail-by-hanging.md)).

**A stop line, and only a line.** `[measured 2026-09-05]` the first version of that trigger
treated end-of-input as the signal too, and this job is what refuted it: a background process on
a runner has no terminal on stdin, so it read `interop: stopping on ""` and the acceptor
**stopped before the counterparty connected**. Only a non-empty line stops it now; `< /dev/null`
leaves it serving and says so
([a-control-channel-the-launcher-already-closed](reference/a-control-channel-the-launcher-already-closed.md)).

**The `789` scenario, and why two of its nine assertions are preconditions.**
`[measured 2026-09-06]` this engine sent `789=1` on its `Logon` and `libquickfix` accepted it;
`libquickfix` answered with `789=2`, which is the same off-by-one this engine implements on its
own acceptor side, arriving from the other direction. Both are asserted **before** anything
about the response, because the gate can otherwise pass having tested nothing: QuickFIX C++
reads settings by name on demand with no validation pass, so a key it does not recognise is
ignored in silence. Run with the QuickFIX/J spelling `EnableNextExpectedMsgSeqNum=Y`, the seven
ordinary steps read **`PASS 7/7`** over a field that never crossed the wire, and only the
`received` precondition tells the two runs apart. Written up, with the table, in
[who-owns-the-outbound-header](reference/who-owns-the-outbound-header.md).

**`369=LastMsgSeqNumProcessed` is not in this table and cannot be.** `[verified 2026-09-06]`
QuickFIX C++ never sends the field and has no configuration key for it — the string occurs once
outside its generated tables, in `Message::isHeaderField` — and 0 of the 59 acceptance
definitions carry the tag. So the send direction of `369` is proven **only by this repository's
own tests**: `crates/session/tests/application.rs`, `crates/library/tests/reply.rs`, and
`crates/engine/tests/settings_wire.rs` for the file-to-socket seam. That is stated here rather
than left for a reader to infer from the table's silence. An outside opinion would need
QuickFIX/J, QuickFIX/n or quickfixgo standing up in the fixture, and that is a different plan.

The six steps of the `SIGKILL` scenario: `dropped` (no `35=5` in the first transcript, so the
ending really was abrupt); `back` (a Logon reaches the restarted acceptor, and **nothing told
this engine to send it** — `reconnect::Policy` did); `next_out` (**relational**: that Logon's
`34=` is one past the last number this engine sent before the kill, read off the transcript
rather than written as a literal); `next_in` (every `35=B` the restarted acceptor sends is
*delivered to the application*, which a session whose inbound count had restarted would have
gap-requested instead); `no_resend` (no `35=2`, no `141=Y`, no `MsgSeqNum too low`, printed beside the number of times
the engine resumed — see below); and `two_sources`, which needs
ADR-0054's handles to exist at all — the number the journal resumes from is never **behind** a
number an `Observer` already saw spent.

**`two_sources` is an inequality, and that is not a weakened equality.** `[measured
2026-09-05]` the first version demanded `live == resumed + 1` and read `resumed 4, live 6`: the
application also speaks first on logon, so the constant was wrong, and a gate resting on a
constant like that breaks when the *application* changes. The deeper reason is ADR-0053's own
argument — an observer knows the number *when somebody asks*, so a message sent between the
last poll and the ending is spent, durable and invisible on that side. On a clean logout it
always is: answering the counterparty's `35=5` and dropping the link happen inside one turn.
Sampling can only make the live number **low**, so the direction is safe where equality is a
race. Its teeth are measured: with the outbound mark removed, `interop-reconnect-beat` reads
`BEHIND (resumed 3, already seen live 5)` — and the logout scenario stays green, which is the
limit above, stated by the gate itself.

The `SIGTERM` scenario makes the **same five**, with `goodbye` in place of `dropped`, plus
`two_sources`. `[measured
2026-09-05]` **until this week it could only make three**, and the third was `known_gap`, which
pinned a known limitation on purpose: after a clean logout this engine answered the venue's
`35=5`, spending an outbound number the journal did not record, so the resumed session was
refused by exactly one — `STATUS.md` item 48. The journal now records that count
([ADR-0053](decisions/ADR-0053-the-journal-answers-two-questions-and-the-second-is-a-number.md)),
so the assertion that pinned the gap is gone and the scenario asserts continuation like the
other two.

**And the third scenario exists because the first two were green for a reason that was not the
engine.** Both ran at `HeartBtInt=30`, chosen so no `Heartbeat` could fall inside their few-second
window — which is exactly the condition item 48 was about, chosen by the fixture. The
`interop-reconnect-beat` round removes it: `HeartBtInt=1` and a deliberate 2.5 s pause before the
kill, so the last number spent belongs to a `Heartbeat`. It came back at `34=5` having sent up to
`34=4`, which is what says the fix is about *every* administrative message rather than about
`35=5`.

**`no_resend` has a premise, and it is printed now.** `[measured 2026-09-05]` the step asserts
no `35=2` in the restarted venue's transcript, which assumes the engine reconnected **once**.
`SIGTERM` makes QuickFIX log its sessions out and *then* shut down, so its listening socket
outlives the goodbye — and on a loaded runner the reconnect ladder's 200 ms first rung dialled
into a venue that was already stopping, spent a number on a `Logon` nobody would answer, and came
back one higher than the restarted venue expected. The venue asked for a resend: **both ends
correct, and the gate red with a message about numbering that had not happened.** The fixture now
takes the listener away as soon as the goodbye is answered, and the step prints `resumes: N`.

**The `SIGTERM` scenario's `no_resend` step is what found a second defect**, in the inbound
direction: the counterparty's `Logout` was consumed and never marked, so a resumed session
expected it again and sent a `ResendRequest` for a message it already had. `35=2: 1` where it
wanted none. The gate found it; this repository's own tests did not.

**Machine and run for these three.** The **3 / 3** the `SIGTERM` row used to carry was gated on
`ubuntu-latest` in the blocking `interop` job: job
[`101218167551`](https://github.com/tmthang86/fixbolt/actions/runs/33933972142/job/101218167551)
of run [`33933972142`](https://github.com/tmthang86/fixbolt/actions/runs/33933972142), commit
`c839854` — **the merge commit on `main`, not the branch tip**, because a branch being green
says nothing about the commit where it meets `main`. 11 jobs of 11.

**The 8 / 8 and the three 6 / 6 are CI's, on the commit that changed them.** `[measured
2026-09-05]` runner image `ubuntu-24.04` (20260831.293.1), cmake 3.31.6, g++ 13.3.0, libquickfix
built from the pinned `386ce46e` (3 262 136 bytes): job
[`101312682472`](https://github.com/tmthang86/fixbolt/actions/runs/33968467214/job/101312682472)
of run [`33968467214`](https://github.com/tmthang86/fixbolt/actions/runs/33968467214) on
`ccf90a9`, **22 checks of 22 across that run and
[`33968465621`](https://github.com/tmthang86/fixbolt/actions/runs/33968465621)**. The job's own
log was read rather than its conclusion, and every line is the development run's:

```
interop: PASS 7/7
interop-acceptor: PASS 7/7
interop-acceptor: shutdown ok Shutdown { sessions: 0, said_goodbye: 0, acked: 0, timed_out: 0 }
interop-reconnect-logout: goodbye     ok    35=5 out: 1, answered by this engine: 1
interop-reconnect-logout: next_out    ok    sent up to 34=3 before the kill, came back at 34=4, wanted 34=4
interop-reconnect-logout: no_resend   ok    35=2: 0, 141=Y: 0, 'MsgSeqNum too low': 0, resumes: 1
interop-reconnect-beat:   next_out    ok    sent up to 34=4 before the kill, came back at 34=5, wanted 34=5
interop-reconnect-beat:   two_sources ok    journal vs Observer: ok 1 resume(s), journal never behind the live count
==> the run added nothing git can see
interop: 7 / 7 + 8 / 8 + 6 / 6 + 6 / 6 + 6 / 6 against libquickfix @ 386ce46e…
```

**Two earlier commits on this branch were red for this job, and are named rather than left in
the run history.** `3b817d7`
([`101308740830`](https://github.com/tmthang86/fixbolt/actions/runs/33966987593/job/101308740830))
— the acceptor role read a stdin the script did not yet supply, and treated its end as a stop.
`9a20562`
([`101311876479`](https://github.com/tmthang86/fixbolt/actions/runs/33968159513/job/101311876479))
— the `SIGTERM` venue's listener outlived its goodbye, the engine dialled into it mid-shutdown,
and `no_resend` reported a correct exchange as a numbering failure. Both are fixed, and both are
in the write-ups above. §8 asks for gates green **for the commit**, not for the branch tip.

**And the merge commit's own run, which is what §9's last box actually asks for.**
`[measured 2026-09-05]` `edb0121` on `main` — *"Merge pull request #44"* — run
[`33973287134`](https://github.com/tmthang86/fixbolt/actions/runs/33973287134), **11 jobs of
11**, job
[`101325480975`](https://github.com/tmthang86/fixbolt/actions/runs/33973287134/job/101325480975)
for this table. Its log, again read line by line:

```
interop: PASS 7/7
interop-acceptor: PASS 7/7
interop-acceptor: shutdown ok Shutdown { sessions: 0, said_goodbye: 0, acked: 0, timed_out: 0 }
interop-reconnect-logout: goodbye     ok    35=5 out: 1, answered by this engine: 1
interop-reconnect-logout: next_out    ok    sent up to 34=3 before the kill, came back at 34=4, wanted 34=4
interop-reconnect-logout: no_resend   ok    35=2: 0, 141=Y: 0, 'MsgSeqNum too low': 0, resumes: 1
interop-reconnect-beat:   next_out    ok    sent up to 34=4 before the kill, came back at 34=5, wanted 34=5
interop-reconnect-beat:   two_sources ok    journal vs Observer: ok 1 resume(s), journal never behind the live count
==> the run added nothing git can see
interop: 7 / 7 + 8 / 8 + 6 / 6 + 6 / 6 + 6 / 6 against libquickfix @ 386ce46e…
```

**Why the two are not the same run, since the phrasing here used to be loose.** A PR job checks
out GitHub's **merge ref** — the `ccf90a9` log reads `HEAD is now at dd2aa12 Merge ccf90a9 into
2470f5f` — so it tests the branch merged into `main` *as of that base*, which is stronger than
the branch tip and still not the commit that ends up on `main`. `edb0121` is that commit, and it
is green. **11 jobs against the PR's 22** is not a smaller check: a PR fires the workflow twice,
once per event, and `main` fires it once.

**`[2026-09-05, later]` The same 33 assertions, unchanged, on `0f9585d`** — *"Merge pull request
#46"*, wave B's first plan — run
[`33998809034`](https://github.com/tmthang86/fixbolt/actions/runs/33998809034), **11 jobs of 11**,
job
[`101393895585`](https://github.com/tmthang86/fixbolt/actions/runs/33998809034/job/101393895585).
That commit added twelve configuration keys, changed `Settings::into_table`'s signature and put a
new question in the pre-session stage, and **the gate did not move**: `PASS 7/7`, `PASS 7/7` +
`shutdown ok`, three × `PASS 6/6` each with `resumes: 1` and `two_sources ok`. A gate that reads
the same before and after a change of this size is worth recording as such — it is the reason to
believe the change was additive.

**These two scenarios are the first evidence for `connect_and_serve` that this repository did
not write.** ADR-0043 said so in its own *Consequences*: *"every test of this is invented … only
an interop scenario driving a real counterparty through a disconnect would close that"*.

**Machine and run.** Recorded on macOS 15 (Apple M5) and gated on `ubuntu-latest`
(cmake 3.31.6, g++ 13.3.0) in the blocking `interop` CI job: job
[`100900997589`](https://github.com/tmthang86/fixbolt/actions/runs/33833427382/job/100900997589)
of run [`33833427382`](https://github.com/tmthang86/fixbolt/actions/runs/33833427382), commit
`f94e36e`. The job's own log was read rather than its conclusion: `interop: PASS 7/7`,
`interop-acceptor: PASS 7/7`, `==> the run added nothing git can see`.

### What these 33 cases do not buy

- **They are not a second corpus.** Seven and eight cases in the two directions, eighteen
  across the three reconnect scenarios, against 59 definitions.
- ~~**The reconnect scenarios do not cover a fixbolt process that restarts.**~~
  `[2026-09-10]` **they still do not, and `interop-reset:` does.** That scenario stops this
  engine and starts it again on the same `FileJournal`, so recovery across *this* process ending
  now has an independent opinion — which is what `crates/engine/tests/on_disk.rs` never had.
  The three reconnect scenarios are unchanged: only the venue dies in those.
- **`hft` mode is not covered here, and that is a property of this script, not of the mode.**
  `serve_hft` spins a core at 100% and a shared CI runner is the wrong place for it.
  `[2026-09-10]` **what changed is that the entry points are no longer ungated**:
  `crates/engine/tests/hft_wire.rs` drives `serve_hft` and `serve_hft_with_recovery` over kernel
  sockets and `crates/engine/tests/shard_hft.rs` drives `serve_sharded_hft` on Linux. Those are
  **behaviour** gates — they prove the doors open and serve, and they demonstrably cannot tell
  `hft` from `standard` (that file's reversal 2 swaps one for the other and stays green). The
  syscall-level half of non-negotiable 4 at the front door is still
  `scripts/check-no-kernel-sleep.sh`, and it still traces `tools/w2w` rather than `serve_hft`.
- **One counterparty, one identity, no TLS, no schedule, no shards.**
- **The scoring reads raw wire strings**, not `libquickfix`'s application callbacks. That is
  deliberate: QuickFIX drops a PossDup replay of a number it has already seen before the
  application sees it, and the `resend` step asks for exactly that. A judge written on the
  callbacks would report a correct answer as a missing message.

---

## 8. The `tls` job, and the two things it had to be built around

`[measured 2026-09-10]` **Before it, no CI job had ever run a TLS test** — `ci.yml` passed
`--features affinity` for the shard tests and `--all-features` to `cargo deny` and `cargo doc`,
and never `--features tls` to `cargo test`. Found by grepping a **green** run's log for a TLS
test name and getting **zero**. `STATUS.md` item 62.

**The environment is asserted first, and fails in its own words.** One of these tests asserts
`/proc/net/tls_stat` moved, which is a claim about the runner rather than about this engine.
Before the assertion, a kernel without the `tls` module surfaced four minutes later as
`tls_stat TlsTxSw did not move` — a line that reads as an engine defect. The job's first step
runs `scripts/check-ktls-available.sh` and requires the verdict `READY`.

`[measured 2026-09-10]` the runner: `Linux 6.17.0-1022-azure`, `CONFIG_TLS=m`, module loaded,
`/proc/net/tls_stat` present, `setsockopt(TCP_ULP, "tls")` **ACCEPTED**, verdict **READY**.

**The job counts the tests that ran, and fails at zero.** `[measured 2026-09-10]` dropping
`--features tls` from the command leaves **cargo at exit 0 having run 0 tests**, against **11**
with it. A green `cargo test` that compiled nothing is indistinguishable from a real one, so the
count is read rather than the exit status.

---
