# `conformance` — internals

Dev-layer crate in [DESIGN.md §3](../DESIGN.md#3-crates): the `.def` acceptance runner for
both roles, the corpus loader, and the echo application the corpus assumes. Depends only on
`codec` and `dict`, and is built **before** `session` — the gate exists before the thing it
gates (CLAUDE.md §10).

## Files, and what each keeps

| File | Keeps |
|---|---|
| `lib.rs` | Crate root; states the before-`session` build order and the no-panic rule this gate holds itself to |
| `script.rs` | Turning a QuickFIX `.def` file into a runnable script: `<TIME>` substitution, `9=`/`10=` computed at send time, not read off the file; also `Corpus`/`fixt_corpora` — the three FIXT `.def` corpora (`fix50`, `fix50sp1`, `fix50sp2`) told apart only by `DefaultApplVerID`, per [ADR-0080](../decisions/ADR-0080-the-dictionary-rides-the-encoding-and-a-fixt-session-is-one-table-built-from-two-xml-files.md) decision 2 |
| `compare.rs` | Comparing what the engine sent against what the `.def` expects, by QuickFIX's own `Comparator.rb` rules — field count, positional order, then value |
| `runner.rs` | Driving a session through a script and scoring it; one `SessionUnderTest` instance sees every connection in a file, not one per connection |
| `echo.rs` | The application the corpus assumes: echoes application messages back, rebuilt through `Template` rather than copied, so re-ordered input still gets ordered output; generic over `Encoding`, same bound shape as `Session` |
| `mirror.rs` | What each mirrored (initiator) `.def` file would need this end to *say*, classified file by file — the count behind [ADR-0076](../decisions/ADR-0076-the-mirrored-ceiling-is-a-classified-count-not-an-estimate.md) |

## Read in this order

1. `lib.rs` — why this crate exists before `session` does
2. `script.rs` — turning a file into something runnable
3. `compare.rs` — the pass/fail rule applied to the result
4. `runner.rs` — the driver that ties `script.rs` and `compare.rs` together
5. `echo.rs` — the one application both roles' corpora assume
6. `mirror.rs` — read last; it is about the initiator corpus, not the acceptor one the rest
   of this crate is built around

## Tests that guard it

- `tests/fix44.rs` — the 59-file acceptor corpus run end to end
- `tests/script.rs`, `tests/compare.rs` — `script.rs`, `compare.rs` in isolation
- `tests/echo.rs` — the echo application, including the re-ordering case
- `tests/mirror.rs`, `tests/mirror_classification.rs` — the mirrored corpus and its
  classification
- `tests/fixt_corpus.rs` — the `fix50`/`fix50sp1`/`fix50sp2` FIXT corpora
