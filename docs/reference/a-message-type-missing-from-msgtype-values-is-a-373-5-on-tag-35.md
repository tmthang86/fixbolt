# A message type missing from MsgType's values is a `373=5` on tag 35

`[found 2026-09-28, step 27 of docs/plans/2026-09-26-docs-for-embedders.md]` An overlay that adds
a `<message>` reads as complete: the message has a name, a `msgtype`, its fields. Generated as a
plain merge, it produced a dictionary in which `is_msg_type(b"U1")` was true and every `U1` on the
wire was still refused:

```text
35=3|…|371=35|372=U1|373=5
```

*Value is incorrect (out of range) for this tag*, on tag 35, before any other check looked at the
message. The example crate's `a_custom_message_type_is_delivered` read exactly that line before the
rule below existed.

**Why.** FIX 4.4 does not only list its messages under `<messages>`. It also lists every message
type as a `<value>` of field 35, `MsgType`: `crates/dict/spec/FIX44.xml` has 93 messages and 93
values on field 35, the same set `[measured 2026-09-28]`. Field 35 is therefore an **enumerated**
field, and the session checks it the way it checks any enumerated field (`Tables::enum_allows`,
`Some(false)` is `373=5`). A message added only under `<messages>` is a message type the
dictionary knows and a value its own field 35 refuses.

**QuickFIX has the same two lists.** Loading a `<message>` adds its type to the message set and a
*name* for the value on field 35, never an allowed value: `src/C++/DataDictionary.cpp` 376–380 at
the vendored pin `386ce46` (`addMsgType`, then `addValueName(35, …)`), while the value check reads
the separate list `addFieldValue` fills. QuickFIX/n's documentation on custom messages makes adding
one two steps, the `<message>` **and** a `<value>` on field 35
([Custom Fields, Groups, and Messages](https://quickfixengine.org/n/documentation/custom-fields-groups-messages.html)).
A QuickFIX user who forgets the second step meets the same `373=5`, at run time, on every such
message.

**Rule** ([ADR-0207](../decisions/ADR-0207-a-custom-dictionary-is-an-overlay-generated-in-the-users-build-into-the-users-own-type.md)
decision 3, revision of 2026-09-28). MsgType(35) follows `<messages>`:

- An **overlay** that adds a message adds its `msgtype` to field 35's values during the merge.
  Listing it on field 35 as well — the QuickFIX habit, and what a migrated file already has — is
  accepted and changes nothing.
- A **whole file** is read as written, so a message whose type its field 35 does not list is
  refused at build time, naming the message, the type and `MsgType(35)`:
  *"message NewOrderSingle has msgtype D, which field MsgType(35) does not list among its values:
  every NewOrderSingle would be refused 373=5 on tag 35."* A field 35 that lists no values at all
  is not enumerated, takes any type, and passes.
- The check runs on the merged model, after the merge's own agreement checks, so an overlay can
  never trip it and an existing refusal still names its own conflict first.

**Guarded by** `crates/dict/tests/overlay.rs`:
`an_added_message_type_is_an_allowed_value_of_msgtype` (red before the rule with *"U1, a message
the overlay adds, is not an allowed MsgType"*), `an_added_message_type_also_listed_on_msgtype_is_accepted`,
`a_whole_file_whose_message_type_is_not_listed_on_msgtype_fails_naming_it` and
`a_whole_file_whose_msgtype_lists_no_values_generates`; and over a socket by
`examples/custom-dictionary/tests/venue.rs::a_custom_message_type_is_delivered`, whose
`venue.xml` adds `U1` without listing it on field 35 and `U2` with it, so both habits run.
`an_existing_message_repeated_with_another_msgtype_fails_naming_both` stays green unedited, which
is what shows the rule runs after the merge's conflict checks.
