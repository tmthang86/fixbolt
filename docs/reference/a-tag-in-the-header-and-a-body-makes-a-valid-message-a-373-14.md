# A tag in the header and in a body makes a valid message a 373=14

`[found 2026-09-26, senior review of PR 5, docs/plans/2026-09-26-docs-for-embedders.md]` The
overlay generator accepted this, and wrote a table that compiled:

```xml
<header>
  <field name='Account' required='N' />
</header>
```

`Account(1)` is a body field of about forty FIX 4.4 messages. After the merge it was in the
header table (`is_header(1)`) **and** in those messages' `allows` bitsets. Nothing refused it,
because the header and a message body are separate tables and each was right on its own.

**What goes wrong is in the readers of `is_header`, not in the tables.**

- The session checks field order by it. Once a body tag has been seen, a tag for which
  `is_header` answers `true` is `373=14`, *Tag specified out of required order*
  (`crates/session/src/lib.rs`, the `14g_HeaderBodyTrailerFieldsOutOfOrder.def` check). An inbound
  `NewOrderSingle` whose `Account` follows `ClOrdID`, which is valid FIX 4.4, is rejected.
- The writer sorts by it. `crates/codec/src/template.rs::key` ranks a header tag before every
  body tag, so an outbound message's `Account` moves out of the body into the header.

The same shape holds for the trailer. `<header><field name='CheckSum'/></header>` puts tag 10 in
the header. `Signature(89)` or `SenderSubID(50)` added to a message body puts a trailer or a
header tag in a body.

**FIX 4.4 never does this.** `[measured 2026-09-26]` On the shipped `spec/FIX44.xml`, the header,
the trailer and every message body (components and groups included) are pairwise disjoint. The
FIXT 1.1 + FIX 5.0 SP2 pair also builds under the rule. So every overlap is one an overlay or a
user's whole file introduced.

**Rule.** A tag has exactly one place: the header, the trailer, or message bodies. After the
merge, `Model::compute` (`crates/dict/src/codegen/model.rs`) refuses any tag in two of them,
naming the tag, its name and both places. A whole file is held to the same rule.

**Guarded by** `crates/dict/tests/overlay.rs`:
- `a_header_field_that_a_body_declares_fails_naming_both`: `Account`
- `a_trailer_field_added_to_the_header_fails_naming_both`: `CheckSum`
- `a_header_field_added_to_a_body_fails_naming_both`: `SenderSubID`
- `a_trailer_field_added_to_a_body_fails_naming_both`: `Signature`
- `fix44_keeps_header_trailer_and_bodies_apart` holds the shipped file and an empty overlay
  to the rule.
