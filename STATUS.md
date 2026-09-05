# Current state

One screen. A pointer, not a store. Detail lives in the ADRs and the plan files.
**A stale status page is worse than none.**

Last updated: **2026-09-05** — **the front door hands out handles, so an engine reached through `serve` can be watched, administered and stopped.**

## Start here — 2026-09-05, latest: the front door hands out handles, and can be stopped

**Item 47 is closed.** `Engine::observer()`, `admin()` and `sender()` all need a `&mut Engine`,
and a caller who came through `serve` never holds one: the engine is built inside the function
and only a `Shutdown` comes back, after everything has already ended. So every capability items
30 and 46 added was **finished and unreachable at the same time** — nothing to watch with,
nothing to administer with, no door 2, and **no way to stop it**.

`observe::Handles` is that same cell, made by the caller **before** the engine, which then
adopts it (`Engine::adopt`, refused if the engine already has one — two cells on one engine are
two truths and every symptom of that is silence). All ten `serve*` / `connect_and_serve*`
functions take one as their last parameter. That contradicts ADR-0047 decision 2 deliberately
and in part; its other four decisions stand and it is not superseded.
[ADR-0054](docs/decisions/ADR-0054-the-handles-are-made-before-the-engine-and-the-engine-adopts-them.md).

**`docs/GETTING-STARTED.md:186` is true now.** It has said since 2026-09-03 that *"`serve`
returns when an operator stops the engine through `Admin::shutdown`"*, and until today **nobody
could do it**. `[measured 2026-09-05]`
`crates/library/tests/end_to_end.rs::an_operator_stops_the_front_door_and_serve_comes_back`
reads `sessions 1, said_goodbye 1, acked 1`, and `scripts/interop.sh` now **asks** its acceptor
role to stop instead of killing it: `interop-acceptor: shutdown ok Shutdown { … }`.

**`connect_and_serve` was quietly stealing the caller's events.** `Observer::events` *drains*
the ring, so two readers share events rather than each seeing them — and the reconnect loop was
one of those readers, taking every `LoggedOn` for its own backoff ladder. It was invisible
because the caller had no cell to be robbed of. It compares `Engine::logons()` across a turn
now; the reversal reads `the caller's Observer saw 0 LoggedOn event(s) of two`.

**The interop gate is `7 / 7 + 8 / 8 + 6 / 6 + 6 / 6 + 6 / 6`**, 33 assertions, up from 29.

**Four things this closing found that the plan did not name, and two of them were CI's.**

**One — `two_sources` cannot be an equality, and the reason was already written down.** The plan
asked the gate to assert that the durable `next_out` and the live one *"are the same number"*.
`[measured 2026-09-05]` the first version demanded `live == resumed + 1` and read `resumed 4,
live 6`: the application also speaks first on logon, so even the constant was wrong. The deeper
reason is ADR-0053's own argument, turned on the plan that came after it — an observer knows the
number **when somebody asks**, so a message sent between the last poll and the ending is spent,
durable and invisible on that side; on a clean logout it always is, because answering `35=5` and
dropping the link happen inside one turn. The assertion is the **direction** instead: what an
operator saw spent, the journal knows about. Sampling can only make the live side low, so the
inequality is safe where equality is a race — and it has teeth: with the outbound mark removed,
`interop-reconnect-beat` reads `BEHIND (resumed 3, already seen live 5)`.

**Two — four entry points now trip `clippy::too_many_arguments` at 8 against a ceiling of 7.**
They take an `#[allow]` whose comment points at ADR-0054, which had already recorded a `Serve`
builder as *deferred* with its reopening condition. The lint is that condition arriving early
and being declined once, on purpose, and it is recorded rather than muted so the next parameter
meets a fact instead of a habit.

**Three — the stop trigger treated end-of-input as the signal, and the `interop` job refuted
that within the hour.** The comment read *"EOF counts: a supervisor that closes the pipe has
stopped supervising"*. CI read `interop: stopping on ""` followed by `acceptor stopped:
Shutdown { sessions: 0, … }` — **the acceptor stopped before the counterparty connected**,
because a background process on a runner has no terminal on stdin and the read returns at once.
A launcher closing stdin is the ordinary case, not a signal. Only a non-empty line stops it now,
and both halves are checked: `stop` into the fifo gives `stopping on "stop"` and the gate's
`shutdown ok`; `< /dev/null` leaves the process alive with *still serving* in the log. Write-up:
[a-control-channel-the-launcher-already-closed](docs/reference/a-control-channel-the-launcher-already-closed.md),
**`[to testing-skills]`**.

**Four — the `no_resend` assertion had a premise nobody had written down, and CI hit it.** It
reads *"no `35=2` in the restarted venue's transcript"*, which assumes the engine reconnected
**once**. `SIGTERM` makes QuickFIX log its sessions out and *then* shut down, so its listening
socket outlives the goodbye; the ladder's 200 ms first rung dialled into a venue that was already
stopping, spent a number on a `Logon` nobody would answer, and came back one higher than the
restarted venue expected. It asked for a resend. **Both ends correct, and the gate red with a
message about numbering that had not happened.** The fixture takes the listener away as soon as
the goodbye is answered now, and the step prints `resumes: 1`. **This is the `HeartBtInt=30`
lesson mirrored**: there a fixture removed the condition under test and the gate went green for
the wrong reason; here a fixture created a condition the assertion never meant to judge and it
went red for the wrong reason. Both are in the same write-up.

**And the intermediate commit `3b817d7` was red for `interop`, which is said here rather than
left in the run history.** §8 asks for gates green *for that commit*, not for the branch tip.
Step 3 changed `tools/interop` and step 4 changed `scripts/interop.sh`, so between them the tool
read a stdin the script did not yet supply — the defect above, in its first environment. The fix
removes the whole class rather than the ordering: `d9a1cb9` is green in both runs and an EOF can
no longer stop anything.

`cargo test --all` **519 → 524**, 0 failed; `--no-default-features` 519; clippy `-D warnings`,
`fmt`, `check-lint-config.sh` (by reversal), `check-no-optional-deps.sh`, `check-links.py`
(1553 links, 0 dead) and rustdoc under `-D rustdoc::broken_intra_doc_links` all clean.
`benches/alloc.rs` gains `adopt-idle` and reads **0 across 30 of 30**; the reversal — one
allocation on the observed branch of `turn` — reads `adopt-idle 10000` while the unobserved
`idle` and `turn` cases stay 0, which is what says the case is on the path it claims.

**CI green on `ccf90a9`, 22 checks of 22**, runs
[`33968467214`](https://github.com/tmthang86/fixbolt/actions/runs/33968467214) and
[`33968465621`](https://github.com/tmthang86/fixbolt/actions/runs/33968465621) — §9's last box.
The `interop` job's log was read rather than its conclusion: `PASS 7/7`, `PASS 7/7`,
`shutdown ok Shutdown { … }`, three × `PASS 6/6` with `resumes: 1`, and
`==> the run added nothing git can see`. **Two earlier commits on this branch were red for that
job and are named rather than left in the run history**: `3b817d7` (the acceptor read a stdin
the script did not yet supply) and `9a20562` (the `SIGTERM` venue's listener outlived its
goodbye). **And the phrasing here used to be loose**: a PR job checks out GitHub's **merge ref**
— `dd2aa12`, *"Merge `ccf90a9` into `2470f5f`"* — so it is this branch merged into `main` as of
that base, not the branch tip alone, and still not the commit that will exist on `main`
afterwards. That run is owed when this lands.

**Three reversals, each red where it was aimed and green elsewhere:** an `adopt` that makes its
own cell (`the pre-made handle never saw the session log on`); `dial` reading the ring again
(`saw 0 LoggedOn event(s) of two`); and the interop stop disconnected, which fails the gate in
**29 seconds** with *"did not return from serve within 10s"* rather than hanging — the bound is
there because [a reversal that hangs proves
nothing](docs/reference/a-reversal-can-fail-by-hanging.md).

**Not done, and named:** `shard::serve_sharded_hft` does **not** take handles, so the asymmetry
is now visible in the public API — N shards are N cells and a `ConnId` is unique only within
one, which ADR-0054 records as an open question rather than an oversight. And `benches/turn.rs`
still has no cost figure (see below); this closed on a container, not the §9 desktop.

## Before that — 2026-09-05: a journal that knows its numbering, and a gate that found what the tests could not

**Item 48 is closed.** `Journal::highest()` answers *what can be replayed*; every worked example
in this repository read it as *how far the outbound count has got*. Those differ by every
administrative message since the last application one — a `Logon`, a `Heartbeat` and a `Logout`
each spend a `34=` no journal holds bytes for — and one clean logout was enough for a real
`libquickfix` to refuse the resumed session: `MsgSeqNum too low, expecting 4 but received 3`.

The journal now answers both questions. `Journal::mark_out` is a **high-water mark** (so a kept
`put` raises it and telling it again writes nothing), `highest_out()` reads it back, `FileJournal`
writes it as a fourth record shape with the format staying v1, and `Resumed::from_journal` does
the arithmetic once so nothing derives it by hand again. The session tells the journal a number it
already has — `next_out - 1` — from every path out of `received_with`, `tick_with` and
`send_application`, plus three `_with` twins for the entry points that spend a number without
holding a journal.
[ADR-0053](docs/decisions/ADR-0053-the-journal-answers-two-questions-and-the-second-is-a-number.md).

**The interop gate is `7 / 7 + 7 / 7 + 5 / 5 + 5 / 5 + 5 / 5`**, up from `+ 3 / 3`. The
`known_gap` step — a red pinned in a script because the engine could not do this — is deleted, and
the `SIGTERM` scenario asserts continuation like the other two.

**Three things this closing found that no plan named, and each is worth more than the fix.**

**One — the item's own blocker was wrong, and it stood for four days.** Item 48 said it *"cannot
be closed before item 47"*, because the right number lives in the live session and `Observer` can
read it. That is true of one imagined fix and the fix is wrong at the root: an observer knows the
number **when somebody asks**, so a Heartbeat between the last poll and the crash leaves the
recorded number short anyway — the same defect with a smaller window. A recorded dependency
outlives the assumption it rests on.

**Two — the `SIGKILL` scenario had been green for a reason that was not the engine.** It ran at
`HeartBtInt=30`, chosen so no Heartbeat could fall inside its few-second window and the transcript
stayed readable. That is *exactly* the condition item 48 is about, removed by the fixture. A third
scenario now runs at `HeartBtInt=1` with a deliberate pause before the kill.

**Three — the inbound direction had the identical hole, on the identical line, and only the gate
could see it.** `Session::received_with` returned as soon as `judge` answered `Link::Dropped` —
which is how the counterparty's own `Logout` is judged — so `journal.mark_in`, sitting after that
return, never ran for it. A resumed session expected that number again and sent a `ResendRequest`
for a message it already had. `[measured 2026-09-05]` **nothing in this repository could see it**:
the unit tests for the inbound mark drive messages that leave the link up, and the corpus compares
bytes that were all correct. It surfaced only because fixing the outbound half let the
clean-logout scenario reach its fifth assertion and read `35=2: 1` where it wanted none. Write-up:
[a-journal-holds-messages-not-numbering](docs/reference/a-journal-holds-messages-not-numbering.md),
**`[to testing-skills]`**.

`cargo test --all` **507 → 519**, 0 failed; `--no-default-features` 514; clippy `-D warnings` and
`fmt` clean; the 59 definitions **59/59, unchanged — which is the point**, the corpus is blind to
this by construction. `benches/alloc.rs` reads `mark-out-mem 0 mark-out-file-async 0` across 29 of
29 cases, proven by a reversal that read `10000` on both.

**CI green on `3d16527`, 22 checks of 22**, runs
[`33964556891`](https://github.com/tmthang86/fixbolt/actions/runs/33964556891) and
[`33964529347`](https://github.com/tmthang86/fixbolt/actions/runs/33964529347) — §9's last box,
for the branch; the merge commit's own run is owed when it lands. Three job logs were read rather
than their conclusions. The interop job's assertion lines are byte-identical to the development
run's. **`scripts/check-no-kernel-sleep.sh` ran, both halves**: the `hft` half made `accept4`,
`recvfrom`, `sendto` and **no `poll`**, and the `standard` half tripped the check with `6 poll`,
which is what says it can still fall. `tools/w2w --path app` printed `allocs 0 (both threads, the
timed window only)` with the mark live on the path.

**Not done, and named:** `benches/turn.rs` **has no cost figure**. It ran in CI and came back
among the 45 `cases w/o a baseline` — *measured and printed but compared against nothing* — and
this closed on a container rather than the §9 desktop, so no number here would be that machine's.
Every turn now carries one more trait call and what it costs is unknown.

**And one bullet in *Not proven* was false within the hour it was written.** It said
`check-no-kernel-sleep.sh` had not run, because *this container* has no `bpftrace` — which was
mistaken for the gate not running at all. **CI is a machine too**, and it had already run it. The
bullet is struck with the log quoted.

**Item 47 closed next, the same day** — see *Start here*. Then wave B.

## Before that — 2026-09-05: item 34 closes at 3.4×, and the 40 ns was never measured

**Item 34 is closed, and the plan that owned 41 → 39 → 34 is closed with it — the only plan in
this repository that required the §9 desktop.** Step 3a asked one question: what is the ratio
ADR-0041 published, on a §9 machine, from committed benchmarks? `[measured 2026-09-05]` n = 20,
`pass 12 fail 0 unknown 1`, `benches/baselines.tsv`:

| Case | ns/op |
|---|---|
| `library, reply only` — a `Template` built, sorted and encoded per message | **804.1** |
| `encode ExecutionReport (template)` — D9's shape, built once and patched | **237.6** |
| `library, on_message` / `library, parse only` | 1028.6 / 159.6 |

**3.4×.** Not the 50× ADR-0041 published, not the 24× ADR-0044 moved it to. **The denominator of
both was a *40 ns* from an experiment that was never committed** — `git log --all` has no trace
of `crates/library/benches/attrib` — and the committed benchmark for exactly that shape read
**177.6–199.4 ns on the same VM class in the same week**. Non-negotiable 10 says a number without
its committed benchmark is somebody else's claim; it was repeated with a `[measured]` tag in six
documents for three days, and the tag was true of the numerator. Withdrawn, not replaced with a
guess. [ADR-0051](docs/decisions/ADR-0051-item-34-is-a-third-of-the-size-it-was-recorded-at.md).

What the per-message `Template` costs, by inference and labelled so: **~570 ns, 2.9%** of the
19 908 ns application round trip. **The owner chose to close item 34 as measured and accepted**
rather than spend a codec hot-path change on 2.9%; the wave-D draft `const-templates-in-dict` is
re-scoped before it becomes a plan. ADR-0041's open question 2 closes on the way: on §9 the
three `library` cases add up (+6.7%), and the VM's *whole-less-than-the-sum* was the VM.

**Merged: PR [#42](https://github.com/tmthang86/fixbolt/pull/42), merge commit `3e6d12f`. CI
green 11 jobs of 11 on the merge commit itself**, run
[`33945858850`](https://github.com/tmthang86/fixbolt/actions/runs/33945858850); on the tip that
was merged, `5f99ebe`, 22 checks of 22 across
[`33945512836`](https://github.com/tmthang86/fixbolt/actions/runs/33945512836) and
[`33945515275`](https://github.com/tmthang86/fixbolt/actions/runs/33945515275).

**What still needs the §9 desktop, asked and answered 2026-09-05 now the plan that owned it is
closed:** item **49** (price one of its four candidates — the only one both machine-bound and
ready), item **40** (blocked on a second machine and a cable, not on the configuration —
`enp9s0` is `NO-CARRIER`), the open half of item **14** (how much of a `Connection`'s 53.3 KiB
a message touches, which decides a cache wall at N≈9 or N≈128), and **wave C** as a whole —
including `benches/turn.rs` at two values of `RX`, which is what the four buffer defaults in
*Not proven* are waiting on. Items **36, 45, 47, 48** are code and need no machine.
## Before that — 2026-09-05: the dictionary pass, timed, and it explains 17%

**Item 39 is closed.** The session's dictionary pass — every field asked `is_defined_tag`,
`field_type`, `allows`, `enum_allows` and `accepts`, then `view.get(tag)` once per required
tag, each a linear scan — **had never been timed**, and now is. `[measured 2026-09-05]` §9
desktop, `pass 12 fail 0 unknown 1`, alignment pinned, **n = 21** qualifying `bench.sh` runs:

| Case | ns/op |
|---|---|
| `validate NewOrderSingle, w2w bytes` | **897.3** |
| `validate TestRequest, w2w bytes` | **218.4** |
| `validate NewOrderSingle` / `validate Heartbeat` (the `parse.rs` shapes) | 882.1 / 169.5 |
| the parse that precedes it, for scale | 120.0 |

**Seven times the parse**, and the single largest user-space row in `DESIGN.md` §8 — whose
`Session machine ~0.1 µs` had been standing in for it. §8's user-space total goes **~0.46 →
~1.36 µs** for an application message and its share of the round trip **2.9% → 8.5%**.

**And then the subtraction was finished out loud, which is the whole point.** Item 39 existed
because the application round trip is **3 898 ns** above the administrative one with ~320 ns
accounted for, and the pass was written down as the largest candidate. It is **678.9 ns,
17.4%**. With everything else the application path adds, the measured subtotal is **~1 094 ns,
28%**, and **~2 804 ns is still unattributed — new open item 49**, with four named candidates
and none of them claimed. A number big enough to feel like an explanation is not one.

**The pass could only be measured by making it public**, and two of the three ways the plan
proposed do not exist: `Fix44` is written into the three functions' bodies rather than passed
to them, and a bench target is a **separate crate**, so `pub(crate)` would not have reached it.
[ADR-0050](docs/decisions/ADR-0050-the-dictionary-pass-is-public-so-it-can-be-timed.md), with
the cost named — a public function added because a benchmark needed one.

**The new bench found a defect in two old ones on its first run — new item 50, closed the same
commit.** Its fixture assertions fired on bytes copied out of `crates/codec/benches/parse.rs`:
both messages there are malformed, and because the body-length check returns *before* the
checksum block, **`parse Heartbeat (validated)` had never summed its own 51 bytes**. 56.3 →
**60.9 ns**. Nothing in the suite could see it — the result is discarded by design, the figure
was stable to 1%, and the baseline came from the same fixture. The guard that found it is now
in `parse.rs` too: **assert the input before timing anything on it**, proven by reversal.
[a-benchmark-parsed-a-message-the-parser-rejects](docs/reference/a-benchmark-parsed-a-message-the-parser-rejects.md),
**`[to testing-skills]`**.

**And ADR-0049's hole reappeared within two hours of the ADR that named it.** The first two
`validate` baselines were taken at 902.5 and 163.6 from a binary holding two cases; adding the
two `w2w bytes` cases to the same file — code the other two never enter — moved them to 882.1
(−2.3%) and 169.5 (+3.6%), in *opposite* directions, against a within-build max/median of
1.005 and 1.017. Both stayed inside the 1.10 band, so nothing went red; the recorded numbers
are from the binary that exists. A margin fitted to run-to-run variance cannot see the variance
that arrives with the next case somebody writes.

`scripts/bench.sh --strict` exits 0, three consecutive runs after recording, read line by line:
`timing over baseline 0 · cases w/o a baseline 0 · cases under the band 0`.

**Merged: PR [#41](https://github.com/tmthang86/fixbolt/pull/41), merge commit `21104d8`. CI
green 11 jobs of 11 on the merge commit itself**, run
[`33944576168`](https://github.com/tmthang86/fixbolt/actions/runs/33944576168) — the run §9's
last box asks for, not the branch tip's. On the tip that was merged, `bfcd465`, 22 checks of 22
across runs [`33943897316`](https://github.com/tmthang86/fixbolt/actions/runs/33943897316) and
[`33943899624`](https://github.com/tmthang86/fixbolt/actions/runs/33943899624). The `bench`
job's log was read rather than its conclusion: `=== alignment read-back (ADR-0049)`, `OK: 13
bench binaries, alignment pinned and read back`, `targets measuring 13 of 13`, `invariant
failures 0`, and `cases w/o a baseline 28` **naming `fixbolt-session/validate` among them** —
which is how a runner that is not this CPU says the new target actually ran.

**One job was red first, and it was one the local sweep never runs.** `No dead internal links`:
the `validate` rustdoc linked ADR-0050 by `https://github.com/…/blob/main/…` instead of
`../../../docs/decisions/…`, which pins a reader to `main` and breaks on a fork or a rename.
`scripts/check-links.py` is not in the per-step gate list the plan runs, so a green local sweep
said nothing about a job that exists — the second time this session, and the shape §9's last
box was written for.

**Only step 3 of the owning plan is left** — item 34, and its measurement half is already paid
for: `library, parse only` 159.6, `reply only` 804.1, `on_message` 1028.6 were recorded on this
box on 2026-09-05, so the library layer finally has §9 figures instead of VM ones.

## Before that — 2026-09-05: a benchmark that measured where the compiler put it

**Item 41 is closed and `scripts/bench.sh --strict` exits 0 on the §9 desktop**, three
consecutive runs, read line by line rather than off the exit code:
`timing over baseline 0 · cases w/o a baseline 0 · cases under the band 0`. While it was red —
since 2026-09-02 — **no §6 figure this project publishes had a gate standing over it**,
including the four wire-to-wire numbers.

**CI green 11 jobs of 11 on the commit closed, `0a06000`, runs [`33940142610`](https://github.com/tmthang86/fixbolt/actions/runs/33940142610) (push) and [`33940157131`](https://github.com/tmthang86/fixbolt/actions/runs/33940157131) (pull request), 22 checks of 22** — and the `bench` job's own log was read rather than its conclusion: `=== alignment read-back (ADR-0049)`, 12 of 12 binaries PASS on a CPU that is not this one, `cases w/o a baseline 24` as it should be off the §9 box.

**Neither of the two red cases was a regression, and the two answers are different.**

`presession, read and route an identity` (+140%) is
[ADR-0026](docs/decisions/ADR-0026-a-counterparty-registry-in-the-pre-session-stage.md)
deliberately making `identity_of` scan four times instead of two. That was diagnosed on
2026-09-04 and re-run rather than trusted: replacing the two new calls with `None` reads
**83.5 · 84.1 · 84.7 · 84.3 · 84.0** against a baseline of 84.0.

`encode ExecutionReport (template)` (+16%) **is not the encoder, and it is not ADR-0044
either**. `[đo 2026-09-05]` the commit that recorded the 239.1 baseline still reads **240.0 ns
today**, so machine and toolchain are exonerated; the bisect lands on `4396d6d`, which
**touches no `crates/*/src/` file at all** and moved the figure **+11.4%** by `include!`ing 150
lines into the bench binary. Adding **inert functions the encoder never calls** walks the same
case across **236.5 – 292.4 ns**.
[ADR-0049](docs/decisions/ADR-0049-bench-builds-pin-function-alignment-and-the-flag-is-read-back.md),
[a-benchmark-that-measures-where-the-compiler-put-it](docs/reference/a-benchmark-that-measures-where-the-compiler-put-it.md),
**`[to testing-skills]`**.

### The plan was wrong twice, and both errors came from reading rather than running

Its *Những gì đã biết chắc* said `presession.rs` had changed **once** since the baseline; `git
log` says three times, and one of the three is the ADR-0026 commit — which is why step 1a went
looking for the cause in the suite's composition and found nothing. And it listed `4396d6d` as
a **negative** fact — *"the measuring instrument did not change"* — a conclusion reached by
reading the diff. Non-negotiable 10 says review of a diff catches almost nothing; here it
removed the actual cause from the list for three days.

### What changed, and what it costs

Bench builds pin function alignment, and `scripts/check-bench-alignment.sh` **reads the flag
back off the binaries** — `[đo 2026-09-05]` **23 of 23** own-crate text symbols aligned pinned
against **5 of 23** unpinned, reversal red at **2 of 11**. A `-C llvm-args=` string that a
later toolchain silently ignores is the false-green shape this repository keeps meeting; the
flag is worth what the read-back is worth. Cost, stated in the ADR: the figures are now of a
binary that is **not the one that ships**, tolerable only because every §6 row is compared
against itself over time on one machine.

**All 24 baselines were re-recorded** under that build, `n = 20`, `pass 12 fail 0 unknown 1` —
19 rewritten and 5 recorded for the first time. **No margin was narrowed** even where the new
sample allowed it.

### What this did NOT do

- **Item 39 and item 34 are untouched** — steps 2 and 3 of the same plan. The 3 898 ns the app
  round trip carries over the admin one is still unattributed.
- **The ladder rule still has the hole this found.** It fits a margin to `max/median` over N
  runs of **one build**, and layout variance is invisible to that by construction. `encode
  ExecutionReport (template)` reads 1.037 over these 20 runs and carries 1.15 anyway.

### Next

**Wave B**, unchanged. Items 39 and 41 share a plan, so the same plan is where 39 resumes.

---


## Start here — 2026-09-05, latest: an acceptor that dies, and an initiator that comes back

[reconnect-interop](docs/plans/2026-09-05-reconnect-interop.md) is closed, all six steps, and
`STATUS.md` item 38 with it. `scripts/interop.sh` now kills a real `libquickfix` acceptor and
starts it again on the same `FileStore`, twice, and reads its transcripts:
`[đo 2026-09-05]` **7 / 7 + 7 / 7 + 5 / 5 + 3 / 3**, and **CI green 11 of 11 on the merged
`main` itself, `c839854`, run
[`33933972142`](https://github.com/tmthang86/fixbolt/actions/runs/33933972142)** — the `interop`
job's own log read rather than its conclusion, every assertion line byte-identical to the
development run. The branch tip was green too (`d31db5e`, run
[`33932950725`](https://github.com/tmthang86/fixbolt/actions/runs/33932950725)), and **the
merge point is the one named**: a branch being green says nothing about the commit where it meets
`main`, and nothing is appended after a merge commit.

Until today every test of `connect_and_serve` was this repository's own reading, and
[ADR-0043](docs/decisions/ADR-0043-backoff-without-jitter-and-a-reconnect-asks-recovery-every-time.md)
said so in its own *Consequences*. It is not any more.

### It is not the eighth step item 38 asked for, and reading the code is why

Item 38 proposed adding a step to `tools/interop --role initiator`. That role drives
`Session<Initiator, 256>` directly over a blocking socket and never calls `connect_and_serve`,
so the step would have tested **the tool's** reconnect loop and left the engine's still unread.
A third role, `--role reconnect`, calls the real function.

### The two scenarios, and why they do not assert the same things

**`SIGKILL`, 5 / 5** — nobody said goodbye; it came back with nothing telling it to; **at one
past the number it last sent**, read off the transcript rather than written as a literal; the
venue's next messages were *delivered to the application* rather than gap-requested; and no
`35=2`, `141=Y` or `MsgSeqNum too low` anywhere.

**`SIGTERM`, 3 / 3** — the goodbye was answered, it redialled anyway (ADR-0043 decision 5,
confirmed by an engine that never heard of the ADR), and it was **refused by exactly one**.
That asymmetry is the result, not a shortcut: see item 48.

### What the first run found, which is bigger than the gate

**New item 48.** Every example here derives `Resumed::next_out` as `journal.highest() + 1`, and
`Journal::put` records **application messages only** — so the derivation is short by exactly the
administrative messages sent after the last application one. A clean logout is enough to do it.
`[đo 2026-09-05]` the counterparty said it in English: *`MsgSeqNum too low, expecting 4 but
received 3`*. And the number that would be right is unreachable, because `connect_and_serve`
hands out no `Observer` — **so item 38's gate found item 47 from the outside**.
[a-journal-holds-messages-not-numbering](docs/reference/a-journal-holds-messages-not-numbering.md).

### Three revisions, all written before the code moved

Sửa 1: `Application::on_message`'s `seq` is the number a **reply** would spend, not the number
that arrived — two News at `34=2` and `34=3` both logged `delivered 34=2`. Sửa 2: the item 48
finding. Sửa 3: the `SIGTERM` scenario dropped from five assertions to three, and its `back`
assertion was reading the wrong seam — it said *"nothing reached the second acceptor"* about a
Logon that had reached it twenty-two times and been **refused below the callback it watched**.
[a-refused-message-never-reaches-the-callback](docs/reference/a-refused-message-never-reaches-the-callback.md),
**`[to testing-skills]`**.

### Not done

- **A fixbolt process that restarts is still not covered by any independent opinion.** Only the
  venue dies in these scenarios. Recovery across *this* process ending is
  `crates/engine/tests/on_disk.rs`, and it is this repository reading itself.
- **Item 48 is open and nothing here fixes it** — it needs item 47's ADR first.

### Next

**Wave B**, unchanged: [settings-for-both-roles](docs/plans/2026-09-04-settings-for-both-roles.md),
then `seq-resync-789-369`, `timestamp-micros`, then TLS. Items 47 and 48 now share one ADR's
worth of question and should be answered together.

---

[reconnect-interop](docs/plans/2026-09-05-reconnect-interop.md) is closed, all six steps: a real
`libquickfix` acceptor is killed and restarted twice on the same `FileStore`, and its own
transcripts say this engine came back at the right number. **Item 38 closed, item 48 opened** —
`Resumed::next_out` cannot be derived from a journal that holds no administrative message, and
the number that would be right is unreachable through `connect_and_serve` (item 47). See the
section immediately above. Before that — **the four buffer sizes are the caller's.**
[buffers-the-caller-can-size](docs/plans/2026-09-05-buffers-the-caller-can-size.md) is closed and
merged: PR [#37](https://github.com/tmthang86/fixbolt/pull/37), merge commit `ca25571`, **CI green
11 of 11 on the merge point, run
[`33893054026`](https://github.com/tmthang86/fixbolt/actions/runs/33893054026)**. Six `*_with`
entry points take `N`, `RX`, `TX` and `APP`; no default changed and no existing call did either.
The build found a **fourth** buffer constant that had no name and failed as silence
(`Outbound::app`, 1 KiB), and four testing traps now in
[a-ceiling-has-more-than-one-floor](docs/reference/a-ceiling-has-more-than-one-floor.md).
**Wave B is next and its first plan is ready.** Before that — **WAVE A IS MERGED.** Items 42, 43 and 44 are closed and on `main`: PR [#35](https://github.com/tmthang86/fixbolt/pull/35) (acceptor-interop and resend-from-the-journal, merge commit `148256a`) and PR [#34](https://github.com/tmthang86/fixbolt/pull/34) (message-log, merge commit `0d11559`), **CI green 12 jobs of 12 on the merged `main` itself, `0d11559`, run [`33862385218`](https://github.com/tmthang86/fixbolt/actions/runs/33862385218)**. That last run is the point: each branch was green on its own, and neither said anything about the commit where they meet. **Wave B is next — item 45 holds the order**, and every one of its drafts is re-verified against the code of the day before it becomes a plan. `settings-for-both-roles` in particular now reads against an eleventh settings key and five entry points that take a message log. Before that — **an acceptor can say what it received, and a journal notices a flipped byte**. [message-log](docs/plans/2026-09-03-message-log.md) is closed, all five steps: `FileLogPath` writes both directions including frames refused before the session judged them, and the journal's on-disk format is version 1 with a CRC32 per record. **Item 44 closed.** Before that — **a resend answers with the messages now, not with gap fills**. [resend-from-the-journal](docs/plans/2026-09-03-resend-from-the-journal.md) is closed: the ring is 4096 slots and O(1), a replay goes out in batches so a large resend no longer ends the session it is answering, and the two silences are counters and events. **Item 43 closed.** Before that — **the acceptor has a second opinion, and it is 7 / 7**. [acceptor-interop](docs/plans/2026-09-03-acceptor-interop.md) is closed: a `libquickfix` `SocketInitiator` drives `fixbolt::serve` over kernel TCP, `scripts/interop.sh` runs both directions in one invocation, and the `interop` CI job is blocking on both. **Item 42 closed, item 46 opened** — this engine cannot originate an application message, found by pointing the tool's two roles at each other. Before that — **drafts for waves B, C and D written** (seven files, all *Draft*, each re-verified before it becomes a plan — item 45). Before that, 2026-09-03 — **an architecture review found three gaps phase 1 closed over, and three plans are written, none started, all *Chờ duyệt***: [acceptor-interop](docs/plans/2026-09-03-acceptor-interop.md) (item 42), [resend-from-the-journal](docs/plans/2026-09-03-resend-from-the-journal.md) (item 43), [message-log](docs/plans/2026-09-03-message-log.md) (item 44). **Item 45 is the order of everything after them.** See the section immediately below. Before that — **`the-doc-set` IS CLOSED AND MERGED**, phases A, E, B and D, PR [#32](https://github.com/tmthang86/fixbolt/pull/32), merge commit `16ad9b6`, **CI green on the branch tip closed, `86c3913`, run [`33742995012`](https://github.com/tmthang86/fixbolt/actions/runs/33742995012), 11 jobs of 11**. Ten new documents: the user track (`INTRODUCTION`, `GETTING-STARTED`, `TUTORIAL`, `CONFIGURATION`, `SESSION-BEHAVIOUR`, `CONFORMANCE`) and the operator track (`best-practices-standard`, `best-practices-hft`, `hft-playbook`), plus `crates/library/README.md` as the crate's rustdoc front page. **Open item 33 is closed for that scope**; phase C (`docs/internals/`) is **deferred to phase 2's first ADR** by Sửa 3 and carries the *engine-has-no-map* half of item 33 with it. **A new CI gate came out of it**: `docs` — `cargo doc --workspace --no-deps` under `-D rustdoc::broken_intra_doc_links`, which was **red on arrival with 13 pre-existing warnings**, six of them public documentation linking to private items. CI is 11 jobs now, not 10. Before that — **2026-09-02, PHASE 1 IS COMPLETE.** Exit criterion 6 is met and **PHASE 1 IS COMPLETE. Exit criterion 6 is met and `[2026-09-02]` **Open item 33 is new, and it is about this repository's documentation rather than its code**: `README.md` has **0 `rust` blocks**, there is **no *Getting Started* heading anywhere** in `README.md` or `docs/`, `engine::settings` accepts **10 keys** of which **`EndDay` and `MaxSkewMillis` are named in no document at all**, no page publishes the conformance results, and `engine`'s **20 modules** have no map. Planned by [the-doc-set](docs/plans/2026-09-02-the-doc-set.md); **phase A is closed**.
every one of the seven is.** `tools/w2w` ran on the owner's §9 desktop, pinned, and the wire-to-wire
figures are published: `hft` **p50 16 010 / p99 20 589 / p99.9 22 127 ns** administrative and
**19 908 / 24 657 / 26 150** application; `standard` **19 447 / 24 106 / 25 609** and
**20 920 / 25 618 / 27 092** — medians of 20 whole runs of 20 000 round trips, spread 0.3–0.6%.
**Open items 6 and 12 are closed**; 12 by [ADR-0045](docs/decisions/ADR-0045-parse-is-under-one-percent-of-the-wire-and-simd-is-declined.md),
which declines SIMD/SWAR because parse is **0.62%** of a round trip. **Three new open items came
out of the same measurement: 39, 40 and 41.** See the section immediately below. Before that —
**items 27, 35, 37 and phase 1 exit criterion 4 closed; 34 halved and 36 moved 0 → 10; criterion 6 is the only exit criterion left and it wants a machine.** Before that — **PHASE 1 EXIT CRITERION 4 IS MET**: the initiator is interop-green against a real `libquickfix`, 7 / 7, blocking in CI — [plan](docs/plans/2026-09-02-the-initiator-and-its-second-opinion.md), [ADR-0042](docs/decisions/ADR-0042-a-second-implementation-is-the-only-independent-opinion.md), PR [#30](https://github.com/tmthang86/fixbolt/pull/30), **CI green on the commit closed, `cdd6fba`, runs [`33623429649`](https://github.com/tmthang86/fixbolt/actions/runs/33623429649) and [`33623385882`](https://github.com/tmthang86/fixbolt/actions/runs/33623385882), 10 checks of 10**. Its first run found that the initiator answered a `Logon` with a `Logon` — a defect six green gates could not see. **So phase 1 now has ONE open exit criterion, 6, and it is blocked on hardware.** Before that — **`DESIGN.md` §7's build order is COMPLETE**: step 8 `library` was the last unbuilt box and it is built — [the-library-layer](docs/plans/2026-09-02-the-library-layer.md), crate `crates/library`, package `fixbolt`, [ADR-0041](docs/decisions/ADR-0041-the-library-layer-buys-an-api-with-a-template-per-message.md). PR [#29](https://github.com/tmthang86/fixbolt/pull/29), **CI green on the commit closed, `4108881`, runs [`33618980677`](https://github.com/tmthang86/fixbolt/actions/runs/33618980677) and [`33619043496`](https://github.com/tmthang86/fixbolt/actions/runs/33619043496), 18 checks of 18**. So **phase 1 now has no unbuilt component and two open exit criteria**: 4 (initiator interop against `libquickfix` in CI — a job, deliberately left for another session) and 6 (wire-to-wire on a machine matching §9 — hardware). `tools/w2w` was run here in both modes and its figures are in *Not proven* under the machine that produced them; **they do not close criterion 6 and open item 6 stays open.** New open item **34**: the library's reply path is ~50x the D9 template shape, measured, published, and not yet fixed. Before that — **`STATUS.md` item 30 is CLOSED, all six parts**, by **six plans across eight PRs today**: [operability](docs/plans/2026-09-01-operability.md) steps 1–2 (PR #18), [session-schedules](docs/plans/2026-09-02-session-schedules.md) (PR #19), [an-engine-can-resume](docs/plans/2026-09-02-an-engine-can-resume.md) (PR #20, #21), [why-a-connection-ended](docs/plans/2026-09-02-why-a-connection-ended.md) (PR #22), [three-in-the-morning](docs/plans/2026-09-02-sequence-numbers-at-three-in-the-morning.md) (PR #23), [what-the-journal-can-answer](docs/plans/2026-09-02-what-the-journal-can-answer.md) (PR #24) and [an-ordered-shutdown](docs/plans/2026-09-02-an-ordered-shutdown.md) (PR #25; **CI green on the commit closed, `ca9ae49`, runs [`33597576154`](https://github.com/tmthang86/fixbolt/actions/runs/33597576154) and [`33597580522`](https://github.com/tmthang86/fixbolt/actions/runs/33597580522), 18 checks of 18**). Items **16**, **30** and **31** are closed; item **32** was opened by the work that closed 31 and **(b) and (c) of it closed the same day** — [recovery-reaches-the-disk](docs/plans/2026-09-02-recovery-reaches-the-disk.md), PR [#26](https://github.com/tmthang86/fixbolt/pull/26), **CI green on the commit closed, `88f4473`, runs [`33600110468`](https://github.com/tmthang86/fixbolt/actions/runs/33600110468) and [`33600141705`](https://github.com/tmthang86/fixbolt/actions/runs/33600141705)**. ADRs **0032–0040**. `[2026-09-02]` **`PRD.md`'s last open line under `many counterparties` is closed too** — [a-registry-from-a-file](docs/plans/2026-09-02-a-registry-from-a-file.md), PR [#27](https://github.com/tmthang86/fixbolt/pull/27), **CI green on `826b672`, runs [`33602482251`](https://github.com/tmthang86/fixbolt/actions/runs/33602482251) and [`33602486701`](https://github.com/tmthang86/fixbolt/actions/runs/33602486701)** — so a counterparty is an edit to a file rather than a rebuild. **Everything reachable from a Mac is now done; what is left needs Linux**: items 6, 11, 12, 14, 21, 22, 24 and **32 (a)**. Before that: four plans closed 2026-09-01 (`pre-session-routing`, `what-mitigations-cost`, `release-profile`), and a doc-sync pass found **eight false bullets in this file's own *Not proven* section** plus four stale paragraphs in `DESIGN.md` and `GUIDE.md`. That pass is open items **26** and **27**. Before that: `ktls-spike` closed 2026-08-31, open item 10 answered. Before that: Re-verified on Linux the same day — see the wire-gate entry under **Proven** and open item 17. Later that day the whole suite ran for the first time on **the owner's own Linux desktop** (AMD Ryzen 7 3700X, Linux 7.0.0-30), which also **unblocked open item 10** and exposed two defects in the scripts that were supposed to be telling us so.

## Start here — 2026-09-05, later: an acceptor that can speak first

**A desk can run this acceptor now, and before today it could not.**
[an-engine-that-can-speak-first](docs/plans/2026-09-05-an-engine-that-can-speak-first.md) is
**Xong**, all five steps; [ADR-0048](docs/decisions/ADR-0048-an-engine-that-can-speak-first-has-two-doors.md),
`DESIGN.md` **D15**. `cargo test --all` 495 → **506**.
**MERGED**, PR [#38](https://github.com/tmthang86/fixbolt/pull/38), merge commit `52a2895`.
**CI green 11 jobs of 11 on the merged `main` itself, `52a2895`, run
[`33900849768`](https://github.com/tmthang86/fixbolt/actions/runs/33900849768)** — which is the
run that matters, because a branch being green says nothing about the commit where it meets
`main`. The branch tip was also green, 22 of 22, run
[`33900339333`](https://github.com/tmthang86/fixbolt/actions/runs/33900339333).

`[2026-09-05]` **Naming the run for "the commit being closed" is not quite possible on a
branch**, and three commits here chased their own tail proving it: each one that wrote a run id
became a newer commit that the id did not cover. **The merge commit ends that**, because nothing
is appended after it — which is the answer §9's last box was always reaching for.

**The `interop` job is green, so `libquickfix` itself has now judged this.** That is the
sentence the *What is not done* list below could not carry an hour ago, and it is the one that
matters: the acceptor's second opinion covers 7 / 7 in both directions.

### What could not be done before this, and can now

Every application message this engine could send was a **reply**. So there was no
`ExecutionReport` for a fill that lands a second after the order, no quote stream, no
out-of-band `35=j`, and nothing to say to a counterparty that is connected and quiet.

**One primitive, two doors.** `Session::send_application` already existed, already rewrote
`8= 9= 34= 52= 10=` and already journalled — so **no session-machine logic changed.** What was
missing was a door to it, because its only caller was D4's `OUT_OF_BAND` block, which is
`false` for the default engine.

```rust
fn on_logon(&mut self, who: Peer<'_>, nth: u32, reply: Reply<'_, P, S>) -> Answer {
    match nth { 0 => reply.message(b"B").field(148, b"desk is up").send(), _ => reply.silent() }
}

let tx = engine.sender();           // Send + Sync + Clone, any thread
tx.send(conn_id, &bytes);           // false = nothing taken, now, at the call
```

### The gate is deliberately not one of this repository's own runners

`scripts/interop.sh`'s acceptor role had two steps, `news` and `resend`, **red by design** with
a comment in `tools/interop` saying so. Both are green and **the exemption is deleted**, so a
red there is a red again. `[measured 2026-09-05]` this repository's two roles pointed at each
other read **PASS 7/7**, from 5/7.

`[measured 2026-09-05]` **removing the door leaves `cargo test -p fixbolt-session --test score`
at 59 / 59** — the corpus saying in its own words that it cannot see this. Three new
`benches/alloc.rs` cases (`origin-idle`, `origin-busy`, `logon-first`) read **0**, and **2000 /
16** under an injected `format!`.

### Three things the plan got wrong, all recorded before the code

**Sửa 1** — door 1 cannot open without touching `crates/session`: `serve_with` wraps
`InlineDispatch` itself, and Rust has no specialization, so `Application` has to carry the
method. Three additive, inert things went in: a defaulted `on_logon` **that nothing in that
crate calls**, and three `Config` getters. **Sửa 2** — `Outbox` and `Counterparty` were both
unnecessary; `Reply::originate` and `fixbolt_session::Peer` are smaller. And the `logon-first`
allocation case measures **16 calls on one session**, not 500 sessions, because `Engine::add`
allocates and one identity may hold only one connection.

### The one that cost the most, and it was not the engine

`[measured 2026-09-05]` a `35=B` carrying only `148=` **reached the wire, was journalled, and
replayed correctly on a resend — and was refused by the counterparty**, because `FIX44.xml:294`
also requires `LinesOfTextGrp`. Two steps of one gate looked at the same two messages and
disagreed, because one matched bytes and the other counted deliveries.
[a-message-on-the-wire-is-not-a-message-delivered](docs/reference/a-message-on-the-wire-is-not-a-message-delivered.md),
marked `[to testing-skills]`.

### What is not done

- **`serve` hands out no handle** — not a `Sender`, not an `Observer`, not an `Admin`. So
  through the front door only door 1 is reachable. **This is older than item 46 and was found
  by fixing it. New open item 47**, deliberately not fixed inside this plan.
- ~~**`libquickfix` has not judged this yet on this machine.**~~ — it has. The `interop` job is
  green on the merged `main` itself, run `33900849768`, and **§9's last box is ticked.**
- **No §9 machine has seen it.** `benches/turn.rs` gains one `Option` check per turn and nobody
  has re-measured it against a baseline.

### Next

**Wave B**, unchanged: [settings-for-both-roles](docs/plans/2026-09-04-settings-for-both-roles.md),
then `seq-resync-789-369`, `timestamp-micros`, then TLS. Item 47 has no plan and needs an ADR
about the shape before it gets one.

---

## Start here — 2026-09-05: the four buffer sizes are the caller's

**A plan outside the wave order, and it earned its place by being cheap and by blocking the one
after it.** [buffers-the-caller-can-size](docs/plans/2026-09-05-buffers-the-caller-can-size.md),
[ADR-0047](docs/decisions/ADR-0047-the-four-buffer-sizes-are-the-callers-through-a-second-function.md),
PR [#37](https://github.com/tmthang86/fixbolt/pull/37), merge commit `ca25571`.
**CI green 11 jobs of 11 on the commit closed, `21d6d68`, run
[`33892489952`](https://github.com/tmthang86/fixbolt/actions/runs/33892489952) — and green again
on the merge point itself, `ca25571`, run
[`33893054026`](https://github.com/tmthang86/fixbolt/actions/runs/33893054026), which is the run
wave A's closing taught this file to name: a branch green on its own says nothing about the commit
where it meets `main`.**
`cargo test --all` 492 → **495**.

### What a deployment could not do before this, and can now

`CLAUDE.md` §6 says *"the caller picks `N` … **no hidden constant**"* and `CONFIGURATION.md` told
readers to instantiate `Engine<..., N, RX, TX>` directly. **Neither was true through the front
door.** All six serving entry points went via type aliases pinning `256, 4096, 8192`, and the
`fixbolt` crate does not re-export `Engine` — so a counterparty sending messages larger than 4 KiB
meant forking the serving loop, pre-session stage included. An alias is as much a hidden constant
as a `const` when nothing above it can say otherwise.

```rust
serve_with::<256, 16_384, 16_384, 8_192, _, _>(addr, table, app, capacity, limits, log)?;
```

Six `*_with` twins; the originals keep their exact signatures and delegate. **No existing call
changed and no file under `crates/*/tests/` was touched.**

### The fourth constant, which had no name and failed as silence

`Outbound::app` was a hard-coded `[u8; 1024]` — **the tightest ceiling in the engine.** An
acceptor could receive 4 KiB and be unable to answer with 1 KiB, and nothing said so:
`Application::on_message` returning `None` means *"nothing to say"*, which is legal, so exceeding
it is indistinguishable from an application declining to reply. `fixbolt_session::DEFAULT_APP_SCRATCH`
names the default now, and `GUIDE.md` §6 has a row whose symptom column reads *"nothing at all"*.

`RX` also sizes the pre-session buffer structurally: **both** copies of `const PRE = 4096` are
gone, each having sat under a comment promising it matched the engine — one invariant, two copies,
nothing checking either.

### Four things went wrong during the build, and all four are written down

[a-ceiling-has-more-than-one-floor](docs/reference/a-ceiling-has-more-than-one-floor.md), marked
`[to testing-skills]`:

1. **A test that spans N constants and names one measures `min` over all N.** The obvious
   observable — *"did the echo come back?"* — read false at every size above ~900 bytes **through a
   16 KiB buffer**. Believed, it would have confirmed a 4 KiB inbound ceiling by measuring a 1 KiB
   outbound one. A size sweep costs thirty seconds and finds where the answer actually flips.
2. **The third sweep found a ceiling belonging to the *test oracle*** — `TemplateBuilder::<128, 4096>`
   in `fixbolt_conformance::echo`. Check the instrument's range before believing a ceiling.
3. **A reversal has a direction, and an inequality has two.** The `PRE == RX` reversal came back
   green twice — flipped in one place, then in both, which was the right technique — because the
   invariant is `PRE ≤ RX` and both flips were toward the safe side.
4. **Two agreeing samples of a flaky measurement are not a comparison.** The allocation gate was
   reported pre-existing on one sample per branch; three runs read **1, 2, 2**. CI settled it — 0
   for all 24 cases. The gate is red only on the development machine, the mirror of item 23.

### What this did NOT do

**No default changed.** `256, 4096, 8192, 1024` are exactly what they were. Raising them needs the
§9 desktop and belongs to wave C, with the defaults-for-a-PROD-server ADR (`SLOTS`, `SLOT_LEN`,
`TX`, `RingDispatch`). See *Not proven*.

### Next

**Item 46 goes first, ahead of wave B.** `[decided 2026-09-05]`
[an-engine-that-can-speak-first](docs/plans/2026-09-05-an-engine-that-can-speak-first.md) is
*Chờ duyệt*. An acceptor that can only answer is a **product** gap; the three remaining wave B
plans are knobs. And the order matters in one concrete way rather than by feeling:
`settings-for-both-roles` carries a `35=j` helper on `Reply`, and an out-of-band `35=j` is one of
the four things item 46 says this engine cannot send — build the helper first and it arrives with
no door to leave by. Item 45 is amended in the same commit.

**Then wave B, otherwise unchanged.**
[settings-for-both-roles](docs/plans/2026-09-04-settings-for-both-roles.md) is *Chờ duyệt*: it was
re-verified against the code of the day, six of its stated facts were stale, and it now reads
correctly against this change too.

**Wave C grew one step.** `[added 2026-09-05]`
[the-second-linux-desk](docs/plans/2026-09-04-the-second-linux-desk.md) step 7 is a head-to-head
against `matthart1983/nanofix` on the §9 desktop: same harness, same machine, same §9 rows,
against `standard` because that engine is thread-per-connection with a blocking `recv` and
comparing it to `hft` would be the mode-mixing non-negotiable 4 forbids. Every number this
repository publishes is its own; the only figure it carries for another Rust engine is that
project's README, correctly labelled a claim, and one visit to the box is the cheapest moment to
turn it into a measurement.

---

## Start here — 2026-09-04, later still: a resend gives back the messages

**Wave A's second plan is closed.** [resend-from-the-journal](docs/plans/2026-09-03-resend-from-the-journal.md),
six steps, opened by
[ADR-0046](docs/decisions/ADR-0046-the-ring-is-the-resend-store-and-a-replay-goes-in-batches.md).
`cargo test --all` 446 → **463**, and CI green on the closing commit:
run [`33850311124`](https://github.com/tmthang86/fixbolt/actions/runs/33850311124), 11 jobs of 11.

### What an acceptor did before this, and does now

| | before | now |
|---|---|---|
| 100 `ExecutionReport`s sent, counterparty asks `7=1 16=0` | replay 8, gap-fill 92, **no counter anywhere** | replay 100 |
| a resend larger than `TX` | `Logout 58=slow consumer`, at a counterparty whose socket was empty | answered over as many turns as it takes |
| messages older than the ring | filled, silently | filled, **counted in messages**, and `EventKind::ResendBeyondJournal { filled, oldest }` |
| a reply longer than a slot | dropped from the journal, silently | `puts_refused`, and `EventKind::JournalRefused` |
| `MemJournal::get` | scan of `N` | one index, one comparison |

### Three things it found that the plan did not name

1. **`get` returned the wrong message.** The scan took the **first** slot carrying a number, so
   after `Admin::SetNextOut` wound the outbound count back and the number was sent again, a
   replay returned the **stale** copy — correctly numbered, correctly checksummed, wrong
   content. `seq % N` addressing fixes it. Red first:
   `left: Some("the first nine")  right: Some("the second nine")`.
2. **Plain `Session::tick` would have gap-filled the remainder of a replay.** It passes
   `NoJournal`, which answers *"not held"* for everything, so continuing a batched replay
   through it emits one `SequenceReset` over the lot — and the counterparty loses messages this
   end is still holding, because the caller used the journal-less form of a timer tick. `tick`
   stalls now; `tick_with` continues. **Stalling is recoverable; filling is not.**
3. **`Engine::add` allocates ~2 MiB on the engine thread.** `benches/alloc.rs` found it —
   `events-busy` 0 → 2000, and the session bench's `deliver`, `resend` and `originate` 0 →
   10 000 each. Not fixed by deleting the call from the window: the engine bench pre-builds
   journals and calls `add_with_journal`, so an allocation appearing in accept for any *other*
   reason is still caught. `GUIDE.md` §6 and `best-practices-hft.md` §6 carry the consequence.

### The reversal that matters

```
with_resend_batch(10_000)            # emit the whole range in one call, as before
→ the session ended on turn 1 while answering a resend
→ and --test score stays 4 passed, --test wire 2 passed
```

**The corpus cannot see this defect and never could**: no acceptance definition asks for more
than three messages. `crates/engine/tests/backpressure.rs` is what can.

### The green run, by id

Run [`33850311124`](https://github.com/tmthang86/fixbolt/actions/runs/33850311124), commit `75d5ec6`,
**11 jobs of 11** on `ubuntu-latest` — §9's last box, and the reason it exists is that a plan
once closed on a laptop's word while CI was red on the same commit.

### Next

Wave A is closed. [message-log](docs/plans/2026-09-03-message-log.md) (item 44) is done, all
five steps `[2026-09-04]`. **Wave B is next** — item 45 holds the order: `settings-for-both-roles`,
`seq-resync-789-369`, `timestamp-micros`, then TLS. Every one of those drafts must be
re-verified against today's code before it becomes a plan, and `settings-for-both-roles` in
particular now has an eleventh settings key and five entry points that take a log to read
against.

---

## Start here — 2026-09-04, later: the acceptor was judged by somebody else, and passed

**Wave A's first plan is closed.** [acceptor-interop](docs/plans/2026-09-03-acceptor-interop.md),
steps 1–4 of 5. Nothing under `crates/` changed: this is a tool, a C++ counterparty, a script,
a CI job and documentation.

```
scripts/interop.sh
interop: PASS 7/7
interop-acceptor: logon        ok  35=A |49=FIXBOLT| |56=QFINI| 141=Y
interop-acceptor: order        ok  35=8 at 34=2 and 34=3, 11= matched
interop-acceptor: heartbeat    ok  35=0 without 112= within 5 s (108=2)
interop-acceptor: testrequest  ok  35=0 112=QF-TR-1
interop-acceptor: resend       ok  35=8 43=Y replayed at 34=[2, 3], wanted [2, 3]
interop-acceptor: gapfill      ok  35=2 7=7 16=0 in: yes, then 35=0 112=QF-TR-3: yes
interop-acceptor: logout       ok  35=5
interop-acceptor: PASS 7/7
interop: 7 / 7 + 7 / 7 against libquickfix @ 386ce46e...
```

### The plan was revised twice, both times before the code moved

**Neither revision was a detail.** `CLAUDE.md` §1's rule is that a plan found wrong mid-build
stops the build; both of these did.

1. **The self-check could not reach 7 / 7 and the reason is item 46.** Step 1 asked for a
   fixbolt-with-fixbolt run scoring 7 / 7. Two of those steps need the counterparty to send two
   unprompted `35=B`, and **this engine has no way to originate an application message** —
   `Handler::on_message` answers, `Admin::Command` moves sequence numbers, nothing else writes.
   Step 1 now expects those two steps red and says why.
2. **The `gapfill` step asked a question the counterparty was entitled to erase.** The first
   run of `initiator.cpp` read `FAIL 6/7` on behaviour that was **correct on both sides**:
   QuickFIX's own gap fill covered the sequence number of the `TestRequest` that opened the
   gap, so the answer this step waited for could never come. Survival is now proven by a fresh
   `TestRequest` sent after the fill.

### What it found, and none of it is a defect in the session layer

| Finding | Where |
|---|---|
| This engine cannot originate an application message | item **46**, [an-acceptor-that-can-only-answer](docs/reference/an-acceptor-that-can-only-answer.md) |
| A gap fill can swallow the question a test is waiting on | [a-gap-fill-can-swallow-the-question](docs/reference/a-gap-fill-can-swallow-the-question.md) |
| `interop` printed `PASS 1/1` over a scenario in which one step of seven ran; and its logon step compared a hard-coded `49=QFACC` while the session was built from `--target` | [a-green-fraction-over-a-scenario-that-never-ran](docs/reference/a-green-fraction-over-a-scenario-that-never-ran.md) |

**The acceptor itself was right on every one of the seven steps, first time.** Including the two
that are easy to be quietly wrong about: it echoed `141=Y`, and it replayed **the two sequence
numbers asked for** rather than gap-filling over them.

### The three reversals, and which one mattered

| Reversal | Result |
|---|---|
| Drop the `11=` echo from the acceptor's handler | `order FAIL`, `resend FAIL` behind it as the plan predicted — and the **initiator direction stayed 7 / 7**, so the reversal was scoped to what it aimed at |
| Invert the resend range | `resend FAIL  replayed at 34=[], wanted [2, 3]`. **This is the reversal that was a no-op in the other direction** ([a-resend-answer-has-two-legal-shapes](docs/reference/a-resend-answer-has-two-legal-shapes.md)); it bites here because the step names the numbers |
| Rename one grep'd step name in the script | the binary printed `interop-acceptor: PASS 7/7` and the script still failed with `MISSING OR FAILED STEP`. The per-step grep is load-bearing |

### Next

Wave A step 2: [resend-from-the-journal](docs/plans/2026-09-03-resend-from-the-journal.md)
(item 43), still *Chờ duyệt*, with ADR-0046 as its step 0.

---

## Start here — 2026-09-03, later: three gaps phase 1 closed over, three plans written

**No code changed, nothing was measured, and every number below is one this repository already
had.** What changed is that the design was read end to end against what the code does, against
the field, and against what an operator asks on the first day — and three things were found that
`STATUS.md`'s *Not proven* section did not name, which is exactly the rot §4's last row exists for.

### The three findings, each with the line that proves it

1. **The acceptor — the headline product — has never been driven by another implementation.**
   `scripts/interop.sh` drives this engine's *initiator* into a `libquickfix` acceptor
   (`tools/interop/src/main.rs:1`, `scripts/interop.sh:2`). Nothing drives a `libquickfix`
   initiator into this engine's acceptor. The 59 `.def` files are the whole of the acceptor's
   evidence, and they are read by this repository's own runner. ADR-0042's sentence — *a second
   implementation is the only independent opinion* — currently applies to half the engine, and
   the half it applies to is the one that is not the differentiator. **Item 42.**
2. **A resend gap-fills every application message older than the journal ring, and nothing
   counts it.** `Store = MemJournal<8, 512>` (`crates/engine/src/journal.rs:40`); `FileJournal`
   answers `get` from its ring only (`journal.rs:440–470`); the resend loop fills over whatever
   `get` cannot find (`crates/session/src/lib.rs:2097–2130`). An acceptor that sent 100
   `ExecutionReport`s and is asked `7=1 16=0` replays 8 and fills 92 — legal on the wire, 92
   fills lost to the counterparty, zero on every counter. Beside it: a resend larger than `TX`
   (8 KiB) trips D10's `Disconnect` and ends the session with `58=slow consumer`
   (`conn.rs:581`), and `MemJournal::get` is a linear scan (`journal.rs:112`). **Item 43.**
3. **There is no message log.** The journal keeps outbound application messages for resend;
   inbound is `mark_in(seq)`, one number; refused frames are gone the moment they are refused;
   `tracing` appears nowhere under `crates/*/src`. *"What did we receive at 10:32:07?"* has no
   answer. **Item 44.**

Three smaller things were confirmed on the way and are folded into the plans rather than
itemised: `benches/alloc.rs` has **no case** for `FileJournal` (the `Async` push into a 1 MiB
`AtomicU8` ring is on the engine thread and uncounted); `tools/w2w` measures back-to-back
round trips only, so every published figure is the warm-cache best case; `mlockall` is in §9
and in no line of code.

### What the review did NOT find

The approach is right for the positioning it states. Nothing below is reversed: the pure
session, the corpus as unit tests, two modes with a machine check each, per-machine baselines,
inline dispatch as the default, `MessageView` at 24 bytes, kernel bypass deferred. The
landscape re-read (`docs/reference/prior-art.md`, 2026-09-03 section) still shows no pure-Rust
acceptor with a production track record.

### The order decided, and why it is this order

**Fix before extend.** Phase 2 changes `MessageView` and `Dictionary`; every session or journal
defect found after that is fixed twice. And *clear all open items first* is not an option: items
32 (a), 40 and 41 wait on the §9 desktop, 14 is research. So:

| Wave | Plans, in order | Why here |
|---|---|---|
| **A — phase 1 honest** (≈1 week) | [acceptor-interop](docs/plans/2026-09-03-acceptor-interop.md) → [resend-from-the-journal](docs/plans/2026-09-03-resend-from-the-journal.md) → [message-log](docs/plans/2026-09-03-message-log.md) | interop first because it is cheapest and may change the other two, as the initiator gate did on its first run; resend before log because both touch `journal.rs` |
| **B — what the first user trips on** (≈1 week) | `settings-for-both-roles` (initiator keys, `ResetOn*`, `LogonTimeout`/`LogoutTimeout`, per-session validation knobs, `MaxMessageSize`, a credentials hook that sees the `Logon`, a `35=j` helper) → `seq-resync-789-369` → `timestamp-micros` | not plans yet: written when wave A closes, so they inherit what it finds |
| **C — one visit to the §9 desktop** (2 days at the box) | `the-second-linux-desk`: NIC to NIC (40), `SO_BUSY_POLL`/`SO_PREFER_BUSY_POLL`, `w2w --interval` for cold-cache figures, `mlockall`, a `FileJournal` and `FileLog` alloc case, item 39, item 41 | everything that needs the machine, batched |
| **D — debts phase 2 would double** (≈1 week) | `const-templates-in-dict` (item 34, and it shapes the encoding ADR) → `deterministic-simulation` (fault-injecting harness + a session fuzz target) | testing infrastructure pays twice once there are two encodings |
| **Phase 2** | one opening ADR: the encoding trait, a dictionary chosen at `Logon` by the registry, and `docs/internals` (phase C of the doc set) | — |

TLS has a plan slot after wave B (ADR-0018 unblocked it; 3–5 days). `io_uring` and kernel bypass
are **not** planned: the first needs the NIC-to-NIC number to be worth an ADR, the second is
item 14 unchanged. The mirrored corpus (36) stays open and blocks nothing. `[2026-09-04]` Drafts for B, C and D
exist at the owner's request; each is re-verified and revised when its wave is reached, and
none is approved — item 45 carries the rule and the links.

## Start here — 2026-09-02, this session: **phase 1 is complete**

**Exit criterion 6 was the last one open and it is met.** It closed the way it was always going
to: not by a decision and not by new engine code, but by taking `tools/w2w` to a machine that
matches `DESIGN.md` §9 and reading what it said. PR [#31](https://github.com/tmthang86/fixbolt/pull/31),
**CI green on the commit closed, `31623fe`, runs [`33644573232`](https://github.com/tmthang86/fixbolt/actions/runs/33644573232) and [`33644576288`](https://github.com/tmthang86/fixbolt/actions/runs/33644576288), 20 checks of 20** — §9's last box,
which exists because a laptop says the gates pass *for you* and only CI says they pass *for the
commit*. Plan:
[w2w-and-linux-numbers](docs/plans/2026-08-30-w2w-and-linux-numbers.md), **half B, steps 5, 8
and 9 — the plan is now closed in full**.

### The machine, and what it took to be entitled to a number

AMD Ryzen 7 3700X, Linux 7.0.0-30-generic, **bare metal**, `scripts/check-machine.sh`
**`pass 12  fail 0  unknown 1`** — the unknown is NIC IRQ affinity and this is loopback, where
there is no NIC to steer. Engine thread pinned to isolated `cpu6`, client to `cpu7`.

**`tools/w2w` could not have satisfied §9 before today and nobody had noticed.** Two gaps, both
in the instrument rather than in the engine:

- **It pinned no threads.** §9 asks for pinned threads and `w2w`'s own printed disclaimer named
  them; the binary had no way to pin. A run of it on a perfectly tuned box was still not a §9
  run. It now pins through `fixbolt_engine::affinity` (ADR-0015 — pinned from inside the thread,
  read back off the scheduler), and **refuses a core `isolcpus` does not name** unless told
  `--allow-unisolated`. `[measured 2026-09-02]` pinning to a non-isolated `cpu2` succeeded and
  printed `engine-core: cpu2`, which reads as a §9 run and is not one.
- **It printed p50, p99 and `max`, but not p99.9** — the one percentile criterion 6 names.

And a third gap that was in the *plan*, recorded there since 2026-08-30: `w2w` measured only an
administrative round trip. `--path app` now drives **`NewOrderSingle` → `ExecutionReport`**
through an application holding a template built once, which is the path `DESIGN.md` §8's table
is actually about.

### The four findings

1. **The design owns 2.9% of the round trip, and for the first time both halves of that
   division are measured.** §8's user-space rows total ~0.46 µs; the `hft` admin round trip is
   16.0 µs. That sentence has been in §8 since it was written, over a *literature* denominator,
   and the measurement agrees with it. It is the only figure in `measured-costs.md` that
   confirms rather than corrects a borrowed number.
2. **`hft` is worth 3 437 ns — 17.7% — against `standard` on the identical path**, for a whole
   core burned permanently per polling thread. That also prices §8's wakeup row at ~3.9 µs, and
   the literature said 2–5.
3. **Pinning to an isolated core buys nothing at p50 and 11× at p99.9.** One variable, both arms
   inside one CCD: p50 **19 968 against 19 407** — the isolated core is 2.9% *slower* — and
   p99.9 **26 300 against 266 887**, or **293 749** unpinned. §9's `isolcpus` row said the
   benefit was **unmeasured**; what could not see it was the instrument. A 500 ns
   micro-benchmark has no readable p99.9 and the excursion is 250 µs long. **The opposite
   verdict to `nohz_full`**, which is worse at p50, p99 and p99.9 and wins only from p99.99.
4. **`DESIGN.md` §8's parse row does not describe what the engine does to an inbound message**,
   and this was found by reading the code the wire figure pointed at. The app round trip is
   3 898 ns above the admin one; the committed benchmarks account for **~320 ns** of that. The
   largest named candidate is the session's **dictionary validation pass** — every field asked
   `is_defined_tag`, `field_type`, `allows`, `enum_allows`, plus `view.get` once per required
   tag — and **no benchmark measures it**, because `benches/parse.rs` parses with `NoDict`. The
   gap is recorded as a gap, not reasoned into a cause: **new open item 39**.

### What is red, and it is not this change

**`scripts/bench.sh --strict` fails on this box**, and it was failing before this session
touched anything — nothing here changed a library crate. It is **new open item 41** and the
figures are under *Not proven*.

### Three new open items, and phase 1 did not ask for any of them

**39** the dictionary pass nobody has timed · **40** `DESIGN.md` §6's *NIC to NIC* row, which
loopback does not close · **41** `bench.sh --strict` red on the §9 desktop.

## Start here — 2026-09-02, last session: four items closed on a VM, two halved

**Items 27, 35, 37 and phase 1 exit criterion 4 are closed; 34 and 36 moved a long way without
closing. Criterion 6 is the only exit criterion left and it is blocked on hardware** — it needs a machine matching `DESIGN.md` §9, and this box is a shared
4-vCPU VM under a hypervisor.

| Item | What closed it |
|---|---|
| **exit criterion 4** | `scripts/interop.sh` — this engine's initiator against a real `libquickfix`, **7 / 7**, blocking in CI. [ADR-0042](docs/decisions/ADR-0042-a-second-implementation-is-the-only-independent-opinion.md) |
| **27** — the *Not proven* section rots | a **row in `CLAUDE.md` §4's sync table** pointing at it, rather than another audit. The audit found an eleventh false bullet on the way |
| **35** — an initiator that lost its connection was on its own | `engine::reconnect::Policy` + `connect_and_serve`. [ADR-0043](docs/decisions/ADR-0043-backoff-without-jitter-and-a-reconnect-asks-recovery-every-time.md) |
| **37** — a shard test red here, green in CI | **not the VM.** It had been failing since 09-01 and *every machine that ran it skipped it* |

**Two more moved a long way without closing**, and both say so in their own rows:

| Item | From → to | What is left |
|---|---|---|
| **34** — the library costs ~50× the fast path | **~50× → ~24×.** `TemplateBuilder` stopped moving an `S`-byte struct once per field; reply 1 549 → **766 ns**, `on_message` 1 594 → **956 ns**, with `parse only` unmoved as the control. [ADR-0044](docs/decisions/ADR-0044-a-builder-that-is-not-moved-per-field.md) | the `Template` is still **materialised per message** — the half ADR-0041 actually named. 766 is the number to beat |
| **36** — the mirrored gate | **0 → 2 → 10 / 50**, and the second jump was **two real defects it found** rather than harness work | ADR-0006's ceiling of 45 is now in doubt and nobody has measured the real one |

**Three new open items, all opened because the work found them rather than because a plan
predicted them**: **36** (the mirrored gate at 2 / 50 against a ceiling of 45), **38** (the
reconnect loop has no second opinion — no corpus covers reconnect, so every test of it is this
project's own reading), and the jitter debt named in ADR-0043.

### The two defects the mirrored gate found the day it became able to fall

Both are the **same family as the `Logon` echo** — an asymmetry an acceptor corpus cannot show,
because an acceptor is always the responder:

- a session that said goodbye **first** answered the acknowledgement with a *third* `Logout`;
- `begin_logout(b"")` wrote an empty `58=`.

Nothing could see either: the 59 definitions never have the acceptor start a logout, the unit
test passed an `emit` of `|_| {}` and counted nothing, and `scripts/interop.sh` stops reading
once it has seen the counterparty's `35=5`.

### What the day's findings had in common

Six of them, and **five were about this repository's own checks rather than about FIX**:

| | |
|---|---|
| An initiator answered a `Logon` with a `Logon` — green in six gates | [a-role-can-be-wrong-in-a-direction-no-gate-runs](docs/reference/a-role-can-be-wrong-in-a-direction-no-gate-runs.md) |
| A control run that was a no-op, because the protocol had a second legal answer | [a-resend-answer-has-two-legal-shapes](docs/reference/a-resend-answer-has-two-legal-shapes.md) |
| A corpus of expected outputs cannot be replayed as inputs — why the mirrored gate was stuck at zero | [expected-output-is-not-valid-input](docs/reference/expected-output-is-not-valid-input.md) |
| A test that skipped itself on every machine that ran it, and reported `ok` | [a-test-that-skipped-itself-on-every-machine-that-ran-it](docs/reference/a-test-that-skipped-itself-on-every-machine-that-ran-it.md) |
| A reversal is only a test if the input is one the two versions disagree about — **three times, unrelated code, a different mechanism each time** | [a-reversal-needs-an-input-where-the-answers-differ](docs/reference/a-reversal-needs-an-input-where-the-answers-differ.md) |

All five carry `[to testing-skills]`. And the rule *commit before running a reversal*, written
down on 2026-09-02, **was broken on 2026-09-02** by the same hand — `git checkout` destroyed a
whole uncommitted fix, the third instance in this repository. The write-up says why the rule is
weak: the reversal loop and the undo share a command.

## Start here — 2026-09-02, latest: **phase 1 exit criterion 4 is met**

**Only criterion 6 is left, and it is blocked on hardware, not on a decision.**

`scripts/interop.sh` builds `libquickfix` from source at the same commit
`fetch-quickfix-assets.sh` pins, runs it as an acceptor, and drives this engine's initiator
through seven steps — **7 / 7**, blocking in CI. Plan
[the-initiator-and-its-second-opinion](docs/plans/2026-09-02-the-initiator-and-its-second-opinion.md),
[ADR-0042](docs/decisions/ADR-0042-a-second-implementation-is-the-only-independent-opinion.md),
PR [#30](https://github.com/tmthang86/fixbolt/pull/30).

**CI green on the commit, named by id**: `cdd6fba`, runs
[`33623429649`](https://github.com/tmthang86/fixbolt/actions/runs/33623429649) and
[`33623385882`](https://github.com/tmthang86/fixbolt/actions/runs/33623385882), **10 checks of
10**, and the `interop` job's own log read back rather than its tick:

```
interop: logon        ok    |8=FIX.4.4|9=67|35=A|34=1|49=QFACC|…|56=FIXBOLT|98=0|108=30|
interop: news         ok    2 application messages delivered
interop: heartbeat    ok    unprompted 35=0, session still answering
interop: testrequest  ok    35=0 back with 112=INTEROP-1
interop: resend       ok    35=B with 43=Y replayed at 34=[2, 3], wanted [2, 3]
interop: gapfill      ok    35=2 in: true, session survived: true
interop: logout       ok    35=5 out, 35=5 back
interop: PASS 7/7
```

`[measured 2026-09-02]` **and the branch tip is green too**: `1000ca8`, runs
[`33625608818`](https://github.com/tmthang86/fixbolt/actions/runs/33625608818) and
[`33625613624`](https://github.com/tmthang86/fixbolt/actions/runs/33625613624), **10 checks of
10**, `interop: PASS 7/7` read out of job
[`100232658943`](https://github.com/tmthang86/fixbolt/actions/runs/33625608818/job/100232658943).
Two commits in between were **red**, and both for the same reason, which is worth more than the
green: `crates/engine/tests/shard_wire.rs` sits behind `--features affinity`, so
`cargo test --all` never compiles it, and adding a variant to the conformance runner's `Input`
made its `match` non-exhaustive. **CI found it and no local command could** — which is exactly
what the `affinity` step's own comment in `ci.yml` says that step exists for. It earned its keep
on the first change that touched a shared type.

### The finding, and it is why this gate exists

`[measured 2026-09-02]` **on its first run, before it had ever been green**, it found that this
engine's initiator **answered a `Logon` with a `Logon`**. One line in the inbound-Logon handler,
shared by both roles, correct for the acceptor, unconditioned. `libquickfix` took the second
Logon, dropped the connection **without a word**, and five of the seven steps failed at once —
none of them the broken one.

**Six gates were green on it**: `--test score` at 59 / 59 (for an *acceptor* the reply is
correct), `--test mirror` at 0 / 50 exactly as asserted, 430 other tests, clippy, fmt, and
`benches/alloc.rs`. One of the two roles could not complete a handshake with anybody real, and
nothing here could say so.
[a-role-can-be-wrong-in-a-direction-no-gate-runs](docs/reference/a-role-can-be-wrong-in-a-direction-no-gate-runs.md),
**`[to testing-skills]`**.

### And the control run was a no-op, which was the second finding

Reversal 1 — put the unconditional reply back — went red at 2 / 7. **Reversal 2 — swap `7=` and
`16=` so this end asks for "3 through 2" — stayed green at 7 / 7.** The counterparty cannot
replay a backwards range, so it answered with a `SequenceReset` gap fill, which is legal and
carries the `43=Y` the step was reading. A legal answer to a question nobody asked passed a test
named for the question. The step now names the sequence numbers it wants back, and the same
reversal reads `FAIL 6/7`.
[a-resend-answer-has-two-legal-shapes](docs/reference/a-resend-answer-has-two-legal-shapes.md),
**`[to testing-skills]`**.

### What is in the session layer now

Three functions an operator can call, alongside the three that existed:
`send_heartbeat`, `send_test_request(id)`, `send_resend_request(from, to)`. **None takes whole
message bytes** — the session builds from its own `Template` and keeps `8`, `9`, `34`, `49`,
`52`, `56`, `10`. `benches/alloc.rs` case `ordered` reads 0, proven by injecting a `to_vec()`
(10 000).

### Where phase 1 stands

| Criterion | State |
|---|---|
| 1, 2, 3, 5, 7 | **met**, and re-run for this change: `score` still 59 / 59 |
| **4 — initiator interop against `libquickfix` in CI** | **MET 2026-09-02.** 7 / 7, blocking, run id above |
| ~~**6 — wire-to-wire on a §9 machine**~~ | **MET later the same day**, on the owner's Ryzen 3700X. This paragraph is the state *before* that; it is kept because "nothing that can be typed produces a §9 machine" is exactly right and is why the plan waited instead of closing. See the section at the top of this file |

**What criterion 4 did NOT buy**, so the tick is not over-read: one scenario against one
counterparty, not a second conformance corpus. It does not cover the engine's polling loop (the
tool drives the session directly), nor reconnect, backoff or schedules for an initiator — and
neither do the `.def` files. **New open item 35.** The mirrored corpus moved **0 → 2 / 50** the same day and, more to the
point, **can now fall**; getting it to its ceiling of 45 is **new open item 36**.

## Start here — 2026-09-02, later: the build order is finished

**`DESIGN.md` §7 step 8 is done, and it was the last box.** `crates/library`, package
**`fixbolt`** — one crate to depend on, and a `Handler` that receives a parsed message and
answers through a `Reply` that writes the seven fields an application does not own.
`examples/acceptor.rs` is the first end-to-end example; `examples/shared/order_handler.rs` is
the handler it runs; `tests/end_to_end.rs` pulls in **that same file** with `#[path]` and
drives it through a kernel socket. One file, two readers.

**Nothing in `session` or `engine` changed for it**, which was the design: `App` cabinets onto
the `fixbolt_session::Application` seam that has been there since 2026-08-28. The one engine
edit is additive and was found *by* the facade — `presession::LimitError` implemented neither
`Display` nor `std::error::Error`, so the crate's own worked example did not compile on its
first line.

### The finding, and it is the reverse of what the plan assumed

`[measured 2026-09-02, Intel Xeon @ 2.80GHz, a shared 4-vCPU VM — **NOT** a §9 machine]`

| | ns/op |
|---|---|
| Encode a `Template` built **once** — D9's shape | **40** |
| `App::on_message` — parse, build a template, encode | **2 062 – 2 131** |
| …of which the second parse | 188 – 195 |

The plan named the second parse as the cost. It is **~9%**. Building a `Template` per message
is the other 91%, and the whole convenience layer was **~50× the fast path** — `[measured
2026-09-02, later]` **~24× since [ADR-0044](docs/decisions/ADR-0044-a-builder-that-is-not-moved-per-field.md)**, which is half of it. That is still
open item **34**, [ADR-0041](docs/decisions/ADR-0041-the-library-layer-buys-an-api-with-a-template-per-message.md),
and it is written into `README.md`, `GUIDE.md` §1b and the crate's own rustdoc rather than
left for somebody to discover with a profiler. `fixbolt_session::Application` is untouched, so
the 40 ns path is still there and `GUIDE.md` §1b tells an `hft` deployment to take it.

`[measured 2026-09-02]` the plan closed on `4108881`, **CI green on the commit closed, `4108881`, runs [`33618980677`](https://github.com/tmthang86/fixbolt/actions/runs/33618980677) and [`33619043496`](https://github.com/tmthang86/fixbolt/actions/runs/33619043496), 18 checks of 18** — `CLAUDE.md` §9's last
box, named by id against the commit rather than against the branch.

**The machine was measured before its numbers were trusted:** ±3–4% over five whole runs. That
is what makes a 50:1 ratio mean something on a box that fails §9 — and it is why the ratio is
published and the absolutes are not recorded in `benches/baselines.tsv`.

### Two things that were nearly false greens

| | |
|---|---|
| **A checksum and a body length are blind to a swap.** Reversing `49`/`56` the wrong way round leaves `9=111` and `10=137` **identical**, because a swap moves the same bytes and a checksum is a sum. A test asserting "well formed" via the frame would have been green on a message addressed to itself. What caught it was a whole-message byte comparison against a hand-written literal | [a-frame-that-cannot-see-a-swap](docs/reference/a-frame-that-cannot-see-a-swap.md), `[to testing-skills]` |
| **The silence test was measuring the wrong silence until a reversal said so.** `an_order_the_desk_refuses_puts_nothing_on_the_wire` passed on the first run — and *the handler declined* and *the session never delivered the message* are the same observable. Making the desk fill a priceless order turned it red, which is what proves the message reaches the handler. **Fifth instance of this exact shape in this repository** | `two-time-rules-share-one-observable`, and the reversal is in the commit body |

### Where phase 1 stands now

**No unbuilt component, two open exit criteria.**

| Criterion | State |
|---|---|
| 1 session conformance, 2 groups, 3 dictionary, 5 allocations, 7 clean build | **met**, and re-run for this change: `score` and `wire` both still assert 59 |
| **4 — initiator interop against `libquickfix` in CI** | **not met.** A CI job, not a decision. Deliberately out of this session's scope, by the owner's choice |
| **6 — wire-to-wire on a §9 machine** | **not met, and blocked on hardware.** `tools/w2w` ran here in both modes; the figures are in *Not proven* with the machine that produced them. Open item 6 stays open |

## Start here — 2026-09-02

**Eight plans closed and merged today, across ten PRs.** `STATUS.md` item 30 is closed — all six
parts — and so are item 32 (b) and (c) and `PRD.md`'s last open line under `many counterparties`.
The engine went from *observable* to *operable*: it can be watched, it says why a connection
ended, it knows what hours it keeps, it resumes a session that outlived the process **from a
journal on disk**, its sequence numbers can be changed at 3 a.m. without stopping it, its journal
can be read by somebody who has never seen Rust, **it can be stopped without lying to the
counterparty**, and **who it serves comes out of a configuration file rather than out of a
rebuild**.

**Everything reachable from this Mac is now done.** What is left — items 6, 11, 12, 14, 21, 22,
24 and 32 (a) — needs a Linux box, and two of the four machine checks for non-negotiable 4
cannot run here at all. *They could not run* is not *they were green*.

| Plan | PR | What it closed |
|---|---|---|
| [operability](docs/plans/2026-09-01-operability.md) steps 1–2 | #18 | item 30 (b) and (f) — [ADR-0032](docs/decisions/ADR-0032-observation-is-a-snapshot-taken-on-request.md) |
| [session-schedules](docs/plans/2026-09-02-session-schedules.md) | #19 | item 33 — [ADR-0033](docs/decisions/ADR-0033-a-schedule-is-utc-arithmetic-and-the-calendar-stays-outside.md) |
| [an-engine-can-resume](docs/plans/2026-09-02-an-engine-can-resume.md) | #20, #21 | items 16 and 31 — [ADR-0034](docs/decisions/ADR-0034-recovery-is-asked-once-the-counterparty-is-known.md); **opened item 32** |
| [why-a-connection-ended](docs/plans/2026-09-02-why-a-connection-ended.md) | #22 | item 30 (d) — [ADR-0035](docs/decisions/ADR-0035-an-event-is-pushed-and-a-loss-is-counted.md) |
| [three-in-the-morning](docs/plans/2026-09-02-sequence-numbers-at-three-in-the-morning.md) | #23 | item 30 (c) — [ADR-0036](docs/decisions/ADR-0036-one-mechanism-two-capabilities.md) |
| [what-the-journal-can-answer](docs/plans/2026-09-02-what-the-journal-can-answer.md) | #24 | item 30 (e) — [ADR-0037](docs/decisions/ADR-0037-reading-a-journal-is-not-recovering-from-one.md) |
| [an-ordered-shutdown](docs/plans/2026-09-02-an-ordered-shutdown.md) | #25 | item 30 (a) — **item 30 closes** — [ADR-0038](docs/decisions/ADR-0038-an-ordered-shutdown-is-a-state-not-a-flag.md) |
| [recovery-reaches-the-disk](docs/plans/2026-09-02-recovery-reaches-the-disk.md) | #26 | item 32 (b) and (c) — [ADR-0039](docs/decisions/ADR-0039-a-fresh-journal-is-the-deployments-to-build.md) |
| [a-registry-from-a-file](docs/plans/2026-09-02-a-registry-from-a-file.md) | #27 | `PRD.md`'s config-file gap — [ADR-0040](docs/decisions/ADR-0040-a-configuration-file-refuses-what-it-does-not-understand.md) |

`[measured 2026-09-02]` the last of them closed on `826b672`, **CI green on that commit, runs
[`33602482251`](https://github.com/tmthang86/fixbolt/actions/runs/33602482251) and
[`33602486701`](https://github.com/tmthang86/fixbolt/actions/runs/33602486701).**
**Every PR today was named by id against the commit it closed**, which is `CLAUDE.md` §9's last
box and the one that had already been missed once. `[measured 2026-09-02]` it earned its keep
again on PR #26: the commit was ready to push and **was not `cargo fmt` clean**, which CI would
have caught and the local run had not, because `fmt --check` had been run before the last edit.

### The nine findings worth carrying forward, all of them about a green that was not evidence

| | |
|---|---|
| **A schedule test passed on the wrong rule.** The corpus `Logon` is stamped 12:00:00; ticking to 03:00 is nine hours of skew against a 120-second bound, so `max_skew_ms` refused it and the schedule was never consulted. The existing midday control was green **honestly** and could not disambiguate | [two-time-rules-share-one-observable](docs/reference/two-time-rules-share-one-observable.md) |
| **Item 16 closed on a mechanism nothing could reach.** `crates/engine/tests/recovery.rs` proves the journal, `Session::resume` and ADR-0017 — with **zero occurrences of `Engine` in the file**. A layer was finished and the seam above it was never asked about, by a plan whose exit criteria were all satisfiable one layer down | [ADR-0034](docs/decisions/ADR-0034-recovery-is-asked-once-the-counterparty-is-known.md) context |
| **A benchmark measured its own fixture, three times.** `events-busy` read 30 000 → 6 000 → 2 000 → 0 and **no wrong number came from the code under test**. Its own diagnostic was a false green too: a loop that skipped every iteration reported zero | [a-benchmark-measured-its-own-fixture](docs/reference/a-benchmark-measured-its-own-fixture.md) |
| **A design defect that no test could see, found by hand.** A command queue reached for its mutex on **every turn** once an `Observer` existed — a worse bargain than ADR-0032 claims — and every test passed, because the expensive implementation and the cheap one produce identical output for every input. **It is the second time that exact blind spot has appeared in this module**, and the earlier fix sits forty lines away with a comment explaining why. None of the four planned reversals would have caught it | [a-cost-claim-needs-its-own-counter](docs/reference/a-cost-claim-needs-its-own-counter.md) |
| **A reversal that failed by hanging.** Removing an ordered shutdown's deadline does not turn a test red — it makes the thing the deadline prevents happen for ever. The suite was killed at 600 s. Every reversal table written here had assumed a broken guard produces a *failing test* | [a-reversal-can-fail-by-hanging](docs/reference/a-reversal-can-fail-by-hanging.md) |
| **The same shape a third time, and this one was not about clocks.** An ordered shutdown reusing `AwaitingLogout` made every wait vacuous, because that state reports the link down *at once*: *they answered* and *they never answered* became one observable. Caught by a test asserting the **reason** rather than the outcome — which exists only because of the first two | [two-time-rules-share-one-observable](docs/reference/two-time-rules-share-one-observable.md) |
| **A reversal that had to fail at the compiler, and one that half-failed.** *"The serving loop no longer needs `J: Default`"* is a claim about what the type system permits, and **no runnable test can falsify it** — both versions behave identically for every type a test can name. Beside it, a sentinel value read by **two** decoders was reversed in only one: `5 passed; 1 failed` looked discriminating, and flipping both read `3 passed; 3 failed` | [a-reversal-that-must-not-compile](docs/reference/a-reversal-that-must-not-compile.md) |
| **A wire test could not tell which silence it was seeing.** An acceptor refuses an unconfigured identity in silence and a counterparty outside its trading hours in silence. A test named for the second passed while measuring the first, and **the near-fix missed too**: asserting on the parsed configuration says nothing about what a later step hands to the registry. `[measured 2026-09-02]` found by a reversal nobody planned | [two-time-rules-share-one-observable](docs/reference/two-time-rules-share-one-observable.md), fourth case |
| **CI caught what `--no-default-features` could not.** `serve_with_recovery` was ungated; `cargo test --all --no-default-features` passed locally at 321, because cargo unifies features across one invocation and `tools/w2w` switches them back on. `scripts/check-no-optional-deps.sh` reproduced it instantly — the second gate is **not** a nicety | [feature-flags-unify-across-a-workspace](docs/reference/feature-flags-unify-across-a-workspace.md) |

### The process rules that came out of the day

1. **A step-1 red must be red at an assertion, using only today's API.** It was written red at
   the *compiler* three separate times, and a test that does not compile says nothing about what
   the code does today — only that you have not written the new thing yet.
2. **Commit before running a reversal.** `git checkout <file>` was used twice to undo a scratch
   edit and destroyed uncommitted work in the same file both times, the second time a whole
   benchmark case. The reversal loop and the undo share a target.
3. **A zero from a counting benchmark needs a positive control inside the same window.** Without
   it, *"nothing allocated"* and *"nothing executed"* are the same output.
4. **A cost claim ships with a counter, or it is prose.** No behaviour test can see *"this costs
   nothing when nobody is using it"*. Assert the counter in a test that only idles — and assert
   in the same test that the counter is not simply stuck at zero.
5. **Name the settle point before choosing the knob.** A test that settles by maximum durability,
   or by sleeping, is a test whose passing you cannot explain — and a forty-second one is a test
   somebody eventually marks ignored.
6. **Before reusing a state, ask what it already promises.** Three of today's defects were two
   conditions with different remedies sharing one observable, and only one of the three was
   about time.

### Built on a Mac, and what that leaves to CI

Every line of today's work was written on an Apple M5. **No nanosecond number was published** —
`benches/baselines.tsv` keys on CPU model and this CPU has no row. The two Linux-only mode checks
(*the engine thread never sleeps in the kernel*, *a standard engine gives the core back*) and the
per-crate feature gate were **adjudicated by CI, not assumed**: not-runnable is not green.

**What the day did not prove is in *Not proven*, and eighteen of its entries were written or
corrected today.** Among them: the `try_lock` choice on the event path, `EVENT_CAPACITY`'s size,
the three event kinds that were planned and not built, a sharded deployment that still cannot
resume, `last_active_ms` that nothing persists, and a `Schedule` in a DST zone that is wrong for
half the year with nothing able to detect it. **Two were corrections to bullets that had drifted**
— which is the point of re-reading the section line by line rather than only appending to it.

## Start here — 2026-09-01, end of session

`[2026-09-01, later]` **A fourth plan closed: [counterparty-registry](docs/plans/2026-09-01-counterparty-registry.md),
open item 28.** This is an acceptor now, not a link — `presession::Registry`/`Table` map an
identity to its `Config`, one engine holds many counterparties, and the single-logon rule
compares identities. Two ADRs came out of it and **both are corrections of things this
repository already believed**:

| | |
|---|---|
| **[ADR-0029](docs/decisions/ADR-0029-the-pre-session-stage-enforces-four-definitions.md)** | the pre-session stage enforces **four** definitions, not ADR-0022's two. `1c_InvalidSenderCompID.def` and `1c_InvalidTargetCompID.def` moved onto it. **It was found only after the test that was supposed to find it was repaired**: `shard_wire.rs::pump` read four fields of `Progress` and the registry had added a fifth, so two connections vanished and CI was green. It destructures with no `..` now, so the compiler maintains the list |
| **[ADR-0030](docs/decisions/ADR-0030-one-engine-holds-many-counterparties.md)** | supersedes ADR-0026 decision 5, written the same day. One engine holds many counterparties; the registry chooses the `Config`, not the engine. Decision 5 had **no implementation** — an entry point builds its engine before any connection arrives and a trait yields no `Config` — and the prior-art table **in its own file** said none of QuickFIX, QuickFIX/J or Artio builds an engine per counterparty |

**Two `docs/reference/` notes, and both are about green that was not evidence:**
[silence-before-a-logon-has-many-causes](docs/reference/silence-before-a-logon-has-many-causes.md)
(a negative assertion with no control) and
[a-counter-that-must-be-remembered-is-not-a-counter](docs/reference/a-counter-that-must-be-remembered-is-not-a-counter.md)
(an enumerating assertion with no compiler behind it).

**Built entirely on a Mac, and here is what that costs.** Every test gate ran; **no nanosecond
number did**. `benches/baselines.tsv` keys on CPU model and an unknown CPU reads `NO BASELINE`,
so `presession, registry lookup of 40` has a value and it is not published. `shard_wire.rs`,
`check-no-kernel-sleep.sh` and `check-standard-gives-the-core-back.sh` are Linux-only and were
adjudicated by CI, twice reversing what the laptop believed.


`[2026-09-01]` **Three plans closed today and nothing is in flight.** In order:
[pre-session-routing](docs/plans/2026-08-31-pre-session-routing.md) (item 24 — the corpus
scores **59 through two shards**, was 57),
[what-mitigations-cost](docs/plans/2026-09-01-what-mitigations-cost.md) (item 22 — the CPU
speculation mitigations are **59–63% of every syscall this engine makes**, and the mechanism
this file had named for two days was wrong), and
[release-profile](docs/plans/2026-09-01-release-profile.md) (item 13 — **keep cargo's
default**, because a profile here reaches this workspace's benchmarks and nobody who depends on
these crates). **CI green on the head being described, `73e48c6`, run
[`33473213210`](https://github.com/tmthang86/fixbolt/actions/runs/33473213210).**

**And then a review of the design against what the desktop measured found the documents behind
the code**, which is what open items **26** and **27** now record. Nothing was wrong with the
work; what was wrong is that every closing plan updated the item it closed and none of them
re-read the list of things this project claims are *not* proven. Eight of those bullets were
false, one for three days — including *"32 of the 59 definitions still fail"* against a
59/59 that has held since 2026-08-29.

~~**The single largest hole is unchanged and is not a document**: `tools/w2w` has still never
run on the tuned desktop, so this project has **no wire-to-wire number at all**~~ — **closed
2026-09-02.** It ran, item 6 is closed, and phase 1 with it. What remains of §8's literature is
only what the **NIC** owns, which loopback cannot contain — open item 40.

`[2026-09-01, later]` **One decision was proposed and deliberately not self-accepted:**
**[ADR-0025](docs/decisions/ADR-0025-hft-has-a-hard-session-ceiling-and-the-engine-advises-rather-than-applies.md)**
`Proposed` — the engine **detects and advises, never applies**, and `hft` carries a **hard
ceiling of four sessions per engine that refuses the fifth** rather than degrading to
`standard`. The question behind it was whether the engine could configure itself from the
machine; four of the five parts of that answer were already decided, in ADR-0013, ADR-0014,
ADR-0015 and ADR-0020, and this ADR is mostly the work of saying which.

**Why four, and why it unblocks something:** an auto-tuner needed two numbers this project does
not have — the `hft`/`standard` crossover (N ≈ 4…11, half of it literature) and the L2 cache
wall (N ≈ 9…128, unmeasured). `2000 / 448.9 = 4.46`, so **four is the largest N that wins under
every reading of both**, and the argument becomes *stay where the curve's shape cannot change
the answer* rather than *tune along it*. **The remaining uncertainty runs one way only**: 448.9
is an *idle* turn, so the busy-path measurement can only lower the ceiling. That is exactly why
it is `Proposed` and not accepted — accepting it today would be accepting a number ahead of the
run that settles it.

**What it also names:** `[2026-09-01]` `serve_hft` takes **no plan, pins nothing and reads no
machine row**. It will spin on a laptop — slower than `standard`, burning a core — and nothing
says so. That is the rest of open item 21, and the ceiling plus a machine probe is what makes
refusing it possible.

`[2026-09-01, later still]` **A feature review then found three things the roadmap did not
have**, and the first is the largest gap in the project:

- **28 — this is a link, not an acceptor.** `[verified]` `Config` pins `target_comp_id` and
  every entry point takes one `Config`, so `serve`, `serve_hft` and `serve_sharded_hft` all
  serve exactly **one counterparty**. The routing machinery for the opposite is already built
  and has nowhere to send anything: `identity_of` reads `(49, 56)` and `HashRoute` spreads
  identities across shards that each reject all but one. It had been named **once**, in another
  item's *Blocks* column.
- ~~**29 — the application-message resend path is written and has never run.**~~ — **REFUTED the
  same day, by running it.** `[measured 2026-09-01]` `cargo test -p fixbolt-engine --test journal`
  → **7 passed**, and one of them replays a real `35=D` and asserts *a replay, not a gap fill*. It
  had been written from **the ninth false bullet** of this file's own *Not proven* section —
  believed **on the same day that section's rot was documented**. The implementation was checked;
  whether a test reached it was not. It is the strongest case in
  [a-known-limitations-list-rots-in-one-direction.md](docs/reference/a-known-limitations-list-rots-in-one-direction.md)
  now: **a written-down failure mode is not a checked one**, and the only thing that settled it
  was one command.
- **30 — nothing exists for running this in production.** `[verified]` `Engine`'s whole
  observable surface is `connections() -> usize`, and there is no shutdown, drain or signal
  handling anywhere in `crates/*/src`.

`[2026-09-01, and this is the last thing that happened]` **All three decisions were taken**, on
the owner's explicit instruction to consult other engines and settle them — *"tham khảo các
engine khác rồi chốt các quyết định cho tôi luôn"*. Prior art is written up in
[prior-art.md](docs/reference/prior-art.md); every figure and API name there is **someone else's
claim**, nothing was run.

| | |
|---|---|
| **[ADR-0026](docs/decisions/ADR-0026-a-counterparty-registry-in-the-pre-session-stage.md)** `Accepted` | the registry lives in **`presession`**, and it is a **trait, not a table**. `[documented]` QuickFIX, QuickFIX/J's `DynamicAcceptorSessionProvider` and Artio's `AuthenticationStrategy` all decide **at the `Logon`, in the accepting stage**, and all three do it through a provider rather than a fixed map — **ADR-0020 put this project there already, from a conformance failure rather than an operational need, which is two independent roads to one shape**. Authentication is `lookup` returning `None`, never a second hook. **Parts from Artio deliberately**: `lookup` is synchronous, because an accept path that can await a network call is a denial-of-service surface no logon deadline closes. **The survey also found a defect nobody had noticed**: `Identity` is `(49, 56)` only, so a counterparty disambiguating by `50=`/`57=` cannot be served — Artio's `SessionIdStrategy` and QuickFIX's `SessionQualifier` both exist for exactly that |
| **[ADR-0027](docs/decisions/ADR-0027-the-engine-owes-a-byte-stream-not-an-archive.md)** `Accepted` | the engine owes **a faithful, ordered, timestamped copy of both directions at a boundary, off the hot path — and nothing past it**. Retention, immutability, tamper evidence and search become **permanent non-goals** (`PRD.md` §5): `[documented]` MiFID II wants 5–7 years in a tamper-evident archive, and every one of those is a storage-system property. The journal stays what D7 made it — `[verified]` `SLOTS = 8` — and the tap **does not share its store**, because merging them is how an audit requirement lands on the hot path. A slow audit consumer gets [ADR-0011](docs/decisions/ADR-0011-a-full-ring-disconnects.md)'s policy like any other |
| **[ADR-0028](docs/decisions/ADR-0028-a-decimal-is-a-copy-value-parsed-on-demand.md)** `Accepted` | **`Decimal { value: i64, scale: u8 }`** — `Copy`, `no_std`, parsed **on demand**, scale preserved exactly as it came off the wire, **no `f64` in the public API on either path**. **This one overturned the guess that raised it**: open decision 10 suspected *decimal / price types* was mislabelled and that ADR-0003 had answered it. `[documented]` Artio ships exactly this shape as `DecimalFloat`, under the same constraints — because **what D2 forbids is the 8 224-byte owned `MessageView` that cost 5.9×, not 16 bytes of `Copy` produced when a caller asks**. The gap is real; the objection was to a design nobody proposed. Kept in the ADR rather than tidied away |

**So items 28, 29 and 30 are now work rather than questions.** The old text follows.

**None of the three is a plan yet**, and two of them wanted a decision first —
[PRD.md](docs/PRD.md) open decisions **8** (where the counterparty registry lives, which decides
whether a counterparty can ever be added without a restart) and **9** (how much this engine owes
an auditor, because an audit tap is not a message store). **Open decision 10 is different in
kind**: it asks whether *decimal / price types*, listed as a phase-1 gap since the PRD was
written, is a gap at all — ADR-0003 hands the application a borrowed view on purpose, so a typed
decimal may be the owned per-message object D2 exists to forbid. That one is a **mislabelling to
settle, not work to schedule**.

---

### The session that came before — 2026-08-31

**Five decisions signed and two plans approved on 2026-08-30, and from that day the owner
delegated plan-writing and plan approval to the agent working here.** Nothing is blocked on a
signature; everything below is blocked on work. **What the delegation did not change is
`CLAUDE.md` §10** — it removed a signature, not the evidence each unit of work owes, and the
approval gate was never the safety net here in the first place.

| | |
|---|---|
| **[ADR-0010](docs/decisions/ADR-0010-a-reconnect-is-not-a-restart.md)** `Accepted`, **implemented 2026-08-31** | a reconnect resumes, `141=Y` resets. `Session::resume(cfg, next_out, next_in)` carries numbers across a restart; a session from `Session::new` still resets. **[session-recovery](docs/plans/2026-08-30-session-recovery.md) is CLOSED** — all six steps, and [ADR-0017](docs/decisions/ADR-0017-the-inbound-count-is-persisted-after-delivery.md) came out of step 5 |
| **[ADR-0011](docs/decisions/ADR-0011-a-full-ring-disconnects.md)** `Accepted`, **implemented 2026-08-31** | a full ring disconnects, the refusal is never silent, capacity → 4 MiB. The engine sends `58=slow application` — deliberately **not** D10's `slow consumer`, because here the counterparty is faultless. **[ring-full-policy](docs/plans/2026-08-30-ring-full-policy.md) is CLOSED**, and open item 5 with it |
| **[ADR-0012](docs/decisions/ADR-0012-latency-first-and-one-session-per-polling-thread.md)** `Accepted` | latency beats density; every figure names its `N`. Decisions 1–2 re-scoped to `hft` by ADR-0013 |
| **[ADR-0013](docs/decisions/ADR-0013-two-modes-standard-and-hft.md)** `Accepted` | **two modes.** `standard` blocks and runs anywhere and **is the default**; `hft` spins, pins and burns a core. **Amended `CLAUDE.md` §2 rule 4** — it is now mode-scoped and its `standard` half **has no machine check yet** |
| **[threads-and-affinity](docs/plans/2026-08-30-threads-and-affinity.md)** approved | 6 steps, `hft`-scoped. Step 1 is the affinity API ADR — **it is ADR-0015, not 0014**: `standard-mode` wrote its ADR first and §5 forbids reusing a number. That plan's own text still says 0014 |
| **[standard-mode](docs/plans/2026-08-30-standard-mode.md)** **closed** | 8 steps, `standard`-scoped. Built the default mode and closed rule 4's unenforced half. One thing deferred to a §9 machine, deliberately |
| **[ADR-0014](docs/decisions/ADR-0014-standard-mode-blocks-on-poll.md)** `Accepted`, **implemented** | `poll(2)` through `libc` behind a default-on `standard` feature; **Windows refused at compile time**, never a silent spin; `Waiting` is given the sources and `Transport` names its own; `Park` → `Yield`, neither mode, **fails both gates**. Answers **all four** of ADR-0013's open questions. Accepted **by standing delegation** — the owner delegated plan-writing and approval on 2026-08-30, so nobody read these nine decisions on their behalf; the ADR says so in its own header |

**On a fresh clone, before anything else:** `scripts/fetch-quickfix-assets.sh`. `vendor/` is
gitignored (`CLAUDE.md` §8) and the 59 acceptance definitions live there, so without it the
gate that decides every session-layer change cannot run at all. Then, on a machine you intend
to *measure* on: `fixbolt-machine on` and read `scripts/check-machine.sh` — five of its ten
rows do not survive a reboot.

**The next session's starting points, in the order that costs least:**

`[2026-09-02]` **the list below is history; this is the live one.** Everything reachable from a
Mac is done, so every item here needs the Linux desktop (AMD Ryzen 7 3700X), tuned to
[ADR-0021](docs/decisions/ADR-0021-nohz-full-leaves-section-9.md) §9 where a
number is wanted:

| # | What | Why it needs Linux |
|---|---|---|
| **first** | Run the two machine checks for non-negotiable 4 — `scripts/check-no-kernel-sleep.sh` and `scripts/check-standard-gives-the-core-back.sh` — on the current `main` | They have not run since PR #17. Six plans have landed on the engine loop since, and *"they could not run"* has been recorded honestly each time and is still not *"they were green"* |
| 32 (a) | `serve_sharded_hft` has no `_with_recovery` variant and cannot be stopped | `shard.rs` is Linux-only and was deliberately not edited from a Mac — [ADR-0034](docs/decisions/ADR-0034-recovery-is-asked-once-the-counterparty-is-known.md) decision 3 |
| 21 | `serve_hft` pins nothing; the refusals D8 promises do not exist for it | `sched_setaffinity` |
| 22, 6, 11 | Any latency number at all | `benches/baselines.tsv` keys on the CPU model, and this Mac reads `NO BASELINE` — no nanosecond figure from it has been published, and none should be |
| 24 | Sharding and the single-logon rule, re-measured | The shard runtime |
| 12, 13, 14 | SIMD/SWAR, release profile, kernel bypass | All three are *measure before deciding*, and the measurement is Linux's |

**And one thing needs the owner, not a machine:** open item 27 wants `CLAUDE.md` §4 to gain a
row — *close a plan → re-read this file's **Not proven** section line by line*. §4 is not one of
the ten non-negotiables but the file is the owner's, so it has not been edited.

The historical list follows.

1. ~~**A plan for `standard` mode**~~ — **CLOSED and MERGED 2026-08-30** (`6d35b75`).
   [standard-mode](docs/plans/2026-08-30-standard-mode.md), 8 steps, and
   **[ADR-0014](docs/decisions/ADR-0014-standard-mode-blocks-on-poll.md)**. `standard` exists,
   **is the default** (`serve` blocks, `serve_hft` spins), and passes the 59 definitions.
   **One thing was left and it is not a slip**: the `standard` wakeup cost needs a §9 machine,
   and the plan's risk table said from the start it would keep `DESIGN.md` §8's *from the
   literature* label rather than close on a number from the wrong box.
2. ~~**The `standard` half of rule 4 has no gate**~~ — **CLOSED.**
   `scripts/check-standard-gives-the-core-back.sh`, four assertions, red halves `hft` **and**
   `yield`. `CLAUDE.md`'s machine-checked list now reads rule 4 as both halves.
3. ~~**`wait::Park` belongs to neither mode**~~ — **CLOSED.** It is `wait::Yield` now, and
   `[measured 2026-08-30]` it is **shown** to fail both gates rather than said to: 99.7% CPU,
   sleeping 0 of 20 samples.

4. ~~**Serialise misses its published 60 ns target and item 11 says why**~~ — **the "why" was
   wrong, and steps 1–4 of
   [serialise-and-the-60ns-target](docs/plans/2026-08-31-serialise-and-the-60ns-target.md)
   closed 2026-08-31** (`314d2b1`, `ebff188`). The linear slot scan is **~24% at most**, not
   the bulk. **~31 ns is spent before a single variable field is written** — 51% of the whole
   target on a message carrying nothing — plus **~7 ns per field in `put`**. The fix item 11
   proposed was then **written, measured and reverted**: predicted −36 ns, measured **+5.2 ns
   (+3.4%)** over 30 runs per arm. Kept from it: `crates/codec/tests/slot_order.rs`, the
   guard the body path never had for non-negotiable 5.

**Two things need the owner, and neither is blocked on work:**

- ~~**Is 60 ns the right target?**~~ — **ANSWERED 2026-08-31 by the owner, and built.**
  *"Hạ mục tiêu xuống mức với tới được, theo baseline từng máy"*, scoped to the whole of §6
  rather than the serialise row, with the absolute target column **removed** rather than
  lowered. That is
  **[ADR-0016](docs/decisions/ADR-0016-per-machine-baselines-replace-absolute-targets.md)**,
  and open items **11 and 20 both close** with it.
- **Branch convention for an agent session.** Work here has been going on one branch per plan
  per `CLAUDE.md` §8; a remote session is handed a single designated branch. They disagree and
  the repo should say which wins.

**What is next, and it is a choice rather than a queue.** `[2026-08-31]` four of the six plans
that were open this morning are now **closed**: `standard-mode`, `serialise-and-the-60ns-target`
(steps 5–6 done, §9 re-measurement **239.1 ns**, and ADR-0016 answers the target question),
**`session-recovery`** (all six steps) and **`ring-full-policy`** (steps 3–4, closing open item 5).

`[2026-08-31, later still]` **`threads-and-affinity` closed as well** — all six steps, CI green
9/9 on `d60c090`, run [`33394684357`](https://github.com/tmthang86/fixbolt/actions/runs/33394684357).
The engine pins its own threads and proves it, refuses a core plan the machine cannot honour
before a thread exists, runs M engines on M cores, and `benches/turn.rs` measures what a turn
actually costs — **505 ns per session on an ordinary core, 675 on the isolated one §9 asks for**.
That last number reversed a line of §9's own advice, and it is now stated as a trade rather than
assumed to be free.

**Two things did not close with it, and both were named**: item 24 (sharding breaks the
single-logon rule) and item 21, narrowed to `serve_hft`, which still pins nothing.
`[2026-09-01]` **24 is closed** by `pre-session-routing`; **21 is still open in that narrowed
form**, and `GUIDE.md` §9 now states it as a constraint on the caller rather than leaving it to
be discovered.

`[2026-08-31, later]` **`ktls-spike` closed too**, all four remaining steps, and with it **open
item 10** — the one ADR-0005 called load-bearing. `ktls-core` **can** be driven from a plain
non-blocking socket with no async runtime; the answer is *yes, with four conditions*, and
[ADR-0018](docs/decisions/ADR-0018-ktls-on-a-plain-socket-answers-adr-0005.md) records them.
A TLS plan is now unblocked and is deliberately not written yet.

~~What is left of the six: **`threads-and-affinity`**~~ — **it closed later that same day**,
all six steps, and `[2026-09-01]` the contradiction it was written to close is gone: the engine
pins its own threads, refuses a core plan the machine cannot honour before a thread exists, and
shards. `DESIGN.md` D8 said otherwise until 2026-09-01 and now does not.
**The `standard`-mode measurements and open item 6 want the same machine, and it already exists** — 11 and 13 closed on it since.
`[measured 2026-08-30]` the desktop reached **`pass 10  fail 0  unknown 1`** and
`scripts/bench.sh --strict` **ran rather than refusing**, which is recorded under **Proven**.
What those items wait on is **time at that box**, not tuning. The one standing catch:
**five of the ten rows are runtime settings that do not survive a reboot** — governor, turbo,
SMT, THP and `busy_poll` — so a measuring session begins with `fixbolt-machine on` and a
`check-machine.sh` read, never with an assumption.

**The upstream contribution is open.** `[2026-08-31]` the owner authorised it explicitly — *"PR
ngược lên testing skill"* — and it went up as **two** stacked draft pull requests rather than one,
because two pull requests were already open upstream and the obvious single PR would have clobbered
both. [PR #6](https://github.com/tmthang86/testing-skills/pull/6) carries the eight cases this plan
found — seven where the evidence broke while the code was right, and one real bug (a wake after
shutdown raising `SIGPIPE`, found by a review bot) whose `unsafe` block had a SAFETY comment that
was correct and insufficient: it proved memory safety and said nothing about the signal contract.
[PR #7](https://github.com/tmthang86/testing-skills/pull/7) carries the protocol reference.
**`[2026-08-31]` [PR #8](https://github.com/tmthang86/testing-skills/pull/8) is the third**, stacked on
#6, carrying the three cases the `per-machine-baselines` plan found — all about a **benchmark** rather
than a test: the instrument that was 80% of its own smallest reading, a baseline recorded through a
shorter path than the gate that judges it, and a measuring loop that was part of what it measured. It
adds §20, §21, §21a and **§22** and renumbers the checklist's red half to 26–33. `[2026-08-31]` **Revised after review**: the owner's note is that `testing-skills` is a *general* testing skill, so a case must read as a shape any project can recognise — this project's case names, constant names and Rust-only mechanism talk were stripped, every measured number kept. §22 was added in that pass and is the strongest of the four, because it happened **twice through two different hardware mechanisms** — a rate carried across a branch-prediction boundary and one carried across a cache boundary — which is what makes it a shape rather than an anecdote. **Opened without running
`npm run validate`** — this machine has no Node — and the PR says so and lists what was checked by
hand instead. See the
contribution entry below for why they are stacked and what was deliberately left out.

**On the delegation itself, recorded so it is not inferred later.** `[2026-08-30]` the owner
delegated plan-writing and plan approval — *"tự lên plan và tự duyệt, tự chạy"* — and then, on
this branch, merging when the tests pass. Three things stayed out of it and stay out: amending
one of `CLAUDE.md` §2's ten non-negotiables, anything that leaves this repository for a public
one, and any claim that a gate is green without the run that says so.

**Read before touching the engine:** [GUIDE.md](docs/GUIDE.md) §0 for the mode split, and
[reference/measured-costs.md](docs/reference/measured-costs.md) for why the numbers are what
they are — including four instruments that could not see what they were pointed at, and eight
refuted hypotheses about a 324 ns mode that is still unexplained.

## Where the work is

**[the-doc-set](docs/plans/2026-09-02-the-doc-set.md) — CLOSED AND MERGED 2026-09-03**, phases A, E,
B and D; **C deferred** to phase 2's first ADR (Sửa 3). Open item 33 closed for that scope. PR
[#32](https://github.com/tmthang86/fixbolt/pull/32), merge `16ad9b6`, CI green on `86c3913`, run
[`33742995012`](https://github.com/tmthang86/fixbolt/actions/runs/33742995012), 11 jobs of 11.

**Every finding this item was opened on is now false, and that is the point of writing them down as
measurements rather than as opinions.** `[measured 2026-09-02, on `ce14417`]` `README.md` had **0
`rust` blocks** and there was **no *Getting Started* heading anywhere**; `engine::settings`' `EndDay`
and `MaxSkewMillis` **appeared in no document**; there was **no `Setting / Valid values / Default`
table** and **no page publishing the conformance results**. `[measured 2026-09-03]` all five are
answered — `GETTING-STARTED.md`, `TUTORIAL.md`, `CONFIGURATION.md` (all ten keys, each row citing
`file:line`, checked in both directions), `SESSION-BEHAVIOUR.md` (each row naming the `.def` or test
that holds it) and `CONFORMANCE.md` (each number traced to **Proven** here, with command, machine and
CI run id).

**What the plan did NOT close, said plainly.** `engine`'s **20 modules still have no map** — that is
phase C, deferred with the rest of the contributor track. And **three `hft` entry points —
`serve_hft`, `serve_hft_with_recovery`, `serve_sharded_hft` — are still called by no test, no bench
and no tool**, so the mode this project makes its claim about still has no gate running through its
own front doors. That is a **code** defect, needs its own plan (Rule Zero), and the doc-set only
recorded it.

It was the first plan here whose gates were all `scripts/check-links.py`, `cargo doc` and reading —
**no benchmark, and no new figure**. It still produced one: the `docs` CI job, and the 13 broken
intra-doc links it found on arrival.

**Next, and each needs its own plan before any code (Rule Zero):**

**Seven plans were written and approved on 2026-08-30**, and the seventh — `standard-mode` — is
**closed**. All of it comes with the owner's standing permission to
revise a plan mid-flight when reality disagrees with it — each revision recorded in that plan's
delivery log. In dependency order:

1. ~~**gates-that-can-be-trusted**~~ — **closed 2026-08-30.** Items 7, 17, 18 and 19 are gone,
   CI is green, and the whole suite passes on Linux for the first time. Everything below was
   waiting on it, because every plan closes by quoting a gate.
2. ~~**[w2w-and-linux-numbers](docs/plans/2026-08-30-w2w-and-linux-numbers.md)**~~ —
   **CLOSED 2026-09-02, all nine steps.** Item 15 closed on 2026-08-30 with half A; half B ran
   on the owner's §9 desktop and closed items **6**, **11**, **13** and the decision on **12**
   ([ADR-0045](docs/decisions/ADR-0045-parse-is-under-one-percent-of-the-wire-and-simd-is-declined.md)).
   **This is the plan that closes phase 1**, and it did it without lowering the bar: it waited
   for the machine for three days rather than publishing a container's numbers.
3. ~~**[ktls-spike](docs/plans/2026-08-30-ktls-spike.md)**~~ — **CLOSED 2026-08-31**, item 10
   with it. **The answer is yes, with conditions**, and the `tokio`-shaped documentation that
   made the question look hard belongs to `ktls`, a *different* crate: `ktls-core` 0.0.5 has no
   async feature at all and every entry point is generic over `AsFd`. `[measured 2026-08-31]`
   `strace -f` over 1000 round trips on an offloaded socket, attributed by tid: `recvfrom` and
   `sendto` and **nothing else**; the gate's red arm puts `poll(2)` in the same loop and must
   trip. Four conditions came out of it, each measured —
   [ADR-0018](docs/decisions/ADR-0018-ktls-on-a-plain-socket-answers-adr-0005.md),
   [reference/ktls-on-a-plain-socket.md](docs/reference/ktls-on-a-plain-socket.md),
   `scripts/check-ktls-on-a-plain-socket.sh`, `spikes/ktls`. **No TLS code is merged and no
   latency number is published** — both were out of scope on purpose, and stayed out.
4. ~~**data-fields**~~ — **closed 2026-08-30**, items 8 and 9.
5. **[session-recovery](docs/plans/2026-08-30-session-recovery.md)** — item 16. **Steps 1–3 done
   2026-08-30**: the journal reads back. Steps 4–5 wait on
   [ADR-0010](docs/decisions/ADR-0010-a-reconnect-is-not-a-restart.md), `Proposed`.
6. **[ring-full-policy](docs/plans/2026-08-30-ring-full-policy.md)** — item 5. **Steps 1–2 done
   2026-08-30**; steps 3–4 wait on [ADR-0011](docs/decisions/ADR-0011-a-full-ring-disconnects.md),
   which is **`Accepted` 2026-08-30** and awaits implementation, not a signature.
7. ~~**[standard-mode](docs/plans/2026-08-30-standard-mode.md)**~~ — **CLOSED 2026-08-30**, 8
   steps, plus **[ADR-0014](docs/decisions/ADR-0014-standard-mode-blocks-on-poll.md)**. Built
   the mode ADR-0013 made the default and nobody had written, and closed non-negotiable 4's
   unenforced half. `serve` blocks and `serve_hft` spins; the 59 definitions pass in both modes;
   all four of ADR-0014 decision 6's latency cliffs are shut. **Left open on purpose**: the
   wakeup cost, which needs a §9 machine.

`[2026-09-02]` **`library` (§7 step 8) is built** —
[the-library-layer](docs/plans/2026-09-02-the-library-layer.md), closed the same day. **So
`DESIGN.md` §7 has no unbuilt step left.**

Still unplanned, and deliberately: **steps 3–4 of the paused initiator plan**, whose gate is
interop against `libquickfix` rather than the mirrored corpus (ADR-0004, ADR-0006) — phase 1
exit criterion 4, and the plan's own risk table says step 4 needs the owner to nod again,
because C++ and CMake in CI is a standing maintenance cost rather than a one-off. And
**open item 34**, the library's per-message template build, which needs a `codec` change.

| | |
|---|---|
| Branch | `[2026-09-03]` **`main`**, at **`16ad9b6`** — PR [#32](https://github.com/tmthang86/fixbolt/pull/32) merged `plan/the-doc-set` no-ff and the branch is deleted on both sides. **CI green on the tip that was merged, `86c3913`, run [`33742995012`](https://github.com/tmthang86/fixbolt/actions/runs/33742995012), 11 jobs of 11.** Before that: `[2026-09-01]` **`claude/linux-desktop-testing-review-pgpaw1`**, and it **is** `origin/main` — the last five plans were merged onto it through PRs [#11](https://github.com/tmthang86/fixbolt/pull/11)–[#15](https://github.com/tmthang86/fixbolt/pull/15) and the remote's default branch now points at the same commit, `73e48c6`. **CI green there, run [`33473213210`](https://github.com/tmthang86/fixbolt/actions/runs/33473213210).** A local `main` ref left over from before that will read 83 commits behind; `git fetch origin main` settles it. **This is the branch-convention question below, answered by practice rather than by decision, and it should still be written down.** Before that: **`ktls-spike-steps-2-5`** — PR [#10](https://github.com/tmthang86/fixbolt/pull/10), `ktls-spike` closed. `[measured 2026-08-31]` **CI green on the commit being closed, `1b9b356`, run [`33386125577`](https://github.com/tmthang86/fixbolt/actions/runs/33386125577), 9 / 9 jobs.** Before that: **`main`.** PR #2 (`claude/project-status-hdx7k1`) merged 2026-08-30 as **`6d35b75`**, no-ff, 12 commits — `standard-mode` closed. `[measured 2026-08-30]` **CI green on the merge commit itself, run [`33326803468`](https://github.com/tmthang86/fixbolt/actions/runs/33326803468)**, and on the merged head `ae3d78a` before it, run `33325530208`, 9 / 9 jobs. `git diff ae3d78a 6d35b75` is **empty**, so the branch's green transfers to the merge exactly rather than by assumption. Before that: PR #1 merged as `76d6989`, run `33307963879` |
| Milestone | **M3 — the engine, closed, and the corpus now passes through four different paths.** `[measured 2026-08-30]` **59 / 59** in process, and through a real socket, on the M5 **and** on Linux x86_64 — the 39/59 this row used to carry was open item 17, the harness's settle criterion rather than the engine, and it is **closed**. `[measured 2026-08-30]` **59 / 59 in `standard` mode**, `[measured 2026-09-01]` **59 / 59 through two shards**. `codec`, `dict`, `conformance` and `session` are closed behind it. `[2026-09-02]` **`DESIGN.md` §7 is complete**: step 8 `library` is built, and the corpus is untouched by it — `score` and `wire` both still assert 59. `[2026-09-02]` **§7 step 7 is complete too, and so is phase 1**: `tools/w2w` ran on the §9 desktop, pinned, and criterion 6 is met — `hft` p50 **16 010 ns** administrative, **19 908** application; `standard` **19 447** and **20 920**. Open item 6 is closed |
| Scope | **[PRD.md](docs/PRD.md)** — phase 1 = FIX 4.4 tag=value both sides; phase 2 = SBE / FAST / FIXML + FIX 5.0. **TLS has ADR-0005 (Accepted), now supplemented by [ADR-0018](docs/decisions/ADR-0018-ktls-on-a-plain-socket-answers-adr-0005.md), and still no plan — but it is no longer blocked**: open item 10 closed 2026-08-31 |
| Plan in flight | `[2026-09-05]` **None.** [what-is-left-and-what-a-message-touches](docs/plans/2026-09-05-what-is-left-and-what-a-message-touches.md) closed and merged as **`44c57ec`**, PR [#43](https://github.com/tmthang86/fixbolt/pull/43); **CI green on the merge commit itself, 11 jobs of 11, run [`33959509459`](https://github.com/tmthang86/fixbolt/actions/runs/33959509459)** — the tip that was merged, `4e3fb1c`, was 22 of 22. **The machine has been returned to desktop settings in full, and §9 is no longer one command away.** `fixbolt-machine off` + `smton` restored the runtime knobs (governor `powersave`, boost on, SMT on, 16 logical cores, THP `madvise`, `busy_poll` 0). `[2026-09-05]` the three boot parameters were then **removed from `/etc/default/grub`** as well — `isolcpus=6,7,14,15`, `rcu_nocbs=6,7,14,15` and `processor.max_cstate=1` — because `fixbolt-machine off` cannot touch them and they are not free on a desktop: four of sixteen logical CPUs were held back from the scheduler, and `max_cstate=1` left only `POLL` and `C1`, so the box never idled deeply. `update-grub` run, `/boot/grub/grub.cfg` clean, a timestamped backup of `/etc/default/grub` beside it. **They take effect at the next boot and are not in force yet.** So the §9 recipe for this box is now: put those three back on the kernel command line, reboot, then `fixbolt-machine on` + `smtoff` — **two reboots, not one command**. Items 49, 51, 40 and wave C all pay that. [what-is-left-and-what-a-message-touches](docs/plans/2026-09-05-what-is-left-and-what-a-message-touches.md) closed today — item 14's open half closed, item 49 narrowed from ~2 804 to ~2 770 ns with two candidates retired, and three new open items came out of it (51, 52, and the framing candidate that item 49 now holds alone). **Four revisions were written into that plan before or during the build and every one of them changed what got built**: the payload sizes were wrong, the struct size was a week stale, `Loopback` could not be used, and N=128 cost more machine time than it was worth. **Nothing is approved-and-not-started** from this session; two plans written in parallel by another session for items 47 and 48 are drafts awaiting review. Before that — `[2026-09-05]` **None.** [the-baselines-and-the-pass-nobody-timed](docs/plans/2026-09-02-the-baselines-and-the-pass-nobody-timed.md) closed today, all three steps — items 41, 39 and 34. **Nothing is approved-and-not-started**: the seven wave B/C/D files are drafts, and item 45 holds their order. Before that — `[2026-09-05]` the same plan, steps 1 and 2 closed, step 3 open. Before that — `[2026-09-05]` **all three plans written 2026-09-03 are closed** (acceptor-interop 09-04, resend-from-the-journal 09-04, message-log 09-04), and so are reconnect-interop, buffers-the-caller-can-size and an-engine-that-can-speak-first, all 09-05. **This cell said *None started, three Chờ duyệt* for two days after the last of them closed** — the same failure it already records one sentence later about counterparty-registry, which is why the plan table above now carries the in-flight plan as its topmost row rather than leaving it to this cell alone. Before that — **None, and nothing is approved-and-not-started either.** `[2026-09-03]` **[the-doc-set](docs/plans/2026-09-02-the-doc-set.md) closed and merged** (A, E, B, D; C deferred to phase 2) — PR [#32](https://github.com/tmthang86/fixbolt/pull/32). **[counterparty-registry](docs/plans/2026-09-01-counterparty-registry.md) is CLOSED, not pending** — this cell called it *approved and not started* for two days after item 28 had been struck; it closed 2026-09-01 with ADR-0029 and ADR-0030. **So the next unit of work starts with writing a plan, not with picking one up**, and the candidates are open items **34** (the library's reply path, machine-independent to fix), **39** (the dictionary pass has never been timed), **36 / 38** (the initiator, still paused on ADR-0006) and **phase 2's first ADR**, which is also what un-defers `docs/internals/`. Before that: `[2026-09-01]` three closed that day — `pre-session-routing`, `what-mitigations-cost`, `release-profile`; `[2026-08-31]` six before them — `threads-and-affinity`, `standard-mode`, `serialise-and-the-60ns-target`, `session-recovery`, `ring-full-policy`, `ktls-spike`; `[2026-08-30]` `gates-that-can-be-trusted` and `data-fields`. `[2026-09-02]` **`w2w-and-linux-numbers` is now closed in full** — half B ran at the desktop and closed items 6, 11, 13 and the decision on 12, which is what closed phase 1. **One plan is neither closed nor in flight and says why**: the initiator plan is **paused** on ADR-0006. **So the next unit of work starts with writing a plan, not with picking one up** |
| Last closed | **[2026-08-30-engine.md](docs/plans/2026-08-30-engine.md)** — closed 2026-08-30. **All six steps done.** `DESIGN.md` §7 step 6, taken before step 5 by decision. The gate that matters — the same 59 definitions **through a real socket** — went green at step 3 and did not move afterwards. Two ADRs came out of it: [ADR-0007](docs/decisions/ADR-0007-spsc-ring-without-unsafe.md) and [ADR-0008](docs/decisions/ADR-0008-journal-is-a-trait.md) |
| Paused | **[2026-08-29-session-initiator.md](docs/plans/2026-08-29-session-initiator.md)** — steps 1–2 done and merged 2026-08-30; steps 3–4 not started. Paused because the mirrored gate measures less than the plan assumed — see the two measurements below |
| Last closed | **[2026-08-28-session-layer.md](docs/plans/2026-08-28-session-layer.md)** — closed 2026-08-29. **All six steps done: 59 / 59.** Steps 1, 3, 4, 5 and 6b hit their prediction; step 2 missed it low (18 predicted) and step 6a missed it high (52 predicted), both for reasons written down in the plan. Eleven revisions recorded there |
| Last closed | **[2026-08-28-dict-validation.md](docs/plans/2026-08-28-dict-validation.md)** — closed 2026-08-28. Four validation tables, agreed with QuickFIX's own generated C++ on 912/912 tag numbers, 12 524/12 524 message-tag pairs and 1 708/1 708 enum values |
| Last closed | **[2026-08-28-conformance-runner.md](docs/plans/2026-08-28-conformance-runner.md)** — closed 2026-08-28. The 59 definitions run in process; a replaying fake scores 59 / 59, which is what makes the real score mean something |
| Last closed | **[2026-08-27-repeating-groups.md](docs/plans/2026-08-27-repeating-groups.md)** — closed 2026-08-28. Groups read and written, nested to depth 4; field order agreed with QuickFIX's own generated C++ on 730/730 groups |
| Last closed | **[2026-08-27-codec-dict.md](docs/plans/2026-08-27-codec-dict.md)** — closed and merged 2026-08-28. 54 tests, 0 allocations, 304M fuzz executions |
| Last closed | Design reviewed against the HFT latency budget and revised: positioning fixed to "fastest acceptor on kernel TCP", ADR-0002 default reversed (inline dispatch, ring optional), D8 busy-poll, D9 template encoder, D10 send backpressure, §8 latency budget, §9 OS checklist, wire-to-wire gate added |

## Proven — the command was run and its output read

- **rustdoc is a gate now, and it was red before it was one.** `[measured 2026-09-03]`
  `RUSTDOCFLAGS="-D rustdoc::broken_intra_doc_links" cargo doc --workspace --no-deps` failed with **13**
  warnings across `codec`, `session`, `engine` and `conformance` — **six of them public documentation
  linking to private items** (`GroupTooDeep`→`MAX_DEPTH`, `LOOSE_TAGS`→`Shape::Timestamp`,
  `DAYS_YEAR_ZERO_TO_EPOCH`→`days_from_civil`, `InboundMark`→`INBOUND_MARK`, `ActivityMark`→`ACTIVITY_MARK`,
  `wake`→`Pipe`), five redundant explicit links, and **two genuinely unresolved links that only appeared
  once the deny compiled past the first failure**. All fixed as doc comments only; `cargo test --all`
  stayed green and the corpus stayed 59 / 59. The `docs` CI job holds it, and **it is proven by
  reversal**: a `[`NoSuchItemXyz`]` link in the `codec` crate doc turned it red naming that item,
  removing it went green, and `git diff --stat` was clean afterwards. CI is **11 jobs**, not 10.
  `[to testing-skills]`: **a link checker that reads markdown cannot see a dead link inside a doc
  comment** — two gates, two file formats, and the second one had been dark since the repository began.

`[measured 2026-09-02]` **the wire, on a machine matching `DESIGN.md` §9 — phase 1 exit
criterion 6.** AMD Ryzen 7 3700X, Linux 7.0.0-30-generic, bare metal, `check-machine.sh`
`pass 12  fail 0  unknown 1`, engine thread pinned to isolated `cpu6` and the client to `cpu7`:

```
cargo build --release -p fixbolt-w2w --features affinity
scripts/w2w-baseline.sh                 # 20 runs an arm, 20 000 timed round trips a run

hft      admin   p50 16 010   p99 20 589   p99.9 22 127 ns   20/20 runs   spread 1.006
hft      app     p50 19 908   p99 24 657   p99.9 26 150 ns   20/20 runs   spread 1.005
standard admin   p50 19 447   p99 24 106   p99.9 25 609 ns   16/20 runs   spread 1.003
standard app     p50 20 920   p99 25 618   p99.9 27 092 ns   19/20 runs   spread 1.005
allocs 0 in the timed window, both threads, every run
```

The five discarded runs were discarded **by the guard, for the reason the guard exists**: Chrome
woke up on the housekeeping cores and `w2w-baseline.sh` re-reads CPU busy before every run.
`standard / admin` was then re-measured on an independent 10-run pass with **0 discarded** and
read **19 451** against 19 447 — 0.02%. **p99 ≤ 50 µs holds in all four arms**, which is §6's
only absolute in that row.

`[measured 2026-09-02]` **and the four arms were re-measured 3 runs each with the runner's
mode/path read-back active**, because the publishing batch ran before that guard existed. Every
p50 matches its published median within **0.35%**: 16 020 / 19 978 / 19 477 / 20 940 against
16 010 / 19 908 / 19 447 / 20 920. Three independent things say the same: the two modes differ
17.7% so they ran different code; `check-standard-gives-the-core-back.sh` reads `standard` at
0.31% CPU and `hft` at 99.59%; and the app path's three in-binary assertions only pass on the
app path.

**Four reversals, all run, all red on the assertion meant:** the app path's reply is checked
for the `ClOrdID` that was just sent (`(11, b"WRONG")` → red, naming both values) and for
`150=F` (`(150, b"0")` → red, *"the template did not patch its slots"*); and the binary's own
allocation counter goes from **0 to 2 000 over 2 000 messages** when one `to_vec()` is put inside
the timed loop. A zero that has never been seen to be non-zero is not a measurement. And making
`Mode::Standard::name()` return `"hft"` reddens the runner's own read-back with
`ran a mode other than 'standard'`.

`[measured 2026-09-02]` **both mode gates run on the §9 desktop for the first time**, and both
halves of each: `check-no-kernel-sleep.sh` reads `accept4 8009 · recvfrom 8008 · sendto 351` and
**zero** blocking calls on the engine thread, with `--mode standard` tripping it on `7 poll`;
`check-standard-gives-the-core-back.sh` reads `standard` at **0.31% CPU, sleeping 20/20, p50
19 557 ns**, with `hft` at **99.59%** and `yield` at **100.04%** both tripping it on the policy.

`[measured 2026-09-02]` **the initiator, against a real `libquickfix`, on GitHub Actions.**
`bash scripts/interop.sh` builds QuickFIX from source at `386ce46e…` — the same commit
`fetch-quickfix-assets.sh` pins, and the script refuses to run if the two ever disagree —
compiles `tools/interop/acceptor.cpp` against it, runs it, and drives
`Session<Initiator, 256>` over a kernel socket through **7 steps, 7 / 7**. The gate reads the
printed transcript, not the exit code: a binary that dies before printing and one that prints
seven failures both exit non-zero. Green on `ubuntu-latest` (cmake 3.28.3, g++ 13.3.0) in
job [`100225508548`](https://github.com/tmthang86/fixbolt/actions/runs/33623429649/job/100225508548)
of run [`33623429649`](https://github.com/tmthang86/fixbolt/actions/runs/33623429649), commit
`cdd6fba`, **and the job's own log was read back rather than its conclusion**. Two controls:
reversing the Logon fix gives `FAIL 2/7`; swapping the resend range gives `FAIL 6/7` — and gave
`PASS 7/7` until the step stopped accepting any `43=Y`.

`[measured 2026-09-04]` **and the other direction: a real `libquickfix` initiator, into this
engine's acceptor.** `scripts/interop.sh` compiles `tools/interop/initiator.cpp` against the
same pinned QuickFIX, stands up `fixbolt::serve` in `standard` mode behind
`tools/interop --role acceptor`, and scores **7 steps, 7 / 7**: `logon` (with `141=Y` echoed),
`order` (two `35=D` → two `35=8`, paired on `11=`), `heartbeat` (unprompted `35=0` carrying no
`112=`, on a deadline read from the `108=` on the wire rather than from a config file),
`testrequest`, `resend` (**the two original numbers replayed with `43=Y`**), `gapfill`
(`35=2 7=7 16=0`, then a **fresh** `TestRequest` answered after the fill), `logout`. The judge
is a `FIX::Log`, not `fromApp`: QuickFIX drops a `PossDup` replay of a number it has already
seen before the application sees it, and the resend step asks for exactly that.
Unlike the initiator direction, **the whole stack is under test** — the poller, the pre-session
table, the settings file, the library `Handler`. Green on macOS 15 (Apple M5) **and on `ubuntu-latest`**
(cmake 3.31.6, g++ 13.3.0): the blocking `interop` CI job runs both directions in one
invocation — job
[`100900997589`](https://github.com/tmthang86/fixbolt/actions/runs/33833427382/job/100900997589) of run
[`33833427382`](https://github.com/tmthang86/fixbolt/actions/runs/33833427382), commit `f94e36e`,
**11 jobs of 11 green, and this job's own log read back** rather than its conclusion. Three reversals, each red on its own step and each restored: dropping the `11=` echo
(`order FAIL`, and the initiator direction still `PASS 7/7`); inverting the resend range
(`resend FAIL  replayed at 34=[], wanted [2, 3]` — **the reversal that was a no-op in the other
direction**); renaming one grep'd step name, which failed the script while the binary printed
`PASS 7/7`.

`[measured 2026-09-02]` **the mirrored corpus moved off zero, and it can now fall.**
`cargo test -p fixbolt-session --test mirror` reads **2 / 50** —
`2k_CompIDDoesNotMatchProfile` and `2o_SendingTimeValueOutOfRange`, asserted by name — with
`harness originated: 0×42 1×21 2×3 4×10 5×30 app×35` — **141 drives** — asserted beside it as
**exact numbers**,
because a score a harness can raise by driving harder is not a score. Three reversals, each
discriminating: neutering `make_receivable` gives `0 / 50` and `0×1`; removing the origination
gives 1 file; and letting the **acceptor** corpus be driven makes `tests/score.rs` panic with
*"the acceptor corpus must not be driven: Heartbeat"*. The acceptor gate is unchanged at
**59 / 59** throughout.

`[measured 2026-09-02]` **three things an operator can order, and they allocate nothing.**
`cargo test -p fixbolt-session --test initiator` — 6 tests, written red at their assertions
against stubs returning `false` (4 failed, 1 passed, and the one that passed is why the silence
test now collects its three results instead of asserting them one at a time: three `assert!`s
in a row stop at the first, so a reversal removing all three guards looked identical to one
removing one). Four reversals, each discriminating: dropping the logged-on guard reads
`left: [true, true, true]`; using the session's own `112=` reddens only the test-request case;
adding a `112=` to the unprompted heartbeat reddens only the heartbeat case; a `to_vec()` on the
ordered path takes `benches/alloc.rs` from `ordered 0` to `ordered 10000`.

`[measured 2026-09-02]` **the library layer, end to end through a kernel socket.**
`cargo test -p fixbolt` — 4 `reply` cases comparing whole messages byte for byte against
hand-written literals, 3 `end_to_end` cases driving the example's own handler over TCP, and
the crate doctest. `cargo test --all` reads **425 passed, 0 failed, 79 binaries**, against
417/75 before the crate existed. `cargo bench -p fixbolt --bench alloc` reads
`handler-reply 0 handler-silent 0 unparsable 0 control-injected 1000` — the control is in the
same run and each case also asserts it took the path it names, so a zero means *did not
allocate* rather than *did not execute*. Six reversals, each discriminating: dropping `52`
(2 of 4 red), not reversing `49`/`56` (2 of 4, and 1 of 3 end-to-end), dropping the
session-owned skip (1 of 4 — which is what shows the two byte-comparison tests are not one
test twice), writing `34` as a constant (2 of 3 end-to-end), making the desk fill a priceless
order (the silence control red), and an allocation inside `Reply` (the bench reads 1000 and
exits 101). `scripts/check-no-optional-deps.sh` gained a `fixbolt:libc` case and it too was
proven by reversal — `default-features = true` on the engine dependency makes it exit 1.


**`[measured 2026-08-30]` A review bot found a real one, and it killed the process.** If the
engine is dropped while another thread still holds a `WakeHandle`, the self-pipe's read end
closes and the write end does not; `libc::write` into it raises `SIGPIPE`, whose default action
terminates. Reproduced before fixing — `signal: 13, SIGPIPE: write on a pipe with no one to
read` — and it is **invisible from any ordinary Rust test**, because the runtime sets `SIG_IGN`
before `main` and the ignored return value swallows the `EPIPE`. A library cannot assume its host
does that. Fixed by holding both ends jointly, so a writer always has a reader. The test lives in
its own binary because it changes a process-global disposition.

The `unsafe` block's SAFETY comment was **correct and insufficient**: it proved memory safety —
live pointer, right length, nothing retained — and said nothing about the signal contract.
`CLAUDE.md` §2 rule 8 asks for "a comment naming what proves it sound", and what was proven was
the wrong kind of soundness.


**`[measured 2026-08-30]` The `standard` gate agrees with itself on two different machines, with
two orders of magnitude of headroom.** Its ceilings are not the marginal kind open item 20 is
about. Same commit, same script:

| | this session's container | GitHub runner (`33324622355`) |
|---|---|---|
| `standard` | 0.00% CPU, sleeping 20/20, p50 **10 917 ns** | 0% CPU, sleeping 20/20, p50 **24 848 ns** |
| `hft` | 98.81% CPU, sleeping 0/20 | 98.83% CPU, sleeping 0/20 |
| `yield` | 99.70% CPU, sleeping 0/20 | 99.74% CPU, sleeping 0/20 |

The p50 ceiling is 1 000 000 ns and the two machines differ by 2.3× while sitting **40× and 100×
below it**; the CPU figures differ by hundredths of a percent. `DESIGN.md` §6's *timing* ceilings
swing 5–232% run to run on one box and 1.7× between two — this gate is measuring a different kind
of thing, a ratio and a count, and it is worth saying which kind before somebody assumes the
worst about both.


**`[measured 2026-08-30]` The 59 acceptance definitions pass with the engine actually blocking.**
`cargo test -p fixbolt-engine --test wire` now runs the corpus twice, once per idle strategy, and
scores **59 / 59** both ways — ADR-0013's *"two modes is two things to test, for ever"*, paid.
The blocking run takes 3.00 s of wall time and **0.25 s of user time**, which is the mode doing
what it says.

**`[measured 2026-08-30]` And a claim written into that test's own documentation was refuted by
reversal.** It said a wiring failure — the listener left out, writability never asked for, the
waker undrained — would show up as the run taking minutes instead of seconds. It does not: with
`Block` made to ignore readiness entirely the run took **3.30 s**, with the listener removed from
the poll set **3.34 s**, against a baseline of **3.28 s**. The settle criterion is 1 ms and the
blocking timeout is 5 ms, so **one block satisfies it whether it was woken by data or by its own
timeout**, and the harness cannot tell those apart. The test proves the protocol is unchanged
under blocking, which is real and worth having; the wiring is proven by `tests/standard.rs`
reading the interest list directly and by the p50 assertion in the `standard` gate. The comment
now says so.


**`[measured 2026-08-30]` Non-negotiable 4's second half is machine-checked, and it took four
assertions rather than the one ADR-0013 asked for.** CPU near zero is also what a **dead** thread
reports, what a run that **never reached the mode** reports, and what an engine woken by its own
100 ms timeout reports. The last is not hypothetical: with `Block` made to ignore readiness,
`scripts/check-standard-gives-the-core-back.sh` measured **0% CPU**, found the thread sleeping in
**20 of 20 samples** — assertions 2 and 3 both green — and a round-trip p50 of **99 046 599 ns**,
one whole timeout. Only the fourth assertion saw it. On this shared container: `standard` 0.00%
CPU / 20-of-20 sleeping / p50 10 917 ns; `hft` 98.81% / 0-of-20 / 19 909 ns; `yield` 99.70% /
0-of-20 / 18 096 ns — **the first time `wait::Yield` has been shown to fail both gates rather
than said to.**

**`[measured 2026-08-30]` And that gate was green for the wrong reason twice before it was
believed.** A missing pair of braces (`$12` is `${1}2`) broke every measurement, and because
*failed the policy* and *could not be measured* shared one exit code, **both red halves reported
`RED ok`** while nothing had been measured at all — a red half that is red because the harness is
broken proves as much as a green half that is green because nothing ran. And the p50 was read
with `grep -oE '[0-9]+' | head -1`, which returned **50** every time: the digits in the *label*
`p50`. The assertion that is the only thing able to distinguish "woken by the data" from "woken
by the clock" was comparing a constant against its ceiling, and passing in all three arms, which
is exactly why nothing looked wrong.


**`[measured 2026-08-30]` A tool that accepted a mode, announced it, and did not run it.**
`tools/w2w --mode standard` printed `mode: standard`, exited 0, and never entered its timed loop:
the branch selecting the blocking strategy sat behind `#[cfg(feature = "standard")]`, and
**features are per-crate** — `w2w` declared none of its own, so the condition was false and every
such branch took its `else`. The engine's feature being on by default did not reach it. `cargo
build` had warned on that exact line the whole time (`unexpected_cfg_condition_value`); what
found it was running the tool and noticing the latency block was missing. This is `CLAUDE.md` §2
rule 6 **inverted** — that rule guards a feature in the manifest with no `#[cfg]`, which makes a
crate unbuildable; this is a `#[cfg]` with no feature in the manifest, and everything builds
while a code path disappears. `scripts/check-no-kernel-sleep.sh` would have been green about two
runs of the same mode, so `w2w` now states the mode it took and the script **reads it back**
(reversal: `w2w ran mode 'yield' when 'hft' was asked for`, exit 1). Write-up:
[reference/feature-flags-unify-across-a-workspace.md](docs/reference/feature-flags-unify-across-a-workspace.md).


**`[measured 2026-08-30]` A CI job was green about a build that never happened.**
`cargo test --all --no-default-features`, the machine check for non-negotiable 6, **still built
`libc`**: `tools/w2w` is a workspace member depending on `fixbolt-engine` with defaults, and
cargo unifies features across one invocation, so the flag under test was switched back on by a
sibling crate. `cargo tree --workspace --no-default-features -i libc` prints the edge. It was
caught by a **test count** — the run should have reported 210 and reported 214, the four tests
of a `cfg`-gated file that should have vanished — and **a module carrying no tests of its own
would have hidden it completely**. Fixed by `scripts/check-no-optional-deps.sh`, which asks per
crate and is proven by reversal (drop `optional = true` → red, with the graph printed).
Write-up: [reference/feature-flags-unify-across-a-workspace.md](docs/reference/feature-flags-unify-across-a-workspace.md).

**`[measured 2026-08-30]` A test that was green because another thread had not reused a file
descriptor yet.** `crates/engine/tests/standard.rs` closed a socket and asserted that `poll`
called its descriptor unknown. It went red on the first cold run and then passed **30 runs in a
row**; the panic site named the `Ok(count == 0)` branch, which says what happened — another test
thread in the same binary had been handed that number, so the descriptor was live and quiet, and
quiet is indistinguishable from closed at that layer. Asking about `i32::MAX` instead is
deterministic: **0 failures in 40 runs**. A retry would have buried it.


- **The whole suite runs on the owner's desktop, 2026-08-30 — the first time any of it has.**
  `[measured]` rustc 1.98.0 on AMD Ryzen 7 3700X / Linux 7.0.0-30: `cargo fmt --check`,
  `cargo clippy --all-targets -D warnings`, `cargo test --all` and
  `cargo test --all --no-default-features` all exit 0, **0 failed**. The two gates that carry a
  number: `-p fixbolt-session --test score` 4 passed, and **`-p fixbolt-engine --test wire`
  passed** — the same 59 definitions through a real socket. That last one is the point:
  open item 17 recorded this gate at **39 / 59 on Linux**, and it is now green on a *second,
  unrelated* Linux machine. The fix from `gates-that-can-be-trusted` holds off the CI runner.
- **`benches/alloc.rs` holds on x86_64, not only on the M5.** `[measured 2026-08-30]`
  `scripts/bench.sh` on the desktop: **8 of 8 targets measuring, 0 silent, invariant failures
  0** — `codec` 6 paths, `session` 13 paths, `engine` 7 paths, every one **0 allocations**.
  Non-negotiable 1 is machine-checked on two architectures now. Three timing groups are over
  their ceilings on this untuned box — see open item 20, where the interesting part is *which
  direction* they miss in.
- **`§9 satisfied`, for the first time in this project — 2026-08-30.** `[measured]` after
  `isolcpus=6,7,14,15 nohz_full=6,7,14,15 rcu_nocbs=6,7,14,15 processor.max_cstate=1` on the
  kernel command line plus the five runtime rows, `scripts/check-machine.sh` reports
  **`pass 10  fail 0  unknown 1`** and `scripts/bench.sh --strict` **ran for the first time**
  rather than refusing. These are the first figures this repository has that non-negotiable 10
  permits publishing. `EXIT=1`, on three ceilings and one marginal:
  `walk 4 levels` **354.6** vs 300, `encode 1 group` **103.5** vs 75,
  `encode ExecutionReport` **240.0** vs 190, `ring, round trip` **500.5** vs 500;
  `ring, one way` **259.6** came in *under* its 260. Clean: `parse NewOrderSingle` 125.5 / 150,
  `parse Heartbeat` 55.6 / 70, `SendingTime from the cache` 4.9 / 5,
  `inline deliver + reply` 6.3 / 15. All eight targets measured, **0 invariant failures**.
  The three over-ceiling cases are the same three that were over in every machine state
  measured that day, which is what makes them real rather than noise.
  **A prediction failed here and is recorded rather than quietly dropped**: `fail 2` was
  predicted after the reboot and the answer was **`fail 5`** — governor, turbo, SMT, THP and
  `busy_poll` are all *runtime* settings that do not survive a reboot, and only the two
  command-line rows do.
- **Isolating a core does not remove the 324 ns mode — hypotheses six and seven.** `[measured
  2026-08-30]` pinned to cores 6,7 under `isolcpus` + `nohz_full` + `rcu_nocbs`, where the
  scheduler places nothing else and the tick is off: **5/60 against 4/60 unpinned**, medians
  259.0 and 259.4. Not the scheduler. Interrupts do still reach an isolated core (15.7k on
  each of 6 and 7 against 512k on CPU0), so those were counted per run too — **two outliers
  carried ~1300 interrupts and three carried exactly zero**, while a normal run carried 1085.
  Not interrupts either. **Hypothesis eight, ASLR, went the same way and refutes itself twice:**
  250 runs with `setarch --addr-no-randomize` against 250 without gave **14/250 vs 14/250,
  z = 0.00** — and with ASLR off the layout is *fixed* across runs, so a layout-dependent
  effect would have to be 0% or 100%, not 5.6%. At n=60 the same comparison had read 8.3%
  against 3.3%; **the third time in one day a small-sample rate difference dissolved.**
- **The box does not drift, which is what makes it publishable.** `[measured 2026-08-30]` a
  7-minute soak — 379 runs of `ring, one way`, §9 satisfied, pinned to the isolated cores, with
  temperature sampled per run — moves the median from **259.0 to 258.9 ns**, 0.2% between the
  first window and the last, while Tctl rises 59 → 64 °C in the first minute and then sits
  there. `r(temp, ns) = +0.060`. The 3700X throttles at 95 °C, so the **31 °C of headroom**
  means the Mini-ITX case and its `silent` fan profile constrain nothing at this load; the
  91 °C seen earlier came from an eight-spinner stress test, and even there the frequency never
  stepped down. Also settled: **GNOME Power Mode `Balanced` does not reach the measurement** —
  the driver is `amd-pstate-epp` and EPP already reads `performance` under `Balanced`, while a
  measurement's `governor=performance` collapses the available preferences to `performance`
  alone. The A/B meant to prove that **failed and is recorded as failed** (`powerprofilesctl`
  errored, so both arms ran `balanced`); the state readings are what answer it. And the failure
  found a real side effect: **`SMT off` breaks `power-profiles-daemon`**, which writes
  `policy11` — a CPU that is offline while SMT is off. Harmless, reverts, worth knowing.
- **`scaling_cur_freq` is frozen on a `nohz_full` core and reads 41% low.** `[measured
  2026-08-30]` it reported the isolated core at **2240 MHz** — exactly `scaling_min_freq` —
  while that core executed **7,895,418 loops/s against an ordinary core's 7,958,092, 0.8%
  apart**. `amd-pstate-epp` refreshes that file from a periodic tick and `nohz_full` stops the
  tick on precisely the cores worth measuring on. **The isolation that makes a core worth
  measuring is what breaks the instrument pointed at it.** Measure work per unit time, or
  `aperf`/`mperf`; `check-machine.sh` is unaffected, it never reads a per-core current
  frequency. Fourth instrument in one day that could not see what it was pointed at.
- **The 324 ns mode is now characterised, even though eight hypotheses about its cause have
  all failed.** `[measured 2026-08-30]` pooling those 500 §9-satisfied runs of
  `ring, one way`: **two discrete states with an empty gap** — main mode n=472, median
  **258.4**, stdev 1.98; second mode n=**28** (5.6%), median **323.7**, stdev **1.25**; and
  **exactly one value out of 500 lies between them**. Both clusters are equally tight, so this
  is not a perturbed run — **a process picks one of two states at startup and keeps it for
  life**, the two differing by a factor of **1.2527**, near 5/4. The practical consequence,
  which is what makes this worth having without a cause: **any single run of this case has a
  5.6% chance of being 25% wrong**, and every `DESIGN.md` §6 ceiling is read off single runs.
  [reference/measured-costs.md](docs/reference/measured-costs.md).
- **A cloud VM cannot be the §9 machine, and the checklist could not say so.**
  `[measured 2026-08-30]` `governor`, `turbo`, `C-states`, `SMT` and NIC IRQ affinity are
  **host** properties. A guest does not fail those rows loudly — the `/sys` files are absent,
  so it collects `unknown` and reads as under-configured rather than structurally unable.
  `check-machine.sh` gained a **`not virtualised`** row (`systemd-detect-virt` + steal over
  the same window), guarded by `scripts/check-machine-verdicts.sh` — 11 cases, no VM and no
  root needed, in CI. **The GitHub runner is itself a guest**, so the bench job's machine
  block now says so on every run — `[measured 2026-08-30]` run `33314721411` prints
  `FAIL not virtualised  guest under 'microsoft', 0% steal` and
  `FAIL machine is quiet  6% CPU busy — Runner.Worker 11% of a core`. That closes the
  commit's own "not proven": the GUEST path had been tested as logic only and has now run on
  a real guest, correctly naming Hyper-V and attributing the load to the Actions worker.
  **Every bench figure CI has ever produced came from a machine that is both a guest and not
  quiet, and from this commit the output says so.** Consequence for the plan: **development can move to cloud;
  measurement stays on the desk.** The first version of the row reported this bare-metal box
  as a guest — `systemd-detect-virt` **exits 1 when the answer is `none`**, so
  `$(... || echo unknown)` ran both halves and set the variable to two lines. The verdict test
  could not catch it, because it feeds the function directly and never sees how the argument
  is obtained; running the real script did.
- **Tuning a machine to §9 moves every bench median by under 2%.** `[measured 2026-08-30]`
  30 full `scripts/bench.sh` runs on the desktop, 15 with §9 tuning on and 15 with it off —
  the first time any machine in this project could be measured **in both states**. Three cases
  are over their ceiling on **15 of 15 runs in both**; the two ring cases are **coin flips**
  with the ceiling at the median. **The first write-up of this overstated it and a repeat
  sample caught that** — see the entry, which is kept with its correction rather than
  rewritten: `over 260` went `1/15` → `9/15` between two samples of the same command, so the
  supportable numbers are the pooled **33% tuned vs 93% untuned**, not a flip from red to
  green. The median moves **0.8%** and reproduces; the pass rate does not. A second mode near
  **324 ns** — five sightings, **all tuned**, none in ~45 untuned runs — is **unexplained**;
  the Zen-2 L3 hypothesis was **tested and refuted** (~259 ns in all three arms), and SMT-off
  is a suspect that has **not** been isolated.
  [reference/measured-costs.md](docs/reference/measured-costs.md).
- **The kTLS blocker was the diagnostic, not the kernel.** `[measured 2026-08-30]`
  `setsockopt(TCP_ULP, "tls")` is **ACCEPTED** on the owner's desktop; open item 10 is
  unblocked and `ktls-spike` is unpaused. The script had been reporting `CONFIG_TLS=m` and
  "it was built without CONFIG_TLS" in the same run. Both that and a second defect found while
  fixing it — `lsmod | grep -q` under `set -o pipefail` reporting **failure on the run where
  the module is found**, because `grep -q` SIGPIPEs `lsmod` — are written up in
  [reference/ktls-on-a-plain-socket.md](docs/reference/ktls-on-a-plain-socket.md) and guarded
  by `scripts/check-ktls-classify.sh`, 8 cases, which scores 3 / 8 against the old logic.
- **The 512-entry inline field array in `matthart1983/nanofix` costs 4–6× the parse time.**
  Changing `MAX_FIELDS` 512 → 64 took the heartbeat from 565.0 ns to **95.4 ns** and
  `NewOrderSingle` from 605.1 ns to **138.8 ns**, on an Apple M5 on 2026-08-27. Method and
  caveats: [docs/reference/measured-costs.md](docs/reference/measured-costs.md).
- `cargo test --no-default-features --lib` on that project **fails to link** — its `aeron`
  feature does not gate `mod aeron_c;`. Its "238 tests" cannot be run as shipped.
- `scripts/fetch-quickfix-assets.sh` runs and reports **59 acceptance definitions** on disk.
- The `.def` format was decoded from the files and `Comparator.rb`: 7 directives, one
  `<TIME>` placeholder, literal `0x01` separators. It pins **field ordering** positionally.
- **The ordering rule is tag-ascending within header and body, not XML order** — checked by
  script over all 247 expected lines, zero violations. Open item 4 closed by data.
- **`test/definitions/client/` holds exactly one file** — `Normal.def`, 271 bytes, 6 lines,
  FIX 4.2, logon then logout. **470 of the 471 `.def` files test the acceptor.** It also uses
  two directives absent from the seven recorded for the server format: a bare `eCONNECT`, and
  `R` for a reply the harness sends. This answers ADR-0001 open question 1: there is no
  initiator conformance suite to run, and that is the central cost priced in ADR-0004.
- The QuickFIX Software License was read directly: BSD-3 in shape, plus attribution and a
  naming restriction.
- `git check-ignore -v` blocks `vendor/`, `testdata/recordings/`, `*.docx`, `target/`,
  `.DS_Store`.
- Repository is **private** as of 2026-08-27 (it was public at creation). `cargo 1.95.0`.
- **ADR-0001, -0002, -0003 accepted 2026-08-27** by the owner, after the latency-budget review.
- **Step 1 of the codec plan is CLOSED, 2026-08-28.** 54 tests green. The closing condition —
  533 real messages through a simulated TCP read loop under 5 chunk patterns and byte-at-a-time
  — passes. `[measured]` parse 77.0 ns, encode 93.8 ns, **0 allocations**, 304 million fuzz
  executions with no crash. Full method and machine in
  [reference/measured-costs.md §5](docs/reference/measured-costs.md).
- **The serialise target is missed: 93.8 ns against a published 60 ns.** Cause identified
  (linear slot lookup, slots × parts), deliberately not optimised — the number that decides is
  the Linux one at the `engine` step.
- **New open item: Criterion is deferred.** `DESIGN.md` §6 names it; the benches use a 24-line
  dependency-free harness instead, because the benches must *assert* and Criterion measures
  without asserting. The cost is outlier detection and confidence intervals. Revisit when
  hot-path work moves to Linux.
- **`codec` parses FIX as of 2026-08-28.** 29 tests green; all 539 `.def` lines classified.
  Two more plan defects surfaced and were decided by the owner: the plan contradicted itself
  about `2t` (the boundary diagram gave it to the session, the parse algorithm to the codec —
  resolved by refusing only what cannot be **framed**), and `14a_BadField` could not pass with
  a plain `Err`, so `ParseError::BadTag` now carries a byte offset and the index keeps every
  field read before the failure.
- **Nothing in the corpus can be checksum-validated.** `[measured]` 244 `E` lines carry `10=`
  and **0** are the real checksum of their own bytes; 238 are literally `10=0`. The comparator
  matches tag 10 by regex. **This constrains the conformance plan**: frame validation belongs
  on the `I` side and on the engine's own output, never on expected lines.
- **`codec` + `dict` step 0 and step 1 landed 2026-08-28** on branch `plan/codec-dict`.
  `dict` generates 5 tables from `FIX44.xml`: 912 tag constants, 93 message types, 30 header
  tags, 16 DATA→length pairs, 84 required-field arms. 11/11 tests green. The plan was wrong in
  three places about the dictionary and was revised and re-approved — see its delivery log and
  the new **[reference/fix44-dictionary-traps.md](docs/reference/fix44-dictionary-traps.md)**,
  which records all four traps with the test that guards each.
- **New open item: the 3 trailer tags are unclassified.** `is_header(89)` and `is_header(93)`
  return `false`, so a written `Signature` would sort into the body. Nothing writes one — no
  `.def` carries a signature — so this is pinned by a test rather than fixed.
- **CI exists as of 2026-08-28**, and closes a gap that had been asserted eight times. "CI
  proves it" appeared in `CLAUDE.md` §2, ADR-0004, ADR-0005, `DESIGN.md` D11 and `PRD.md`
  while `.github/` did not exist. It now has three jobs. Two of them cannot do anything yet —
  there are no crates — and they emit a *skipped, not passed* annotation rather than a silent
  green tick.
- **`[workspace.lints.clippy] all = "warn"` did not enforce non-negotiable 7.** `[measured]`
  2026-08-28: a library crate containing `unwrap()`, `expect()` and `panic!()` passed
  `cargo clippy --all-targets -- -D warnings` with **exit 0**, because those three are
  `clippy::restriction` lints and `clippy::all` never includes them. `CLAUDE.md` §2 had
  claimed the workspace lints enforced this since the repository was initialised. Fixed, and
  `scripts/check-lint-config.sh` now proves it by reversal so it cannot silently regress.
  `priority = -1` on the group is mandatory — cargo rejects the config without it.
- **DESIGN.md, README.md, ADR-0003 and three reference pages swept 2026-08-27** to match the
  accepted decisions: bidirectional positioning, one latency floor figure (10–20 µs, was two
  conflicting), `MessageView` corrected to 24 bytes in all four places that claimed 16 or
  "two words", the real `parse_into` signature, D9 rewritten as a parts list, D11 added for
  TLS, an 8-step build order with the initiator in it, and three new gate rows. The
  `codec-dict` plan's doc checklist records which items this closed and which still need code.
- **ADR-0004 and ADR-0005 accepted 2026-08-27** by the owner. -0004 makes the engine
  bidirectional; -0005 makes TLS a transport with the hot-path guarantee stated per mode.
  Both accepted on reasoning, not measurement — ADR-0005's open question 1 is unanswered and
  load-bearing (open item 10).

- **Repeating groups, 2026-08-28.** `dict` generates group tables keyed by `(msg_type,
  counter)` — **59 counters, 731 positions**, from **93** `<group>` declarations of which
  **91 sit inside `<component>`**. `codec` reads groups nested to FIX 4.4's full depth of 4
  and writes them ordered by the dictionary. `group_roundtrip.rs`: **357 top-level positions
  round-trip byte-identical**, exercising all 59 counters. `benches/alloc.rs`:
  **0 allocations** walking four levels.
- **The group field order agrees with QuickFIX's own generated C++ on 730 / 730 groups**
  — delimiter exact everywhere, and QuickFIX's `message_order` an exact subsequence of this
  crate's member list in every case, with the 7 extra tags all being nested group counters.
  `crates/dict/tests/interop_quickfix_order.rs`. The one group QuickFIX has no file for is
  `NoHops(627)`, which lives in `<header>`: 730 + 1 = 731.
- **That interop test catches what the round-trip cannot, demonstrated by reversal.**
  Swapping two adjacent members in every generated group leaves `group_roundtrip.rs`
  **green** — it generates its messages from the same table — and turns the interop test
  **red**. Run 2026-08-28.

- **The conformance runner, 2026-08-28.** The 59 definitions load as **669 steps** — 289 `I`,
  250 `E`, 65 connects, 1 disconnect, 64 expected disconnects — and run in process against a
  `SessionUnderTest`. `NullSession` scores **0 / 59**, which is the true state of the world;
  `Replay`, which answers with each file's own expected output, scores **59 / 59**, which is
  what makes the zero evidence rather than a coincidence.
- **The echo application the corpus assumes.** `[measured]` 42 of the 250 `E` lines carry
  `35=D` and there are **22 application `(I, E)` pairs**; all 22 are reproduced, `9=101`
  included. A session state machine alone cannot pass this suite.
- **`<TIME>` is 17 bytes on an `I` line and 21 on an `E` line**, solved from the corpus's own
  `9=` values. The single-width substitution both loaders used before was wrong by 4 bytes per
  timestamp, and one loader now serves both crates.
- **The session layer scores 14 / 59, 2026-08-28 — step 2 of six.** The plan predicted 18. Its
  classification table said 12 files expect only `{A, 5}` back; solving that off the corpus
  gives **9**, and two of the reachable set turn on refusing a second connection with the same
  identity, which is step 6. The ceiling for step 2 is 14 and it scores 14. Eleven reversals
  were run; nine take the score down.
- **Two files had been passing by accident, and only a step that could *lose* points found it.**
  `1b_DuplicateIdentity` and `AlreadyLoggedOn` were passing because the second Logon was refused
  as *sequence too low* rather than as a duplicate identity — right answer, wrong reason. Making
  `connect` reset the sequence numbers, which `2i_BeginStringValueUnexpected` requires, exposed
  it. The score gate now names all 14 files.
- **The corpus cannot see the value of `52=`.** Stamping `SendingTime` from a constant instead of
  from the clock leaves the score at 14 / 59 with every test green: `52` is one of the five tags
  `fields.fmt` matches by shape. Held by `crates/session/tests/logon.rs` instead.
- **The step-1 six are still in the fourteen** — asserted separately, because a count that only
  goes up cannot say so.
- **Step 1 scored 6 / 59, 2026-08-28, exactly as predicted.**
  The six are `1c_InvalidSenderCompID`, `1c_InvalidTargetCompID`, `1d_InvalidLogonBadSendingTime`,
  `1d_InvalidLogonLengthInvalid`, `1d_InvalidLogonWrongBeginString`, `1e_NotLogonMessage` —
  named in the assertion, because a different six scoring 6 is a different result. Six
  reversals were run: five take the score to 5.
- **The sixth reversal did not.** Deleting "the first message must be a Logon" leaves 6 / 59,
  because `1e_NotLogonMessage.def` also carries a wrong `56=`. Two rules, one observation —
  written up in
  [reference/quickfix-acceptance-def-format.md](docs/reference/quickfix-acceptance-def-format.md)
  and now held by `crates/session/tests/logon.rs`.
- **The session layer allocates nothing**, on the accept path and on the refusal path, counted
  separately. `[measured]` `accept 0 refuse 0 tick 0 clock 0 text 0`; the reversal — one
  `format!` on the error path — reports `refuse 30000`.
- **`00000000-00:00:00` is not a date, and this loader was substituting it for `<TIME>`.**
  It is the corpus's placeholder for output the comparator never reads by value. Fixed to a
  real instant, which also fixed `<TIME-121>` running 86 279 seconds *forward*.
- **The four dictionary validation tables exist and agree with an independent generator,
  2026-08-28.** `[measured]` 912/912 tag numbers, 898/912 field types with 14 differences each
  named by tag, 12 524/12 524 (message, tag) pairs checked as 84 816 exhaustive answers, and
  1 708/1 708 enum values. Eight reversals were run and all eight go red.
- **The plan called the enum oracle weak and the plan was wrong.** A scouting script matched
  `const char Name_X = 'v';` and missed `const char Name_X[] = "vv";`, so it reported 228 of 245
  fields covered. The array form is 17 fields — including `SecurityType(167)`, the one field
  `14e_IncorrectEnumValue.def` actually tests. Read properly the oracle covers 245/245 and
  1 708/1 708, with zero exceptions. Written up in
  [reference/fix44-dictionary-traps.md](docs/reference/fix44-dictionary-traps.md).
- **`dict` grew about 33 KB of static data** and its build script still runs in under a second.
- **The session scores 27 / 59, 2026-08-28 — step 3, and the revised prediction was 27.** All
  thirteen `Reject (35=3)` files. Thirteen reversals were run and all thirteen go red.
- **All twelve `373` codes are produced, and that is asserted rather than inferred.** The test
  reads the codes out of the corpus's own `E` lines and checks the session emits each. The file
  count cannot say it: `14a_BadField.def` holds four cases, and answering all four with one code
  still passes the file.
- **One reversal was worthless and it is worth recording why.** It deleted a
  `next_in = seq + 1` that could never be reached, and nothing changed — which says nothing
  about the guard. A reversal has to alter behaviour before its green means anything.
- **The session scores 37 / 59, 2026-08-29 — step 4, and the revised prediction was 37.**
  Heartbeat, TestRequest, inbound `SequenceReset`, `ResetSeqNumFlag`, and a garbled frame that
  is ignored rather than fatal. Thirty-two reversals were run and all thirty-two go red.
- **Whether a message's sequence number is checked is per `MsgType`, not one rule.** A Logout is
  never checked; a `SequenceReset` never advances the count at all. Applying one rule to
  everything costs a file whichever rule is picked — measured four ways. Written up in
  [reference/quickfix-acceptance-def-format.md](docs/reference/quickfix-acceptance-def-format.md).
- **A rule this project invented was refuted by the corpus.** `FieldType::SeqNum` refused `34=0`,
  and its comment cited `11c_NewSeqNoLess.def` — which it had misread. `11a`, `11b` and `11c` all
  send `34=0` and QuickFIX processes them; restoring the rule costs three files. The refused case
  lived in the block whose own doc comment says the cases there are invented. **An invented test
  agreeing with an invented rule is one guess written twice.**
- **Three reversals were worthless in step 4, all the same shape: two guards covering one rule.**
  Each pair was reduced to the single guard that can actually be broken. A fourth was a test that
  checked the output and not the link, so a reversal that dropped the connection stayed green.
- **The session scores 42 / 59, 2026-08-29 — step 5, and the prediction was 42.** A message running
  ahead of the count is held and the gap is asked for; an inbound `ResendRequest` over
  administrative messages is answered with one `SequenceReset` gap fill. Nineteen reversals were
  run and all nineteen go red.
- **A gap is asked for once, and a Logon that runs ahead is answered before it is asked.** Both are
  stated exactly once in the corpus, by `10_MsgSeqNumGreater` and `1a_ValidLogonMsgSeqNumTooHigh`.
  Written up in
  [reference/quickfix-acceptance-def-format.md](docs/reference/quickfix-acceptance-def-format.md).

- **The session scores 55 / 59, 2026-08-29 — step 6a, and the prediction was 52.** The session
  now owns seven administrative message types and hands everything else to an `Application`,
  giving it the outbound sequence number and the clock and sending back whatever it returns.
  Eight reversals into the score go red; two more behaviours the corpus cannot see are held by
  `crates/session/tests/application.rs`.
- **The prediction was beaten for a measurable reason.** The 6a/6b split was drawn from the
  expected `35=` sets of the remaining files, and an expected set cannot tell an *echo* from a
  *replay*: `2d`, `3b` and `3c` look like they need an outbound store and do not — the
  counterparty resends and this end only echoes.
- **QuickFIX reads a tag as a *signed* integer**, so `-1=x` is a field and is Rejected, while
  `4garbled9=x` is not a field at all and the whole message is ignored. Three files turn on
  that one distinction. Written up in
  [reference/quickfix-acceptance-def-format.md](docs/reference/quickfix-acceptance-def-format.md).
- **"One identity, one connection" is an engine rule, not a session rule.** `1b_DuplicateIdentity`
  and `AlreadyLoggedOn` need it, and `engine` does not exist, so `crates/session/tests/score.rs`
  plays the smallest engine that can hold two connections. That is stated there and here.

- **The session scores 59 / 59, 2026-08-29 — step 6b, and the plan's last four files came in as
  predicted.** A resend replays the application messages this end sent, at the numbers they were
  sent with and spending none, and fills over every contiguous run it cannot replay. Nine
  reversals into the score go red; three more behaviours the corpus cannot see are held by
  `crates/session/tests/journal.rs`.
- **`2m_BodyLengthValueNotCorrect` turned out to be about framing, not about a store.** `9=` is
  taken at its word: a body length that does not land on a `10=` trailer discards the whole
  receive buffer, which is why a too-long one swallows the message after it. That is the
  engine's job, so it lives in `tests/score.rs` next to the identity rule — and the rubbish is
  still handed to the session once, so "fatal only if it claims to be a Logon" stays in one
  place.
- **The outbound journal is a stopgap, and it is in the wrong crate.** Eight 512-byte slots
  inside the session, in memory, lost on restart. `DESIGN.md` D1 has the session emitting a
  `Store` action and the engine holding the journal; that is what `engine`'s plan has to build.

- **The mirrored acceptance gate tops out at 45 of 50, not 50, and it is worth less than it
  looks.** `[measured 2026-08-30]` 46 of the 50 mirrorable files need this end to *originate* a
  message no state machine can invent — 42 a Logout, 19 an application message, 14 an unprompted
  Heartbeat, 13 a TestRequest with a given `112=`, 6 a SequenceReset, 4 a ResendRequest — so the
  harness has to play the operator, and wherever it does the gate measures numbering and framing
  rather than a protocol decision. A further 5 ask this end to send a message that is wrong on
  purpose, which a correct engine cannot do.
- **`ADR-0004`'s mirroring criterion is syntactic; the corpus's client-side badness is
  semantic.** `ADR-0006` fixed one gap in it (`iDISCONNECT`). The five files above are a second,
  and they cannot be detected by looking at bytes: all five are syntactically perfect. Interop
  against `libquickfix` is the answer, and this is the evidence for why ADR-0004 made it the
  primary gate rather than the secondary one.

- **The 59 definitions pass through a real socket, first run.** `[measured 2026-08-30]`
  `cargo test -p fixbolt-engine --test wire` → **59 / 59**. Kernel TCP, the real
  framer, the real session, the real application; the clock is the only injected part,
  because every `I` line in the corpus carries a fixed instant. No background thread and no
  sleep: `Engine::turn` is one non-blocking pass and the test drives it by hand, so it is as
  deterministic as the in-process gate.
- **The engine allocates nothing on the byte path.** `[measured 2026-08-30]` six paths at
  zero — idle, send, recv, frame, turn, busy — each proven by injection
  (`crates/engine/benches/alloc.rs`).
- **Non-negotiable 4 has no machine check, and the attempt to build one failed.** `dtruss` is
  refused by macOS SIP; reading undefined symbols out of the compiled rlib passes even with a
  `thread::sleep` added, because `Engine` and `serve` are generic and are never
  code-generated into the library. The script was deleted rather than shipped. Hand-check
  until `tools/w2w` runs on Linux — open item 15.
- **A bench reporting "1 allocation per 1000" was measuring a dropped connection.** The busy
  case replayed one Logon, which the session refuses the second time as a sequence number
  already used; from iteration three the engine held no connections and the count was the
  test double's queue doubling. Every case now asserts its own path is live.
  [reference/measured-costs.md](docs/reference/measured-costs.md).

- **The ring hop costs ~50x the inline call, and the number is published rather than
  assumed.** `[measured 2026-08-30]` inline **2.7 ns**, ring **128.0 ns** one way, **242.5 ns**
  round trip, on a 163-byte `NewOrderSingle`, Apple M5, macOS 25.6, unpinned.
  `crates/engine/benches/dispatch.rs` asserts 15 / 260 / 500 ns and was proven to assert by
  lowering a ceiling and watching it go red.
- **The dispatch chooses a thread, not a protocol.** The same message produces byte-identical
  output under `InlineDispatch` and `RingDispatch` — `crates/engine/tests/dispatch.rs`. And a
  reply for a connection that has hung up is dropped rather than delivered to whoever took its
  slot, because routing is by id and `swap_remove` reuses indices.
- **The ring is `AtomicU8`, not `unsafe`, and that was a decision with a price.**
  [ADR-0007](docs/decisions/ADR-0007-spsc-ring-without-unsafe.md): the plan authorised neither
  `unsafe` nor a dependency, so the copy is byte-at-a-time and costs ~0.8 ns per byte. The
  reversal is cheap and is written down.

- **A slow consumer ends with a reason, not in silence.** `DESIGN.md` D10 built:
  `Disconnect` (default), `Queue { max_bytes }`, `Block`.
  `crates/engine/tests/backpressure.rs`, six tests, and both guards proven by reversal —
  truncating instead of refusing turns three red, and removing the dead-socket check turns one
  red.
- **Two real bugs came out of writing those tests, not out of reading the code.** A socket that
  died with bytes still queued left the connection `Up` for ever, because "finished" meant
  *closing and the queue is empty*. And `Queue { max_bytes }` smaller than one Logout would
  have ended the session silently — the message that says why is now written into the whole
  `TX` buffer after the queue is discarded.
- **The tick moved to the front of `Connection::turn`, and the wire gate did not move.**
  `[measured 2026-08-30]` still **59 / 59**. `received_with` has no clock (D1), so a session
  that has never ticked judges `SendingTime` against zero and refuses the first message on
  every connection. It had been worked around in three places; the workarounds were deleted in
  the same commit as the fix.

- **The journal left the session, and D1's debt is paid.** `[measured 2026-08-30]`
  `fixbolt_session::journal::Journal` is a trait the caller supplies, like `Application`; the
  session holds no bytes it did not generate. Three D7 tiers as three types: `NoJournal`,
  `MemJournal`, `FileJournal` with `Durability::{Async, Fsync}`.
  [ADR-0008](docs/decisions/ADR-0008-journal-is-a-trait.md) records why a trait and not the
  emitted `Action::Store` D1 sketched — **a resend has to read, and an action cannot answer.**
- **The acceptance score depends on the journal, and the reversal proves it.** Making
  `MemJournal::put` keep nothing turns four of `tests/journal.rs`'s seven red **and** drops
  `--test score` below 59. Restored: 59 / 59, and 59 / 59 over a socket.

- **The wire gate was 39 / 59 on Linux, the cause was Nagle, and the first diagnosis of it was
  wrong.** `[measured 2026-08-30]` `cargo test -p fixbolt-engine --test wire` scored 39 / 59
  while `--test score` over the same corpus on the same machine scored 59 / 59. Walking the
  harness's `quiet` bound walked the score — 200 → 39, 2 000 → 43, 20 000 → 59 — and that was
  read as "a spin count is not a settle criterion". **It was Nagle on the harness's own client
  socket.** `2m_BodyLengthValueNotCorrect` sends a frame that produces no reply, so no
  piggybacked ACK; the peer's delayed ACK holds; four `I` lines coalesce into one 477-byte read
  and the framer discards all four. The longer timeouts were outwaiting the delayed ACK. The
  engine already sets `TCP_NODELAY` (`transport.rs:68`); the harness did not, which made the
  test rig the only Nagle-enabled peer. **Fixed with one line, and proven by a 2 × 2**: spin
  count and wall-clock bound both score 39 / 59 without `set_nodelay` and 59 / 59 with it.
  Removing that line from the finished fix returns exactly 39 / 59. **Item 17 closed**;
  full write-up, including what the wrong diagnosis cost, in
  [reference/measured-costs.md](docs/reference/measured-costs.md).
- **The wire gate is 59 / 59 on Linux, and its bounds are flat.** `[measured 2026-08-30]`
  59 / 59 at a 1 ms and a 20 ms quiet window; only the run time moves, 0.8 s against 14.5 s.
  The `settle` hook an earlier draft added to `fixbolt_conformance`'s public trait was
  **deleted rather than shipped**: the reversal meant to prove it left the gate at 59 / 59 with
  it disabled.
- **The toolchain is pinned and the lint that turned `main` red is fixed.** `[measured
  2026-08-30]` `clippy::byte_char_slices` reproduces on `1.98.0` — installable here, so it did
  not need CI to prove — and `rust-toolchain.toml` now pins 1.98.0, with an advisory
  `clippy-latest-stable` CI job that never blocks a merge. **Item 19 closed.**
- **The rest of the suite is green on Linux, on this toolchain.** `[measured 2026-08-30]`
  same box, `cargo 1.94.1`: `cargo fmt --check` clean, `cargo clippy --all-targets -- -D
  warnings` clean **here and red on the runner's newer clippy — see the next entry**,
  `scripts/check-lint-config.sh` green in both directions, `cargo test --all` **158 passed /
  1 failed** across 30 test binaries — the one failure being the wire gate above — and
  `cargo test --no-default-features` fails on that one and nothing else.
- **CI had been red on `main` for both of those, and nobody read it.** `[measured 2026-08-30]`
  GitHub Actions run `33291318638`, commit `9986890` — the tip this branch was cut from —
  fails two jobs. *Builds with nothing optional installed* fails on
  `the_fifty_nine_definitions_pass_through_a_real_socket`, which is the wire gate above: **the
  runner had been saying 39 / 59 within a minute of the engine merging**, while this page, the
  `README`, `DESIGN.md` §6 and `PRD.md` all said 59 / 59 on the strength of a laptop run.
  *fmt · clippy · test* fails on `clippy::byte_char_slices` at
  `crates/dict/tests/interop_quickfix_fields.rs:133` — a lint that does not exist in
  `clippy 0.1.94` here or `1.95.0` on the M5, and does in the runner's `1.98.0`. **CI installs
  whatever stable is current and there is no `rust-toolchain.toml`**, so `-D warnings` denies
  lints that had not been written when the code was. Both written up in
  [reference/measured-costs.md](docs/reference/measured-costs.md). **Open items 18 and 19.**
- **Item 7 closed: the corpus is pinned and verified.** `[measured 2026-08-30]`
  `scripts/fetch-quickfix-assets.sh` defaults to commit `386ce46e917a` instead of `master` and
  checks the three counts the documents quote — **59 definitions, 539 message lines, 244 `E`
  lines carrying `10=`** — failing at the fetch rather than three layers away. Proven by
  reversal: adding one `.def` gives `CORPUS MISMATCH: acceptance definitions is 60`, EXIT=1;
  removing it, EXIT=0. **The first reversal was worthless and it is worth saying why**:
  `QUICKFIX_REF=master` passed, because `master` currently *is* the pinned commit. And the pin
  buys nothing today — upstream has not drifted. Its value is entirely future; only the
  mechanism is proven.
- **Item 18 closed, and the plan was wrong about how.** The two dead rustdoc links were
  **external URLs**, which `check-links.py` skips by design, so walking `.rs` files would not
  have caught them. The rule that does, and needs no network: **a file in this repository must
  be linked by relative path, never by absolute URL.** It found **three**, not two. Scanning
  `.rs` first produced **13 false positives**, all rustdoc intra-doc links naming Rust items
  rather than files — the trap the plan had named in advance, caught by the guard it named.
  `CLAUDE.md` §9 now requires a green CI run named by id for the commit being closed, and §10
  gains two more failures no gate can see.
- **Item 15 closed: *the engine thread never sleeps in the kernel* has a machine check, and
  the check proves itself.** `[measured 2026-08-30]` `tools/w2w` exists and runs;
  `scripts/check-no-kernel-sleep.sh` traces it with `strace -f` and attributes syscalls to the
  engine thread **by tid** — the client blocks on purpose and would mask everything. The engine
  thread made **3111 `recvfrom`, 3111 `accept4`, 351 `sendto`, and zero** of `epoll_wait`,
  `poll`, `select`, `futex`, `nanosleep`, `sched_yield`. **The script then runs the binary again
  with `wait::Park` and fails if that does not trip it** — `RED ok — --park trips it: 1749
  sched_yield`. That second half is the point: this rule had two machine checks before and both
  were green with a `sleep` present. The zero is only accepted because the same run separately
  proves the thread did socket work.
- ~~**`tools/w2w` measures an administrative round trip, not an application echo.**~~ —
  **fixed 2026-09-02.** `--path app` drives `NewOrderSingle` → `ExecutionReport` through an
  application holding a template built once, which is the path `DESIGN.md` §8 describes.
  `--path admin` is still the default and is what both mode gates drive, because changing it
  would move every figure those scripts have printed.
- ~~**`[measured 2026-08-30]` w2w on this box: min 14 967 / p50 29 745 / p99 67 943 ns**,
  5 000 samples, 4 vCPU container~~ — **superseded 2026-09-02 by the §9 desktop figures.** Kept
  because the disclaimer it carried was correct and was honoured: no row of §8 was ever changed
  on the strength of it.
- **The blocker on item 10 was wrong, and knowing that is most of the value.** `[measured
  2026-08-30]` it had been recorded as needing the §9 machine of item 6. kTLS is a **kernel
  feature, not a latency property**, so it needs no such machine — but a Linux box is not
  enough either: it needs a kernel built with `CONFIG_TLS`. This one is not.
  `setsockopt(TCP_ULP, "tls")` on a real connected socket returns **`ENOENT` on both ends**,
  and the kernel config reads `# CONFIG_TLS is not set`. **The config line alone would not
  have been enough** — a config says what was compiled, not what a container may do; the
  syscall says both, which is why `scripts/check-ktls-available.sh` makes the call rather than
  grepping.
- **Items 8 and 9 closed: the DATA write path has invariants, and they are refusals.**
  `[measured 2026-08-30]` `TemplateBuilder::build` refuses a DATA field declared without its
  length field (`EncodeError::DataWithoutLength`); `encode_with` refuses the same inside a
  repeating group before a byte is written; and **the encoder computes the length from the
  data**, ignoring what the caller passed — `0x01` inside the value included. Six tests in
  `crates/codec/tests/data_encode.rs`, each rule proven by reversal with the injection
  confirmed present first.
- **The ordering defect was real and had shipped.** `[measured 2026-08-30]` fifteen of FIX
  4.4's sixteen DATA pairs have `length == data - 1`, so sorting body tags ascending put them
  right **by arithmetic accident**. `Signature(89)` takes `SignatureLength(93)` and was emitted
  **before** its length — unframable by any reader. Field order now places a DATA field one
  place behind its length field's tag, which fixes all sixteen without a special case. Written
  up in [reference/fix44-dictionary-traps.md](docs/reference/fix44-dictionary-traps.md).
- **In groups the order was already right, and that is not the same as tested.** `[measured
  2026-08-30]` **66 DATA members across the group tables, all 66 with the length declared
  immediately in front.** `group_roundtrip.rs` no longer skips them: it writes **508 DATA
  members, each with a separator inside its value**, and asserts that count is non-zero —
  a round-trip that covered no DATA member would look exactly like one that did.
- **`benches/alloc.rs` writes a DATA field at zero allocations**, and the case asserts its own
  path is live before the zero counts. Proven by injection: one `format!` in that loop reports
  **10 000**.
- **The ring holds 56.7 µs of slack, and ADR-0002 assumed milliseconds.** `[measured
  2026-08-30]` `crates/engine/benches/ring_full.rs`, Linux 6.18 x86_64: at the 65 536-byte
  capacity `benches/dispatch.rs` measures the hop at, a stalled application costs the engine
  **352 messages and 56.7 µs** before the ring starts refusing. ADR-0002 bought the ring so that
  *an application that stalls does not stall the session layer*, priced against a stall of
  "milliseconds" — **one millisecond overflows this ring about eighteen times over.** The ring
  as sized does not buy what it was bought for, which makes capacity part of the policy decision
  rather than a tuning detail. The bench asserts the ring **accepted** before it refused: one
  that rejected everything from the first message would print a plausible number.
- **A gate has been red on Linux the whole time and nothing ran it.** `[measured 2026-08-30]`
  `cargo bench -p fixbolt-engine --bench dispatch` fails here: **ring one way 332.5 ns against
  its 260 ns ceiling**, while inline is 5.4 ns against 15 and passes. The ceilings came from the
  M5 (2.7 ns and 128.0 ns) with ~2× headroom; a cross-thread hop on a shared 4 vCPU host is
  2.6× the M5's, and **nothing is regressing** — the gate is measuring the machine.
  **`cargo test --all` does not run a `harness = false` bench and no CI job runs `cargo bench`,
  so this has never been reported by anything.** Found by hand, while doing something else.
  It also means the commit before this one stated its gates without this one, because
  `CLAUDE.md` §7 names `alloc` and the Criterion suite together and only `alloc` was run.
  **Open item 20**; not fixed here, because every fix changes how a `DESIGN.md` §6 gate is
  measured.
- **The journal reads back, and it could not have before.** `[measured 2026-08-30]` the
  on-disk record was `seq(4) || message` with **no length**, so records could not be separated
  on read: the file was append-only by construction and item 16 could not be closed without
  changing it. Now `seq(4) || len(4) || bytes`. `FileJournal::open` reads the file before
  appending, `Journal::highest()` says what is held — **deliberately with no default
  implementation**, because a default `None` would let a journal that holds messages report
  that it holds none — and **a torn tail is dropped rather than half-read**. Four tests in
  `crates/engine/tests/recovery.rs`, each using `FileJournal` and dropping it between the write
  and the read: a `MemJournal` there would prove nothing, since the restart is the question.
- **A reversal reported PASS because it inserted nothing, and `grep` is what caught it.**
  Changing `max()` to `min()` in `highest()` silently failed to apply — `cargo fmt` had joined
  the line, so the replacement string did not match. The count came back **0** and that is the
  only reason it was not read as "this guard cannot fail". Re-injected properly it goes red
  (`left: Some(7), right: Some(8)`). This is `false-greens.md` §5's own case, hit while quoting
  it. **Confirm the injection is in the file before reading the result** — every time.
- **The corpus resets on every connect, and it cannot settle whether that is right.**
  `[measured 2026-08-30]` three files reconnect — `2i` (2 connects), `2k` (3), `2o` (2) — and
  **all seven expect `34=1` back**, with **no `141=Y` on any of those Logons**; one file in the
  corpus mentions `141=` at all. But FIX numbers a *session*, not a *connection*, and QuickFIX
  persists across a reconnect in a deployment while its harness starts each `iCONNECT` from a
  clean store. **The corpus and a real deployment want opposite behaviour, and `connect` cannot
  tell which it is in.** [ADR-0010](docs/decisions/ADR-0010-a-reconnect-is-not-a-restart.md)
  proposes separating the two and is `Proposed`.
- **Seven cases went back to `testing-skills`, and the queue now matches the rule.**
  [PR #2](https://github.com/tmthang86/testing-skills/pull/2), draft, on branch
  `claude/false-greens-from-a-protocol-engine`: three new numbered cases in `false-greens.md`
  (the knob that moved with the fix and was not the cause; the check nobody ran; fifteen out of
  sixteen), a fourth on instruments that cannot see what they were aimed at, two more ways a
  reversal inserts nothing added to its §5, and two foldings into its §2 and §11. **The first
  commit contributed four of the seven due and the second fixed that** — `CLAUDE.md` §11 says a
  case goes back when the plan that found it closes, and three came from closed plans.
  `[measured]` upstream's own checks on the branch: `validate-repo.mjs` 0 errors 0 warnings,
  `compare-design.mjs --self-test` 33/33, `check-theme-contract.mjs --self-test` all passed.
  **One case is correctly still held**: the `ktls` blocker case, because that plan is paused
  rather than closed.
- **The rest went back as two stacked pull requests, because a third would have collided with
  two that were already open.** `[2026-08-31]` the obvious move — one branch off `main` with
  everything on it — was written first and **rejected on push**: upstream already had
  `claude/false-greens-from-a-protocol-engine` at PR #2, the exact branch name, with §10–13 of
  `false-greens.md` already occupied. So:
  [**PR #6**](https://github.com/tmthang86/testing-skills/pull/6) is based on **PR #2's branch,
  not `main`**, and renumbers this plan's six cases to **§14–19** — the negative result that was
  negative for the wrong reason (and the gate whose own red halves both printed `RED ok`); the
  configuration under test that was never built (and the A/B harness that verified the result but
  not the selection); the number parsed from the label `p50`; the test that assembled what it
  checked (and the doc comment refuted by reversal); the identifier already given back; and green
  because the runtime was masking it. The checklist gains a **second half**, 22–29, for red
  results and for gates that contain their own red half.
  [**PR #7**](https://github.com/tmthang86/testing-skills/pull/7) is based on **PR #1's branch**
  and carries the protocol reference into `docs/drafts/`, **not** into the skill's `references/`.
  **That placement is the point.** PR #1 proposes `protocol-e2e-testing` as a *sibling* skill and
  states the design decision *"one skill owns one medium"*; the first draft here had added a sixth
  row to the `e2e-testing` router, which would have answered an open design question by accident.
  A draft in `docs/`, linked only from the roadmap item, lands the measurement and leaves the
  decision where it belongs. `[measured]` on both branches: `validate-repo.mjs` 0 errors 0
  warnings, `test:unit` all passed, TOC anchors 24/24 and section numbers `1..19` and `1..29` each
  exactly once. `test:integration` was **not run** — it wants a Playwright browser build the
  container does not have, and both diffs are markdown-only.
- **The markers stay until the pull requests merge.** `CLAUDE.md` §11 says
  `[to testing-skills]` is replaced by the pull-request link *when it lands*; #2, #6 and #7 are
  all open drafts, so `grep -rn '\[to testing-skills\]' docs/` is still the backlog and still
  reads honestly.
- **The contribution is evidence upstream could not previously have.** Its roadmap names its
  biggest gap as *nothing proven against a real system*, with everything measured on one Tauri
  app through a UI. These came from a system with **no UI at all** — no browser, no locators, no
  screenshots — and the same shapes appear, which is what makes them not browser problems.

## Not proven — claimed, researched, or simply not yet run

`[2026-09-05]` **The four buffer defaults have never been checked against a real counterparty.**
`N = 256`, `RX = 4096`, `TX = 8192`, `APP = 1024` are now parameters rather than literals
([plan](docs/plans/2026-09-05-buffers-the-caller-can-size.md)), so a deployment can name its own
— but **the defaults themselves rest on no measurement.** The acceptance corpus never exceeds
200 bytes and its longest reply is 177 body bytes, so it cannot speak to any of them. Artio, the
closest engine in philosophy, defaults its receive buffer to **16 KiB, four times this one**;
QuickFIX C++ has no ceiling at all and grows a `std::string` until the process dies
(`docs/reference/prior-art.md`). Raising `RX` costs `[measured 2026-09-04]` **+0.57%** per
connection against the ~2 MiB journal ring, so memory is not the obstacle — **`benches/turn.rs`
at two values of `RX` on the §9 desktop is**, and it belongs to wave C with the
defaults-for-a-PROD-server ADR (`SLOTS`, `SLOT_LEN`, `TX`, `RingDispatch`).

`[2026-09-01]` **eight of these bullets were false when this section was read, and one had been
false for three days.** This is the section whose entire job is honesty, and it is the one that
rotted — because every closing plan updated the item it closed and nobody re-read the list of
things not yet proven. **The rule that follows from it: a plan does not close until this section
has been read line by line**, not only until its own open item is struck. The false ones are
kept with a strike-through and their closing date rather than deleted, so the failure is legible
instead of tidied away.

- ~~**`tools/w2w` was run on 2026-09-02 on a shared 4-vCPU cloud VM and none of its figures is
  a latency number**~~ — **superseded the same day by the §9 desktop.** Kept because it is now
  the comparison it was recorded to be: the VM read `hft` p50 **34 087** and `standard`
  **54 787**; the §9 desktop reads **16 010** and **19 447**. **The VM was 2.1× and 2.8× slower
  on the same binary and the same path**, and it was labelled unpublishable at the time for the
  right reasons. That is what a figure recorded honestly and then superseded looks like.
- ~~**`scripts/bench.sh --strict` is RED on the §9 desktop and two cases reproduce over their
  band.**~~ **STRUCK 2026-09-05 — item 41 is closed and `--strict` exits 0, three consecutive
  runs**, `timing over baseline 0 · cases w/o a baseline 0 · cases under the band 0`, read line
  by line rather than off the exit code. **Neither case was a regression**, and it took two
  different answers: `presession, read and route` does twice the `field_value` scans since
  ADR-0026 widened `Identity`, proven by a one-variable reversal reading **84.1 ns** against a
  baseline of 84.0; `encode ExecutionReport (template)` was **a 16% measurement of where the
  compiler put the code** — the jump is one commit that touches no `crates/*/src/` file, and
  adding inert functions the encoder never calls walks it across 236.5–292.4 ns
  ([ADR-0049](docs/decisions/ADR-0049-bench-builds-pin-function-alignment-and-the-flag-is-read-back.md),
  [reference](docs/reference/a-benchmark-that-measures-where-the-compiler-put-it.md)). Bench
  builds now pin function alignment and **read the flag back off the binaries**; all 24
  baselines are recorded under that build, `n = 20`, `pass 12 fail 0 unknown 1`.
- ~~**Every figure for `crates/library` is from that same machine**, including the 40 ns for a
  prebuilt template and the ~2.1 µs for `App::on_message`. `benches/baselines.tsv` has no row
  for this CPU and none was added — `cost.rs` prints `NO BASELINE`, which is the honest state.
  The **ratio** is what open item 34 and ADR-0041 rest on; a §9 run could move the absolutes
  in either direction and nothing here predicts by how much.~~ **STRUCK 2026-09-05, and the
  ratio did not transfer — its denominator was never measured.** All three `library` cases are
  in `baselines.tsv` from the §9 desktop, n = 20: parse **159.6**, reply **804.1**, `on_message`
  **1028.6**, against `encode ExecutionReport (template)` **237.6** → **3.4×**, not 24×. The
  *40 ns* came from an experiment that was never committed, and the committed benchmark for
  that shape read 177–199 on the same VM class. [ADR-0051](docs/decisions/ADR-0051-item-34-is-a-third-of-the-size-it-was-recorded-at.md);
  item 34 closed.
- **`examples/acceptor.rs`'s `main` is compiled and not run.** Its two command-line arguments,
  its two `println!` lines and the stdin thread that calls `admin.shutdown` are the only part of
  that example `tests/end_to_end.rs` does not drive; every other line of it does run, because the
  test loads the same handler file. `[2026-09-05]` **what that thread does is covered, by a
  different test**: `an_operator_stops_the_front_door_and_serve_comes_back` takes an `Admin` off
  a `Handles`, stops a real `serve`, and reads the `Shutdown`. What is still unrun here is this
  file's own wiring of it.
- **The three `cost.rs` cases do not add up and nobody knows why.** parse 190 + reply 2140 =
  2330, `on_message` reads 2062–2131 — about 200 ns *less* than the sum, against a 3%
  run-to-run spread. Recorded in the bench's own module comment and in ADR-0041's open
  questions rather than smoothed over.
- **Every figure in [prior-art.md](docs/reference/prior-art.md) is someone else's claim**,
  including all of fix8's and Artio's. Nothing from those projects was run here.
- ~~The **150 ns gates** in `DESIGN.md` §6~~ — **gone since 2026-08-31.**
  [ADR-0016](docs/decisions/ADR-0016-per-machine-baselines-replace-absolute-targets.md)
  withdrew every absolute nanosecond ceiling; §6 now compares each case against **that
  machine's own line in `benches/baselines.tsv`**, and a CPU with no baseline reads
  `NO BASELINE` and is fatal under `--strict`. What this bullet warned about — a laptop number
  posing as an SLA — is now structurally impossible rather than merely disclaimed.
- ~~**Every figure in `DESIGN.md` §8 is from the literature**~~ — **five stage rows and the
  bottom line are measured now**, on the §9 desktop: parse **122.6 ns**, serialise
  **239.1 ns**, inline dispatch **8.5 ns**, ring hop **267.4 ns**, the `hft` wakeup as
  `Engine::turn` at **449 ns per session**, and `[measured 2026-09-02]` **the round trip
  itself**, `hft` p50 **16 010** and `standard` **19 447** — which turns §8's *"under 5% of the
  total"* from arithmetic over a borrowed denominator into **2.9%** over a measured one. **The
  `standard` wakeup row stops being literature too, as a difference**: 3 437 ns measured plus
  the ~449 ns sweep it replaces is ~3.9 µs, inside the 2–5 the literature gave.
  **What is still literature is the part the NIC owns** — NIC→socket 3–8 µs, `send`→NIC
  3–10 µs, and therefore the 10–20 µs floor, none of which loopback contains. Open item **40**.
  ~~**And one row is now known to be measuring less than its label says**: parse is a `NoDict`
  figure and the session's dictionary pass has no benchmark — open item **39**.~~ **STRUCK
  2026-09-05 — the pass is timed.** `[measured 2026-09-05]` **897.3 ns** on the
  `NewOrderSingle` `tools/w2w` sends, **218.4 ns** on its `TestRequest`, n = 21, `pass 12 fail
  0 unknown 1` ([ADR-0050](docs/decisions/ADR-0050-the-dictionary-pass-is-public-so-it-can-be-timed.md)).
  §8 has a row for it and the 2.9% above is **8.5%** for an application message. **The pass was
  named as the largest candidate for the 3 898 ns and is 17.4% of it** — new open item **49**
  carries the ~2 804 ns that is still unattributed.
- ~~The ring-buffer hop (200–500 ns)~~ — **measured 2026-09-01: 267.4 ns one way, 515.7 ns
  round trip**, against **8.5 ns** inline (`benches/dispatch.rs`, 22–24 qualifying runs).
  ~~**The busy-poll *saving* is still a literature figure**~~ — **measured 2026-09-02**:
  `hft` against `standard` on the identical path is **3 437 ns, 17.7%**, wire to wire on the §9
  desktop, and the wakeup it buys out is therefore ~3.9 µs. It was deliberately left open when
  `standard-mode` closed and it wanted the same box as item 6, which is the box it got.
- `MAX_FIELDS = 64` is a starting number. No real message population has been surveyed.
- **In-group order is agreed with one other implementation, not with a counterparty.**
  QuickFIX's generator reads the same `FIX44.xml`. Two programs agreeing on how to read one
  file is real evidence and is not the same as a venue accepting the bytes. Nothing here has
  been sent to a real FIX peer.
- ~~**DATA fields inside a repeating group are untested**~~ — **closed 2026-08-30** with the
  `data-fields` plan, open items 8 and 9 with it.
- **None of the three heartbeat thresholds is visible to the corpus.** The acceptance harness
  can only tick a whole `HeartBtInt` at a time, so any test-request threshold in (1×, 2×] and any
  timeout in (2×, 3×] reproduces `6_SendTestRequest.def` exactly. The numbers 1.0, 1.2 and 2.4
  are QuickFIX's, and `crates/session/tests/heartbeat.rs` is the only thing holding them.
- **Nothing about a *second* gap is visible to the corpus.** Every file that opens one ends before
  opening another, and the deepest any of them holds is two messages. Closing a filled gap,
  replaying held messages in sequence order, and dropping one there is no room for are all held by
  `crates/session/tests/resend.rs` alone.
- ~~**No application message has ever been replayed.**~~ — **FALSE, and it is the ninth.**
  `[measured 2026-09-01]` `cargo test -p fixbolt-engine --test journal` → **7 passed, 0 failed**,
  and `a_replay_says_when_it_is_being_sent_and_when_it_first_was` does exactly what this bullet
  says has never happened: it feeds a real `35=D` from `8_OnlyApplicationMessages.def`, takes the
  echo, sends a `ResendRequest`, and asserts **one replay and not a gap fill** — at the number it
  was sent with, carrying `43=Y`, a fresh `52=` and the original as `122=`. Three more tests in
  the same file replay application messages under the other D7 policies. The discriminator exists
  too: `none_keeps_nothing_and_fills_over_everything` is the arm that *does* answer with a gap
  fill, so the two outcomes are told apart rather than assumed.
- **Whether a Reject consumes the inbound sequence number is invisible to the corpus.** The
  *too high* branch does not exist yet, so a message running ahead is read as if it were in
  order and a sequence number that never advanced looks exactly like one that did. Held by
  `crates/session/tests/reject.rs`.
- **What each of the 23 field types accepts is invented, not captured.** The corpus supplies two
  cases — `38=+200.00` and `126=20040415`. The other 21 types are held by hand-written rows in
  `crates/dict/tests/field_types.rs`, and that is the weakest evidence in this crate.
- ~~**32 of the 59 definitions still fail.**~~ — **59 / 59 since 2026-08-29**, in process, and
  `[measured 2026-08-30]` through a real socket, and in `standard` mode, and `[measured
  2026-09-01]` through two shards. This bullet described the session layer mid-build and was
  three days stale when it was found.
- ~~**`Input::Tick` is sent but never advances.**~~ — **closed at step 4 of the session plan,
  2026-08-29.** Time advances and `4a_NoDataSentDuringHeartBtInt.def` passes. What remains true
  is the *heartbeat thresholds* bullet above: the corpus can only tick a whole `HeartBtInt` at
  a time, so it still cannot see which of the three thresholds is in force.
- **The 120-second `SendingTime` skew is QuickFIX's documented default, not a measured one.**
  `1d_InvalidLogonBadSendingTime` is 2001 years out, so nothing in the corpus distinguishes 120
  seconds from any other bound.
- ~~**`Role` is parameterised and the initiator is still barely exercised.**~~ — **FALSE since
  2026-09-02, and it was the eleventh.** This bullet said *"ADR-0004's cost is not paid until an
  initiator has been driven against `libquickfix`, and nothing here has been"*. It has been:
  `scripts/interop.sh` drives it through seven steps against a real one, **7 / 7**, blocking in
  CI ([ADR-0042](docs/decisions/ADR-0042-a-second-implementation-is-the-only-independent-opinion.md)),
  and it found a defect six green gates could not see. The initiator also has six things an
  operator can order it to say, and `connect_and_serve` with a reconnect policy
  ([ADR-0043](docs/decisions/ADR-0043-backoff-without-jitter-and-a-reconnect-asks-recovery-every-time.md)).
  The mirrored gate is **10 / 50**, not paused, and **ADR-0006's ceiling of 45 is itself now in
  doubt** — item 36.

  ~~What is still true, and is the part worth keeping: **no corpus covers reconnect**, so every
  test of `connect_and_serve` is this project's own reading — item 38.~~ — **closed 2026-09-05.**
  No *corpus* covers reconnect and none ever will, but a **counterparty** does now:
  `scripts/interop.sh` kills a `libquickfix` acceptor and restarts it on the same `FileStore`,
  twice, and reads its transcripts. `[đo 2026-09-05]` **5 / 5** after a `SIGKILL` and **3 / 3**
  after a `SIGTERM`. Item 38. **`[measured 2026-09-05]`, later the same day: 5 / 5 + 5 / 5 + 5 / 5**
  — the `SIGTERM` scenario asserts continuation like the other two now that item 48 is closed
  (ADR-0053), and a third scenario at `HeartBtInt=1` removes the fixture condition the first two
  were passing under. **On a development machine only; no CI run is named for the commit that
  changed it**, which is the honest state of that number until one is.
- ~~**Sequence numbers reset on every connect.**~~ — **closed 2026-08-31.**
  [ADR-0010](docs/decisions/ADR-0010-a-reconnect-is-not-a-restart.md) is implemented:
  `Session::resume(cfg, next_out, next_in)` carries the numbers, `Session::new` still resets,
  and `141=Y` resets deliberately. **The unproven half survives and is worth keeping**: nothing
  in the corpus *requires* persistence, so the 59/59 does not prove the policy is right for a
  real deployment — what proves the corpus can see it at all is that forcing `connect` to never
  reset drops the score to **56/59**.
- **The single-logon rule's *discrimination* is invisible to the corpus, and this is the tenth
  bullet of its kind.** `1b_DuplicateIdentity.def` and `AlreadyLoggedOn.def` both connect twice
  **as the same counterparty**, so neither can tell *"this identity is already logged on"* from
  *"somebody is logged on"*. `[measured 2026-09-01]` the engine implemented the second for
  three days and scored 59/59 throughout; only a second counterparty makes the two different,
  and `crates/engine/tests/registry.rs::a_duplicate_of_one_counterparty_is_refused_and_the_other_is_not`
  is the only thing holding it — [ADR-0030](docs/decisions/ADR-0030-one-engine-holds-many-counterparties.md).
  **The corpus catches deletion of the rule; only that test catches the failure to compare.** `[2026-09-02]` a second test now holds part of it from the other side —
  `crates/engine/tests/events.rs::a_duplicate_identity_says_so_rather_than_blaming_the_socket`
  asserts the refusal is reported as `DuplicateIdentity` and not as a transport fault. It
  covers the *reporting*, not the comparison; the sentence above still stands for the rule
  itself.
- **What a turn costs while somebody *is* watching has never been measured.** `[2026-09-01]`
  the *allocation* half is proven — `benches/alloc.rs` cases `observe-idle` and
  `observe-asked` both read **0**, the second asking on every one of ten thousand turns. The
  *nanosecond* half is not: [ADR-0032](docs/decisions/ADR-0032-observation-is-a-snapshot-taken-on-request.md)
  claims the idle cost is one relaxed load, and the reversal that would price the other side —
  make `publish` unconditional and watch `benches/turn.rs` slow down measurably — **needs the
  §9 machine and was not run**. Everything built for `observe` was built on a Mac. `[2026-09-02]`
  **the event path inherits the whole gap and adds to it**: `events-idle` and `events-busy`
  read 0 allocations, and what a `try_lock` plus a ring write costs in nanoseconds on a turn
  that logs a session on is not measured either — and unlike the snapshot, that cost is paid
  whether or not anybody ever calls `events()`.
- **Nothing in the corpus can see the measured clock skew, in either direction.**
  `1d_InvalidLogonBadSendingTime.def` is 2001 years out, so every sign convention and every
  bound reproduces it identically — the same blind spot this file already records for the
  120-second default. `Session::last_skew_ms` is held by `crates/session/tests/skew.rs` alone,
  and `[measured 2026-09-01]` **the engine-level test cannot substitute**: the corpus's own
  instant is what its engine's clock reads, so flipping the sign leaves
  `crates/engine/tests/observe.rs` green at `Some(0)` and turns three tests in `skew.rs` red.
- **Ring depth and pending-set occupancy are not in the snapshot**, though open item 30 (b)
  asked for both. What an operator can see today is per-session state, both sequence numbers,
  whether output is backed up, and the skew — not how full the application ring is, which is
  the number that predicts an ADR-0011 disconnect before it happens.
- **No schedule rule is visible to the acceptance corpus.** `[2026-09-02]` all 59 definitions
  run inside one interval, so the primary gate cannot tell a working `Schedule` from one that
  is never consulted — proven from the other side: forcing `same_session` to `true`
  unconditionally turns five tests in `crates/session/tests/schedule.rs` red and leaves
  **59/59 green**. That is what makes `Schedule::always()` demonstrably neutral, and it is
  also what makes this file the only thing holding every rule in
  [ADR-0033](docs/decisions/ADR-0033-a-schedule-is-utc-arithmetic-and-the-calendar-stays-outside.md).
- **`try_lock`-never-`lock` on the event path is read from the code, not proven by a test.**
  `[2026-09-02]` the plan's reversal 3 — swap the `try_lock` in `observe::Events::push` for a
  blocking `lock` — turns **no test red**, because a contended blocking acquisition needs a
  scheduler this suite does not control. Non-negotiable 4's hot-path rule rests here on
  inspection. The same gap already exists for the snapshot's `try_lock` and is the reason
  `Observer::published()` was added; the event ring has no equivalent counter, because a
  refused push and a full ring both land in `events_lost` and cannot be told apart.
- **`EVENT_CAPACITY = 256` has no measurement behind it.** `[2026-09-02]` it is one ring for a
  once-per-connection event, and nothing run here says a mass reconnect does not overflow it —
  which is precisely the moment an operator needs the stream. `events_lost()` reports the
  overflow honestly; what is unproven is that the number is large enough to make the report
  rare.
- **The event stream carries three kinds, and the plan named six.** `[2026-09-02]` gap
  detected, resend issued and reject sent are **not** implemented. They are message-rate rather
  than connection-rate, and the cost of recording anything message-rate has not been measured,
  so D8 keeps them out until it is. `GUIDE.md` §8a says so to the reader as well.
- **The activity mark is written at two moments and neither is periodic.** `[2026-09-02]` the
  engine records when a session was last alive at **logon** and at an **ordered shutdown**. A
  process killed between the two reports the logon instant, which after a long session may be a
  whole day stale — and a killed process is exactly when the instant is consulted. A periodic
  mark needs a frequency, and a frequency needs a measurement nobody has taken.
- **Nothing stops two processes opening the same journal file.** `[2026-09-02]` both append,
  the records interleave, and the result is undefined. There is no lock, and nothing detects it
  afterwards — a `Reader` would show a plausible file.
- **A journal file carries no marker saying what its timestamps mean.** They are D13's scale
  (milliseconds since 0000-01-01) and nothing in the file says so, so a reader that assumes the
  Unix epoch gets a plausible wrong date rather than an error.
- **`serve_sharded_hft` cannot be stopped.** `[2026-09-02]` `run`, `serve` and `serve_hft`
  return a `Shutdown`; the sharded entry point was **not touched**, because it is Linux-only
  and could not be run on the machine this was built on. A sharded deployment still has no
  ordered shutdown — the second thing that entry point is missing, after recovery.
- **The `SIGPIPE` shutdown test asserts the weaker thing.** `[2026-09-02]` the Rust runtime
  sets `SIG_IGN` before `main`, so an ordinary test cannot observe the bug that killed a
  process on 2026-08-30. What
  `crates/engine/tests/shutdown.rs::shutting_down_and_dropping_with_a_live_handle_is_survivable`
  proves is that the sequence completes and the wake afterwards is not an error — **not** that a
  host with default signal handling survives it.
- **Nothing stops accepting during a shutdown.** `[2026-09-02]` `pump` keeps admitting sockets
  until the engine reports finished, and they are dropped rather than told anything. A
  counterparty that connects during the grace period sees a socket open and close in silence.
- **The application is not consulted by a shutdown.** `[2026-09-02]` there is no *"let the
  dispatcher drain"* phase, so a shutdown with an out-of-band dispatcher can discard work the
  application had already accepted. D10 has a policy for a full ring and there is no equivalent
  here.
- **The journal is not explicitly flushed by a shutdown.** `Durability::Async` joins its writer
  on `Drop`, so durability depends on the engine being dropped before the process exits — a
  sequencing rule the type system does not enforce. It is named in `GUIDE.md` and guarded by
  nothing.
- **A journal larger than memory cannot be read at all.** `[2026-09-02]` `journal::Reader`
  loads the whole file, and `tools/jrnl` with it. Nothing has hit the limit and nothing measures
  where it is; a streaming reader is the fix and was not built.
- **Reading a journal the engine is still appending to is undefined.** `[2026-09-02]` the reader
  will see a consistent prefix and probably report a torn tail. **That case has no test**, and
  the tool is used at exactly the moment an engine is most likely to be running.
- **Nothing correlates a journal file to a counterparty.** One file per session is a convention
  the file itself does not record, so *which* journal answers a question is knowledge that lives
  outside the journal.
- **`jrnl` searches by sequence number only.** Finding a `ClOrdID` means `grep` over the dump,
  which is the right answer for a small file and a poor one for a large journal.
- **Nothing authenticates the holder of an `Admin`, and nothing can.** `[2026-09-02]`
  `Engine::admin()` hands out the power to reset a live session's sequence numbers, and the
  engine has no idea who is on the phone. Capability separation — `Observer` for everything
  that watches, `Admin` only for what administers — is the whole of the access control, and it
  is enforced by whoever passes the handle around rather than by anything here. `GUIDE.md` §8a
  says so to the reader.
- **`SetNextOut` is a loaded gun and the type system cannot say so.** `[2026-09-02]` it is
  documented as a lie until the counterparty is told, and nothing prevents it being reached for
  where `SendSequenceReset` was meant. The failure does not show up here — it shows up as the
  counterparty's `ResendRequest`, at which point the session is already wrong. **A reset
  downwards has no guard at all**, deliberately, and is still a foot-gun.
- **The order rule for commands rests on one test.** `[measured 2026-09-02]` a command must be
  applied before a turn numbers anything, and moving `administer()` to the end of `turn()`
  turns exactly two tests red. Nothing structural prevents a later refactor doing it, and the
  59 definitions are blind to it — no acceptance definition administers anything.
- **`COMMAND_CAPACITY = 32` has no measurement behind it**, the same admission
  `EVENT_CAPACITY` carries. It is sized for a person rather than for a loop, which is an
  argument and not a number.
- **An outcome cannot be correlated to the submission that caused it.** `[2026-09-02]` two
  identical commands produce two identical `EventKind::Administered` events; there is no command
  id. An operator submitting one at a time is fine, and one submitting in a loop cannot tell
  which answer is whose.
- **A sharded deployment still cannot resume, and no deployment can resume onto a journal
  that is on disk.** `[2026-09-02]` `serve_with_recovery` and `serve_hft_with_recovery` exist;
  `serve_sharded_hft` has no variant, and `pump` fixes the journal type as `journal::Store`, so
  `FileJournal` per counterparty is unreachable through the serving loop. `STATUS.md` item 32.
- **What the outbound mark costs a turn has not been measured.** `[2026-09-05]` ADR-0053 puts a
  `Journal::mark_out` on every path out of `Session::received_with`, `tick_with` and
  `send_application` — a comparison when the count has not moved, and one record when it has.
  What **is** proven: it allocates nothing (`benches/alloc.rs` `mark-out-mem 0`,
  `mark-out-file-async 0`, proven by reversal at `10000` each, and CI's bench job read
  `invariant failures 0` across 16 of 16 targets on its own CPU); and `tools/w2w --path app` on
  the CI runner printed `allocs 0 (both threads, the timed window only)` with the mark live on
  the path, which is the binary-level gate `benches/alloc.rs` cannot see.
  **What is not: the cost.** `benches/turn.rs` ran in CI and is one of the 45 cases listed under
  `cases w/o a baseline` — *"measured and printed but compared against nothing"* — so no number
  for it exists on a machine that has a baseline. This closed on a container, not the §9 desktop.
  Under `Durability::Fsync` the cost is a `sync_data` per administrative message, which is
  ADR-0017's inbound price arriving outbound — argued, not timed.
- ~~**`scripts/check-no-kernel-sleep.sh` has not run against the outbound mark.**~~ — **FALSE
  within the hour it was written.** `[measured 2026-09-05]` it ran in CI on `3d16527`, job
  [`101302253539`](https://github.com/tmthang86/fixbolt/actions/runs/33964556891/job/101302253539),
  **both halves**, and the log was read rather than its conclusion: the `hft` half made
  `accept4`, `recvfrom` and `sendto` and **no `poll`, no futex** — `GREEN ok — engine thread made
  no blocking call; it did make socket calls` — and the `standard` half tripped it, `RED ok —
  --mode standard trips it: 6 poll`, which is what says the check can still fall. The bullet was
  written because *this container* has no `bpftrace`, and that was mistaken for the gate not
  running at all: **CI is a machine too.**
- **`Recovery::recover` runs on the acceptor thread and nothing bounds how long it takes.**
  ADR-0020 allows that thread to block, which is what makes reading a file legal there — and a
  slow implementation delays every connection queued behind it, with the pending deadline as
  the only backstop and no way for anyone to learn why a socket was refused.
- **Nothing persists `last_active_ms`.** `Session::last_active_ms()` is what a caller saves and
  no journal field holds it, so a deployment that wants ADR-0033's boundary reset across a
  restart must keep that instant somewhere of its own.
- **`crates/engine/tests/recovery.rs` contains zero occurrences of `Engine`**, and that is how
  item 16 closed in 2026-08-31 believing recovery worked while no engine could reach it.
  `crates/engine/tests/engine_recovery.rs` exists because of it and **every test in that file
  builds an `Engine` and goes through its public API** — a rule a `grep` can check but only the
  file's own construction can enforce.
- **The weekday constant rests on exactly one unit test.** `[measured 2026-09-02]` changing
  `+ 5` to `+ 6` in `Weekday::from_days_since_year_zero` leaves **every** weekday case in
  `crates/session/tests/schedule.rs` green, because those tests find a Monday by probing seven
  days rather than naming one — deliberately, so they do not depend on which day the corpus
  falls on, and the price of that independence is that they cannot see the constant at all.
  `schedule::tests::the_weekday_offset_is_derived_not_recalled` is the whole guard.
- **A `Schedule` in a DST zone is wrong for half the year and nothing can detect it.**
  `with_utc_offset_ms` is a fixed offset. A venue on `America/New_York` is `-5h` in winter and
  `-4h` in summer; whoever deploys must rebuild the schedule twice a year, and `GUIDE.md` §5a
  is the only thing that says so.
- **No counterparty has ever been added to a *running* acceptor.** `[2026-09-02]` reading them
  from a file is done — `engine::settings`,
  [ADR-0040](docs/decisions/ADR-0040-a-configuration-file-refuses-what-it-does-not-understand.md)
  — but the table is still read-only after startup, so a new counterparty costs a restart.
  ADR-0026 made `Registry` a trait so a hot-reloading implementation is possible; **nothing has
  written one**, and *"hot reload stays possible"* in that ADR's Consequences is still an
  argument rather than a demonstration.
- **A configuration file has never been read by anything but a test.** `[2026-09-02]` no binary
  in this repository takes a `--config` path: `tools/w2w` builds its table in code. The parser
  is exercised end to end through `serve` and real sockets, which is the property that matters,
  but *an operator edited a file and the acceptor came up* has not happened.
- **Two ways to describe a counterparty now exist**, `Table::serving` and a settings file, and
  nothing reconciles them. A deployment may use both, and two ways to say one thing is the
  shape that eventually disagrees.
- The ADRs are accepted on the strength of the reasoning in them, **not on measurement** — see the §8 caveat above.

- ~~**The acceptor has never been driven by another implementation.**~~ — **CLOSED
  2026-09-04**, and struck in the same commit as the code, which is the rule §4's last row
  exists for. `scripts/interop.sh` now drives a `libquickfix` `SocketInitiator` into
  `fixbolt::serve` and scores **7 / 7**, in the same blocking CI job as the other direction.
  `docs/CONFORMANCE.md` §7. **What is still not proven and is not this bullet**: it is 7 cases,
  not a second corpus; `hft` mode has no interop gate; one counterparty, no TLS, no schedule,
  no shards. And every other acceptor result on this page — 59 / 59 in process, over a socket,
  in `standard`, through two shards — is still this repository's runner reading QuickFIX's
  definitions.
- ~~**A resend that reaches past the journal ring is silent.**~~ — **CLOSED 2026-09-04**, and
  struck in the same commit as the code. It is counted (`resend_beyond_journal`, in messages),
  emitted (`EventKind::ResendBeyondJournal { filled, oldest }`), and the ring is 4096 slots
  rather than 8. **What is still not proven**: no deployment has run with a 4096-slot ring
  under load, the `+2 195 456` byte RSS reading is from a laptop and not a §9 machine, and the
  `FileJournal` `Async` ring still drops records in silence when full — the same shape one
  layer down, named in ADR-0046's *Consequences* as remaining debt and deferred to wave C.
- **Nothing records what was received.** `[2026-09-03]` `grep -rn tracing crates/*/src` is
  empty; the journal is outbound-only by design (D7). Item 44.

## Open items

Every one of these is either inside a plan or has a stated reason for not being in one. Every
plan below is **approved**; the topmost is the one in progress.

| Plan | Closes |
|---|---|
| ~~[a-journal-that-knows-the-numbering](docs/plans/2026-09-05-a-journal-that-knows-the-numbering.md)~~ | 48 — **CLOSED 2026-09-05**, all five steps, [ADR-0053](docs/decisions/ADR-0053-the-journal-answers-two-questions-and-the-second-is-a-number.md). `Journal::mark_out` / `highest_out` as a high-water mark, `Resumed::from_journal`, a fourth record shape on disk, three `_with` twins. `scripts/interop.sh` reads **7 / 7 + 7 / 7 + 5 / 5 + 5 / 5 + 5 / 5** — `known_gap` deleted and the `SIGTERM` scenario asserting continuation like the other two, plus a third scenario at `HeartBtInt=1` that removes the fixture condition the first two were green under. `cargo test --all` 507 → 519. **Revised four times, every revision written before the code moved**, and **the fifth step produced work for the second**: the first green interop run read `no_resend FAIL 35=2: 1` — the inbound direction had the same hole on the same `return`, and nothing in this repository's own tests could see it. **Not done: `benches/turn.rs` was not re-measured** (this is not the §9 box) and `check-no-kernel-sleep.sh` did not run |
| [handles-through-the-front-door](docs/plans/2026-09-05-handles-through-the-front-door.md) | 47 — **Chờ duyệt 2026-09-05**, not started. `fixbolt::Handles` built before the engine and adopted by it; the ten `serve*` / `connect_and_serve*` signatures gain one parameter, `dial` stops draining the user's event ring, and `serve` returns through `Admin::shutdown` for the first time — which `GETTING-STARTED.md:186` has promised since 2026-09-03 |
| ~~[the-baselines-and-the-pass-nobody-timed](docs/plans/2026-09-02-the-baselines-and-the-pass-nobody-timed.md)~~ | 41 → 39 → 34 — **CLOSED 2026-09-05, all three steps, all on the §9 desktop in one day.** Step 1 (item 41): ADR-0049, `check-bench-alignment.sh`, 24 baselines re-recorded — PR [#40](https://github.com/tmthang86/fixbolt/pull/40), merge `32f416e`. Step 2 (item 39): ADR-0050, `crates/session/benches/validate.rs`, §8 gains a row and a remainder (item 49), item 50 found and closed — PR [#41](https://github.com/tmthang86/fixbolt/pull/41), merge `21104d8`, CI green on the merge itself, run [`33944576168`](https://github.com/tmthang86/fixbolt/actions/runs/33944576168). Step 3 (item 34): ADR-0051 — the ratio the item rested on divided by a number that had no benchmark; 3.4× on §9, closed as accepted. **Revised four times, every revision written before the code moved.** The one plan that required this machine, and the machine can go back to normal after it |
| ~~[reconnect-interop](docs/plans/2026-09-05-reconnect-interop.md)~~ | 38 — **CLOSED 2026-09-05**, all six steps, and **revised three times during the build, every revision written before the code moved**. Sửa 1: `Application::on_message`'s `seq` is the number a *reply* would spend, not the one that arrived, so two News at `34=2` and `34=3` both logged `delivered 34=2`. Sửa 2: the journal holds no administrative message, so `next_out` came back as 1 and the counterparty refused it — which became item 48. Sửa 3: the `SIGTERM` scenario asserts three things instead of five, because two of the five cannot be true while item 48 stands, and one of the three pins item 48's size. No file under `crates/` changed |
| ~~[an-engine-that-can-speak-first](docs/plans/2026-09-05-an-engine-that-can-speak-first.md)~~ | 46 — **CLOSED AND MERGED 2026-09-05** (`52a2895`), all five steps, ADR-0048, `DESIGN.md` D15. **Moved ahead of wave B** by item 45 (a). Five steps: ADR-0048, then `Outbox` + `Handler::on_logon`, then `Sender`, then the interop acceptor role sending two unprompted `35=B`, then the alloc cases and the sync table. Touches `engine` and `library`; **`session` is untouched**, because `Session::send_application` is already the right hole |
| ~~[acceptor-interop](docs/plans/2026-09-03-acceptor-interop.md)~~ | 42 — **CLOSED 2026-09-04**, steps 1–4 of 5. Both directions in one script, **7 / 7 + 7 / 7**, blocking in CI. Revised twice during the build, both times recorded in the plan before the code changed. Step 5 (item 38) not done |
| ~~[resend-from-the-journal](docs/plans/2026-09-03-resend-from-the-journal.md)~~ | 43 — **CLOSED 2026-09-04**, six steps, ADR-0046 accepted. Ring 4096 and O(1), two counters, two events, replay in batches. `cargo test --all` 446 → 463. CI run [`33850311124`](https://github.com/tmthang86/fixbolt/actions/runs/33850311124), 11 / 11 |
| ~~[message-log](docs/plans/2026-09-03-message-log.md)~~ | 44 — **CLOSED 2026-09-04**, all five steps, CI green 12 / 12 on `0e0b5f3`, run [`33861215461`](https://github.com/tmthang86/fixbolt/actions/runs/33861215461). `MessageLog` trait in `engine`, `FileLog` on the journal's ring→writer pattern, both directions **including refused frames**, `FileLogPath` in the settings file; step 4 (journal CRC32, format v1) separable |
| [settings-for-both-roles](docs/plans/2026-09-04-settings-for-both-roles.md) · [seq-resync-789-369](docs/plans/2026-09-04-seq-resync-789-369.md) · [timestamp-micros](docs/plans/2026-09-04-timestamp-micros.md) · [tls](docs/plans/2026-09-04-tls.md) | 45, wave B — **Draft 2026-09-04**, re-verified and revised before approval; see item 45 |
| [the-second-linux-desk](docs/plans/2026-09-04-the-second-linux-desk.md) | 45, wave C, and items 39, 40, 41 — **Draft 2026-09-04**; needs the §9 desktop and a second machine with a NIC |
| [const-templates-in-dict](docs/plans/2026-09-04-const-templates-in-dict.md) · [deterministic-simulation](docs/plans/2026-09-04-deterministic-simulation.md) | 45, wave D, and item 34 — **Draft 2026-09-04**; both before phase 2's encoding ADR |
| ~~[the-initiator-and-its-second-opinion](docs/plans/2026-09-02-the-initiator-and-its-second-opinion.md)~~ | **STEPS 1–3 CLOSED 2026-09-02** — **phase 1 exit criterion 4**, [ADR-0042](docs/decisions/ADR-0042-a-second-implementation-is-the-only-independent-opinion.md). Merged PR [#30](https://github.com/tmthang86/fixbolt/pull/30), CI green on `cdd6fba`, runs [`33623429649`](https://github.com/tmthang86/fixbolt/actions/runs/33623429649) and [`33623385882`](https://github.com/tmthang86/fixbolt/actions/runs/33623385882), 10 checks of 10. Three origination functions, an interop gate against a real `libquickfix` at 7 / 7, and a blocking CI job. **Its first run found a defect six green gates could not see**, and its second reversal was a no-op that exposed a hole in the gate itself — two `docs/reference/` entries, both `[to testing-skills]`. **Step 4 (the mirrored gate at 45 / 50) is not done** and became open item 36 |
| ~~[the-doc-set](docs/plans/2026-09-02-the-doc-set.md)~~ | 33 — **CLOSED AND MERGED 2026-09-03**, phases A, E, B, D; **C deferred to phase 2's first ADR**. PR [#32](https://github.com/tmthang86/fixbolt/pull/32), merge `16ad9b6`, CI green on `86c3913`, run [`33742995012`](https://github.com/tmthang86/fixbolt/actions/runs/33742995012), 11 jobs of 11. **B6 was the one step that found a defect rather than a gap**: `cargo doc` under `-D rustdoc::broken_intra_doc_links` was red with 13 pre-existing warnings, six of them public docs linking to private items; all fixed, and the `docs` CI job now holds them, proven by reversal. **PHASE A CLOSED 2026-09-02**: item 33 registered, `README.md:23` corrected, `GUIDE.md:331`'s fence closed, and a **Where to start** section written — the bootstrap was not recorded anywhere a newcomer would look. CI green on the commit closed, `37cb3c5`, run [`33608929249`](https://github.com/tmthang86/fixbolt/actions/runs/33608929249), **9 jobs of 9**. **Written twice**: the first draft was measured against `73e48c6`, and 73 commits had landed by the time it was written down — `GUIDE.md` had already grown 501 → 1054 lines and absorbed operations, so `OPERATIONS.md` was dropped before it was written. Sửa 1 in the plan records it. **Sửa 3, 2026-09-03, owner-reviewed**: phase C (`docs/internals/`) is deferred until phase 2's first ADR decides whether `MessageView` generalises — those pages would describe exactly what that ADR may change; E gains `#![warn(missing_docs)]` on `library` (`[measured 2026-09-03]` **0** missing today, so it is a ratchet, not work) and crate docs served from `crates/library/README.md` so its snippet compiles under `cargo test -p fixbolt --doc`; order is A → E → B → D; **10 files, not 17**. Hosting stays `docs/`: the repository is private, Pages there needs GitHub Pro and publishes publicly; mdBook can read the directory as-is later, so new files avoid GitHub-only syntax |
| ~~[a-registry-from-a-file](docs/plans/2026-09-02-a-registry-from-a-file.md)~~ | **CLOSED 2026-09-02, all four steps** — `PRD.md`'s last open line under `many counterparties`, [ADR-0040](docs/decisions/ADR-0040-a-configuration-file-refuses-what-it-does-not-understand.md). Merged PR [#27](https://github.com/tmthang86/fixbolt/pull/27), CI green on `826b672`, runs [`33602482251`](https://github.com/tmthang86/fixbolt/actions/runs/33602482251) and [`33602486701`](https://github.com/tmthang86/fixbolt/actions/runs/33602486701). `engine::settings` reads a QuickFIX-shaped INI with no new dependency; 34 tests, five reversals. **The plan's own reversal 2 was wrong** — it predicted one test would stay green and nine went red, because every fixture inherits its required keys; 2b is the half that discriminates. And a reversal *not* in the plan found that all three wire tests were blind to which of two silences they were seeing |
| ~~[recovery-reaches-the-disk](docs/plans/2026-09-02-recovery-reaches-the-disk.md)~~ | **CLOSED 2026-09-02** — item 32 (b) and (c), [ADR-0039](docs/decisions/ADR-0039-a-fresh-journal-is-the-deployments-to-build.md). Merged PR [#26](https://github.com/tmthang86/fixbolt/pull/26), CI green on `88f4473`, runs [`33600110468`](https://github.com/tmthang86/fixbolt/actions/runs/33600110468) and [`33600141705`](https://github.com/tmthang86/fixbolt/actions/runs/33600141705) |
| ~~[gates-that-can-be-trusted](docs/plans/2026-08-30-gates-that-can-be-trusted.md)~~ | **CLOSED 2026-08-30** — 7, 17, 18, 19 |
| ~~[w2w-and-linux-numbers](docs/plans/2026-08-30-w2w-and-linux-numbers.md)~~ | **CLOSED** — 15 on 2026-08-30, 11 on 2026-08-31, 13 on 2026-09-01, **6 on 2026-09-02 once the §9 machine existed**; 12 decided by ADR-0045. Its own delivery log: *nothing of this plan is left* |
| ~~[threads-and-affinity](docs/plans/2026-08-30-threads-and-affinity.md)~~ | **CLOSED 2026-08-31** — all six steps. Item 21 stays open, narrowed; item 24 is new |
| ~~[pre-session-routing](docs/plans/2026-08-31-pre-session-routing.md)~~ | **CLOSED 2026-09-01** — 24. All six steps, plus [ADR-0020](docs/decisions/ADR-0020-a-pre-session-stage-owns-the-socket-until-logon.md) and [ADR-0022](docs/decisions/ADR-0022-the-pre-session-stage-enforces-two-definitions.md) |
| ~~[what-mitigations-cost](docs/plans/2026-09-01-what-mitigations-cost.md)~~ | **CLOSED 2026-09-01** — 22, and [ADR-0023](docs/decisions/ADR-0023-section-9-records-the-cpu-mitigations.md) gives §9 the row it was missing |
| ~~[release-profile](docs/plans/2026-09-01-release-profile.md)~~ | **CLOSED 2026-09-01** — 13, on *keep the default* ([ADR-0024](docs/decisions/ADR-0024-the-workspace-keeps-the-default-release-profile.md)). It is also what found **25** |
| ~~[ktls-spike](docs/plans/2026-08-30-ktls-spike.md)~~ | **CLOSED 2026-08-31** — 10 |
| ~~[an-engine-can-resume](docs/plans/2026-09-02-an-engine-can-resume.md)~~ | **CLOSED 2026-09-02, all four steps.** Item 31 closed; item 32 is what it did not do. Merged PR #20 (CI [`33588190145`](https://github.com/tmthang86/fixbolt/actions/runs/33588190145) on `38ca25c`) and PR #21 (CI [`33589117703`](https://github.com/tmthang86/fixbolt/actions/runs/33589117703) on `1f10f78`). **PR #21's first push was red** — `serve_with_recovery` unguarded by `#[cfg(feature = "standard")]`, invisible to `cargo test --all --no-default-features` and caught by `scripts/check-no-optional-deps.sh`, exactly as `docs/reference/feature-flags-unify-across-a-workspace.md` predicts |
| ~~[session-schedules](docs/plans/2026-09-02-session-schedules.md)~~ | **STEPS 1–3 CLOSED 2026-09-02, merged PR #19, CI [`33587321972`](https://github.com/tmthang86/fixbolt/actions/runs/33587321972) green on `d00e964`** — `PRD.md`'s Phase-1 gap, named three times and never planned until now. [ADR-0033](docs/decisions/ADR-0033-a-schedule-is-utc-arithmetic-and-the-calendar-stays-outside.md). **Step 4 is item 31** and is not started |
| ~~[operability](docs/plans/2026-09-01-operability.md)~~ | **STEPS 1–2 CLOSED 2026-09-02** — item 30 (b) and (f), [ADR-0032](docs/decisions/ADR-0032-observation-is-a-snapshot-taken-on-request.md). Steps 3–6 (ordered shutdown, sequence-number admin, event stream, journal reader) are **not started**, and the plan said up front that stopping here is a whole result |
| ~~[counterparty-registry](docs/plans/2026-09-01-counterparty-registry.md)~~ | 28 — **CLOSED 2026-09-01**, six steps, ADR-0029 and ADR-0030. Shape decided by [ADR-0026](docs/decisions/ADR-0026-a-counterparty-registry-in-the-pre-session-stage.md). **Chosen to be closable on macOS**: every gate is a test, and the plan names the two things that are not |
| ~~[data-fields](docs/plans/2026-08-30-data-fields.md)~~ | **CLOSED 2026-08-30** — 8, 9 |
| ~~[session-recovery](docs/plans/2026-08-30-session-recovery.md)~~ | 16 — **CLOSED 2026-08-31**, and item 16's row says the closure is narrower than it reads: nothing was reachable from an `Engine` until item 31 |
| ~~[ring-full-policy](docs/plans/2026-08-30-ring-full-policy.md)~~ | 5 — **CLOSED 2026-08-31**, ADR-0011 implemented: a full ring ends the connection with `Logout(58=slow application)` |

**The two with no plan, and why.** **1** (the final name) is a decision for the owner, not a
piece of work — when it is made it is an ADR and a rename, and neither can be planned around
an undecided name. **14** (kernel bypass) is phase 3, is excluded by PRD §5 as it stands, and
needs a Solarflare/AMD X2-class NIC that nobody here has; planning it would be planning
against hardware that does not exist.

| # | Item | Blocks |
|---|---|---|
| 33 | ~~**Two of the three readers have no documentation, and the front door is empty.**~~ — **CLOSED for the-doc-set's scope 2026-09-03** (A, E, B, D); the contributor code-map (`docs/internals/`, phase C) is **deferred to phase 2's first ADR** by Sửa 3, and carries the *engine-has-no-map* part of this item with it. `[measured 2026-09-02, on `ce14417`]` `README.md` has **0 `rust` blocks**, and `grep` finds **no *Installation*, *Getting Started*, *Usage* or *Quickstart* heading** in `README.md` or anywhere under `docs/`; `engine::settings` accepts **10 keys**, of which **`EndDay` and `MaxSkewMillis` appear in no document at all**, and there is **no `Setting / Valid values / Default` table** anywhere; **no page publishes the conformance results** — 59/59, 93 groups, 730/730, 912 tags — though every number is already in this file under **Proven**; and `engine` has **20 modules with no map of them**, against `DESIGN.md`, which records what was decided rather than how the code works today. **Three `hft` entry points are called by no test, no bench and no tool** — `serve_hft`, `serve_hft_with_recovery`, `serve_sharded_hft` — so the mode this project makes its claim about has no gate running through its own front doors. **The outside view is the argument for doing this at all**: across ~20 Rust FIX crates there is **no configuration reference, no acceptor guide, no published conformance result and no performance figure with a method** — and the C++ binding `quickfix` has **~345k downloads, ~3.4× every pure-Rust engine combined**, while `fefix` has 101,866 downloads, **no release since 2021**, and is still the first page a newcomer reads. Planned by [the-doc-set](docs/plans/2026-09-02-the-doc-set.md), five phases, **of which C is deferred to phase 2 by Sửa 3 (2026-09-03)**. **Phase A closed 2026-09-02** — this item registered, `README.md`'s `standard` line corrected, a broken fence in `GUIDE.md` closed, and a **Where to start** section written, because the bootstrap was recorded nowhere a newcomer would look. **Revised twice, and both times by the repository moving underneath it**: the first draft was measured against `73e48c6` and 73 commits landed before it was written down, which cost it `OPERATIONS.md`; the second was measured against `89c785b` and by the time phase A was pushed `library` existed with a worked example, which **unblocked phase E and made two of this item's own findings false** — `examples/` and the coverage of `serve`. `[to testing-skills]`: **an inventory of what is missing is a measurement with a shelf life, and it expires silently** — nothing failed, the numbers simply stopped being true, and only re-running the commands showed it. | the front door, and every contributor after the first |
| 1 | ~~**Final name**~~ — **closed 2026-08-30: `fixbolt`.** The placeholder `nanofixengine` was not merely undecided, it was **a near-collision with the project this repository measures itself against**: `matthart1983/nanofix` is *"Ultra-low-latency FIX protocol engine in Rust"* (item 12; `CLAUDE.md` §2 rule 7's "276 unwraps"), and `LMAX-Exchange/nanofix` is a Java FIX test client with 40 stars in the same domain. `DESIGN.md` had said so since it was written and the rename kept being deferred. `fixbolt` was checked free on crates.io **and** GitHub — checking only the global registry is how `nanofix` itself nearly got re-adopted. Rejected, with reasons: `rustfix` (taken — the crate behind `cargo fix`, and literally a repair tool), `tesla-fix` (TSLA is a ticker; the engine would be named after an instrument it carries), `swiftfix` (SWIFT is the interbank network), `flashfix` ("flash crash"), `fixarc` (`Arc<T>`), `fleetfix` (37 fleet-repair apps), and the whole `<speed-word>fix` family, because *quick fix* is an English idiom for a shoddy repair — putting FIX **first** is what makes it read as the protocol. **Fully closed 2026-08-30**: the GitHub repository is renamed, `github.com/tmthang86/fixbolt`, verified by `git ls-remote` returning the merge commit through the new URL. `Cargo.toml`'s `repository` field had pointed at `.../fixbolt` since the rename commit and was **wrong until this happened**. GitHub redirects the old URL. The five surviving occurrences of `nanofixengine` in the tree are deliberate history — *"the old name was X"* — not leftovers | Nothing. Publishing to crates.io is unblocked |
| 5 | ~~**Ring-buffer policy when the application behind the ring falls behind**~~ — **CLOSED 2026-08-31.** [ADR-0011](docs/decisions/ADR-0011-a-full-ring-disconnects.md) is **implemented**: a full ring ends the connection with `Logout(58=slow application)` — a different text from D10's `slow consumer` on purpose, because here the counterparty is faultless — `ring::DEFAULT_CAPACITY` is **4 MiB**, and `Block` is not offered on this side. **The ADR's open question 2 is answered too**: the refusal reaches the outside in two places, the `58=` the counterparty reads and `Engine::refused_connections()` for whoever embeds the engine. The signal from dispatch to engine is a defaulted `Dispatch::take_refusal()`, carrying no id because the engine asks straight after one connection's turn — the session layer's `Application` trait could not be touched (non-negotiable 2). Proven by three reversals: engine ignoring the refusal, the Logout carrying D10's text, and `RingDispatch` never reporting; each red, then green. **Open questions 1 and 3 of that ADR stay open** — per-connection rings, and whether the slack is enough — and both need a real application nobody has yet. `[measured 2026-08-31]` **the slack itself was re-measured rather than extrapolated: 5.05–5.36 ms at 4 MiB, not the 3.6 ms four documents were carrying**, because the per-message cost goes 135 → ~230 ns once the buffer leaves cache and a ring that fills slower gives more time, not less. The decision is unaffected and its margin is bigger than it claimed — [measured-costs.md](docs/reference/measured-costs.md). | closed |
| 20 | ~~**The bench ceilings are tuned to a machine no gate runs on.**~~ — **CLOSED 2026-08-31 by [ADR-0016](docs/decisions/ADR-0016-per-machine-baselines-replace-absolute-targets.md)**, which implements this item's own conclusion: *a per-machine baseline is viable, keyed on the CPU model that `check-machine.sh` prints; a single absolute ceiling across the pool is not.* Ceilings are gone; `benches/baselines.tsv` holds one line per (CPU, case) with its margin, sample size, date and machine verdict, and an unknown CPU reads **NO BASELINE**, counted on its own summary row and fatal under `--strict`. Two of this item's findings were **re-measured and both held**: the unexplained second mode on `ring, one way` is still there at **+24%**, 2 runs in 21 with the old harness and 2 in 30 with the new — and `[measured 2026-08-31]` it appeared on the **quietest** runs of the set, not the loaded ones, which confirms from the other side that load is *sufficient* to produce the mode and not *necessary*. **What was wrong was the instrument, not this item**: dropping one parameter from `Suite::bench` moved `inline deliver + reply` 6.3 → **1.3 ns** with the timed loop byte-identical, and the three discrete clusters that case showed were the harness's, not the engine's — see [measured-costs.md](docs/reference/measured-costs.md). | closed |
| 22 | ~~**The engine is syscall-bound**~~ — **CLOSED 2026-09-01, and the last lever was the largest.** `[measured 2026-09-01]` **the CPU speculation mitigations cost 59–63% of every syscall this engine performs**: `engine turn, 1 idle sessions` **448.9 → 175.2 ns**, `recv on a quiet socket` 420.5 → 156.9, bare `getpid` 154.5 → 59.45, while **thirteen pure user-space benchmarks moved −4.1% to +4.1% with no direction** and `user_loop` agreed to 0.1% across all three boots. The tail follows too: p50 216 → 80 ns, **p99.99 2848 → 88**. **And the mechanism this item had named since 2026-08-30 was wrong.** It said *"`vmscape` alone does an IBPB on every syscall return"*; arm B turned off exactly `vmscape` and **nothing moved** — `getpid` read 154.5 ns unchanged to the digit, 0.5–1.1% across the benches. Arm C turned off `retbleed`'s untrained return thunk and `spec_rstack_overflow`'s Safe RET, **put `vmscape`'s IBPB back on**, left the retpolines on, and **recovered the whole of it to within 0.5% on every row**. It also explains what the first arm could not: the saving was unequal — `getpid` 95 ns, `recv` 264 — because a return thunk is not a fixed per-syscall cost but work on **every return inside the kernel**, so it scales with how much kernel code a syscall runs. **That mismatch was evidence for the right mechanism and against the named one, and nobody read it that way until the arm that could settle it ran.** `[to testing-skills]`: **turning the exact knob you named and watching nothing move is the cheapest way to learn the mechanism you named is wrong** — the cheaper face of *a cause accepted because a knob moved with it*, and the one people skip, because a named mechanism that explains the data feels finished. **The finding that changed a document is not the 61%**: `check-machine.sh` read **`pass 11 fail 0`** in **all three arms**, including the one with every mitigation disabled — §9 had no row for them, so two machines could both be called publishable and differ by 61% on the operation §8 calls dominant. Same hole ADR-0021 closed for `nohz_full`, same checklist, found the same way. [ADR-0023](docs/decisions/ADR-0023-section-9-records-the-cpu-mitigations.md) adds the row: it **PASSes when mitigations are ON**, reads `/sys` rather than `/proc/cmdline`, and is **not** advice to disable them — a machine with them off reads 60% *under* every syscall baseline, which passes, because a baseline is a ceiling. **Not separated**: `retbleed` from `spec_rstack_overflow` — same class of mechanism, two smaller numbers, no different decision. **Turned off beyond its name**: `retbleed=off` also drops `STIBP: always-on`; SMT is off under §9 so it almost certainly costs nothing, and *almost certainly* is not a measurement. Write-up: [measured-costs.md](docs/reference/measured-costs.md). **What is still open here is `recvmmsg`/`io_uring` with `SQPOLL`, and item 14** — the syscall itself, now that its mitigation surcharge is known. The original finding: |
| 22 | `[measured 2026-08-31]` **and then removing it moved the cores that never had it.** After the reboot onto the ADR-0021 §9 line, `cpu5` — which carried **no isolation flag in any of the three boots** — went from **501.8 ns to 455.7 ns** per `Engine::turn`, and from 198.87 ns to **154.62 ns** on a bare `getpid`. **Naming any CPU in `nohz_full` costs ~45 ns per kernel entry on *every* CPU**, on top of the ~155 ns paid by the ones in the list; three independent readings agree on the size — `getpid` +44, `recv` +47, `turn` +46, the shape of one fixed cost per kernel entry — and `user_loop` did not move, so again not the clock. **The four-arm experiment could not have seen this: its own control was inside the tax.** It was found only by re-measuring the baseline arm after the setting was removed everywhere. `[to testing-skills]`: **a setting scoped to a subset can charge the whole system, and the control group is inside the blast radius — after you remove it for good, re-measure the arm you were treating as the baseline.** **Verified by a prediction written before the runs**: only syscall-bound cases should move. 24 qualifying `bench.sh --strict` runs of 25 — the one that did not qualify was the **first after the reboot**, gnome-shell at 29% — gave twelve pure user-space cases moving −1.7% to +4.8% with no direction (`parse` −0.7%, `ring, one way` −1.7%) and all four syscall cases down together: **470.9→420.5, 500.3→448.9, 2002.9→1807.1, 8139.4→7333.5.** Had `parse` moved 10% too, the explanation was wrong. `benches/baselines.tsv` takes the four new figures at n=24, margin 1.10, verdict `pass 11 fail 0 unknown 1`; proven by reversal — setting one row to 400.0 turns **exactly one of four** cases red on its own limit. **`DESIGN.md` §8's dominant row is now 449 ns, was 505**, and the 2026-08-30 figure of 703 ns is explicable as 449 + ~45 global + ~155 per-core + two different programs: **§9's own tuning was about a third of the number this design was budgeted against.** §8, §9, `GUIDE.md`, `PRD.md` and D8's N-crossover (wins at N=1, loses by **N=11**, was N=8) all follow. `check-machine.sh` reads **`pass 11 fail 0 unknown 1`** and `bench.sh --strict` is **OK**, 9/9 targets, 0 over baseline, 0 without one. The original finding: |
| 22 | `[measured 2026-08-31]` **the isolation question is closed: it is `nohz_full`, and it loses until p99.99.** One reboot separated the three options by giving them to three **different** CPUs in the same boot rather than one flag per boot — same kernel, same temperature, same session across the arms. `cpu5` nothing **501.8 ns**, `cpu6` `isolcpus` **494.8 ns**, `cpu7` `rcu_nocbs` **498.2 ns**, `cpu4` `isolcpus`+`nohz_full` **670.7 ns** per `Engine::turn`; on a bare `getpid`, 198.86–199.04 against 352.96–354.76 over four interleaved repetitions. **`isolcpus` and `rcu_nocbs` are free and the `isolcpus`-only core is the fastest of the four.** It is not the clock — a user-space loop that never enters the kernel agrees across all four cores to 0.3% — and it is not interrupts, because the `nohz_full` core takes **3743 fewer** timer interrupts a second and is still 78% slower per kernel entry. **The other half was measured too**, after the plan was revised and re-approved to bring it in: `nohz_full` is worse at p50 (376 vs 216), worse at p99 (376 vs 224) and **worse at p99.9** (384 vs 224), and ahead only from p99.99 (504 vs 2848). The count of calls over 1 µs tracks the local timer interrupt count **call for call** — 1130/1283, 1078/1281, 1120/1281 and **2/2** — so the tail *is* the tick rather than something that moved when a knob did. The arithmetic that decided it: ~2 000 000 kernel entries per second per core, 160 ns taxed on every one, ~1100 excursions of 3 µs removed — **0.32 s against 0.0033 s, a hundred to one against**. [ADR-0021](docs/decisions/ADR-0021-nohz-full-leaves-section-9.md) takes `nohz_full` out of §9 and prices it instead; `scripts/check-machine.sh` **reverses**, now failing a machine for *having* it. **Two near-misses are the more valuable half.** `scaling_cur_freq` read **2 240 000 kHz** for a core at 100% load while its neighbour read 3 792 929 — a tidy 1.69× explanation for a 1.36× slowdown, and wrong: `amd-pstate-epp` updates that value from a path tied to the tick, which `nohz_full` had stopped. The user-space loop refuted it. `[to testing-skills]`: **an instrument that reports a plausible cause can be downstream of the thing under test.** And the guard against a false green was itself one: the tick counter used `awk '/^LOC:/`, `/proc/interrupts` right-aligns its first column, and it printed a delta of **0 for every core** on a boot where two cores ticked three million times. `[to testing-skills]`: **a guard reporting the same value everywhere is reporting nothing, and "all clear" is the disguise it wears.** Write-up: [measured-costs.md](docs/reference/measured-costs.md); tool: `scripts/measure-isolation-cost.{c,sh}`. **Item 22 is narrowed, not closed.** The remaining levers are untouched: `mitigations=off` (**`[unproven]`**, needs its own reboot, and is a security decision rather than a measurement), `recvmmsg`/`io_uring` with `SQPOLL`, and item 14. What is also **not** measured is what `isolcpus` buys **under load** — on a quiet box it removed 1078 excursions against 1130, which is nothing, because there was nothing to remove; it is kept because it is free, and §9 says so rather than implying a benefit. The original finding: |
| 22 | **`[2026-08-30]` decided by [ADR-0012](docs/decisions/ADR-0012-latency-first-and-one-session-per-polling-thread.md), `Accepted`, and **re-scoped to `hft` by [ADR-0013](docs/decisions/ADR-0013-two-modes-standard-and-hft.md)** — in `standard` mode the engine blocks instead of sweeping, so this whole term is replaced by an `epoll`-class wakeup and is **unmeasured**. The tension is resolved in favour of latency: one session per polling thread is the shape the design is optimised, budgeted and measured for; many-per-thread is supported as a labelled **`density`** mode carrying `N × 703 ns` instead of the latency figures; and **every published latency number names its `N`**. `PRD.md` §1, `DESIGN.md` §1 and §8, `README.md` and the new **[GUIDE.md](docs/GUIDE.md)** follow from it. `DESIGN.md` §8 priced busy-poll at **`~0`** and that row is now **703 ns × N**; its bottom line said `< 1 µs` for "everything this design controls" while measuring only user space, and now reads `< 1 µs + N × 703 ns`. **The trade D8 makes — `epoll`'s 2–5 µs wakeup for a 703 ns poll — wins at N=1 and loses by N=8**, a sentence the table did not contain until the poll was measured. What remains open is the measuring, not the deciding. The original finding: |
| 22 | `[measured 2026-08-31]` **the 703 ns is now `Engine::turn` rather than a floor, and the §9 tuning turned out to be part of it.** `crates/engine/benches/turn.rs`: **505 ns per session on an ordinary core**, flat 1 → 16 within 2%, of which **~475 ns is the `recv` syscall and ~30 ns is the engine's own work** — measured in the same run, so the subtraction is not across programs. **And the isolated core §9 recommends is 36% slower**: 680 ns on `cpu6` against 498 ns on `cpu5`, which sits in the *same* L3 domain, so it is isolation and not cache. Which of `isolcpus` / `nohz_full` / `rcu_nocbs` was **not** separated — one kernel command line applied all three — and `nohz_full`'s context tracking on every kernel entry is the named mechanism, labelled a hypothesis. The old 703 ns was a C program pinned to an isolated core; matched for placement the two agree to 4%. **What this does not measure is the jitter isolation buys**, which could easily be worth 175 ns of median — the trade is now stated rather than assumed. `DESIGN.md` §8 and §9 both say so, and [measured-costs.md](docs/reference/measured-costs.md) carries the four arms and the `[to testing-skills]` shape: *a setting adopted for performance, never measured against the operation it changes.* The original finding: |
| 22 | **The engine is syscall-bound, and "many sessions on one core" costs 703 ns each.** `[measured 2026-08-30]` D8 makes an idle turn one non-blocking `read` per connection. On the §9 box that read costs **703 ns and is flat from N=1 to N=256**, so a turn is exactly `N × 703 ns` and a session's added latency is one whole turn. Of that, **353.8 ns is kernel entry and exit doing nothing** — `syscall(getpid)` measured beside it — against `parse NewOrderSingle` at **125.5 ns** and a vDSO `clock_gettime` at **22.9 ns**. *The syscall that discovers there is nothing to parse costs 5.6× the parse.* `DESIGN.md` §8 budgets the whole user-space path under 1 µs; **two sessions on one polling thread exceed that budget in polling alone**, and `PRD.md` targets *"many sessions on one core"*. Both can be true of different products, but **not of one polling thread**, and that choice has not been made. It also reorders the open items: item 12 defers SIMD for 20–40 ns, while removing a syscall is worth **703**. Levers in measured order — fewer sessions per thread (free); `mitigations=off` (**`[unproven]`**, full mitigations are on, `vmscape` alone does an IBPB on every syscall return, needs a reboot and is a security decision); `recvmmsg` or `io_uring` with `SQPOLL`; then item 14. Write-up: [reference/measured-costs.md](docs/reference/measured-costs.md) | `DESIGN.md` §8 budget; `PRD.md`'s deployment shape; the priority of items 11, 12, 14 |
| 21 | `[2026-08-31]` **half closed. Pinning exists; the refusals do not.** Steps 1–2 of [threads-and-affinity](docs/plans/2026-08-30-threads-and-affinity.md) are done: [ADR-0015](docs/decisions/ADR-0015-explicit-cores-pinned-from-inside-and-read-back.md) decided the shape, [ADR-0019](docs/decisions/ADR-0019-two-unsafe-blocks-and-an-error-the-enum-can-hold.md) corrected three things writing the code found, and `fixbolt_engine::affinity` pins a thread and **asks the kernel back** rather than trusting the call. `[measured 2026-08-31]` proven by a **second** reversal, not the first: removing `sched_setaffinity` made the tests red at the read-back guard, which said nothing about whether the residency assertion was worth having — so the read-back was removed too, and the thread was then observed on **cpu0, cpu4 and cpu5** in one run. An unpinned thread really does move. `[2026-08-31]` **step 3 landed too**: `Topology` and `ShardPlan::validate()` refuse a core that is absent, offline, duplicated, an SMT sibling of another in the plan, or — for shard cores — outside `isolcpus`, **before any thread is created**. Proven by reversing all three topology checks at once: exactly 5 of 18 tests went red, and the two *acceptance* tests stayed green, which is what shows the suite is not simply refusing everything. `[2026-08-31]` **step 4's runtime landed too** — `fixbolt_engine::shard`: one pinned thread per shard, each confirming its own pin before any of them serves, connections handed over a channel that `[measured]` makes no syscall and no allocation. **It does not close the step**: running the acceptance corpus through it found that sharding breaks the single-logon rule, which is now open item 24. `[2026-08-31]` **step 5 landed**: `affinity::spawn_pinned` and `FileJournal::open_pinned` — the one thread this crate spawns now has a home and confirms it, and pinning a `Fsync` journal is refused rather than ignored. The `RingDispatch` consumer is the caller's own thread, so it is validated rather than pinned, and the rustdoc says so. `[2026-08-31]` **the plan is closed and this item is narrowed, not closed.** D8's sentence is now true of the **sharded** path: every engine thread pins itself and reads the mask back. It is still false of `serve_hft`, the single-engine entry point, which pins nothing — ADR-0015 decision 1 forbids the engine picking a core and that function takes none. Deliberately not fixed while closing a plan: adding public API outside a closing plan's scope is the drift `CLAUDE.md` §1 exists to stop. `[2026-08-31]` **step 6 landed**: `benches/turn.rs` measures the real `Engine::turn` — **505 ns per session**, of which ~475 ns is the syscall — with baselines from 21 qualifying `bench.sh` runs, and `bench.sh --strict` exits 0. It also found that **§9's isolated core is 36% slower** at that syscall; item 22 carries it. **Still open here**: step 4 cannot close while item 24 stands, and the shard corpus gate's settle bounds are **10 and 50 ms**, raised after a GitHub runner — two vCPUs on one physical core — scored **58/59 at 1 ms**. `[2026-08-31]` **two corrections, and the second is the finding.** I concluded that gate *hung* on CI for 35 minutes, cancelled a run, made the test skip where there is one physical core, and started adding a CI timeout. Then I "corrected" that by blaming a stale GitHub API. **Both were wrong.** The run timestamps: `33393632071` created 12:48:47Z, cancelled 12:50:59Z — **it had run 2 minutes 12 seconds**; `33393962624` created 12:52:43Z, completed 12:53:55Z, success. The API was accurate throughout. **I never read a clock**: elapsed time was inferred from my own sequence of overlapping background waits, and every conclusion downstream inherited the error — including the correction, which accused a system that had done nothing wrong. Both responses are reverted. The 58/59 at 1 ms stands, because it came from a failed run's **log**. **And with the skip removed the gate runs on the CI runner and passes**: run [`33394373832`](https://github.com/tmthang86/fixbolt/actions/runs/33394373832), `7e8ee1f`, 9/9, `one_shard_passes_all_fifty_nine_at_any_settle_bound ... ok`, whole run 2m26s. There was never a hole. `[to testing-skills]`: *elapsed time inferred from your own activity is not a measurement*, and *a self-critical explanation is not thereby a correct one* — the first correction had a mechanism, a right number and an admission of fault, and was still false. `[measured 2026-08-31]` **the SMT-sibling rule fired for real on the first CI run** — a GitHub runner reports `cpu0` and `cpu1` as two threads of one physical core and refused the plan. ADR-0015 had written that this rule can never fire on a §9 machine because SMT is off there, and called the gap between *the reading is tested* and *the reality is not* a real hole; CI closed it on the first try. `DESIGN.md` D8 now says exactly this rather than claiming the whole sentence. **Two facts the machine changed while reading it**: `isolated` lists `6-7,14-15` while `online` is `0-7`, so a validator reading `isolated` alone would accept a core that cannot run anything; and §9 turns SMT off, so the SMT-sibling rule can never fire on a correctly tuned box — it fires on the one set up wrong, which is where the mistake gets made. The original finding: | `DESIGN.md` D8 and §8; every jitter claim; item 20 |
| 21 | **`DESIGN.md` D8 says "the engine thread is pinned to an isolated core". Nothing pins it.** `[measured 2026-08-30]` `grep` for `sched_setaffinity`, `affinity`, `core_affinity` or `libc` across `crates/` and `tools/` returns **nothing**: no dependency, no call, no test. §8's latency budget and §9's `isolcpus` row both assume a pinned engine thread, so the design's central jitter defence is **asserted in prose and absent from the code** — `CLAUDE.md` §4's "prose does not hold a constraint", on the one claim it would cost most. Either the engine pins and something proves it, or D8's sentence is wrong and must say so. Cheap to settle, and it is the mechanism the 324 ns mode most plausibly needs, since load amplifies that mode 5% → 92% and pinning is what isolates a thread from load. **NUMA is deliberately NOT part of this**: this machine reports **1 NUMA node**, cross-L3 placement was measured and had **no effect** (~259 ns in all three `taskset` arms), and topology-aware allocation would be designing against hardware nobody here has — the same reason item 14 keeps kernel bypass out. Revisit if the box ever becomes multi-socket | `DESIGN.md` D8 and §8; every jitter claim; item 20 |
| 6 | ~~**A Linux box for `tools/w2w`, and the wire-to-wire figures from it**~~ — **CLOSED 2026-09-02. Phase 1 exit criterion 6 is met and phase 1 is complete.** AMD Ryzen 7 3700X, Linux 7.0.0-30-generic, bare metal, `check-machine.sh` `pass 12  fail 0  unknown 1`, engine pinned to isolated `cpu6` and client to `cpu7`, medians of **20 whole runs of 20 000 timed round trips**: `hft` **p50 16 010 / p99 20 589 / p99.9 22 127 ns** administrative and **19 908 / 24 657 / 26 150** application; `standard` **19 447 / 24 106 / 25 609** and **20 920 / 25 618 / 27 092**. Spread of the per-run p50 **0.3–0.6%**, and `standard / admin` reproduced at **19 451** on an independent 10-run pass. Allocations in the timed window **0**, counted by the binary on both threads, red at 2 000 on a reversal. **What it took**: `w2w` pinned no threads and printed no p99.9, so no run of it could have satisfied §9 however well the box was tuned; both are fixed, the pin flag **refuses** rather than shrugs when the build cannot, and it refuses a core `isolcpus` does not name. `scripts/w2w-baseline.sh` is the committed procedure. **Three findings and three new items** — the D8 trade is **17.7%** end to end, `isolcpus` is worth **11× at p99.9 and nothing at p50**, and §8's parse row turns out not to describe the engine's inbound path (item **39**). **NOT closed by this**: §6's NIC-to-NIC row, item **40**. [measured-costs.md](docs/reference/measured-costs.md) | closed |
| 23 | **A gate can be green in CI and red on the machine doing the work.** `[measured 2026-08-31]` `scripts/check-lint-config.sh` built its throwaway crate in `mktemp -d`, where `rust-toolchain.toml` does not reach, and on a desktop with no `rustup default` it exited 1 saying *"the workspace lints do not deny: unwrap_used expect_used panic"* — while `cargo clippy` had not run at all. A **false red about the system under test**, and the same construction had a quieter twin: on any machine that *did* have a default, the gate was checking the workspace's lint config against a different clippy from the one the workspace pins, which `rust-toolchain.toml`'s own comment calls load-bearing. Fixed by copying `rust-toolchain.toml` into the scratch crate; proven by reversal — commenting out `unwrap_used = "deny"` names that one lint and exits 1, restoring it exits 0. **Found only because §9's checklist requires every gate to be run here and its output read.** Nothing in CI could have shown it: CI is the environment where it passes. Write-up: [reference/a-scratch-fixture-inherits-the-machine.md](docs/reference/a-scratch-fixture-inherits-the-machine.md), marked `[to testing-skills]`. **The other scripts were then audited and are clean**: three more use `mktemp -d`, but only for output files — `check-no-kernel-sleep.sh` and `check-standard-gives-the-core-back.sh` run a binary built in the tree, and `check-ktls-on-a-plain-socket.sh` runs `cargo build` inside `spikes/ktls`, where rustup still walks up to the repository's `rust-toolchain.toml`. One script was affected and it is fixed. **What stays open is the class, not an instance**: nothing prevents the next fixture from being written the same way | closed as an instance; kept as a shape to watch |
| 25 | ~~**The bench gate cannot catch a benchmark that got faster.**~~ — **CLOSED 2026-09-01** by [a-baseline-is-a-band](docs/plans/2026-09-01-a-baseline-is-a-band.md) and [ADR-0031](docs/decisions/ADR-0031-a-baseline-is-a-band.md), which amends ADR-0016 decision 1. The comparison is a **band**, `[baseline / margin, baseline * margin]`. **Under the floor is reported and counted, never red** — a real optimisation lands there too, and making it red would teach people to widen the margin, which destroys the ceiling as well; both causes need the same thing from a person, so it takes the shape `NO BASELINE` already had and is fatal only under `bench.sh --strict`. **Its side effect is larger than its purpose**: ADR-0016's own Consequences accepted *"a real speed-up leaves the baseline generous until somebody re-records"* — now something asks, every run. The rule moved to `crates/codec/benches/verdict.rs` and **got its first test**: a `harness = false` target is a `main()` `cargo test` never runs, so the logic deciding every §6 timing gate had none. `[measured 2026-09-01]` proven through the real harness with three injected baselines — in-band passes, over prints `OVER BASELINE` and still panics, under prints `UNDER BASELINE` and does not — and end-to-end through `bench.sh`. **Two things it did not close**: the `--strict` half is unverified, because `bench.sh --strict` exits 1 at the §9 check before reaching it and CI does not run `--strict` at all (ADR-0031 open question 3); and a separately measured `low_margin` column needs `n >= 20` runs on a §9 box. **A fourth Linux-only gate fell out of it**: `bench.sh` used `mapfile`, bash 4+, and macOS ships bash 3.2 — the script had **never run on a development laptop**, dying before it measured anything. Fixed; it now reports `targets measuring 10 of 10` here | *(nothing — this was the last of the three shapes)* |
| 26 | ~~**CI still carries the escape hatch it was given when the workspace held no crates.**~~ — **CLOSED 2026-09-01.** `count == 0` is now `exit 1` in all six jobs that had it, and **the 22 `if: steps.ws.outputs.count != '0'` guards are gone with it** — the `ws` step fails the job before any of them is reached, so a condition that could never be false was a comment pretending to be a check. `[measured 2026-09-01]` proven by reversal on this machine, which needs no CI run to see: `cargo metadata --no-deps \| jq '.packages \| length'` reads **6** and the block exits 0; with `members = []` it reads **0**, prints the `::error` and exits **1**. Restored, 6, exit 0. **The third of the three shapes items 22 and 25 name is now closed**; item 25 is the one still live, and it is the hardest of them | *(nothing)* |
| 27 | ~~**A section whose job is honesty is the one nothing re-reads.**~~ — **CLOSED 2026-09-02, by giving it a row rather than another audit.** The diagnosis was right and was never laziness: every closing plan walked `CLAUDE.md` §4's sync table and struck its own *open item*, and **no row of that table pointed at *Not proven***. So `CLAUDE.md` §4 now has one — *"Prove something this repository listed as unproven → strike the bullet in the same commit"* — and it is walked with every other row before a plan closes. `[said out loud, per CLAUDE.md's own rule about editing itself mid-session: the row added is in §4's table, and no §2 non-negotiable was touched.]` The audit was done too: an **eleventh** false bullet, *"the initiator is still barely exercised … nothing here has been driven against `libquickfix`"*, which stopped being true the moment `scripts/interop.sh` went green at 7 / 7. **The rot has a direction and it is worth naming**: this section only ever over-states how little is proven, which reads as modesty and is simply wrong — a reader deciding whether to trust the engine is misled downward. **What is still not machine-checked** is the row itself; like every other row in §4 it is walked by hand, and the next false bullet will be found the same way this one was | — |
| 28 | ~~**This is a link, not an acceptor: it serves exactly one counterparty.**~~ — **CLOSED 2026-09-01.** [counterparty-registry](docs/plans/2026-09-01-counterparty-registry.md), six steps, and **two ADRs came out of building it**: [ADR-0029](docs/decisions/ADR-0029-the-pre-session-stage-enforces-four-definitions.md) and [ADR-0030](docs/decisions/ADR-0030-one-engine-holds-many-counterparties.md). `presession::Registry` is a trait, `Table` the default implementation, and **an empty one refuses everything** — `serve` will not start on it. `Identity` carries `50=`/`57=` when the message does. `[measured 2026-09-01]` **one engine now holds many counterparties**, and the single-logon rule compares identities rather than counting connections, which is what `1b_DuplicateIdentity.def`'s own comment always asked for. 12 tests, six reversals; `benches/alloc.rs` reads `registry-lookup 0` and goes to 100000 when `lookup` is given a `to_vec()` key. **Two things it did NOT close**: there is still **no configuration file** — a `Table` is built in code — and **no nanosecond number**, because `benches/baselines.tsv` keys on CPU model and this was built on a Mac. ADR-0030 open question 1 is the debt: the identity comparison is O(n²) in connections on the `turn` path. **The finding worth keeping** is ADR-0026 decision 5, which was written on 2026-09-01 and superseded on 2026-09-01: it said one engine holds one counterparty, had no implementation, and was contradicted by the prior-art table in its own file | *(nothing — GUIDE.md §1a0 is written, entry points are changed, sharding has a purpose)* |
| 29 | ~~**The application-message resend path is implemented and has never run.**~~ — **REFUTED 2026-09-01, by running it.** `[measured 2026-09-01]` `cargo test -p fixbolt-engine --test journal` → **7 passed, 0 failed**. `a_replay_says_when_it_is_being_sent_and_when_it_first_was` feeds a real `35=D` from `8_OnlyApplicationMessages.def`, takes the echo, sends a `ResendRequest` and asserts **one replay, not a gap fill**, at the original number, with `43=Y`, a fresh `52=` and the first `52=` carried as `122=`. Three sibling tests replay application messages under the other D7 policies, and `none_keeps_nothing_and_fills_over_everything` is the arm that *does* gap-fill — so the two outcomes are distinguished, not assumed. **Also: `cargo test --all` is 272 passed, 0 failed, 56 binaries on this container.** **How this item came to exist is the point.** It was written from `STATUS.md`'s own *Not proven* bullet — *"No application message has ever been replayed"* — which is **the ninth false bullet in that section**, and it was believed **on the same day, in the same session, that the rot in that section was documented and written up as [a-known-limitations-list-rots-in-one-direction.md](docs/reference/a-known-limitations-list-rots-in-one-direction.md)**. The code was checked (`Session::replay` exists); **whether a test reached it was not**. `[to testing-skills]`: *the author of a warning about stale claims is not thereby immune to them* — a written-down failure mode does not become checked behaviour, and the only thing that settled this was **running the test**, which took one command. The write-up now carries it as its strongest case | nothing — it is closed |
| 31 | ~~**An `Engine` cannot resume a session**~~ — **CLOSED 2026-09-02.** `Engine::add_resumed` for a caller driving the engine, and **`serve_with_recovery` / `serve_hft_with_recovery`** for one using the serving loop, asking a `Recovery` once per connection after the registry names the counterparty ([ADR-0034](docs/decisions/ADR-0034-recovery-is-asked-once-the-counterparty-is-known.md)). The journal travels with the counts, because correct numbers over an empty journal answer the first `ResendRequest` with a gap fill — legal, and a silent loss of exactly what was asked for. `last_active_ms` travels with them too, which is what makes [ADR-0033](docs/decisions/ADR-0033-a-schedule-is-utc-arithmetic-and-the-calendar-stays-outside.md)'s boundary reset reachable from an engine. **Proven by two reversals that fail on opposite tests**: discarding what `Recovery` answered reddens only the resuming test; making `NoRecovery` fabricate a session reddens only the plain-`serve` control. **Three things it does not do, and they are item 32:** `serve_sharded_hft` has no variant, `pump` fixes `J = journal::Store` so a per-counterparty `FileJournal` is not reachable through the serving loop, and **nothing persists `last_active_ms`** | closed |
| 32 | **Sharded deployments still cannot resume.** `[2026-09-02]` **(b) and (c) are closed** — [ADR-0039](docs/decisions/ADR-0039-a-fresh-journal-is-the-deployments-to-build.md), [plan](docs/plans/2026-09-02-recovery-reaches-the-disk.md). `Recovery::fresh` moved the `J: Default` bound off the serving loop and onto the implementations that want it, so a `FileJournal` per counterparty now runs through `serve_with_recovery`; and `Journal::mark_active` / `last_active` put the instant a session was last alive on disk, as a record whose sequence number is zero — the format did not change. **The reversal for (b) is a compile error rather than a red test**, which proves the bound is absent and not merely unexercised. What remains, and the old text follows: **Recovery reached two of the three serving entry points, and no journal on disk.** `[2026-09-02]` what item 31 left: **(a)** `serve_sharded_hft` has no `_with_recovery` variant — it is Linux-only and was deliberately not touched from a Mac, which ADR-0034 records as the reason; ~~**(b)** `pump` builds a concrete engine…~~ and ~~**(c)** nothing persists `Session::last_active_ms()`…~~ — **both closed 2026-09-02.** **(a) needs a Linux session**, and is now the third thing that entry point is missing, after recovery and an ordered shutdown. Also unbounded: `Recovery::recover` runs on the acceptor thread and a slow implementation delays every connection behind it, with ADR-0020's pending deadline as the only backstop and no way to say why | A sharded or file-journalled deployment; unattended restart across a trading day |
| ~~30~~ | **CLOSED 2026-09-02 — all six.** `[2026-09-02]` **(a), (b), (c), (d), (e) and (f) are done** — module `observe`, [ADR-0032](docs/decisions/ADR-0032-observation-is-a-snapshot-taken-on-request.md), `GUIDE.md` §8a. `Engine::observer()` hands out a `Send + Sync` handle; `Observer::request()` gets a `Snapshot` carrying, per session, logged-on state, both sequence numbers, whether output is backed up and **the measured clock skew** — recorded whether the message was accepted or refused, because a `max_skew_ms` refusal is silent by protocol. `Snapshot::healthy()` is the probe, a pure function on the same data. On request only: the cost while nobody watches is one relaxed load per turn, and `benches/alloc.rs` cases `observe-idle` and `observe-asked` both read 0. **How the last one closed:** The shared constraint — readable from another thread without touching the hot path — **is now solved once**, in `observe`, and (a), (c) and (d) reuse that mechanism rather than inventing a second. `PRD.md` open decision 9 is answered by [ADR-0027](docs/decisions/ADR-0027-the-engine-owes-a-byte-stream-not-an-archive.md). **Two things this leaves unproven:** the nanosecond cost of a turn that *does* publish (needs the §9 machine — [plan](docs/plans/2026-09-01-operability.md) reversal 2), and ring depth and pending-set occupancy, which (b) asked for and the snapshot does not yet carry **(d) closed 2026-09-02** — [ADR-0035](docs/decisions/ADR-0035-an-event-is-pushed-and-a-loss-is-counted.md), [plan](docs/plans/2026-09-02-why-a-connection-ended.md). `DropReason` gives the session's eighteen `Link::Dropped` sites a name each, and `Observer::events()` carries them off the engine thread. **Pushed, not asked for**: a snapshot not taken at the right moment is a stale number, an event not recorded is gone. Losses are counted rather than swallowed. **Three kinds only** — logon, ended, ended-without-reason; gap/resend/reject are message-rate and stayed out. `[measured 2026-09-02]` `events-idle` and `events-busy` both 0. **What it did not prove is in *Not proven*: the `try_lock`, the ring's size, and the three missing kinds.** **(c) closed 2026-09-02** — [ADR-0036](docs/decisions/ADR-0036-one-mechanism-two-capabilities.md), [plan](docs/plans/2026-09-02-sequence-numbers-at-three-in-the-morning.md). `Engine::admin()` is a second handle over `observe`'s own `Arc`: **one mechanism, two capabilities** — `Observer` looks, `Admin` changes. `SetNextOut` / `SetNextIn` / `SendSequenceReset`, applied at the **top of a turn, before anything is numbered**. The lock asymmetry is the design: the operator's thread may block, the engine's may not, and **a refused `try_lock` loses nothing** — unlike an event, a lost command is an action that silently did not happen. A full queue refuses at the call. Outcomes ride the event stream. `[measured 2026-09-02]` `admin-idle` and `admin-busy` both 0. **What it did not do is in *Not proven*: nothing authenticates an `Admin` holder, `SetNextOut` is a foot-gun the type system cannot flag, and the order rule rests on one test.** **(e) closed 2026-09-02** — [ADR-0037](docs/decisions/ADR-0037-reading-a-journal-is-not-recovering-from-one.md), [plan](docs/plans/2026-09-02-what-the-journal-can-answer.md). `journal::Reader` is an `Iterator` over the **whole file** — `FileJournal`'s ring answers the next `ResendRequest`, which is a different question — and `tools/jrnl` is the binary, because *"nothing outside the process can read it"* is not answered by a library function. **It also fixed a defect in the engine**: `FileJournal::open` counted torn trailing bytes and then did `let _ = torn;`, so a process killed mid-write left no trace in the one file that exists to say what happened. Skipping them is right; being silent was not. `jrnl` warns **and exits 2**. **What it did not do is in *Not proven*: the whole file is read into memory, reading a file the engine is appending to is undefined, and nothing correlates a journal to a counterparty.** **(a) closed 2026-09-02** — [ADR-0038](docs/decisions/ADR-0038-an-ordered-shutdown-is-a-state-not-a-flag.md), [plan](docs/plans/2026-09-02-an-ordered-shutdown.md). `Admin::shutdown(grace_ms)`; `run`, `serve` and `serve_hft` **return** a `Shutdown` naming what they could not do. **`State::LoggingOut` is the load-bearing part** — `[measured 2026-09-02]` reusing `AwaitingLogout` made every wait vacuous, because it reports the link down at once, so *they answered* and *they never answered* were the same observable. The deadline is the caller's, and without it the reversal is **a hang, not a red test** — killed at 600 s. Nothing leaves anonymously: a session closed at the deadline gets `DropReason::EngineShutdown` and emits an `Ended` event before the vector is cleared. **`serve_sharded_hft` is Linux-only, untouched, and still cannot be stopped** — see *Not proven*. | ~~Phase-1 deployability~~ — **item 30 is closed**; step 3 of [operability](docs/plans/2026-09-01-operability.md) was overtaken by the four plans that did the work |
| 24 | ~~**Sharding breaks the single-logon rule**~~ — **CLOSED 2026-09-01.** `[measured 2026-09-01]` the corpus scores **59 through two shards**, where it scored 57. All six steps of [pre-session-routing](docs/plans/2026-08-31-pre-session-routing.md) except the closing measurement: [ADR-0020](docs/decisions/ADR-0020-a-pre-session-stage-owns-the-socket-until-logon.md) decided the shape, `presession::PendingSet` holds each socket until its `Logon` arrives, and `Shards::hand` routes on a **stable** hash of `(49, 56)` so both connections claiming one identity reach the same engine. **`Assign` and `RoundRobin` are gone** — `Assign` was asked at accept time, when nothing knew whose socket it was, and round-robin is the policy that produced the defect; keeping it would be leaving a documented trap in a public API. **The characterisation test went red first**, on 59 against 57, which is what it was written to do rather than a target. **And then the number was not accepted on its own.** `1b_DuplicateIdentity.def` and `AlreadyLoggedOn.def` both expect *no response at all* on the second connection — and **a socket the new stage threw away produces exactly that**, so 59/59 could not tell *"the session refused the duplicate"* from *"the stage dropped it first"*. The test was made to count every disposal and assert zero; `[measured 2026-09-01]` **it went red at `[timed_out 0, not_logon 1, gone 1, unrouted 0]`** — two connections never reached an engine. Both turned out legitimate and are now pinned **by name and by count**: `1e_NotLogonMessage.def` (*"if first message is not a Logon, we must disconnect"*) and `1d_InvalidLogonLengthInvalid.def` (`9=40` is a lie the framer takes at its word). A **third** disposal would be a new defect wearing the same 59/59. That moved the framing-garbage rule into a second home, which `frame.rs` had explicitly promised it would not have — corrected in the same commit, and decided in [ADR-0022](docs/decisions/ADR-0022-the-pre-session-stage-enforces-two-definitions.md) rather than left to be found. **Two hard limits arrived with the layer and neither has a default**: a logon deadline and a ceiling on waiting connections, both refused at zero — an acceptor without them is an open port. `[measured 2026-09-01]` **the guard for step 3's allocation claim was itself a false green** and is written up at [the-guard-measured-a-window-that-excluded-the-thing.md](docs/reference/the-guard-measured-a-window-that-excluded-the-thing.md); so is [a-conformance-corpus-is-not-an-adversarial-one.md](docs/reference/a-conformance-corpus-is-not-an-adversarial-one.md) from step 2, where **289 real corpus messages stayed green against two broken readers** and one hand-built message caught both. `[measured 2026-09-01]` **step 6 closed it with a price**: `crates/engine/benches/presession.rs`, 20 qualifying runs on the ADR-0021 §9 line — the stage's sweep costs **426.2 ns per socket** against `Engine::turn`'s 458.3, so waiting for a `Logon` is *cheaper* per socket than serving one, and its own work over the bare `recv` is **~15 ns** against the engine's ~28. Reading both comp IDs and picking a shard is **84.0 ns, once per connection** — a fifth of one `recv`. **`DESIGN.md` §8 does not move**: none of this is on the message path. **What is NOT measured and is said so**: the wall-clock latency a `Logon` gains from the channel hop and the cross-thread handoff, which a bench of this shape cannot reach — `tools/w2w`, open item 6, is what would. The original finding: |
| 24 | **Sharding breaks the single-logon rule, and the acceptance corpus is what found it.** `[measured 2026-08-31]` the 59 definitions score **59 through one shard and 57 through two**, at both settle bounds so it is not timing, failing exactly `1b_DuplicateIdentity.def` and `AlreadyLoggedOn.def` — `crates/engine/tests/shard_wire.rs`. **The rule was right and sharding invalidated its premise**: an `Engine` carries one `Config`, therefore one FIX identity, so it can answer *"is this identity already logged on"* by counting the other connections **it** holds. Split those across engines and there is nothing to count. **`Assign` cannot fix this** — it is asked at accept time, and the `Logon` that names the identity has not arrived; a real acceptor reads the `Logon` first and routes afterwards, which means a pre-session stage that owns the socket until then. **Decided 2026-08-31 by the owner: option A, a pre-session stage** — the acceptor holds the socket, reads the `Logon`, and routes by identity, *the way real engines do it*. The alternative offered was a shared registry the engine consults; it was not taken. Planned in [pre-session-routing](docs/plans/2026-08-31-pre-session-routing.md), six steps, step 1 is **ADR-0020**. Not started. **What was deliberately not done**: giving the test an assignment policy that keeps both connections on one shard would have made it green and proved nothing — `CLAUDE.md` §10's named failure. Instead `two_shards_break_the_single_logon_rule_and_this_records_it` pins the defect and its two files, and **goes red when the defect is fixed**, which is the point. Until then `shard::Shards`, `serve_sharded_hft`, `GUIDE.md` §1a and `DESIGN.md` §3 all say so in their own words | `threads-and-affinity` step 4 closing; any multi-counterparty acceptor |
| 7 | ~~**`scripts/fetch-quickfix-assets.sh` tracks mutable `master`.**~~ — **CLOSED, and this row was stale when it was read on 2026-09-01.** The script pins `PINNED_SHA=386ce46e…` (2026-05-20), fetches that commit rather than a branch, **and verifies what it got**: 59 definitions, 539 message lines, 244 lines carrying `10=`, checked in the script so a ref that disagrees fails at the fetch rather than three layers away in a test whose message would be about a field count. `[measured 2026-09-01]` re-run here, green. **Item 27's shape, in the open-items table rather than in *Not proven*** — the fix and its verification were both in the tree and nothing had re-read the row | *(nothing)* |
| 10 | ~~**Can `ktls-core` be driven from a plain non-blocking socket with no async runtime?**~~ — **CLOSED 2026-08-31: yes, with four conditions.** [ADR-0018](docs/decisions/ADR-0018-ktls-on-a-plain-socket-answers-adr-0005.md) supplements ADR-0005 rather than superseding it; `DESIGN.md` D11 keeps its shape and its kTLS row goes from reasoned to measured. **The question looked hard because ADR-0005 cited three crates as one.** `ktls` 6.0.2 (rustls org) really is `tokio-rustls`-specific and `ktls-stream` defaults to a `tokio` feature — but `ktls-core` 0.0.5, the crate the question names, depends on `bitfield-struct`, `libc`, `nix`, `zeroize`, has **no async feature at all**, and exposes only synchronous entry points generic over `AsFd`. `Context::handle_io_error` is exactly the shape a spin loop wants. `[measured 2026-08-31]` Ryzen 7 3700X / Linux 7.0.0-30-generic / `ktls-core` 0.0.5 / `rustls` 0.23.43 / TLS 1.3 AES-128-GCM: **15 assertions, `fail 0`**, and `strace -f` over 1000 round trips attributed by tid shows `recvfrom` 3033 + `sendto` 1000 and nothing else — against the red arm's `poll` 1000, which is what proves the green arm could fail. Spinning costs ~3.0 `recvfrom` per message where blocking costs 1.0; that is ADR-0013's bargain, unchanged by TLS. **The four conditions, each measured:** every read error goes to `ktls_core::Context` (`EIO` once per connection from session tickets); the transport **never** reads the socket outside the offload (hand-draining desynchronises the kernel permanently — `EBADMSG`, and `TlsDecryptError` moves); the handshake hands over with an empty buffer; `setup_ulp` needs `ESTABLISHED`. **Not answered, and not to be inferred:** any latency number (§8's TLS row stays empty on purpose), key update under kTLS, TLS 1.2, mutual TLS, the kernel/suite floor, and what asserts which mode is live — ADR-0005 open questions 2, 3, 4, 5, 6. **A false green of my own is written up with it**: the wire check asserted a record's *shape* and passed against a session-ticket record the test never sent. [reference/ktls-on-a-plain-socket.md](docs/reference/ktls-on-a-plain-socket.md), `spikes/ktls`, `scripts/check-ktls-on-a-plain-socket.sh`. | closed — the TLS plan is unblocked and unwritten |
| 11 | ~~**Serialise misses its gate: 93.8 ns against 60 ns**~~ — **CLOSED 2026-08-31 by [ADR-0016](docs/decisions/ADR-0016-per-machine-baselines-replace-absolute-targets.md).** The item asked how to reach 60 ns and the answer is that **60 ns was never a measurement of this engine** — `DESIGN.md` §4 D9 says it is how the fastest commercial engines are *reported* to perform. Three steps got here. The cause this item named — the linear slot scan — was measured at **~24% at most**, not the bulk; the fix it proposed was written, measured at **+5.2 ns** against a predicted −36, and **reverted**; and the split that does hold is **~31 ns of fixed cost before the first variable field** plus **~7 ns per field** in `put`, so a perfect scan still leaves **~116 ns**. §6 now gates on a per-machine baseline: `[measured 2026-08-31]` **239.1 ns** on the §9 desktop, median of 24 qualifying `bench.sh` runs, margin 1.10, and **`bench.sh --strict` exits 0** there — 0 of 12 cases over, 0 without a baseline. The ~116 ns floor survives as a **Stretch row that is explicitly not a gate**. | closed |
| 12 | ~~**SIMD / SWAR for SOH scan and checksum**~~ — **CLOSED 2026-09-02 by [ADR-0045](docs/decisions/ADR-0045-parse-is-under-one-percent-of-the-wire-and-simd-is-declined.md), on the data its own condition asked for.** The item said *do it only when `benches/parse.rs` on the Linux box shows parse on the critical path*, and that condition could not be evaluated for as long as this project had only the numerator. `[measured 2026-09-02]` parse is **0.62%** of the application round trip (122.6 ns of 19 908) and **0.36%** of the administrative one (57.3 of 16 010). The item's own estimate of the gain, **20–40 ns**, is **0.10–0.20%** — **smaller than the 0.5% run-to-run spread of the only instrument that could see whether it helped**. That is the decision: not that 20–40 ns is worthless, but that this design cannot tell whether it got it. **The 20–40 ns is not a measurement either** and the ADR says so — no SWAR scan was written, so only the denominator is measured. **One thing reopens it, named**: a transport that removes the kernel term (item 14). At ~2 µs a round trip, parse's share goes to ~6% and the arithmetic changes. | closed |
| 13 | ~~**Release profile is default**~~ — **CLOSED 2026-09-01, and the answer is *keep it*.** `[đo 2026-09-01]` four arms, ten `bench.sh` runs each, no reboots: `lto="fat"` gives **−2.9% to −5.6%** on the syscall-bound path and up to **−31%** on one pure function, `codegen-units=1` gives −0.1% to −16.6%, and a clean build goes from **5.2 s to ~16 s**. The prediction written before the runs — user-space improves, syscall-bound barely moves — was directionally right and **understated the syscall side**. **And then the decision went the other way**, on two things the table does not show. **Cargo honours `[profile.*]` only from the top-level package being built**, so a profile here reaches this workspace's benchmarks and `tools/w2w` and **not anybody who depends on these crates** — it would make the published numbers better and no consumer's program faster, which is the shape non-negotiable 10 exists to stop. **And part of the gain is an artifact of measuring**: a benchmark is a separate crate, so LTO inlines library internals into the benchmark loop. `presession, read and route` fell 83.4 → 57.7 ns, but production calls `identity_of` from inside the same crate where it is already inlinable; `recv` fell 2.9% on a case that is 94% kernel time, which is 12 ns off ~25 ns of user-space work. **How much survives into a real application was not established and no figure is claimed for it.** [ADR-0024](docs/decisions/ADR-0024-the-workspace-keeps-the-default-release-profile.md) keeps the default and puts the range in [GUIDE.md](docs/GUIDE.md), where the reader's own profile *does* apply. **This experiment is also what found open item 25**: `inline deliver + reply` read 1.3 ns in three arms and 7.4–8.6 in two, which looked like a `codegen-units` regression and was the benchmark deleting a 163-byte copy. **PGO and `#[cold]` stay out** and the ADR says why. Write-up: [measured-costs.md](docs/reference/measured-costs.md). The original finding: |
| 13 | **Release profile is default.** No `lto = "fat"`, no `codegen-units = 1`, no PGO, no `#[cold]` on error paths. Cheap, but each is a number to be measured before and after, not a setting to be assumed. **Start with `scripts/bench.sh --strict`**, once before any profile change and once after — same machine, same settings, or the comparison means nothing. | The `engine` step; every §6 number published from Linux |
| 14 | ~~**What is unmeasured and decides the cache wall is how much of that 53.3 KiB a message touches**~~ — **CLOSED 2026-09-05, and both halves of the question changed under it.** `[measured 2026-09-05]` the 53.3 KiB **does not exist**: `size_of::<Connection<..>>()` is **21 456 bytes**, because [ADR-0046](docs/decisions/ADR-0046-the-ring-is-the-resend-store-and-a-replay-goes-in-batches.md) boxed the journal ring on 2026-09-04 and 33 288 B left the struct for the heap. So a connection **does** fit in a 32 KiB L1d, and the two bounds this row offered were arithmetic on a withdrawn number. **The measured answer: a message touches ~2 to 4 KiB.** `crates/engine/benches/density.rs` sweeps N logged-on busy sessions and finds **a ramp, not a wall** — per message 1.000 / 1.018 / 1.031 / 1.067 / **1.139** at N = 1 / 8 / 16 / 32 / 64, medians of 20 clean runs of 22. **The absence of a step at N ≈ 20 is the proof**: a message touching most of its connection would put one at the 512 KiB L2 edge, and there is none. Recomputed, the edge is nearer N ≈ 200, and at N = 64 cache alone has added **13.9%** to every message — the smallest of this page's three density terms. And a 2 MiB ring costs nothing measurable: identical work with the ring swept 4 KiB → 2 MiB moves 1.5% and **not monotonically**. [ADR-0052](docs/decisions/ADR-0052-two-candidates-are-retired-with-numbers-and-a-baseline-disagrees-with-its-neighbours.md), [measured-costs.md](docs/reference/measured-costs.md). The original entry: |
| 14 | **Kernel bypass path, if PRD §5 is ever reversed: Onload first, `ef_vi` second, DPDK never.** Onload runs the engine unchanged (`onload ./engine`, socket API, TCP in userspace) — D8 spin already fits it; the first measurement is `tools/w2w` twice on the same box, kernel vs `onload`, and that difference decides whether an `ef_vi` L0 is worth writing. `ef_vi`/TCPDirect is a second `impl Transport` behind a real feature flag (D5). DPDK ships no TCP stack — it means writing or embedding one (smoltcp, F-Stack), which is what fixbolt claims and does not do. Any bypass path is plaintext: it and D11 exclude each other. Needs a Solarflare/AMD X2-class NIC — none available | Phase 3, and open item 6 before it |
| 16 | ~~**The journal is written and never read back**~~ — **CLOSED 2026-08-31, and its closure is narrower than it reads.** `[2026-09-02]` everything below was true and **none of it was reachable from an `Engine`** for five days: `crates/engine/tests/recovery.rs` proves the whole mechanism with **zero occurrences of `Engine` in the file**, and both of the engine's `add` methods built `Session::new`, which resets. A layer was finished and the seam above it was never asked about, by a plan whose exit criteria were all satisfiable one layer down. `Engine::add_resumed` is the join — item 31. The original entry: All six steps of [session-recovery](docs/plans/2026-08-30-session-recovery.md) are done. Steps 1–3 made the journal readable; step 4 is [ADR-0010](docs/decisions/ADR-0010-a-reconnect-is-not-a-restart.md) — `Session::resume`, and `connect` keeps the count for a session that persisted something; step 5 is **[ADR-0017](docs/decisions/ADR-0017-the-inbound-count-is-persisted-after-delivery.md)** — `Journal::mark_in`/`highest_in`, written **after** the application sees the message. `Durability::Fsync` is now a recovery mechanism rather than an audit trail, in both directions. **59/59 unchanged and no corpus file exempted**; forcing `connect` to never reset drops it to **56/59**, which proves the corpus exercises that branch. Six reversals across the two steps, and one of them found a worthless test of mine — see the plan's log. **Two things are NOT proven and are named rather than implied**: nothing has measured what the extra `sync_data` costs the inbound path, and no `.def` file restarts a process, so every test here is one this project invented. | closed |
| 18 | ~~**A plan can close on a laptop's word while CI is red.**~~ — **CLOSED.** The engine plan closed and merged with its gates reported green from an Apple M5; the GitHub run on that same commit failed and was not read, and four documents carried the laptop's number for a day. **`CLAUDE.md` §9 now carries the box** — *a green CI run is named, by id, for the commit being closed* — and every plan closed since has named one: `33394684357`, `33386125577`, `33473213210`. The rule that was missing exists; **what no gate does is check that the named run is green, or is for that commit at all**, so this stays a hand-check on the §9 list | Every "gates green" claim in a merge commit |
| 34 | ~~**The library layer materialises a `Template` per message, and the half that is left is ~24× the fast path.**~~ — **CLOSED 2026-09-05 as measured and accepted, and it was a third of the size it was recorded at.** `[measured 2026-09-05]` §9 desktop, n = 20, `pass 12 fail 0 unknown 1`, all four from `benches/baselines.tsv`: `library, reply only` **804.1 ns** against `encode ExecutionReport (template)` **237.6 ns** — **3.4×**, not 24× and not 50×. **The denominator of the published ratio had no committed benchmark.** ADR-0041 attributed its *40 ns* to a `crates/library/benches/attrib` experiment that never entered the repository; the committed benchmark for exactly that shape read **177.6–199.4 ns on the same VM class** in the same week. Withdrawn under non-negotiable 10, and not replaced with a guess about what it timed. By inference — labelled as such — materialising a `Template` per message costs **~570 ns**, which is **2.9%** of the 19 908 ns application round trip on this box; the whole convenience layer over a hand-written `Application` is at most ~800 ns, 4%. ADR-0041 open question 2 closes too: on §9 the three cases **add up** (159.6 + 804.1 = 963.7 against 1028.6, +6.7%) — the VM's *whole-less-than-the-sum* was the VM. **Owner's choice, 2026-09-05: not worth a codec hot-path change for 2.9%.** [ADR-0051](docs/decisions/ADR-0051-item-34-is-a-third-of-the-size-it-was-recorded-at.md); six documents corrected in its commit; the wave-D draft `const-templates-in-dict` re-scoped from *close item 34* to *an optimisation worth ≤ 570 ns per reply*. What is **not** claimed: what the 40 ns experiment measured — the experiment is gone | ~~A reply that costs what D9 says a message costs~~ — a reply that costs 3.4× it, said from committed benchmarks |
| 35 | ~~**An initiator that loses its connection is on its own.**~~ — **CLOSED 2026-09-02.** [an-initiator-that-comes-back](docs/plans/2026-09-02-an-initiator-that-comes-back.md), [ADR-0043](docs/decisions/ADR-0043-backoff-without-jitter-and-a-reconnect-asks-recovery-every-time.md). `engine::reconnect::Policy` — doubling to a ceiling that holds, reset by a `Logon` and **not** by a socket, an optional `Schedule` that outranks the ladder, and `stop()`. It **answers a question and never sleeps**: `Next::At(instant)`, the caller's own wait strategy does the waiting, which is what non-negotiable 4 is about on that thread. `connect_and_serve` is the loop, and it needed **no new `Engine` API** — `recovery.recover(&cfg)` is asked on *every* attempt and `add_resumed` was built for exactly this. Gates: `--test reconnect` 8 cases with no I/O and no clock; `--test reconnect_wire` 2 over a real socket, with **orthogonal** reversals — ignoring the policy reddens the stop control alone, never coming back reddens the reconnect test alone. `benches/alloc.rs` case `reconnect` reads 0. **Three things it deliberately does not do, all in the ADR's consequences and in `GUIDE.md` §8c**: no jitter (a fleet against one venue reconnects in lockstep); `NoRecovery` restarts the numbering on every reconnect, which is right for an in-memory journal and wrong for a counterparty expecting continuity; and there is **no `hft` initiator entry point**. **And every test of it is invented** — no corpus here covers reconnect, so unlike `tests/score.rs` it measures this engine against this project's own reading. Closing that needs an interop scenario driving a real counterparty through a disconnect, which `scripts/interop.sh` could grow and today does not — **new open item 38** | — |
| 38 | ~~**The reconnect loop has no second opinion.**~~ — **CLOSED 2026-09-05.** [reconnect-interop](docs/plans/2026-09-05-reconnect-interop.md), and it is **not** the eighth step this row proposed. Reading the code first showed why: `tools/interop --role initiator` drives `Session` directly over a blocking socket and never calls `connect_and_serve`, so a step added there would have tested the tool's own loop and left the engine's still unread. A third role does call it. `[đo 2026-09-05]` two scenarios, the C++ acceptor killed and restarted on the same `FileStore`: **`SIGKILL` 5 / 5** — no goodbye, came back unprompted, **at one past the number it last sent**, the venue's next messages *delivered to the application* rather than gap-requested, and no `35=2`, `141=Y` or `MsgSeqNum too low`; **`SIGTERM` 3 / 3** — the goodbye answered, redialled anyway (ADR-0043 decision 5, from an engine that never heard of it), and refused by exactly one. **Three reversals, each biting in a different place**: `--no-recovery` turns `next_out` red and leaves `dropped` green; a policy that will not redial inside the deadline turns `back` red and leaves `no_resend` **green**, which is what tells "did not come back" apart from "came back wrong"; renaming one step in the grep list fails the script while the scenario still prints `PASS 5/5`. **And the first run found something bigger than the scenario** — new item **48**, plus the assertion that read *"nothing arrived"* about a message that had arrived twenty-two times ([a-refused-message-never-reaches-the-callback](docs/reference/a-refused-message-never-reaches-the-callback.md), **`[to testing-skills]`**). **Not covered**: a fixbolt process that restarts — only the venue dies here | an initiator whose reconnect was checked by an engine that never heard of us |
| 36 | **The mirrored corpus reads 10 / 50, and the ceiling of 45 is now in doubt.** `[measured 2026-09-02]` **0 → 2 → 10 in one session, and the last jump was two real defects rather than harness work.** The 2 came from `Input::Originate(Intent)` plus [expected-output-is-not-valid-input](docs/reference/expected-output-is-not-valid-input.md); the 10 came from the gate finding that a session which said goodbye first **answered the acknowledgement with a third `Logout`**, and that `begin_logout(b"")` wrote an empty `58=`. Neither was visible to the acceptor corpus — an acceptor never starts a logout — and both are the same family as the `Logon` echo. `crates/session/tests/goodbye.rs` holds them, with the pair that says a goodbye we did *not* start is still answered. **The drive count is unchanged at 141**, which is what says the eight new passes came from the session and not from the harness working harder. **What stops it going further is not effort.** `[measured 2026-09-02]` at least **six** of the remaining files ask this end to originate a `SequenceReset` in shapes no operator API should expose: three want `34=0` — a QuickFIX-ism for *not sequenced*, which would mean the harness dictating a sequence number, i.e. the byte back door [ADR-0042](docs/decisions/ADR-0042-a-second-implementation-is-the-only-independent-opinion.md) decision 1 exists to refuse — and three want `123=Y`, a gap fill, which is not an operator action at all: it is the session's own answer to a `ResendRequest` and it already does it. **The other 34 are not classified, and this row does not pretend they are.** So **ADR-0006's ceiling of 45 is probably wrong and nobody has measured the real one**; correcting it needs a new ADR (an accepted one is never edited) and, first, a file-by-file classification of what each remaining one would require. That classification is the next piece of work here, and it is worth more than the score | A mirrored ceiling that was measured rather than estimated, and a score that stops where the operator API honestly ends |
| 37 | ~~**`shard::the_same_identity_always_lands_on_the_same_shard` is red on a shared cloud VM and green on `ubuntu-latest`.**~~ — **CLOSED 2026-09-02, and the cause is not the one this row guessed.** The row said *"the VM is slow" is a guess, not a finding* — correct, and the guess was wrong. `[measured 2026-09-02]` **the test had been failing since 2026-09-01 and no machine could see it fail.** ADR-0026 moved the counterparty registry into the pre-session stage, so the helper was given `One::new(cfg())`, serving `TW44` and nothing else; that test is the only one needing a **second** identity and hands the stage a `TW45` the registry is right to refuse. `Progress { unknown: 1 }`. The five-second deadline then turned a refusal that took microseconds into what read like a slow socket. **It went unseen because every machine that ran it skipped it**: `plan_for` returns `None` where two shards will not fit, and a `#[test]` that returns early reports `ok` — GitHub's runner has too few physical cores, and the reference desktop last ran the suite the day before the break. Fixed: the registry serves both identities; the failure counts every `Progress` outcome and names which happened; `plan_for` prints `SKIPPED` on stderr. **And the fix turned a second blind assertion red**: a shard asserted `cfg.serves(b"TW44", …)`, true of the only config a one-counterparty registry can produce, so it held whether or not the config had travelled — the property its own comment claimed. It now reads `49=` off the connection's own wire, and the reversal proves the difference: with the runtime patched to stop the config travelling, the wire-based assertion goes **red** and the old constant one goes **green**. `cargo test -p fixbolt-engine --features affinity --test shard` reads **7 passed; 0 failed**, from 6/1. [a-test-that-skipped-itself-on-every-machine-that-ran-it](docs/reference/a-test-that-skipped-itself-on-every-machine-that-ran-it.md), **`[to testing-skills]`** | — |
| 39 | ~~**The dictionary pass on every inbound message has never been timed.**~~ — **CLOSED 2026-09-05, and it is not the answer it was opened as the leading candidate for.** `fixbolt_session::validate` is public ([ADR-0050](docs/decisions/ADR-0050-the-dictionary-pass-is-public-so-it-can-be-timed.md)) because the pass could not be reached otherwise: `Fix44` is written into the bodies of the three functions rather than passed to them, so the plan's preferred "difference against an empty dictionary" does not exist; the only public route in is `received_with` into a private `judge` that also parses, checks identity and calls the application; and a bench target is a **separate crate**, so `pub(crate)` would not have reached it either. `crates/session/benches/validate.rs`, `[measured 2026-09-05]` **n = 21**, `pass 12 fail 0 unknown 1`, alignment pinned: **`validate NewOrderSingle, w2w bytes` 897.3 ns**, **`validate TestRequest, w2w bytes` 218.4 ns**, 882.1 and 169.5 for the two `parse.rs` shapes. **Seven times the parse it follows** (120.0 ns) and the largest single user-space row in §8. Proven by reversal: `validate` returning `None` immediately reads **1.1 ns**. **And the subtraction was finished out loud, which is the point**: the pass is **678.9 ns of the 3 898 ns, 17.4%**; with inbound parse (~+60), dispatch (+9), the application's own re-parse (+114) and its template encode (+233) the measured subtotal is **~1 094 ns, 28%**, and **~2 804 ns is still unattributed — new open item 49**. `DESIGN.md` §8 carries the table; §8's user-space total was ~0.46 µs and is now **~1.36 µs** for an application message, because `session ~0.1 µs` had been silently standing in for this pass. **The bench also found a defect in two older ones** — see item 50 | ~~`DESIGN.md` §8's honesty about its own parse row~~ — it has a row, and a remainder with a number |
| 40 | **`DESIGN.md` §6's wire-to-wire row asks for NIC to NIC and item 6 closed over loopback.** The figures published on 2026-09-02 contain no driver, no interrupt and no wire — which is why §9's NIC IRQ affinity row reads `unknown` beside every one of them, correctly. §6 now carries **two** rows: the loopback one, met, and this one, open. **What it needs and does not have**: a second machine to run the load generator on, `SO_TIMESTAMPING` in `tools/w2w` so the timestamps come off the NIC rather than off `Instant::now()` in the client, and a switch between them. **Phase 1 did not ask for this** — `PRD.md` §2 criterion 6 asks for `tools/w2w` on Linux with the §9 settings stated, and that is what closed. This row exists so the stricter reading is not quietly retired by the weaker one being satisfied **`[measured 2026-09-02]` the hardware on this box is better than expected and the cable is not plugged in.** `ethtool -T enp9s0` on the §9 desktop — Intel I211 Gigabit — reports **`hardware-transmit`, `hardware-receive`, `hardware-raw-clock`**, *provider qualifier: Precise (IEEE 1588 quality)*, and `/sys/class/ptp/` holds `ptp0` and `ptp1`. So the hard half of this row is already satisfied. `Link detected: no` — the box is on `wlp7s0` and Tailscale. **And the shape of the answer is not the one this row assumed.** It asked for a load generator on a second machine, which read as *a second Linux box with its own PTP clock*; it is not. A round trip whose **RX and TX hardware timestamps are both taken on THIS NIC** needs **one clock and no PTP synchronisation at all** — the other end only has to echo. So the generator can be **any** machine, macOS included, and macOS lacking Linux's `SO_TIMESTAMPING` does not matter because it is never asked to timestamp anything. What is needed is a **direct Ethernet cable** (auto-MDIX, no switch), a USB-C/Thunderbolt adapter on the Mac end, static addresses, and `tools/w2w` split into an acceptor and a generator. **WiFi is out, and not marginally**: `ethtool -T wlp7s0` reports `software-receive` only with `PTP Hardware Clock: none`, so there are no NIC timestamps to read on that path at all, and 802.11 contention puts RTT in the **milliseconds** against a 16-20 µs budget — the jitter alone would be tens of times the whole p99.9. **A GitHub runner is out on four independent grounds**: it is outbound-only in Azure and cannot reach this box without a tunnel (and a tunnel is a userspace hop with its own crypto — `tailscale0` reports software-only timestamping, which is the kernel saying so); WAN RTT dominates by three orders of magnitude; its NIC is virtualised with no PTP clock to expose; and §9's first row makes a guest a FAIL. **One arithmetic term is knowable in advance and is not measured**: at 1 Gb/s the wire alone is 8 ns a byte, so a ~240-byte frame is ~1.9 µs each way before PHY, driver or IRQ — the first real NIC figure may well make §8's 10-20 µs floor look optimistic, and that is a result rather than a problem. | §6's second wire-to-wire row |
| 41 | ~~**`scripts/bench.sh --strict` is red on the §9 desktop.**~~ — **CLOSED 2026-09-05, and neither red case was a regression.** `--strict` exits 0, three consecutive runs, read line by line: `timing over baseline 0 · cases w/o a baseline 0 · cases under the band 0`. **`presession, read and route` (+140%)**: the 2026-09-04 diagnosis stands and was re-run rather than trusted — replacing the two `field_value` calls ADR-0026 added with `None` reads **83.5 · 84.1 · 84.7 · 84.3 · 84.0**, median 84.1 against a baseline of 84.0. The plan's own step 1a hypothesis was **refuted** on the way: removing the two `registry lookup` cases moves the figure 191.8 → 186.8, not to 84, and the second direction the plan demanded shows the number follows *presence* (−5 ns) rather than *position* (<1 ns). **`encode ExecutionReport (template)` (+16%): not the encoder, and not ADR-0044 either.** `[measured 2026-09-05]` the baseline commit `bf798ea` still reads **240.0 ns** today, so machine and toolchain are exonerated; the bisect lands on **`4396d6d`**, which touches **no `crates/*/src/` file at all** — `git show 4396d6d -- crates/codec/src/` is empty — and moved the figure **+11.4%** by `include!`ing 150 lines into the bench binary. ADR-0044 is real and is **+4.5%**, arriving on a case already 12% over. Proven twice, the second time without the knob: forcing alignment takes HEAD 278.9 → 233.0, and adding **inert functions the encoder never calls** walks it **236.5 → 292.4 ns** across four layouts (4.0% under pinning, 14.6% without). **And the plan had ruled this commit out by reading its diff** — non-negotiable 10's *review of a diff catches almost nothing*, one line from the guard. [ADR-0049](docs/decisions/ADR-0049-bench-builds-pin-function-alignment-and-the-flag-is-read-back.md) (owner chose both halves: pin **and** widen to 1.15), `scripts/check-bench-alignment.sh` reading the flag back off the binaries — **23 of 23 own-crate symbols aligned pinned, 5 of 23 unpinned, reversal red at 2 of 11** — and all 24 baselines re-recorded under that build, `n = 20`, `pass 12 fail 0 unknown 1`. Write-up: [a-benchmark-that-measures-where-the-compiler-put-it](docs/reference/a-benchmark-that-measures-where-the-compiler-put-it.md), **`[to testing-skills]`**. **What is left of the plan that owns this row**: step 2 (item 39, the dictionary pass) and step 3 (item 34) | ~~`bench.sh --strict`~~ — the gate non-negotiable 10 leans on, green |
| 42 | ~~**The acceptor has no second opinion.**~~ — **CLOSED 2026-09-04.** [acceptor-interop](docs/plans/2026-09-03-acceptor-interop.md), steps 1–4; step 5 was optional and is **not done**, so item 38 stays open. `tools/interop` grew `--role acceptor` (`fixbolt::serve` in `standard`, a `Desk` handler, a readiness line printed by a thread that actually connected); `tools/interop/initiator.cpp` is a `libquickfix` `SocketInitiator` that scores seven steps **on the raw wire strings**, because QuickFIX drops a `PossDup` replay before the application sees it and the resend step asks for exactly that. `scripts/interop.sh` runs both directions in one invocation — **7 / 7 + 7 / 7** — and the `interop` CI job publishes four transcripts. **Three reversals, each red on its own step**: dropping the `11=` echo reddened `order` while the initiator direction stayed 7 / 7; the inverted resend range that was a **no-op in the other direction** reddened `resend` here, because the step names the two sequence numbers it wants; and renaming one grep'd step name failed the script while the binary still printed `PASS 7/7`. **Two findings came out of it, neither a defect in this engine**: the first run was red on `gapfill` because QuickFIX's own gap fill swallowed the `TestRequest` the step was waiting on ([a-gap-fill-can-swallow-the-question](docs/reference/a-gap-fill-can-swallow-the-question.md)), and the assembly check found that **this engine cannot originate an application message** — new item 46, [an-acceptor-that-can-only-answer](docs/reference/an-acceptor-that-can-only-answer.md). Two false greens in `tools/interop` itself are at [a-green-fraction-over-a-scenario-that-never-ran](docs/reference/a-green-fraction-over-a-scenario-that-never-ran.md) | the honesty of *fastest acceptor* as a claim |
| 43 | ~~**A resend gap-fills every application message older than the journal ring, silently, and a large resend ends the session.**~~ — **CLOSED 2026-09-04.** [resend-from-the-journal](docs/plans/2026-09-03-resend-from-the-journal.md), six steps, [ADR-0046](docs/decisions/ADR-0046-the-ring-is-the-resend-store-and-a-replay-goes-in-batches.md) as step 0. **All five parts of the row are answered**: `SLOTS` 8 → 4096 (≈2 MiB a session, `[measured 2026-09-04]` +2 195 456 bytes of RSS); `Journal::put -> bool` and `Journal::oldest`, neither with a default; two counters and two events, in **messages** not occurrences, and a fill over an administrative message deliberately not counted; a replay in batches of `Config::resend_batch`, so a resend larger than `TX` no longer ends the session; and `get` is one index and one comparison. **Three things it found that the row did not name**: the old `get` returned the **stale** message for a number reused after `Admin::SetNextOut` — a scan that took the first matching slot; plain `Session::tick` would have gap-filled the remainder of a replay through `NoJournal`, so it now stalls and `tick_with` continues; and `Engine::add` now allocates ~2 MiB on the engine thread, which `benches/alloc.rs` caught at `events-busy` 0 → 2000 and which `GUIDE.md` §6a0 and `best-practices-hft.md` §6 carry. **The reversal that matters**: `with_resend_batch(10_000)` reads *"the session ended on turn 1 while answering a resend"* **while `--test score` stays 59 / 59** — the corpus never asks for more than three messages, so only `crates/engine/tests/backpressure.rs` can see it | deployability of the acceptor for order flow |
| 44 | ~~**There is no message log.**~~ — **CLOSED 2026-09-04, all five steps, CI green 12 / 12 on `0e0b5f3`, run [`33861215461`](https://github.com/tmthang86/fixbolt/actions/runs/33861215461).** [message-log](docs/plans/2026-09-03-message-log.md). `FileLogPath` writes one text file, one line per message, **both directions including frames refused before the session judged them** — the class the journal cannot hold, because its key is `seq` and those frames never got one (`DESIGN.md` D14, `GUIDE.md` §6c). **Step 0 was added by the eng review and blocked everything**: an independent read asked why a second ring→writer→file stack exists beside `journal.rs`; the answer — the journal's key, `session` purity, and two durability policies one file cannot serve — was sound and had **never been written down**, so it had never been challenged ([why-the-message-log-is-not-the-journal](docs/reference/why-the-message-log-is-not-the-journal.md)). **The review also found the silence that mattered**: the plan wired the log into `*_with_recovery` only, and `serve` is the entry point `GETTING-STARTED` teaches — so `FileLogPath` in a file would have done nothing and said nothing. All five entry points take a log now. **Three defects this work found in itself, all in `docs/reference/`**: a commit claimed the inbound hook existed when it did not, because every step-2 test had been deferred; the first `Drop` test passed with `Drop` deleted, because a detached writer won the race it was really measuring; and two reversals were no-ops that reported success until each was made to prove its own patch applied ([a-background-thread-wins-the-race](docs/reference/a-background-thread-wins-the-race-your-test-was-measuring.md), `[to testing-skills]`). `benches/alloc.rs` is **24 cases**, not 21. **Step 4 came with it and is a separate thing worth naming**: the journal's on-disk format is version 1, a `FXBJ\\x01` header and a CRC32 per record, because a flipped byte in a v0 journal read back as a whole, correctly framed, correctly numbered record and would have been replayed to a counterparty as the message that was sent — torn-tail accounting catches a process *killed*, never a file *changed*. A bad checksum stops the read exactly as a tear does. **A file without the header stays v0 for ever**, and the compatibility test asserts against bytes committed from `df94c08`, the commit before the change. Note at the end of [ADR-0008](docs/decisions/ADR-0008-journal-is-a-trait.md) | the first dispute with a counterparty |
| 47 | ~~**`serve` hands out no handle at all: not a `Sender`, not an `Observer`, not an `Admin`.**~~ — **CLOSED 2026-09-05**, [ADR-0054](docs/decisions/ADR-0054-the-handles-are-made-before-the-engine-and-the-engine-adopts-them.md), [plan](docs/plans/2026-09-05-handles-through-the-front-door.md). `observe::Handles` is the one cell made **before** the engine, with `observer()`, `admin()` and `sender()` on it; `Engine::adopt` takes it and refuses a second; all ten `serve*` / `connect_and_serve*` functions take one as their last parameter. **`GETTING-STARTED.md:186` is true now** — `[measured 2026-09-05]` `end_to_end.rs::an_operator_stops_the_front_door_and_serve_comes_back` reads `sessions 1, said_goodbye 1, acked 1`, and `scripts/interop.sh` asks its acceptor role to stop rather than killing it. **Closing it found `connect_and_serve` was draining the caller's event ring** — `Observer::events` drains, so two readers share events, and the reconnect loop was one of them; invisible until the caller had a cell to be robbed of. It reads `Engine::logons()` now. **The shape the item guessed at was wrong**: it said *"a single `on_ready(Handles)` callback is the likelier answer"*, and that is blocked by the layering — `Application` is a `fixbolt_session` trait and non-negotiable 2 keeps that crate unable to name `engine::Handles`. Handing the cell **in** needs no callback at all. **Left open on purpose**: `shard::serve_sharded_hft` takes no handles — N shards are N cells and a `ConnId` is unique only within one | closed |
| 48 | ~~**A deployment cannot recover its own outbound sequence number, and the naive way looks right.**~~ — **CLOSED 2026-09-05**, [ADR-0053](docs/decisions/ADR-0053-the-journal-answers-two-questions-and-the-second-is-a-number.md), [plan](docs/plans/2026-09-05-a-journal-that-knows-the-numbering.md). `Journal` gained `mark_out` / `highest_out`, the session tells it from every path out of `received_with`, `tick_with` and `send_application`, three entry points gained `_with` twins, and `Resumed::from_journal` does the arithmetic once so no example derives it by hand. `[measured 2026-09-05]` the `SIGTERM` interop scenario reads `next_out ok sent up to 34=3 before the kill, came back at 34=4, wanted 34=4` where it read `known_gap expecting 4 but received 3`. **Two things this item asserted were wrong and both mattered.** **(1) It said it could not close before item 47** — true of one imagined fix, reading `next_out` from an `Observer`, and that fix is wrong at the root: an observer knows the number only when asked, so a Heartbeat between the last poll and the crash leaves it short anyway. The durable source present at `recover` time is the journal. The dependency stood for four days as though it were a property of the problem. **(2) The `SIGKILL` scenario that passed was passing because of `HeartBtInt=30`**, chosen so no Heartbeat could fall in its window — the exact condition this item is about, removed by the fixture. A third scenario at `HeartBtInt=1` now removes it. **And closing it exposed the mirror defect in the inbound direction**: the counterparty's `Logout` was consumed and never marked, because `received_with` returned before `mark_in` on exactly that path, so a resumed session asked for a message it already had. Found by the gate, not by this repository. Write-up updated: [a-journal-holds-messages-not-numbering](docs/reference/a-journal-holds-messages-not-numbering.md), **`[to testing-skills]`** | closed |
| 49 | **~2 770 ns of the application round trip is unattributed, two of the four candidates are dead with numbers, and the survivor holds almost all of it.** `[measured 2026-09-05]` `--path app` costs **3 898 ns** more than `--path admin` at p50 on the §9 desktop. Committed benchmarks now account for **~1 128 ns, 28.9%**: inbound parse ~+60, **the dictionary pass +679** (item 39), dispatch +9, the application's own `Validation::NONE` re-parse +114, its template encode +233, **the kernel copying a bigger payload +24.5**, **`Journal::put` of the reply +8.9**. **Retired:** the payload term is **0.9%** of what it was nominated for — read off an 8 → 8192 byte lever, because the two real sizes differ by tens of ns inside 12 600 and their direct difference changed sign across three repetitions (−4, +13, +46). `Journal::put` is **8.9 ns**, confirmed in situ by sweeping the ring 4 KiB → 2 MiB for a 1.5% non-monotone spread. **Still open, and now holding almost the whole remainder:** the engine's framing and read-buffer management, which no bench isolates. Also unsubtracted in either direction: the session's own `Heartbeat` serialise, which has no committed case. **Twice now the largest named candidate has not been the answer** — the dictionary pass is 17.4% of the gap and the payload term 0.9%. [ADR-0052](docs/decisions/ADR-0052-two-candidates-are-retired-with-numbers-and-a-baseline-disagrees-with-its-neighbours.md) | `DESIGN.md` §8's bottom line |
| 50 | ~~**Two committed `parse` benchmarks timed a message the parser rejects.**~~ — **CLOSED 2026-09-05, in the commit that found it.** `[measured 2026-09-05]` `crates/codec/benches/parse.rs` declared `10=098` on a `NewOrderSingle` whose checksum is 097, and `9=49` on a `Heartbeat` whose body is 51. Both checks live at the end of `parse_into`, so the `NewOrderSingle` had already computed its checksum and only failed the comparison (~+1% when corrected, inside its own spread) — but **the body-length check returns before the checksum block, so `parse Heartbeat (validated)` had never summed its own 51 bytes**: 56.3 → **60.9 ns**, and the first corrected run read 64.2 against a 61.9 ceiling. **Nothing in the suite could see it**: the result is discarded by design in a bench, the figure was stable to 1%, and the baseline it was compared against came from the same fixture. It was found by a **different** bench, written for item 39, that asserts its own fixture is valid before timing anything — and that assert is now in `parse.rs` too, proven by reversal (restoring `10=098` panics `Err(BadCheckSum)`). `crates/engine/benches/dispatch.rs` carries the same literal and is unaffected: it never parses. Checked, not assumed. Write-up: [a-benchmark-parsed-a-message-the-parser-rejects](docs/reference/a-benchmark-parsed-a-message-the-parser-rejects.md), **`[to testing-skills]`** | ~~a published 56.3 ns~~ — 60.9 ns, and the fixture is asserted before it is timed |
| 51 | **A TCP loopback write costs thirty-two bare syscalls on this box, and nothing here explains it.** `[measured 2026-09-05]` `getppid` **170.5 ns**; a `pipe` write-and-read of 8 bytes **778.9 ns**; a UNIX `socketpair` **1 924.9 ns**; TCP `127.0.0.1` **10 228.6 ns**, of which the write alone is **5 450 ns**. **Not the read waiting** — a non-blocking version reads **0.00 `EAGAIN` per operation** and costs the same. **Not scheduling** — `taskset` agrees to 0.1%. **Not this project's code** — thirty lines of `std::net` under `rustc -O`. **Candidates, neither tested and neither claimed:** `nf_tables`, `nf_conntrack`, `nft_compat`, `xt_MASQUERADE` are all loaded and `tailscale0` is up, so every loopback packet traverses chains Tailscale and Docker installed; and Zen 2 carries retpolines, `IBPB: conditional`, `Safe RET` and `IBPB before exit to userspace`. **Why it matters:** every wire-to-wire figure this project has published went over `127.0.0.1`, `DESIGN.md` §8's floor is 10–20 µs, and if four syscalls are ten of them then the budget's largest term is one nothing here owns — and item 40's NIC figures will not be comparable to what they are about to be compared against. **Testing either candidate changes the owner's machine** (an `iptables -t raw -j NOTRACK` rule, or a boot with `mitigations=off`), which is their call and not a benchmark's. [a-loopback-write-costs-thirty-two-syscalls.md](docs/reference/a-loopback-write-costs-thirty-two-syscalls.md) `[to testing-skills]` | `DESIGN.md` §8's floor, and item 40 |
| 52 | **Recording a baseline changed the baseline, because the table is compiled into the binary.** `[measured 2026-09-05]` `journal put, 191 bytes, one slot` read **8.2 ns** over 20 clean runs — eighteen of them between 8.1 and 8.3. The seventeen new lines were appended to `benches/baselines.tsv`, and the very next `bench.sh --strict` went **red on that case at 6.4**. On the binary that now exists it reads **6.3, eight times, never anything else**, while its two siblings in the same binary do not move (8.9 and 5.3–5.4). The file is `include!`d into `harness.rs`, so **appending the results changed the binary the results came from**, and one small case moved **23%**. **Alignment was pinned throughout** — ADR-0049's flag was in force, read back off all sixteen binaries, and did not prevent it. Correcting the value converges immediately (8.2 → 6.3, rebuild, reads 6.3), so it is a fixed point and not a chase. **What is open:** whether the ladder rule can mean anything for cases this small, and whether the table should stop being compiled in — a build script writing it to `OUT_DIR` would move the problem rather than remove it, since that file is an input too. For now one line carries **6.3 / 1.35 / n = 8** against neighbours that all say n = 20, and the disagreement is deliberate. Third appearance of the layout hole, and the first that is self-referential. [recording-a-baseline-changed-the-baseline.md](docs/reference/recording-a-baseline-changed-the-baseline.md) `[to testing-skills]`, [ADR-0052](docs/decisions/ADR-0052-two-candidates-are-retired-with-numbers-and-a-baseline-disagrees-with-its-neighbours.md) | ADR-0016, ADR-0031, ADR-0049 |
| 45 | **The order of everything after wave A, so it does not live only in a chat.** `[decided 2026-09-03]` **B** — `settings-for-both-roles` (initiator keys, `ResetOnLogon/Logout/Disconnect`, `LogonTimeout`/`LogoutTimeout`, per-session validation knobs, `MaxMessageSize`, a registry that sees the `Logon` for 553/554/96, a `35=j` helper on `Reply`), `seq-resync-789-369`, `timestamp-micros`; then TLS (ADR-0018 unblocked it). **C** — `the-second-linux-desk`: item 40, `SO_BUSY_POLL`/`SO_PREFER_BUSY_POLL`/`SO_INCOMING_NAPI_ID`, `w2w --interval` for cold-cache figures, `mlockall` read back from `/proc/self/status`, alloc cases for `FileJournal` and `FileLog`, items 39 and 41. **D** — `const-templates-in-dict` (item 34) and `deterministic-simulation` (fault-injecting harness plus a session fuzz target), both before phase 2 because two encodings pay for them twice. **Then phase 2**, opened by one ADR: the encoding trait, a dictionary chosen at `Logon` by the registry, `docs/internals` — **and the consolidation pass of `DESIGN.md`, deliberately not before.** `[decided 2026-09-04]` The review asked whether the design needs rewriting; it does not: no decision in D1–D13 is reversed, the findings are defaults, evidence and missing features, and each plan amends the section it touches. What the page does need is editorial — 1 258 lines that read as layered `[measured]` paragraphs, so D7 takes four *As built* passes to say what it is today. Doing that now would be redone by waves A–D; the encoding ADR is the one moment the architecture genuinely changes (D2, D3, D9 presuppose tags on the wire), so that is when §4 is restated to hold only the current decision, and the history moves to `measured-costs.md` and the ADRs. **Deliberately not planned**: `io_uring` (needs the NIC-to-NIC number first; ADR-0012 open question 3), kernel bypass (item 14), `SendRedundantResendRequests`, `RefreshOnLogon`, a database store, `DefaultApplVerID` before FIXT. `[2026-09-04]` **Drafts exist for all of B, C and D, at the owner's request, and every one carries the same rule in its header: it is not a plan until its turn comes.** When a wave is reached, the draft's *Những gì đã biết chắc* is re-verified against the code of that day (waves A and B change `journal.rs`, `observe`, `settings`, the resend loop), the plan is revised, and only then does it move to *Chờ duyệt* — a draft is a starting point, not an approval. B: [settings-for-both-roles](docs/plans/2026-09-04-settings-for-both-roles.md), [seq-resync-789-369](docs/plans/2026-09-04-seq-resync-789-369.md), [timestamp-micros](docs/plans/2026-09-04-timestamp-micros.md), then [tls](docs/plans/2026-09-04-tls.md). C: [the-second-linux-desk](docs/plans/2026-09-04-the-second-linux-desk.md). D: [const-templates-in-dict](docs/plans/2026-09-04-const-templates-in-dict.md), [deterministic-simulation](docs/plans/2026-09-04-deterministic-simulation.md). **One fact surfaced while drafting and is not yet a test**: `clock::parse_utc` accepts 17 and 21 bytes only (`crates/session/src/clock.rs:40`), so a counterparty's microsecond `52=` reads as `None` — what the session does next is the first step of the micros draft. **`[amended 2026-09-05]` Two changes, both recorded here rather than in a chat.** (a) **Item 46 moves ahead of wave B and becomes its own plan**, [an-engine-that-can-speak-first](docs/plans/2026-09-05-an-engine-that-can-speak-first.md), *Chờ duyệt* — the draft order put it *inside* wave B beside `settings-for-both-roles`, and that is the wrong way round: an acceptor that cannot originate is a product gap while the rest of wave B is knobs, and `settings-for-both-roles`'s own `35=j` helper is one of the four things item 46 says cannot be sent, so building the helper first delivers it with no door. Wave B is otherwise unchanged and follows. (b) **Wave C grows step 7**, a head-to-head against `matthart1983/nanofix` on the §9 desktop — same harness, same machine, same §9 rows, measured against `standard` because that engine is thread-per-connection with a blocking `recv` (`server.rs:117`, `transport_tcp.rs:23–37`) and pairing it with `hft` is the mode mixing non-negotiable 4 refuses. The result goes to `measured-costs.md` as this repository's own measurement; `prior-art.md`'s row stays a claim and points at it. **`[amended 2026-09-05]` (c) Items 48 and 47 move ahead of wave B, as two plans, in that order** — [a-journal-that-knows-the-numbering](docs/plans/2026-09-05-a-journal-that-knows-the-numbering.md) then [handles-through-the-front-door](docs/plans/2026-09-05-handles-through-the-front-door.md), both *Chờ duyệt*. Same argument as (a): an initiator that cannot survive a clean logout and a front door that cannot be stopped are product gaps, and wave B is knobs. **And 48 does not wait for 47 after all**: item 48's own text said the right number was unreachable without an `Observer`, but an `Observer` is the wrong source — it knows the number only when asked, so a Heartbeat between the last question and the crash still leaves it short. The durable source that is present at `recover` time is the journal, which already records the inbound count per message (ADR-0017) and not the outbound one; the fix is a `mark_out`, and it needs nothing from item 47. `settings-for-both-roles` is re-verified only after both close: its `ResetOn*` keys are about numbering, which 48 makes durable, and its entry points are the ten signatures 47 changes — re-verifying it before would be done twice. **`[2026-09-05]` items 48 and 47 are both closed** (ADR-0053, ADR-0054), so **wave B is next**, opening with `settings-for-both-roles` — whose re-verification was deliberately held until both landed: its `ResetOn*` keys are about numbering, which 48 made durable, and its entry points are the ten signatures 47 changed | nothing; it is the map |
| 46 | ~~**This engine cannot originate an application message.**~~ — **CLOSED 2026-09-05**, all five steps. [an-engine-that-can-speak-first](docs/plans/2026-09-05-an-engine-that-can-speak-first.md), [ADR-0048](docs/decisions/ADR-0048-an-engine-that-can-speak-first-has-two-doors.md), `DESIGN.md` D15. **One primitive, two doors**: `Session::send_application` already existed and already left `34=` and `52=` with the session, so nothing in the session machine changed; what was added is `Handler::on_logon` (engine thread, empty default body, bounded by `MAX_ON_LOGON`) and a `Send + Sync` `Sender` riding the `Arc` `Observer` and `Admin` already ride. **The gate is not one of this repository's own runners**: `scripts/interop.sh`'s acceptor `news` and `resend` steps were red *by design* and the exemption is now gone from `tools/interop`, so a red there is a red. `[measured 2026-09-05]` the two roles pointed at each other read **PASS 7/7**, from 5/7. Three new `benches/alloc.rs` cases — `origin-idle`, `origin-busy`, `logon-first` — all **0**, and **2000 / 16 under an injected `format!`**. `[measured 2026-09-05]` removing the door leaves `--test score` at **59 / 59**, which is the corpus saying in its own words that it cannot see this. **What it cost is in ADR-0048's *Consequences* and is not small**: `crates/session` carries an `Application::on_logon` nothing in that crate calls, plus three `Config` getters — the plan's Sửa 1 records why, and the three rejected alternatives `[2026-09-05]` **and door 2 is reachable from the front door now** — item 47 gave every `serve*` a `Handles`, so a deployment can hold a `Sender` without depending on `fixbolt-engine` and rewriting the loop ([ADR-0054](docs/decisions/ADR-0054-the-handles-are-made-before-the-engine-and-the-engine-adopts-them.md)). | an acceptor that can only answer is not one a desk can run |
