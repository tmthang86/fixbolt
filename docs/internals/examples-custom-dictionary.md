# `examples/custom-dictionary` — internals

Package `fixbolt-example-custom-dictionary`, `publish = false`: a FIX 4.4 acceptor over a venue
dialect generated in its own `build.rs`, and the one committed crate that compiles a file
`fixbolt_dict::codegen::generate` wrote
([ADR-0207](../decisions/ADR-0207-a-custom-dictionary-is-an-overlay-generated-in-the-users-build-into-the-users-own-type.md)
decisions 3–6). It depends on `fixbolt` alone at run time and on `fixbolt-dict` with `codegen` as a
build-dependency, which is the shape an embedder copies —
[Use a venue dictionary](../how-to/use-a-venue-dictionary.md) quotes it by `sample:` region, so a
change to a region is a change to that page (`scripts/check-doc-samples.sh`). `standard` is
forwarded rather than inherited from `fixbolt`'s defaults, for the reason its `Cargo.toml` gives
([feature-flags-unify-across-a-workspace](../reference/feature-flags-unify-across-a-workspace.md)).

## Files, and what each keeps

| File | Keeps |
|---|---|
| `venue.xml` | The overlay: **invented**, never a venue's. A header field (5002), a required body field (5001), a group declared amount first (5003–5005), a value added to `OrdType (40)`, and two message types — `U1` left off field 35, `U2` listed on it, so both habits of the MsgType rule run |
| `build.rs` | Generates `Venue` from `venue.xml` into `$OUT_DIR/venue.rs` with `Paths::facade()`, printing `table_size()` as a `cargo:warning`; and `Plain`, from an empty overlay, into `$OUT_DIR/plain.rs`, for `tests/plain_is_fix44.rs` only |
| `src/lib.rs` | `mod venue` (the `include!`), the tag constants, and `Desk`: a `Handler<256, 64, 1024, Venue>` that fills an order, echoing the venue's fields and group, and answers `U1` with `U2` |
| `src/main.rs` | `serve_over` with `TagValue<Venue, 256>` and `App<Desk, 256, 64, 1024, Venue>` — the call the how-to quotes |

## Read in this order

1. `venue.xml` — what the dialect adds, and its header comment
2. `build.rs` — how it becomes a type
3. `src/lib.rs` — the type included, and a handler over it
4. `src/main.rs` — the door, and the dictionary named twice

## Tests that guard it

- `tests/venue.rs` — each addition over a real socket (custom tag, enum value, group in declared
  order, header field, message type, required field, undefined tag still `373=0`, undefined user
  tag under `ValidateUserDefinedFields=N`, and a defined one left unchecked under it —
  `a_defined_user_tag_is_not_type_checked_when_user_defined_fields_are_skipped`, today's behaviour
  pinned), and
  `a_venue_app_behind_a_fix44_door_compiles_and_the_session_rejects_the_venue_tag_373_0`, the
  proof that the `App`'s dictionary and the door's are not tied by the types (`GUIDE.md` §3a)
- `tests/plain_is_fix44.rs` — `an_empty_overlay_type_answers_every_dictionary_question_as_fix44`:
  `Plain` answers all fourteen `Dictionary` and `Tables` functions as `Fix44`, which carries the
  59 definitions' result to a generated type (ADR-0207 *Consequences*)
- `benches/alloc.rs` — non-negotiable 1 over `Venue`: a reply carrying the venue's group, a silent
  answer and a business reject, each proven to take its path, with a control that must read
  non-zero; run by `scripts/bench.sh` in the `bench` job, not by `cargo test`
