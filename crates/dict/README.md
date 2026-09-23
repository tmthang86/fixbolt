# fixbolt-dict

The FIX 4.4 tables [`fixbolt-codec`](https://crates.io/crates/fixbolt-codec) and
[`fixbolt-session`](https://crates.io/crates/fixbolt-session) read — tag constants, message
shapes, required fields, field order, repeating groups and validation tables — generated at
build time from the FIX XML dictionaries shipped inside this crate under `spec/`. The build
needs no network and nothing outside the crate. Part of [fixbolt](https://github.com/tmthang86/fixbolt/blob/main/README.md).

```sh
cargo add fixbolt-dict
```

Most applications need only [`fixbolt`](https://crates.io/crates/fixbolt), which re-exports what an
application uses from this crate. Start from the [repository README](https://github.com/tmthang86/fixbolt/blob/main/README.md) and
[docs/GETTING-STARTED.md](https://github.com/tmthang86/fixbolt/blob/main/docs/GETTING-STARTED.md).

## Features

| Feature | Default | What it adds |
|---|---|---|
| `fix50sp2` | off | a second table, `Fixt11Fix50Sp2Tables`, for FIXT 1.1 / FIX 5.0 SP2 |

## Licence

`(MIT OR Apache-2.0) AND LicenseRef-QuickFIX-1.0`. fixbolt's own code is dual-licensed under
[MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option. The three XML files under
`spec/`, and the tables generated from them, come from QuickFIX's data dictionaries and are
distributed under the QuickFIX Software License 1.0, reproduced in [NOTICE](NOTICE). Its clause 2
asks a binary redistribution to reproduce that notice.
