# A Linux-only module is invisible to a gate run on a Mac

**The trap.** `mod shard`, `mod affinity` and the kTLS half of `mod tls` are
`#[cfg(target_os = "linux")]`. On macOS they are not compiled at all, so every `cargo check`,
`clippy` or `cargo doc` run on the development laptop is green about them — not because they are
correct but because they are absent. A gate that "passed on the desk" says nothing about the code
the production target actually builds.

## What it cost

`[measured 2026-09-13]` plan `2026-09-13-what-the-residue-review-found`, items 78 and 79:

- A `cargo doc` powerset run on the Mac found **14** intra-doc links broken without `standard`.
  They were fixed and the fix read `EXIT=0` for every set the Mac could build.
- The same job's first run on Linux (CI run `34750085195`) went red on **4 more** — in `lib.rs`
  and `tls.rs`, inside modules the Mac had never compiled. Clippy was green on every set; only
  rustdoc saw them.
- The unused import of item 78 (`crates/engine/src/shard.rs:43`) had the same shape: `cargo check
  --no-default-features --features affinity` is clean on macOS and warns on Linux.

## What works on the desk

A Linux **type-check, lint and rustdoc** runs on Apple silicon with the Linux target installed
(`rustup target add x86_64-unknown-linux-gnu`). The one obstacle is `ring`, pulled in by `rustls`
and the dev-dependency `rcgen`: its build script needs a C cross-compiler. `clang` is one, given
ring's own switch for building without a target libc:

```sh
export CC_x86_64_unknown_linux_gnu=clang
export CFLAGS_x86_64_unknown_linux_gnu="--target=x86_64-unknown-linux-gnu -DRING_CORE_NOSTDLIBINC=1 -ffreestanding"
export AR_x86_64_unknown_linux_gnu=ar
RUSTDOCFLAGS="-D warnings" \
  cargo hack doc --workspace --no-deps --feature-powerset --depth 2 --keep-going \
  --target x86_64-unknown-linux-gnu
```

`[measured 2026-09-13]` that command read `EXIT=0, 32 sets, 0 errors` on the tree CI then passed.
**It builds nothing that links or runs** — use it for `check`, `clippy` and `doc` only.

A Linux **test run** needs a Linux kernel. A container does it (`orbctl start`, then
`docker run --rm -v "$PWD":/w -w /w rust:1 cargo test -p fixbolt-engine --features affinity --test
shard_hft`): `[measured 2026-09-13]` that is how reversal R75-1 was first seen red
(`got Err(Io(Os { code: 98, kind: AddrInUse, … }))`) before any CI run could show it.

## A second surprise in the same gate

`cargo hack --feature-powerset --depth 2` counts `default` as a feature. With three real features
it built 10 sets for `fixbolt-engine`, two of them `default,X` duplicates, and **never all three
together** — so it did not contain `--all-features`, which the job it replaced had run. Recorded in
ADR-0065's revision; the job now runs `--all-features` as its own step.

## Regression test

The `feature-sets` CI job itself (`.github/workflows/ci.yml`, DESIGN.md §6): it runs on Linux, so
the modules above are compiled, linted and documented for every set to depth two and for
`--all-features`.

## The transferable half

A green result is about the configuration that was built. Before trusting a desk run, ask which
`cfg` branches the desk's target never enters — and either build for the real target or say that
half is unobserved.
