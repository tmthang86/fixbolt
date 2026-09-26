# API reference

The API reference is the rustdoc of the `fixbolt` crate, and you generate it yourself. fixbolt is
released as git tags rather than on crates.io, so there is no docs.rs page
([ADR-0161](decisions/ADR-0161-0-1-0-is-a-git-tag-not-a-crates-io-upload-and-the-stranger-and-the-semver-gate-read-the-tag.md)),
and this site does not host rustdoc either.

## Generate it

In your own project, once `fixbolt` is a dependency ([Getting started](GETTING-STARTED.md)):

```sh
cargo doc --open -p fixbolt
```

The same command works from a checkout of this repository. The pages show the version your
`Cargo.lock` resolved, with the features you enabled: an item behind a feature that is off, such
as `fixbolt::sbe` behind `sbe`, is not on them. Which feature enables what is
[CONFIGURATION.md §4](CONFIGURATION.md).

## Where to begin reading

`fixbolt` is a facade: it defines `Handler`, `App`, `Incoming` and `Reply`, and re-exports from
the lower crates only what an application needs. Its crate documentation,
[crates/library/README.md](../crates/library/README.md), explains that choice. The entry points,
in the order an acceptor meets them:

| Item | What it is | Defined in |
|---|---|---|
| `Settings` | A configuration file in QuickFIX's shape that refuses any key it does not understand ([CONFIGURATION.md §1](CONFIGURATION.md)) | [crates/engine/src/settings.rs](../crates/engine/src/settings.rs) |
| `Table`, `Registry`, `Limits` | The counterparties the engine will admit, known before any of them logs on | [crates/engine/src/presession.rs](../crates/engine/src/presession.rs) |
| `Handler` | What your application implements: `on_message`, and optionally `on_logon` to speak first. It runs on the engine thread ([GUIDE.md §2](GUIDE.md)) | [crates/library/src/app.rs](../crates/library/src/app.rs) |
| `Incoming` | The message handed to a handler: already parsed, borrowed from the engine's read buffer | [crates/library/src/app.rs](../crates/library/src/app.rs) |
| `Reply`, `Answer` | How a handler answers, or declines to. `Reply` writes `8`, `9`, `10`, `34`, `49`, `52` and `56` itself and orders fields from the generated tables | [crates/library/src/reply.rs](../crates/library/src/reply.rs) |
| `app`, `App` | Wraps a `Handler` into the `Application` the engine drives | [crates/library/src/app.rs](../crates/library/src/app.rs) |
| `serve`, `serve_hft` | Start an acceptor in `standard` or `hft` mode; the `_with` and `_with_recovery` forms name buffer sizes and recover from a journal | [crates/engine/src/lib.rs](../crates/engine/src/lib.rs) |
| `connect_and_serve`, `reconnect` | Dial out as an initiator, and the rule for coming back after a connection ends ([GUIDE.md §8c](GUIDE.md)) | [crates/engine/src/lib.rs](../crates/engine/src/lib.rs) |
| `Shutdown`, `ServeError`, `DropReason` | What serving returned, and why a connection ended ([SESSION-BEHAVIOUR.md §1](SESSION-BEHAVIOUR.md)) | [crates/engine/src/lib.rs](../crates/engine/src/lib.rs), [crates/session/src/lib.rs](../crates/session/src/lib.rs) |
| `MessageView`, `as_i64`, `as_decimal`, `Decimal` | Reading fields and prices out of a message ([GUIDE.md §3c](GUIDE.md)) | [crates/codec/src/index.rs](../crates/codec/src/index.rs), [crates/codec/src/decimal.rs](../crates/codec/src/decimal.rs) |
| `Handles`, `Observer`, `Snapshot`, `Admin` | Watching and administering a running engine ([GUIDE.md §8a](GUIDE.md)) | [crates/engine/src/observe.rs](../crates/engine/src/observe.rs) |
| `FileJournal`, `FileLog` | The journal a session recovers from, and the message log ([GUIDE.md §6](GUIDE.md)) | [crates/engine/src/journal.rs](../crates/engine/src/journal.rs), [crates/engine/src/msglog.rs](../crates/engine/src/msglog.rs) |
| `NOTICE` | The QuickFIX Software License notice a distributed binary owes ([ADR-0104](decisions/ADR-0104-the-published-dictionary-is-quickfixs-xml-shipped-with-a-notice.md)) | [crates/dict/src/lib.rs](../crates/dict/src/lib.rs) |

The shortest complete program is
[crates/library/examples/acceptor.rs](../crates/library/examples/acceptor.rs), with
[acceptor.cfg](../crates/library/examples/acceptor.cfg) beside it.

## What the facade leaves out on purpose

`Engine`, `Dispatch`, `Transport`, the wait strategies, sharding, core affinity, framing and the
ring are not re-exported by `fixbolt`. Reaching one means adding `fixbolt-engine` to your own
manifest, and generating its documentation with `cargo doc --open -p fixbolt-engine`. The same
is true of the TLS entry points, which sit behind `fixbolt-engine`'s `tls` feature and on Linux
only, and of the sharded `hft` entry point ([GUIDE.md §1a](GUIDE.md)).

Which file in which crate holds what, crate by crate, is
[docs/internals/](internals/README.md).
