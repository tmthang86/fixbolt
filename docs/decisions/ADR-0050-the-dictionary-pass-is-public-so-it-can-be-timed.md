# ADR-0050 — The dictionary pass is public, so it can be timed

> **Status:** Accepted (2026-09-05) · **Relates to:** [ADR-0016](ADR-0016-per-machine-baselines-replace-absolute-targets.md)
> (per-machine baselines), [ADR-0049](ADR-0049-bench-builds-pin-function-alignment-and-the-flag-is-read-back.md)
> (the build those baselines are recorded under)
> **Closes:** the measurement half of `STATUS.md` open item 39

## Context

`[measured 2026-09-02]` the wire-to-wire application round trip is **3 898 ns** above the
administrative one, and every committed benchmark together accounts for about **320 ns** of
that. `DESIGN.md` §8's parse row is not the difference: `crates/codec/benches/parse.rs` parses
with `NoDict`, a dictionary whose every answer is a no-op, so its 120.4 ns is framing, field
indexing, `9=` and `10=`.

The session then runs a **second, separate pass** over the same message — for every field
`is_header`, `is_defined_tag`, `field_type`, `allows`, a duplicate check, `enum_allows` and
`accepts`; then `view.get(tag)`, a linear scan of the field index, once per required header tag
and once per required body tag; then the group counters. **Nothing timed it**, and open item 39
has said so since.

Step 2a of the owning plan is "answer *measure it from where* by reading the code, not by
guessing". It was run, and it refuted two of the plan's own three options:

- **Measure the difference against an empty dictionary.** `Fix44` is written into the bodies of
  the three functions in thirteen places; it is not a type parameter, and `Session<R, N, APP>`
  has no dictionary parameter to choose. `codec`'s `parse_into::<D, N>` *is* generic, which is
  exactly why `parse.rs` can substitute `NoDict` — and why its figure is not the pass. Making
  `Session` offer the choice would add an eleventh type parameter to `Engine`: a separate plan,
  not a benchmark.
- **Use an existing public path.** The only entry is `Session::received_with` into the private
  `judge`, which also parses, checks `BeginString`, the schedule, CompIDs, `SendingTime` and
  sequence numbers, calls the application and may send. `self.state != State::LoggedOn` skips
  the whole pass and looks like a one-variable switch, but the two arms diverge downstream, so
  that difference is not the pass — non-negotiable 10's knob.
- **A `pub(crate)` API plus a bench in the same crate**, which the plan preferred, does not
  compile: a bench target is a **separate crate**. `crates/session/benches/alloc.rs` reaches
  this crate as `use fixbolt_session::…`, exactly as an outside user does.

## Decision

**1. `fixbolt_session::validate` is public.**

```rust
pub fn validate<const N: usize>(
    view: &MessageView<'_, N>,
    msg_type: &[u8],
) -> Option<SessionText>
```

It calls `scan_fields`, `missing_required` and `bad_group_count` **unchanged**, in the order
`judge` calls them, and returns the first fault. The three stay private.

**2. The `371=` tag reference is not returned.** It is a `Held<12>` — a fixed buffer, private,
and `14a_BadField.def` is why it is text rather than a number. A caller that needs it wants the
session. This keeps the new signature to two public types that already existed, `MessageView`
and `SessionText`.

**3. It is documented as an API, not as a hole for a benchmark.** "What would the session fault
this message for" is answerable without a session, the answers are the ones
`docs/SESSION-BEHAVIOUR.md` already documents, and the rustdoc says what it does *not* look at:
sequence numbers, CompIDs, `SendingTime`, state.

**4. `crates/session/benches/validate.rs` times it** on a `NewOrderSingle` and a `Heartbeat` —
14 fields against 6, ~13 required tags against ~8, the two ends of the range item 39 names, and
the same two messages `parse.rs` uses so the figures read side by side. The parse happens once
outside the timed loop, because the parse already has its own three cases.

## What was measured

§9 desktop, AMD Ryzen 7 3700X, `scripts/check-machine.sh` `pass 12 fail 0 unknown 1`, bench
build pinned per ADR-0049.

| Case | ns/op, first run |
|---|---|
| `validate NewOrderSingle` | **899.4** |
| `validate Heartbeat` | **162.9** |
| `parse NewOrderSingle (validated)`, for scale | 120.4 |

Proven by reversal, which is the only thing here that says the case measures the pass rather
than the harness: `validate` returning `None` immediately reads **1.1 ns** for both cases — the
signature this repository already knows for a benchmark measuring nothing
([a-benchmark-can-delete-its-own-work](../reference/a-benchmark-can-delete-its-own-work.md)).

**The size is a figure, not an explanation.** Whether it accounts for the 3 898 ns is the
owning plan's step 2d, which is required to add it back and name the remainder. This repository
has already published a wrong cause for a day on arithmetic of exactly this shape.

## Consequences

**Good**

- Item 39's measurement exists, and it keeps existing: the next change to the pass moves a
  number that a band watches, instead of moving nothing anybody can see.
- The pass is now reachable by tests as well as by benches, which is where
  `docs/SESSION-BEHAVIOUR.md`'s claims can be asserted directly rather than through a session.
- A caller embedding this engine can pre-validate a message it is about to send with the same
  code that will judge it on the way in.

**Bad, and named**

- **A public function was added because a benchmark needed one.** The API is defensible on its
  own terms, but that is not why it exists today, and this ADR is where that is recorded rather
  than smoothed over. It is now covered by semver like anything else public.
- **`validate` is not byte-identical to what `judge` runs on a faulting message.** It drops the
  `371=` tag reference, which is built only on the faulting arms, so the compiler may treat
  those arms differently. On a fault-free message — the only kind the bench times, and the case
  item 39 asks about — nothing is dropped.
- **Two entry points now reach the same three functions**, `judge` and `validate`. They cannot
  drift, because `validate` is a call rather than a copy, but a reader has one more hop.
- The figures are large enough to be tempting. `899.4 ns` against a `120.4 ns` parse invites
  "so *that* is where the 3.6 µs went", and that inference is not licensed by anything here.

**Neutral**

- `Held<12>` and `tag_text` stay private, so the surface added is one function.

## Alternatives rejected

- **A feature-gated `pub` plus `required-features` on the bench target.** Keeps the shipped API
  clean. Rejected because cargo **silently skips** a bench target whose feature is off, and
  `scripts/bench.sh` and `scripts/check-bench-alignment.sh` would then see different sets of
  binaries — a gate that passes by not running, which is the exact shape this repository has
  been caught by three times.
- **`#[path = "../src/…"] mod` in the bench, compiling a second copy of the code.** Needs every
  `crate::` import rewritten and `Held`/`tag_text` made public anyway, and it measures a
  recompilation rather than the shipped function. ADR-0049 spent a day on precisely "the binary
  measured is not the binary that ships".
- **Narrow item 39 to a wire-to-wire difference and call it an upper bound.** Free and honest,
  and it was the third option put to the owner. Rejected because the difference would carry the
  application callback, the send and the socket, and §8 would gain a row whose error bars are
  larger than the row.
