# ADR-0046 — The in-memory ring is the whole resend store, and a replay goes out in batches

- **Status**: Proposed — 2026-09-04
- **Date**: 2026-09-04
- **Deciders**: Tran Manh Thang
- **Related**: [ADR-0008](ADR-0008-journal-is-a-trait.md) — why the journal is a trait the
  session asks rather than a buffer it owns ·
  [ADR-0017](ADR-0017-the-inbound-count-is-persisted-after-delivery.md) — the other half of
  what the journal records ·
  [ADR-0039](ADR-0039-a-fresh-journal-is-the-deployments-to-build.md) — the journal on disk,
  and what it is for ·
  [ADR-0035](ADR-0035-an-observer-is-asked-once-per-turn.md) — how a counter becomes an event ·
  [DESIGN.md](../DESIGN.md) §4 D7 (the journal), D8 (the engine thread), D10 (backpressure) ·
  `STATUS.md` item 43 · [plan](../plans/2026-09-03-resend-from-the-journal.md)

## Context

An acceptor sends 100 `ExecutionReport`s during a day. The counterparty drops, reconnects, and
asks `35=2 7=1 16=0` — *send me everything*. **Today this engine replays the last 8 and gap-fills
the other 92.**

On the wire that is a legal answer: FIX lets the sender gap-fill anything it chooses not to
replay. To the counterparty it is **92 fills that vanished**, and on this side there is no
counter, no event and no log line saying it happened.

Three facts produce it, and none of them is a bug in isolation:

| Fact | Where |
|---|---|
| `Store = MemJournal<8, 512>` — the default ring holds **eight** messages | `crates/engine/src/journal.rs:40` |
| `FileJournal::get` answers from its in-memory ring, **never from the file** | `journal.rs:440–470` |
| the resend loop gap-fills over every number `get` cannot answer | `crates/session/src/lib.rs:2097–2130` |

The second is a *correct* decision that has never been written down as one: reading a file on
the engine thread is non-negotiable 4 broken, in `hft` outright and in `standard` as "two modes,
two rules". The first is an accident of the corpus — the rustdoc says *"the acceptance corpus
never asks for more than three at once; a real acceptor sets its own"* — and nothing forces a
real acceptor to set anything, nothing counts what the default costs, and `GUIDE.md` has no
arithmetic for choosing.

Three smaller defects sit in the same place:

1. **`Journal::put` refuses a message longer than `LEN` in silence.** The trait says *"there is
   nothing the session could do about it"* — true, and *counting* it is still something.
2. **A large resend ends the session.** The replay loop emits the whole range in one call; `TX`
   is 8 KiB by default; `Out::push` refuses a message that does not fit (`conn.rs:581`), sets
   `overflow`, and D10's `Disconnect` answers with `Logout 58=slow consumer`. Fifty messages of
   200 bytes is enough. **A counterparty that asked for a resend is hung up on for being slow,
   with its own socket empty.**
3. **`MemJournal::get` and `highest` scan all `N` slots.** Harmless at 8. At 4096, a
   1000-message resend is four million comparisons on the engine thread.

## Decisions

### 1. The in-memory ring is the entire resend store. Disk is for restart and audit.

**The engine thread never reads a file to answer a `ResendRequest`.** Anything older than the
ring is gap-filled, exactly as today — and from now on that gap fill is **counted and emitted as
an event**, so the operator learns that the ring is too small from an event rather than from a
counterparty's complaint.

This is not new behaviour. It is behaviour that was implicit in the code and is now a decision
with a name, a cost and a counter.

### 2. The ring is a deployment parameter, it has a formula, and the default holds a normal day.

`SLOTS` moves from **8 to 4096**. Memory is `N × (LEN + 8)` ≈ **2 MiB per session** at the
default sizes.

`GUIDE.md` §6 carries the arithmetic:

> **N ≥ the number of application messages you send during the longest disconnection you are
> willing to replay across** — for most desks, one trading day.

A gateway holding hundreds of sessions chooses a smaller `N` through the const generic, which is
the mechanism `GUIDE.md` §1a already describes for exactly this trade.

### 3. The ring is addressed by `seq % N`, O(1), and never scanned.

`get` indexes one slot and compares its number; `highest` comes from the write cursor; `oldest`
reads **one slot** — the one about to be overwritten, which once the ring has wrapped is the
oldest still standing. **`oldest` is what makes decision 1's counter possible**:
without it the session cannot tell *"this number was never sent"* from *"this number fell out of
the ring"*, and only the second is worth an event.

The slots move from an inline `[Slot<LEN>; N]` to a `Box<[Slot<LEN>]>` allocated **once in
`new()`**. This is not an allocation on the hot path — it is startup, in the same class as the
pre-faulted buffers D8 already asks for — and `benches/alloc.rs` counts a window that excludes
construction. It is also the only shape that is *safe*: `MemJournal<4096, 512>::new()` as an
inline array builds 2 MiB on the stack and moves it, and at 65536 slots that is 32 MiB against an
8 MiB default stack — a SIGSEGV, not a red test.

### 4. A replay goes out in batches, bounded by a configured count per turn.

The session keeps a resend cursor and replays at most `Config::resend_batch` messages each time
it is called — from `received_with` or from `tick`. The rest go out on later turns, interleaved
with new traffic, which is legal and unambiguous: a replayed message carries its **original**
`34=` and `43=Y`, so no counterparty can confuse the two.

The default is **8**, and the constraint that matters is `resend_batch × SLOT_LEN < TX`:
8 × 512 = 4 KiB against 8 KiB. **D10 does not change** — backpressure still disconnects a
consumer that genuinely cannot keep up. What changes is that answering a resend is no longer
something this engine does to itself.

### 5. What was rejected, and why

- **(a) `pread` from the journal file when the ring misses.** Non-negotiable 4 in `hft`, and in
  `standard` it makes the two modes answer the same protocol question differently — which is the
  shape ADR-0013 exists to prevent.
- **(b) A side thread that reads the file and sends the replay itself.** The session owns `34=`
  and `52=`; another thread writing to the socket between its messages breaks D1's purity and the
  ordering it rests on.
- **(c) A growable `Vec` per session, the way QuickFIX does it.** Non-negotiable 1.

**Deferred, not rejected**: a disk fallback for `standard` only. If a real deployment needs a
resend deeper than memory, it gets its own ADR, its own mode-scoped gate, and its own
measurement. Nothing here forecloses it.

## Revision `[2026-09-04]`, while `Proposed`

Decision 3 first said `oldest` would be **a field maintained on overwrite**. It is a one-slot
read instead, and the reason is a fact about the data rather than a preference: **only
application messages are journalled, so the numbers in the ring are sparse.** An acceptor that
answers one order in three keeps 2, 5, 8 — so "the number that just left, plus one" names a
message that was never sent, and any arithmetic from the newest number does the same. What is
true regardless of sparseness is *which slot goes next*, and that is a cursor this ring already
has. Same O(1), one less invariant to keep, and no state that can drift from the slots it
describes.

## Consequences

### Good

- A counterparty asking for a day of messages gets **the messages**, not 92 fills.
- A resend larger than `TX` no longer ends the session. That is a wire test
  (`crates/engine/tests/backpressure.rs`), not a claim.
- **Two silences become events**: `ResendBeyondJournal { filled, oldest }` says the ring is too
  small, with the numbers; `JournalRefused { count }` says messages are longer than `SLOT_LEN`.
  Both reach `SessionSnapshot`, so an operator can see them without a debugger.
- `get` and `highest` become O(1), which is what makes a 4096-slot ring affordable at all.

### Bad, and each is a real cost

- **+2 MiB of resident memory per session at the default.** A gateway with 200 sessions pays
  400 MiB it did not pay before. The const generic is the answer, and `GUIDE.md` §6 has to say
  so where somebody will read it.
- **A long resend now takes many turns instead of one.** 100 messages at batch 8 is 13 turns. In
  `hft` a turn is ~449 ns; in `standard` it is one poll wakeup. Under a millisecond either way,
  and it is still slower than it was — the old behaviour was "one turn, then the session ends",
  which is faster and useless.
- **`Journal` gains two required methods**, `oldest` and a `bool` from `put`. Every
  implementation in the tree changes in the same commit; nothing is published, so nobody else's
  code breaks. Neither gets a default, for the reason `highest` has none: a default that lies is
  worse than a compile error.
- **The `FileJournal` `Async` ring still drops records in silence when full.** It is the same
  shape as the `put` refusal this ADR counts, one layer down, and it is **not fixed here** —
  measuring it needs the §9 desktop, so it belongs to wave C. Named as remaining debt rather
  than quietly left.

### Not claimed

- **No latency figure.** Nothing here was measured for speed; the only number this ADR asserts
  is the memory arithmetic, and the plan records the RSS reading that checks it.
- **Nothing about recovery.** What a restart replays is ADR-0039's question and is untouched:
  the file format does not change.
