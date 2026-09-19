# A bitset keyed by tag scales with the highest tag, not the field count

`[measured 2026-09-19]` on the cloud Linux box (**not** the `DESIGN.md` §9 desk — the seconds
below are a ratio, not a published figure).

## The prediction, and the measurement

[ADR-0080](../decisions/ADR-0080-the-dictionary-rides-the-encoding-and-a-fixt-session-is-one-table-built-from-two-xml-files.md)
wrote down the cost of the second dictionary before it existed:

> **A second table doubles `dict`'s build time and binary** when `fix50sp2` is on; measured
> when it lands.

It landed at B1. Measured, three cold builds each (`cargo clean -p fixbolt-dict` between runs):

| | FIX 4.4 only | `--features fix50sp2` | Ratio |
|---|---|---|---|
| generated file | 156 620 B, 3 725 lines | 4 010 039 B, 42 256 lines | **25.6×** bytes |

`[re-measured 2026-09-19 at 69fb801]` the row first read 156 397 / 3 721 and 4 008 198 /
42 221, measured at B1. B4a and B4b then grew both files — `TRANSPORT_DEFINED_TAGS` and the
per-token enum tables — and the row was not re-measured in the same commit, which §4 asks
for. The **ratio is unchanged** at 25.6×, so nothing this page concludes moves.
| cold build of the crate | 0.56 s | 5.25 s | **9.4×** (+4.7 s) |

Not 2×. The estimate was not lazy — it is what "one more table of the same shape" predicts, and
every other table in the file *is* about twice the size. One table is not of the same shape.

## Why

`ALLOWED` is a bitset over `0..=max_tag`, one row per message type, chosen (the comment in
`crates/dict/build.rs` says so) because a bitset beats a binary search over up to 300 tags and
the session asks once per field of every message it validates. Its size is

```
messages × ceil((max_tag + 1) / 64) words
```

and **`max_tag` is not the number of fields**. FIX 4.4 stops at tag 956, so 15 words. FIX 5.0
SP2's highest tag is **50002**, so 782 words — while carrying 6 028 fields, only about 6.6× as
many as FIX 4.4's 912. The sparse top of the tag space costs the same as a dense one:

| | messages | words/row | table |
|---|---|---|---|
| FIX 4.4 | 93 | 15 | 0.011 MB |
| FIXT 1.1 + FIX 5.0 SP2 | 164 | 782 | **1.03 MB** |

That 1.03 MB is the bitset's own bytes; it reaches 4 MB of source because each word is emitted
as `0x…` hex text, sixteen characters plus a separator for eight bytes. `DEFINED_TAGS` shares
`words` with `ALLOWED` by construction, so it grows too, but it is one row, not 164.

The lookup stays O(1) and the runtime table is still a `&'static` — **nothing on the hot path
got slower**, and no allocation was added. The cost is entirely build time and binary size.

## What this does and does not justify

It is **not** a defect and nothing was changed in response: the feature is off by default, so a
FIX 4.4 user pays none of it, and correctness is unaffected. What it changes is the honesty of
the sentence in ADR-0080, which is why the number is written here rather than left as "about
double".

It does put a real question on the table for whoever revisits the dictionary's shape: at 782
words per message, a sorted tag list with a binary search — the thing the bitset was chosen
over — costs about 300 × 4 bytes per message against 6 256, and the comparison that justified
the bitset was made against FIX 4.4's numbers, not SP2's. A hybrid (bitset up to a dense
cutoff, list above it) is the obvious third option. **None of that is phase 2's**, and no
measurement here says the bitset is the wrong choice at run time — only that the build-time
arithmetic behind it was done for a tag space eight times smaller.

## What guards it

Nothing, and nothing should: this is a cost, not an invariant. The numbers are here so the next
person who reads ADR-0080's "doubles" does not budget from it. If the dictionary's shape is
revisited, the row to beat is the table above.

## Related

- [ADR-0080](../decisions/ADR-0080-the-dictionary-rides-the-encoding-and-a-fixt-session-is-one-table-built-from-two-xml-files.md)
  decision 2 and its *Bad — and accepted* list, which this measures.
- [ADR-0083](../decisions/ADR-0083-ten-field-types-one-field-spelled-two-ways-one-empty-component-and-where-the-sp2-oracle-comes-from.md)
  — what the pair build had to decide before it could emit anything.
- [fixt-dictionary-traps](fixt-dictionary-traps.md) — the six traps in the pair itself.
