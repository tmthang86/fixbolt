# ADR-0023 — §9 records the CPU mitigation state, and `--strict` refuses a machine that has them off

> **Status:** **Accepted — 2026-09-01.** Adds a row to `DESIGN.md` §9 and to
> `scripts/check-machine.sh`. Does not amend an existing ADR.
>
> **Accepted by standing delegation**, `[2026-08-30]`.

- **Date**: 2026-09-01
- **Deciders**: Tran Manh Thang
- **Related**: [ADR-0021](ADR-0021-nohz-full-leaves-section-9.md), `DESIGN.md` §8 and §9,
  `CLAUDE.md` §2 non-negotiable 10,
  [measured-costs.md](../reference/measured-costs.md),
  [plans/2026-09-01-what-mitigations-cost.md](../plans/2026-09-01-what-mitigations-cost.md),
  `STATUS.md` open item 22

## Context

`[measured 2026-09-01]` disabling the CPU speculation mitigations makes every syscall this
engine performs **59–63% cheaper**. `engine turn, 1 idle sessions` goes from **448.9 ns to
175.2**. Thirteen pure user-space benchmarks move −4.1% to +4.1% with no direction, so it is
the kernel boundary and nothing else.

Three boots attributed it: `vmscape` — the mechanism `STATUS.md` had named since 2026-08-30 —
costs **nothing**, and the whole of it is `retbleed`'s untrained return thunk plus
`spec_rstack_overflow`'s Safe RET.

**And `scripts/check-machine.sh` read `pass 11 fail 0 unknown 1` in all three arms.** §9 had
no row for mitigations, so a machine with every one of them disabled reported as *"§9
satisfied. Latency numbers from this machine carry their settings."*

That is the identical hole ADR-0021 closed for `nohz_full`: a setting outside the checklist,
sitting on the operation §8 calls dominant, large enough that two machines both passing §9
are not comparable. It was found the same way — by measuring something the checklist never
mentioned.

## Decision

**1. §9 gains a row: CPU mitigations must be ON for a number to be publishable.** Not off.
The row **PASSes when the machine is mitigated** and FAILs when it is not.

**2. The direction is deliberate and is not a security recommendation.** It would have been
possible to make the row report-only, or to make it pass either way. Both were rejected:

- `benches/baselines.tsv` was recorded on a mitigated machine. A machine with them off reads
  60% under every syscall-bound baseline, which **passes** — a baseline is a ceiling — so the
  bench gate cannot catch this on its own. Something must.
- Non-negotiable 10 says a number carries the machine and the §9 settings. A configuration
  that changes the dominant term by 61% and is not in §9 makes that promise untrue.

So `--strict` refuses to publish from a machine whose mitigation state does not match the one
the baselines came from. **This ADR takes no position on whether anything should run with
mitigations disabled.** It says only that a latency figure measured that way is not comparable
to the ones in this repository, and must not be published as though it were.

**3. The row reads `/sys/devices/system/cpu/vulnerabilities/` and names what it found.** Not
`/proc/cmdline`: the command line says what was *asked for*, the sysfs files say what the
kernel is *doing*, and `[measured 2026-09-01]` those differ — `retbleed=off` also removed
`STIBP: always-on` from `spectre_v2`'s line, which no reading of the command line would show.
The check counts entries reading `Vulnerable` for a vulnerability this CPU is affected by.

**4. The failure message carries the measured price**, so the reader learns why the row exists
without going anywhere: 61% of every syscall, and which two mitigations it is.

**5. Existing `baselines.tsv` rows keep their `pass 11` verdict.** They were recorded under
this ADR's *behaviour* — mitigations on — and only the checklist's arithmetic changed. New
rows record `pass 12`. The file's header already explains that the pass count is part of the
record and not a constant.

## Consequences

**Good**

- A machine that cannot produce comparable numbers now says so, instead of printing *"§9
  satisfied"*. That sentence was false in two of this experiment's three arms.
- §8's dominant row gains an attribution it never had: **the largest single term in this
  design's budget is a security mitigation**, not the syscall itself. `recv` on this machine
  is 420 ns mitigated and 157 ns not.
- The next person to wonder why their fork is three times faster has the answer in the
  checklist rather than in a two-day experiment.

**Bad, and named**

- **A gate that fails a machine for being secure reads badly**, and the wording has to carry
  the whole explanation. Mitigated is the default, the safe state, and the one the baselines
  came from — but a reader meeting `FAIL` on a correctly-configured machine will misread it
  the first time, and no wording fully prevents that.
- **The row is CPU- and kernel-specific.** `retbleed` and `spec_rstack_overflow` are AMD;
  an Intel machine pays a different set for different reasons and this repository has not
  measured one. The row counts what the kernel reports rather than naming vendors, which
  generalises the *check* but not the 61%.
- **It adds a reason for `--strict` to refuse**, and every such reason is a way for a
  legitimate measurement to be blocked by a machine detail. The mitigation state is not a
  detail at 61%, which is the justification, but the cost is real.
- **One machine, one kernel, one CPU.** Everything here is a Ryzen 7 3700X on Linux
  7.0.0-30-generic.

## Alternatives considered

**Make the row informational — always printed, never counted.** Rejected: `check-machine.sh`
counts `unknown` separately and `--strict` treats it as not-a-pass, so an informational row is
either ignored or fatal, and neither is what this needs. A row nobody counts is a row that
stops being read.

**Fail when mitigations are ON, since that is the slow configuration.** Rejected outright.
That is a gate telling a reader to disable their CPU's security to make a benchmark pass, and
this project does not make that recommendation in a checklist, in a script, or anywhere else.

**Record the mitigation state in `baselines.tsv` alongside the verdict and compare per row.**
More precise and more machinery: a second key per baseline, for a configuration that should be
constant. The verdict column already carries the checklist's answer, and making that answer
include mitigations is the smaller change.

**Leave it alone and rely on the write-up.** Rejected. `CLAUDE.md` §4: *prose does not hold a
constraint.* The write-up is where the number lives; the gate is what stops the number being
quoted from the wrong machine.
