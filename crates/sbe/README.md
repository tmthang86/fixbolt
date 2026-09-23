# fixbolt-sbe

SBE 1.0 (Simple Binary Encoding) decode and encode in place, over the `&'static` tables a
schema supplies: message header, root block, repeating groups, `varData`. No allocation,
`#![no_std]`, `#![forbid(unsafe_code)]`. Part of [fixbolt](https://github.com/tmthang86/fixbolt/blob/main/README.md); the tables are
generated from a schema XML by `fixbolt-sbe-gen`, which is not published on crates.io — depend
on it by git from the repository, at the release tag.

```sh
cargo add fixbolt-sbe
```

Through `fixbolt`, this crate is the `sbe` feature. Start from the
[repository README](https://github.com/tmthang86/fixbolt/blob/main/README.md).

## Features

| Feature | Default | What it adds |
|---|---|---|
| `encoding` | on | `Sbe<S>: fixbolt_codec::Encoding` and the writer; takes `fixbolt-codec`. Off, the crate has no dependency at all |

## Licence

Dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option.
