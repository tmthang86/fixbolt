# Use a venue dictionary

A venue's rules of engagement add to FIX 4.4: its own tags, new values on standard enums, repeating
groups and message types of its own, fields it makes required. In fixbolt that dialect is **a Rust
type your own build generates**, from QuickFIX-format XML, and the engine is served over that type.
Nothing is read at run time: the venue's fields are validated, parsed and written from generated
tables exactly as FIX 4.4's are
([ADR-0207](../decisions/ADR-0207-a-custom-dictionary-is-an-overlay-generated-in-the-users-build-into-the-users-own-type.md)).

This page follows [`examples/custom-dictionary`](../../examples/custom-dictionary/), a crate that
serves one counterparty over a dialect called `Venue`. Its dialect is **invented** — no field in
it comes from a real venue — and its socket tests are the proof behind each step. You need four
things: the dependencies, the XML, a `build.rs`, and the door that takes your type.

If all you need is to accept one extra tag at or above 5000, [Add a custom tag](add-a-custom-tag.md)
has a shorter way.

## 1. An overlay or a whole file

The generator takes FIX 4.4 dictionaries in two shapes, both QuickFIX-format XML
(`fixbolt_dict::codegen::Source`):

| | `Source::Fix44Overlay` | `Source::Fix44Whole` |
|---|---|---|
| What you write | only what the venue adds | a complete FIX 4.4 dictionary |
| Merged onto | the FIX 4.4 file fixbolt ships (`crates/dict/spec/FIX44.xml`) | nothing: read as written |
| May add fields, values, groups, components, messages, required fields, header fields | yes | yes |
| May remove or retype anything, or narrow an open field to a list | no — refused, naming both sides | yes |
| A message whose type field 35 does not list | its type is added to field 35 for you | refused at build time, naming it |
| Where it comes from | written by hand from the venue's specification | your existing customised QuickFIX `FIX44.xml`, as it is |

Use an overlay unless the venue changes something FIX 4.4 already says. An overlay that disagrees
with FIX 4.4 about a number, a name, a type or a `msgtype` stops the build with a sentence naming
both sides, so a mistake in the diff cannot silently become the dictionary. Both shapes are FIX
4.4 only; a dialect of FIXT 1.1 / FIX 5.0 SP2 is not supported (ADR-0207 decision 8).

Coming from QuickFIX with a `DataDictionary=` file already written?
[Migrate from QuickFIX](migrate-from-quickfix.md#4-your-datadictionary-xml) says how to turn it
into either shape.

## 2. The dependencies

Your crate depends on `fixbolt` at run time, and on `fixbolt-dict` with its `codegen` feature as a
**build-dependency** only — the XML parser runs in `build.rs` and never reaches your binary:

```toml
[dependencies]
fixbolt = { git = "https://github.com/tmthang86/fixbolt", rev = "<commit>" }

[build-dependencies]
fixbolt-dict = { git = "https://github.com/tmthang86/fixbolt", rev = "<commit>", features = ["codegen"] }
```

**The two lines must name the same fixbolt version.** They are two copies of `fixbolt-dict` — one
runs your `build.rs`, one is linked into your binary — and the generated file checks at compile
time that they agree (step 8). `codegen` and the doors in step 6 are listed under *Unreleased* in
[`CHANGELOG.md`](../../CHANGELOG.md) and are not in the `v0.1.0` tag, so until a tag carries them,
pin both lines to one commit; afterwards, to one `tag`. The example's own
[`Cargo.toml`](../../examples/custom-dictionary/Cargo.toml) takes both by path, since it lives in
this repository.

## 3. The overlay

The example's whole overlay, [`venue.xml`](../../examples/custom-dictionary/venue.xml). The comment
at its top says what each section adds:

<!-- sample: examples/custom-dictionary/venue.xml -->
```xml
<!--
  INVENTED FOR THIS EXAMPLE. This is not any venue's specification, rules of
  engagement or dialect, and no field, value or message in it is taken from
  one. Every name, number and value below was made up in this repository to
  show, and to test, what an overlay can add (ADR-0207 decision 3).

  An overlay onto the FIX 4.4 dictionary fixbolt-dict ships
  (crates/dict/spec/FIX44.xml). It adds, and never removes or retypes:

    <header>      VenueSessionTag(5002), a custom header field.
    <messages>    NewOrderSingle(D) gains VenueClientID(5001) as required='Y'
                  (a venue-required field) and the component VenueFeeGrp.
                  ExecutionReport(8) gains the same, optional, so the desk can
                  echo them. VenueFeeReport(U1) and VenueFeeAck(U2) are new
                  application message types.
    <components>  VenueFeeGrp holds NoVenueFees(5003), a custom repeating
                  group declared amount first: its delimiter is VenueFeeAmt
                  (5005), although VenueFeeType (5004) is the lower tag. The
                  order on the wire is this declaration's, never a handler's.
    <fields>      OrdType(40) gains the value 'Z'; 5001-5005 are added, and
                  VenueFeeType has enumerated values of its own.
                  MsgType(35): FIX 4.4 lists every message type as a value of
                  field 35. The generator adds each new message's msgtype
                  there itself (ADR-0207 decision 3), so U1 is not listed
                  below; U2 is, the QuickFIX habit, which is accepted and
                  changes nothing. Both are delivered.
-->
<fix type='FIX' major='4' minor='4' servicepack='0'>
 <header>
  <field name='VenueSessionTag' required='N' />
 </header>
 <messages>
  <message name='NewOrderSingle' msgtype='D' msgcat='app'>
   <field name='VenueClientID' required='Y' />
   <component name='VenueFeeGrp' required='N' />
  </message>
  <message name='ExecutionReport' msgtype='8' msgcat='app'>
   <field name='VenueClientID' required='N' />
   <component name='VenueFeeGrp' required='N' />
  </message>
  <message name='VenueFeeReport' msgtype='U1' msgcat='app'>
   <field name='ClOrdID' required='Y' />
   <component name='VenueFeeGrp' required='N' />
  </message>
  <message name='VenueFeeAck' msgtype='U2' msgcat='app'>
   <field name='ClOrdID' required='Y' />
  </message>
 </messages>
 <components>
  <component name='VenueFeeGrp'>
   <group name='NoVenueFees' required='N'>
    <field name='VenueFeeAmt' required='N' />
    <field name='VenueFeeType' required='N' />
   </group>
  </component>
 </components>
 <fields>
  <field number='35' name='MsgType' type='STRING'>
   <value enum='U2' description='VENUE_FEE_ACK' />
  </field>
  <field number='40' name='OrdType' type='CHAR'>
   <value enum='Z' description='INVENTED_VENUE_PEG' />
  </field>
  <field number='5001' name='VenueClientID' type='STRING' />
  <field number='5002' name='VenueSessionTag' type='STRING' />
  <field number='5003' name='NoVenueFees' type='NUMINGROUP' />
  <field number='5004' name='VenueFeeType' type='CHAR'>
   <value enum='1' description='INVENTED_FEE' />
   <value enum='2' description='INVENTED_REBATE' />
  </field>
  <field number='5005' name='VenueFeeAmt' type='AMT' />
 </fields>
</fix>
```

The rules an overlay is held to (`Source::Fix44Overlay`'s rustdoc has the full list):

- **Every section is optional**: `<header>`, `<messages>`, `<components>`, `<fields>`. Anything
  else — a `<trailer>`, a misspelt section — is refused, not ignored.
- **A new field needs a number FIX 4.4 does not use.** A field FIX 4.4 defines is repeated with its
  own number, name and type, and may gain `<value>`s only if FIX 4.4 already lists values for it:
  a list on a field FIX 4.4 leaves open would narrow it
  ([the trap](../reference/a-value-list-added-to-a-field-fix44-leaves-open-narrows-it-to-373-5.md)).
- **A message FIX 4.4 defines is repeated with its own `msgtype`** (and `msgcat`, if given) and
  lists only what it gains; `required='Y'` makes a gained field required. A new message needs a
  `msgtype` FIX 4.4 does not use.
- **A group keeps the order you declare.** `VenueFeeGrp` declares `VenueFeeAmt (5005)` before
  `VenueFeeType (5004)`, so 5005 is the delimiter and every entry is written amount first, whatever
  order a handler names the fields in.
- **One tag has one place**: the header, the trailer, or message bodies. A tag in two of them is
  refused ([why](../reference/a-tag-in-the-header-and-a-body-makes-a-valid-message-a-373-14.md)).
- **A DATA field needs its length field**, and inside a group the length field is declared
  immediately in front of it.

### MsgType (35): an overlay adds it, a whole file must list it

FIX 4.4 lists every message type twice: as a `<message>`, and as a `<value>` of field 35,
`MsgType`. The session checks field 35 like any other enumerated field, so a message type missing
from those values is answered `373=5` on tag 35, every time, before anything else is checked.
QuickFIX behaves the same way at run time, and its documentation makes adding a message two steps.

Here, **an overlay adds each new message's type to field 35 by itself** — `venue.xml` lists `U2`
there, the QuickFIX habit, and leaves `U1` out, and both are delivered. **A whole file is read as
written**, so a message its field 35 does not list stops the build:

```text
message NewOrderSingle has msgtype D, which field MsgType(35) does not list
among its values: every NewOrderSingle would be refused 373=5 on tag 35. Add
<value enum='D' .../> to field 35, or list no values there.
ADR-0207 decision 3.
```

That is a difference from QuickFIX worth knowing when you migrate: the same file there loads, and
every such message is rejected on the wire.
[The reference page](../reference/a-message-type-missing-from-msgtype-values-is-a-373-5-on-tag-35.md)
has the sources and the tests.

## 4. The build script

`build.rs` reads the overlay, reports the size of the tables it will generate, and writes one Rust
file into `$OUT_DIR`. The body of the example's `fn main() -> Result<(), Box<dyn std::error::Error>>`:

<!-- sample: examples/custom-dictionary/build.rs#generate -->
```rust
    println!("cargo:rerun-if-changed=venue.xml");
    let overlay = std::fs::read_to_string("venue.xml")?;
    let source = Source::Fix44Overlay(&overlay);
    // What `codegen`'s own documentation recommends: a high custom tag widens
    // every per-tag bitset, so the build says how large the tables came out.
    println!(
        "cargo:warning={}",
        codegen::merged_model(source)?.table_size()
    );
    let rust = codegen::generate(source, "Venue", Paths::facade())?;
    let out_dir = std::path::PathBuf::from(std::env::var("OUT_DIR")?);
    std::fs::write(out_dir.join("venue.rs"), rust)?;
```

(The rest of the example's `build.rs` generates a second, empty type for one of its tests; your
build does not need it.)

- `generate(source, "Venue", Paths::facade())` writes the tables, a unit struct named `Venue`, and
  its `impl Dictionary` and `impl Tables`. `Paths::facade()` makes the file name
  `::fixbolt::dict::…`, which is why `fixbolt` is the only run-time dependency. `Paths::direct()`
  names `::fixbolt_dict` and `::fixbolt_codec` instead, for a crate that uses `fixbolt-engine`
  without the facade.
- For a whole file, pass `Source::Fix44Whole(&text)`.
- `cargo:rerun-if-changed=venue.xml` is what makes an edit to the XML regenerate the type.
- A refusal is a `GenError` returned from `main`: cargo prints its sentence and the build stops.
  Nothing in the generator panics.

## 5. Include the type

Put the generated file in a module of its own — it brings a `tables` module and a format check with
it, and those names must not meet your crate's:

<!-- sample: examples/custom-dictionary/src/lib.rs#include -->
```rust
/// The venue's dictionary, generated by `build.rs` from `venue.xml`.
///
/// A module of its own, because the generated file brings a `tables` module
/// and a format check with it, and those names must not meet this crate's.
pub mod venue {
    include!(concat!(env!("OUT_DIR"), "/venue.rs"));
}
```

`venue::Venue` now implements `fixbolt::dict::Dictionary` and `fixbolt::dict::Tables`, like
`fixbolt::dict::Fix44`.

## 6. Write the handler over it, and serve it through a `_over` door

The dictionary is the last type parameter of `Handler`, `Incoming`, `Reply` and `App`; it defaults
to `Fix44`, so a FIX 4.4 application never writes it. The example's handler names `Venue`:

<!-- sample: examples/custom-dictionary/src/lib.rs#handler -->
```rust
impl Handler<256, 64, 1024, Venue> for Desk {
    fn on_message(
        &mut self,
        msg: &Incoming<'_, 256, Venue>,
        reply: Reply<'_, 64, 1024, Venue>,
    ) -> Answer {
        match msg.msg_type() {
            b"D" => self.fill(msg, reply),
            b"U1" => reply
                .message(b"U2")
                .field(11, msg.get(11).unwrap_or_default())
                .send(),
            _ => reply
                .business_reject(
                    msg.seq().unwrap_or_default(),
                    msg.msg_type(),
                    3,
                    b"this desk takes orders and fee reports",
                )
                .send(),
        }
    }
}
```

A custom repeating group is read through the dialect's tables — `group::<Venue>` knows
`NoVenueFees`' delimiter and members because the overlay declared them:

<!-- sample: examples/custom-dictionary/src/lib.rs#group -->
```rust
        // The group, read through the venue's tables. Each entry is named here
        // in tag order — type, then amount — and goes out in `venue.xml`'s
        // declared order, amount first: the order is the dictionary's, never
        // this call site's (non-negotiable 5).
        let mut rows: [[(u32, &[u8]); 2]; MAX_FEES] = [[(0, b""); 2]; MAX_FEES];
        let mut n = 0;
        if let Some(fees) = msg.view().group::<Venue>(b"D", NO_VENUE_FEES) {
            for (row, entry) in rows.iter_mut().zip(fees) {
                *row = [
                    (
                        VENUE_FEE_TYPE,
                        entry.get(VENUE_FEE_TYPE).unwrap_or_default(),
                    ),
                    (VENUE_FEE_AMT, entry.get(VENUE_FEE_AMT).unwrap_or_default()),
                ];
                n += 1;
            }
        }
```

The engine is told the dictionary through its **encoding**, `TagValue<Venue, N>`, handed to one of
three doors that take an encoding: `serve_over` (`standard` acceptor), `serve_hft_over` (`hft`
acceptor, same arguments) and `connect_and_serve_over` (initiator). `serve`, `serve_hft` and
`connect_and_serve` are these doors with `TagValue<Fix44, N>` filled in. The example's `main`:

<!-- sample: examples/custom-dictionary/src/main.rs#serve -->
```rust
    use custom_dictionary::Desk;
    use custom_dictionary::venue::Venue;
    use fixbolt::dict::TagValue;
    use fixbolt::{App, Config, Limits, NoLog, NoRecovery, Table};

    let addr = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "127.0.0.1:9877".to_owned());
    let table = Table::new().serving(Config::acceptor(b"FIX.4.4", b"ISLD", b"TW44"));
    println!("serving TW44 over the Venue dictionary on {addr}");

    let handles = fixbolt::Handles::new();
    let admin = handles.admin();
    // A line on stdin stops it, as in the FIX 4.4 example.
    std::thread::spawn(move || {
        let mut line = String::new();
        let _ = std::io::BufRead::read_line(&mut std::io::stdin().lock(), &mut line);
        admin.shutdown(5_000);
    });

    // **`Venue` twice, and the two must be the same type.** The encoding is
    // what the session validates by; the `App`'s last parameter is what the
    // handler reads and writes by. Nothing in the types ties them.
    let shutdown = fixbolt::serve_over::<
        256,
        4096,
        8192,
        1024,
        TagValue<Venue, 256>,
        _,
        fixbolt::Store,
        _,
        _,
    >(
        &addr,
        table,
        App::<Desk, 256, 64, 1024, Venue>::with_sizes(Desk::default()),
        64,
        Limits::new(64, 30_000)?,
        NoRecovery,
        NoLog,
        handles,
    )?;
```

**`Venue` is written twice, and nothing checks that the two agree.** The encoding is what the
session validates by; the `App`'s last parameter is what the handler reads and writes by. An `App`
over `Venue` behind a door told `TagValue<Fix44, 256>` — or behind `fixbolt::serve` — compiles, and
the session then refuses the venue's own tags as `373=0`
(`tests/venue.rs::a_venue_app_behind_a_fix44_door_compiles_and_the_session_rejects_the_venue_tag_373_0`).
Name the type once, as an alias, and use the alias in both places. [GUIDE.md §3a](../GUIDE.md)
says why the types do not tie them.

The sharded and TLS doors are FIX 4.4 only. A deployment that needs one of them over a
dialect drives `fixbolt_engine::Engine` itself, whose encoding is a type parameter
([GUIDE.md §3a](../GUIDE.md)).

## 7. Several venues are several engines

Each dialect is its own type, so two venues that give tag 5001 two different meanings live in one
binary without conflict: `VenueA` and `VenueB`, each generated from its own overlay by the same
`build.rs`, each served by its own engine on its own port (ADR-0207 decision 6). One engine does
not choose a dictionary per counterparty: every counterparty in one engine's table speaks the
encoding that engine was built with.

## 8. What you pay, and what can go wrong

**A dialect change is a rebuild.** The venue changes its specification; you edit the XML, rebuild,
and redeploy. There is no file to swap under a running process, and a configuration file cannot
name a dictionary: `DataDictionary=` and its three siblings are refused by name
([CONFIGURATION.md §1](../CONFIGURATION.md#1-configuration-file-keys)). That is the cost of tables
the compiler can see, and it was accepted as one (ADR-0207 *Consequences*).

**A high tag number makes the tables larger.** Several tables are bitsets indexed by tag number, one
per message type, so their size follows the **highest** tag, not the number of fields
([the trap](../reference/a-bitset-keyed-by-tag-scales-with-the-highest-tag-not-the-field-count.md)).
The generator reports the size, and the example's `build.rs` prints it as a warning:

```text
warning: fixbolt-example-custom-dictionary@0.0.0: the highest tag, 5005, makes each per-tag bitset 79 words: 60672 bytes of static tables for 95 message types
```

FIX 4.4 alone is 15 words per bitset (highest tag 956); one tag at 20 000 makes it 313 words and
237 880 bytes (`crates/dict/tests/overlay.rs::a_high_tag_reports_the_table_size`). The tables are
static data: nothing is allocated at run time, but binary size and cache footprint grow. Tables
over **64 MiB** are refused, naming the tag and the size: *"the highest tag, …, would make the
per-tag bitsets … bytes, over the 64 MiB ceiling"* (`a_tag_whose_tables_pass_64_mib_fails_naming_it`).
If a venue uses one very high tag, that tag sets the size for all of them.

**The format check.** If the build-dependency and the dependency are different fixbolt versions,
the build fails at the generated file's first line, with a sentence like this one:

```text
error[E0080]: evaluation panicked: Venue was generated by fixbolt-dict's codegen at format 1, and the fixbolt-dict it is compiled against reads another format: give the build-dependency and the dependency the same fixbolt version
```

It means what it says: the generator that wrote the file and the crate compiling it read the
tables differently. Make the two lines in step 2 name the same commit or tag. The check exists so
that the mismatch is this sentence rather than a table read wrongly at run time
(`codegen`'s rustdoc holds it as a `compile_fail` doctest).

**A refused dictionary.** Every other build failure from the generator is a sentence naming what
disagrees: both sides of a conflict with FIX 4.4, the field twice at one level with its component
path, a tag in the header and a body, a message missing from field 35. Not well-formed XML is
*"not well-formed XML: …"* followed by the parser's own message.

## 9. The QuickFIX notice travels with your tables

An overlay is merged onto `FIX44.xml` as fixbolt ships it — QuickFIX's file, under the QuickFIX
Software License — so your generated tables are derived from it, exactly as `Fix44`'s are. A binary
you distribute owes that licence's conditions 2 and 3, as [GUIDE.md §10](../GUIDE.md#10-distributing-a-binary-carries-a-quickfix-notice-obligation)
describes; printing `fixbolt::NOTICE` covers condition 3. A whole file brings whatever licence your
own copy is under, in addition. The obligation is yours as the distributor.

## What the example proves, and what it does not

`examples/custom-dictionary/tests/venue.rs` sends each thing `venue.xml` adds through a real socket
and reads the answer: a custom tag, a custom enum value, a custom group in declared order, a custom
header field written back in the header, a new message type, a venue-required field, and an
undefined tag still refused. `benches/alloc.rs` holds the handler over `Venue` to zero allocations
per message. `tests/plain_is_fix44.rs` holds a type generated from an empty overlay to `Fix44`,
answer for answer, which is what carries the 59 acceptance definitions' result to a generated type
([SESSION-BEHAVIOUR.md §3c](../SESSION-BEHAVIOUR.md)).

None of it says anything about **your** venue: the fixture is invented, and no venue's rules of
engagement may be committed to this repository. Test your dialect against the venue's own
certification environment.
