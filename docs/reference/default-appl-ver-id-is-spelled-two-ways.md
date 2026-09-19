# `DefaultApplVerID` is spelled two different ways at the two ends

`[measured 2026-09-19]` building the interop `FIXT` arm against QuickFIX at pin
`386ce46e917ae494ab6e90b1be90fd421cdbe3f9`.

## The trap

FIXT 1.1 splits the transport version from the application version, and the application
version is named in **two places that do not use the same spelling**:

| Where | Spelling | Example |
|---|---|---|
| QuickFIX's `.cfg` file | the **version name** | `DefaultApplVerID=FIX.5.0SP2` |
| this engine's settings file | the **enum value** | `DefaultApplVerID=9` |
| on the wire, tag `1137` | the **enum value** | `1137=9` |

Write `9` into the QuickFIX cfg, or `FIX.5.0SP2` into this engine's settings, and you get a
session that dies at logon. The failure is not helpful about why.

A FIXT config is also not shaped like a FIX 4.4 one. It takes **two** dictionaries, not one:

```
BeginString=FIXT.1.1
DefaultApplVerID=FIX.5.0SP2
TransportDataDictionary=<abs>/spec/FIXT11.xml
AppDataDictionary=<abs>/spec/FIX50SP2.xml
```

against FIX 4.4's single `DataDictionary=`. QuickFIX wants absolute paths for both.

## Why the two spellings exist

They are answering different questions. QuickFIX's cfg is read by a human and by a dictionary
loader, so it names the version the way the specification does. Tag `1137` is `ApplVerID`, an
enumerated field, and enumerated fields carry their enum value on the wire — `9` *is* FIX 5.0
SP2 in that enumeration, the same way `7` is FIX 5.0 and `8` is FIX 5.0 SP1.

So neither end is wrong, and neither is a translation of the other. There is simply a value and
a name for the same thing, and the two files disagree about which to use.

## What this engine does

`crates/engine/src/settings.rs` takes the **wire value**, because that is what it writes into
`1137=` and what `Config::acceptor_fixt` stores. It deliberately does **not** validate the value
against the ten `ApplVerID` strings: that list is `fixbolt-session`'s to enforce, on the wire,
the same way an out-of-family `1128` already is. A second copy in the settings parser would be a
second rule.

`tools/interop` checks the pair symmetrically at its own front door — `--begin-string FIXT.1.1`
requires `--default-appl-ver-id`, and `--default-appl-ver-id` is refused without it — so a
mismatch fails *there, with a reason*, rather than three seconds later as a silent timeout.

## What guards it

`scripts/interop.sh`, section `4k`, both directions. The guard that actually distinguishes this
arm from the eight FIX 4.4 arms beside it is the **`header` assertion**, which reads the real
inbound bytes off libquickfix's own transcript:

```
interop-fixt: header ok  acceptor: in  8=FIXT.1.1|9=76|35=A|34=1|49=FIXBOLT|…|1137=9|10=205|
```

That matters because the seven step names in this arm are identical to the seven in the FIX 4.4
arm, so without it a misconfigured FIXT arm would score 7/7 while running a 4.4 session. Proven
by reversal: pointing the initiator at `--begin-string FIX.4.4` while the C++ acceptor is
configured for FIXT makes libquickfix close the socket and prints

```
interop-fixt: header FAIL  no inbound 8=FIXT.1.1 Logon carrying 1137= in the C++ acceptor's transcript
```

Three further reversals hold the settings and argument halves: `FIXT.1.1` without
`--default-appl-ver-id`, `--default-appl-ver-id` under `FIX.4.4`, and a settings file with
`BeginString=FIXT.1.1` and no `DefaultApplVerID` — the last arriving as
`Problem::DefaultApplVerIdRequired` through the tool's front door.

## Related

- [ADR-0080](../decisions/ADR-0080-the-dictionary-rides-the-encoding-and-a-fixt-session-is-one-table-built-from-two-xml-files.md)
  decision 4 — why the key is required under `FIXT.1.1` and refused otherwise.
- [ADR-0042](../decisions/ADR-0042-a-second-implementation-is-the-only-independent-opinion.md)
  — why an arm judged by somebody else's engine is worth this much trouble.
- [fixt-dictionary-traps](fixt-dictionary-traps.md) — the traps inside the pair itself.
