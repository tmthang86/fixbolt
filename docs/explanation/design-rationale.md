# The design in ten decisions

fixbolt's architecture is ten decisions, D1 to D10, written in full in
[DESIGN.md §4](../DESIGN.md#4-the-decisions-that-shape-it). That section is written for the people
who build the engine: it carries measurements, dated corrections and the traps that shaped each
decision. This page tells the same ten for a developer deciding whether to embed fixbolt. For each
one it gives the problem, what was decided, what the decision costs you, and where the full
reasoning is.

It restates no measurement. Where a decision rests on a number, the link goes to the place that
records the number with its benchmark, its machine and its operating-system settings, because a
figure without those is not one this project publishes (`CLAUDE.md` §2, non-negotiable 10).

## The finding behind all ten

Before any of the ten, the project measured where a FIX engine's time goes, and found two things
([DESIGN.md §1](../DESIGN.md#1-the-finding-this-architecture-is-built-around)):

- **The cost is the framework, not the bytes.** Object models built per message, dictionary
  lookups through run-time structures, virtual dispatch and validation that cannot be switched
  off cost more than reading the fields does. So the rule is to keep the framework off the hot
  path.
- **On kernel TCP, parsing is a rounding error.** A round trip is dominated by system calls and
  the network stack. Parsing is under one percent of it on the machine it was measured on
  ([ADR-0045](../decisions/ADR-0045-parse-is-under-one-percent-of-the-wire-and-simd-is-declined.md)).
  A design that optimises the parser and says nothing about how it waits for the socket has
  optimised the wrong part. D8, D9 and D10, and the latency budget in
  [DESIGN.md §8](../DESIGN.md#8-latency-budget-on-kernel-tcp), exist because of this.

## D1 — The session layer is a pure state machine

**The problem.** The FIX session protocol (logon, heartbeats, sequence numbers, resend, gap fill,
logout) is where engines have subtle, expensive bugs. A session layer that owns a socket and reads
the clock can only be tested through a socket, with timing windows. Those tests flake, and flaky
tests get switched off.

**The decision.** The session layer has no socket, no clock and no heap allocation. Bytes arrive as
input; time arrives as an explicit tick from outside; output leaves through a closure the caller
supplies. Errors are enums with no fields, so the error path cannot allocate either. The acceptor
and initiator are one machine, with the role as a compile-time type parameter.

**What it costs.** Someone outside the session has to deliver time, and a session that is judged
before its first tick refuses rather than guessing. Recovery of sequence numbers after a restart,
the journal, and every I/O concern belong to the engine layer, so there are two layers to
understand rather than one. And the whole session layer was written from nothing: the acceptance
tests say whether it is right, they do not write it
([ADR-0001](../decisions/ADR-0001-relationship-to-quickfix.md) *Consequences*).

**What it buys.** QuickFIX's 59 FIX 4.4 acceptance definitions run as unit tests, in milliseconds,
with no socket ([CONFORMANCE.md §1](../CONFORMANCE.md#1-the-59-quickfix-acceptance-definitions)).

**Where it is written down.** [DESIGN.md §4 D1](../DESIGN.md#d1--the-session-layer-is-a-pure-state-machine-with-no-io);
[ADR-0001](../decisions/ADR-0001-relationship-to-quickfix.md) (clean-room, with QuickFIX's tests as
the oracle); [ADR-0004](../decisions/ADR-0004-bidirectional-engine.md) (both roles on one core);
[ADR-0010](../decisions/ADR-0010-a-reconnect-is-not-a-restart.md) (a connection is not a session).

## D2 — The field index is separate from the message view

**The problem.** The obvious parser returns an owned message: a map from tag to value, or a struct
per message type. Building it costs more than reading the bytes, and a large inline array of
fields makes every parse pay for fields the message does not have.

**The decision.** The parser writes into a flat index of `(tag, offset, length)` entries that the
caller owns and reuses for every message on a connection. The message you are handed,
`MessageView`, is 24 bytes: a borrowed slice of the receive buffer plus a reference to that index.
It copies no field. The index size `N` is a const generic you choose, and overflow is an error,
never a silent truncation. Repeating groups are resolved only when you ask for one, so a message
without groups pays nothing for them.

**What it costs.** The API is less obvious than `parse(buf) -> Message`. You own an index. The view
carries two lifetimes, the buffer's and the index's, so you cannot keep a message past the call
that gave it to you, and the index cannot be reused while a view of it is alive. The const `N`
appears in signatures. [ADR-0003](../decisions/ADR-0003-message-representation.md) lists these under
*Bad — and these are real*.

**Where it is written down.** [DESIGN.md §4 D2](../DESIGN.md#d2--the-field-index-is-separate-from-the-message-view);
[ADR-0003](../decisions/ADR-0003-message-representation.md); the measurement that decided it,
[measured-costs.md §1](../reference/measured-costs.md#1-a-512-entry-inline-field-array-costs-6-the-parse-time);
what it means for your handler, [GUIDE.md §3](../GUIDE.md#3-messageview-borrows-the-engines-buffer).

## D3 — Field ordering comes from generated tables

**The problem.** Field order is easy to get wrong and expensive to get wrong. The acceptance
corpus this engine is tested against compares messages field by field, by position, so a correct
message with its fields in another order fails
([the trap](../reference/quickfix-acceptance-def-format.md)). Inside a repeating group the order
is the dictionary's declaration order, not ascending tags, and a DATA field must be written directly
behind its length field. If the order is decided at each call site, every call site can get it
wrong.

**The decision.** The FIX XML dictionary is compiled at build time into tables: header membership,
field order, group delimiters and members, DATA length pairs, and the validation tables. The
encoder walks those tables and never the order the caller supplied. The tables are reached through
associated functions of a trait, with no `dyn` and no structure built at start-up; which dictionary
a session uses is decided at compile time, never at run time
([ADR-0080](../decisions/ADR-0080-the-dictionary-rides-the-encoding-and-a-fixt-session-is-one-table-built-from-two-xml-files.md)
decision 1).

**What it costs.** The dictionary is part of your binary. Changing a venue's dialect means a
rebuild. Today the only way to change the FIX 4.4 dictionary is to replace the whole XML file at
build time ([CONFIGURATION.md §5](../CONFIGURATION.md#5-build-time-environment-variables)); an
overlay for venue fields is designed but not built
([ADR-0207](../decisions/ADR-0207-a-custom-dictionary-is-an-overlay-generated-in-the-users-build-into-the-users-own-type.md),
*Proposed*). The validation tables are bitsets keyed by tag, so a single very high custom tag
number grows them
([the trap](../reference/a-bitset-keyed-by-tag-scales-with-the-highest-tag-not-the-field-count.md)).

**Where it is written down.** [DESIGN.md §4 D3](../DESIGN.md#d3--field-ordering-comes-from-generated-tables-never-from-hand-written-code);
the agreement of the generated order with QuickFIX's generated C++ is
`crates/dict/tests/interop_quickfix_order.rs`, reported in
[CONFORMANCE.md §2](../CONFORMANCE.md#2-dictionary-agreement-with-quickfix).

## D4 — Dispatch is a trait; inline is the default

**The problem.** Where does your code run? On the engine's thread, it adds no hand-off and no copy,
but a handler that blocks stalls the session. On its own thread, a slow handler cannot stall the
session, but every message pays for a copy and a thread hop.

**The decision.** Both exist behind one trait. `InlineDispatch`, the default, calls your handler on
the engine thread straight after the session machine, with the borrowed view. `RingDispatch` copies
each message into a single-producer, single-consumer ring for an application thread. The engine
code that collects replies from another thread is removed at compile time when dispatch is inline.
The idea of separating the engine from the application comes from Artio. In fixbolt the separate
thread is the option and not the default: one reason for that separation is to contain
garbage-collection pauses, and Rust has no collector
([ADR-0002](../decisions/ADR-0002-engine-library-split.md)).

**What it costs.** With the default, a handler that blocks, sleeps or does slow I/O stops its
session and the counterparty sees missed heartbeats ([GUIDE.md §2](../GUIDE.md#2-the-engine-calls-you-on-its-hot-path)).
With the ring, you pay the hop and the copy, memory for the ring, and D10's second rule: a ring
that fills ends the connection.

**Where it is written down.** [DESIGN.md §4 D4](../DESIGN.md#d4--dispatch-is-a-trait-inline-is-the-default-the-ring-buffer-is-the-option);
[ADR-0002](../decisions/ADR-0002-engine-library-split.md);
[ADR-0007](../decisions/ADR-0007-spsc-ring-without-unsafe.md) (the ring, in safe Rust, and its
price); `crates/engine/tests/dispatch.rs` asserts that the same message produces the same bytes on
the wire under either dispatch.

## D5 — Transport is a trait, and a feature gates the module itself

**The problem.** Optional transports (TLS, `io_uring`, and one day kernel bypass) bring
dependencies and sometimes system libraries. A crate whose optional module is declared
unconditionally, or whose `build.rs` runs an external toolchain regardless, cannot be built by
anyone who lacks that toolchain.

**The decision.** `Transport` is a two-method trait; kernel TCP is the only implementation built by
default. A Cargo feature gates the `mod` declaration itself, and `build.rs` runs no external
toolchain unless its feature is on. CI builds with `--no-default-features` on a machine with
nothing optional installed. A transport that cannot start where it was asked for (for example
`io_uring` blocked by a container's seccomp filter) refuses at start-up with a named cause; it never
falls back silently.

**What it costs.** Little in code. The cost is in testing: Cargo unifies features across one
invocation, so a feature combination can compile in the workspace and fail on its own
([the trap](../reference/feature-flags-unify-across-a-workspace.md)), and every combination worth
shipping needs its own build in CI.

**Where it is written down.** [DESIGN.md §4 D5](../DESIGN.md#d5--transport-is-a-trait-tcp-is-the-only-implementation-that-ships-by-default);
[measured-costs.md §3](../reference/measured-costs.md#3-a-feature-flag-that-does-not-gate-its-module-makes-a-crate-unbuildable).

## D6 — No `panic!`, `unwrap()` or `expect()` in a library crate

**The problem.** A panic inside an engine you embed takes down your process, or at least the thread
that owns your sessions. Rules written as prose do not hold; a count of panicking calls in a mature
codebase shows how far discipline alone gets.

**The decision.** The workspace denies clippy's `unwrap_used`, `expect_used` and `panic` lints in
every library crate, and separately denies `indexing_slicing`, because `a[i..j]` panics without
naming any of the three. `scripts/check-lint-config.sh` proves the lints bite by breaking them on
purpose.

**What it costs.** Every fallible path returns a typed error that the caller must handle. Older
code that indexes slices directly is recorded as debt with a count that may only go down
(`scripts/check-indexing-debt.sh`). Some panics are invisible to every lint; the project records
the ones it has found
([a debug assertion](../reference/a-debug-assertion-is-a-panic-the-lint-cannot-see.md),
[`split_at`](../reference/split-at-panics-where-no-lint-looks.md)).

**Where it is written down.** [DESIGN.md §4 D6](../DESIGN.md#d6--no-panic-unwrap-or-expect-in-any-library-crate);
the lint block in the workspace `Cargo.toml`.

## D7 — Persistence is a policy, and it is off the hot path

**The problem.** FIX requires an engine to resend what the counterparty missed, so outbound
messages must be kept. Writing them to disk synchronously puts a disk write, or a sync, on every
message.

**The decision.** How much is kept is your choice: nothing, an in-memory ring, a file appended by a
background thread, or a file synced before the message is acknowledged. In every policy the
in-memory ring answers resend requests, so the engine thread never reads the disk. A message older
than the ring is gap-filled, which FIX allows, and the engine counts and reports it. The inbound
sequence number is recorded after delivery, not before.

**What it costs.** The ring is memory per session, sized by default for thousands of messages. A
resend request older than the ring gets a gap fill, not the original. Because the inbound count is
written after delivery, a crash at the wrong moment delivers a message twice (flagged as a possible
duplicate), so your application must be idempotent per sequence number. Under the synced policy
the inbound path pays a sync per message.

**Where it is written down.** [DESIGN.md §4 D7](../DESIGN.md#d7--persistence-is-a-policy-and-it-is-off-the-hot-path);
[ADR-0008](../decisions/ADR-0008-journal-is-a-trait.md);
[ADR-0017](../decisions/ADR-0017-the-inbound-count-is-persisted-after-delivery.md);
[ADR-0046](../decisions/ADR-0046-the-ring-is-the-resend-store-and-a-replay-goes-in-batches.md);
choosing a policy, [GUIDE.md §6](../GUIDE.md#6-journalling-pick-the-policy-deliberately).

## D8 — Two modes: `standard` blocks, `hft` busy-polls

**The problem.** Waiting for a socket to become readable by blocking in the kernel costs a wake-up
of several microseconds, with scheduler jitter. Not blocking means spinning, which burns a whole
core forever. An engine that burns a core out of the box looks broken to most people who try it,
and one that blocks gives up the largest cost it controls.

**The decision.** Two named modes. `standard` is the default: the engine thread blocks on readiness
with a timeout and gives the core back, and it runs on any Unix (Linux and macOS). `hft` is opt-in and Linux-only: the
engine thread polls non-blocking sockets and never sleeps in the kernel while serving, on a core
you isolate and pin. Both halves are rules, and both are machine-checked: a `standard` engine that
spins is as much a defect as an `hft` engine that sleeps. In `hft` the shape the budget is built
for is one session per polling thread
([ADR-0012](../decisions/ADR-0012-latency-first-and-one-session-per-polling-thread.md)).

**What it costs.** `standard` pays the wake-up on every message that arrives at an idle engine.
`hft` costs a core per engine thread, needs a tuned Linux host, and has a hard session ceiling,
because every session polled adds a system call to every turn
([ADR-0025](../decisions/ADR-0025-hft-has-a-hard-session-ceiling-and-the-engine-advises-rather-than-applies.md)).
Every test and gate has to run in both modes, and a `standard` figure and an `hft` figure are never
comparable ([ADR-0013](../decisions/ADR-0013-two-modes-standard-and-hft.md) *Consequences*).

**Where it is written down.** [DESIGN.md §4 D8](../DESIGN.md#d8--in-hft-the-engine-thread-busy-polls-in-standard-it-blocks);
[ADR-0013](../decisions/ADR-0013-two-modes-standard-and-hft.md);
[ADR-0014](../decisions/ADR-0014-standard-mode-blocks-on-poll.md); the measured wake-up and round
trips, [DESIGN.md §8](../DESIGN.md#8-latency-budget-on-kernel-tcp); choosing, [GUIDE.md §0](../GUIDE.md#0-first-decide-your-mode).

## D9 — Outbound messages are patched templates

**The problem.** Most of an outbound message is the same every time a session sends that message
type: the begin string, the comp IDs, the message type and the field order. Building it field by
field, sorting it, and formatting `SendingTime` from a clock value each time repeats work whose
answer has not changed. Formatting a timestamp naively costs about as much as parsing a message.

**The decision.** Each message type's skeleton is encoded once per session into a template: a list
of static byte ranges and slots, sorted at build time by D3. Sending fills the slots and skips the
ones you leave empty. The body is written first and the header is written in front of it, because
`BodyLength` is variable-width. `SendingTime` comes from a cache that rebuilds its date-and-minute
prefix once a minute and formats only the seconds per message.

**What it costs.** Templates are memory per session per message type. The shape has a measured
floor that is not free, and the project withdrew a borrowed target it once graded itself against
([ADR-0016](../decisions/ADR-0016-per-machine-baselines-replace-absolute-targets.md)). Configurable
timestamp precision is a run-time branch in the cache, measured rather than assumed
([ADR-0057](../decisions/ADR-0057-sub-millisecond-time-arrives-beside-the-tick.md)).

**Where it is written down.** [DESIGN.md §4 D9](../DESIGN.md#d9--outbound-messages-are-templates-a-pre-sorted-parts-list-patched-not-built);
[ADR-0041](../decisions/ADR-0041-the-library-layer-buys-an-api-with-a-template-per-message.md);
`crates/codec/src/template.rs`, `crates/codec/src/timestamp.rs`.

## D10 — Backpressure has a stated policy, at both ends

**The problem.** Two things can fall behind. A counterparty that reads slowly fills the socket's
send buffer. An application thread behind `RingDispatch` that stops reading fills the ring. Blocking
the engine thread on either stops every session on it; dropping messages silently loses orders.

**The decision.** On the wire (D10), outbound bytes queue up to a bound, then the session ends with
a Logout saying `slow consumer`; that is the default, because a FIX counterparty that cannot keep
up is broken. A message goes into the queue whole or not at all. A blocking policy exists for tests
only. Behind the ring (D10b), a full ring ends the connection with a Logout saying
`slow application`, because a message the session accepted and numbered but the application never
saw is silent loss.

**What it costs.** A slow peer or a stalled application thread loses its connection rather than
slowing the engine. The ring's default capacity is memory per connection, and an application that
pauses longer than the ring holds drops its session. The ring's capacity comes from one synthetic
saturation run; no real application has stalled against it yet.

**Where it is written down.** [DESIGN.md §4 D10](../DESIGN.md#d10--tcp-send-backpressure-has-a-stated-policy)
and [D10b](../DESIGN.md#d10b--a-full-ring-to-the-application-ends-the-connection);
[ADR-0011](../decisions/ADR-0011-a-full-ring-disconnects.md); `crates/engine/tests/backpressure.rs`;
what it means for your handler, [GUIDE.md §4](../GUIDE.md#4-when-the-ring-fills-you-lose-the-connection).

## After D10

[DESIGN.md §4](../DESIGN.md#4-the-decisions-that-shape-it) continues past these ten with decisions
that extend them rather than change them: TLS as a second transport with its guarantee stated per
mode (D11), how a tick counts time (D13), the message log (D14), how an application speaks first
(D15), and encodings other than tag=value behind a trait (D16). The decision records, listed at
the end of this book, hold every choice with its alternatives and its costs; an accepted record is
never edited, only superseded.
