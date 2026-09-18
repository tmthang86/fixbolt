# ADR-0080 — The dictionary rides the encoding, and a FIXT session is one table built from two XML files

- **Status**: Proposed — 2026-09-19
- **Date**: 2026-09-19
- **Deciders**: Tran Manh Thang
- **Related**: [ADR-0079](ADR-0079-one-view-per-encoding-and-one-trait-over-them.md)
  (the `Encoding` trait); [ADR-0078](ADR-0078-sbe-enters-as-an-encoding-without-a-session-and-fixp-is-its-own-phase.md);
  [ADR-0001](ADR-0001-relationship-to-quickfix.md) (the XML is data); `DESIGN.md` D1, D3;
  `CLAUDE.md` §2 items 3, 5, 6; the plan
  [phase-2-fixt-and-sbe](../plans/2026-09-19-phase-2-fixt-and-sbe.md)
- **Answers**: three questions ADR-0079 leaves open once the session is generic over an
  encoding — *where does the dictionary go*, *what is a FIXT 1.1 dictionary*, and *what does
  the session do with `1137` and `1128`*.

## Context

Today `Session` names `Fix44` seventeen times: `parse_into::<Fix44, N>`, `Fix44::is_msg_type`,
`Fix44::required_header`, `Fix44::allows`, `Fix44::enum_allows`, `Fix44::group_delimiter` and
the rest of the validation pass (`crates/session/src/lib.rs:2985`, `:3150`, `:3735-3826`,
read 2026-09-19). Those are inherent functions on a generated struct, not a trait, so the
session cannot be generic over them. ADR-0079 makes the session generic over `E: Encoding`
and says nothing about the dictionary; `STATUS.md` line 2989 says *"a dictionary chosen at
`Logon` by the registry"*, which under static dispatch can only mean *chosen among the
dictionaries the engine was compiled with*.

FIXT 1.1 is the reason the question is not academic. `[measured 2026-09-19]` in `vendor/`:
`FIXT11.xml` carries the header (with `ApplVerID(1128)`, `ApplExtID(1156)`,
`CstmApplVerID(1129)`), the trailer and eight admin messages; `FIX50SP2.xml` carries
`<header />` and `<trailer />` **empty**, 156 messages and every application field. Neither
file alone validates a FIXT session. QuickFIX configures the pair as `TransportDataDictionary`
+ `AppDataDictionary` and picks the application dictionary per message from `ApplVerID(1128)`,
falling back to the `DefaultApplVerID(1137)` the Logon carried (`Session.cpp`, `next()` and
`newMessage()`, read 2026-09-19); Artio's `CodecGenerationTool` takes *"both the transport and
data files"* and generates one codec (Artio wiki *Codecs*). Both engines end up with one merged
table per session; they differ only in when the merge happens.

The oracle: `[measured 2026-09-19]` the three FIXT directories under
`vendor/quickfix/test/definitions/server/` (`fix50`, `fix50sp1`, `fix50sp2`, 60 files each)
differ only in the CompID (`TW50`, `TW50SP1`, `TW50SP2`) and the value of `1137` (`7`, `8`,
`9`). Against `fix44` the only new file is `1d_InvalidLogonNoDefaultApplVerID.def`: a Logon
without `1137` is answered by a **disconnect**, nothing sent. Every `E` Logon in the corpus
echoes `1137=<our value>`. **No `.def` carries `1128`**, so per-message versioning has no
oracle; and none carries a `1137` value other than the one the directory is named after, so a
mismatch has none either.

## Decision

1. **The dictionary is an associated type of the encoding, not a second type parameter.**
   `Encoding::Dict: Tables`, where `Tables` is a trait in `dict` over exactly the functions
   the session's validation pass calls today (`is_header`, `is_defined_tag`, `field_type`,
   `allows`, `enum_allows`, `required`, `required_header`, `is_msg_type`, `is_admin`, the
   three `group_*`). `Fix44` implements it by delegating to its generated functions, so the
   generated code does not change. `Session<E, R, N, APP>` reads
   `<E::Dict as Tables>::…`; `Fix44TagValue = TagValue<Fix44, 64>` is the alias the 59
   definitions run against, and `Fixt11Fix50Sp2 = TagValue<Fixt11Fix50Sp2Tables, 64>` the
   one the 180 run against. Static dispatch, as ADR-0079 decision 2 requires; the registry
   picks among compiled encodings, never a dictionary at run time.
2. **A FIXT session's dictionary is one generated table built from two XML files at build
   time.** `dict/build.rs` reads `FIXT11.xml` for header, trailer and the eight admin
   messages, and `FIX50SP2.xml` for fields, components, groups and the 156 application
   messages, and emits `fixt11_fix50sp2.rs` behind the cargo feature `fix50sp2`, which gates
   the `include!` (`CLAUDE.md` §2 item 6: the feature gates the module, and `build.rs` parses
   the second file only when the feature is on). A field defined in both files must agree
   on number, name and type, or the build fails: a silent "first wins" is how a wrong enum
   table ships. `FIX50` and `FIX50SP1` are not generated in phase 2 — their corpora are the
   SP2 corpus with a different `1137`, and the conformance runner covers them by parameter,
   not by table.
3. **The session's FIXT rules, and what has no oracle:**
   - When the configured `BeginString` is `FIXT.1.1`, an inbound Logon without `1137` is
     dropped with a new fieldless `DropReason::LogonWithoutDefaultApplVerID`, nothing sent
     (`1d_InvalidLogonNoDefaultApplVerID.def`). Our Logon carries `1137=<configured>`. A
     `FIX.4.x` session neither requires nor emits `1137`.
   - The counterparty's `1137` is stored. A value other than ours is **accepted** and the
     session validates with its one compiled table: SP2's tables are a superset of SP0/SP1's,
     and the corpus never exercises a mismatch, so a refusal here would be a rule with no
     test. Recorded in `SESSION-BEHAVIOUR.md` as *not covered by the corpus*.
   - `1128` on an application message is parsed and **ignored for validation** when it names
     any FIX 5.0 family value (`7`, `8`, `9`); a value naming FIX 4.x or earlier (`0`–`6`),
     or an unknown value, is a session `Reject 373=5` (*Value is incorrect*) with
     `371=1128`. The header fields `1128`, `1156`, `1129` are **not permitted on session
     messages** (FIXT 1.1 session protocol, OnixS dictionary, read 2026-09-19); phase 2
     does not enforce that, because no `.def` does and QuickFIX does not either — listed as
     an open item, not built.
4. **`DefaultApplVerID` is a settings key** (`[DEFAULT]` or `[SESSION]`), required when
   `BeginString=FIXT.1.1` and refused otherwise, following `settings.rs`'s rule that an
   unrecognised or misplaced key is an error carrying a line number (ADR-0040). The values
   are the `ApplVerID` enum strings (`7`, `8`, `9`), spelled as QuickFIX spells them.

## Consequences

**Good**

- The session's generalisation is proven twice by corpora that already exist: 59 / 59 on
  `Fix44TagValue` shows nothing moved, 180 / 180 on `Fixt11Fix50Sp2` shows the second table
  works through the same code.
- One generated table per session type keeps D3's rule — field order and validation from
  generated tables, never a call site — and keeps the per-message dictionary lookup QuickFIX
  pays (`getApplicationDataDictionary` per inbound message) off the hot path.
- `Tables` as a trait is the seam SBE's "validation is the schema's" (ADR-0079 decision 4)
  plugs into: an `SbeTables` impl whose `allows` and `enum_allows` are structural.

**Bad — and accepted**

- **`1128` is read and mostly ignored.** A venue that sends SP1 messages on an SP2 session
  gets SP2 validation. The alternative — three tables and a per-message switch — is a
  branch per message for a case the corpus cannot test.
- **Two hand-decided behaviours with no oracle** (the mismatched `1137`, the out-of-family
  `1128`). They are the smallest defensible rules, and each is named in
  `SESSION-BEHAVIOUR.md` as decided here, so the day a venue disagrees the text says where
  to look.
- **A second table doubles `dict`'s build time and binary** when `fix50sp2` is on; measured
  when it lands, and the feature is off by default so a FIX 4.4 user pays nothing.
- **`FIX50`/`FIX50SP1` have no table**, so an engine that must *emit* SP0-only ordering has
  nothing to configure. Their corpora pass because they differ from SP2's only in `1137`.

## Alternatives considered

| Alternative | Why rejected |
|---|---|
| `Session<E, D, R, N, APP>` with the dictionary as its own parameter | Every front door and alias gains a second parameter for a choice that is never made independently of the encoding; an SBE `View` over a FIX 4.4 dictionary is not a thing |
| Merge at run time, QuickFIX-style, two tables and a lookup per message | A lookup per message on the hot path, for a corpus that never exercises the choice |
| Refuse a `1137` that differs from ours (Logout with text) | A rule no `.def`, no spec sentence and no engine surveyed enforces; it would refuse counterparties the SP2 tables validate fine |
| Enforce "no `1128` on session messages" now | Spec-correct, but no oracle and QuickFIX accepts it; an item, not a phase-2 line |

## Sources

- `vendor/quickfix/spec/FIXT11.xml` (header lines 1–37, fields 1128/1129/1130/1137/1156 at
  lines 215–231), `vendor/quickfix/spec/FIX50SP2.xml` (`<header />` line 2, `<trailer />`
  line 4674); `vendor/quickfix/test/definitions/server/{fix50,fix50sp1,fix50sp2}` — `diff -r`
  on 2026-09-19.
- QuickFIX C++ `src/C++/Session.cpp` — `nextLogon()` reads `DefaultApplVerID` with
  `FIELD_GET_REF` on a FIXT session; `next()` chooses the application dictionary from the
  message's `ApplVerID` falling back to the target's default; `generateLogon()` sets `1137`
  (<https://github.com/quickfix/quickfix/blob/master/src/C++/Session.cpp>, read 2026-09-19).
- QuickFIX/J configuration: `DefaultApplVerID` *"required only for FIXT 1.1"*,
  `TransportDataDictionary` / `AppDataDictionary` (search 2026-09-19).
- Artio wiki *Codecs*: *"XML file definitions that are split into data and transport files
  for FIX 5.0 / FIXT transports … provide multiple file arguments to the
  CodecGenerationTool"* (<https://github.com/artiofix/artio/wiki/Codecs>, read 2026-09-19).
- OnixS FIXT 1.1 dictionary: `DefaultApplVerID(1137)` required on Logon; *"the use of the
  Explicit Application Version fields is not permitted on FIX Session Level Messages"*; the
  precedence *explicit > message-type default > session default*
  (<https://www.onixs.biz/fix-dictionary/fixt1.1/section_session_protocol.html>,
  <https://www.onixs.biz/fix-dictionary/fixt1.1/msgType_A_65.html>, read 2026-09-19).
