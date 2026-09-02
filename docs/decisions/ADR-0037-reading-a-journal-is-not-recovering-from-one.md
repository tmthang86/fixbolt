# ADR-0037 — Reading a journal is not recovering from one

- **Status:** Accepted
- **Date:** 2026-09-02
- **Related:** [ADR-0010](ADR-0010-a-reconnect-is-not-a-restart.md),
  [ADR-0017](ADR-0017-the-inbound-count-is-persisted-after-delivery.md),
  [ADR-0027](ADR-0027-the-engine-owes-a-byte-stream-not-an-archive.md),
  [ADR-0034](ADR-0034-recovery-is-asked-once-the-counterparty-is-known.md)
- **Plan:** [2026-09-02-what-the-journal-can-answer.md](../plans/2026-09-02-what-the-journal-can-answer.md)
- **Closes:** `STATUS.md` open item 30 (e)

## Context

**The question every operations desk is asked:** *"we sent order X at 10:32, did you receive
it?"* `[verified 2026-09-02]` the answer here was *"I don't know."*

The file format was never the obstacle — it is `[seq:u32-le][len:u32-le][bytes]`, with a length
of zero meaning [ADR-0017](ADR-0017-the-inbound-count-is-persisted-after-delivery.md)'s inbound
mark, and `[measured 2026-08-30]` the length field was added specifically so the file could be
parsed rather than only appended to.

The obstacle was that **the only thing that read it was `FileJournal::open`**, which loads the
file into a fixed ring of `N` messages. That is right for what `FileJournal` is for — answering
the next `ResendRequest`, which is about recent traffic. It is wrong for a question about a
message from three weeks ago, which the ring dropped long ago. And opening it at all required a
Rust process that knew the right `N` and `LEN`; the person on call at 3 a.m. has none of the
three.

## Decision

### 1. Offline reading is an `Iterator`, not a ring

`journal::Reader` reads the whole file and hands back every record in order. No `N`, no `LEN`,
no bound — because the question is about the whole history rather than the recent end.

**It does not reuse `FileJournal`.** Recovery and lookup are different purposes with different
shapes, and ADR-0027 already drew this line: the engine owes a byte stream, not an archive.
Bending the recovery type into a lookup tool would give both jobs to one thing that is well
suited to neither.

### 2. It allocates, and that is allowed — stated where it can be read

Non-negotiable 1 forbids allocation **on the hot path**. Nothing here runs on the engine thread
or on any path the engine takes, and the rustdoc says so at the type rather than leaving the
next reader to infer it. A file too large to hold in memory is a real limit; it is named in
`GUIDE.md` rather than worked around by a design nobody has needed yet.

### 3. It does not interpret FIX

Records come back as bytes. Interpreting them needs a dictionary, and a program whose job is to
read a file has no business pulling one in. `tools/jrnl` shows `SOH` as `|` and lets `grep` do
the rest.

### 4. A binary, because that is what the complaint was

*"Nothing outside the process can read it"* is not answered by a library function. `tools/jrnl`
is a crate with one dependency, `fixbolt-engine` with `default-features = false` — so
`scripts/check-no-optional-deps.sh` cannot be quietly defeated by a sibling crate, which
`[measured 2026-08-30]` is exactly how that gate was once green about a build that never
happened.

It has **no `[features]` block**, unlike `tools/w2w`. `w2w` needs one because it branches on
`#[cfg(feature = "standard")]` and a `cfg` never reaches into a dependency's features. Nothing
in `jrnl` branches on a feature, and declaring one would be a lie the compiler cannot catch.

### 5. A torn tail is reported, in both directions

`[2026-09-02]` `FileJournal::open` counted torn trailing bytes into a local called `torn` and
then did `let _ = torn;`.

**Skipping those bytes is correct**: replaying bytes that never went on the wire is worse than
replaying nothing, because a gap fill is a legal answer to a `ResendRequest` and a corrupt
message is not. **Being silent about it was a defect.** A process killed mid-write left no trace
an operator could find, in the one file that exists to answer questions about what happened.

So `FileJournal::torn_tail_bytes()` is readable, `Reader::torn_tail_bytes()` is its counterpart,
and `tools/jrnl` warns on stderr **and exits 2** — because an answer of *"no, we never received
it"* drawn from a damaged file might be wrong, and a script that checks only the exit status
must not read success.

## Consequences

**Good**

- The operations question is answerable, by running a program, by somebody who knows neither
  Rust nor the ring's parameters.
- `[measured 2026-09-02]` the three reversals fail on three different tests: a reader that stops
  silently at a torn tail, one that reads an inbound mark as an empty message, and a
  `torn_tail_bytes` stuck at zero.
- The torn-tail fix improves the **engine's** honesty too, not only the tool's.

**Bad, and named**

- **The whole file is read into memory.** A journal larger than RAM cannot be read by this tool
  at all. Streaming would fix it and was not built, because nothing has needed it.
- **It cannot read a file the engine is currently appending to** in any defined way. It will see
  a consistent prefix and probably report a torn tail; that is not tested and not promised.
- **Nothing correlates a record to a counterparty.** One journal per session is the convention,
  and the file itself carries no identity — so *which* journal to read is knowledge that lives
  outside it.
- **`jrnl` has no way to search by content**, only by sequence number. `grep` on the dump is the
  answer, and it is a worse answer for a large file than a `--grep` flag would be.
- **The torn count describes the file as it was opened.** A `FileJournal` that is appended to for
  a week still reports the tail it found on the day it opened.
- **The exit code 2 is a convention with nothing enforcing it** beyond one test.

## Alternatives rejected

| Alternative | Why not |
|---|---|
| Widen `FileJournal`'s ring so it can answer old questions | The ring is bounded by design (non-negotiable 1) and sizing it for history defeats the point |
| Give `FileJournal` a `scan()` method | Two jobs in one type: one runs beside the engine and must stay bounded, the other is a tool that may allocate freely |
| Make the reader stream instead of loading | More code for a limit nobody has hit. The limit is named in `GUIDE.md`, which is what makes revisiting it cheap |
| Have `jrnl` decode FIX and pretty-print fields | Needs the dictionary. `grep` covers the actual use, and the dependency would outlive the convenience |
| Keep skipping the torn tail silently | It is the file that answers *"what happened?"*, and a silent loss there is the worst place for one |
| Fail loudly on a torn tail instead of continuing | Then a damaged file answers nothing, when it can still answer everything before the tear |
