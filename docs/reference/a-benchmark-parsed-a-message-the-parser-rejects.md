# A benchmark parsed a message the parser rejects

> `[measured 2026-09-05]` — found while building the bench for `STATUS.md` open item 39,
> [plans/2026-09-02-the-baselines-and-the-pass-nobody-timed.md](../plans/2026-09-02-the-baselines-and-the-pass-nobody-timed.md)
> step 2b, Sửa 4. **`[to testing-skills]`**
>
> The fourth in the family: [a benchmark can delete its own
> work](a-benchmark-can-delete-its-own-work.md), [a benchmark measured its own
> fixture](a-benchmark-measured-its-own-fixture.md), [a benchmark that measures where the
> compiler put it](a-benchmark-that-measures-where-the-compiler-put-it.md). This one times the
> right function on the wrong input, and the input is wrong in a way that makes the function
> **do less work and return early**.

## What was wrong

`crates/codec/benches/parse.rs` had carried two hand-written FIX messages since the codec was
first measured. Both are malformed:

| Message | `9=` declared | `9=` actual | `10=` declared | `10=` actual | `parse_into::<_, 64>(…, Validation::ALL)` |
|---|---|---|---|---|---|
| `NewOrderSingle` | 126 | 126 | 098 | **097** | `Err(BadCheckSum)` |
| `Heartbeat` | **49** | **51** | 000 | **226** | `Err(BadBodyLength)` |

The bench threw the result away — `black_box(r).ok();` — so nothing ever said so.

## Why it mattered for one case and not the other

Both checks live at the **end** of `parse_into`, in the arm that runs when the loop reaches
`tag == 10`. By then every field has been read and pushed into the index, so a rejected parse
has already done nearly all the work a good one does. That is what makes this survivable at
all, and it is also why reading the code was necessary before claiming a size:

```rust
if v.body_length && pos != trailer_at {
    return Err(ParseError::BadBodyLength);   // <-- before the checksum
}
if v.check_sum {
    …
    if want != u32::from(checksum(&buf[..pos])) {
        return Err(ParseError::BadCheckSum);  // <-- after computing it
    }
}
```

- **`parse NewOrderSingle (validated)`** still computes the checksum over 148 bytes and only
  then fails the comparison. `[measured 2026-09-05]` correcting the fixture: 119.8 → 121.6 ·
  125.0 · 119.6 ns, **≈ +1%**, inside its own run-to-run spread.
- **`parse Heartbeat (validated)`** returns on the body-length line, which is **before** the
  checksum block, so this case had never summed its 51 bytes at all. `[measured 2026-09-05]`
  correcting the fixture: 59.1 → **64.2 · 61.7 · 60.1 ns**, and the first of those is over the
  61.9 ceiling of its recorded 1.10 band.

So a published figure of 56.3 ns was the cost of a parse that gives up one field before the
end. It had a committed benchmark, a named machine and §9 settings in force — non-negotiable
10 satisfied on all three counts — and it still did not measure what its name says.

## What found it, and it was not a review

Nothing in the suite could find this. The bench is stable to 1%, the case name is accurate
about the function it calls, and the diff that introduced it reads correctly: the message
*looks* like a `NewOrderSingle` because every field of it is one.

It was found by a **different benchmark, written later, that asserted its own fixture before
timing it**:

```rust
let r = parse_into::<Fix44, 64>(nos, &mut nos_idx, Validation::ALL);
assert!(matches!(r, Ok(Parsed::Complete { .. })), "NewOrderSingle {r:?}");
…
assert_eq!(validate(&nos_view, b"D"), None, "NewOrderSingle is clean");
```

Both assertions fired on the first run, on bytes copied verbatim from the older bench. The
second one caught a second layer: `52=00000000-00:00:00.000` is not a timestamp any dictionary
accepts, and the old bench never learned that because it parsed with `NoDict`, whose every
answer is a no-op.

## The general shape

**A fixture that is invalid in a way that makes the code under test bail out early produces a
benchmark that is stable, plausible, and too fast — and no gate can see it, because the gate
compares the figure against a baseline recorded from the same fixture.** Every run agrees.
Every subsequent run agrees. The band is tight *because* the work is small.

Three things make it specifically hard to notice:

1. **The error is discarded.** A benchmark has no natural place to care about a return value,
   and `black_box(r).ok()` reads as "keep the optimiser honest", not as "ignore a failure".
2. **The invalid part is at the end.** A message with a bad checksum is 99% of a good message,
   so nothing about the figure looks wrong.
3. **The fixture is shared by copy.** The same byte literal reached three bench files by being
   pasted, and a comment in each says it is "the same shape every other benchmark is measured
   on" — which was true, and was the problem.

**The cheap guard is one line per fixture: assert that the input is valid before timing
anything on it.** Not the output — the *input*. It costs one execution at startup and it is
the only thing here that would have failed.

## What it does not mean

It does not mean the parse figures were fabricated or that the parser is slower than published:
the NewOrderSingle case is within 1% of correct, and the codec is unchanged. It means one
published number — `parse Heartbeat (validated)` — was **7% low for a structural reason**, and
that the guard against this class did not exist until now.

`crates/engine/benches/dispatch.rs` carries the same byte literal and is **unaffected**: it
hands the bytes to `deliver` and never parses them, so the checksum is never read. Checked
rather than assumed.

## Related

- [a-benchmark-measured-its-own-fixture](a-benchmark-measured-its-own-fixture.md) — the timed
  region containing the scaffolding rather than the system. This one is the input rather than
  the region.
- [a-benchmark-can-delete-its-own-work](a-benchmark-can-delete-its-own-work.md) — the optimiser
  removing the work. Same symptom, a figure that is too fast and perfectly stable; different
  cause, and the doubling test that catches that one does **not** catch this one.
- [a-benchmark-that-measures-where-the-compiler-put-it](a-benchmark-that-measures-where-the-compiler-put-it.md)
  — the figure as a property of the binary.
- [measured-costs.md](measured-costs.md) — the §9 figures this corrects.
