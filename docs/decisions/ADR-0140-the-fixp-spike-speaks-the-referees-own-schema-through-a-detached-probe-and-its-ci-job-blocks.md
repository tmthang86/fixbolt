# ADR-0140 — The FIXP spike speaks the referee's own schema through a detached probe, and its CI job blocks

- **Status**: **Accepted — 2026-09-23** (manager, owner's standing mandate, with the phase 3 row 9 plan). Proposed 2026-09-23. Written by the architect (Opus) for phase 3 row 9
  ([plan](../plans/2026-09-23-p3-fixp-spike.md)); accepted, if at all, by the manager under the
  owner's standing mandate, with that plan. This ADR decides **how the spike is built**. It does
  not decide anything about a FIXP session: that is the FIXP ADR the spike's last row writes
  ([ADR-0097](ADR-0097-phase-3-makes-the-engine-dependable-by-a-stranger-and-fixp-waits-on-a-running-oracle.md)
  decision 5).
- **Date**: 2026-09-23
- **Deciders**: proposed by the architect; accepted by the manager under the owner's standing
  mandate, or by the owner.
- **Related**: [ADR-0097](ADR-0097-phase-3-makes-the-engine-dependable-by-a-stranger-and-fixp-waits-on-a-running-oracle.md)
  decision 5 and Q4 (B3 Binary EntryPoint, spike only);
  [ADR-0078](ADR-0078-sbe-enters-as-an-encoding-without-a-session-and-fixp-is-its-own-phase.md)
  decision 2 (FIXP opened only with a venue and an oracle);
  [ADR-0081](ADR-0081-sbe-tables-come-from-this-repositorys-generator-and-the-oracle-is-the-specs-bytes-plus-sbe-tool-behind-a-script.md)
  decisions 1, 3 and 5 (the generator, its oracles, its SBE scope);
  [ADR-0042](ADR-0042-a-second-implementation-is-the-only-independent-opinion.md);
  [ADR-0018](ADR-0018-ktls-on-a-plain-socket-answers-adr-0005.md) (the `spikes/` precedent);
  [ADR-0001](ADR-0001-relationship-to-quickfix.md) (third-party data is fetched, never committed).

## Context

Phase 3 row 9 must show, as a command in CI, that a second FIXP implementation runs headless
against a trivial client, and then write the FIXP ADR from what that showed. Four questions have
to be settled before a developer can be briefed, and each is expensive to reverse once a CI job
depends on it: what the client is, what the referee is, which schema both speak, and whether the
job may fail a pull request.

### Facts found — `[documented]` unless marked

| Source | What it says | Bearing |
|---|---|---|
| <https://github.com/artiofix/artio/wiki/FIXP-Support> | *"Binary EntryPoint: Acceptor/Server only"*; iLink3 initiator only; *"an Artio instance only supports a single FIXP protocol"* | Artio can judge a fixbolt **initiator** only |
| <https://github.com/artiofix/artio/wiki/Binary-EntryPoint-Support> | `configuration.acceptFixPProtocol(FixPProtocolType.BINARY_ENTRYPOINT)`, `fixPAuthenticationStrategy((context, authProxy) -> authProxy.accept())`, `FixPConnectionExistsHandler` + `FixPConnectionAcquiredHandler`; *"Acceptor only, doesn't support Engine owned sessions, sole library mode"* | The public API a referee of our own needs |
| `artio-samples/.../example_fixp_exchange/FixPExchangeApplication.java` (<https://github.com/artiofix/artio>, master, read 2026-09-23) | Launches `ArchivingMediaDriver` **in process** beside `FixEngine`; Aeron Archive control on `aeron:udp?endpoint=localhost:10010` / `10020` | No separate media-driver process; two localhost UDP ports |
| `artio-binary-entrypoint-impl/.../BinaryEntryPointContext.java` (same repo) | The context passed to the authentication strategy exposes `sessionID()`, `sessionVerID()`, `enteringFirm()`, `credentials()`, `clientIP()`, `clientAppName()`, `clientAppVersion()` | The referee can read back **every** field and all four `varData` the client wrote |
| `.../InternalBinaryEntryPointConnection.java` (same repo) | Rejects a Negotiate whose timestamp is outside `sendingTimeWindow` (`NegotiationRejectCode.INVALID_TIMESTAMP`); rejects Establish on bad timestamp or `nextSeqNo`; answers a client Terminate with a Terminate | The referee can say **no**, in named codes |
| `artio-codecs/.../fixp/SimpleOpenFramingHeader.java` (same repo) | B3 framing: `SOFH_LENGTH = 4`, message size `uint16` little-endian, `BINARY_ENTRYPOINT_TYPE = (short)0xEB50` little-endian | B3's SOFH is a compact LE variant, not the FIXP-standard 4-byte BE length |
| <https://github.com/artiofix/artio/blob/master/.github/workflows/ci.yml> | `ubuntu-24.04`, Java 17 and 25, `timeout-minutes: 30`, `./gradlew clean build` (includes `artio-binary-entrypoint-system-tests`); last three runs 8 min each | A Binary EntryPoint acceptor already runs on GitHub-hosted runners |
| Maven Central `uk.co.real-logic:artio-*` metadata `[measured 2026-09-23]` | `0.184`, updated 2026-09-18; `artio-core` → `aeron-client`, `aeron-archive` `1.53.2`, `HdrHistogram 2.2.2`; `artio-codecs` → `agrona 2.6.1`, `sbe-tool 1.40.2`; `sourceCompatibility = 17` | 11 jars, 5.2 MB, all Apache-2.0 |
| `binary_entrypoint.xml` inside `artio-binary-entrypoint-codecs-0.184.jar` `[measured 2026-09-23]` | `package="b3.entrypoint.fixp.sbe" id="1" version="5" semanticVersion="5.6"`, little-endian, dated 2022-08-24; SHA-256 `c31fcd6228e613fa6ee3af441832393a4a35c30263bb5cd80929f16529af4a71`, byte-identical to the file on Artio's `master` | The schema the referee actually speaks |
| <https://www.b3.com.br/en_us/solutions/platforms/puma-trading-system/for-developers-and-vendors/entrypoint/> | Publishes `b3-entrypoint-messages-8.4.2.xml` (and 8.3.2); **no terms of use or licence stated** for the XML | Current venue schema is `version="6" semanticVersion="8.4.2"`; three majors ahead of the referee |
| <https://github.com/FIXTradingCommunity/fixp-specification> `v1-0-STANDARD/doc/06SummaryOfSessionMessages.md` | Negotiate / NegotiationResponse / NegotiationReject, Establish / EstablishmentAck / EstablishmentReject, Sequence, RetransmitRequest / Retransmission (recoverable only), Terminate, FinishedSending / FinishedReceiving; Applied / NotApplied for idempotent | The probe's sequence is the spec's initialization, binding and unbinding stages |
| same repo, `03CommonFeatures.md` | *"Recoverable: Guarantees exactly-once message delivery"*; *"Idempotent: Guarantees at-most-once delivery"* | B3's schema pins `clientFlow = IDEMPOTENT`, `serverFlow = RECOVERABLE` as constants |
| <https://github.com/aeron-io/simple-binary-encoding> `sbe-tool/.../xml/EncodedDataType.java:81-130` | A `<type>` element reads `valueRef` and resolves it to the constant | Real Logic's generator accepts `valueRef` on a composite member |
| SBE 1.0 Standard, `vendor/sbe-spec/v1-0-STANDARD/doc/04MessageSchema.md:449-464` and `resources/sbe.xsd:272,368` `[read in repo]` | `valueRef` is listed under **Field attributes** and declared on `fieldType` only | The normative table allows it on `<field>` only |
| same, `doc/02FieldEncoding.md:884,902,921,936` `[read in repo]` | The spec's own `UTCTimestampNanos` examples put `presence="constant" valueRef="TimeUnit.nanosecond"` on a composite member `<type>` | The spec contradicts its own table; sbe-tool and B3 follow the example |

### Measured here — `[measured 2026-09-23, desk tmt-B450-I-AORUS-PRO-WIFI, a scratch crate outside the tree calling fixbolt_sbe_gen::generate at 2f0a0dc]`

```text
binary_entrypoint.xml: ERR schema error: constant '' is not an unsigned integer: cannot parse integer from empty string
b3-8.4.2.xml:          ERR schema error: constant '' is not an unsigned integer: cannot parse integer from empty string
```

With the three `valueRef="TimeUnit.*"` members rewritten to literal constants (`9`, `3`) in a
scratch copy, and nothing else changed:

```text
hacked-binary_entrypoint.xml: OK, 121134 bytes generated
hacked-b3-8.4.2.xml:          ERR unsupported SBE construct: presence or valueRef on a field whose type is a composite
```

The second error comes from twelve fields in 8.4.2, one of them in a session message
(`NegotiateResponse.semanticVersion`, type `Version`, `presence="optional"`); the rest are
application messages (`price`, `stopPx`, `custodianInfo`, `receivedTime`, …). Artio's 5.6 copy
has none of them.

**Searched, found nothing:** a published Binary EntryPoint *initiator* from Artio (the only
client is `BinaryEntryPointClient`, a JUnit helper in `artio-binary-entrypoint-system-tests`); a
licence or terms statement for B3's schema XML; a FIXP conformance corpus (unchanged since
ADR-0097).

## Options considered

**The client.** (a) Artio's `BinaryEntryPointClient`: runs today, but it is Artio talking to
Artio — it proves the referee starts and nothing about this repository. (b) A Rust probe on
`fixbolt-sbe`: the real question a FIXP session would ask first — *do our generated tables and
our encoder produce bytes a Real-Logic-generated decoder accepts, and do we read its replies* —
and it is small (four messages out, four kinds in). **(b).**

**Where the probe lives.** (a) `tools/fixp-probe`, a workspace member with its schema generation
behind a feature: keeps workspace lints, but its `build.rs` needs a fetched venue schema, so
every `cargo` invocation on the root either grows a new `vendor/` dependency or grows a feature
the `feature-sets` and `no-default-features` jobs must learn. (b) `spikes/fixp-probe`, detached
(empty `[workspace]`, listed in the root `exclude`), exactly as `spikes/ktls` is: invisible to
`cargo test --all`, built only by its script and its CI job. A spike answers a question and is
not shipped. **(b)**; the FIXP ADR decides whether it graduates to `tools/`.

**The schema.** (a) B3's current 8.4.2: what production speaks, but the referee does not, and
our generator cannot read it without widening ADR-0081 decision 5. (b) The copy inside the
referee's own codecs jar, 5.6: what the other end of the socket actually decodes with. **(b)**;
the version skew is itself a finding for the FIXP ADR.

**The CI job.** (a) Non-blocking (`continue-on-error`): cheap, but a job nobody can fail is read
by nobody, and this repository's rule is that a check proves nothing until something reads it.
(b) Blocking: costs a second JVM job and a Maven Central dependency on every pull request — the
same cost ADR-0130 accepted for QuickFIX/J. **(b)**, for the reason in decision 5.

## Decision

1. **The probe is Rust, in `spikes/fixp-probe`, detached from the workspace.** It depends on
   `fixbolt-sbe` (normal) and `fixbolt-sbe-gen` (build) by path; its `build.rs` generates tables
   from `vendor/fixp/binary_entrypoint.xml` and fails with a message naming
   `scripts/fixp-spike.sh` when the file is absent. It is **one straight-line program**, not a
   session machine: no state type, no retransmission, no timer beyond a read deadline. Nothing
   in `crates/` or `tools/` depends on it. It writes the 4-byte B3 framing header itself
   (`u16` LE length including the header, `u16` LE `0xEB50`), citing Artio's
   `SimpleOpenFramingHeader.java`; framing does not enter `crates/sbe`.
2. **The referee is our own Java program on Artio's public API**, `spikes/fixp-probe/referee/Referee.java`
   (the pattern of `tools/interop-qfj/Judge.java`): `EngineConfiguration.acceptFixPProtocol(BINARY_ENTRYPOINT)`,
   an in-process `ArchivingMediaDriver`, a `FixLibrary` that acquires the connection. Its
   authentication strategy compares every field of the `BinaryEntryPointContext` — including the
   four `varData` — with expected values passed on its command line, prints one line per field,
   and **rejects** on any mismatch. So the referee judges our encoder, and the probe (decoding
   with our tables) judges the referee's encoder: the check runs in both directions.
3. **Both ends speak the schema inside the pinned `artio-binary-entrypoint-codecs-0.184.jar`**,
   extracted by the script and checked against SHA-256 `c31fcd62…4a71`. The 11 jars are pinned
   by SHA-256 as `scripts/interop-qfj.sh` pins its five. **No B3 schema byte is ever committed**
   — B3 states no terms for the file, so it is handled like an exchange specification under
   `.gitignore`'s confidential section: fetched into `vendor/`, never staged.
4. **`sbe-gen` gets exactly one change in this plan: `valueRef` on a composite member `<type>`.**
   It follows the SBE 1.0 Standard's own examples and Real Logic's `sbe-tool`, over the
   narrower attribute table and `sbe.xsd`; the error for an unresolvable `valueRef` names
   `valueRef`, not an empty constant. It is a senior-developer step (layouts are D3's generated
   tables). The second gap — `presence` on a composite-typed field, which B3 8.4.2 needs — is
   **not** fixed here: it would widen ADR-0081 decision 5, and it is written into the FIXP ADR as
   a precondition of targeting the current venue schema.
5. **The CI job `fixp-spike` is blocking.** Beyond ADR-0097 decision 5's wording (*"prove, as a
   command in CI"*), it is the first check anywhere in this repository of `fixbolt-sbe`'s encoder
   against a decoder generated by Real Logic's tool on a real venue schema — the second
   implementation `PRD.md` §2 *Phase 2 starts with an architectural decision* (SBE row) still lists as unbuilt (`tools/sbe-interop`). That value survives
   whatever the FIXP ADR decides. The FIXP ADR states the job's fate: kept as the SBE interop
   check, promoted into a FIXP session gate, or deleted.
6. **The referee must be seen to refuse.** Beside the passing arm, the job runs two arms that
   must end in a named rejection: a Negotiate timestamp outside the referee's window
   (`NegotiateReject` with `INVALID_TIMESTAMP`, Artio's own check) and a wrong `credentials`
   value (`NegotiateReject` from our referee's check). A referee that has only ever said yes has
   judged nothing.

## Consequences

**Good**

- The spike answers the question a FIXP session would face first — does our SBE stack
  interoperate with a real FIXP peer — before anyone designs a session machine.
- A one-line generator fix, measured to be the only thing between `sbe-gen` and the referee's
  schema, is found by a scratch run on the design page rather than mid-build.
- Nothing in the workspace changes shape: `cargo test --all`, the feature-set jobs and
  `--no-default-features` never see the probe or the venue schema.
- The two-direction check and the two refusal arms make a green mean something.

**Bad — and accepted**

- **The referee lags the venue by three major schema versions** (5.6 against 8.4.2). A green
  spike says fixbolt speaks *Artio's* Binary EntryPoint, not B3's production dialect. The FIXP ADR
  must say what that is worth.
- **It judges only a fixbolt initiator.** The acceptor-first positioning (ADR-0077) gets no
  evidence from this spike; the only acceptor-side counterparts found are a third-party C#
  client (MIT, 0 stars) and two early Rust crates.
- **A second JVM job blocks every pull request** on Maven Central and on Artio's and Aeron's
  runtime behaviour on a shared runner (Aeron needs shared memory and two localhost UDP ports).
  A flake there reads as a red PR unrelated to the change.
- **A detached crate forgoes workspace lints**; the job runs `cargo clippy --manifest-path`
  itself, and the probe copies the workspace's lint table, or its warnings go unseen.
- **The generator's rule becomes looser than the spec's table.** A schema that puts `valueRef`
  on a `<type>` is now accepted although `sbe.xsd` would reject it; that choice follows the
  reference implementation and the venue, and is recorded in `docs/reference/`.
- **The version skew and the second generator gap are deferred, not solved.** Targeting 8.4.2 is
  a later ADR-0081 scope change with its own tests.
