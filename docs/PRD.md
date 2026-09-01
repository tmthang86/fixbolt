# fixbolt — Product Requirements

What this engine is for, what ships in which phase, and — stated plainly — **how far it is
from QuickFIX**, which is the only honest baseline for a FIX engine.

`DESIGN.md` says *how* it is built. This page says *what* it must do and *when*.
Decisions live in [decisions/](decisions/); what is about to be built lives in [plans/](plans/).

**Evidence marks used throughout:**
`[measured]` counted or run in this repository on the stated date ·
`[documented]` read off another project's own documentation, not verified here ·
`[unproven]` neither — an intention.

---

## 1. Who this is for

**Two modes, and `standard` is the default** —
[ADR-0013](decisions/ADR-0013-two-modes-standard-and-hft.md):

| Mode | For | Buys | Costs |
|---|---|---|---|
| **`standard`** — the default | anybody, any OS, any hardware, a container, a laptop | portability, and **the core back** — it blocks on readiness when idle | the microsecond |
| **`hft`** — opt-in, Linux only | a tuned box with isolated cores | the microsecond | **a core burned per polling thread**, and a machine that satisfies `DESIGN.md` §9 |

An engine whose out-of-the-box configuration pins a core at 100% is one most people cannot
evaluate — it looks broken. **`hft` is the claim; `standard` is the front door.** Every figure
this project publishes names **which mode**, as well as its session count and its machine.

**Inside `hft`, latency is a tie-breaker rather than an adjective.**
[ADR-0012](decisions/ADR-0012-latency-first-and-one-session-per-polling-thread.md), re-scoped
to `hft` by ADR-0013: when latency and session density conflict **inside `hft`**, latency wins,
and a change that trades per-session latency for sessions-per-core needs its own ADR to reverse
that. The shape `hft` is optimised for, budgeted for and measured at is **one session on an
isolated polling thread**. Inside `standard` the tie-breaker is **portability, then the core
back** — many sessions on one blocked thread is simply how it is run.

`[measured 2026-08-31]` the reason is arithmetic, not preference. An idle turn is one
non-blocking `read` per connection, and `Engine::turn` costs **449 ns per session on a core
set up to `DESIGN.md` §9** — ~670 ns if that core carries `nohz_full`, which §9 no longer asks
for ([ADR-0021](decisions/ADR-0021-nohz-full-leaves-section-9.md)) — flat from 1 to 16
sessions within 2%. A message waits up to one whole sweep to be seen. **Two sessions on one
polling thread exceed `DESIGN.md` §8's entire budget in polling alone.**

The figure published here is the **isolated-core** one, because that is the machine this
engine tells people to run on. It is also the more expensive one: the isolation §9 asks for
costs **36%** on this exact operation, and what it buys — a shorter tail — is not measured yet.
The `703 ns` this paragraph carried until 2026-08-31 was a **C program's bare `read`**, not this
engine's turn. [reference/measured-costs.md](reference/measured-costs.md),
`crates/engine/benches/turn.rs`.

| User | What they need | Served in |
|---|---|---|
| A firm on a latency-critical path to a venue | An **acceptor or initiator** on a dedicated polling thread, budgeted end to end | Phase 1 — **the shape this engine is built for** |
| A venue or broker running a FIX gateway for its clients | Many sessions per core, in the **`density`** shape — supported, and carrying its own budget of `[measured 2026-08-31]` **`N × 449 ns`** per polling thread plus the per-message path, rather than the latency figures above. `[2026-08-31]` **the shape is now concrete**: `fixbolt_engine::shard` gives M pinned threads of N sessions each, so a gateway budgets `N × 449 ns` where N is sessions **per shard** rather than in total — `GUIDE.md` §1a does that arithmetic. `[2026-09-01]` **and it routes by counterparty**: each socket is held until its `Logon` arrives and a stable hash of `(49, 56)` picks the shard, so the single-logon rule holds across shards — the corpus scores 59 through two, where it scored 57 | Phase 1, [ADR-0012](decisions/ADR-0012-latency-first-and-one-session-per-polling-thread.md) |
| A firm connecting out to venues | An **initiator** with reconnect, schedules and sequence persistence | Phase 1 ([ADR-0004](decisions/ADR-0004-bidirectional-engine.md)) |
| A team building a simulator or a QA exchange | Both sides, plus a dispatch mode that survives an application that blocks | Phase 1 ([ADR-0002](decisions/ADR-0002-engine-library-split.md)) |
| A market-data consumer | Binary encodings — SBE today, FAST for legacy feeds | Phase 2 |
| A post-trade / clearing integration | FIXML | Phase 2, explicitly outside the hot-path guarantee |

**Every latency figure this project publishes names its session count**, exactly as
non-negotiable 10 already requires it to name its machine and its §9 settings. A figure without
`N` is not a figure. `[2026-08-30]` every number published before that date was taken at **N=1**
in benches that hold no socket at all — the best case, and it was never labelled as one.

**The differentiator is the acceptor.** `[documented]` As of 2026-08-27 the Rust ecosystem has
no production-proven FIX acceptor: `hotfix` and `IronFix` are initiator-only, `ferrumfix`
calls itself *"wildly unstable"*, and the `quickfix` crate is an FFI binding to C++. See
[reference/prior-art.md](reference/prior-art.md).

## 2. Phases

```
Phase 1 — a FIX 4.4 engine you can actually deploy          ← all current work
  tag=value · both sides · 59/59 · repeating groups · full dictionary validation
  ├── codec + dict          ← done 2026-08-28
  ├── conformance runner    ← done 2026-08-28
  ├── session (Role-parameterised)  ← acceptor done 2026-08-29; initiator steps 3-4 paused
  ├── engine (accept + connect drivers, journal, dispatch, backpressure)  ← done 2026-08-30
  ├── tools/w2w             ← wire-to-wire, on Linux — NEXT, and needs a Linux box
  ├── machine probe + advice ← detect and recommend, never apply (ADR-0025, Proposed)
  ├── many counterparties   ← a registry: identity -> Config, journal, policy. NOT STARTED,
  │                            and until it exists this is a link, not an acceptor (item 28)
  ├── session schedules     ← start/end/weekday reset. Named a gap three times, never planned
  ├── operability           ← ordered shutdown · operator snapshot · sequence-number admin ·
  │                            event stream · offline journal reader · health probe (item 30)
  └── library               ← not started

Phase 2 — the encoding axis, and the version axis
  ├── Encoding trait        ← the architectural gate for everything below
  ├── SBE                   ← the one modern venues actually use
  ├── FIX 5.0 / FIXT 1.1    ← SBE needs ApplVerID; the two arrive together
  ├── FAST                  ← legacy market-data feeds; stateful decode
  └── FIXML                 ← post-trade; outside the hot-path guarantee

Phase 3 — not scoped, listed so scope creep has to argue with a document
  kernel bypass — Onload first, ef_vi second, DPDK never (STATUS open item 14)
  SIMD SOH scan / checksum — only if the Linux parse number asks for it (open item 12)
  clustering · HA · replication
```

### Phase 1 — exit criteria

Every one is a command that either passes or fails. A criterion nobody can run is not one.

| # | Criterion | Gate |
|---|---|---|
| 1 | Session conformance | **59 / 59** — `cargo test -p fixbolt-session --test score`, in-process, no socket. **Met** `[measured 2026-08-29]`, re-run 59 / 59 on a second machine 2026-08-30. Through a real socket, `cargo test -p fixbolt-engine --test wire`: `[measured 2026-08-30]` **59 / 59 on both machines — met.** It read 39 / 59 on Linux until the harness's own client socket was given `TCP_NODELAY` |
| 2 | Repeating groups | **Met 2026-08-28.** Read and written for all **93** groups `[measured]`; order agreed with QuickFIX's generated C++ on 730/730. **The 59 definitions do not test this** — see §4 |
| 3 | Dictionary validation | Required fields, field types, enum values, unknown tags, group structure — generated from XML, with `<component>` recursion. `[measured 2026-08-28]` **the tables exist**: 912 tags, 93 message types, 12 524 (message, tag) pairs, 23 field types, 1 708 enum values. **Applied by the session as of step 3** — all twelve `373` codes are produced, and a test asserts that rather than inferring it from the file count. `[2026-08-29]` one type rule was wrong and the corpus caught it: `SEQNUM` refused `0`, on a rule this project invented and an invented test that agreed with it |
| 4 | Both sides | Acceptor 59/59 — **met**. Initiator interop-green against `libquickfix` in CI — **not met, and deferred behind `engine` on 2026-08-30**. `[measured]` the initiator speaks first and can originate; the mirrored corpus scores 0/50 and tops out at 45 for reasons in `STATUS.md` |
| 5 | Allocations on the hot path | **0**, proven by `benches/alloc.rs`. `[measured 2026-08-30]` **met on every crate that has a hot path**: codec 3 cases, session 13, engine 7, each proven by injection |
| 6 | Wire-to-wire | p50 / p99 / p99.9 published from `tools/w2w` on Linux with the §9 settings stated |
| 7 | Builds clean | `cargo clippy -D warnings`, `--no-default-features`, no `unwrap`/`expect`/`panic` in a library crate |

**Where the criteria stand on 2026-08-30, after the `engine` plan closed.** 1, 2, 3, 5 and 7
are met. **4 and 6 are not**, and neither is blocked on a decision:

- **4 — the initiator's interop gate** needs a CI job that builds `libquickfix` and drives this
  engine's initiator into it (ADR-0004). Steps 3–4 of the paused initiator plan.
- **6 — wire-to-wire** needs `tools/w2w` and a Linux box. It is the only criterion blocked on
  hardware, and it also carries open items 15 and 16 with it: the syscall trace that would give
  non-negotiable 4 a machine check can only be taken there.

### Phase 2 — the architectural gate that comes first

**`MessageView` as designed does not generalise, and that is the finding phase 2 turns on.**
[ADR-0003](decisions/ADR-0003-message-representation.md) defines
`FieldEntry { tag, offset, len }` — it presupposes tags on the wire. SBE has none: fields sit
at schema-determined offsets and decoding is closer to a struct cast than to a scan. So phase 2
does not begin with "add SBE"; it begins with deciding whether there is one view type or
several. That decision needs its own ADR before any encoding work starts.

Each encoding also drags something with it, and none of the three is a drop-in:

| Encoding | What it really costs | Status |
|---|---|---|
| **SBE** | Has **no session layer of its own**. Venues pair it with FIXP or a proprietary session, so "support SBE" implies a second session state machine — comparable in size to the FIX 4.4 one — unless SBE rides a tag=value session. That fork must be decided, not assumed | `[unproven]` |
| **FIX 5.0 / FIXT 1.1** | Arrives with SBE, because SBE messages are versioned by `ApplVerID`. `[measured]` `spec/FIXT11.xml` and `FIX50*.xml` are already in `vendor/`, and 180 ready-made `.def` files exist across the three 5.0 variants — the cheapest item in phase 2 | `[unproven]` |
| **FAST** | **Stateful decoding.** Message N depends on the dictionary state left by message N−1 (field operators, presence maps). `parse_into(buf, idx)` is stateless by design; FAST needs a decoder that owns per-stream state. Legacy — most venues have moved to SBE | `[unproven]` |
| **FIXML** | Needs an XML parser, which allocates. **Explicit carve-out:** FIXML lives outside non-negotiable #1 (no allocation on the hot path) because post-trade is not a hot path. Stated here so it is a decision, not a leak | `[unproven]` |

`[measured]` **QuickFIX ships no SBE, FAST or FIXML schema.** `vendor/quickfix/spec/` holds
only the 8 tag=value dictionaries plus `FIXT11.xml`. Phase 2 has no free test oracle from that
direction — the same problem ADR-0004 priced for the initiator, one level worse.

## 3. Where this stands against QuickFIX

**Short answer: not close, and two of the gaps are load-bearing.**

QuickFIX is 20+ years old with complete spec coverage. Matching it is not a phase-1 goal and
possibly never a goal — but the gaps have to be named, because a user comparing the two will
find them whether or not this page does.

| Capability | QuickFIX | fixbolt | Phase |
|---|---|---|---|
| FIX versions | 4.0–5.0 SP2 + FIXT 1.1 — **8 dictionaries** `[measured]` | 4.4 | 5.0/FIXT → P2 |
| Acceptor | Yes `[documented]` | Yes | P1 |
| Initiator | Yes `[documented]` | Yes | P1 (ADR-0004) |
| **Repeating groups** | Full — **93 groups in FIX 4.4** `[measured]` | Read and written, nested to depth 4. Field order agreed with QuickFIX's generated C++ on **730/730** groups `[measured]` | P1, **closed** |
| **Dictionary validation** — types, enums, structure | Full `[documented]` | 10 generated tables with `<component>` recursion. `[measured 2026-08-28]` types and enum values **are** now tabulated and agreed with QuickFIX's own generated C++ on 912/912 tag numbers, 898/912 type names (14 named differences), 12 524/12 524 message-tag pairs and 1 708/1 708 enum values. Applied by the session's `Reject (35=3)` | P1, **closed for the session layer**; application-message validation is phase 2 |
| Decimal / price types | Yes `[documented]` | Bytes and integers only | P1, gap |
| Session schedules — start/end time, weekday | Yes `[documented]` | Entered scope with ADR-0004; unspecified | P1, gap |
| Message stores | File, memory, and SQL backends `[documented]` | mmap journal, 3 policies (`None`/`Async`/`Fsync`) | P1, by design narrower |
| Logging | File, screen, SQL backends `[documented]` | `tracing` behind a feature flag; **never on the hot path** | P1, by design narrower |
| SSL / TLS | Yes `[documented]` | [ADR-0005](decisions/ADR-0005-tls.md) **Accepted** — `rustls` handshake, kTLS steady state on Linux — supplemented by [ADR-0018](decisions/ADR-0018-ktls-on-a-plain-socket-answers-adr-0005.md). `[measured 2026-08-31]` kTLS **is** drivable from a plain non-blocking socket with no async runtime, so the blocker is gone. **Still no plan**, and the phase does not move on the strength of a spike | **P1, gap** |
| Configuration file format | Yes `[documented]` | Nothing specified | P1, gap |
| Typed message classes / cracker | Generated per message `[documented]` | Borrowed `MessageView` over the wire buffer | Deliberate — [ADR-0003](decisions/ADR-0003-message-representation.md) |
| Code generation targets | C++, Python, Ruby `[measured]` | Rust only | Not a goal |
| SBE / FAST / FIXML | **None** `[measured]` | Phase 2 | P2 — *ahead* of QuickFIX |
| Throughput | 6,000–8,000 msg/s per session `[documented]` | Target: an order of magnitude more `[unproven]` | P1 |
| **Many counterparties on one acceptor** | Yes — a `SessionSettings` file holds one block per session `[documented]` | **No.** `[verified 2026-09-01]` `Config` pins `target_comp_id` (`session/src/lib.rs:259`) and `Logon` requires the inbound `49=`/`56=` to match it (`:1154`–`:1157`); `serve_sharded_hft` takes **one** `cfg` and hands the same one to every shard (`shard.rs:410`, `:431`). The whole public API serves exactly **one** counterparty | **P1, gap — and the largest one** |
| Logon authentication — `553`/`554`, per-counterparty credentials, IP allowlist | Yes `[documented]` | **Nothing.** Anything presenting the configured comp-ID pair is admitted | P1, gap |
| Ordered shutdown — `Logout`, journal flush, drain | Yes `[documented]` | **Nothing.** `[verified 2026-09-01]` no `shutdown`, `drain` or signal handling anywhere in `crates/*/src`. Dropping the engine while a `WakeHandle` lives was a `SIGPIPE` kill until 2026-08-30 | P1, gap |
| Operator visibility — session state, sequence numbers, counters | Session state via the `Application` callbacks and a log backend `[documented]` | **`Engine::connections() -> usize`, and nothing else.** `[verified 2026-09-01]` that is the entire observable surface: no session state, no `next_out`/`next_in`, no refusal count, no ring depth | P1, gap |
| Sequence-number administration while running | Yes — reset and set, from config or the store `[documented]` | `Session::resume` exists as a **constructor**; there is no path to it on a live engine | P1, gap |
| Reading the message store offline | File and SQL stores are plain, readable formats `[documented]` | mmap journal with **no reader outside the process** | P1, gap |
| Health / readiness probe | n/a — not a library concern for QuickFIX | **Nothing** | P1, gap |
| Production track record | Thousands of counterparties `[documented]` | **Zero** | — |

**The three that matter most:**


1. **Repeating groups — closed 2026-08-28, and here is what closed it.** `[measured]` FIX 4.4
   defines 93 of them, and the 59 acceptance definitions populate exactly one — `386=3` in
   `14i_RepeatingGroupCountNotEqual.def`, a file whose purpose is testing a *wrong* count.
   `454` appears twice, both `=0`. **So 59/59 proves nothing about repeating groups.** The
   gate that does is two independent ones: `group_roundtrip.rs`, which round-trips 357
   top-level positions byte-for-byte across all 59 counters, and
   `interop_quickfix_order.rs`, which agrees with QuickFIX's own generated C++ on 730/730
   groups. The second exists because the first reads the same table the encoder does.
2. **Production track record.** `[documented]` [ADR-0001](decisions/ADR-0001-relationship-to-quickfix.md)
   says it directly: QuickFIX's real value is that thousands of counterparties have already
   found its bugs. No amount of test coverage substitutes. This gap does not close by writing
   code; it closes by being deployed.

3. **Many counterparties on one acceptor — `[verified 2026-09-01]` and it is the gap that
   decides whether this is an *acceptor* or a *link*.** A broker's FIX gateway is
   multi-counterparty by definition. This one is not: `Config` carries a single
   `target_comp_id`, the session refuses any `Logon` whose `49=` does not match it, and every
   entry point — `serve`, `serve_hft`, `serve_sharded_hft` — takes one `Config`. **The routing
   machinery for the opposite already exists and has nowhere to send anything**:
   `presession::identity_of` reads `(49, 56)` off the `Logon` and `HashRoute` spreads distinct
   identities across shards, but every shard rejects all identities but one. Until a registry
   maps an identity to its own `Config`, journal and policy, sharding by identity is routing
   between engines that all say no. It was named once, in the *Blocks* column of `STATUS.md`
   open item 24, and never given a home; it is **open item 28** now.

## 4. What "done" does not mean

`[measured]` The acceptance suite is the primary gate and it has a stated blind spot. Of the
59 FIX 4.4 definitions:

- **Repeating groups: untested** (one populated group, in a negative test).
- **Application-message semantics: untested.** The suite is a *session-layer* suite.
  `[measured 2026-08-29]` it does hand 42 application messages to an application — and only to
  have them echoed back unchanged. Nothing in it asks what an order *means*.
- **Reconnect, backoff, session schedules: untested** — zero definitions, on either side.
- **Field types, enum values, decimal precision: untested.**
- **Application-message resend: implemented and never exercised.** `[verified 2026-09-01]`
  `Session::replay` (`session/src/lib.rs:1030`) reads the journal and re-emits the real message
  with `43=Y` and `122=`, so the code is there. But every message the corpus sends is
  administrative, so **the branch has never run carrying a business message**. A real
  counterparty asking to resend real orders is the first thing that will exercise it, and a
  `ResendRequest` answered wrongly is a protocol violation visible from outside. Open item 29.

59/59 means the session state machine is right. It does not mean the engine is usable. Phase 1
exit criterion 2 and 3 exist because of this paragraph.

## 5. Permanent non-goals

Not "later" — these are out unless a new ADR reverses them.

- **Kernel bypass** (DPDK, OpenOnload, `ef_vi`). Not before an ordinary TCP path has been
  measured and found to be the limit. `DESIGN.md` §8 puts that limit at 10–20 µs. If an ADR
  ever reverses this, the order is fixed in advance — Onload (engine unchanged), then `ef_vi`
  as a second `Transport`, DPDK never because it ships no TCP stack — and it is plaintext
  only: it excludes TLS (D11). `STATUS.md` open item 14.
- **Clustering, HA, replication.**
- **Metrics dashboards, web UIs.**
- **Code generation for languages other than Rust.**
- **Matching engine, order book, risk.** This is a protocol engine.

## 6. Open decisions

| # | Question | Blocks |
|---|---|---|
| 1 | Does the headline positioning stay "the fastest acceptor on kernel TCP" now that the engine is bidirectional? | `DESIGN.md` §1, `README.md` |
| 2 | ~~Repeating groups — which plan owns them?~~ **Answered:** [their own plan](plans/2026-08-27-repeating-groups.md), starting after `codec` step 1. Only the `Dictionary` trait shape lands in step 1, because the trait is public API | Phase 1 exit criterion 2 |
| 3 | Does SBE ride a tag=value session, or does FIXP enter scope? | The size of phase 2, by roughly 5× |
| 4 | One view type or several, once encodings stop having tags on the wire? | Every phase-2 line, and possibly `codec`'s public API today |
| 5 | ~~TLS — own implementation, `rustls`, or terminate outside the process?~~ **Answered by [ADR-0005](decisions/ADR-0005-tls.md)**, which raised six of its own. The blocking one — can `ktls-core` be driven from a non-blocking socket with no async runtime? — is **answered 2026-08-31: yes, with four conditions**, [ADR-0018](decisions/ADR-0018-ktls-on-a-plain-socket-answers-adr-0005.md). Five remain open, and question 2 (which kernel and which cipher suites are the floor) is the one that decides how deployable this actually is | Phase 1 deployability |
| 6 | ~~Final name~~ — **decided 2026-08-30: `fixbolt`**. Was blocking any crates.io publish |
| 7 | **Can the engine configure itself from the machine — mode, cores, bypass — instead of making the caller do it?** `[2026-09-01]` **proposed answer: it detects and advises, it never applies**, plus a hard ceiling of four sessions per `hft` engine that **refuses** the fifth rather than degrading to `standard`. Four is not a round number: `2000 / 448.9 = 4.46`, so it is the largest N that beats an `epoll`-class wakeup under **both** ends of the 2–5 µs literature range, and it sits below the pessimistic bound of the cache wall as well — which is what lets the feature exist without first measuring either. **The remaining uncertainty runs one way only**: 448.9 ns is an *idle* turn, so a busy-path measurement can only lower the ceiling, never raise it. [ADR-0025](decisions/ADR-0025-hft-has-a-hard-session-ceiling-and-the-engine-advises-rather-than-applies.md), **`Proposed`** — deliberately not self-accepted, because its number rests on a run nobody has taken. Bypass detection stays in phase 3 and needs nothing built: `onload ./engine` runs this engine unchanged | `GUIDE.md` §1a's shard arithmetic; the rest of `STATUS.md` open item 21 |
| 8 | **Where does the counterparty registry live — `presession`, `engine`, or `library`?** `[2026-09-01]` `presession` already reads the identity off the `Logon` and is the only layer that sees a socket before a session exists, which argues for it; but a registry owns a `Config`, a journal and a credential per counterparty, which is `library`-shaped. **The choice decides whether a counterparty can be added without a restart**, so it is not a placement detail. First step of the plan that closes open item 28, and it wants an ADR before any code | Every entry point's signature; `GUIDE.md` §1a; hot reload, for ever |
| 9 | **How much does this engine owe an auditor?** A FIX acceptor is usually required to answer *"what exactly did we send that counterparty, and when"* years later. The journal keeps what a **resend** needs — [D7](DESIGN.md) — which is not the same question. **An audit tap (a byte-for-byte copy of both directions, off the hot path via the ring) is a different feature from a message store**, and conflating them is how one ends up on the hot path. Decide the scope before either is built | The journal's format; open item 30's offline reader |
| 10 | **Does the application get bytes, or typed values?** `PRD` §3 lists *decimal / price types* as a phase-1 gap, but [ADR-0003](decisions/ADR-0003-message-representation.md) hands the application a borrowed view on purpose and a typed decimal is exactly the owned, per-message object D2 exists to avoid. **This may already be answered and mislabelled as a gap.** It needs a decision, not an implementation | `codec`'s public API; the phase-1 gap list's honesty |
