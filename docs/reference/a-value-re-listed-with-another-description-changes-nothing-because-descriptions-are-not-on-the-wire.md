# A value re-listed with another description changes nothing, because descriptions are not on the wire

`[found 2026-09-26, step 22 of docs/plans/2026-09-26-docs-for-embedders.md]` An overlay that
repeats a field FIX 4.4 defines must agree with it on number, name and type, or the build fails
naming both (ADR-0207 decision 3). The same strictness for `<value>` would compare
`description` too, and would refuse a venue that writes this:

```xml
<field number='40' name='OrdType' type='CHAR'>
  <value enum='1' description='VENUE_MARKET' />
</field>
```

FIX 4.4 describes `OrdType=1` as `MARKET`. A venue's rules of engagement commonly rename values
like this, and a user copying their venue's field list into an overlay would hit the refusal on
every such line.

Only `enum` reaches the wire. The generator never reads `description`: `parse.rs` collects a
field's `enum` attributes and nothing else, and no table holds a name for a value. So the
re-listed `1` changes nothing that any generated table holds. Refusing it would protect no
message and would cost the user an edit per line.

**Rule.** A value FIX 4.4 already lists is skipped, whatever its description says
(`crates/dict/src/codegen/merge.rs::overlay_values`). Only a value FIX 4.4 does not list is added,
and only to a field that is already enumerated
([a-value-list-added-to-a-field-fix44-leaves-open-narrows-it-to-373-5](a-value-list-added-to-a-field-fix44-leaves-open-narrows-it-to-373-5.md)).
The consequence: a description in an overlay is documentation for its reader and is not visible
anywhere in the generated type.

**Guarded by**
`crates/dict/tests/overlay.rs::a_value_already_listed_with_another_description_is_accepted_because_descriptions_are_not_on_the_wire`,
which re-lists `OrdType=1` as `VENUE_MARKET`. It asserts that the value is still allowed and that
the merged `Model` equals the shipped FIX 4.4's, field for field.
