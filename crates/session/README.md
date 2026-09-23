# fixbolt-session

The FIX 4.4 session state machine: logon, heartbeats, sequence numbers, resend and gap fill,
rejects, logout. **Pure** — no socket, no clock, no allocation; time arrives as an input. Part of [fixbolt](https://github.com/tmthang86/fixbolt/blob/main/README.md); the TCP side
is [`fixbolt-engine`](https://crates.io/crates/fixbolt-engine).

```sh
cargo add fixbolt-session
```

Most applications need only [`fixbolt`](https://crates.io/crates/fixbolt), which re-exports what an
application uses from this crate. Start from the [repository README](https://github.com/tmthang86/fixbolt/blob/main/README.md) and
[docs/GETTING-STARTED.md](https://github.com/tmthang86/fixbolt/blob/main/docs/GETTING-STARTED.md).

## Features

| Feature | Default | What it adds |
|---|---|---|
| `fix50sp2` | off | FIXT 1.1 / FIX 5.0 SP2 sessions, through `fixbolt-dict`'s second table |

## Licence

Dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option.
