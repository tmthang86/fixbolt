# ADR-0097 — Phase 3 makes the engine dependable by a stranger, and FIXP waits on an oracle that runs

- **Status**: **Accepted — 2026-09-23, by the owner, all eight recommendations (Q1–Q8) as
  proposed.** Proposed the same day with
  [plans/2026-09-23-phase-3-scope](../plans/2026-09-23-phase-3-scope.md), which lists the
  questions. The owner's answers, in conversation 2026-09-23:
  Q1 the theme is *dependable by a stranger* (D + E1 + E2 + E3) — **yes**;
  Q2 the published dictionary comes from **FIX Orchestra (Apache-2.0) with QuickFIX's XML kept as
  the oracle**, decided only after step 1's diff spike (decision 4 stands as written: if the
  spike shows a disagreement too large to name, the ADR for step 1 returns to the owner);
  Q3 the first release is **`0.1.0`**; Q4 FIXP is **the oracle spike only, as the last row**,
  target B3 Binary EntryPoint; Q5 **the owner runs `cargo publish`**; Q6 **no** Onload copy-mode
  trial; Q7 a second deployment is **a `1.0` condition, not an exit criterion**; Q8 the published
  crates are **`fixbolt-codec`, `fixbolt-dict`, `fixbolt-session`, `fixbolt-engine`,
  `fixbolt-sbe` and `fixbolt`**; not `fixbolt-conformance`, not `fixbolt-sbe-gen`.
- **Date**: 2026-09-23
- **Deciders**: Tran Manh Thang. Written by the architect (Opus) from `PRD.md` §1–§6,
  ADR-0001, ADR-0045, ADR-0074, ADR-0077, ADR-0078, `STATUS.md` *Start here — boot F*, and
  the research below.
- **Related**: `PRD.md` §2 *Phase 3*, §3, §5; [ADR-0001](ADR-0001-relationship-to-quickfix.md)
  (QuickFIX assets are data and oracle, decision 5: `NOTICE` if any ever ships);
  [ADR-0026](ADR-0026-a-counterparty-registry-in-the-pre-session-stage.md) (the admission hook);
  [ADR-0028](ADR-0028-a-decimal-is-a-copy-value-parsed-on-demand.md) (accepted, not built);
  [ADR-0042](ADR-0042-a-second-implementation-is-the-only-independent-opinion.md);
  [ADR-0045](ADR-0045-parse-is-under-one-percent-of-the-wire-and-simd-is-declined.md);
  [ADR-0074](ADR-0074-kernel-bypass-io-uring-and-the-logon-hop-stay-unmeasured-by-decision.md);
  [ADR-0077](ADR-0077-acceptor-first-stays-and-fastest-is-said-only-beside-a-reproduced-pair.md);
  [ADR-0078](ADR-0078-sbe-enters-as-an-encoding-without-a-session-and-fixp-is-its-own-phase.md)
- **Answers**: `PRD.md` §2 *Phase 3: not scoped* — what phase 3 is, in which order, what it
  is not, and how anyone knows it is finished.

## Context

Phase 1 (a deployable FIX 4.4 engine, both roles, 59 / 59) and phase 2 (the `Encoding` trait,
SBE with no session, FIX 5.0 SP2 / FIXT 1.1) are delivered. `PRD.md` §2 lists phase-3
*candidates* only so that scope creep has to argue with a document: kernel bypass, SIMD,
clustering, HA, replication. `PRD.md` §3 names the two gaps that matter most: **a production
track record of zero**, which "closes by being deployed, not by writing code", and TLS that has
never spoken to an engine this repository did not write.

Four facts decide the shape of phase 3. The first two were found while writing this ADR.

1. **Nobody outside this repository can depend on it today.** `[measured 2026-09-23]` every
   crate is `version = "0.0.0"`, `publish = false`, and `docs/GETTING-STARTED.md` says *"Not on
   crates.io yet"*. Worse, **the published crate could not build even if the flag were
   flipped**: `crates/dict/build.rs` generates its tables from `../../vendor/quickfix/spec/FIX44.xml`
   and fails loudly when it is absent. A crate downloaded from crates.io has no `vendor/`. The
   only ways to make it build are to ship QuickFIX's XML (or tables generated from it) inside
   the package — which triggers ADR-0001 decision 5 and non-negotiable 9's `NOTICE` — or to
   generate from a source that carries its own licence. `[documented 2026-09-23]` the FIX
   Trading Community publishes `OrchestraFIX44.xml` and `OrchestraFIXLatest.xml` under
   Apache-2.0 in `FIXTradingCommunity/orchestrations/FIX Standard/`. Whether its content agrees
   with QuickFIX's `FIX44.xml` on the 912 tags, 12 524 message-tag pairs and 1 708 enum values
   this repository already checks is **not known**; nothing was run.
2. **The Rust acceptor gap is closing without us.** `PRD.md` §1 (as of 2026-08-27) says
   `IronFix` is initiator-only. `[documented 2026-09-23]` `ironfix-engine` 0.4.1, published
   2026-09-18, ships an `Acceptor` — its own docs say no TLS, no dictionary validation, an
   opt-in in-memory store, resend by gap fill. `hotfix` 0.13.0 (2026-09-12) is still
   initiator-only; `easyfix` 0.14.11 (2026-09-08) has an acceptor example. `fixbolt` is **free
   on crates.io** `[measured 2026-09-23, crates.io API: "crate fixbolt does not exist"]`. The
   differentiator in `PRD.md` §1 — *no production-proven pure-Rust acceptor* — is still true,
   but it is a property of the field, not of this repository, and it expires the day someone
   else publishes one with TLS and validation.
3. **FIXP now has a candidate target and a candidate second implementation, but still no
   conformance oracle.** ADR-0078 decision 2 opens FIXP "only when there is a target venue and
   an oracle for it". `[documented 2026-09-23]` B3's Binary EntryPoint is FIXP + SBE, its
   schema (`b3-entrypoint-messages-8.4.2.xml`) is downloadable from B3's developer page, and
   **Artio implements Binary EntryPoint on the acceptor side** (Apache-2.0; its iLink 3 support
   is initiator-only; one Artio instance serves one FIXP protocol). A third-party C# client with
   a conformance suite organised by the B3 spec's sections exists (`pedrosakuma/B3EntryPointClient`,
   MIT, 0 stars). None of these is B3's certification environment, and FIXP 1.1 is still a
   *Draft Standard* (2019); the 1.0 *Technical Standard* (2021) required "two interoperable
   implementations", not a test corpus. So a *second implementation* exists; an *oracle* in the
   sense the 59 `.def` files are one does not.
4. **The positioning is fixed.** ADR-0077 decision 2: *a FIX acceptor on kernel TCP whose
   latency is a published, reproduced number*. An option that leaves kernel TCP has to
   supersede ADR-0077 first; an option that adds no latency number does not conflict with it.

## Research

What sibling engines grew after their core worked, and what users choose engines by:

| Source | What it says | Bearing |
|---|---|---|
| <https://www.onixs.biz/insights/the-complete-guide-to-fix-engine-selection-in-2026> | Selection criteria: latency *"under your actual peak message rate"*, venue dialects, pluggable session storage, operational visibility, vendor support, binary protocols (iLink 3, BOE, ETI), acceptance testing | Adoption is decided by trust and fit, not by a microsecond |
| <https://chronicle.software/tech-hub/technical-information/chronicle-fix/failover> | HA = replicate the message-store queue to a secondary acceptor; `NEVER_TIMES_OUT` (default) *"sacrifices consistency"*, `WAIT_FOREVER_FOR_REPLICA_ACK` sends only after the replica acks | Exactly-once sequence continuity costs a replica round trip on the send path |
| <https://aeron.io/docs/aeron-cluster/cluster-gateway-patterns/> and <https://weareadaptive.com/trading-resources/blog/accelerating-trading-system-development-aeron-hydra-platform-adaptive-part-iii/> | Artio gateways sit in front of Aeron Cluster; on leader failure a message *"may be lost"* and the client may have to disconnect sessions and reconcile order status | HA is a cluster product, not an engine feature |
| <https://ref.onixs.biz/cpp-fix-engine-guide/group__failover.html> | OnixS "failover" = initiator reconnects to a backup server; state restored from files after restart | The common case needs a journal that restores, which this engine has (`FileJournal`) |
| <https://quickfixj.org/docs/architecture/> , <https://pkg.go.dev/github.com/quickfixgo/quickfix/store/mongo> | QuickFIX/J: JMX session admin, `DynamicAcceptorSessionProvider`, `JdbcStore`; QuickFIX/Go: SQL, MongoDB, file, memory stores | Admin and dynamic sessions exist here (ADR-0036, ADR-0026); database stores are a `Journal` impl, not engine work |
| <https://github.com/fix8/fix8> | Fix8: async persister, optional Redis/Memcached/BerkeleyDB backends | Same: store backends are pluggable, not core |
| <https://github.com/FIXTradingCommunity/fixp-specification> | FIXP 1.0 Technical Standard 2021 (promotion needed "two interoperable implementations"); 1.1 Draft Standard 2019; reference implementation Silverflash; no conformance suite named | No public FIXP oracle |
| <https://cmegroupclientsite.atlassian.net/wiki/spaces/EPICSANDBOX/pages/714145834/iLink+Binary+Order+Entry+-+Session+Layer> | CME iLink 3 uses FIXP | Target exists; oracle is CME's certification, not public |
| <https://github.com/artiofix/artio> and its wiki page *FIXP Support* | *"Artio supports two different FIXP Protocols: iLink3 and Binary EntryPoint"* — iLink 3 initiator only, Binary EntryPoint acceptor only; venue-specific, not generic | A runnable second implementation for a B3-shaped initiator |
| <https://www.b3.com.br/en_us/solutions/platforms/puma-trading-system/for-developers-and-vendors/entrypoint/> | B3 publishes the EntryPoint SBE schema | A target venue whose schema is public |
| <https://github.com/pedrosakuma/B3EntryPointClient> | Third-party MIT C# client + conformance suite by spec section (§4.6 sequence/heartbeat, §4.7 retransmit/NotApplied) | Useful as a second opinion; not authoritative |
| <https://github.com/Xilinx-CNS/onload/blob/master/README.md> | Onload runs on non-Solarflare NICs through AF_XDP (generic, in-driver, zero-copy); AF_XDP support is community-supported | Unchanged since ADR-0074 decision 1 |
| <https://github.com/ASherjil/ABTRDA3> | ef_vi CTPIO 1.866 µs median RTT vs DPDK 3.506 µs on Solarflare hardware; also AF_XDP on commodity NICs | Bypass pays on bypass hardware; the positioning excludes it |
| <https://docs.rs/ironfix-engine/latest/ironfix_engine/> | 0.4.1 ships `Acceptor`; *"There is no TLS, and no dictionary validation"* | The acceptor gap is closing (fact 2) |
| <https://crates.io/crates/hotfix> , <https://docs.rs/fefix> | `hotfix` 0.13.0 initiator-only; `fefix` last released 0.7.0 in 2021 | No mature pure-Rust acceptor on crates.io |
| <https://github.com/FIXTradingCommunity/orchestrations> | Apache-2.0; `FIX Standard/OrchestraFIX44.xml`, `OrchestraFIXLatest.xml`, `FIX44Session.xml`, `FIXTSession.xml` | A dictionary source a published crate may ship (fact 1) |
| <https://raw.githubusercontent.com/quickfix/quickfix/master/LICENSE> | Redistribution in source or binary must reproduce the notice; *"This product includes software developed by quickfixengine.org"* | Why shipping QuickFIX-derived tables means `NOTICE` |
| <https://blog.rust-lang.org/2025/09/18/Rust-1.90.0/> | `cargo publish --workspace` stabilised in 1.90; verification builds the set as if published; publish is non-atomic | The packaging gate is one command on the pinned 1.98.0 |
| <https://github.com/obi1kenobi/cargo-semver-checks> | Lints a release for semver breakage from rustdoc JSON | The API-stability gate |

**Searched, found nothing:** a public FIXP conformance corpus comparable to QuickFIX's `.def`
files (none — only the venue certification environments and one third-party C# suite); a venue
that carries SBE inside a FIXT tag=value session (unchanged since ADR-0078); a published,
reproduced latency figure for any Rust FIX acceptor under §9-style conditions; an open-source
FIX engine that ships HA as an engine feature rather than on top of a replicated log product
(Chronicle Queue, Aeron Cluster).

## Options considered

Each option names the non-negotiables (`CLAUDE.md` §2) it pressures and whether it fits
ADR-0077.

### A. A FIXP session over SBE

A second pure session machine beside `Session` (ADR-0078 decision 2: free by D1), SOFH framing,
Negotiate / Establish / Sequence / RetransmitRequest / NotApplied, for one named venue dialect.

- **Fits ADR-0077**: yes — FIXP rides kernel TCP; "FIX 4.4" in the positioning already widened
  in phase 2.
- **Pressures**: 2 (a second pure machine: no clock, no socket, fieldless errors), **3** (the 59
  `.def`s say nothing about FIXP — a FIXP machine needs its own gate, and that gate is the
  question), 1, 4 (both modes), 5 (layouts from generated SBE tables), 7, 10.
- **Blocked on**: ADR-0078's condition. A target venue is now nameable (B3 Binary EntryPoint,
  public schema); a second implementation is runnable (Artio, acceptor side) — **but only for a
  fixbolt *initiator***, which is not the acceptor-first shape; and there is no corpus. For a
  fixbolt *acceptor* the counterparty would be Artio's system-test client or a third-party C#
  suite.
- **Verdict**: **conditional, last, and only its oracle spike is in phase 3.** Building a
  session machine whose only judge is a venue certification nobody here can reach is the trap
  ADR-0078 named.

### B. HA / hot standby with journal replication

Replicate `FileJournal` to a standby acceptor; the standby takes over the counterparty's
reconnect with sequence numbers intact.

- **Fits ADR-0077**: badly. Exactly-once continuity needs the send gated on the replica's ack
  (Chronicle's `WAIT_FOREVER_FOR_REPLICA_ACK`) — a network round trip inside every published
  latency number, or a mode that silently loses the guarantee (Chronicle's default).
- **Pressures**: **4** (an ack wait is a kernel sleep on the `hft` engine thread, or a spinning
  wait in `standard`), 1, 2, 10; and **`PRD.md` §5** lists *clustering, HA, replication* as a
  permanent non-goal — this option needs an ADR that reverses it.
- **Verdict**: **excluded; §5 unchanged.** The research says HA is a cluster product built on a
  replicated log (Chronicle Queue, Aeron Cluster), and even Aeron's own docs say a message can
  be lost at failover. What this engine owes an HA deployment already exists: an ordered
  `FileJournal`, `tools/jrnl`, `serve_with_recovery`, sequence admin. A `GUIDE.md` paragraph on
  warm standby from a copied journal is a documentation item, not a phase.

### C. Kernel bypass / AF_XDP

- **Fits ADR-0077**: no — *on kernel TCP* is the positioning; this option supersedes it.
- **Pressures**: 6 (a real feature flag around a second `Transport`), 8 (`ef_vi` FFI is
  `unsafe`), 10 (needs a Solarflare/X2-class NIC this project does not own — ADR-0074), D11
  (bypass is plaintext); ADR-0045 decision 3 says SIMD must be re-read before any bypass plan.
- **Verdict**: **excluded; §5 unchanged; ADR-0074 stands.** The only bypass step this project
  may take without hardware — `onload ./w2w` in AF_XDP copy mode as a smoke test, never a
  number (ADR-0074 decision 1) — is not scheduled either: it serves no user and proves no gap
  in §3. `io_uring` keeps ADR-0074 decision 2's trigger.

### D. Release and adoption

Make the engine something a stranger can `cargo add`, read, trust and upgrade.

- **Fits ADR-0077**: yes — it is the positioning's own promise ("published, reproduced") applied
  to the code, not only to the numbers.
- **Pressures**: **9** (the dictionary problem in fact 1: a published `fixbolt-dict` either ships
  an Apache-2.0 source or ships QuickFIX-derived data with `NOTICE`), **6** (every published
  feature combination must build from crates.io, where `--no-default-features` is the user's
  choice, not CI's), 7, and 10 (docs.rs and README may quote only figures that name benchmark,
  machine and §9 settings). No hot path is touched.
- **Verdict**: **recommended, first.** It is the only option that moves `PRD.md` §3's largest
  gap — track record closes by deployment, and nobody deploys a crate they cannot download.

### E. What the research surfaced

- **E1. The security floor a stranger's deployment needs.** The admission hook exists
  (`Registry::admit` sees `553` / `554` / `96`, ADR-0026, `GUIDE.md` §*refusal*), and `PRD.md` §3's
  row saying *"nothing beyond identity"* is stale on that point. What is missing: `[grep, not
  run, 2026-09-23]` no redaction of `554=Password` or `96=RawData` was found in `crates/engine/src`
  or `crates/library/src`, so the D14 message log may write a counterparty's password to disk
  in clear. **Recommended**, early — a published engine that logs passwords is a CVE with our
  name on it. Pressures 1 (the log path), 2 (nothing enters `session`), 7.
- **E2. `Decimal` (ADR-0028, accepted 2026-09-01, not built).** Every price the application
  touches is bytes today. **Recommended** — a stranger's first order handler needs it. Pressures
  1, 7; `codec` keeps zero dependencies; `no_std`-clean.
- **E3. A second engine family in the interop gate, plaintext and TLS.** Interop today is
  `libquickfix` only (7 / 7 both roles), and `PRD.md` §3 gap 2 is that TLS has never spoken to
  another engine. QuickFIX/J is a different implementation (Java, MINA, its own SSL stack) of the
  same session rules. **Recommended** — ADR-0042: a second implementation is the only
  independent opinion, and a third is cheaper than the first production incident. Pressures 9
  (the jar is fetched into `vendor/`, never committed), 3 (it is not a substitute for 59 / 59).
- **E4. Sharded ordered shutdown** (`STATUS.md` item 32 residue, ADR-0088 decision 5). Real, but
  a design of its own; **stays an open item**, not phase scope.
- **E5. Database message stores** (QuickFIX/J `JdbcStore`, QuickFIX/Go SQL/Mongo). **Excluded**:
  `Journal` is a trait (ADR-0008); a database impl is a user's crate, and an async client on the
  engine thread breaks non-negotiable 4.

## Decision

1. **Phase 3's theme is *dependable by a stranger*: D + E1 + E2 + E3.** The engine becomes a
   published crate that builds from crates.io with no `vendor/`, does not write credentials to
   disk, hands the application prices as `Decimal`, and is interop-green against a second
   engine family in plaintext and TLS.
2. **Order**: (i) the dictionary source ADR and its build (fact 1 blocks everything
   published); (ii) E1, E2, E3 in parallel — disjoint crates (`engine` log path, `codec`,
   `tools/interop` + `scripts/`); (iii) packaging and the API gate; (iv) documentation for a
   stranger and the first publish; (v) the post-publish check; (vi) *conditional*: the FIXP
   oracle spike.
3. **The first published version is `0.1.0`, not `1.0`.** A `1.0` is a promise about an API no
   stranger has used yet; `cargo-semver-checks` runs from the first release so that `0.x` breaks
   are at least *named*. The condition for `1.0` is written into `CHANGELOG.md` at `0.1.0`: one
   deployment this repository did not write, reported publicly, and one minor release with no
   semver-check exemption. It is not a phase-3 exit criterion, because no command proves it.
4. **The dictionary a published crate builds from is decided by its own ADR (step 1)**, between
   (a) Apache-2.0 FIX Orchestra files, with QuickFIX's XML kept as the oracle the generated
   tables must agree with, (b) QuickFIX-derived tables shipped with a `NOTICE`, and (c) the user
   supplying the XML. This ADR recommends (a) and does not decide it: the agreement between the
   two sources is unmeasured, and (a) is only viable if the disagreement is small and nameable.
5. **FIXP (option A) is opened only as a spike**, the last row of phase 3: prove, as a command in
   CI, that a second implementation (Artio's Binary EntryPoint acceptor) runs headless against a
   trivial client, and write the FIXP ADR from what it showed — including what replaces
   non-negotiable 3 for a session layer with no corpus. The FIXP session build is **not** phase
   3; it is its own plan after that ADR is accepted. If the owner names no target venue, the
   spike does not run and phase 3 closes without it.
6. **Excluded from phase 3, by name**: HA, replication, clustering, hot standby (§5); kernel
   bypass, Onload, `ef_vi`, AF_XDP, DPDK (§5, ADR-0074, ADR-0077); `io_uring` / `recvmmsg`
   (ADR-0074 decision 2); SIMD SOH scan and checksum (ADR-0045); FAST, FIXML, SBE inside FIXT
   (ADR-0078); database stores; metrics exporters, dashboards and web UIs (§5); bindings for
   other languages (§5); a FIXP session machine (decision 5); a `1.0` release (decision 3);
   sharded ordered shutdown (E4, stays in `STATUS.md`). **`PRD.md` §5 is unchanged**: nothing
   moves across it.
7. **Exit criteria**, each a command that passes or fails, all on the closing commit, named in
   CI by run id (`CLAUDE.md` §9):

   | # | Criterion | Command |
   |---|---|---|
   | 1 | The dictionary builds with no `vendor/` | on a checkout with `vendor/` absent: `cargo build -p fixbolt-dict` and `cargo build -p fixbolt --no-default-features` exit 0 |
   | 2 | The published tables still agree with the oracle | with `vendor/` fetched: the dictionary agreement test (named in step 1's plan) asserts 912 / 912 tags, 12 524 / 12 524 message-tag pairs, 1 708 / 1 708 enums, or names each divergence by content; and `cargo test -p fixbolt-session --test score` still 59 / 59 |
   | 3 | The workspace packages as published | `cargo publish --workspace --dry-run` (published crates only) exits 0 in a CI job with no `vendor/` |
   | 4 | No credential reaches disk | a test in `crates/engine/tests/` writes a Logon carrying `554=` and `96=` through the message log and `FileJournal`, then greps both files for the secret and fails on a hit; proven red by reversal |
   | 5 | `Decimal` per ADR-0028 | `cargo test -p fixbolt-codec decimal` green; `benches/alloc.rs` decimal case reads 0, proven by injection |
   | 6 | A second engine family agrees | `scripts/interop.sh` against QuickFIX/J, both roles, **7 / 7**, plaintext **and** TLS, as a blocking CI job |
   | 7 | A stranger can depend on it | after `0.1.0` is on crates.io: a script creates a crate outside the tree, `cargo add fixbolt@0.1.0`, pastes `docs/GETTING-STARTED.md`'s code verbatim, builds it and drives one Logon/Logout through it; exits 0 |
   | 8 | The API is watched | `cargo semver-checks --baseline-version 0.1.0` runs, blocking, in CI on every pull request after the publish |
   | — | Phase 1 and 2 gates hold | 59 / 59 in process and through a socket; 179 / 180 FIXT with the one pinned divergence; `libquickfix` interop 7 / 7 both roles; allocation benches 0 |

## Consequences

**Good**

- The largest gap in `PRD.md` §3 — zero track record — gets the only thing that can move it: a
  crate a stranger can download. No other option touches it.
- Fact 1 is found now, on a design page, not on the day of `cargo publish`.
- The phase has a test oracle for every item: QuickFIX's XML for the dictionary, QuickFIX/J for
  interop, a grep for the credential, `cargo` itself for packaging. Nothing in it waits for an
  oracle that does not exist — which is exactly why FIXP is a spike and not a build.
- No hot-path figure moves, so no §9 boot is needed to close phase 3 (E2 adds a codec path,
  measured on the normal bench gate, not the wire).

**Bad — and accepted**

- **Publishing is irreversible.** crates.io yanks; it never deletes. The name `fixbolt`, every
  published API mistake and every `0.1.x` bug stay downloadable. A premature publish costs more
  than a late one — which is why criterion 1–6 come before it, and why decision 3 says `0.1`.
- **A semver gate slows design change.** Every public-API edit after `0.1.0` is a named break or a
  compatibility shim. The project has changed its public API often (ADR-0048, ADR-0054, ADR-0088
  all added required methods); that pace ends or becomes a version bump each time.
- **The dictionary may stop being QuickFIX's.** If option (a) of decision 4 wins, the tables
  come from FIX Orchestra, while the 59 `.def` files were written against QuickFIX's XML. Every
  disagreement is a place where the gate and the product read different dictionaries. `PRD.md`
  §3 already names 14 type-name differences with QuickFIX; the Orchestra diff could be larger,
  and it is unmeasured.
- **A stranger's issues arrive.** Publishing invites bug reports, feature requests and support
  questions on a one-person project. Nothing here budgets for that.
- **No latency work for a whole phase.** The headline stays where it is; item 101-style bench
  work continues as open items, not phase scope. A reader comparing release notes sees no
  faster number.
- **FIXP stays unbuilt for another phase.** ADR-0078's cost continues: a user who wants B3 or CME
  order entry gets an SBE codec and has to bring the session. Meanwhile Artio already serves
  both.
- **HA stays the user's problem.** A venue or broker evaluating the engine (`PRD.md` §1 users
  row 2) will ask for hot standby; the answer remains "a copied journal and a reconnect", which
  loses the in-flight window Chronicle's consistent mode closes.
- **CI gets a JVM.** QuickFIX/J interop adds a Java toolchain, a fetched jar and minutes to
  every run; its supply chain becomes this repository's.
- **"Second deployment" is not a criterion.** Phase 3 can close with the track-record row still
  reading *zero*. Decision 3 makes that honest rather than hidden.

## Sources

The table under *Research*; all read 2026-09-23. `[measured 2026-09-23]` crates.io API for
`fixbolt` (absent), `hotfix` 0.13.0, `ironfix-engine` 0.4.1, `fefix` 0.7.0, `easyfix` 0.14.11,
`quickfix` 0.2.1; GitHub contents API for `FIXTradingCommunity/orchestrations/FIX Standard`.
In-repository: `crates/dict/build.rs` lines 1–45 (the `vendor/` dependency); `crates/*/Cargo.toml`
(`0.0.0`, `publish = false`); `crates/engine/src/presession.rs:228-244` (the `admit` hook);
`rust-toolchain.toml` (1.98.0); `Cargo.toml` `rust-version = "1.85"`.
