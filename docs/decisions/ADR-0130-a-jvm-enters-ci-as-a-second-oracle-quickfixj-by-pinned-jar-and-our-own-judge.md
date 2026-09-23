# ADR-0130 — A JVM enters CI as a second oracle: QuickFIX/J by pinned jar, judged by a harness of our own, the initiator through the engine

- **Status**: Accepted — 2026-09-23 (by the manager under the owner's standing mandate, with the phase 3 row 5 plan). Proposed 2026-09-23
- **Date**: 2026-09-23
- **Deciders**: Tran Manh Thang (approval by the manager under the owner's standing mandate of
  2026-09-18). Written by the architect (Opus) from ADR-0097 decision 7 criterion 6,
  [plans/2026-09-23-phase-3-scope](../plans/2026-09-23-phase-3-scope.md) *Chia việc* row 5 and
  *Bẫy đã lường trước*, `scripts/interop.sh` §1–§5, `tools/interop/src/main.rs`,
  `crates/engine/src/tls.rs`, and the research below.
- **Related**: [ADR-0001](ADR-0001-relationship-to-quickfix.md) (QuickFIX assets are data and
  oracle; decision 5: `NOTICE` only if QuickFIX-derived text or data ships);
  [ADR-0004](ADR-0004-bidirectional-engine.md) decision 7 (C++ lives in CI and `scripts/`, never
  in a `Cargo.toml`); [ADR-0042](ADR-0042-a-second-implementation-is-the-only-independent-opinion.md)
  decisions 2–4; [ADR-0060](ADR-0060-a-deployment-that-requires-the-kernel-is-refused-twice.md)
  (a userspace fallback is reported and, under `TlsRequireKernel=Y`, refused);
  [ADR-0063](ADR-0063-a-peers-key-update-is-the-second-named-carve-out-and-a-ticket-is-not-read.md) (tickets are ignored);
  [ADR-0097](ADR-0097-phase-3-makes-the-engine-dependable-by-a-stranger-and-fixp-waits-on-a-running-oracle.md)
  decision 7 criterion 6.
- **Plan**: [plans/2026-09-23-p3-quickfixj-interop](../plans/2026-09-23-p3-quickfixj-interop.md)
- **Note**: section "Sửa 1 — 2026-09-23" of [that plan](../plans/2026-09-23-p3-quickfixj-interop.md)
  (2026-09-23, manager-accepted) records a build-time deviation from decision 6's "an IP SAN of
  `127.0.0.1`" wording — the `fixbolt-acceptor` leaf also carries `DNS:localhost`, with the
  `javap` evidence for why. This ADR's substance is unchanged; the plan is the record.

## Context

ADR-0097 criterion 6: *"`scripts/interop.sh` against QuickFIX/J, both roles, **7 / 7**, plaintext
**and** TLS, as a blocking CI job."* Today the only second implementation in CI is `libquickfix`
(C++), built from source at a pinned commit by `scripts/interop.sh`, in plaintext only
(`-DHAVE_SSL=OFF`, `scripts/interop.sh` §1). TLS has no counterparty but this engine itself:
`crates/engine/tests/tls*.rs` put `rustls` on both ends.

Four facts shape the decision:

1. **The `libquickfix` initiator direction drives the pure session, not the engine**
   (ADR-0042 decision 4): `tools/interop --role initiator` is `Session<Initiator, 256>` over a
   blocking `TcpStream`. That path has no TLS and never will: the kTLS handover lives in the
   engine's `connect_and_serve_tls` (`crates/engine/src/lib.rs`, `tls.rs` `ClientTls`).
2. **The phase-3 scope names the trap**: *"Interop TLS xanh nhưng thực ra rơi về userspace TLS"*.
   `EventKind::TlsFellBackToUserspace` (`crates/engine/src/observe.rs`) and `TlsRequireKernel`
   exist for it; `/proc/net/tls_stat` is a count the kernel keeps and the engine cannot write.
3. **This engine's TLS is narrow on purpose**: TLS 1.3 only, `TLS13_AES_128_GCM_SHA256` only,
   no client authentication on the acceptor, server certificate always verified against
   `CertificationAuthoritiesFile` on the initiator (`tls.rs` `server_config`, `client_config`).
4. **`tools/interop --role acceptor` already calls `fixbolt::serve`** with `desk::Desk`, stopped
   through `Admin::shutdown` on a stdin line. `Settings::into_table` refuses a file with
   `SocketUseSSL=Y` (`NeedsTlsDoor`); `into_tls_table` + `tls::load_pem` + `serve_tls_requiring`
   is the TLS door, `into_tls_initiator` + `tls::load_client_pem` + `connect_and_serve_tls` the
   initiator's.

## Research

| Question | Found | Source |
|---|---|---|
| Current release, coordinates | `org.quickfixj:quickfixj-core` **3.0.2**, published to Maven Central 2026-08-04 (`latest` = `release` = 3.0.2) | <https://repo1.maven.org/maven2/org/quickfixj/quickfixj-core/maven-metadata.xml>, <https://repo1.maven.org/maven2/org/quickfixj/quickfixj-core/3.0.2/> |
| Runtime closure | core depends at compile scope on `quickfixj-base`, `mina-core`, `slf4j-api` (and `HikariCP`, used only by the JDBC store); parent pins MINA **2.2.9**, SLF4J **2.0.18**, bytecode target Java **8** | <https://repo1.maven.org/maven2/org/quickfixj/quickfixj-core/3.0.2/quickfixj-core-3.0.2.pom>, <https://repo1.maven.org/maven2/org/quickfixj/quickfixj-parent/3.0.2/quickfixj-parent-3.0.2.pom> |
| Where `FIX44.xml` comes from | packaged inside `quickfixj-messages-fix44` (the jar-plugin includes `FIX44.xml` and `FIX44.modified.xml`) — QFJ's own dictionary, not the `libquickfix` spec file | <https://repo1.maven.org/maven2/org/quickfixj/quickfixj-messages-fix44/3.0.2/quickfixj-messages-fix44-3.0.2.pom> |
| SSL settings | `SocketUseSSL`, `SocketKeyStore`, `SocketKeyStorePassword`, `KeyStoreType`, `SocketTrustStore`, `SocketTrustStorePassword`, `TrustStoreType`, `NeedClientAuth`, `EnabledProtocols`, `CipherSuites`, `UseSNI`, `SNIHostName`, `EndpointIdentificationAlgorithm` | <https://quickfixj.org/docs/configuration/> |
| SSL defaults | default keystore `quickfixj.keystore` / `quickfixjpw`, store type `JKS`, protocols and suites fall back to the JSSE defaults, endpoint identification `null` (no host check) | <https://github.com/quickfix-j/quickfixj/blob/master/quickfixj-core/src/main/java/quickfix/mina/ssl/SSLSupport.java> |
| Who verifies whom | *"Acceptor certificates are always authenticated by the initiator"* — the QFJ initiator needs a trust store holding our CA | <https://github.com/quickfix-j/quickfixj/blob/master/quickfixj-core/src/main/doc/usermanual/usage/secure_communications.html> |
| TLS 1.3 under MINA | DIRMINA-1132, messages randomly dropped under TLS 1.3, fixed in MINA 2.2.2 (QFJ 3.0.2 ships 2.2.9) | <https://issues.apache.org/jira/browse/DIRMINA-1132> |
| TLS 1.3 on old JDK 8 | *"closing inbound before receiving peer's close_notify"*, cured by a newer JDK / QFJ 2.3.2 | <https://sourceforge.net/p/quickfixj/mailman/message/59126185/> |
| Initiator Logon before the TLS handshake finishes | an NPE in `SslFilter.filterWrite` on QFJ 3.0.2 / JDK 17 when the handshake takes seconds; no fix recorded. **Unverified**: filed on `github.com/quickfixj/quickfixj`, not the project's usual `quickfix-j` organisation | <https://github.com/quickfixj/quickfixj/issues/1> |
| JSSE server after a TLS 1.3 handshake | sends `NewSessionTicket` (count set by `jdk.tls.server.newSessionTicketCount`) | <https://bugs.openjdk.org/browse/JDK-8333581>, <https://bugs.openjdk.org/browse/JDK-8242399> |
| Field-order rejections | QFJ rejects *"Tag specified out of required order"* under `ValidateFieldsOutOfOrder=Y` (default) | <https://github.com/quickfix-j/quickfixj/discussions/571> |
| Latency / precision defaults | `CheckLatency=Y`, `MaxLatency=120`, `TimeStampPrecision=MILLIS`, `ValidateUserDefinedFields=Y`, `AllowUnknownMsgFields=N`, `ResetOnLogon=N` | <https://quickfixj.org/docs/configuration/> |
| Licence | *"The QuickFIX Software License, Version 1.0"*; binary **redistribution** must reproduce the notice, and end-user documentation *"included with the redistribution"* must carry the acknowledgment | <https://raw.githubusercontent.com/quickfix-j/quickfixj/master/LICENSE> |
| `actions/setup-java` | latest major v6 (6.0.1) | <https://github.com/actions/setup-java/releases> |

**Searched and found nothing**: a public report of QuickFIX/J against any Rust FIX engine; a
report of JSSE against a kTLS peer; a QFJ-specific `SendingTime`/`CheckLatency` interop defect
(only the defaults above).

## Options considered

**How QFJ is obtained.** (a) Maven with a `pom.xml` committed under `tools/`; (b) `curl` of the
exact jars from Maven Central, each checked against a SHA-256 pinned in the script; (c) Gradle.
Chosen **(b)**. The closure is five jars; a build tool adds a resolver whose checksum policy is
`warn` by default and a cache to reason about, to fetch five files. (b) is also the shape
`scripts/fetch-quickfix-assets.sh` already has — one pin, refused on mismatch.

**Where the scenario runs.** (a) a new arm inside `scripts/interop.sh`; (b) its own script and
its own job. Chosen **(b)**, `scripts/interop-qfj.sh` and job `interop-qfj`. ADR-0042's
*one script, both directions* argument is *one pin per counterparty*; QFJ has its own pin and
shares nothing with `libquickfix` but the protocol. One script would put CMake, g++ and a JVM
in every run, serialise two independent oracles, and make a C++ build failure hide a Java
verdict. The TLS arms also need a runner where kTLS is `READY`, which the `interop` job does
not assert.

**How this engine's initiator is judged.** (a) as with `libquickfix`: the pure-session driver
judges, the counterparty is a passive acceptor; (b) **the counterparty judges**: this engine
runs its production initiator door (`connect_and_serve`, `connect_and_serve_tls`) with
`desk::Desk`, and a QFJ **acceptor** drives the seven steps and reads the raw bytes. Chosen
**(b)**. Under (a) the TLS arm would need a second TLS client written inside the tool, which
is not the code a deployment runs — the trap the scope names. Under (b) plaintext and TLS are
the same scenario with one variable changed, so a TLS-only red is attributable to TLS, and one
Java judge serves both roles.

## Decision

1. **QuickFIX/J 3.0.2 is fetched, never committed.** `scripts/interop-qfj.sh` downloads
   `quickfixj-core`, `quickfixj-base`, `quickfixj-messages-fix44` (3.0.2), `mina-core` 2.2.9 and
   `slf4j-api` 2.0.18 from `repo1.maven.org` into gitignored `vendor/quickfixj/`, and refuses to
   run on any SHA-256 that differs from the one pinned in the script. The version lives in one
   variable in that script. The run ends with the `libquickfix` script's check: *what this run
   added* that `git status` can see must be nothing.
2. **The JVM lives in CI and `scripts/`, never in a `Cargo.toml`** — ADR-0004 decision 7
   extended to Java. The job installs Temurin 21 with `actions/setup-java@v6`; the desk uses
   `openjdk-21-jdk-headless`. No Maven, no Gradle.
3. **The judge is this repository's own code**: one file, `tools/interop-qfj/Judge.java`, under
   the workspace licence, calling only QFJ's public API, compiled by `javac` against the fetched
   jars. It carries no QFJ source (non-negotiable 9). It records raw strings through its own
   `quickfix.Log`, and judges on them.
4. **Seven steps, the same in both roles, judged by QFJ**: `logon`, `order`, `heartbeat`,
   `testrequest`, `resend`, `gapfill`, `logout` — the `interop-acceptor:` steps of
   `scripts/interop.sh` §4b, whose judge was also the counterparty. This engine runs
   `desk::Desk` behind `serve` / `serve_tls_requiring` (acceptor) and `connect_and_serve` /
   `connect_and_serve_tls` (initiator), both brought up from a settings file.
5. **Four runs, each 7 / 7**: acceptor plaintext, acceptor TLS, initiator plaintext, initiator
   TLS. Beside the seven, each run asserts `shutdown` (the engine returned through
   `Admin::shutdown`) and `clean` (no `35=3` and no `35=j` in either direction); each TLS run
   also asserts `kernel` — `TlsTxSw` and `TlsRxSw` in `/proc/net/tls_stat` rose across the run,
   no `TlsFellBackToUserspace` event was printed, and `TlsRequireKernel=Y` was in force.
6. **TLS on the QFJ side is pinned to what this engine offers**: `EnabledProtocols=TLSv1.3`,
   `CipherSuites=TLS_AES_128_GCM_SHA256`, `KeyStoreType=TrustStoreType=PKCS12`, the QFJ initiator
   with `EndpointIdentificationAlgorithm=HTTPS` against an IP SAN of `127.0.0.1`. Certificates are
   generated per run by `openssl` (a CA and two leaves, P-256) and `keytool`, under `vendor/`.
7. **Mode**: `standard` only. `hft` is not exercised by this gate, as it is not by the
   `libquickfix` one; that stays in `STATUS.md` as not proven by interop.
8. **Criterion 6's command is `scripts/interop-qfj.sh`**, run by the blocking job `interop-qfj`.
   ADR-0097's wording (`scripts/interop.sh`) is not edited; this ADR records the refinement and
   the phase-3 close cites both.
9. **No `NOTICE`.** ADR-0001 decision 5 attaches one when QuickFIX-derived text or data *ships
   inside a fixbolt artifact*. The jars are fetched into `vendor/` at test time and never
   redistributed; the judge is our own code; no published crate depends on anything Java.

## Consequences

**Good**

- The acceptor — the product — is judged by a second family in both transports, and the TLS
  judgement goes through the code a deployment runs, with the kernel's own counter as witness.
- The initiator's production door gets its first counterparty judge; until now only the pure
  session was judged from outside, and `connect_and_serve_tls` had only this engine at the
  other end.
- One judge, two roles, two transports: a disagreement between runs names its variable.
- The pin is five SHA-256 lines; a hostile or silently rebuilt artefact stops the job.

**Bad — and these are real**

- **A JVM joins the supply chain** of CI: the Temurin download and five jars. Estimated, not
  measured: 4–7 minutes of one runner per pull request, most of it compiling `tools/interop`
  with `rustls` and `ring`.
- **QuickFIX/J is QuickFIX's sibling, not a stranger.** Its code is independent of
  `libquickfix`, its reading of FIX descends from the same project. A misreading the two share
  stays green in both. The *second family* ADR-0097 asks for is weaker than it sounds.
- **The initiator judged through the engine is a different lens from ADR-0042 decision 4.** It
  no longer shows that the session layer, driven alone, speaks to QFJ — `libquickfix` still
  shows that. What this engine's initiator *sends on its own initiative* is limited to what
  `desk::Desk` and the session do unprompted: its `ResendRequest` is exercised by the gap, its
  `TestRequest` is not.
- **The TLS arms need a kTLS-ready runner.** A runner image that drops the `tls` module turns
  this job red for an environment reason; the job asserts that first, in its own words, as the
  `tls` job does.
- **Narrow TLS is visible**: a QFJ side left at JSSE defaults would still connect (TLS 1.3 with
  AES-128-GCM is in the default list), but a counterparty that disables that suite cannot. The
  pin in decision 6 says what was tested; it does not widen what is supported.
- `desk::Desk` is now the application for three counterparty roles. A change to it moves three
  gates at once.

## Sources

The URLs in *Research*, each read 2026-09-23.
