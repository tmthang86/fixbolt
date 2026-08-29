# Changelog

Notable changes to the published crates. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versioning will follow
[Semantic Versioning](https://semver.org/spec/v2.0.0.html) from the first release.

**Scope, stated so this file does not drift into being a second `STATUS.md`:** it records
changes to the **public API and observable behaviour of released crates**, and nothing else.
Decisions live in [docs/decisions/](docs/decisions/); where the work stands lives in
[STATUS.md](STATUS.md); what is about to be built lives in [docs/plans/](docs/plans/). A thing
that has not shipped does not belong here — `CLAUDE.md` §4: one rule, one place.

## [Unreleased]

**Nothing has been released.** Four crates now exist and none is published; the entries
below describe what a first release would contain.

### Added

- **`nanofix-session`** — the FIX session state machine. Pure: no socket, no clock, no
  allocation, no `format!` on any path. Depends on `codec` and `dict`.
  - `Session<R: Role, N>` with `connect` / `disconnect` / `received` / `tick`, each taking an
    `emit` closure and returning `Link`. `Role` is a sealed marker — `Acceptor` and
    `Initiator` — so the two ends differ at compile time rather than on a branch per message.
  - `Config::acceptor(begin_string, sender_comp_id, target_comp_id)`. CompIDs are held inline;
    one too long for its buffer **fails closed** and matches nothing, rather than being
    compared on its first 32 bytes.
  - `clock::parse_utc` and `clock::MILLIS_YEAR_ZERO_TO_EPOCH` — `Tick` counts milliseconds
    from **0000-01-01**, not from 1970, so every year `SendingTime` can name is a non-negative
    `u64` and the skew cannot wrap. See `DESIGN.md` D13.
  - `text::SessionText` — the 17 expected `58=` values and their `373=` codes, rendered with
    no `format!` and no allocation. **Moved here from `nanofix-conformance`**, where it lived
    only because the session did not exist yet.
  - Answers a Logon by echoing `98=` and `108=`, answers a Logout, and tracks sequence numbers
    in both directions. A message with `43=Y` and a sequence number already seen is dropped in
    silence; one without it ends the session with a Logout saying so.
  - `Reject (35=3)` with all twelve `SessionRejectReason` codes, driven by `nanofix-dict`'s
    validation tables. Routing tags are reversed on the way back — `115` in becomes `128` out and
    the other way round. A CompID or SendingTime fault answers with a Reject **and** a Logout;
    the other ten leave the session running.
  - `Heartbeat (35=0)` and `TestRequest (35=1)`, on the session's own clock. Three thresholds,
    QuickFIX's: a heartbeat one `HeartBtInt` after this end last spoke, a test request 1.2
    intervals after the counterparty last did, the link at 2.4. A `TestRequest` is answered with
    the `112=` it carried; the one this session invents is the literal `TEST`, because the
    acceptance comparator reads tag 112 byte for byte.
  - Inbound `SequenceReset (35=4)`, gap fill and plain, and `ResetSeqNumFlag` on a Logon.
    **Whether a message's sequence number is checked, and whether it advances the count, is per
    `MsgType`** — a Logout is never checked and a `SequenceReset` never advances.
  - A frame the codec cannot read is now ignored rather than fatal, **unless it identifies
    itself as a Logon**. `MsgType` must be the third field; a message that puts it elsewhere is
    treated the same way.
  - `ResendRequest (35=2)` when a message runs ahead of the count. The message is **held**, not
    refused, and replayed in sequence order once the gap closes; the gap is asked for **once**,
    and a Logon that runs ahead is answered before it is asked for. Four held messages per
    connection, 512 bytes each — one that does not fit is dropped, which costs a round trip and
    never an allocation.
  - An inbound `ResendRequest` is answered with one `SequenceReset` gap fill. Every message this
    session has sent so far is administrative and QuickFIX never replays those. A store of
    application messages, and a real replay, are still to come.
  - Scores **42 / 59** on the acceptance definitions: step 5 of six.

- **`nanofix-dict`** — four validation tables, generated from `FIX44.xml`, answering the
  dictionary half of `Reject (35=3)`.
  - `Fix44::is_defined_tag(tag)` — a bitset over 0..=956. **No user-defined range**: QuickFIX's
    own header calls 5000..=9999 user-defined and the acceptance corpus expects `5000=HI`
    refused anyway. Answers `373=0`.
  - `Fix44::is_msg_type(msg_type)` — the 93 FIX 4.4 message types. `required()` could not answer
    this: it gives `&[]` for an unknown type and for a known one with no required fields alike.
    Answers `373=11`.
  - `Fix44::allows(msg_type, tag)` — one 120-byte bitset per message, header and trailer folded
    in so a caller asks once rather than three times. 12 524 (message, tag) pairs. Answers
    `373=2`.
  - `Fix44::field_type(tag)` and `FieldType::accepts(value)` — the 23 FIX 4.4 types and what each
    takes on the wire. `FieldType` is the one place the XML type names map to behaviour;
    `build.rs` includes the same file by path rather than restating it. Answers `373=6`.
  - `Fix44::required_header()` — the 7 header fields every message must carry. `required()`
    answers for a message body and says so in its own doc; this is the other half, and
    `14b_RequiredFieldMissing.def` needs both.
  - `Fix44::enum_allows(tag, value)` — 245 enumerated fields, 1 708 values, 98 distinct lists
    after deduplication. `None` means *not enumerated*, never *fine*. Answers `373=5`.
  - `SEQNUM` accepts `0`. It did not, on a rule this project invented and an invented test
    that agreed with it. `11a`, `11b` and `11c` all send `34=0` and QuickFIX processes them —
    the restored rule costs three files. See `docs/reference/fix44-dictionary-traps.md`.
  - Roughly **33 KB** of static data; the build script's run time is unchanged at under a second.
  - **A field type the enum does not know stops the build.** Falling through to `STRING` would
    make `373=6` blind to a whole type, and no acceptance definition would notice.

- **`nanofix-conformance`** — the 59 QuickFIX FIX 4.4 acceptance definitions, run in process
  with no socket. Zero runtime dependencies. Not published: it is a measuring instrument.
  - `script` — the corpus as 669 typed steps. Refuses to skip a directive it cannot read.
  - `compare` — `Comparator.rb`'s positional rules, with the five loosely-matched tags read
    out of `fields.fmt` rather than hard-coded.
  - `runner` — `SessionUnderTest`, keyed by connection so `1b_DuplicateIdentity` is
    expressible; `NullSession` scores **0 / 59** and `Replay` scores **59 / 59**.
  - The harness clock moves forward, and only when the file is waiting: before matching an `E`
    line the session has not answered, it advances one `HeartBtInt` — the file's own, from its
    Logon — and retries, at most three times. `[measured]` 33 of the 250 `E` lines have no `I`
    line in front of them, and that absence is the only "wait" the `.def` grammar has.
  - `echo` — the echo application the corpus assumes. All 22 application pairs reproduced.

- **`nanofix-codec`** — FIX 4.4 read and write, `no_std`, zero runtime dependencies.
  - `parse_into::<D, N>(buf, &mut idx, validation) -> Result<Parsed, ParseError>`. `Incomplete`
    is an `Ok`, because TCP delivers bytes and not messages.
  - `FieldIndex<N>` and `MessageView<'_, N>` — the caller owns the index and reuses it;
    the view is 24 bytes and `Copy`.
  - `Template<P, S>` and `TemplateBuilder` — a message skeleton that sorts its fields once, at
    build time, and fills holes at send time. Optional slots may be omitted.
  - `TimestampCache` — `SendingTime` with the minute prefix cached.
  - `MessageView::group` / `GroupIter` / `GroupEntry` — repeating groups **read** off the
    flat index, nested to the 4 levels FIX 4.4 reaches. No allocation: an iterator is a pair
    of positions into the index the parser already filled. `GroupIter` is an `Iterator`.
    `declared()` (what the counter says, `Option` — a non-numeric count is not a count) and
    `counted()` (what is on the wire) are reported separately and never reconciled here.
    `GroupIter` yields `GroupEntry`, which can itself be descended into.
  - `TemplateBuilder::group(counter)` and `Template::encode_with::<D>` — repeating groups
    **written**. `GroupData` / `GroupEntryData` are borrowed and recursive, so nesting costs
    no allocation. Field order inside an entry comes from `D::group_order`, never from the
    order the caller supplied: inside a group the order is not ascending by tag, so the rule
    that governs the body cannot catch a mistake there. The counter's value is
    `entries.len()`, so the count and the entries cannot disagree.
  - `EncodeError` gains `UnknownGroup`, `NotAGroupMember`, `MissingDelimiter`,
    `MsgTypeMissing` and `GroupTooDeep`.
  - `Dictionary` trait. `is_header` and `data_length_tag` are answered by `nanofix-dict`;
    of the three repeating-group methods, all three are now implemented there — reading and
    writing groups is not, and lands with the rest of the repeating-groups plan.
- **`nanofix-dict`** — 912 tag constants, 93 message types, `is_header`, `data_length_tag`,
  `required` and the group tables, all generated from `FIX44.xml` at build time.
  - `group_members(msg_type, counter)` — one table serving `group_delimiter` (its head) and
    `group_order` (itself), so the three cannot disagree. Keyed by **`(msg_type, counter)`**:
    four counters take a different delimiter in different messages.
  - `GROUP_COUNTERS = 59`, `GROUP_POSITIONS = 731`, and `GROUP_KEYS` — every declared
    `(msg_type, counter)` pair, so a caller can enumerate the groups rather than name them.

### Changed

- **`nanofix-conformance::script`** — `<TIME>` now substitutes a **real instant**
  (`20260828-12:00:00`, and `…​.000` on `E` lines) instead of `00000000-00:00:00`. The old
  value is the corpus's placeholder for output the comparator never reads by value, and it is
  not a date at all — month 00, day 00 — so no `SendingTime` check could be written against it.
  `FIXED_TIME_IN` and `FIXED_TIME_OUT` keep their names and their two widths; `FIXED_TIME_MILLIS`
  is new, and is what the runner ticks with.
- **`<TIME±N>` is now real arithmetic.** With the base at midnight of year zero there was
  nowhere to go backwards to, so the offset wrapped: `<TIME-121>` came out 86 279 seconds
  *forward*, in the one file that exists to test `SendingTime` accuracy.
- **`nanofix-conformance::text` moved to `nanofix-session::text`.** The table describes what a
  session says, and it lived in `conformance` only because no session crate existed. `codec`'s
  allocation bench loses its `text` case, which reappears in `session`'s.
- **`scripts/fetch-quickfix-assets.sh` fetches four more QuickFIX headers** — `FixFieldNumbers.h`,
  `FixFields.h`, `FixCommonFields.h` and `FixValues.h` — read as oracles, never copied. `vendor/`
  stays gitignored (ADR-0001).
- **`runner::run_scenario` seeds the clock**, sending `Input::Tick` before the connect and
  before every message. A session has no clock, so the harness is its clock. The value is fixed;
  advancing it is the heartbeat rule and belongs to a later step.

### Known limitations, stated rather than discovered

- `required()` does not descend into `<component>`, so it is wrong for 21 of the 93 message
  types. Nothing calls it. Component recursion arrives with the repeating-groups plan.
- Repeating groups are not read or written. `Template` sorts by tag, which would reorder a
  group's members.
- The trailer tags — `Signature(89)`, `SignatureLength(93)`, `CheckSum(10)` — are classified as
  neither header nor body, so a written `Signature` would sort into the body. Nothing writes one.
- `[measured]` Encoding an `ExecutionReport` costs 93.8 ns against a published target of 60 ns.

Crate names change before any publish: `nanofixengine` is a placeholder.

**Before the first publish, the crate names change.** `nanofixengine` is a placeholder taken
to clear a collision with `matthart1983/nanofix`; the shortlist is in [STATUS.md](STATUS.md).
Renaming after a crates.io publish is not possible, so it happens before.
