# ADR-0079 — One view per encoding, and one trait over them

- **Status**: **Accepted — 2026-09-18**, by the plan [closing-the-open-items](../plans/2026-09-18-closing-the-open-items.md), approved by the manager under the owner's 2026-09-18 mandate. Proposed the same day.
- **Date**: 2026-09-18
- **Deciders**: Tran Manh Thang
- **Related**: `PRD.md` §2 *Phase 2 starts with an architectural decision*, §6 row 4;
  [ADR-0003](ADR-0003-message-representation.md) (`MessageView`, `FieldEntry { tag, offset, len }`);
  [ADR-0078](ADR-0078-sbe-enters-as-an-encoding-without-a-session-and-fixp-is-its-own-phase.md);
  `DESIGN.md` D2, D3, D9; `CLAUDE.md` §6 *Public API takes borrowed views*
- **Answers**: PRD §6 open decision 4 — *one view type or several, once encodings stop having
  tags on the wire? Blocks every phase-2 line, and possibly `codec`'s public API today*

## Context

`MessageView` is 24 bytes, `Copy`, and indexes fields by `(tag, offset, len)` — it presupposes
a tag on the wire. SBE has none: a field is at a schema-determined offset in a fixed block,
repeating groups are counted blocks, and variable-length data follows. A single view type that
served both would either carry a discriminant and a `match` on every field access (a branch on
the hot path for the tag=value users, who are all the users today) or be a trait object (a
vtable call and no inlining). The generated-code engines that carry both encodings do not
try: Artio generates separate FIX and iLink 3 (SBE) codecs and exposes them on separate
connection types; the SBE reference implementation generates *flyweights* — per-message
structs over a buffer with offset arithmetic, one type per message — and never a generic view.

What must be shared is not the view but the **shape of access**: parse a frame into a view
without copying (D2), read a field by a generated identifier (D3), patch an outbound template
(D9), and hand the session a small `Copy` value. All four are satisfiable per encoding.

## Decision

1. **Several view types, one per encoding.** `MessageView` stays exactly what it is — the
   tag=value view, its name, size and API unchanged; **`codec`'s public API is not blocked by
   phase 2** and nothing in it is renamed. SBE gets `SbeView` (a buffer, a schema id, a template
   id and a block offset; `Copy`; sized to stay ≤ 24 bytes), generated per schema.
2. **One trait over them, `Encoding`, with associated types**: `type View<'a>: Copy`,
   `type Field` (the generated identifier — a tag for FIX, a field id for SBE), `type Template`
   (the D9 parts list), and the four operations above. Static dispatch only: the session,
   the engine and the application are generic over `E: Encoding`, chosen at compile time per
   engine (and per `Logon` by the registry only in the sense that the registry picks the
   dictionary; the encoding of a listener is fixed). No `dyn Encoding` anywhere on a path a
   message takes.
3. **The session layer is generic over `Encoding` and stays pure.** It needs from a view only
   the session fields (message type, sequence number, sender/target, sending time); the trait
   exposes those as methods on `View`, so `Session` reads them through the trait and never
   touches encoding-specific bytes. The 59 definitions run against `Session<Fix44TagValue>`
   exactly as today, which is what proves the generalisation did not move the session.
4. **Repeating groups, dictionaries and validation are per encoding**, behind the same trait
   surface: the tag=value dictionary pass (item 39) stays as is; SBE's validation is the
   schema's (types and presence are structural), so its pass is close to free and says so in
   its bench.
5. **The guard that this costs the tag=value path nothing**: `benches/parse.rs`,
   `benches/serialize.rs` and `benches/alloc.rs` must read inside the ADR-0031 band on the §9
   machine after the trait lands, on the same commit — the first step of phase 2's plan, before
   any SBE line is written. A band miss is a stop.

## Consequences

**Good**

- Every phase-2 line is unblocked and `codec`'s public API today is untouched, which was the
  row's second worry.
- Static dispatch keeps D2/D3/D9's cost model: what was inlined stays inlined.
- The session layer's generalisation is proven by the corpus it already has (59/59), not by a
  new oracle.

**Bad — and accepted**

- **Generic engines mean generic front doors.** Every `serve*` signature gains an `E`
  parameter or a type alias; `GUIDE.md`, `GETTING-STARTED.md` and `TUTORIAL.md` show
  `Fix44TagValue` spelled out or hidden behind aliases — a visible API change even for users
  who never touch SBE. It is the cost of not paying a branch per field.
- **Two views is two of everything downstream**: two template builders, two alloc benches,
  two sets of generated tables, two `Validation` shapes. The trait keeps them the same shape,
  not the same code.
- **Monomorphisation grows compile time and binary size** for a crate that builds in 5 s
  today; measured when it happens, not now.
- **`Copy` and ≤ 24 bytes for `SbeView` is a target**; if a schema needs more (deep group
  nesting), the view becomes a cursor and the rule bends for SBE only, in its own ADR.

## Sources

- ADR-0003; `crates/codec/src` (`MessageView`, `FieldEntry`), read 2026-09-18.
- Artio: separate FIX and iLink 3 codecs and connection types —
  <https://github.com/artiofix/artio> (`artio-codecs`, `artio-ilink3-codecs`);
  <https://deepwiki.com/artiofix/artio> (read 2026-09-18).
- Real Logic SBE: generated flyweights per message —
  <https://github.com/real-logic/simple-binary-encoding> (read 2026-09-18).
- `PRD.md` §2 *Phase 2 starts with an architectural decision* (the table of what each
  encoding drags with it).
