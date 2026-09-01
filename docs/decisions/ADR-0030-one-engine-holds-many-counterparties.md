# ADR-0030 — One engine holds many counterparties, and the single-logon rule compares identities

> **Status:** **Accepted — 2026-09-01.** **Supersedes
> [ADR-0026](ADR-0026-a-counterparty-registry-in-the-pre-session-stage.md) decision 5.**
> Decisions 1–4 and 6 of ADR-0026 stand unchanged; the registry is still a trait in
> `presession`, still synchronous, still the authentication hook, and an empty one still
> refuses everything.
>
> **Accepted by standing delegation**, `[2026-08-30]`, and the owner's instruction of
> 2026-09-01 to research prior art and decide. **The deciding evidence is in this
> repository**, not on the web: `1b_DuplicateIdentity.def`'s own comment.

- **Date**: 2026-09-01
- **Deciders**: Tran Manh Thang
- **Related**: [ADR-0026](ADR-0026-a-counterparty-registry-in-the-pre-session-stage.md),
  [ADR-0025](ADR-0025-hft-has-a-hard-session-ceiling-and-the-engine-advises-rather-than-applies.md),
  [ADR-0020](ADR-0020-a-pre-session-stage-owns-the-socket-until-logon.md),
  [ADR-0012](ADR-0012-latency-first-and-one-session-per-polling-thread.md),
  `DESIGN.md` D8, [prior-art.md](../reference/prior-art.md),
  [plans/2026-09-01-counterparty-registry.md](../plans/2026-09-01-counterparty-registry.md) step 4

## Context

[ADR-0026](ADR-0026-a-counterparty-registry-in-the-pre-session-stage.md) decision 5 said:

> **One `Engine` still carries one `Config`, and the ceiling still applies per engine.** The
> registry decides *which* engine a connection belongs to; it does not make an engine
> multi-identity. This keeps the single-logon rule answerable by counting the connections one
> engine holds.

**Step 4 of the plan is where that stops working.** `serve`, `serve_hft` and
`serve_sharded_hft` each build their engine before any connection arrives. If one engine may
hold only one counterparty, an acceptor serving forty of them must build forty engines — and
then two questions have no answer:

1. **A `Registry` is a trait.** It maps an identity to an `Entry`; it cannot be enumerated,
   so an entry point given one cannot know how many engines to build or which. The registry
   would have to become a map again, which is exactly what ADR-0026 decision 1 refused.
2. **Sharding already decides which engine.** `Route` picks the shard. If the registry also
   picked the engine, an engine would have to be one per *(shard × counterparty)* pair, and
   the two decisions would have to agree with no mechanism making them.

Forty engines on one `standard` thread is also forty connection vectors, forty interest
lists and forty `turn()` calls per loop, to hold what is at most forty sockets.

### What the corpus says, and it is not ambiguous

`1b_DuplicateIdentity.def`, first line of the file, written by QuickFIX:

> *"If two logons with the **same SenderCompID/TargetCompID combination** logon the second
> one must be disconnected"*

**The rule is per identity.** This engine implemented it as *"is any other connection in this
engine logged on"*, which is the same answer only while an engine holds exactly one identity.
`AlreadyLoggedOn.def` runs the identical shape and cannot distinguish the two either — both
definitions connect twice **as the same counterparty**, so neither could ever have caught the
difference.

### And it is what the reference engines do

Already documented in [ADR-0026](ADR-0026-a-counterparty-registry-in-the-pre-session-stage.md)'s
prior-art table, `[documented 2026-09-01]`, re-read here for this question:

| Engine | Engines per counterparty |
|---|---|
| **QuickFIX** | One acceptor, a `SessionSettings` file with **one block per session**, sessions keyed by `(BeginString, SenderCompID, TargetCompID)` plus an optional `SessionQualifier` |
| **QuickFIX/J** | One acceptor; `DynamicAcceptorSessionProvider` **materialises a session on demand** from a template |
| **Artio** | One `FixEngine`; `SessionExistsHandler` notifies libraries and a `FixLibrary` takes ownership of each session by id |

**None of the three builds an engine per counterparty.** All three hold many sessions in one
accepting process. This decision is not novel; ADR-0026 decision 5 was.

## Decision

**1. An `Engine` holds as many counterparties as reach it.** `Engine::add_with_prefix_and_config`
builds the connection's `Session` from the configuration the registry chose, not from the
engine's own. `Engine::new`'s `Config` remains the default for `Engine::add`, which is the
path an engine driven without a pre-session stage uses — `tests/wire.rs` runs the whole corpus
that way and is unchanged.

**2. The single-logon rule compares identities.** `Config::same_identity_as` — BeginString and
both comp IDs, and deliberately not `HeartBtInt` or `MaxLatency`, because two entries differing
only in those are still one counterparty. The comparison lives on `Config` beside
`Config::serves`, so the registry and the rule read the same fields.

**3. `Config` is what travels, and `Session` already carried one.** `Session::new(cfg)` has
always copied a `Config` into the session; the engine simply always passed its own. This
decision changes which value is passed, not where identity lives. `Session::config()` is the
accessor the rule needs.

**4. The serving entry points take a `presession::Table`, not an `impl Registry`.** An entry
point must build its engine before any connection arrives, and a trait yields no default
`Config`. `Table` is ADR-0026's own default implementation and can. **The trait's generality
is not lost** — it lives in `PendingSet`, which is public, and a deployment with a custom
`Registry` writes the loop `serve` would have written for it. `crates/engine/tests/registry.rs`
has such a `Registry` in eight lines.

**5. An empty registry is refused at startup, not at every connection.**
`ServeError::NoCounterparties` and `ShardError::NoCounterparties`. An empty `Table` is a valid
registry that refuses everything ([ADR-0026](ADR-0026-a-counterparty-registry-in-the-pre-session-stage.md)
decision 6) — a *serving loop* built on one is a configuration mistake, and the reasoning that
gives [ADR-0020](ADR-0020-a-pre-session-stage-owns-the-socket-until-logon.md) decision 4's
`Limits` no defaults says to refuse it where somebody can still read the message.

**6. ADR-0025's ceiling is unaffected, and reads better.** It is titled *"`hft` has a hard
**session** ceiling"* — a statement about how many sessions one polling thread may hold, which
is what it still is. It never needed engines to be one per counterparty.

## Consequences

**Good**

- **The entry points can exist at all.** Step 4 of the plan had no implementation under
  decision 5, which is how this was found.
- **One engine per thread, whatever the counterparty count.** One interest list, one `turn`,
  one connection vector.
- **The rule now says what the corpus says.** `[measured 2026-09-01]` a new test puts two
  counterparties on one engine and a duplicate of one of them behind them; reverting the
  comparison to a count makes it red, and *deleting the rule entirely* also makes it red while
  the corpus alone would not notice — `tests/wire.rs` catches deletion, and only this test
  catches the failure to compare.

**Bad, and these are the price**

- **A decision made on 2026-09-01 was superseded on 2026-09-01.** ADR-0026 decision 5 was
  written before the entry points were, and reasoned from a premise — *the single-logon rule
  needs engine ≡ identity* — that the corpus contradicts in its first line of prose. **The
  prior art was in the same ADR and pointed the other way.** Reading it as *"where does the
  registry live"* and not as *"how many sessions does an accepting process hold"* is the
  mistake.
- **The single-logon rule got more expensive.** It was a count; it is now a count with a
  comparison of three `Name`s per candidate, per connection, per turn — O(n²) in connections
  on a path `benches/turn.rs` measures. With `hft`'s ceiling of four
  ([ADR-0025](ADR-0025-hft-has-a-hard-session-ceiling-and-the-engine-advises-rather-than-applies.md))
  that is at most twelve comparisons; `standard` has no such ceiling and this is **not yet
  measured**. It belongs in the next `benches/turn.rs` run on a §9 machine.
- **`serve` takes a concrete type.** A caller with a database-backed registry cannot use it
  and must write the loop. That loop is about thirty lines and `pump` is the worked example,
  but it is a real step down from `impl Registry`.
- **`Shardable::add` gained a parameter**, so anything implementing it outside this repository
  breaks. Nothing is published.

## Open questions

1. **What does the identity comparison cost on the `turn` path?** Unmeasured. `benches/turn.rs`
   exists and a §9 machine has not run it since this change.
2. **Should `Engine::new` stop taking a `Config` at all?** It is now only a default for `add`.
   Making it `Option<Config>` would make `add` fallible, which is a wide break for a case that
   only arises when somebody mixes the two paths.
3. **Does the registry belong behind `Arc` for the sharded loop?** Today the `Table` moves into
   the acceptor thread's `PendingSet`, which is the only thing that looks anything up. A
   per-shard registry would need sharing; nothing needs it yet.
