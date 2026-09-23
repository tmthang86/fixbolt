# ADR-0131 — The QuickFIX/J oracle gets a logger: `slf4j-simple` is the sixth pinned jar

- **Status**: Accepted — 2026-09-23, by the manager under the owner's standing mandate of
  2026-09-18
- **Date**: 2026-09-23
- **Deciders**: Tran Manh Thang (through the manager). Written by the architect (Opus) from the
  senior review of PR #100 and `scripts/interop-qfj.sh` lines 44–69 and 138–149.
- **Supersedes**: [ADR-0130](ADR-0130-a-jvm-enters-ci-as-a-second-oracle-quickfixj-by-pinned-jar-and-our-own-judge.md)
  decision 1, **the jar list only**. Every other part of decision 1 (fetched and never committed,
  refused on any SHA-256 mismatch, one version variable, the *added nothing git can see* check)
  and decisions 2–9 of ADR-0130 stand unchanged.

## Context

ADR-0130 decision 1 pins five jars: `quickfixj-core`, `quickfixj-base`,
`quickfixj-messages-fix44` (3.0.2), `mina-core` 2.2.9, `slf4j-api` 2.0.18. The senior review of
PR #100 found that this set contains **no SLF4J provider**. With no provider, SLF4J 2.x uses its
no-operation logger, so everything QuickFIX/J, MINA and the JSSE layer log through SLF4J
disappears. That includes a failed TLS handshake on the QFJ acceptor side, which happens before
any session exists and so never reaches the judge's `RawLog`. A red TLS run would show a missing
`logon` step with no cause.

## Decision

1. **The pinned set is six jars**: ADR-0130's five, plus
   `org.slf4j:slf4j-simple:2.0.18`. It is pinned like the others. Maven Central's `.sha1` is
   `503354e24cb3a2c61f808a83934b76c2dc4ec4d8`, checked 2026-09-23. The SHA-256
   `8268bd018a5709b07209e0d8ca6221a37584ba1bc12ba985b8335a82c648bdd0` lives in
   `scripts/interop-qfj.sh`, and the script refuses to run on a mismatch.
2. **It prints WARN and above to stdout**, the stream the transcript already captures
   (`-Dorg.slf4j.simpleLogger.logFile=System.out`, `-Dorg.slf4j.simpleLogger.defaultLogLevel=warn`).
   INFO is left out so that a green run's transcript stays readable.

**Why:** a gate whose referee says nothing when it fails proves less than one that gives a
reason. §12 of `CLAUDE.md` asks that every finding be reproduced and classified. A red step
with no cause from the oracle's side can only be guessed at, so observing the oracle is part
of the gate, not an extra.

## Consequences

**Good**

- A handshake, MINA or dictionary failure on the Java side reaches the transcript and the CI
  step summary in the library's own words.

**Bad**

- One more artefact in the supply chain, 15 721 bytes (`content-length` from Maven Central,
  2026-09-23). It is pinned by hash, like the other five.
- WARN lines from a healthy run (for example a close after `Logout`) are now in the transcript.
  A reader has to tell noise from cause. The gate still judges on step lines only, never on
  the absence of WARN.

**Licence**

- `slf4j-simple` is **MIT**: the SLF4J 2.0.18 BOM declares `<name>MIT</name>`
  (<https://repo1.maven.org/maven2/org/slf4j/slf4j-bom/2.0.18/slf4j-bom-2.0.18.pom>), and the
  project's `LICENSE.txt` is the MIT text, *"Copyright (c) 2004-2022 QOS.ch Sarl"*
  (<https://raw.githubusercontent.com/qos-ch/slf4j/master/LICENSE.txt>).
- Like the other five jars, it is fetched into gitignored `vendor/` at test time and never
  redistributed. It adds no `NOTICE` obligation (ADR-0001 decision 5, ADR-0130 decision 9).
- Searched and found nothing: no `<licenses>` block in `slf4j-parent-2.0.18.pom` itself. The
  licence is read from the BOM and the repository.
