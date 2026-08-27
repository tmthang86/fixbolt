# Current state

One screen. A pointer, not a store. Detail lives in the ADRs and the plan files.
**A stale status page is worse than none.**

Last updated: **2026-08-27**.

## Where the work is

| | |
|---|---|
| Branch | **`main`** |
| Milestone | **M0 — decisions and scaffolding.** No engine code |
| Plan in flight | **None.** Next plan to write: the `.def` acceptance-test runner |
| Last closed | Repository initialised; ADR-0001 written (Proposed, not yet Accepted) |

## Proven — the command was run and its output read

- `tmthang86/nanofix` exists on GitHub and is **PUBLIC**, created 2026-08-27, empty before
  this commit.
- `scripts/fetch-quickfix-assets.sh` runs and reports **59 acceptance definitions** in
  `vendor/quickfix/test/definitions/server/fix44/`. Counted on disk after the fetch, and
  independently via the GitHub contents API.
- The `.def` format was decoded from the files and from `Comparator.rb`, and written up in
  [docs/reference/quickfix-acceptance-def-format.md](docs/reference/quickfix-acceptance-def-format.md):
  7 directives, one `<TIME>` placeholder, literal `0x01` separators, 1,319 lines total.
  It carries a trap — the comparator pins **field ordering** of every generated message.
- `git check-ignore -v` confirms `vendor/`, `testdata/recordings/`, `*.docx`, `target/` and
  `.DS_Store` are all blocked.
- The QuickFIX Software License text was read directly: BSD-3 in shape, plus an attribution
  clause and a restriction on using the "QuickFIX" name.
- `cargo 1.95.0` is installed locally.

## Not proven — claimed, researched, or simply not yet run

- **Every performance figure in `docs/reference/prior-art.md` is someone else's claim.**
  Nothing has been benchmarked here.
- **Whether the 59 server definitions cover the acceptor side completely** is unknown;
  `test/definitions/client/` has not been read.
- The 1–2 day runner estimate is a reading-based estimate. No runner code exists.
- No crates exist. `cargo metadata` resolves the workspace; `cargo build` errors with *"manifest is
  virtual, and the workspace has no members"* — expected until the first crate lands.

## Open items

| # | Item | Blocks |
|---|---|---|
| 1 | **Name collision with `matthart1983/nanofix`** — same name, same language, same purpose | Nothing technically; costs more to change the longer it waits |
| 2 | ADR-0001 is **Proposed**, not Accepted | Every downstream decision |
| 3 | Read `test/definitions/client/` — do the 59 server defs cover the acceptor fully? | The acceptance-test runner plan |
| 4 | Latency target has no number yet | The benchmark harness, and what "done" means |
| 5 | Does QuickFIX field ordering follow `spec/FIX44.xml` declaration order in every case? | Whether the serialiser can be generated from the XML alone |
