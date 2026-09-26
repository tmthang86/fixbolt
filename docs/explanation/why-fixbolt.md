# Why fixbolt

This page explains, mechanism by mechanism, why fixbolt is fast on the path that matters, what each
mechanism avoids paying, and what it costs you in return. It ends with what fixbolt does not do.
It is an explanation of this engine's own design, not a comparison with any other engine. Another
engine appears only where a design fact from its own documentation makes an explanation clearer.

## What "fast" means here

fixbolt's claim is narrow on purpose: it is **a FIX acceptor on kernel TCP whose latency is a
published, reproduced number**, and a kernel-bypass figure, if one is ever published, appears only
as a second, labelled row beside a kernel-TCP figure from the same boot
([ADR-0099](../decisions/ADR-0099-kernel-tcp-stays-the-default-and-the-headline-and-a-bypass-figure-is-a-second-labelled-row.md)).

The number that matters is the **round trip**: a message arrives on a socket, the engine parses and
judges it, your handler answers, and the reply leaves on the socket. Every figure fixbolt publishes
names the committed benchmark that produced it, the machine, the operating-system settings in force
([DESIGN.md §9](../DESIGN.md#9-deployment--the-os-is-part-of-the-design)), the mode (`standard` or
`hft`) and the number of sessions on the thread. A figure missing any of those is not one of this
project's figures. The recorded figures live in
[DESIGN.md §8](../DESIGN.md#8-latency-budget-on-kernel-tcp) and
[reference/measured-costs.md](../reference/measured-costs.md); this page quotes two and links the
rest.

## Where the time goes

The design starts from a measurement, not from the parser. From
[ADR-0045](../decisions/ADR-0045-parse-is-under-one-percent-of-the-wire-and-simd-is-declined.md),
`[measured 2026-09-02]` on the project's tuned Linux desktop (AMD Ryzen 7 3700X, Linux
7.0.0-30-generic, `scripts/check-machine.sh` reading `pass 12 fail 0 unknown 1`, engine pinned to
isolated `cpu6` and the client to `cpu7`), `hft` mode, one session, over loopback, medians of 20
`scripts/w2w-baseline.sh` runs of 20 000 messages each:

| | Parse | Round trip, p50 | Parse's share |
|---|---|---|---|
| `TestRequest` → `Heartbeat` | 57.3 ns (`Heartbeat`) | 16 010 ns | 0.36% |
| `NewOrderSingle` → `ExecutionReport` | 122.6 ns | 19 908 ns | 0.62% |

Those round trips were taken with the CPU's speculative-execution mitigations on and the host's
netfilter (firewall) rules loaded, both large parts of the kernel's share
([DESIGN.md §8](../DESIGN.md#8-latency-budget-on-kernel-tcp)), and over loopback, which has no
network card, driver or interrupt in it. The table is one measurement procedure; the project's
later rule publishes two procedures side by side
([ADR-0068](../decisions/ADR-0068-a-published-figure-is-two-procedures-shown-side-by-side.md)).

What the table says: **on kernel TCP the round trip is spent in system calls and the network stack,
and parsing is under one percent of it.** So fixbolt's speed does not come from a clever parser. It
comes from two things: keeping everything a FIX engine usually builds per message (objects, lookups,
allocations, sorted field lists, formatted timestamps) off the hot path, and controlling what the
engine thread does around the kernel: how it waits, how often it enters, and what it never does
there. The mechanisms below are those two things, one at a time.

## 1. Parse in place into a borrowed view

**What it does.** The parser does not build a message object. It records each field as
`(tag, offset, length)` in a flat index that you own and reuse for every message on the connection,
and hands you a 24-byte `MessageView`: a borrowed slice of the receive buffer plus a reference to
the index. A field's value is a slice of the bytes that arrived. A price becomes a number only when
you ask for it. A repeating group is walked only when you ask for it.

**The cost it avoids.** Building a map or a struct per message, copying each value out of the
buffer, and paying for group structure on messages that have none.

**What proves it.** `crates/codec/src/lib.rs` fails to compile if `MessageView` grows past 24 bytes;
`crates/codec/benches/alloc.rs` counts zero allocations for parse and group walk.
[DESIGN.md §4 D2](../DESIGN.md#d2--the-field-index-is-separate-from-the-message-view),
[ADR-0003](../decisions/ADR-0003-message-representation.md).

**What it costs you.** You cannot keep a message past the call that gave it to you; copy what you
need ([GUIDE.md §3](../GUIDE.md#3-messageview-borrows-the-engines-buffer)).

## 2. No heap allocation on the hot path, counted

**What it does.** Parse, serialise, the session machine and dispatch allocate nothing per message.
Buffers, rings and tables are allocated up front and reused.

**The cost it avoids.** The allocator's own latency, its occasional slow path, and the lock
contention between threads that allocate. These show up in the tail, not the median.

**What proves it.** Not reading the code: a counting global allocator in each hot-path crate's
`benches/alloc.rs` (`codec`, `session`, `engine`, `library` and others) counts every allocation over
the timed loop and asserts zero, and each case asserts that its own path actually ran. The guard is
proven by breaking it: adding one allocation to the counted loop turns the count from zero into one
per message. The `tools/w2w` harness counts allocations on both of its threads across its timed
window and asserts zero there too
([CONFORMANCE.md §4](../CONFORMANCE.md#4-zero-allocation-on-the-hot-path)).

## 3. The dictionary is compiled, not consulted

**What it does.** The FIX XML dictionary is turned into Rust tables when fixbolt is built: which
tags are header fields, the order fields are written in, each repeating group's delimiter and
members, which tags are defined, allowed per message type, their types and their enum values. The
session reaches them through a trait whose functions are associated functions: no `self`, no `dyn`,
no pointer to a structure built at start-up. The compiler sees the table the session uses.

**The cost it avoids.** A lookup through a data structure assembled at start-up, on every field of
every message, and the indirection of choosing the dictionary at run time. For contrast in
mechanism, not in speed: QuickFIX (C++) reads its XML data dictionary at run time, per session,
from a path in its configuration
([configuration](https://quickfixengine.org/c/documentation/getting-started/configuration.html)).
That design lets an operator change a venue's dialect without a rebuild; fixbolt's does not.

**What proves it.** The tables are generated by `crates/dict/build.rs`; which dictionary a session
uses is a compile-time type, never a run-time choice
([ADR-0080](../decisions/ADR-0080-the-dictionary-rides-the-encoding-and-a-fixt-session-is-one-table-built-from-two-xml-files.md)
decision 1). The generated order is checked against QuickFIX's own generated C++ by
`crates/dict/tests/interop_quickfix_order.rs`
([CONFORMANCE.md §2](../CONFORMANCE.md#2-dictionary-agreement-with-quickfix)).

**What it costs you.** A dialect change is a rebuild. See *What fixbolt does not do*, below.

## 4. Outbound messages are patched, not built

**What it does.** For each message type a session sends, the fixed skeleton (begin string, comp
IDs, message type, field order) is encoded once into a template of static byte ranges and slots,
already sorted by the generated tables. Sending a message fills the slots; a slot you leave empty is
skipped. The body is written first and the header in front of it, so `BodyLength` never forces the
body to move. `SendingTime` comes from a cache that rebuilds its `YYYYMMDD-HH:MM` prefix once a
minute and formats only the seconds per message.

**The cost it avoids.** Assembling and sorting the field list per message, shifting the body once
its length is known, and formatting a full timestamp from a clock value every time, which on its
own costs about as much as parsing a message.

**What proves it.** `crates/codec/src/template.rs` and `crates/codec/src/timestamp.rs`; timed by
`crates/codec/benches/serialize.rs` against a per-machine baseline in `benches/baselines.tsv`
([ADR-0016](../decisions/ADR-0016-per-machine-baselines-replace-absolute-targets.md));
[DESIGN.md §4 D9](../DESIGN.md#d9--outbound-messages-are-templates-a-pre-sorted-parts-list-patched-not-built).

## 5. A pure session machine

**What it does.** The FIX session layer (logon, sequence numbers, heartbeats, resend, gap fill,
logout, rejects) has no socket, no clock and no allocation. Time is handed to it; output leaves
through a closure. Its errors are enums with no fields.

**The cost it avoids.** Honestly, this one is mainly about correctness: it is what lets the 59 FIX
4.4 acceptance definitions run as unit tests
([CONFORMANCE.md §1](../CONFORMANCE.md#1-the-59-quickfix-acceptance-definitions)). Its latency
share is that the session makes no system call and reads no clock of its own, and that its refusal
path (which a hostile counterparty controls) cannot build an error string.
`crates/session/benches/alloc.rs` counts the refusal path separately for that reason.
[DESIGN.md §4 D1](../DESIGN.md#d1--the-session-layer-is-a-pure-state-machine-with-no-io).

## 6. Your handler runs inline, on the engine thread

**What it does.** By default your handler is called on the engine thread, directly after the session
machine accepts a message, with the borrowed view. When dispatch is inline, the engine code that
would collect replies from another thread is removed at compile time, through a constant on the
`Dispatch` trait.

**The cost it avoids.** A copy of every message into a queue, a hand-off to another thread, and the
wake-up of that thread. The alternative, `RingDispatch`, exists for handlers that may block, and its
price is recorded
([measured-costs.md](../reference/measured-costs.md#what-a-thread-hop-costs-when-the-ring-may-not-use-unsafe)).

**What proves it.** `crates/engine/tests/dispatch.rs` asserts the same message produces the same
bytes on the wire under either dispatch.
[DESIGN.md §4 D4](../DESIGN.md#d4--dispatch-is-a-trait-inline-is-the-default-the-ring-buffer-is-the-option).

**What it costs you.** A handler that blocks stops its session
([GUIDE.md §2](../GUIDE.md#2-the-engine-calls-you-on-its-hot-path)).

## 7. Two ways to wait, chosen by you

**What it does.** In `standard` mode, the default, the engine thread blocks on socket readiness with
a timeout when there is nothing to do, and gives the core back. In `hft` mode, opt-in and
Linux-only, it polls non-blocking sockets and never sleeps in the kernel while serving, on a core
you isolate and pin.

**The cost it avoids.** `hft` avoids the kernel wake-up, and its jitter, on every message that
arrives at an idle engine. `standard` avoids burning a core on a machine that has other work.
In the same measurement as the table above (same machine, settings, procedure, one session, over
loopback), the `TestRequest` → `Heartbeat` round trip at p50 was **16 010 ns in `hft`** and
**19 447 ns in `standard`**
([DESIGN.md §8](../DESIGN.md#the-round-trip-measured), which also gives p99 and p99.9, and the
application round trip).

**What proves it.** Both halves are checked by machine, and each check is also shown to fail on the
wrong mode: `scripts/check-no-kernel-sleep.sh` traces the `hft` engine thread's system calls
while serving, `scripts/check-no-kernel-sleep-by-ctxt.sh` counts its voluntary context switches, and
`scripts/check-standard-gives-the-core-back.sh` asserts that an idle `standard` engine blocks and
uses almost no CPU.
[DESIGN.md §4 D8](../DESIGN.md#d8--in-hft-the-engine-thread-busy-polls-in-standard-it-blocks),
[ADR-0013](../decisions/ADR-0013-two-modes-standard-and-hft.md).

**What it costs you.** `hft` spends a whole core per engine thread and needs a tuned host
([best-practices-hft.md](../best-practices-hft.md), [hft-playbook.md](../hft-playbook.md)).
Choosing is [GUIDE.md §0](../GUIDE.md#0-first-decide-your-mode).

## 8. The system calls are priced, not ignored

**What it does.** One engine turn is one pass over its connections: flush what is queued, deliver
the tick, read each socket once, cut out whole messages, judge them, flush again. Reading once per
turn keeps a fast counterparty from starving the others. The `hft` shape the budget is built for is
one session per polling thread, because each extra session on the thread adds a system call to
every turn; many sessions per thread is supported, named, and budgeted separately
([ADR-0012](../decisions/ADR-0012-latency-first-and-one-session-per-polling-thread.md)).

**The cost it avoids.** A latency figure that silently depends on how many sessions share a thread.
Every published figure names its session count.

**Where it is recorded.** The per-turn cost of a read and the arithmetic of polling many sessions
are in
[measured-costs.md](../reference/measured-costs.md#the-engine-is-syscall-bound-and-the-8-budget-is-spent-before-fix-begins);
running many sessions is [GUIDE.md §1a](../GUIDE.md#1a-running-many-sessions-shard-across-threads-do-not-stack-on-one).

## 9. Nothing slow happens on the engine thread

**What it does.** Resend requests are answered from an in-memory ring, never from disk. A file
journal is written by a background thread; when a connection ends, its writer is retired without
the engine thread waiting for it. A counterparty that reads too slowly is disconnected with
`slow consumer` rather than being waited for; a full ring to your application thread ends that
connection with `slow application`.

**The cost it avoids.** A disk read or write, a sync, a join on another thread, or an unbounded wait
for a slow peer, on the thread that serves every session it holds.

**What proves it.** `crates/engine/tests/backpressure.rs`; `crates/engine/tests/retire.rs`, which
counts the engine thread's voluntary context switches while sessions with file journals end.
[DESIGN.md §4 D7](../DESIGN.md#d7--persistence-is-a-policy-and-it-is-off-the-hot-path),
[D10](../DESIGN.md#d10--tcp-send-backpressure-has-a-stated-policy).

## What these choices cost

The mechanisms above are not free, and the design records each cost where it is decided
([The design in ten decisions](design-rationale.md) collects them). The ones an embedder feels:

- **A dialect is a rebuild.** The dictionary is compiled in.
- **A core, in `hft`.** The engine thread spins on it whether or not messages arrive.
- **Borrowed messages.** You copy what you want to keep, and the index size is a type parameter.
- **Memory spent for latency.** Each session's resend ring and each template is allocated up front;
  the defaults are in [CONFIGURATION.md §2](../CONFIGURATION.md#2-programmatic-limits-and-defaults).
- **Disconnection instead of waiting.** A slow peer or a stalled application thread loses its
  session rather than slowing the others.

## What fixbolt does not do

Stated plainly, because anyone evaluating a FIX engine will look for these. The full list, capability
by capability, is [PRD.md §3](../PRD.md#3-where-this-stands-against-quickfix); the permanent
non-goals are [PRD.md §5](../PRD.md#5-permanent-non-goals); the embedding-level gaps are
[GUIDE.md §9](../GUIDE.md#9-what-this-engine-does-not-do-for-you).

- **Its production track record is zero.** No test coverage substitutes for counterparties having
  found an engine's bugs, and this gap closes only by being deployed.
- **FIX 4.4 is the version.** FIX 5.0 SP2 over FIXT 1.1 exists behind an off-by-default feature,
  with its conformance score and one asserted divergence recorded in
  [CONFORMANCE.md §9](../CONFORMANCE.md). Earlier FIX versions are not supported.
- **No dictionary at run time, and today no overlay.** A venue's custom fields and messages are
  expressible today only by replacing the whole FIX 4.4 XML at build time
  ([CONFIGURATION.md §5](../CONFIGURATION.md#5-build-time-environment-variables)); a plain custom
  tag at or above 5000 can instead be let through unvalidated with `ValidateUserDefinedFields=N`
  ([CONFIGURATION.md §1](../CONFIGURATION.md#1-configuration-file-keys)). An overlay generated in
  your own build is designed and not yet built
  ([ADR-0207](../decisions/ADR-0207-a-custom-dictionary-is-an-overlay-generated-in-the-users-build-into-the-users-own-type.md),
  *Proposed*).
- **No typed message classes.** You read fields from a view by tag; this is deliberate
  ([ADR-0003](../decisions/ADR-0003-message-representation.md)).
- **No application-message semantics.** The session validates required fields, types, enum values
  and structure against the dictionary; whether an order makes business sense is your
  application's decision.
- **No credential check of its own.** Admission of a counterparty goes through a hook you implement;
  nothing beyond identity is checked for you.
- **Session schedules are UTC only.** No timezone database lives in the pure session layer.
- **The sharded runtime has no ordered shutdown.** A single engine stops in order; the sharded
  `hft` runtime cannot yet.
- **No kernel bypass.** Every figure is kernel TCP; bypass is a later-phase candidate, not a feature.
- **`hft` is Linux only, and so is TLS.**
- **Not on crates.io.** Releases are git tags
  ([ADR-0161](../decisions/ADR-0161-0-1-0-is-a-git-tag-not-a-crates-io-upload-and-the-stranger-and-the-semver-gate-read-the-tag.md)),
  and the engine is pre-1.0.

What the acceptance corpus does not test (repeating groups, application semantics, reconnect and
schedules) is listed in [PRD.md §4](../PRD.md), and what the conformance evidence does not prove in
[CONFORMANCE.md §6](../CONFORMANCE.md#6-what-is-not-proven-here).
