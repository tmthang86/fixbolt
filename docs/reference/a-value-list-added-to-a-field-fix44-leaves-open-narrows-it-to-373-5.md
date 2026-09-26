# A value list added to a field FIX 4.4 leaves open narrows it to `373=5`

`[found 2026-09-26, step 22 of docs/plans/2026-09-26-docs-for-embedders.md]` An overlay adds and
never removes (ADR-0207 decision 3). Adding a `<value>` to a field reads like an addition. For an
**enumerated** field, one that already lists values in `FIX44.xml` (`OrdType(40)`, `ExecInst(18)`),
it is one: the list grows, and every value it held before is still allowed.

For a field FIX 4.4 leaves **open**, with no `<value>` at all (`Symbol(55)`, `Text(58)`), it is the
opposite. `Tables::enum_allows` answers `None` for a field that is not enumerated, which means
*not enumerated* and not *the value is fine*
(`crates/dict/src/tables.rs`). The session rejects only on `Some(false)`
(`crates/session/src/lib.rs`, the `SessionText::ValueIsIncorrect` arm). So one `<value
enum='VENUE'>` on `Symbol` turns the field from open into a list of one. After that, every other
symbol on the wire is `35=3` with `373=5` (*Value is incorrect (out of range) for this tag*). The
overlay removed every value but one while looking like it added one.

The FIXT pair merge has no such case. There both files are complete dictionaries, and its
superset rule (`crates/dict/src/codegen/merge.rs`, `merge_fields`) compares two lists that each
side meant.

**Rule.** An overlay that lists values for a field FIX 4.4 leaves open is refused, naming the field
and its number. The sentence (`merge.rs::overlay_values`) says the change narrows the field and
that a dictionary which does narrow a field is a whole file (`Source::Fix44Whole`). A whole file
is read as written, so a venue that really does restrict `Symbol` to a list can say so there, on
purpose.

**Guarded by** `crates/dict/tests/overlay.rs::a_value_list_on_a_field_fix44_leaves_open_fails`,
which lists one value on `Symbol(55)` and expects a refusal naming `Symbol` and `55`.
`an_added_enum_value_is_allowed_and_the_old_ones_still_are` is the allowed case, and
`an_added_value_on_a_multi_value_field_is_checked_per_token` is its `MULTIPLEVALUESTRING` form.
