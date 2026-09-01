# ADR-0026 — The counterparty registry lives in the pre-session stage, and identity is pluggable

> **Status:** **Accepted — 2026-09-01.** Answers `PRD.md` open decision 8. Closes nothing on its
> own: `STATUS.md` open item 28 is the work, and this is the decision it was waiting on.
>
> **Decided by the owner, explicitly, on 2026-09-01** — *"tham khảo các engine khác rồi chốt các
> quyết định cho tôi luôn"*. Not a standing-delegation acceptance: the owner asked for prior art
> to be consulted and the call to be made.

- **Date**: 2026-09-01
- **Deciders**: Tran Manh Thang
- **Related**: [ADR-0020](ADR-0020-a-pre-session-stage-owns-the-socket-until-logon.md),
  [ADR-0022](ADR-0022-the-pre-session-stage-enforces-two-definitions.md),
  [ADR-0012](ADR-0012-latency-first-and-one-session-per-polling-thread.md),
  [ADR-0025](ADR-0025-hft-has-a-hard-session-ceiling-and-the-engine-advises-rather-than-applies.md),
  `DESIGN.md` §3 and D1, `PRD.md` §3 and §6, `STATUS.md` open items 28 and 30,
  [prior-art.md](../reference/prior-art.md)

## Context

`[verified 2026-09-01]` **This engine serves exactly one counterparty.** `Config` pins
`target_comp_id` (`crates/session/src/lib.rs:259`); the `Logon` check requires the inbound `49=`
to match it and `56=` to match ours (`:1154`–`:1157`); and `serve`, `serve_hft` and
`serve_sharded_hft` each take **one** `Config` — `serve_sharded_hft` hands the same one to every
shard (`crates/engine/src/shard.rs:410`, `:431`).

**The machinery for the opposite already exists with nowhere to send anything.**
`presession::identity_of` reads `(49, 56)` off the `Logon` and `HashRoute` spreads distinct
identities across shards — each of which rejects every identity but one. Identity routing today
chooses between engines that all say no.

The question this ADR answers: **where does a registry mapping an identity to its own `Config`
live** — `presession`, `engine`, or `library`?

### What other engines do — `[documented 2026-09-01]`, not measured here

| Engine | How a `Logon` reaches its configuration |
|---|---|
| **QuickFIX** | Session identity is `(BeginString, SenderCompID, TargetCompID)` plus an optional **`SessionQualifier`** to disambiguate otherwise-identical sessions. A `SessionSettings` file holds one block per session and the incoming `Logon`'s triple is matched against those definitions |
| **QuickFIX/J, dynamic acceptors** | `DynamicAcceptorSessionProvider` — a **provider interface**, not a table. A session block marked `AcceptorTemplate=Y` is a **template** rather than a registered session; `TemplateMapping` maps a *sessionID pattern* (with `*` wildcards, `ANY_SESSION`) to a template, and the session is materialised on demand |
| **Artio** (real-logic; the closest architectural cousin — low-latency, no allocation on the path) | Identity comes from a pluggable **`SessionIdStrategy`**, which *"may"* be the comp-ID pair or may also include **SubID and LocationID**. On a `Logon` the engine runs an **`AuthenticationStrategy`**: `authenticateAsync` is invoked and the application calls `AuthenticationProxy.accept(…)` — **choosing the FIX dictionary at that moment** — under an `authenticationTimeoutInMs`. The accepting process (`FixEngine`) then notifies libraries via `SessionExistsHandler` and a `FixLibrary` takes ownership with `requestSession(surrogateSessionId)` |

**Three things transfer, and one of them contradicts this repository's current code.**

1. **Every one of them decides at the `Logon`, in the accepting stage.** None routes at accept
   time; none consults a table before the identity is on the wire. That is
   [ADR-0020](ADR-0020-a-pre-session-stage-owns-the-socket-until-logon.md), already decided here
   for a different reason, and it is the industry shape.
2. **Every one of them is a callback or a provider, not a fixed table.** QuickFIX/J's
   `DynamicAcceptorSessionProvider`, Artio's `AuthenticationStrategy`. A static map is the
   *degenerate case* of a provider, not the general one — and a provider is what makes
   authentication, per-counterparty policy and eventual hot reload possible at all.
3. **Identity is not always `(49, 56)`.** Artio's `SessionIdStrategy` may include SubID and
   LocationID; QuickFIX carries a `SessionQualifier` for the same reason. `presession::Identity`
   is the comp-ID pair only, and **a real counterparty that disambiguates by `50=`/`57=` cannot
   be served today.**

## Decision

**1. The registry lives in `presession`, and it is a trait, not a table.**

```rust
pub trait Registry {
    fn lookup(&self, id: Identity<'_>) -> Option<&Entry>;
}
pub struct Entry { /* Config, journal handle, policy, credential */ }
```

`presession` is the only layer that sees a socket before a session exists, it already reads the
identity, and [ADR-0020](ADR-0020-a-pre-session-stage-owns-the-socket-until-logon.md) already put
the decision *"which shard"* there. *"Which configuration"* is the same decision one field
earlier. Putting it in `engine` would mean an `Engine` that no longer has one `Config` — which is
what makes the single-logon rule answerable at all — and putting it in `library` would mean the
accepting stage asking a higher layer a question, on the path, before it may hand off.

**A trait rather than a map is the load-bearing half.** A `HashMap` is one implementation of it.
The trait is what lets a deployment answer from a file, from a database, or from a policy that
accepts a whole class of identities — and it is the only version of this that can grow an
authentication step without a breaking change.

**2. `Identity` gains the optional sub-IDs, and the comparison is a strategy.** `Identity` carries
`(49, 56)` and **`50=`/`57=` when present**; how much of it forms the key is the `Registry`
implementation's business. `identity_of` keeps reading bytes only, with no dictionary and no parse
([ADR-0020](ADR-0020-a-pre-session-stage-owns-the-socket-until-logon.md) decision 2).

**3. Authentication is a result of `lookup`, not a second mechanism.** A registry that can refuse
is an authenticator. `lookup` returning `None` refuses the connection exactly as an unknown
identity does today, and `Entry` is where a credential check on `553`/`554` and an IP allowlist
belong. **There will not be a separate `AuthStrategy` trait**: two hooks answering *"may this
counterparty in"* is two rules that will disagree — `CLAUDE.md`'s *one rule, one place*.

**4. Refusal is synchronous, and this is where this design deliberately parts from Artio.** Artio
offers `authenticateAsync` with a timeout, so an application can consult a remote service during
logon. **`lookup` returns immediately**: it runs on the acceptor thread, which in `hft` must not
block ([ADR-0013](ADR-0013-two-modes-standard-and-hft.md)), and an engine whose accept path can
await a network call has a denial-of-service surface that no `logon_ms` deadline closes. A
deployment needing a remote check does it **out of band** and hands the registry a snapshot.

**5. One `Engine` still carries one `Config`, and the ceiling still applies per engine.** The
registry decides *which* engine a connection belongs to; it does not make an engine
multi-identity. This keeps the single-logon rule answerable by counting the connections one
engine holds — the premise sharding broke and
[ADR-0020](ADR-0020-a-pre-session-stage-owns-the-socket-until-logon.md) restored — and it keeps
[ADR-0025](ADR-0025-hft-has-a-hard-session-ceiling-and-the-engine-advises-rather-than-applies.md)'s
ceiling of four meaningful, because it stays a statement about one polling thread.

**6. Nothing gets a default.** A registry with no entries refuses every connection, and
constructing one is the caller's act — the same rule as
[ADR-0020](ADR-0020-a-pre-session-stage-owns-the-socket-until-logon.md) decision 4's `Limits`.
**An acceptor that admits an identity nobody configured is an open port**, which is precisely what
QuickFIX/J's wildcard `ANY_SESSION` template is, and it is not offered here.

## Consequences

**Good**

- **The engine becomes an acceptor.** It is the difference between a point-to-point link and a
  gateway, and it is what makes identity sharding mean anything: today `HashRoute` spreads
  identities across engines that all reject them.
- **It is the industry shape, arrived at independently.** ADR-0020 put the decision at the
  `Logon` for a conformance reason — `1b_DuplicateIdentity.def` scoring 57 — and all three
  reference engines are already there for an operational one.
- **Authentication stops being a separate future feature.** It is `lookup` returning `None`.
- **Hot reload stays possible.** A trait can be backed by something that changes; a `HashMap`
  baked into a constructor cannot. This decision is what keeps that door open, which is why
  `PRD.md` called it not a placement detail.

**Bad, and these are the price**

- **`presession` grows a generic parameter**, and it is on the accept path's type signature.
  Every entry point's shape changes: `serve_sharded_hft(addr, cfg, …)` becomes
  `serve_sharded_hft(addr, registry, …)`. That is a breaking change to every public entry point,
  and nothing is published, so the price is a `CHANGELOG.md` entry and this repository's own call
  sites.
- **`lookup` is on the connection path** — once per connection, not per message, and the
  pre-session sweep is already priced at **426.2 ns per socket against `Engine::turn`'s 458.3** —
  `[measured 2026-09-01]` `crates/engine/benches/presession.rs`, both arms **in the same run** so
  the comparison is not across programs, AMD Ryzen 7 3700X, ADR-0021 §9 line, `check-machine.sh` **pass 11 fail 0 unknown 1**, 20 qualifying runs. A registry lookup must be measured against
  that, not assumed cheap, and **an implementation that
  allocates puts an allocation on a path `benches/alloc.rs` currently proves is zero.**
- **Synchronous refusal is a real limitation**, not a simplification. A deployment whose
  entitlements live in a remote service must snapshot them. Artio can do what this cannot.
- **`Identity` carrying sub-IDs is a wider struct** on a path that reads bytes only, and the
  fields are optional, so every consumer gains a case to handle.

## Open questions

1. **What does `lookup` cost?** It sits beside that 426.2 ns sweep and must be measured on the
   same machine and the same §9 line, with
   `benches/presession.rs` gaining a case and `benches/alloc.rs` gaining a count.
2. **Does a per-counterparty journal change `Durability`?** `Entry` holds a journal handle; forty
   counterparties is forty journals, forty mmaps and forty writer threads under
   `Durability::Async`. That is a resource question this ADR does not answer.
3. **Does the registry own the session schedule too?** Start/end times are per counterparty in
   every engine surveyed. It probably belongs in `Entry`, and the session-schedule work is what
   should decide it rather than this ADR guessing.
