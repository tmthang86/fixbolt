# Current state

One screen. A pointer, not a store. Detail lives in the ADRs and the plan files.
**A stale status page is worse than none.**

Last updated: **2026-08-27**.

## Where the work is

| | |
|---|---|
| Branch | **`main`** |
| Milestone | **M0 — decisions and architecture.** No engine code |
| Plan in flight | **None.** Engineering rules (`CLAUDE.md`) written against the accepted design. Next plan to write: `codec` + `dict` (build order step 1) |
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
| 3 | Read `test/definitions/client/` — do the 59 server defs cover the acceptor fully? | The `conformance` plan |
| 4 | Does QuickFIX field ordering follow `spec/FIX44.xml` declaration order in every case? | Whether the serialiser can be generated from XML alone |
| 5 | Ring-buffer policy when the library falls behind: block, drop, or disconnect? | ADR-0002, and the `engine` plan |
| 6 | A Linux box for `tools/w2w`. The design's own §9 says a latency number from a macOS laptop is not a number | Every gate in §6 that matters |
