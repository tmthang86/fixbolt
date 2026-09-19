# `library` — internals

Layer L4 in [DESIGN.md §3](../DESIGN.md#3-crates), the package **`fixbolt`**: the
application-facing API — `Handler`, `Incoming`, `Reply`, `App` — and a curated re-export of
what an application needs from `engine`, `session` and `codec`. What is deliberately absent
(`Engine`, `Dispatch`, `Transport`, `wait`, `shard`, `affinity`, `frame`, `ring`) is named in
the same `DESIGN.md §3` row; reaching for one of those means depending on `fixbolt-engine`
directly, not a gap in this crate.

## Files, and what each keeps

| File | Keeps |
|---|---|
| `lib.rs` | No logic of its own — the curated `pub use` list from `fixbolt_codec`, `fixbolt_session`, `fixbolt_engine`, plus `mod app; mod reply;` |
| `app.rs` | `Handler` — what an application implements — and `App`, the adapter that makes one look like `fixbolt_session::Application` |
| `reply.rs` | `Reply`, `Answer`, `Message`, `ReplyError` — the answer a handler writes, and the seven header/frame fields it never writes itself |

## Read in this order

1. `lib.rs` — the surface area, read as a list of what is exported and what is withheld
2. `app.rs` — why the adapter exists: the session hands raw bytes and a `Range<usize>`, and
   `App` is what turns that into something an application writes against
3. `reply.rs` — the other half of the adapter, on the way out

## Tests that guard it

- `tests/end_to_end.rs` — a full `Handler` driven through `App` and back
- `tests/reply.rs` — `reply.rs`, including the fields a handler is refused from writing
- `examples/acceptor.rs` (+ `examples/acceptor.cfg`, `examples/shared/`) — the worked example
  `docs/TUTORIAL.md` builds around
- `benches/alloc.rs`, `benches/cost.rs` — non-negotiable 1, and the adapter's own overhead
