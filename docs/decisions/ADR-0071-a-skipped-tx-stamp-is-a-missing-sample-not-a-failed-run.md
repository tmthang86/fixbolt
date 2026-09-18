# ADR-0071 — A skipped TX stamp is a missing sample, not a failed run

- **Status**: Proposed — 2026-09-18
- **Date**: 2026-09-18
- **Deciders**: Tran Manh Thang
- **Related**: [ADR-0068](ADR-0068-a-published-figure-is-two-procedures-shown-side-by-side.md),
  `DESIGN.md` §6 *Wire-to-wire, NIC to NIC*, §9 EEE row,
  [plans/2026-09-04-the-second-linux-desk.md](../plans/2026-09-04-the-second-linux-desk.md)
  Sửa 2 Điều 4 (c), `STATUS.md` open item 40
- **Answers**: item 40's *"what closes it now is a decision, not a cable: accept runs with a
  counted missing stamp, pace just enough for one stamp at a time, or a NIC with more TX
  timestamp slots"*

## Context

`[measured 2026-09-15]` boot B read the first hardware-stamped wire figures on `enp9s0` (Intel
I211, `igb`), paced at one message a second: `hft` admin p50 45 146 ‖ 42 918 ns, application
49 626 ‖ 45 082, `hw-tx-missing 0` in every published run. Back to back (interval 0) a TX stamp
was skipped within 1–4 runs of 20 000 every time, `tx_hwtstamp_skipped` climbed 0 → 52 over the
morning, and `scripts/w2w-baseline.sh` FAILs a run with any missing stamp — so no interval-0
figure exists, only diagnostic p50s of 26 178–26 218 ns from the runs that happened to complete.

The cause is the driver, verified in Sửa 2 Điều 4 (c) and by the search of 2026-09-18:
`igb_xmit_frame_ring` admits one TX timestamp request at a time (`__IGB_PTP_TX_IN_PROGRESS`,
`ptp_tx_skb`) and counts every other one in `tx_hwtstamp_skipped`; the August 2026 patch set
on intel-wired-lan describes the I210/I211 as having *"the single timestamp slot"* and
*"requests are only counted as skipped when hardware timestamping is enabled and the slot is
occupied"*. The I225/I226 (`igc`) has **four** TX timestamp registers (`IGC_MAX_TX_TSTAMP_REGS`,
in mainline since the 2023 patch *"igc: enable multiple TXSTMP registers"*), and its list
discussion puts the four-slot ceiling at roughly 65 000 stamps a second. At this engine's
~26 µs round trip a stamp is outstanding for the time the observer takes to drain the error
queue; one slot is exactly one round trip's worth of margin, four slots are four.

A skipped stamp is **not correlated with the engine being slow on that request**: the slot is
busy because the *previous* stamp has not been read yet, which is the observer's lag, not the
acceptor's. Dropping that sample therefore does not bias the acceptor's distribution the way
dropping a slow sample would — but it is a claim about the mechanism, and decision 3 makes it
checkable.

## Decision

1. **A run with skipped stamps is a valid run whose sample count is smaller.** `tools/w2w
   --wire-timestamps` already pairs by TCP byte offset so one missing stamp costs one sample;
   `scripts/w2w-baseline.sh` stops FAILing on `hw-tx-missing > 0` and instead **FAILs when the
   missing count exceeds 0.1% of the timed requests** (20 of 20 000) or when
   `hw-rx-missing > 0` (an RX stamp never competes for a slot; a missing one is a real fault).
   The missing count is printed beside every percentile of every run and in the summary, and
   `tx_hwtstamp_skipped` before/after the run is printed with it; the two must agree or the run
   is marked.
2. **The published NIC figure is the interval-0 pair**, per ADR-0068, with its missing counts
   in the table. The paced figure at 1 s stays published beside it as the *cold* figure it
   already is (`DESIGN.md` §8 *Boot B*), never as a substitute.
3. **The check that dropping the sample did not move the distribution is a join of the two
   halves' per-sample dumps, by request index.** `[revised 2026-09-18, step 2.2's finding]`
   As first written this decision asked `w2w --listen` for a counterparty table it cannot
   have: the halves are two processes with no channel, `--connect` prints percentiles only,
   and the acceptor's own table has no value for an unstamped request. Buildable form: both
   halves gain `--dump <file>`, written **after** the timed window from data they already hold.
   `--connect` writes one line per timed request — index (send order after warmup), round-trip
   ns on its clock. `--listen --wire-timestamps` writes one line per timed request — index
   (rank of the request's TCP byte offset after the acceptor's own `--warmup`, so the two
   indexes agree by construction), `stamped`/`missing`, wire ns when stamped. The baseline
   script fetches the generator's dump (`scp` when `GENERATOR_SSH`), joins on index, and
   computes the generator's p50 / p99 / p99.9 twice — over every row and over the rows the
   acceptor stamped. **The run is published only if the two agree within 1% at p50 and 5% at
   p99 and p99.9** (chosen, like the 0.1%: a stamp skipped for observer lag is uncorrelated with
   the request's own RTT, so the stamped subset should read as the whole to within sampling
   noise; the slowest 20 of 20 000 going missing would move p99.9 by far more than 5%).
   Disagreement marks the run, per ADR-0068 decision 3. The join is a pure function with a test
   fed a synthetic dump. The generator's noise (an unpinned Mac) cancels: the check compares the
   same process's samples against a subset of themselves.
4. **A NIC with more slots is the right next purchase, and it is named**: an I225/I226
   (`igc`, four TX stamp registers) is the cheapest card with a PHC that removes the
   single-slot limit; it is not required to close item 40 and is not bought under this ADR.
5. **The sweep that finds where skips stop is a desk measurement, not a rule**: interval 0,
   10, 20, 30, 50 µs on the I211, one procedure of 10 runs each, A/B by ADR-0068 decision 4,
   recorded in `measured-costs.md` so the 0.1% threshold has a measured neighbour.

## Consequences

**Good**

- Item 40's row can be met at the rate §6 asks for, on the NIC that exists, without a
  purchase and without a change to `crates/`.
- The missing count is visible on every published number; nobody has to trust that it was
  small.
- Decision 3 makes the "unbiased drop" claim falsifiable per run, which is more than most
  timestamping setups offer.

**Bad — and accepted**

- **0.1% is a chosen number**, like ADR-0068's 5%. It is small enough that a bimodal tail
  cannot hide in it (p99.9 is the 20th slowest of 20 000; losing 20 samples moves it by at
  most one rank), and nothing measured put it there.
- **The Mac dump is a software clock on a different machine.** It is a check on the *shape*
  of the drop, not a second wire figure; its 1% / 5% bands are chosen, not measured.
- **Two dumps and a join is more machinery for a check that should usually pass**, and a
  request index is a convention two binaries must keep in step — a `--warmup` mismatch
  between halves joins the wrong rows; the script passes one value to both and the dump header
  records it.
- **Two scripts change behaviour on a FAIL they used to raise**; the old strictness was itself
  a decision (Sửa 2), and a reader of boot B's log will see a rule reversed within a week. The
  reversal is recorded here rather than silently in the script.
- **The sweep (decision 5) costs desk time** in a boot that already has more arms than hours.

## Sources

- `STATUS.md` item 40 `[measured 2026-09-15]`; `DESIGN.md` §6 NIC row and §8 *Boot B*.
- Intel `igb` driver, `igb_xmit_frame_ring` (`__IGB_PTP_TX_IN_PROGRESS`, `tx_hwtstamp_skipped`)
  — <https://github.com/torvalds/linux/blob/master/drivers/net/ethernet/intel/igb/igb_ptp.c>;
  the August 2026 series *igb: PTP Tx timestamp state fixes* —
  <https://ratatoskr.run/intel-wired-lan/2026/08/17415163/t> (read 2026-09-18).
- `igc`: *enable multiple TXSTMP registers reporting TX timestamp* —
  <https://lore.kernel.org/netdev/20230423075312.544244-1-xiaoyan.gong@intel.com/T/>, and the
  four-slot discussion on the AF_XDP TX timestamp series (read 2026-09-18).
