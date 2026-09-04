# A background thread wins the race your test was measuring

`[measured 2026-09-04]` Two false greens, both found in one afternoon while building the
message log, both in checks that looked like textbook reversals. `[to testing-skills]`

## Case 1 — the guard was deleted and the test stayed green

`FileLog` writes through a ring to a writer thread. `close()` sends a stop record and joins;
`impl Drop` calls `close()`. The obvious test:

```rust
{
    let mut log = FileLog::open(path)?;
    log.record(...);
    // no close(); Drop is the point
}
assert_eq!(lines(path).len(), 1);
```

**Delete `impl Drop` and it still passes.** The writer thread is detached, not stopped: it keeps
draining the ring and flushes as soon as the ring runs dry, which happens in microseconds. The
line reaches the file either way. The assertion was not measuring `Drop`; it was measuring
whether a background thread could finish before the main thread read the file, and it could,
every time.

What `close()` uniquely does is **end the writer**. So make that the assertion. The writer owns a
clone of the shared loss counter, so the counter's strong count observes the thread's lifetime
with no timing at all:

```rust
let held;
{
    let mut log = FileLog::open(path)?;
    held = log.counter();           // log + writer + test == 3
    log.record(...);
}
assert_eq!(Arc::strong_count(&held), 1);   // only the test is left
```

With `Drop`: `left: 1`. Without it: `left: 2, right: 1` — red, deterministically, on the first
run and every run.

**The shape.** When the thing under test is *"X gets shut down"*, asserting on X's **output** is
usually a race, because the output was already produced before shutdown was due. Assert on X's
**lifetime** instead. A shared handle's reference count, a completion flag the worker sets on the
way out, a channel that closes — any of them turns a timing question into an ownership question.

Corollary worth stating on its own: **a reversal that leaves the test green has told you
something, and it is not "the code is fine".** It has told you the test does not test that. That
is the whole value of running reversals rather than reasoning about them.

## Case 2 — the reversal that changed nothing

The first attempt at the newline-escaping reversal used a regular expression to delete the
escape rule. The expression matched a **different** function — the decoder, several hundred lines
away, where the same byte literal appears for the opposite reason — so the escape rule was never
removed. The test passed. Written down as *"reversal confirmed, guard proven"*, that would have
been a lie produced entirely by a bad pattern.

**The fix is one line of discipline: a reversal must prove it changed the file.** Every reversal
here now asserts its own patch applied before the test runs, and prints what it removed:

```
assert old in source, "escape_into arms not found"
```

Two of six reversals were caught by that assertion on the first run.

## Case 3 — the search that proved the absence of something present

While reviewing the plan, two separate searches for `impl Drop` reported that `FileJournal` had
none, and the review nearly recommended adding a duplicate. The implementation reads:

```rust
impl<const N: usize, const LEN: usize> Drop for FileJournal<N, LEN> {
```

The literal string `impl Drop` does not occur in it. Every generic implementation in the language
breaks that pattern, and the search returning nothing reads exactly like the search returning a
true negative.

**Searching for a declaration by its simplest spelling proves nothing about generic code.** Search
for the part that cannot move — here `Drop for` — or ask the language server, which knows what a
trait implementation is. The same trap already cost this repository once, with `nm -u` over
generic code (`docs/reference/`), and it will keep costing until the reflex is *"what spelling
would a generic version have?"*

## What all three have in common

Each one produced a **green result that was never observed to be capable of being red**. That is
the same sentence `CLAUDE.md` §10 ends with, and these are three concrete instances of it:

| | The check | Why it could not fail |
|---|---|---|
| 1 | `Drop` writes the queued line | a detached thread wrote it anyway |
| 2 | escaping is required | the patch removing it hit the wrong function |
| 3 | `FileJournal` needs a `Drop` | the search could not match a generic `impl` |

None of the three is about FIX, files, or Rust in particular. They are about the moment between
*"I ran the check"* and *"the check means what I think"*, which is where every one of them lived.
