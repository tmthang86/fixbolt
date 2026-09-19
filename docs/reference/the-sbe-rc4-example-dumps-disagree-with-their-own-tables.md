# The SBE 1.0 RC4 example dumps disagree with their own interpretation tables

`[found 2026-09-19, step C1 of the phase 2 plan]` The three hex dumps in
`v1-0-RC4/doc/07Examples.md` (FIXTradingCommunity/fix-simple-binary-encoding at
`418a8f6a8b93c65b308638dab2bc6a35dddcd864`, fetched by `scripts/fetch-sbe-assets.sh`) and the
tables beneath them, which say what each byte means, do not describe the same bytes:

| Message | Field | Dump says | Table says |
|---|---|---|---|
| NewOrderSingle | `TransactTime` | 1412627244432000000 | 1381412133135000000 |
| NewOrderSingle | `StopPx` mantissa | `…80` = `i64::MIN`, the int64 null | `…08` |
| ExecutionReport | `TradeDate` | `dd3f` = 16349 = 2014-10-06 | `753e` = 2013-10-11 |
| BusinessMessageReject | header template / schema | 97 / 100 | 100 / 0 |

The dates suggest the dumps were regenerated about a year after the tables were written.
`v1-0-STANDARD/` at the same commit is no better: its prose recommends a 12-byte header and an
8-byte `groupSizeEncoding`, while its dumps use the 8-byte header and carry `schemaId` 91
against a schema saying `id="100"`.

**Rule.** The oracle is the dump's bytes, never the table's prose. `crates/sbe` tests decode
the RC4 dumps and assert what the bytes say; where a table cell disagrees, the test comment
names the disagreement. Anyone transcribing an expected value from the table instead gets a
red test that looks like a decoder bug.

**Also learned in C1, recorded for `sbe-spec-facts.md` (step C8):**

- Schema extension is RC4 chapter 5 (`05SchemaExtensionMechanism.md`), not "§3.6" as
  ADR-0081 decision 4 cites it.
- An unknown template can be skipped by `blockLength` alone only when the message has no
  groups or `varData`; past the root block the length must come from framing (SOFH). ADR-0081
  decision 4 holds fully for flat messages only.

Guarded by: `crates/sbe/src/tests.rs` (the `ORDER_DUMP`, `EXEC_DUMP`, `REJECT_DUMP` cases).
