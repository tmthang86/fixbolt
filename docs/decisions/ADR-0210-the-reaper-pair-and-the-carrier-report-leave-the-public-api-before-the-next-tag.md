# ADR-0210 — The reaper pair and the carrier report leave the public API before the next tag

- **Status**: **Accepted — 2026-09-27**, by the manager under the owner's delegation of that day ("tôi ủy quyền cho bạn duyệt toàn bộ"); proposed the same day. Answers `STATUS.md` open item 115. Built by its own small
  pull request, described in the plan [2026-09-27-p4-simd](../plans/2026-09-27-p4-simd.md),
  section *Mục 115 — PR riêng*. It amends nothing in
  [ADR-0190](ADR-0190-the-io-uring-transport-is-reaped-by-the-idle-strategy-and-an-hft-turn-enters-the-kernel-once-without-waiting.md):
  that ADR's *Result* leaves the question open ("Whether these hooks stay is not decided here"),
  and this is the decision it left open.
- **Date**: 2026-09-27
- **Deciders**: Tran Manh Thang (by delegation to the manager, 2026-09-27). Written by the architect (Opus).
- **Related**: ADR-0190 decision 1 and *Result*;
  [ADR-0060](ADR-0060-a-deployment-that-requires-the-kernel-is-refused-twice.md) decision 3 (the
  defaulted-trait-item shape); [ADR-0014](ADR-0014-standard-mode-blocks-on-poll.md)
  decision 4 (the compile-time refusal shape); [ADR-0161](ADR-0161-0-1-0-is-a-git-tag-not-a-crates-io-upload-and-the-stranger-and-the-semver-gate-read-the-tag.md)
  and [ADR-0162](ADR-0162-the-semver-baseline-is-the-newest-release-tag-head-descends-from-and-zero-checks-are-excused-only-by-a-major-bump.md) (tags and
  the semver gate); [ADR-0204](ADR-0204-kernel-bypass-is-a-later-phase-candidate-that-reopens-on-a-named-machine.md).

## Context

Phase 4 row 5 added five public items to `fixbolt-engine` for the `io_uring` transport, outside
its feature, so that they compiled on every build:

| Item | Where | Who reads it today (`grep`, 2026-09-27) |
|---|---|---|
| `Transport::NEEDS_REAPER` (defaulted `false`) | `crates/engine/src/transport.rs:241` | nobody sets it `true`; the `const` block in `Engine::new` (`lib.rs:386-395`) and one `compile_fail,E0080` doctest with a transport of its own |
| `Waiting::REAPS` (defaulted `false`) | `crates/engine/src/wait.rs:56` | the same `const` block; no strategy sets it `true` |
| `Carrier` {`Kernel`, `Other`} | `transport.rs:176` | `tools/w2w/src/main.rs:486-499` (maps it to `kernel`/`other`) |
| `Transport::carrier` (defaulted `Other`) | `transport.rs:294` | overridden by `TcpTransport` (`:343`) and the TLS test transport (`tls.rs:1520`) to `Kernel` |
| `Engine::carrier(ConnId)` | `lib.rs:856` | `tools/w2w` only, once after logon; it prints `transport: kernel`, refuses to measure otherwise (`main.rs:2135`, `:2619`), and `scripts/w2w-baseline.sh:469` requires the line |

The transport they served failed its kill line and was removed on 2026-09-27. None of the five
is in the only tag, `v0.1.0` (`git grep NEEDS_REAPER v0.1.0` finds nothing); `CHANGELOG.md`
lists them under the next release, with the question open.

Three more facts:

- **`Carrier` cannot grow without a major break.** It is not `#[non_exhaustive]`, and `w2w`
  matches it exhaustively. The Cargo SemVer reference classes a new variant of such an enum as
  **major** ([`enum-variant-new`](https://doc.rust-lang.org/cargo/reference/semver.html#enum-variant-new)).
  As shipped, the "extension point" is closed.
- **`Carrier` cannot see the one bypass this project may reopen.** Onload works by `LD_PRELOAD`
  over the socket calls; under it `TcpTransport` still calls `read(2)` and would report `Kernel`.
  ADR-0204 reopens bypass only on a named machine and under its own plan.
- **`w2w` has one transport type.** Since the `--transport` flag is refused by name, every
  `w2w` binary builds a `TcpTransport` (optionally under TLS over it). The `transport: kernel`
  line reads back a value that has no other possible answer in that binary.

What semver says about timing: removing a public item is **major**
([`item-remove`](https://doc.rust-lang.org/cargo/reference/semver.html#item-remove)); for
`0.y.z`, a change in `y` is the major one. Adding a defaulted trait item is **possibly-breaking**,
normally shipped in a minor release
([`trait-new-default-item`](https://doc.rust-lang.org/cargo/reference/semver.html#trait-new-default-item)).
So removing now costs nothing, removing after the next tag costs a `0.(y+1)`, and adding the
items back later costs at most what adding them cost the first time. `cargo-semver-checks`
treats `#[doc(hidden)]` items as outside the public API since v0.25
([predr.ag](https://predr.ag/blog/checking-semver-for-doc-hidden-items/)), which is the
alternative considered below. The Rust API guidelines' future-proofing chapter
([C-SEALED](https://rust-lang.github.io/api-guidelines/future-proofing.html)) is about keeping
room to add items later, which a defaulted item already has; nothing there argues for shipping
an item with no user.

## Decision

1. **Remove `Transport::NEEDS_REAPER`, `Waiting::REAPS`, the `const` block in `Engine::new`
   that pairs them, and the `compile_fail,E0080` doctest that holds it**, before the next tag.
   ADR-0190 decision 1 stays the written design: a future transport whose bytes arrive only when
   the idle strategy reaps them re-adds the pair and the refusal in its own plan, as defaulted
   items, and proves the refusal again by the same doctest.
2. **Remove `Carrier`, `Transport::carrier` and `Engine::carrier`**, before the next tag.
   `Transport::tls_mode` / `Engine::tls_mode` are **not** touched: they answer a question with two
   live answers (kTLS or userspace) that a published figure depends on.
3. **`tools/w2w` keeps printing `transport: kernel`**, now from the transport type it constructs
   rather than from the engine: a trait private to `w2w` (`Named`, one associated
   `const NAME: &str`) implemented for each transport type `w2w`'s engine is built over —
   `TcpTransport`, and `TlsTransport` where the `tls` feature builds it (`EngineSide`,
   `main.rs:2844`), all `"kernel"` — and the line is printed through the engine's transport type
   parameter, so a second transport type in `w2w` does not compile until it names
   itself. The comment on the trait names this ADR and says that a transport whose answer is not
   fixed by its type must bring back a report read from the engine. Its two refusals on a non-`kernel` carrier (`main.rs:2135`, `:2619`)
   cannot fire and are removed. `scripts/w2w-baseline.sh`, `scripts/boot-p4.sh` and every
   evidence file keep the same line, so no script and no recorded evidence changes shape.
4. **Order**: a pull request of its own, merged before the next tag; it does not wait for the
   SIMD boot. The semver gate (`scripts/check-semver-against-tag.sh`, baseline `v0.1.0`) must
   read no break, which it will, because none of the five is in `v0.1.0`.

## Alternatives considered

- **Keep all five public.** Rejected: five items with no implementor outside a doctest, frozen
  into the next tag, one of them an enum that cannot grow. Once tagged, removing them is a
  `0.2`.
- **`#[doc(hidden)]`.** Rejected: `cargo-semver-checks` would stop guarding them, but they stay
  reachable, and "hidden" is a promise kept by convention. It costs the same edit as removing
  them and leaves the code in place.
- **Keep `Carrier`, add `#[non_exhaustive]`.** Rejected: there is no second value to add today,
  and the one bypass that may come back would not be visible to it.
- **Remove only the reaper pair and keep `Carrier`**, because `Carrier` has a reader. Rejected:
  the reader asks a question whose answer is fixed by `w2w`'s own code (decision 3 keeps the
  line without the API).
- **Remove the `transport:` line too.** Rejected: `w2w-baseline.sh`, `boot-p4.sh`'s arm table and
  every evidence directory since 2026-09-24 carry it; removing it changes three scripts to save
  one `const`.

## Consequences

**Good**

- The next tag carries no public item without a user, and no enum that cannot be extended.
- No script, evidence format or recorded figure changes.
- A later completion-based transport starts from ADR-0190's design, not from an API frozen before
  anyone knew what it needed.

**Bad — and accepted**

- **`w2w`'s `transport:` line stops being a read-back from the engine.** It becomes a statement
  about the binary's own code. That is exactly what ADR-0190 wanted to avoid when `w2w` had two
  transports; with one, it is true by construction, and the comment in decision 3 is the only
  thing that makes a future second transport restore the read-back. Prose does not hold a
  constraint (`CLAUDE.md` §4): the private `Named` trait of decision 3 is what holds it (a new
  transport type does not compile without a name), and a `w2w` unit test asserts
  `<TcpTransport as Named>::NAME == "kernel"`. What neither can see is a transport type whose
  receive path is not decided by its type — an `LD_PRELOAD` bypass is the example — and that is
  written in the comment, not proven.
- **The compile-time refusal pattern loses one of its two examples.** ADR-0014 decision 4's
  `POLLABLE` refusal in `idle_with` stays and still shows the shape.
- **`CHANGELOG.md`'s entry for the hooks is rewritten, not deleted**: it says they were added
  and removed before a tag, so a reader of an old `main` commit is not confused.
- **Re-adding costs more than keeping would have**, if a reaping transport comes back: the
  doctest and the `const` block are re-written from ADR-0190 decision 1.

## Sources

- Cargo Book, *SemVer Compatibility*: <https://doc.rust-lang.org/cargo/reference/semver.html>
  (`item-remove`, `enum-variant-new`, `trait-new-default-item`, and the `0.y.z` rule under
  *Change categories*).
- Rust API Guidelines, *Future proofing*: <https://rust-lang.github.io/api-guidelines/future-proofing.html>.
- Predrag Gruevski, *Checking semver for `#[doc(hidden)]` items*:
  <https://predr.ag/blog/checking-semver-for-doc-hidden-items/>.
- The repository: the `grep` in the table above, run on `origin/main` 9bd45e2; `git grep` on tag
  `v0.1.0`; ADR-0190 *Result*; `CHANGELOG.md` *Unreleased*.
