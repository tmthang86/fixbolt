# Add a custom tag

A counterparty sends a tag FIX 4.4 does not define — `5001=ACCT-7` on every order, say. With the
defaults, fixbolt answers it the way QuickFIX does with its own defaults: a session-level
`Reject` (`35=3`) naming the tag, with `373=0`, *Invalid tag number*. The order never reaches your
handler.

There are two ways to accept it, and they differ in what the session still checks.

| | A. Let it through, unchecked | B. Define it in your own dictionary |
|---|---|---|
| What you change | one setting | your build: an overlay XML, a `build.rs`, the type it generates |
| Which tags | every tag **at or above 5000**, defined or not | exactly the tags you define, at any unused number |
| What the session checks about the tag | nothing but its presence in the frame | its type, its enum values, which messages carry it, whether it belongs in the header, its repeating group, whether it is required |
| A custom repeating group, header field, message type or required field | not possible | yes |
| Changing it later | a configuration edit and a restart | a rebuild |

Choose **A** when the venue's extra tags are few, flat, at 5000 or above, and your handler only
reads them. Choose **B** as soon as one of them is a repeating group, a new message type, a
required field, a tag below 5000, or a value you want the session to refuse when it is wrong.

---

## A. Let it through: `ValidateUserDefinedFields=N`

In the configuration file, in `[DEFAULT]` or in the counterparty's `[SESSION]`:

```ini
ValidateUserDefinedFields=N
```

In code, the same setting is `Config::with_validation(DictionaryChecks::new().skipping_user_defined_fields())`.
Every key and its default are in [CONFIGURATION.md §1](../CONFIGURATION.md#1-configuration-file-keys).

Your handler then reads the tag like any other — `msg.get(5001)` returns its bytes. Reading a tag
needs no dictionary: the message's index holds every tag in the frame.

**What it does.** The session's field scan skips every tag at or above 5000
(`fixbolt_session::FIRST_USER_DEFINED_TAG`) wherever it sits, in the body or inside a group. It does
not ask whether the tag is defined (`373=0`), whether this message type carries it (`373=2`),
whether its value is in range (`373=5`) or well formed (`373=6`), or whether it sits where header
fields sit (`373=14`); and the count of a group whose counter is at or above 5000 is not checked.

**What it does not do.**

- **It is a range, not an amnesty.** An undefined tag *below* 5000 — `999`, `4000` — is still
  `373=0`. So is `0`, and so is `-1`.
- It does not forgive a tag below 5000 that FIX 4.4 defines, on a message that does not carry it.
  That is `AllowUnknownMsgFields=Y` (`373=2`), which never forgives a tag the dictionary does not
  know.
- It cannot make a custom repeating group readable as a group. Iterating a group needs its
  delimiter and members, which only a dictionary holds; with this setting the group's tags arrive
  as flat fields.

`crates/session/tests/validation_knobs.rs` holds the setting's effect and its limit, each against
the other knob's fault; `crates/engine/tests/settings_wire.rs::a_validation_knob_written_in_a_file_reaches_a_session_over_a_real_socket`
proves the key in a file reaches a session over a socket.

---

## B. Define it: a dictionary of your own

Write only what the venue adds, as QuickFIX-format XML, and let your build merge it onto the FIX
4.4 dictionary fixbolt ships. For the one tag above, on `NewOrderSingle`, the overlay is:

```xml
<fix type='FIX' major='4' minor='4' servicepack='0'>
 <messages>
  <message name='NewOrderSingle' msgtype='D' msgcat='app'>
   <field name='VenueClientID' required='N' />
  </message>
 </messages>
 <fields>
  <field number='5001' name='VenueClientID' type='STRING' />
 </fields>
</fix>
```

`NewOrderSingle` is repeated with its own `msgtype` and only the field it gains; everything FIX 4.4
already says about it stays. `required='Y'` would make the venue's field required. The tag number
may be anything FIX 4.4 does not use, below 5000 included.

Your `build.rs` turns that file into a type — call it `Venue` — and your engine is served over it.
[Use a venue dictionary](use-a-venue-dictionary.md) is that walk-through, step by step, around the
crate [`examples/custom-dictionary`](../../examples/custom-dictionary/), whose overlay defines five
custom tags, a custom enum value, a group, a header field and two message types.

The handler reads the tag exactly as in A. In the example, the desk echoes the venue's two custom
fields back on its `ExecutionReport`:

<!-- sample: examples/custom-dictionary/src/lib.rs#echo -->
```rust
        for tag in [VENUE_CLIENT_ID, VENUE_SESSION_TAG] {
            if let Some(value) = msg.get(tag) {
                m.field(tag, value);
            }
        }
```

The difference is on the way in. Over `Venue` the session checks `5001` as it checks a FIX 4.4
field — and still refuses every tag neither FIX 4.4 nor the overlay defines:

| Sent | Answered | Held by (`examples/custom-dictionary/tests/venue.rs`) |
|---|---|---|
| an order carrying `5001=ACCT-7` | an `ExecutionReport` echoing `5001=ACCT-7` | `a_custom_tag_reaches_the_handler` |
| an order with `40=Z`, a value the overlay adds to `OrdType` | an `ExecutionReport` with `40=Z` | `a_custom_enum_value_is_not_rejected` |
| an order without `5001`, which the example's overlay makes required | `35=3`, `371=5001`, `373=1` | `a_missing_venue_required_field_is_rejected_373_1` |
| an order carrying `4000=X` | `35=3`, `371=4000`, `373=0` | `an_undefined_tag_is_still_rejected_373_0` |

## Do not combine the two by accident

`ValidateUserDefinedFields=N` keeps its meaning over your own dictionary, and that meaning is the
range: **it also skips the tags at or above 5000 that your overlay defines.** Their type and value
go unchecked; a required one is still required. [CONFIGURATION.md §1](../CONFIGURATION.md#1-configuration-file-keys)
has the measurement. The setting is still useful beside a dialect — for a venue that sends
undefined tags at or above 5000 you do not care about, which pass while your own are read
(`an_undefined_user_tag_passes_when_user_defined_fields_are_skipped`):

<!-- sample: examples/custom-dictionary/tests/venue.rs#skipping -->
```rust
#[test]
fn an_undefined_user_tag_passes_when_user_defined_fields_are_skipped() {
    let mut c = Client::logged_on(DictionaryChecks::new().skipping_user_defined_fields());
    c.send("D", &order("5999=X\u{1}"));
    let got = c.next();
    assert!(
        got.contains("|35=8|"),
        "tag 5999, undefined but user-defined, was refused while skipped: {got}"
    );
}
```

— but then the session no longer validates the dialect's own tags at or above 5000. If you defined
them so that a wrong value is refused, leave the setting at `Y`.
