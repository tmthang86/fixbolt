# An encoding that ignores a const parameter makes every caller name it

`[found 2026-09-19, step C4 of the phase 2 plan]` `codec::Encoding::Template<const P: usize,
const S: usize>` carries the prefix and slot capacities of tag=value's `Template<P, S>`. SBE has
no prefix, so `Sbe<S>`'s template (`SbeTemplate<S, N>`) takes only `N` and ignores `P`. Rust
cannot infer a const parameter that appears nowhere in the argument types, so every call
through the trait has to spell it out:

```rust
E::encode::<0, 64>(&template, &mut out, &slots)
```

Writing `E::encode(&template, …)` fails with `type annotations needed` on `P`. This is a cost
of one trait shape over two encodings (ADR-0079 decision 2), not a defect of either impl.

**Rule.** Code generic over `E: Encoding` names both const arguments at the call site. `0`
is the value to pass for an encoding that ignores `P`.

Guarded by: `crates/sbe/tests/encoding.rs`, `crates/sbe-gen/tests/encoding.rs` (both call
`E::encode::<0, _>`).
