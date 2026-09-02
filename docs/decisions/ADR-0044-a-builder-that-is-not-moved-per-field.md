# ADR-0044 — A builder that is not moved once per field

- **Status**: Accepted — 2026-09-02
- **Date**: 2026-09-02
- **Deciders**: Tran Manh Thang
- **Related**: [ADR-0041](ADR-0041-the-library-layer-buys-an-api-with-a-template-per-message.md) —
  which published the cost this halves · [ADR-0016](ADR-0016-per-machine-baselines-replace-absolute-targets.md)
  — why the figures below name their machine · [DESIGN.md D9](../DESIGN.md) — the template
  encoder these numbers are measured against

## Context

`STATUS.md` item 34. `[measured 2026-09-02]` a twelve-field reply through
`fixbolt::App::on_message` cost **~1 594 ns** against **40 ns** for encoding a `Template` built
once. ADR-0041 published that ratio rather than hiding it, and named the cause: *building a
`Template` per message*, with `P` and `S` both on the clock because
`TemplateBuilder::field` **took `self` by value**.

An `S`-byte struct is therefore copied once per field. With `S = 1024` and a dozen fields that
is kilobytes of memcpy to add a handful of bytes — and `fixbolt::Message` made it worse, holding
the builder by value and taking `self` by value itself, so each `.field()` moved the builder
**out of** the message, into the call, back out, and back **in**.

## Decision

**`TemplateBuilder::field`, `slot` and `group` take `&mut self` and return `&mut Self`. `build`
takes `&mut self`.**

One API, not two. Adding `push_*` methods beside the chaining ones was tried first and rejected:
two ways to add a field is exactly the shape `CLAUDE.md` §6 warns about, and there is no
behavioural difference to justify it — the by-value form was never *better*, only older.

**The chaining reads the same at a call site that starts from a temporary**, which is most of
them:

```rust
TemplateBuilder::<24, 320>::new(begin)
    .field(tag::MSG_TYPE, b"A")
    .slot(tag::MSG_SEQ_NUM)
    .build::<Fix44>()
```

Rust auto-refs the temporary, so `crates/session/src/out.rs` — seven templates, ~70 chained
calls — **did not change at all**. Four call sites that bound the chain to a variable and kept
adding to it did, mechanically: bind first, then mutate.

**`fixbolt::Message::field`, `group`, `send` and `send_with_groups` follow**, which keeps the
handler-facing shape identical:

```rust
reply.message(b"8").field(37, order_id).field(39, b"0").send()
```

## What was measured

`[measured 2026-09-02, Intel(R) Xeon(R) Processor @ 2.80GHz, a shared 4-vCPU VM that does
**NOT** meet `DESIGN.md` §9]` — `crates/library/benches/cost.rs`, same commit, same machine,
same run shape:

| | before | after | |
|---|---|---|---|
| `library, parse only` | 143.8 ns | 146.2 ns | unchanged, as expected — it touches no builder |
| `library, reply only` | **1 549.0 ns** | **766.0 ns** | **−51%** |
| `library, on_message` | **1 594.5 ns** | **955.6 ns** | **−40%** |

The parse row is the control. It shares the machine and the run and moves by 1.7%, which is
inside this box's own ±3–4% noise — so the other two rows are not the machine having a good day.

**The ratio ADR-0041 published moves from ~50× the 40 ns template path to ~24×.** It does not
reach it, and the rest is named below.

## Consequences

### Good

- **Half the cost of the convenience layer, for no ergonomic change.** A handler reads
  identically; `out.rs` is byte-for-byte the same file.
- **Two other per-message template builds get it for free**, and neither is the library:
  `fixbolt_session`'s resend path (`S = 1024`, rebuilt per resent message) and
  `fixbolt_conformance`'s echo (`S = 4096`). Neither was measured before or after — they are
  named here so nobody reads the library figures as the whole benefit.
- **`build` can now be called twice**, which the by-value signature made unaskable.
  `crates/codec/tests/slot_order.rs::building_twice_gives_the_same_template` answers it rather
  than arguing that sorting a sorted list is idempotent.

### Bad, and accepted

- **A breaking change to two public APIs**, `fixbolt_codec::TemplateBuilder` and
  `fixbolt::Message`. Nothing is published, so the cost is this repository's four call sites and
  five test helpers, all mechanical. `CHANGELOG.md` records it.
- **`&mut self` is easier to misuse than a consuming builder.** A value-consuming `build` made
  "use the builder afterwards" a compile error; now it compiles. The idempotence test is what
  stands in for the type system, and it is weaker.
- **~24× is still not 1×.** What remains is the thing ADR-0041 actually named: a `Template` is
  **materialised per message** — sorted, and its scratch laid out — where D9's shape builds it
  once. Removing that needs `codec` to encode straight out of a builder without producing a
  `Template<P, S>` value at all, or a builder reused across messages with a `clear`. Neither is
  in this ADR, both are now cheaper to reach, and **item 34 stays open** with 766 ns as the
  number to beat rather than 1 549.
- **The figures are from a machine that fails §9.** The ratio transfers; the absolutes do not,
  and they are deliberately **not** recorded in `benches/baselines.tsv`.
