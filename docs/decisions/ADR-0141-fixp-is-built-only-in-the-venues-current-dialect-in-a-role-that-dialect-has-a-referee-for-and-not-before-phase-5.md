# ADR-0141 — FIXP is built only in the venue's current dialect, in a role that dialect has a running referee for, as a second pure machine — and not before phase 5

- **Status**: **Accepted — 2026-09-23** (manager, owner's standing mandate; the owner's Q4 asked for the spike then this ADR). Proposed 2026-09-23. Written by the architect (Opus) as phase 3 row 9's last
  output ([plan](../plans/2026-09-23-p3-fixp-spike.md), *Chia việc* row 5); the manager accepts
  it under the owner's standing mandate, or the owner does. It is **the FIXP ADR** that
  [ADR-0097](ADR-0097-phase-3-makes-the-engine-dependable-by-a-stranger-and-fixp-waits-on-a-running-oracle.md)
  decision 5 and [ADR-0078](ADR-0078-sbe-enters-as-an-encoding-without-a-session-and-fixp-is-its-own-phase.md)
  decision 2 require before any FIXP session is planned. It authorises **no build**: it states
  what a build would require and what stays out.
- **Date**: 2026-09-23
- **Deciders**: proposed by the architect; accepted by the manager under the owner's standing
  mandate, or by the owner. Decision 7 (the phase) is the owner's.
- **Related**: [ADR-0140](ADR-0140-the-fixp-spike-speaks-the-referees-own-schema-through-a-detached-probe-and-its-ci-job-blocks.md)
  (how the spike was built); [ADR-0077](ADR-0077-acceptor-first-stays-and-fastest-is-said-only-beside-a-reproduced-pair.md)
  (acceptor-first); [ADR-0081](ADR-0081-sbe-tables-come-from-this-repositorys-generator-and-the-oracle-is-the-specs-bytes-plus-sbe-tool-behind-a-script.md)
  decision 5 (the generator's SBE scope); [ADR-0082](ADR-0082-the-session-is-generic-over-tag-value-encodings-and-the-boundary-to-sbe-is-the-session-not-the-trait.md)
  decision 4 (`Session<Sbe<S>>` is rejected by design); [ADR-0042](ADR-0042-a-second-implementation-is-the-only-independent-opinion.md);
  [ADR-0098](ADR-0098-phase-4-is-the-owners-five-items-each-entering-behind-a-measurement-that-can-kill-it.md)
  decision 7 (no FIXP session build in phase 4); `DESIGN.md` D1, D3, D16; `CLAUDE.md` §2
  non-negotiable 3; `docs/CONFORMANCE.md` §10;
  [b3-binary-entrypoint-facts](../reference/b3-binary-entrypoint-facts.md);
  [sbe-valueref-on-a-composite-member](../reference/sbe-valueref-on-a-composite-member.md).

## Context

### What the spike proved — `[measured 2026-09-23, desk tmt-B450-I-AORUS-PRO-WIFI, commits 473e66d, 2bf09aa, d7f015c, 2375ec1]`

```
fixp-spike: accept PASS 5/5, reject-timestamp PASS, reject-credentials PASS
reject-timestamp: refused ok: NegotiateReject INVALID_TIMESTAMP(7)
reject-credentials: refused ok: NegotiateReject CREDENTIALS(1)
```

A Rust probe encoding and decoding with `fixbolt-sbe`, from tables `fixbolt-sbe-gen` generated
out of the schema inside `artio-binary-entrypoint-codecs-0.184.jar` (`semanticVersion="5.6"`),
completed Negotiate → Establish → Terminate against Artio 0.184's Binary EntryPoint acceptor over
a loopback socket, both directions checked field by field (Artio's decoder read back all seven
Negotiate fields including four `varData`; ours read NegotiateResponse, EstablishAck and
Terminate), and Artio refused the two arms it had to. **What that settles:** the SBE stack
interoperates with a Real-Logic-generated codec on a real venue schema, and a FIXP referee runs
headless in a script and a CI job. **What it does not settle** (`docs/CONFORMANCE.md` §10 *What
is not proven here*): any session behaviour; B3's own dialect; the acceptor role;
`EstablishAck.nextSeqNo`, which Artio echoes from the client's Establish rather than tracks.

### What the spike found — each `[measured 2026-09-23]` unless marked

1. **`sbe-gen` needed `valueRef` on a composite member `<type>`** — fixed in `2bf09aa`, test
   `crates/sbe-gen/tests/` per the reference page above.
2. **B3's current schema needs a second generator change.** `b3-entrypoint-messages-8.4.2.xml`
   stops at `unsupported SBE construct: presence or valueRef on a field whose type is a
   composite`, on twelve fields, among them session fields (`NegotiateResponse.semanticVersion`,
   `EstablishAck.semanticVersion`, both type `Version`, `presence="optional"`). Real Logic's
   `sbe-tool` accepts the attribute on any field and takes nullness from the composite's own
   members (`sbe-tool/src/main/java/uk/co/real_logic/sbe/xml/Message.java:307-325`,
   <https://github.com/aeron-io/simple-binary-encoding>, read 2026-09-23). Whether a third gap
   hides behind the second is **unmeasured**: 8.4.2 carries 96 `sinceVersion` attributes,
   including one on a composite (`Version sinceVersion="4"`).
3. **The referee's dialect and the venue's differ in the session layer itself, not only in
   application messages.** A scripted diff of messages 1–14 between Artio's 5.6 copy and B3's
   8.4.2 (scratch, never committed):
   - 8.4.2 **removes** `FinishedSending` (10) and `FinishedReceiving` (11);
   - 8.4.2 **appends** `semanticVersion` to NegotiateResponse and EstablishAck,
     `currentSessionVerID` to NegotiateReject, `lastIncomingSeqNo` to EstablishReject;
   - 8.4.2 application messages carry an explicit `msgSeqNum` in an inbound / outbound business
     header; 5.6 application messages carry none (FIXP's implicit sequencing);
   - `DeltaInMillis.time` changes from optional (null `0`) to required.

   So a session machine built to pass against Artio would be a machine for a sequencing model
   B3 has since replaced. `[not verified]`: whether B3's gateways still accept schema version 5
   at all — the B3 page lists only 8.3.2 and 8.4.2
   (<https://www.b3.com.br/en_us/solutions/platforms/puma-trading-system/for-developers-and-vendors/entrypoint/>).
4. **Framing is B3-specific**: 2-byte little-endian length plus `0xEB50`, not FIXP's standard
   SOFH (reference page, fact 1). The probe wrote it by hand; nothing in `crates/` knows it.
5. **Artio answers a timestamp refusal only after ~5 s** (its authentication timeout), a
   credentials refusal after ~0.5 s (reference page, trap 8) — a behaviour a server-side machine
   must decide for itself rather than copy.

### Who can judge which role — the referee inventory

| Dialect | Judges a fixbolt **initiator** (referee plays acceptor) | Judges a fixbolt **acceptor** (referee plays initiator) |
|---|---|---|
| Artio's 5.6 | **Artio 0.184 acceptor — runs, proven by the spike** | none: Artio's only client is `BinaryEntryPointClient`, a JUnit helper, not a released initiator (ADR-0140 *Facts found*) |
| B3's 8.4.2 | **none runnable**: Artio lags at 5.6; B3's certification environment is not reachable from CI | **B3EntryPointClient** (C#, MIT, one author, 0 stars): a client plus a conformance suite by spec section (§4.6 sequence/heartbeat, §4.7 retransmit/NotApplied, §4.8 terminate/reconnect/cancel-on-disconnect) that targets an external peer via `B3EP_PEER`, `B3EP_SESSION_ID`, `B3EP_SESSION_VER_ID`, `B3EP_FIRM`, `B3EP_ACCESS_KEY` — <https://github.com/pedrosakuma/B3EntryPointClient>, `[documented, not run]` |

Other implementations found: `fefixp` 0.1.0 and `rustyfixp` 0.7.4 (message structs, 5.56 %
documented — <https://docs.rs/fefixp>, <https://docs.rs/rustyfixp>); Silverflash, the FIXP
reference implementation, no commit since 2020-10-13
(<https://github.com/FIXTradingCommunity/silverflash>). **Searched, found nothing:** a public
FIXP conformance corpus; a released open-source B3 8.x acceptor; a Go or C++ FIXP implementation.
CME iLink 3 is FIXP too, but signs Negotiate and Establish with HMAC and is judged only by CME's
certification environment
(<https://cmegroupclientsite.atlassian.net/wiki/spaces/EPICSANDBOX/pages/714145834/iLink+Binary+Order+Entry+-+Session+Layer>),
and Artio supports it only as an initiator (<https://github.com/artiofix/artio/wiki/FIXP-Support>).

## The seven questions and their answers

The plan's row 5 names them (i)–(vii); decisions 1–7 below answer them in that order.

## Decision

1. **(i) What replaces non-negotiable 3 for FIXP: a running referee per role *and* a written
   scenario corpus, both blocking.** A FIXP session change is done only when (a) the pure
   machine passes a scenario corpus this repository writes itself, one scenario per clause of
   the FIXP 1.0 Technical Standard §4 (point-to-point) and of B3's messaging guidelines that the
   machine implements, each scenario naming its clause, run in process like the 59 `.def`s; and
   (b) a second implementation drives the same role over a socket, in the same dialect, and is
   seen both to accept and to refuse. (a) alone is the author grading his own homework — it is
   the regression net, not the judge; (b) is the independent opinion ADR-0042 requires. **A role
   in a dialect with no running referee is not built.** The spec's text is CC BY-ND 4.0, so
   scenarios are written in this repository's own words and cite section numbers; no clause is
   copied.
2. **(ii) The role is chosen by the referee, and in the venue's current dialect that is the
   acceptor.** Decision 1 plus the inventory leaves exactly two buildable pairs: a 5.6 initiator
   judged by Artio, and an 8.4.2 acceptor judged by B3EntryPointClient's suite. Decision 3 rules
   out the first. The second agrees with ADR-0077's acceptor-first positioning, and its users are
   real but narrow: exchange simulators and test venues for firms certifying B3 clients — not
   B3 itself. **A fixbolt FIXP initiator waits for a referee that speaks 8.x**: an Artio release
   with a current schema, or access to B3's certification environment from somewhere a gate can
   run. Before any acceptor build, a second spike must show B3EntryPointClient's suite running
   headless and **refusing** something — ADR-0140 decision 6's rule, applied again — because
   today it is `[documented, not run]`, with one author and no users.
3. **(iii) The target schema is the venue's current one, never the referee's older copy.** Finding
   3 shows 5.6 and 8.4.2 differ in session messages and in the sequencing model; a machine
   passing against Artio's 5.6 would be built for a dialect nobody is shown to run — the same
   reason ADR-0078 decision 3 declined SBE inside FIXT. Preconditions, in order, each its own
   step with a red-first test on a fixture written here (the pattern of
   [sbe-valueref-on-a-composite-member](../reference/sbe-valueref-on-a-composite-member.md)):
   (a) `sbe-gen` accepts `presence="optional"` on a composite-typed field, nullness carried by
   the composite's members as `sbe-tool` does, `presence="constant"` there still refused — this
   **widens ADR-0081 decision 5**, and the build plan says so; (b) `generate()` then returns `Ok`
   on 8.4.2, or the next gap is named and scheduled the same way. The pinned version moves only
   by an ADR amendment, because every scenario in decision 1 is written against it.
4. **(iv) Framing is a transport concern, selected per listener; it enters neither `crates/sbe`
   nor the session machine.** The engine's read path splits frames — today by tag=value
   `BodyLength`, for FIXP by a framing header — and hands the machine whole messages; on write
   the machine fills the SBE message and the engine writes the 4 framing bytes in front. B3's
   compact little-endian header is the first framing; the FIXP-standard big-endian SOFH is a
   second, added only with a venue that uses it. `crates/sbe` stays an encoding (ADR-0079).
5. **(v) The session is a second pure machine beside `Session`, not `Session<Sbe<S>>`.** ADR-0082
   decision 4 already makes `Session<Sbe<S>>` fail to compile, and FIXP shares none of the seven
   tag=value administrative messages D1's `Session` owns. The new machine keeps D1's contract
   whole: no socket, no clock, no allocation, no `format!`; time arrives only as `Tick`; the four
   inputs `connect`, `disconnect`, `tick`, `received` with a caller-supplied `emit`; fieldless
   errors; layouts from generated tables (D3, non-negotiable 5); both engine modes
   (non-negotiable 4). What the machine must hold, for B3's fixed flows (client idempotent,
   server recoverable):
   - **Negotiation and binding**: `sessionVerID` rises with every Negotiate; a reconnect may
     re-Establish without re-Negotiating; the timestamp window is checked against the last
     `Tick`, so the check stays pure.
   - **Inbound, idempotent (client → server)**: gaps in the client's `msgSeqNum` are answered
     with `NotApplied(fromSeqNo, count)`; the server never asks the client to resend.
   - **Outbound, recoverable (server → client)**: `RetransmitRequest` is answered by
     `Retransmission` plus the stored messages, or by `RetransmitReject` with a named code — so
     the acceptor needs a resend store, and the build plan decides whether `Journal` (ADR-0008,
     ADR-0046) serves it unchanged or needs an ADR.
   - **Liveness**: `Sequence` when the keep-alive interval passes idle; `Terminate` with
     `KEEPALIVE_INTERVAL_LAPSED` when the peer is silent past its own interval; both driven by
     `Tick`.
   - **Refusal timing is the machine's choice**, not Artio's (finding 5): a refused Negotiate
     ends the connection promptly on every refusal path, and a scenario in decision 1 holds it.
6. **(vi) The `fixp-spike` job stays, blocking, as the SBE interop check.** It is the only check in
   this repository of `fixbolt-sbe` against a decoder `sbe-tool` generated from a real venue
   schema, and that stays true whatever happens to FIXP. It keeps speaking Artio's 5.6 — a codec
   check needs a codec peer, not a dialect — and is retired only by a later ADR that names what
   replaces it. `docs/CONFORMANCE.md` §10's run id is filled from its first run on a merged
   commit.
7. **(vii) A FIXP session is not phase 3 and not phase 4; phase 5 at the earliest, and only if the
   owner opens it.** Phase 3 ends with this ADR (ADR-0097 decision 5); ADR-0098 decision 7 keeps
   the build out of phase 4. The owner decides whether a later phase takes it, knowing it would
   deliver **an 8.4.2 B3-dialect acceptor judged by a one-author third-party suite**, not a B3
   client and not a certification. Its plan cannot be written until decision 3's (a) and (b)
   are green and decision 2's second spike has run.

**Stays out, by name:** a FIXP session in phase 3 or 4; a FIXP initiator until an 8.x referee
runs; the 5.6 dialect as a build target; iLink 3 (HMAC-signed, CME-judged, Artio initiator-only);
FIXP multicast (`Topic`), UDP and WebSocket transports; the FIXP-standard SOFH before a venue uses
it; SBE inside FIXT (ADR-0078 decision 3); FIXP over TLS; business-message semantics
(`SimpleNewOrder`, `ExecutionReport` …) beyond what the session must sequence; any FIXP latency
figure (non-negotiable 10 would need a committed benchmark first); B3 certification.

## Consequences

**Good**

- The design question that could have wasted a whole phase — building a session for the
  referee's dialect instead of the venue's — is settled on a page, with the diff that shows it.
- The rule for FIXP's gate is written before any machine exists, so it cannot be bent to fit one.
- The chosen role agrees with ADR-0077, and the codec check the spike produced keeps its value
  with no further FIXP work.
- Every precondition is a command that fails or passes; none of them waits on a person outside
  this repository except the owner's phase decision.

**Bad — and accepted**

- **The only candidate referee for the chosen pair is weak**: one author, no users, a suite that
  has never been run from here. If the second spike shows it cannot run headless or cannot
  refuse, no FIXP role in the current dialect has a referee and the build does not start.
- **The users of a B3 *acceptor* are few.** Most people who want FIXP want to connect to B3, as
  an initiator — the role this ADR defers. A reader looking for "fixbolt speaks B3" will not find
  it.
- **The spike's own success is partly stranded.** It proved a 5.6 initiator handshake that
  decision 3 says not to build on; its lasting value is the codec check (decision 6).
- **Widening `sbe-gen` for 8.4.2 is open-ended**: one more gap is measured, the number behind it
  is not, and each change moves ADR-0081 decision 5's line.
- **A self-written scenario corpus is weaker than the 59 `.def`s**, which someone else wrote. It
  catches regressions, not misreadings of the spec; only the referee catches those, and only in
  the clauses its suite covers.
- **A blocking 5.6 CI job stays in the pipeline** for a dialect this ADR says not to build — paid
  for by every pull request, for the codec check alone.
- **Phase 5 is not defined.** Decision 7 points at a phase no ADR has scoped.
