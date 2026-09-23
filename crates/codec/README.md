# fixbolt-codec

FIX 4.4 tag=value parse and serialise **in place**: a message is read as borrowed views into
the caller's buffer and written into a pre-built template, with no heap allocation on either
path. `#![no_std]`, no dependencies. Part of [fixbolt](https://github.com/tmthang86/fixbolt/blob/main/README.md), a FIX acceptor on
kernel TCP.

```sh
cargo add fixbolt-codec
```

Most applications need only [`fixbolt`](https://crates.io/crates/fixbolt), which re-exports what an
application uses from this crate. Start from the [repository README](https://github.com/tmthang86/fixbolt/blob/main/README.md) and
[docs/GETTING-STARTED.md](https://github.com/tmthang86/fixbolt/blob/main/docs/GETTING-STARTED.md).

## Features

None a user selects. The `fix50sp2` feature exists only for this crate's own benches and does
nothing in the published crate.

## Licence

Dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option.
