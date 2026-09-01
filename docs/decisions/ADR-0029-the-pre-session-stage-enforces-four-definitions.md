# ADR-0029 — The pre-session stage enforces four acceptance definitions, and the identity rule moved a stage earlier

> **Status:** **Accepted — 2026-09-01.** **Amends
> [ADR-0022](ADR-0022-the-pre-session-stage-enforces-two-definitions.md)**, whose count of
> two is now four. Everything else in ADR-0022 stands, including its argument, which this
> decision reuses without change.
>
> §5 forbids editing an accepted ADR's substance, so this arrives as its own decision.
> **Like ADR-0022, it exists because a test found it** — and unlike ADR-0022, the test found
> it only after the test itself was repaired. That is the part worth keeping in the record.
>
> **Accepted by standing delegation**, `[2026-08-30]`.

- **Date**: 2026-09-01
- **Deciders**: Tran Manh Thang
- **Related**: [ADR-0022](ADR-0022-the-pre-session-stage-enforces-two-definitions.md),
  [ADR-0026](ADR-0026-a-counterparty-registry-in-the-pre-session-stage.md),
  [ADR-0020](ADR-0020-a-pre-session-stage-owns-the-socket-until-logon.md),
  `crates/engine/tests/shard_wire.rs`, `CLAUDE.md` §2 non-negotiable 3 and §10,
  [plans/2026-09-01-counterparty-registry.md](../plans/2026-09-01-counterparty-registry.md) step 2,
  [reference/a-counter-that-must-be-remembered-is-not-a-counter.md](../reference/a-counter-that-must-be-remembered-is-not-a-counter.md)

## Context

[ADR-0026](ADR-0026-a-counterparty-registry-in-the-pre-session-stage.md) put a counterparty
registry in the pre-session stage: on a `Logon`, `Registry::lookup` decides which
configuration serves the identity, and `None` refuses the connection in silence.

**That moves a rule.** Before it, the stage let every identity through and `Session` refused
the wrong ones with `Refusal::WrongSenderCompId` / `WrongTargetCompId`. Now the stage
answers first, and a socket whose identity nobody configured is gone before an engine has
seen it.

Two acceptance definitions land on exactly that path:

| Definition | Its own comment | The `Logon` it sends |
|---|---|---|
| `1c_InvalidSenderCompID.def` | *"If a bad SenderCompID is used, we must disconnect"* | `49=WT` |
| `1c_InvalidTargetCompID.def` | *"If a bad TargetCompID is used, we must disconnect"* | `56=DLSI` |

Both expect `eDISCONNECT` and **no reply at all**, which is precisely what the stage now
does. The observable on the wire is unchanged; the stage that produces it is not.

### The first measurement was not a measurement

`shard_wire.rs` exists to notice this class of change: ADR-0022 made it count how the
pre-session stage disposed of every socket, *because a connection it quietly threw away is
indistinguishable from a duplicate the session refused*. Its comment reads:

> *"Pinned rather than relaxed to a range: a THIRD connection disappearing here would be a
> new defect wearing the same green."*

`[measured 2026-09-01]` **CI run
[33509748294](https://github.com/tmthang86/fixbolt/actions/runs/33509748294) was green, on
Linux, with `--features affinity`, and it was not evidence.** `pump()` read four fields of
`Progress` and the registry had added a fifth. Two connections disappeared into a counter
nothing read, and every assertion still held.

The guard was not wrong about the shape. It depended on somebody remembering to widen it,
which is not a guard. It now destructures `Progress` field by field with no `..`, so the
next disposal reason breaks the build. Written up in
[a-counter-that-must-be-remembered-is-not-a-counter.md](../reference/a-counter-that-must-be-remembered-is-not-a-counter.md).

`[measured 2026-09-01]` with the counter repaired, CI run
[33512983304](https://github.com/tmthang86/fixbolt/actions/runs/33512983304), AMD EPYC
GitHub-hosted runner, `cargo test -p fixbolt-engine --features affinity`:
`one_shard_passes_all_fifty_nine_at_any_settle_bound` **ok**,
`two_shards_pass_all_fifty_nine_because_identity_decides_the_shard` **ok**, and
`unknown == 2`.

## Decision

**1. The pre-session stage enforces four acceptance definitions, not two.** ADR-0022's table
gains two rows:

| Definition | Disposed as | Since |
|---|---|---|
| `1e_NotLogonMessage.def` | `not_logon` | ADR-0022 |
| `1d_InvalidLogonLengthInvalid.def` | `gone` | ADR-0022 |
| `1c_InvalidSenderCompID.def` | `unknown` | this ADR |
| `1c_InvalidTargetCompID.def` | `unknown` | this ADR |

**2. The identity comparison has one home, and it is `Config`.** The registry and the
session do not each own a copy: `Config::serves` is composed from
`Config::inbound_sender_matches` and `Config::inbound_target_matches`, and the session's own
`Logon` check calls those two. The session keeps them apart because each has its own
refusal — `1c_InvalidSenderCompID` and `2k_CompIDDoesNotMatchProfile` are different
definitions — and the registry only needs the conjunction. **Two copies of this comparison
would be two rules that disagree, and the one that disagreed would be the one deciding
whether to let a stranger in.**

**3. The session's pre-logon identity check stays, and is now unreachable for a routed
connection.** It is not deleted. It is what a caller who builds an `Engine` directly still
depends on — `tests/wire.rs` drives one with no pre-session stage at all and scores 59 —
and it remains the rule for `2k`-shaped faults *after* logon, which no registry sees.
Defence in depth, and the cost is one comparison per `Logon`, once.

**4. `unknown` is its own count, not folded into `not_logon`.** A counterparty that sent a
perfectly good `Logon` nobody configured is an operational fact — a typo in a comp ID, or a
stranger — and one this acceptor is otherwise silent about. A number is the only trace it
leaves.

## Consequences

**Good**

- **The refusal happens where the knowledge is.** The stage that reads the identity is the
  stage that knows whether anyone serves it. Routing an unserved identity to an engine so
  that engine can refuse it is work done to reach a conclusion already available.
- **It scales the way the registry does.** With forty counterparties, a stranger is refused
  once by one lookup rather than routed to one of forty engines to be refused there.
- **The corpus is unmoved.** 59 through one shard and through two, on the same definitions,
  with two of them now scored one stage earlier — which is the strongest available evidence
  that this is a move and not a change.

**Bad, and these are the price**

- **An accepted ADR's count was wrong for the length of one commit.** ADR-0022 said two, and
  between `0cfa904` and `2ba03f3` the real number was four with nothing able to say so. The
  repair is machine-checked now; the fact that it needed one is the record.
- **A guard proved its own shape and missed its own instance.** ADR-0022 predicted exactly
  this failure in prose — *"a new defect wearing the same green"* — and could not catch it,
  because the counter it wrote was a habit rather than a check. **Prose does not hold a
  constraint** (`CLAUDE.md` §4), and this is what that costs when the prose is right.
- **`1c` is now scored by code that never parses the message.** The stage reads `49=` and
  `56=` by byte scan; a malformed header that happens to yield the configured bytes would be
  admitted here and refused by the session one step later. That is the existing division of
  labour ([ADR-0020](ADR-0020-a-pre-session-stage-owns-the-socket-until-logon.md) decision
  2), not a new hole, but the surface it applies to is larger than it was.
- **Anyone reading ADR-0022 alone gets the wrong number.** That is the cost §5 accepts for
  never editing an accepted decision, and it is why this file names it in its own header.

## Open questions

1. **Should `Progress` be `#[non_exhaustive]`?** It would force every external reader to
   handle a new variant too, at the price of making the exhaustive destructure that fixed
   this impossible outside the crate. The trade runs in both directions and no consumer
   exists yet to decide it against.
2. **Does a deployment want to distinguish *unknown* from *refused by policy*?** Today both
   are `lookup` returning `None`. When `Entry` grows a credential check
   ([ADR-0026](ADR-0026-a-counterparty-registry-in-the-pre-session-stage.md) decision 3),
   "nobody by that name" and "that name, wrong password" become different operational
   events, and one counter will not carry both.
