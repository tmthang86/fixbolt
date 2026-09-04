# ADR-0008 — The journal is a trait the session is handed, not an action it emits

- **Status**: Accepted — 2026-08-30
- **Date**: 2026-08-30
- **Deciders**: Tran Manh Thang
- **Related**: [DESIGN.md §4 D1 and D7](../DESIGN.md),
  [ADR-0007](ADR-0007-spsc-ring-without-unsafe.md),
  [plans/2026-08-30-engine.md](../plans/2026-08-30-engine.md)

## Context

[DESIGN.md D1](../DESIGN.md) sketched the session emitting actions:

```rust
pub enum Action { Send { .. }, Deliver, Store { seq: SeqNum, range: Range<u16> }, Disconnect }
```

Everything else in that sketch came out narrower when it was written — `ActionBuf` became four
methods taking an `emit` closure, `Deliver` became the `Application` trait — and `Store` was
left as a debt: a ring of eight 512-byte slots **inside** `Outbound`, which D1 itself calls out
as the wrong crate.

The engine plan's step 6 says to move it, and says an ADR is required if the shape differs from
`Action::Store`. It does, in two ways.

## Decision

**1. The journal is a trait the caller supplies, like `Application`.**

```rust
pub trait Journal {
    fn put(&mut self, seq: u32, bytes: &[u8]);
    fn get(&self, seq: u32) -> Option<&[u8]>;
}
```

`Session::received_with` and `Session::send_application` take `&mut J`. `Session::received`
passes `NoJournal`, so a caller using the session as a pure protocol machine is unchanged.

**An emitted `Action::Store` cannot work, and the reason is `get`.** A resend has to *read*.
The session must ask "do you still hold `34=n`?" and get an answer in the same call, because
what it does next — replay it, or fill over it — depends on the answer. An action is a
one-way statement; this needs a question. Once `get` is a method on something the caller
supplies, `put` belongs beside it.

**2. `FileJournal` is a plain appended file, not a memory-mapped one.**

D7 says "memory-mapped journal". `memmap2` is a dependency and hand-rolled `mmap` is `unsafe`,
and the engine plan authorises neither — the same fork ADR-0007 documents for the dispatch
ring. So: `std::fs::File`, appended.

- `Durability::Fsync` writes and `sync_data`s inline, **blocking the engine thread on purpose**.
  It is the one place non-negotiable 4 is traded away by the user rather than by the engine, and
  a deployment required to fsync is buying exactly that.
- `Durability::Async` — D7's default — pushes the bytes into an [ADR-0007](ADR-0007-spsc-ring-without-unsafe.md)
  ring and a writer thread does the I/O. Nothing blocks.

**3. A file journal still keeps the ring in memory, and answers `get` from it.**

Reading a replay back off disk would be a blocking `read` on the engine thread. Memory index,
durable log — the shape every real engine uses.

## Consequences

**Good**

- The session holds no bytes it did not generate, and `out::Outbound` lost a 4 KiB field. D1's
  debt is paid: the session says *keep this* and asks *do you still have it*.
- The policy is the deployment's, per connection, and the three D7 tiers are three types rather
  than three branches: `NoJournal`, `MemJournal`, `FileJournal`.
- `[measured 2026-08-30]` the session's allocation bench is still **0 on all thirteen paths**
  and the acceptance score is still **59 / 59** — the move changed where the bytes live and
  nothing else. Reversal: making `MemJournal::put` keep nothing turns four journal tests red
  **and drops the acceptance score**, which is what proves the score depends on it.

**Bad, and stated plainly**

- **Two public signatures grew a parameter.** `received_with` and `send_application` take a
  journal. Every caller had to change, including the acceptance score adapter and two
  benchmarks. That is the price of the session not owning the store, and it is paid once.
- **A file journal is not read back on startup.** Nothing recovers a session's outbound
  sequence number or its unacknowledged messages from the log. The file is written and never
  read, which makes `Fsync` today an audit trail rather than a recovery mechanism. Recovery
  needs the session to be *constructible from* a journal, which is a session-layer change and
  a plan of its own. **Named here so it is not mistaken for done.**
- **`Async` has no way to say when the disk has it** except `close()`. That is what "off the
  hot path" costs, and it is the correct trade for the default.
- **No `Engine`-level factory for per-connection file journals.** `Engine::add` uses
  `J::default()`; a deployment that wants one file per session calls `add_with_journal`. A
  factory trait would be a fourth type parameter on an already eight-parameter type, and no
  caller has asked for one yet.

---

## `[sửa 2026-09-04]` — the on-disk format gains a header and a per-record checksum

**A dated note, not a revision.** Nothing above is edited: every sentence of the accepted
decision still holds, the trait is unchanged, and `Durability`'s two variants mean what they
meant. What this adds is the shape of the bytes `Durability` writes, which the ADR never
pinned down and which the *Consequences* above turn out to depend on.

**The defect.** A record is `seq(4) ‖ len(4) ‖ bytes`, and there is nothing in it that a
flipped byte disagrees with. `[2026-09-04]` a journal whose payload was changed after it was
written read back as a whole, correctly framed, correctly numbered record and would be replayed
to a counterparty as though it were the message that was sent. The torn-tail accounting the
*Consequences* rely on catches a process **killed mid-write**; it cannot catch a file that was
**changed**, because a changed record is still exactly as long as it says it is.

**The change.** A file that does not exist yet is created with a five-byte header, `FXBJ\x01`,
and every record in it carries a CRC32 (IEEE, from a 256-entry table built by a `const fn` —
**no dependency**) over `seq ‖ len ‖ bytes`. A record whose checksum does not match is treated
**exactly as a torn tail is**: the read stops there, everything before it stands, and the count
is published as `FileJournal::corrupt_records` and `Reader::corrupt_records`. `tools/jrnl`
warns and exits **2**, as it already did for a tear.

**A file without the header is version 0 and is read exactly as it always was**, and it stays
version 0 for as long as it is appended to. A file whose first half has no checksums and whose
second half does is a file no reader can parse without guessing where the change happened. The
compatibility guarantee is asserted against **bytes committed in the test**, produced by
`df94c08` — the commit before this note — because a fixture regenerated with the new code would
only assert that the new code agrees with itself.

**Where the checksum is computed, and what it costs.** `Async` computes it on the **writer
thread**, which is the whole point of that variant: a CRC over a 200-byte record is ~100 ns and
that thread exists to hold work like it. `Fsync` computes it on the engine thread, over the
pieces rather than a joined buffer, because joining would allocate where nothing may — and it
is the smaller half of a cost that already includes `sync_data`. `[unmeasured]`: no benchmark
here times the CRC, only that it allocates nothing.

**What this does not change.** `Durability::Async` still has no way to say when the disk has
it. A file journal is read back on startup — that changed in ADR-0034, not here. And a
checksum says a record is **as it was written**; it says nothing about whether writing it was
the right thing to do.

**One behaviour changed for a reader**: `Reader::is_empty` now means *no records and no torn
tail* rather than *zero bytes*, because a version-1 journal that was opened and never written
to is five bytes of header, and a session that has sent nothing must not read as a file with
something in it.
