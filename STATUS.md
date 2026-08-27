# Current state

One screen. A pointer, not a store. Detail lives in the ADRs and the plan files.
**A stale status page is worse than none.**

Last updated: **2026-08-27**.

## Where the work is

| | |
|---|---|
| Branch | **`main`** |
| Milestone | **M0 — decisions and architecture.** No engine code |
| Plan in flight | **None.** Next plan to write: `codec` + `dict` (build order step 1) |
| Last closed | Proposed architecture written: `DESIGN.md`, ADR-0002, ADR-0003, `reference/measured-costs.md`. Repository renamed `nanofix` → `nanofixengine` |

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

## Not proven — claimed, researched, or simply not yet run

- **Every figure in [prior-art.md](docs/reference/prior-art.md) is someone else's claim**,
  including all of fix8's and Artio's. Nothing from those projects was run here.
- The **150 ns gates** in `DESIGN.md` §6 are anchored to one measurement on one macOS
  laptop. macOS gives no thread pinning and schedules across three core types — these rank
  designs against each other, they are **not an SLA**.
- The ring-buffer handoff cost in ADR-0002 is arithmetic, not a measurement. It is the first
  benchmark to write after `codec`.
- `MAX_FIELDS = 64` is a starting number. No real message population has been surveyed.
- No crates exist. `cargo metadata` resolves the workspace; `cargo build` errors with
  *"manifest is virtual, and the workspace has no members"*.
- **All three ADRs are `Proposed`, none `Accepted`.**

## Open items

| # | Item | Blocks |
|---|---|---|
| 1 | **Final name.** `nanofixengine` is a placeholder. Free on both crates.io and GitHub: `machfix`, `veloxfix`, `tachyonfix`, `luxfix`, `fixwire`, `fixbolt`, `sohwire` | Nothing yet — but renaming after a crates.io publish is expensive |
| 2 | ADR-0001, -0002, -0003 are **Proposed** | Everything downstream |
| 3 | Read `test/definitions/client/` — do the 59 server defs cover the acceptor fully? | The `conformance` plan |
| 4 | Does QuickFIX field ordering follow `spec/FIX44.xml` declaration order in every case? | Whether the serialiser can be generated from XML alone |
| 5 | Ring-buffer policy when the library falls behind: block, drop, or disconnect? | ADR-0002, and the `engine` plan |
