# `gen` is a reserved keyword in Rust 2024, so a module cannot be called `gen`

`[measured 2026-09-26]`

## The trap

[ADR-0207](../decisions/ADR-0207-a-custom-dictionary-is-an-overlay-generated-in-the-users-build-into-the-users-own-type.md)
decision 1, as first accepted, named the dictionary generator's module `pub mod gen`, behind a
feature `gen`. The workspace is `edition = "2024"`, where `gen` is reserved for generator
blocks:

```
$ rustc --edition 2024 kw.rs      # pub mod gen { pub fn f() {} }
error: expected identifier, found reserved keyword `gen`
 --> kw.rs:1:9
  |
1 | pub mod gen { pub fn f() {} }
  |         ^^^ expected identifier, found reserved keyword
```

`pub mod r#gen;` compiles, and the item is still named `gen`. But whether a caller can write
`gen` depends on the **caller's** edition, because the keyword is a property of the token in the
crate that writes it: a 2021 crate writes `fixbolt_dict::gen::…`, a 2024 crate must write
`fixbolt_dict::r#gen::…`. For a module whose whole audience is a user's `build.rs`, that is a
wart in every example and how-to, and a surprise the day a user moves to 2024.

A Cargo **feature** named `gen` is fine: feature names are strings, not identifiers.

## What was decided

Step 17 of plan `2026-09-26-docs-for-embedders` found it while laying the module's skeleton. The
manager renamed both module and feature to **`codegen`** (`fixbolt_dict::codegen`, feature
`codegen`), and ADR-0207 was updated to that name before any code depended on the old one. No
`r#` appears anywhere.

## Regression test

The compiler. Renaming the module back to `gen` fails every build that enables the feature, and
the `gates` CI job's `codegen` step builds, lints and tests `-p fixbolt-dict` with `codegen` and
with `codegen,fix50sp2` on every commit. Nothing else is needed: the failure cannot be silent.

## The transferable half

An edition can reserve a word that was an ordinary identifier the edition before. Before an ADR
fixes a public identifier, compile it under the workspace's edition — one `rustc --edition` line
settles it.
