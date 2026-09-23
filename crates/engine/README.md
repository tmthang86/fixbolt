# fixbolt-engine

The TCP acceptor and connector that drives [`fixbolt-session`](https://crates.io/crates/fixbolt-session)
state machines on an engine thread, with the journal and the message log. Two modes: `standard`
blocks in `poll(2)` when idle and gives the core back; `hft` spins and never sleeps in the
kernel on the hot path. Part of [fixbolt](https://github.com/tmthang86/fixbolt/blob/main/README.md).

```sh
cargo add fixbolt-engine
```

Most applications need only [`fixbolt`](https://crates.io/crates/fixbolt), which re-exports what an
application uses from this crate. Start from the [repository README](https://github.com/tmthang86/fixbolt/blob/main/README.md) and
[docs/GETTING-STARTED.md](https://github.com/tmthang86/fixbolt/blob/main/docs/GETTING-STARTED.md).

## Features

| Feature | Default | What it adds |
|---|---|---|
| `standard` | on | the blocking mode and `serve`; takes `libc` |
| `affinity` | off | core pinning (Linux); takes `libc` |
| `tls` | off | TLS through `rustls` and kernel TLS; takes `rustls`, `ktls-core`, `libc` |
| `fix50sp2` | off | FIXT 1.1 / FIX 5.0 SP2 sessions |

With `default-features = false` the crate has no dependency outside fixbolt and no `unsafe`:
`hft` mode only.

## Licence

Dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option.
