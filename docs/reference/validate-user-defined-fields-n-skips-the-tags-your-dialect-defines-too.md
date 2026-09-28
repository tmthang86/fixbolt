# `ValidateUserDefinedFields=N` skips the tags your dialect defines, too

`[found 2026-09-28, step 29 of docs/plans/2026-09-26-docs-for-embedders.md]` ADR-0207 decision 7
said the setting "governs only tags the dialect does not define". Read that way, a venue dialect
that defines tag 5004 with enumerated values would still see a wrong value refused with the setting
off. It does not.

**What happens.** With `ValidateUserDefinedFields=N` (`DictionaryChecks::skipping_user_defined_fields`),
the session's field scan skips **every** tag at or above 5000 (`FIRST_USER_DEFINED_TAG`) before it
asks anything about it (`crates/session/src/lib.rs`, the `skips_user_defined_fields()` checks in the
field scan, the group-member pass and the group-count pass). Whether the dictionary defines the tag
is never asked. Over the example crate's `Venue` dialect:

| Sent on an order | Default | `ValidateUserDefinedFields=N` |
|---|---|---|
| `5004=9`, a value `VenueFeeType` does not list | `373=5` on 5004 | accepted, `35=8` |
| `5005=abc`, an `AMT` that is no amount | `373=6` on 5005 | accepted, `35=8` |
| no `5001`, which the dialect makes required | `373=1` on 5001 | `373=1` on 5001 |

The last row differs because the required-field check reads the dictionary's list of required tags
and looks each one up in the message; it never walks the wire's tags, so the skip does not reach it.

**QuickFIX C++ differs.** At the vendored pin `386ce46`, `DataDictionary::iterate`
(`src/C++/DataDictionary.cpp` 167–171) calls `checkValidFormat` and `checkValue` on every field
before `shouldCheckTag` decides whether to skip a user-defined one; only the tag-number, in-message
and group-count checks are skipped. A defined user tag's format and value are still checked there.

**Rule, for now.** The behaviour stays: changing it is a session-layer change, left to a later plan
(ADR-0207 decision 7, revised 2026-09-28). With a dialect that defines your venue's tags at or above
5000, leave the setting at `Y`; turn it off only for a venue that also sends undefined tags you do
not care about, knowing your own tags then go unchecked too.

**Guarded by**
`examples/custom-dictionary/tests/venue.rs::a_defined_user_tag_is_not_type_checked_when_user_defined_fields_are_skipped`,
which sends both faulty orders with the default (the control: `373=5`, `373=6`) and with the
setting off (`35=8` for both), and a missing `5001` with the setting off (`373=1`). Proven by
reversal: with the setting-off client given the default checks, it fails with *"under
ValidateUserDefinedFields=N, 5004=9 is no longer passed unchecked"* on a `373=5` reply. When a
later plan changes the behaviour, this test goes red first, as it should.
