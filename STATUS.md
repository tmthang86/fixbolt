# Current state

One screen. A pointer, not a store. Detail lives in the ADRs and the plan files.
**A stale status page is worse than none.**

Last updated: **2026-08-27**.

## Where the work is

| | |
|---|---|
| Branch | **`main`** |
| Milestone | **M0 — decisions and architecture.** No engine code |
| Scope | **[PRD.md](docs/PRD.md)** — phase 1 = FIX 4.4 tag=value both sides; phase 2 = SBE / FAST / FIXML + FIX 5.0. **Phase-1 gap still without a plan: TLS** |
| Plan in flight | **[2026-08-27-codec-dict.md](docs/plans/2026-08-27-codec-dict.md)** — reviewed (eng + Codex), 16 decisions folded in, chờ duyệt |
| Plan queued | **[2026-08-27-repeating-groups.md](docs/plans/2026-08-27-repeating-groups.md)** — chờ duyệt. Bắt đầu sau khi codec-dict xong bước 1. Đóng luôn open item 8 |
| Decision in flight | **[ADR-0004](docs/decisions/ADR-0004-bidirectional-engine.md)** — bidirectional engine (acceptor + initiator, one session core). **Proposed**, awaiting acceptance. `DESIGN.md` and `README.md` are untouched until it is accepted |
| Last closed | Design reviewed against the HFT latency budget and revised: positioning fixed to "fastest acceptor on kernel TCP", ADR-0002 default reversed (inline dispatch, ring optional), D8 busy-poll, D9 template encoder, D10 send backpressure, §8 latency budget, §9 OS checklist, wire-to-wire gate added |

## Proven — the command was run and its output read

- **The 512-entry inline field array in `matthart1983/nanofix` costs 4–6× the parse time.**
  Changing `MAX_FIELDS` 512 → 64 took the heartbeat from 565.0 ns to **95.4 ns** and
  `NewOrderSingle` from 605.1 ns to **138.8 ns**, on an Apple M5 on 2026-08-27. Method and
  caveats: [docs/reference/measured-costs.md](docs/reference/measured-costs.md).
- `cargo test --no-default-features --lib` on that project **fails to link** — its `aeron`
  feature does not gate `mod aeron_c;`. Its "238 tests" cannot be run as shipped.
- `scripts/fetch-quickfix-assets.sh` runs and reports **59 acceptance definitions** on disk.
- The `.def` format was decoded from the files and `Comparator.rb`: 7 directives, one
  `<TIME>` placeholder, literal `0x01` separators. It pins **field ordering** positionally.
- **The ordering rule is tag-ascending within header and body, not XML order** — checked by
  script over all 247 expected lines, zero violations. Open item 4 closed by data.
- **`test/definitions/client/` holds exactly one file** — `Normal.def`, 271 bytes, 6 lines,
  FIX 4.2, logon then logout. **470 of the 471 `.def` files test the acceptor.** It also uses
  two directives absent from the seven recorded for the server format: a bare `eCONNECT`, and
  `R` for a reply the harness sends. This answers ADR-0001 open question 1: there is no
  initiator conformance suite to run, and that is the central cost priced in ADR-0004.
- The QuickFIX Software License was read directly: BSD-3 in shape, plus attribution and a
  naming restriction.
- `git check-ignore -v` blocks `vendor/`, `testdata/recordings/`, `*.docx`, `target/`,
  `.DS_Store`.
- Repository is **private** as of 2026-08-27 (it was public at creation). `cargo 1.95.0`.
- **ADR-0001, -0002, -0003 accepted 2026-08-27** by the owner, after the latency-budget review.

## Not proven — claimed, researched, or simply not yet run

- **Every figure in [prior-art.md](docs/reference/prior-art.md) is someone else's claim**,
  including all of fix8's and Artio's. Nothing from those projects was run here.
- The **150 ns gates** in `DESIGN.md` §6 are anchored to one measurement on one macOS
  laptop. macOS gives no thread pinning and schedules across three core types — these rank
  designs against each other, they are **not an SLA**.
- **Every figure in `DESIGN.md` §8 (latency budget) is from the literature**, not measured.
  The `tools/w2w` harness replaces that table; nothing in it is evidence until then.
- The ring-buffer hop (200–500 ns) and the busy-poll saving (2–5 µs) are literature figures.
  `benches/dispatch.rs` and `tools/w2w` are what turn them into numbers.
- `MAX_FIELDS = 64` is a starting number. No real message population has been surveyed.
- No crates exist. `cargo metadata` resolves the workspace; `cargo build` errors with
  *"manifest is virtual, and the workspace has no members"*.
- The ADRs are accepted on the strength of the reasoning in them, **not on measurement** — see the §8 caveat above.

## Open items

| # | Item | Blocks |
|---|---|---|
| 1 | **Final name.** `nanofixengine` is a placeholder. Free on both crates.io and GitHub: `machfix`, `veloxfix`, `tachyonfix`, `luxfix`, `fixwire`, `fixbolt`, `sohwire` | Nothing yet — but renaming after a crates.io publish is expensive |
| 5 | Ring-buffer policy when the library falls behind: block, drop, or disconnect? | ADR-0002, and the `engine` plan |
| 6 | A Linux box for `tools/w2w`. The design's own §9 says a latency number from a macOS laptop is not a number | Every gate in §6 that matters |
| 7 | **`scripts/fetch-quickfix-assets.sh` tracks mutable `master`.** Every acceptance number in the codec plan (539 lines, 247 with `9=`, 244 with `10=`, 8 tag-set patterns for `35=3`) can change silently upstream. Pin a commit and verify it | Reproducibility of every step-1 gate |
| 8 | **`dict::required()` does not recurse through `<component>`.** `FIX44.xml` uses `<component>` in 632 places; `NewOrderSingle` references `Parties`, `PreAllocGrp`, `TrdgSesGrp`, so the generated table is missing fields | The `session` plan |
| 9 | **The encoder has no DATA invariant.** Writing a dynamic DATA field must regenerate its length field, place it immediately before, and count bytes including embedded `0x01`. Only the read path is specified | Any counterparty that sends `RawData`/`XmlData` |
