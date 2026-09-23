# Masking a DATA field to the next SOH under-masks

`[2026-09-23]` ADR-0110 decision 2, phase 3 row 3
([2026-09-23-p3-redact-secrets](../plans/2026-09-23-p3-redact-secrets.md)). `redact::mask` and
`redact::carries_secret` find a field's value by splitting the message on SOH, without a
dictionary: a STRING value cannot legally contain an SOH, so every real `554=` or `925=`
begins right after one and ends at the next. `96` RawData is not a STRING — it is a DATA field,
and [DESIGN.md D3](../DESIGN.md#d3--field-ordering-comes-from-generated-tables-never-from-hand-written-code)
already says why one exists: *"a DATA value may legally contain `0x01`"*. Splitting a DATA
field's value on the same byte that legally sits inside it reads only the first fragment as the
secret and leaves the rest — which can be the tail half of the very credential being masked —
on disk exactly as it arrived.

## What almost shipped

The first shape of the scanner treated every field the same way: find the tag, find `=`, mask to
the next SOH. Against `LOGON` in `crates/engine/tests/redact.rs` —

```text
95=14|96=raw1|raw2-tail|98=0|...|554=hunter2|...
```

— `96`'s *declared* length is 14, which reaches past the SOH sitting between `raw1` and
`raw2-tail`. Masking only to that first SOH stars `raw1` and leaves `raw2-tail` — half the
RawData value — in clear, and worse, leaves the scan resuming in the middle of a field rather
than at a boundary. `password` and `925` never hit this, because STRING truly cannot contain the
delimiter; only the three DATA secrets (`96`, `1402`, `1404`) can.

## The rule chosen instead

**Over-mask, never under-mask** (ADR-0110 decision 2, `CLAUDE.md` §4). The scan tracks the most
recent value of each DATA secret's LENGTH field (`95`, `1401`, `1403`) as it passes it, and masks
the DATA field that follows over **the larger of** that declared length and the distance to the
next SOH — clamped to the end of the buffer, so a length larger than what remains cannot walk
past it or panic. Concretely:

- A RawData holding an SOH is masked **whole**, embedded SOH included, because the declared
  length is the larger bound.
- A **lying** length — larger than the field actually is — masks further into the message than
  the true field, which is over-masking, not under-masking, and the scan resumes at the next SOH
  after the declared end so it does not restart mid-field.
- A length **smaller** than the run to the next real SOH still masks all the way to that SOH, the
  larger of the two bounds again.

None of this needs `fixbolt_dict`: the three LENGTH→DATA pairs are hard-coded in `MASKED` and
pinned against the dictionary's own tables by a test, so a dictionary change cannot move them
silently underneath the scanner.

## What proves it

- `crates/engine/tests/redact.rs::a_raw_data_holding_an_soh_is_masked_whole` — the exact `LOGON`
  fixture above: all 14 declared bytes of `96=`, the embedded SOH included, come back as `*`,
  and `raw2-tail` does not survive anywhere in the buffer.
- `crates/engine/tests/redact.rs::a_declared_length_past_the_end_is_clamped_not_a_panic` — a
  `95=` bigger than the whole remaining buffer masks to the end and does not panic or read past
  it.
- `crates/engine/tests/redact.rs::a_declared_length_that_lands_mid_field_masks_to_the_next_boundary`
  and `::a_declared_length_shorter_than_the_value_still_masks_to_the_soh` — both directions of
  "declared length disagrees with the SOH", each resolved by taking the larger bound.
- `crates/engine/tests/redact.rs::every_prefix_of_a_logon_is_scanned_without_a_panic` — every
  prefix and suffix of a real Logon, including ones that cut a DATA field or its length field in
  half, scanned without a panic.
- `crates/engine/tests/secrets_stay_off_disk.rs` drives a real `Logon` whose `96=` carries an
  embedded SOH through `FileLog` over a socket and asserts no fragment of that RawData value, on
  either side of the SOH, reaches the file.
- **Reversal R4** (`docs/plans/2026-09-23-p3-redact-secrets.md`, *Chia việc* row 3): masking a
  DATA field only to the next SOH, dropping the declared length, was shown red against exactly
  this case before being restored.

## The general shape

A delimiter-based scanner is only as sound as the guarantee that the delimiter cannot occur
inside the thing it is used to bound. Where the format's own rule says it can — a DATA field, by
design, unlike a STRING field, by the same design — a length carried elsewhere in the message
must be allowed to win over the delimiter, and where the two disagree the safe answer is not
"trust the length" or "trust the delimiter" but **whichever bound is larger**, so a malformed or
adversarial length can only make the code mask more than it needs to, never less.
