# Migrate from QuickFIX

fixbolt reads the same kind of configuration file QuickFIX does — one `[DEFAULT]` block, one
`[SESSION]` block per counterparty, QuickFIX's key names — and the same XML dictionary format. This
page takes a working QuickFIX setup across: the file, key by key, then the data dictionary.

It is not a port: fixbolt shares no code with QuickFIX
([ADR-0001](../decisions/ADR-0001-relationship-to-quickfix.md)), and where the two differ below,
it is on purpose.

## 1. The file is read strictly

Three differences meet you before any single key does:

- **An unknown key stops startup**, with its line number and the text written there
  ([ADR-0040](../decisions/ADR-0040-a-configuration-file-refuses-what-it-does-not-understand.md)).
  QuickFIX C++ ignores a key it does not recognise, in silence. Expect your first start to name
  every QuickFIX key this engine does not have; the table below says what each becomes.
- **`Y` and `N`, nothing else.** `true`, `yes` and `1` are refused with their line.
- **An integer is read exactly as written**: no leading `+`, no leading zero. `HeartBtInt=030` loads
  in QuickFIX C++ and is refused here.

[CONFIGURATION.md §1](../CONFIGURATION.md#1-configuration-file-keys) is the full list, with every
default.

## 2. Key by key

The same name, the same meaning — change nothing:

| QuickFIX key | Notes |
|---|---|
| `BeginString`, `SenderCompID`, `TargetCompID` | each at most 32 bytes; a longer value is refused, not truncated |
| `DefaultApplVerID` | required when `BeginString=FIXT.1.1`, refused otherwise |
| `ConnectionType` | `acceptor` or `initiator`, in `[DEFAULT]` only: one file configures one role |
| `HeartBtInt` | `0` means no heartbeats |
| `StartTime`, `EndTime`, `StartDay`, `EndDay`, `Weekdays` | **UTC only** — see *TimeZone* below |
| `SocketConnectHost`, `SocketConnectPort`, `ReconnectInterval` | initiator only; an initiator file has exactly one `[SESSION]` |
| `ResetOnLogon`, `ResetOnLogout`, `ResetOnDisconnect` | |
| `LogonTimeout`, `LogoutTimeout` | seconds; `0` is off |
| `AllowUnknownMsgFields` | |
| `ValidateUserDefinedFields` | a range: tags at or above 5000 — see [Add a custom tag](add-a-custom-tag.md) |
| `SendNextExpectedMsgSeqNum` | QuickFIX C++'s spelling; QuickFIX/J's `EnableNextExpectedMsgSeqNum` is an unknown key |
| `EnableLastMsgSeqNumProcessed` | QuickFIX/J's spelling; QuickFIX C++ has no such setting |
| `SocketUseSSL`, `ServerCertificateFile`, `ServerCertificateKeyFile` | acceptor TLS, `[DEFAULT]` only, Linux, `tls` feature |
| `CertificationAuthoritiesFile`, `ClientCertificateFile`, `ClientCertificateKeyFile` | initiator TLS, `[DEFAULT]` only; the key must be in its own file |

A different key, or a different shape:

| QuickFIX key | Here |
|---|---|
| `SocketAcceptPort` | Not a key. The address is the first argument of the door you call: `serve(&addr, …)` |
| `MaxLatency` (seconds), `CheckLatency` | `MaxSkewMillis`, in **milliseconds**, default `120000`. `CheckLatency` is an unknown key |
| `MillisecondsInTimeStamp`, `TimestampPrecision` | `TimestampPrecision`: `3`, `6` or `9` fractional digits on outbound `52=` |
| `FileLogPath` | Same name, but a **file**, not a directory: one engine writes one log, `[DEFAULT]` only, with each line naming its connection |
| `FileStorePath`, `PersistMessages` | Not keys. The journal is chosen in code — a `FileJournal` and a `Recovery` ([GUIDE.md §6](../GUIDE.md#6-journalling-pick-the-policy-deliberately)) |
| `TimeZone`, `UseLocalTime` | Not keys. Schedules are UTC; your code resolves the offset ([GUIDE.md §5a](../GUIDE.md#5a-session-schedules-and-the-timezone-trap)) |
| `CertificateVerifyLevel` | Not offered: an initiator always verifies, against `CertificationAuthoritiesFile` only |

New here, with no QuickFIX key: `ReconnectCeiling` (the largest backoff delay; without one the
ladder doubles for ever), `TlsRequireKernel`, and `ListenerEveryTurns` (`hft` mode).

Anything not in these tables is an unknown key: delete the line, or find what it did in
[CONFIGURATION.md](../CONFIGURATION.md) and [GUIDE.md §9](../GUIDE.md#9-what-this-engine-does-not-do-for-you).

## 3. The two keys that are refused on purpose

**`ValidateFieldsOutOfOrder` is not supported.** QuickFIX uses it to switch off `373=14`, *tag
specified out of required order*. Here the header-before-body order is one comparison inside the
single scan that checks everything else; there is no separate pass to skip, and the key is an
unknown key. [CONFIGURATION.md §1](../CONFIGURATION.md#1-configuration-file-keys) explains it in
full.

**`UseDataDictionary`, `DataDictionary`, `TransportDataDictionary` and `AppDataDictionary` are
refused by name.** Whatever the value — `UseDataDictionary=N` included — and in `[DEFAULT]` or
`[SESSION]`, the file stops startup with its line and this sentence:

```text
the dictionary is chosen when the application is built, not in this file — see docs/how-to/use-a-venue-dictionary.md
```

The dictionary is a Rust type compiled into your application, never a file read at run time, so a
configuration file has nothing to choose
([ADR-0207](../decisions/ADR-0207-a-custom-dictionary-is-an-overlay-generated-in-the-users-build-into-the-users-own-type.md)
decision 7). Delete the lines. If they pointed at QuickFIX's stock `FIX44.xml`, you are done:
`Fix44`, the default, is generated from that file, shipped byte-identical to QuickFIX's at a pinned
commit ([ADR-0104](../decisions/ADR-0104-the-published-dictionary-is-quickfixs-xml-shipped-with-a-notice.md)).
If they pointed at a customised copy, carry it across as the next section says.

## 4. Your DataDictionary XML

fixbolt's generator reads QuickFIX-format XML, so your customised FIX 4.4 file is already the input.
You choose how to hand it over (both shapes are in
[Use a venue dictionary](use-a-venue-dictionary.md#1-an-overlay-or-a-whole-file)):

**As a whole file — the least work.** Pass the file, unchanged, as `Source::Fix44Whole` in your
`build.rs`. It is read as written, retypes and removals included. One check may stop the build that
QuickFIX never made at load time: every `<message>` must have its `msgtype` among field 35's
`<value>`s. A file QuickFIX has been running on already lists them — QuickFIX needs the same value,
or it rejects every such message with `373=5` on tag 35 — so this passes. If it does not, the
message named in the error was being rejected on your wire, and the fix is the `<value>` it names.

**As an overlay — the smaller file.** Keep only what your copy adds to FIX 4.4, and pass it as
`Source::Fix44Overlay`; it is merged onto the `FIX44.xml` fixbolt ships:

1. Start from an empty `<fix type='FIX' major='4' minor='4' servicepack='0'>`, with only the
   sections you need: `<header>`, `<messages>`, `<components>`, `<fields>`.
2. Copy each **new** `<field>` definition whole.
3. For a standard field you added `<value>`s to, repeat the field with its own number, name and
   type, and only the new values.
4. For a standard message or component you added fields to, repeat it with its own `name` and
   `msgtype`, and only the added children.
5. Copy each **new** `<message>` and `<component>` whole. Its `<value>` on field 35 may stay or go:
   the merge adds it either way.

An overlay can only add. If your copy **changes** FIX 4.4 — retypes a field, removes one, turns an
open field such as `Symbol` into a list, edits the trailer — the build refuses it, naming both
sides; that dictionary is a whole file. If your copy started from a different QuickFIX release than
the one fixbolt pins, the same refusal names any standard definition the two disagree on.

Then follow [Use a venue dictionary](use-a-venue-dictionary.md) from step 2: the dependencies, the
`build.rs`, the `_over` door. Three things work differently from QuickFIX:

- **One dictionary per engine, not per session.** QuickFIX points each `[SESSION]` at its own
  file. Here, every counterparty an engine serves speaks that engine's dictionary; two dialects are
  two types and two engines.
- **A change to the XML is a rebuild**, not a restart.
- **FIX 4.4 only.** A customised FIXT 1.1 or FIX 5.0 SP2 pair cannot be an overlay or a whole file
  yet (ADR-0207 decision 8).
