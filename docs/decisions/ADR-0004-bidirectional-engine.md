# ADR-0004 — The engine is bidirectional: acceptor and initiator share one session core

- **Status**: Accepted — 2026-08-27
- **Date**: 2026-08-27
- **Deciders**: Tran Manh Thang
- **Related**: [ADR-0001](ADR-0001-relationship-to-quickfix.md), [ADR-0002](ADR-0002-engine-library-split.md),
  [DESIGN.md §1, §5, §7](../DESIGN.md)
- **Changes**: the *"acceptor-first, the initiator side is a non-goal for v1"* position stated in
  `DESIGN.md` §1 and §5. Supersedes no ADR — that position was never written as one.
- **Answers**: ADR-0001 open question 1 (*"does `test/definitions/client/` also need to run?"*).

## Context

`DESIGN.md` opens with a positioning statement — *the fastest FIX acceptor that can be built
on kernel TCP* — and §5 lists *"the initiator side, beyond what the acceptance definitions
require"* as a non-goal for v1. This ADR asks whether that boundary is architectural or
merely a sequencing choice, and concludes it is the latter.

**1. Almost nothing in the design is acceptor-specific.** Measured against the crate table in
§3 and the session sketch in D1:

| Crate | Shared between the two sides | What actually differs |
|---|---|---|
| `codec` | **100%** | Nothing. FIX bytes have no notion of side |
| `dict` | **100%** | Nothing |
| `transport` | **100%** | The trait is `recv`/`send`. `connect` vs `accept` belongs to `engine` |
| `session` | **~90%** | Logon handshake direction; who proposes `HeartBtInt(108)` and who echoes it; which side of a CompID pair is validated |
| `engine` | ~60% | Acceptor: listen + accept loop. Initiator: connect, reconnect backoff, and a **session schedule** — FIX sessions have start/end times and weekday rules |
| `library` | **100%** | Nothing |

Everything expensive — the codec, the generated tables, and the symmetric bulk of the session
machine (heartbeats, `TestRequest`, `ResendRequest`, gap fill, sequence-number too-high and
too-low, `PossDupFlag`/`OrigSendingTime`, `Reject`, `Logout`) — is already side-agnostic. The
asymmetric part is the Logon handshake and the reconnect policy, and the reconnect policy
does not live in the pure session machine at all.

**2. The test asset is severely asymmetric.** Counted in `vendor/quickfix/test/definitions/`
on 2026-08-27:

```
471 .def files total
├── server/  470   ← the acceptor oracle, across 8 FIX versions
│   fix40 56 · fix41 56 · fix42 58 · fix43 58 · fix44 59
│   fix50 60 · fix50sp1 60 · fix50sp2 60 · future 2 · validate 1
└── client/    1   ← the entire initiator oracle
    Normal.def — 271 bytes, 6 lines, FIX 4.2, logon then logout
```

`client/Normal.def` also uses two directives absent from the seven recorded in
[reference/quickfix-acceptance-def-format.md](../reference/quickfix-acceptance-def-format.md):
a bare `eCONNECT`, and `R` for a line the harness sends in reply. That page scoped itself to
the 59 server files and is not wrong; the client-side grammar is simply larger and has not
been decoded.

**The consequence is the whole cost of this decision.** The version axis is cheap to extend
later — 56 to 60 ready-made definitions per FIX version, free. The **side** axis has no
ready-made oracle at all, and `CLAUDE.md` §7 makes the acceptance definitions the primary gate
precisely because a session layer without one is graded on opinion. Decision 6 below recovers
much of that ground by mirroring 51 of the 59 server definitions; what it cannot recover is
everything an initiator does that an acceptor never does, which is where decision 5 earns its
cost.

**3. The Rust landscape argues both ways.** From
[reference/prior-art.md](../reference/prior-art.md): `hotfix` and `IronFix` are initiator-only,
`fixer-rs` is early, `ferrumfix` (450★) describes itself as *"wildly unstable"*. The empty
space is the acceptor — that is the whole reason this project exists. But `ferrumfix` is also
the cautionary tale for the opposite move: it is the one project attempting a general engine,
and generality is a plausible part of why it has not stabilised.

## Decision

**Revised 2026-08-27, same day, before acceptance.** Open question 2 — *how many of the 59
definitions actually mirror?* — was estimated at 20–30 in the first draft and has since been
answered by script: **51 of 59**. Decision 6 and the corresponding consequence are rewritten
below; the estimate is left visible here rather than quietly replaced. A second revision the
same day removed this ADR's claim that SBE / FAST / FIXML *stay* non-goals — [PRD.md](../PRD.md)
moved them to phase 2, and an ADR must not assert a scope another document has since changed.

**Make the engine bidirectional. One session core, parameterised by role. The acceptor
remains the differentiator and is proven first; the initiator is a peer, not an afterthought.**

This ADR moves the **side** axis only, and that separation is the point: a bidirectional FIX
4.4 engine is a far smaller commitment than a general one. The **encoding** axis — SBE, FAST,
FIXML — and the **version** axis — FIX 5.0 / FIXT 1.1 — were moved into phase 2 by
[PRD.md §2](../PRD.md) on the same day. Phase 2 does not begin with an encoding; it begins
with deciding whether `MessageView` generalises to a wire format that has no tags on it, and
that decision needs its own ADR.

1. **`session` takes a role from day one.** `Role { Acceptor, Initiator }` enters the session
   configuration in the first session plan, even while only one variant is implemented. If the
   Logon direction is instead hardcoded, extracting it later is a rewrite of the state
   machine's entry transitions.
2. **The `engine` core is side-agnostic**: *a set of connections driven by session machines*.
   `listen`/`accept` and `connect`/reconnect are two drivers over that core, not two engines.
3. **No public type in the core is named for a side.** No `Acceptor`-prefixed types outside
   the driver layer.
4. **Build order is unchanged in shape: the acceptor reaches 59/59 first.** The shared ~90%
   must be right before the asymmetric ~10% means anything. The initiator role lands after
   the session layer is green, and before `library`.
5. **The initiator's primary gate is interop against QuickFIX C++.** A CI job builds
   `libquickfix`, runs it as an acceptor, and drives nanofixengine's initiator against it
   through logon, heartbeat, `TestRequest`, `ResendRequest`, gap fill and logout. A
   disagreement is a disagreement with twenty years of real deployment.
6. **51 of the 59 definitions mirror, and they are the secondary gate.** The harness plays
   acceptor: `E` lines become inputs to our initiator and `I` lines become expected outputs.
   The criterion is mechanical and reproducible — a definition mirrors when every one of its
   `I` lines is something a correct initiator would actually send, i.e. it begins with
   `8=FIX.4.4`, every tag is numeric, and no field is empty. The 8 that fail are exactly the
   ones whose purpose is acceptor-side rejection of malformed input, and no initiator has an
   analogue for them:

   | Excluded | Why |
   |---|---|
   | `14a_BadField`, `2d_GarbledMessage`, `3c_GarbledMessage` | non-numeric tag |
   | `14d_TagSpecifiedWithoutValue`, `ReverseRouteWithEmptyRoutingTags` | empty field value |
   | `1d_InvalidLogonWrongBeginString`, `2i_BeginStringValueUnexpected` | `BeginString` is `FIX.3.9` / `FIX.4.1` |
   | `2t_FirstThreeFieldsOutOfOrder` | `35=` before `8=` |
7. **The C++ toolchain is confined to CI and `tools/`.** It never enters `Cargo.toml`, never
   the library build, never a user's machine. This is the distinction that separates it from
   the FFI wrapper ADR-0001 rejected: that proposal put C++ in the shipping path.

## Consequences

**Good**

- The ~90% that is shared gets written once and gated once, by the strongest gate available
  (59/59), instead of being written once and then partially duplicated.
- The load generator `DESIGN.md` §6 already requires for `tools/w2w` stops being throwaway
  scaffolding. It needs to speak FIX to get past Logon, which makes it an initiator whether or
  not this ADR exists; this decision makes that work count.
- `Role` in the session configuration costs approximately nothing today and removes a
  guaranteed refactor later. This is the cheapest part of the decision by a wide margin.
- An engine that does both sides is usable by a firm that runs an acceptor for its clients
  *and* connects out to venues — which is most firms that run either.
- Interop against `libquickfix` is a better test than anything this project could write for
  itself, and it also benefits the **acceptor**: the same job can point QuickFIX's initiator
  at nanofixengine's acceptor and cross-check the 59/59 result against a real counterparty.

**Bad — and these are real**

- **The cost does not vanish, it relocates.** 51 of 59 definitions mirror, so the *symmetric
  session mechanics* — heartbeat, `TestRequest`, `ResendRequest`, gap fill, sequence gaps,
  `Logout` — do get real coverage on the initiator side. But mirroring is still **this
  project's own reading** of a suite written for the other direction, not an independent
  opinion; a wrong reading stays green. And the genuinely initiator-specific work —
  reconnect, backoff, session schedules, sequence persistence across a reconnect — is covered
  by **zero** of the 59, by zero of the mirrored 51, and by the one client definition. That
  residue is where a false sense of safety would live, and interop against `libquickfix`
  (decision 5) is the only thing standing in front of it.
- **A C++ toolchain now has to be maintained.** CMake and C++17 in CI, a slower pipeline, and
  a job that will go red for reasons unrelated to any Rust in this repository. ADR-0001 listed
  *"keeps the C++ toolchain"* as a reason to reject the FFI wrapper; that reasoning is
  narrower here but it is not zero, and pretending otherwise would be dishonest.
- **Session schedules are real work with no test at all.** Start time, end time, weekday
  rules, and what happens to sequence numbers across a scheduled reset. QuickFIX has twenty
  years of accumulated behaviour here and the `.def` suite tests none of it.
- **Reconnect is subtle and untested by the suite**: backoff, sequence-number persistence
  across a reconnect, and `ResetSeqNumFlag(141)` negotiation on re-logon. Each of these is a
  place where a wrong answer looks fine in a lab and fails against a venue.
- **Scope grows before the acceptor has been proven at all.** The acceptor has zero production
  track record — ADR-0001 says so explicitly — and this decision adds surface before that zero
  has changed. The mitigation is decision 4 (59/59 first), and a mitigation is not a fix.
- **The positioning gets less sharp.** *"The only production-proven Rust FIX acceptor"* is a
  sentence that sells itself. *"A bidirectional Rust FIX engine"* competes directly with
  `ferrumfix`'s stated ambition, and `ferrumfix` has 450 stars and calls itself unstable.
  Whether the headline changes is an open question below.
- **The version axis now looks arbitrary.** Once the engine does both sides, a user reasonably
  asks why it does only FIX 4.4 when 470 definitions across 8 versions sit in the same
  directory. The answer is good — 4.4 is where the acceptance work is — but it has to be said
  out loud rather than implied by a non-goal list.

## Alternatives considered

| Alternative | Why rejected |
|---|---|
| **Acceptor-only for v1** (the position this ADR changes) | Correct on the evidence and cheaper, but it leaves `Role` out of the session configuration, which is the one thing that is expensive to add later. Rejected on the owner's decision after the cost was stated |
| **Initiator as a test tool only** — build it inside `tools/w2w`, never publish it | Genuinely attractive: the load generator is needed anyway, and a tool carries no compatibility promise. Rejected because the same code would then exist twice the moment anyone wants it as a product |
| **A general engine now** — both sides, FIX 4.0 through 5.0 SP2, full repeating groups | This is what `ferrumfix` attempted. Generality before a single version is proven is how a FIX engine stays at 0.x for years |
| **Wrap the `quickfix` crate for the initiator side only** | Ships a C++ toolchain and the 6–8k msg/s ceiling to every user, for the half of the protocol where latency matters most. Rejected for the same reasons as ADR-0001 |
| **Loopback only** — our initiator against our acceptor | Cheapest gate, and it does catch asymmetry bugs. Rejected as a *primary* gate: two sides sharing one misreading stay green forever. Kept as a third-tier smoke test |

## Open questions

1. **Does the headline positioning change?** `DESIGN.md` §1 and `README.md` both lead with
   "acceptor". The engine being bidirectional does not require the marketing to be, and the
   acceptor is still the differentiator. Decide deliberately rather than by drift.
2. ~~How many of the 59 definitions actually mirror?~~ **Answered 2026-08-27: 51 of 59.**
   Criterion and the 8 exclusions are in decision 6. The script that produced it belongs in
   the repository before this number is quoted anywhere else.
3. **Which QuickFIX ref does the interop job pin?** `scripts/fetch-quickfix-assets.sh`
   currently tracks mutable `master` (STATUS.md open item 7). An interop job that builds a
   moving target is a job that fails for no reason.
4. **What defines a session schedule?** Inherit QuickFIX's `StartTime`/`EndTime`/`StartDay`/
   `EndDay` semantics, or define a narrower one. Inheriting means inheriting its edge cases;
   defining our own means no counterparty expects our behaviour.
5. **Does `engine` need `libquickfix` in `vendor/` at all**, or does the interop job pull it
   separately? The current sparse-checkout takes only `spec/`, `test/definitions/` and
   `LICENSE` — deliberately no `src/`. Widening it puts QuickFIX C++ source in the tree, which
   ADR-0001 §5 has consequences for.
