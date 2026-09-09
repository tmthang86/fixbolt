# ADR-0057: Sub-millisecond time arrives beside the tick, not inside it

- **Status:** Accepted — 2026-09-09, **approved by the owner on the day it was written**
- **Date:** 2026-09-09
- **Plan:** [timestamp-micros](../plans/2026-09-04-timestamp-micros.md) — this page **is** that
  plan's gate. Half A is closed; half B opened on this approval. **Sửa 2 of that plan was written
  before any code moved**, and it revises three of the plan's own lines against this decision —
  the const generic, a trap that does not exist in this codebase, and the second tick door
  decision 1 requires
- **Constrained by** [D13](../DESIGN.md) (`Tick` counts milliseconds from year zero),
  non-negotiable 2 / [ADR-0002](ADR-0002-engine-library-split.md) (the session is pure — time
  enters as a tick and by no other door), [D9](../DESIGN.md) (`SendingTime` is patched from a
  cache, not formatted per message), and
  [ADR-0056](ADR-0056-the-application-is-told-what-the-session-owns.md) (`Header` is the whole
  of what the session can tell a reply)
- **Supersedes nothing, and reverses nothing.** D13 stands after this decision; §2 below is the
  half of it that says so out loud

## Context

### What this engine does today, and it is half a feature

`[2026-09-08]` half A of `timestamp-micros` made the **receive** side read a `UTCTimestamp` at
four widths — 17, 21, 24 and 27 bytes (`crates/session/src/clock.rs:37–40, 63`). Before it, a
valid microsecond `52=` from a European venue read as `None`, became
`Refusal::BadSendingTime`, and was answered with a silent hang-up.

The **send** side was not touched and cannot be. `codec::TimestampCache` is
`TIMESTAMP_LEN = 21`, a `const` (`crates/codec/src/timestamp.rs:19`), and every message this
engine emits carries 21 bytes at every configuration. A venue that requires microsecond `52=`
*from* this engine is still not served.

### Time enters this session in exactly one place, and it is milliseconds

| Fact | Source |
|---|---|
| `Session::tick` / `tick_with` are the only entry points that carry time | `crates/session/src/lib.rs:1845, 1865` |
| `Session::received` / `received_with` take **no time at all** | `crates/session/src/lib.rs:2063, 2070` |
| `self.now_ms` is assigned in one place, from the tick | `crates/session/src/lib.rs:1921` |
| `52=` is formatted from `self.now_ms`, at **two** call sites — the session's own messages and the originate path | `crates/session/src/lib.rs:2489, 3289` |
| `SystemClock::now_ms` truncates the reading with `as_millis()` | `crates/engine/src/clock.rs:35` |

So there is no path by which a finer instant reaches the formatter, and inventing one inside
`session` would need a clock in a layer non-negotiable 2 forbids to have one. **That is why
this is an ADR and not a step in a plan**: the plan cannot pick a door that does not exist.

### The fact that reframes the question, and it was not in the plan

**The engine reads the clock once per turn.** `Engine::turn` takes one reading
(`crates/engine/src/lib.rs:950`) and hands that same value to every connection;
`Conn::turn` states the intent — *"One clock read per turn, so every line written on this pass
carries the same millisecond"* (`crates/engine/src/conn.rs:292`). And because `received` takes
no time, a reply is stamped with the tick that preceded it **in the same turn**.

**So `52=` names the instant the engine turn began, not the instant of the `send`.** That is
already true today and nobody had to care, because a millisecond is coarser than a turn.
Widening the field to microseconds makes the gap expressible, and publishing six digits without
saying what they are the resolution *of* would claim a precision this engine does not have.
This is the same failure mode as alternative (c) below, only quieter.

### What the interop oracle actually does — read from source, at the SHA this repository pins

`[researched 2026-09-09]` `quickfix/quickfix` at `386ce46e`, the commit pinned in
`scripts/fetch-quickfix-assets.sh`. Read from raw source into a scratch directory; **nothing
fetched into `vendor/` and nothing committed** (ADR-0001).

| Fact | Source |
|---|---|
| `TimestampPrecision` is a real key, read as an **integer 0–9**; out of range throws | `SessionSettings.h:133`, `Session.h:167–174`, `SessionFactory.cpp:219–221` |
| Its default is **3** | `Session.cpp:68` |
| `MillisecondsInTimeStamp` is the legacy boolean, mapping to 3 or 0 | `Session.h:159–166` |
| **FIX 4.4 supports sub-second stamps** — `beginString >= FIX.4.2` | `Session.h:175–184` |
| The parser accepts **any length from 17 to 27 bytes**: `.` at index 17, then 0–9 digits | `FieldConvertors.h:492–596` |
| The serialiser writes `17 + 1 + precision` bytes | `FieldConvertors.h:465–489` |

Three things follow, and they do not point the same way.

**1. `scripts/interop.sh` can be an oracle for this feature in both directions.** The C++ end
can be told `TimestampPrecision=6` and will both send and accept 24-byte stamps on a FIX.4.4
session. That is worth naming, because it is not the usual case: ADR-0056 had to record that
`369` **can never** be judged by that oracle, since QuickFIX C++ does not implement it. Here the
oracle is real, and half B's gate should be it rather than this repository's own tests alone.

**2. The key's shape is an integer, not the enum this repository wrote down.** The plan's
*Những gì đã biết chắc* table carries the row `TimestampPrecision=SECONDS|MILLIS|MICROS|NANOS`,
cited to `reference/prior-art.md` — **a spelling that document does not contain**. It says only
*"`TimestampPrecision` beyond milliseconds"*. The enum belongs to QuickFIX/**J**. This is the
failure `prior-art.md` already records at its own line 168 — *a family name on a feature list is
a claim about whichever member you happened to read* — repeated inside this repository by a
citation that looked sourced. The draft predates that correction (2026-09-04 against
`[corrected 2026-09-06]`), which is not the interesting part: **the row survived the
re-verification of 2026-09-08**, two days after the lesson was written down. `prior-art.md` gains
the measured C++ facts in the same commit as this page.

**3. This engine is stricter than the oracle on receive, and the gap is not empty.** `parse_utc`
accepts 17, 21, 24 and 27 bytes; QuickFIX C++ accepts every length from 17 to 27. So a QuickFIX
C++ end configured `TimestampPrecision=1, 2, 4, 5, 7` or `8` sends a stamp this engine refuses —
and before a Logon that refusal is a silent hang-up
(`crates/session/src/lib.rs:2846–2856`), which is exactly the defect half A was written to
close, still reachable through the oracle's own configuration file. **Not this ADR's to decide**:
it is a receive question and half A is closed. Registered as `STATUS.md` open item 59.

### The three options the plan named, and a fourth it did not

| | Where the sub-millisecond part comes from | What it costs |
|---|---|---|
| (a) | `Tick` carries microseconds | Reverses D13's unit. Everything else in the layer — skew, schedule, heartbeat, timeouts, journal — is milliseconds and has no reason to change |
| (b) | A second number, carried beside `now_ms`, consumed only by the formatter | The session stays pure and every unit stands. The objection: a second notion of time in a layer whose design rests on there being one |
| (c) | Emit 24 bytes with `000` in the last three | Cheapest, works today, **and is a lie about resolution** |
| (d) | **Read the clock once per message** rather than once per turn | The only option where the stamp names the send instant. Puts a clock read where D9 put a cached format, at a cost this repository has not measured |

## Decision

### 1. Option (b), in one specific shape: one instant, split at the millisecond, delivered in one call

The sub-millisecond remainder reaches the session **beside `now_ms`, in the same call, derived
from the same clock reading**. The engine splits it at the edge — which is precisely what D13
already makes the engine's job for the year-zero conversion, so this adds no new
responsibility to a layer that did not have it.

**The shape matters more than the label, and the objection to (b) is answered by the shape
rather than argued away.** *"A second notion of time"* is a real risk if the two numbers can be
set independently: a `set_fraction()` called at a different moment than the tick would print a
remainder belonging to some other millisecond, on a message stamped with this one, and nothing
in the type system would say so. Delivered in one call, from one reading, they cannot disagree.
Half B's plan must build it that way; a design in which the remainder has its own setter is a
different decision and needs a different ADR.

`Session::received*` still take no time. The remainder is tick-scoped, exactly as `now_ms` is.

### 2. D13 stands. Every unit in this repository remains milliseconds

Skew, schedules, heartbeats, logon and logout timeouts, the journal, the message log, reconnect
backoff and the observer's events keep their unit and their names. `[measured 2026-09-09]` an
`_ms`-suffixed identifier occurs **396 times across 42 files** under `crates/` and `tools/`;
this decision touches the formatter and leaves the other 395 alone.

No comparison against a parsed `52=` changes either: `parse_utc` still returns milliseconds
(`crates/session/src/clock.rs:63`), and the remainder is never compared to anything — it is
formatted and nothing else. That is the property that makes this cheap, and it is the property
half B's review has to check rather than assume.

### 3. The stamp is the turn's instant, and the documents say so rather than leaving it to be discovered

`Header::stamp` stays a `&[u8]` and **its length becomes authoritative**; its rustdoc stops
promising 21 bytes (`crates/session/src/lib.rs:223–225`). A reply copies what it is handed.

`GUIDE.md` gains the sentence the type system cannot enforce: **the stamp names the instant the
engine turn began**, and the distance from there to the `send` is that turn's own work. A
deployment that needs the send instant itself is asking for alternative (d), which is an open
question below and not a setting.

### 4. The default is milliseconds, and 21 bytes stay the default wire

Two reasons, and the second is the load-bearing one:

- It agrees with the oracle's default of 3 (`Session.cpp:68`).
- **A venue that accepts only 21 bytes rejects 24.** A default that widens would break working
  deployments to serve the ones that asked. Opt-in is the only safe direction here.

Every byte this engine emits today stays byte-identical when the key is absent, which is also
what lets `benches/serialize.rs` and the 59 definitions be read as a no-regression statement.

### 5. What this ADR deliberately does not decide

**The formatter's parameterisation** — `TimestampCache<const FRAC>` as the plan drafted, or a
runtime field — is half B's plan, under one constraint stated here because it is the constraint
that could be forgotten: whatever is chosen is measured against
`benches/baselines.tsv:141` — `SendingTime from the cache`, **4.9 ns**, AMD Ryzen 7 3700X,
n = 20, `[2026-09-05]` — **on that machine and in that way**, and the number is published with
its machine (non-negotiable 10). A const generic and a well-predicted branch are both plausible
and the benchmark decides, not this page.

**The key's name and value spelling** is likewise the plan's, with `docs/CONFIGURATION.md`. What
this page contributes is the finding that the oracle's key is `TimestampPrecision` taking an
**int**, so a `.cfg` shared between the two ends is a real consideration and not a hypothetical.
It would be the 26th key (`crates/engine/src/settings.rs:139–165` holds 25 today).

## Alternatives rejected

### (a) `Tick` carries microseconds

Range is not the problem — year 9999 in microseconds since year zero is about `3.2 × 10^17`,
comfortably inside a `u64`. The problem is that a unit change touches **every comparison in the
layer**, and a comparison that is wrong after it is silent: a heartbeat that fires a thousand
times too often is loud, but a skew bound of `120_000` read as microseconds is a 120 ms window
that rejects almost nothing and looks like it is working.

Against that: it buys nothing (b) does not. The formatter is the only consumer of the finer
number. Paying 42 files for one call site is the trade this decision refuses.

### (c) 24 bytes with `000` in the last three digits

The cheapest option and the one this decision most wants to name, because it would pass every
test in this repository. MiFID II RTS 25 requires a clock traceable to UTC at microsecond
granularity for high-frequency trading; sending `.123000` to a counterparty that is *measuring*
divergence is worse than sending `.123`, because `.123` claims millisecond resolution truthfully
and `.123000` claims microsecond resolution falsely. Declined on the ground ADR-0045 declined
SIMD: the cheap thing does not do the job the feature exists to do.

### (d) Read the clock once per message

The honest maximum: the stamp would name the send instant rather than the turn's start.

**Rejected for now, not forever, and the reason is that nobody here has measured it.** It puts a
clock read on the path where D9 deliberately put a cached format costing 4.9 ns. `clock_gettime`
is a vDSO call rather than a syscall, so open item 51's measured `getppid` at 170.5 ns says
nothing whatever about its cost — quoting that number here would be exactly the error
non-negotiable 10 exists to prevent. A change to the serialise row of `DESIGN.md` §8 needs a
number before it needs an argument. What would settle it: a Criterion arm beside
`SendingTime from the cache`, on the §9 desktop, in the same run. Carried as open question 2.

### Widen only the seven messages the session generates itself, leaving `Header::stamp` at 21

Would avoid the public API change in decision 3. Rejected: it puts **two precisions on one
session's wire** — Heartbeats and Logouts at microseconds, application replies at milliseconds —
and a desk reconciling that capture would have a question nobody could answer well. ADR-0056
already established that a reply's header is the application's to write; this decision keeps
that seam consistent rather than making it precision-dependent.

## Consequences

### Good

- **The gate on wave B plan 3 half B opens.** Steps B1–B5 can be written against a decided time
  source instead of a guess.
- The session stays pure, D13 stands, and 395 of the 396 `_ms` identifiers do not move.
- **The feature gets a real second opinion.** `scripts/interop.sh` can judge both directions,
  which `369` never could — so half B is gated by another engine and not only by tests written
  in the same repository as the code.
- The receive/send asymmetry against the oracle is now written down (open item 59) instead of
  waiting to be found by a counterparty.

### Bad, and none of it is free

- **It is a public API change with a failure the compiler cannot see.** `Header::stamp` goes from
  a documented 21 bytes to a slice whose length is the truth. An application written as
  `out[..21].copy_from_slice(hdr.stamp)` still compiles and produces a body-length failure four
  bytes later — the very sentence already in `on_message`'s rustdoc, now reachable a second way.
  `GUIDE.md` and `CHANGELOG.md` in the same commit, and it is a `GUIDE.md` constraint precisely
  because the type system cannot hold it.
- **The engine's `Clock` trait changes, and `ManualClock` with it** — every test that drives
  time. The acceptance corpus is one of them.
- **The 59 definitions cannot see this feature.** 0 of the 59 carry a stamp wider than 21 bytes.
  Running them green says *nothing regressed*; it does not say the feature works, and half B's
  evidence must not be written as though it did.
- **A resolution claim this engine has to be able to defend.** Decision 3 makes the stamp the
  turn's instant; defending that needs a measured bound on turn work, on the §9 machine — and
  `[2026-09-05]` that machine is two reboots from §9, not one command.
- **A second number in the session's hot state**, carried through every `_with` twin of the tick
  and through both formatter call sites (`lib.rs:2489` and `:3289` — the plan named one).
- One more configuration key, and one more row in `docs/CONFIGURATION.md` that must agree with
  the code in both directions.

## Open questions

1. **No real capture of a venue sending a 24- or 27-byte `52=` exists here.** `STATUS.md`'s
   *Not proven* already records it. This feature is built from the FIX 5.0 SP2 EP spec and from
   MiFID II RTS 25, not from a byte anybody caught — and §7's *real captures over invented
   messages* is unmet for it. What would settle it: a UAT gateway capture, which cannot be
   committed here (ADR-0001) but can be run against.
2. **Alternative (d), the per-message clock read.** Settle with a Criterion arm beside
   `SendingTime from the cache` on the §9 desktop before deciding, not by argument.
3. **`TimestampPrecision` as an int 0–9 like the oracle, or as four named values?** An int makes
   1, 2, 4, 5, 7 and 8 expressible in a configuration file this engine cannot emit — a
   configuration error at load is honest and a silent clamp is not. Named values cannot express
   them but disagree with the file a QuickFIX C++ end reads. Half B's plan decides, with
   `docs/CONFIGURATION.md`.
4. **Open item 59** — the six precisions the oracle can be configured to *emit* and this engine
   refuses, silently before a Logon. (Its *parser* is looser still: it takes any length 17–27,
   seven of which this engine rejects, but its serialiser only ever writes `17 + 1 + precision`.) A receive-side question, needing its own plan under Rule
   Zero; recorded here because this page is where it was found.
