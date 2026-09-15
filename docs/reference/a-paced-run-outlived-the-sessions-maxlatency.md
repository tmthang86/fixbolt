# A paced run outlived the session's MaxLatency

> `[measured 2026-09-15]` on the `DESIGN.md` §9 desktop, running boot B step B4 of
> [plans/2026-09-04-the-second-linux-desk.md](../plans/2026-09-04-the-second-linux-desk.md) — the
> "latency at 3 a.m." arm, `hft` admin, loopback, `--interval` 1 s. The plan's `MESSAGES=120
> WARMUP=5` at 1 s pacing failed procedure 1 at run 1: the engine answered sequence 122 with
> `35=3`, `373=10` *SendingTime accuracy problem*, and `w2w` exited 101 (`expected 35=0`).
>
> **`[to testing-skills]`**

## What happened

Step B4 runs `tools/w2w` with `--interval 1s`, spacing sends by spinning so the generator's core
is busy for the whole wait between them (`docs/hft-playbook.md` §6, item 3). At `WARMUP=5
MESSAGES=120`, run 1 of procedure 1 did not complete: the engine rejected message 122 (sequence
122, past the 5 warmup messages) with

```
35=3  373=10   SendingTime accuracy problem
```

and `w2w` exited **101**, its own check reading `expected 35=0` against the `35=3` it received.
The run was re-taken with `WARMUP=5 MESSAGES=100` (105 s total instead of 125 s) and went green;
that is the figure boot B publishes for the 1 s arm
([DESIGN.md](../DESIGN.md) §8, *Boot B*, the paced table — "Latency at 3 a.m.").

## Why

`tools/w2w` renders every message — `52=` (`SendingTime`) included — **before the clock starts**
(`tools/w2w/src/main.rs`, the comment "Every message is rendered before the clock starts"). At
`--interval 1s`, message *N* is sent roughly *N* seconds after the run began, but its `52=` was
stamped at render time, at the start of the run. Once *N* exceeds the session's `MaxLatency` —
120 s, QuickFIX's default (`crates/session/src/lib.rs`) — the `52=` a paced run's later messages
carry is older than the counterparty will accept, and the engine is correct to reject it: the
125 s run asked the engine to judge a `SendingTime` two minutes stale for no reason the protocol
recognizes.

**The engine's rejection is the right behaviour, not a defect.** The bug, such as it is, is in the
benchmark's own parameters: `interval × (warmup + messages)` must stay under `MaxLatency`, and
nothing checked that before this run.

## The rule

**A pre-rendered paced run must finish inside the counterparty's `SendingTime` tolerance:**

```
interval × (warmup + messages) < MaxLatency
```

For `MaxLatency` 120 s and `--interval 1s`, `warmup + messages` must stay under 120 — the plan's
own 120 + 5 = 125 was already over it before a single message moved.

## What guards it

**Nothing yet.** This is an unmet `CLAUDE.md` §4 obligation: a protocol trap, once found, is
supposed to leave a regression test behind it, and this one has not.

The guard that would close it: a refusal inside `tools/w2w` when `--interval × (warmup +
messages)` ≥ 120 s — computed from the flags it is already given, before the run starts, with a
message naming which of the two would need to shrink — plus a test exercising that refusal.
Owned by `STATUS.md`, open items.
