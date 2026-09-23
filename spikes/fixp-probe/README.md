# `fixp-probe` spike — does our SBE stack speak B3 Binary EntryPoint to Artio?

This answers one question and then stops. It is **not** a FIXP session: one straight-line
program, no state type, no retransmission, no timer beyond a read deadline. Nothing in `crates/`
or `tools/` depends on it — see [the plan](../../docs/plans/2026-09-23-p3-fixp-spike.md) and
[ADR-0140](../../docs/decisions/ADR-0140-the-fixp-spike-speaks-the-referees-own-schema-through-a-detached-probe-and-its-ci-job-blocks.md).

- **`src/main.rs`**: the probe. It encodes with `fixbolt-sbe`'s `MessageWriter` and decodes with
  its `SbeView`, over tables `build.rs` generates with `fixbolt-sbe-gen` from the schema inside
  Artio's pinned `artio-binary-entrypoint-codecs-0.184.jar`. It writes B3's 4-byte framing
  header itself (`u16` little-endian length including the header, then `0xEB50`).
- **`referee/Referee.java`**: the referee, our own code on Artio's public API — Artio's Binary
  EntryPoint acceptor behind an authentication strategy that compares all seven Negotiate fields
  with expected values and refuses on a mismatch, and a wire tap in front of it that decodes every
  frame the probe sends with Real Logic's generated decoders and judges every field, the header's
  `blockLength` and the frame length. The test values fill their declared widths (no zero byte),
  so a field encoded too narrow or too wide cannot read back right by accident.

## Running it

```sh
scripts/fixp-spike.sh      # from the repository root; needs a JDK >= 17, curl, python3, cargo
```

That pins and checks 11 jars and the schema, compiles the referee, builds this crate, and runs
three arms: `accept` (Negotiate, Establish, Terminate and back, five steps), `reject-timestamp`
(Artio refuses an hour-old Negotiate with `INVALID_TIMESTAMP`) and `reject-credentials` (our
referee refuses a wrong `credentials`). Only the printed lines decide; the summary
`fixp-spike: accept PASS 5/5, reject-timestamp PASS, reject-credentials PASS` appears only when
all three pass. `FIXP_SPIKE_ARMS` selects arms; `referee-only` starts the referee with no client.

## Why it is outside the workspace

The root `Cargo.toml` excludes it and its own manifest has an empty `[workspace]` table. Its
`build.rs` reads `vendor/fixp/binary_entrypoint.xml`, which is B3's, is fetched by the script, and
is **never committed** (ADR-0140 decision 3) — inside the workspace, `cargo test --all` would fail
on every checkout that has not run the script. The workspace lint table is copied into
`Cargo.toml`, because a detached crate does not inherit it; check it with
`cargo clippy --manifest-path spikes/fixp-probe/Cargo.toml --all-targets -- -D warnings`.

**Its `Cargo.lock` is committed**, like `spikes/ktls`'s, and the script builds `--locked`: the
answer is about this code against Artio 0.184, and a re-run should not resolve anything new.
