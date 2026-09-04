# Why the message log is not the journal

`[2026-09-04]` Step 0 of [message-log](../plans/2026-09-03-message-log.md), opened by an
independent review of that plan. The question was fair and had never been answered in writing:
`journal.rs` already owns a ring, a writer thread, a file, and a durability policy, and it was
extended by [ADR-0046](../decisions/ADR-0046-the-ring-is-the-resend-store-and-a-replay-goes-in-batches.md)
the same week. **Why build a second one beside it?**

**Answer: keep two.** Not because the second is cheap, but because the first cannot hold what
the second is for. The four numbered questions the plan set are answered below, each against
code rather than against intent.

## 1. A journal record has no key for a frame that has no sequence number

Every method of the session-layer `Journal` trait is keyed by `seq: u32` —
`put(seq, bytes)`, `get(seq)`, `highest()`, `oldest()`, `mark_in(seq)`, `highest_in()`
(`crates/session/src/journal.rs`). `MemJournal` addresses `slots[(seq as usize) % N]`
(`crates/engine/src/journal.rs:149`) and compares the stored number back on read, because
`seq % N` collides every `N` (`journal.rs:159`).

The three things the log exists for have no sequence number at all:

| What the log must hold | Its sequence number |
|---|---|
| An inbound frame, before the session has judged it | not yet parsed |
| A garbage frame (`Cut::Garbage`) | none — the bytes are not a message |
| A frame refused pre-session (wrong `56=`, duplicate identity) | never assigned |

The on-disk format has already spent both of its spare key values on sentinels: `len == 0` is an
inbound mark (`journal.rs:288`) and `seq == 0` is an activity mark (`journal.rs:298`). Holding
the three rows above needs more sentinels **and a second addressing scheme inside one file** —
which is two structures sharing a file, not one structure.

## 2. It would put pre-session bytes inside a pure session's trait

`Journal` is a `crates/session` trait. Non-negotiable 2 says the session layer is pure: no
socket, no clock, no file. A refused frame is by construction one the session never sees —
`conn.rs` calls `refuse(self.rx.bytes(taken))` and returns `Turn::Gone` **before**
`session.received_with(...)` is reached. Merging the two paths means either the session learns
about bytes it must not know exist, or the engine writes into a session-owned trait behind the
session's back. The first breaks D1; the second gives one file two writers and one owner.

## 3. One file cannot serve two durability policies

`Durability::Fsync` blocks the engine thread **on purpose**, and its own doc comment calls it
"the one place non-negotiable 4 is traded away by the user rather than by the engine"
(`journal.rs`; `sync_data` at `:555`, `:599`, `:628`). The log must never fsync — it is a
diagnostic, and a syscall per message on the engine thread is exactly what D8 forbids.

A merged file would have to choose. Choosing `Fsync` makes a diagnostic cost a syscall on the
hot path. Choosing per-record durability means branching on record kind on the engine thread
before every write, inside the one loop that is not allowed to branch on anything.

## 4. The merge is the more expensive option, not the cheaper one

- **Two paths:** one new module reusing `ring.rs` — 212 lines, already shared with
  `RingDispatch`, already proven at 0 allocations. No new concept enters the codebase.
- **One path:** the on-disk record constants are referenced in **31 places in `journal.rs`
  alone**, and `Reader`, `Record`, `Records` and `tools/jrnl` all decode that format. Each
  would have to learn to tell six record kinds apart where it now tells three, and `jrnl`'s
  exit codes are a published contract.

## What the review got right that this does not overturn

The independent read also said the integrity effort was inverted: step 4 gives CRC32 to the
binary journal while the text log — the file actually opened during a dispute — gets nothing.
Half of that lands. The two files have different failure modes and want different answers:

| | Journal | Message log |
|---|---|---|
| Feared failure | a flipped byte replayed as a real message | a `kill -9` merging two lines into one |
| Right answer | CRC32 per record, step 4 | a torn tail marked at `open`, step 1 |
| Why not the other one | a torn tail is already handled (`journal.rs:396–424`) | a CRC is unreadable to `grep`, which is the whole interface |

So step 4 stays where it is, and the log's integrity answer is the torn-tail marker rather than
a checksum. **A file people read with `grep` cannot carry a checksum people will not check.**

## The general shape

`[to testing-skills]` A store's key is a stronger constraint than its storage. Two components
that both "append bytes to a file with a background writer" can look like duplicated
infrastructure and still be un-mergeable, because what separates them is what they can be
*addressed by* — here, a sequence number that one side's data does not have. The reusable check
is cheap: **before merging two stores, ask what the key of the merged store would be.** If the
answer needs a second addressing scheme or a new sentinel, they were never one store. The same
question also catches the inverse case, where two stores really should be one and nobody noticed
because they had different names.
