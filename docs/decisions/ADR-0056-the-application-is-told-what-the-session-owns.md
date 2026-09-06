# ADR-0056: The application is told what the session owns, so a reply can carry `369`

- **Status:** Proposed
- **Date:** 2026-09-06
- **Plan:** [seq-resync-789-369](../plans/2026-09-04-seq-resync-789-369.md), step 1
- **Supersedes nothing.** Amends the `Application` seam that
  [ADR-0002](ADR-0002-engine-library-split.md) splits and
  [ADR-0048](ADR-0048-an-engine-that-can-speak-first-has-two-doors.md) widened; constrained by
  [ADR-0044](ADR-0044-a-builder-that-is-not-moved-per-field.md), whose measurement rules out the
  obvious alternative.

## Context

`LastMsgSeqNumProcessed (369)` is a FIX 4.4 **header** field — *"the last sequence number of
yours I have processed"* — and the protocol allows it on every message. `STATUS.md` item 45 put
it in wave B beside `NextExpectedMsgSeqNum (789)`.

The plan said it was one line: *when enabled, every outbound message carries
`369=<next_in - 1>`, patched like `34=`*. **It is not, and the reason is structural.**

### There are three ways out of this engine, and the session holds the pen on two

| Path | Who writes the header | `369` reachable |
|---|---|---|
| The seven messages the session generates itself | `out.rs` templates, patched by the session | yes — one `.slot()` |
| An **originated** application message | `rebuild()`, `lib.rs:3297` | yes — one `b.field()` |
| An application **reply** | **the application writes the whole message**; the session emits `buf[r]` untouched (`lib.rs:2982`) | **no** |

The third is the common path on an acceptor. `Application::on_message(msg, seq, stamp, out)`
hands the application a buffer and three facts; the application lays out the entire message,
header included, and the session emits exactly those bytes. The session knows `next_in - 1`
and has no way to say so.

That is not an oversight. It is the property that makes a reply cheap: nothing re-parses or
re-encodes what the application wrote.

### What the other engines do, and why they do not have this problem

`[researched 2026-09-06]`, source read rather than recalled —
[who-owns-the-outbound-header](../reference/who-owns-the-outbound-header.md).

**Every QuickFIX-family engine owns the header of every outbound message, application messages
included, because the application hands it a message *object* which the engine then
serialises**: C++ `Session::fill(header)`, QuickFIX/J `initializeHeader`, QuickFIX/n's header
fill, quickfixgo `fillDefaultHeader(msg, inReplyTo)`. Adding `369` there is genuinely one line,
and three of them have taken it.

So the difference is not that this engine forgot a feature. It is that this engine made a
different trade at the seam, and the bill for that trade arrives here.

Two further facts from the same survey shape the decision:

- **QuickFIX C++ never sends `369` and has no configuration key for it.** The string occurs
  once outside its generated tables, in `Message::isHeaderField`. So `scripts/interop.sh`
  **can never be an oracle for this field in the send direction** — whatever is built here is
  judged by this repository's own tests.
- **`369` is not decoration.** OnixS documents its option as existing because **CME iLink
  requires tag 369 on every message**. Declining it needs an argument, not a shrug.

## Decision

### 1. The session tells the application what the session owns

`Application::on_message` takes a `Header<'_>` — the values this session owns on the way out —
in place of the two loose facts it takes today.

```rust
pub struct Header<'a> {
    pub seq: u32,             // 34, the number this reply will spend
    pub stamp: &'a [u8],      // 52, 21 bytes with milliseconds
    pub last_processed: u32,  // 369, this end's last processed inbound number
}

fn on_message(&mut self, msg: &[u8], hdr: Header<'_>, out: &mut [u8]) -> Option<Range<usize>>;
```

**A struct rather than a fourth positional argument, and the reason is the same one
[ADR-0054](ADR-0054-the-handles-are-made-before-the-engine-and-the-engine-adopts-them.md) gave
`Peer`**: the loose form would put `seq: u32` and `last_processed: u32` side by side in a call
nobody can read, and swapping them compiles. `on_logon` already takes a `Peer`; this is the
same shape at the sibling method.

### 2. `library::Handler` does not change, and that is most of the point

The trait a user of this framework actually implements —
`Handler::on_message(&mut self, msg: &Incoming, reply: Reply) -> Answer` — is **untouched**.
`App` is the adapter that implements `Application` in terms of it, and `Reply::message()`
already writes `34=` and `52=` into the template it hands back (`reply.rs:272`). One more line
there puts `369` in.

So for everybody above the `library` seam, **`369` is guaranteed rather than offered**: it
appears because `Reply` writes it, not because a handler remembered to. That is the same
guarantee QuickFIX/J and QuickFIX/n give, reached a different way.

`Reply::new` gains the value as a parameter. It is public — writing your own adapter over
`Application` is supported — but its own rustdoc already says it *"is not on the path a handler
takes"*.

### 3. Below the `library` seam it is offered, not guaranteed, and the docs say so

An application implementing `session::Application` directly writes its own bytes. If it ignores
`hdr.last_processed`, no `369` appears and nothing detects it — there is no gate that can, short
of the rebuild this ADR exists to avoid.

**This is a real and permanent weakness relative to the four engines surveyed**, and it belongs
in `GUIDE.md` as a constraint the type system cannot enforce, not in a comment.

### 4. Default off, both knobs

`Config::with_last_processed(false)` and `Config::with_next_expected(false)`. Every engine
surveyed that has these defaults them off, and for a good reason: a counterparty that does not
expect an optional header field can answer a `Reject`.

## Alternatives rejected

### The session rewrites the application's bytes after it returns

The direct analogue of what the other four engines do — and here it means running `rebuild()`
on every reply.

**Refuted by measurement, not by preference.** `[measured 2026-09-05]` that per-reply rebuild is
the ~24× of `STATUS.md` item 34, and
[ADR-0044](ADR-0044-a-builder-that-is-not-moved-per-field.md) exists because moving an `S`-byte
builder per field was already 48% of a reply. Paying it back for one optional informational
field would undo the design's central property to add a field QuickFIX C++ does not even send.

### `369` only on the messages the session already generates

Cheapest, breaks nothing, and **no engine surveyed does it** — all four that send `369` set it
on the generic send path, meaning every message.

It is also the semantically worst of the three: `369` would appear on a Heartbeat and be absent
from the ExecutionReport answering an order, so the counterparty would learn this end's progress
during the quiet moments and nothing during the busy ones — which inverts what the field is for.
Rejected on the survey, not on taste.

### A fourth positional argument instead of a struct

Two adjacent `u32` parameters that compile when swapped. See decision 1.

### Decline `369` entirely, as ADR-0045 declined SIMD

Genuinely available, and it was offered: QuickFIX C++ does not have the feature, `nanofix` does
not, the 59 acceptance definitions are blind to it, and nothing in this repository needs it
today. The reopening condition would have been *a counterparty that requires tag 369* — a venue
of the CME iLink family.

**Not taken**, by the owner's decision on 2026-09-06: the seam widening is worth doing once,
while the shape is small enough to change, rather than under a counterparty's deadline.

## Consequences

### Good

- A reply can carry `369`, and through `library::Handler` it does so **guaranteed**, with no
  change to the trait a user writes.
- The one-line-per-path shape survives: seven templates gain a slot, `rebuild()` gains a field,
  `Reply::message` gains a line. Nothing re-parses, nothing re-encodes, no allocation.
- `Header` is where the next session-owned value goes. There will be one — FIXT's `1137=` is
  already named in `STATUS.md` item 45 as phase 2 — and it will cost no signature change.
- The struct removes a swap hazard that the loose form would have introduced permanently.

### Bad, and none of it is small

- **Every implementation of `session::Application` in this repository changes** — `[measured
  2026-09-06]` **11 implementations across 37 files**: benches in three crates, `tools/interop`,
  `tools/w2w`, `engine`'s `Deliver`, `library`'s `App`, and the doctest in `block.rs`. It is a
  compile error rather than a silent one, which is the only good thing about it.
- **`Reply::new` is a public signature change**, so `CHANGELOG.md` carries a breaking entry for
  a crate that has not been published — cheap today, and the reason to do it today.
- **Below the `library` seam the field is offered, not guaranteed.** Decision 3.
- **`369` in the send direction has no external oracle and will not get one** from the current
  fixture: QuickFIX C++ never sends it. Every assertion about it is this repository judging its
  own work, and `CONFORMANCE.md` must say exactly that rather than listing it beside the
  interop-proven rows.
- `crates/session` grows a public type whose only purpose is to carry a field the session layer
  itself never writes — the same shape of cost ADR-0048 recorded for `Application::on_logon`,
  which `crates/session` also never calls.

## Open questions

1. **Should `Header` carry the inbound `34=` instead of `next_in - 1`?** quickfixgo writes the
   replied-to message's own sequence number when it has one, and that is the better answer to
   the question `369` asks — the two differ whenever anything arrived between the message and
   the reply. Not adopted here because the session's `next_in` is what every other engine uses
   and is one field rather than a lifetime; revisit if a counterparty disagrees.
2. **Does `369` belong on a resend?** A replayed message carries its original `34=` and `43=Y`;
   whether its `369` should be the original or the current value is unstated in the spec and
   unanswered by the four engines' code read so far. `rebuild()` carries the source's fields
   through, so today the original would survive — which is at least defensible, and is what
   step 5 must assert deliberately rather than inherit.
