# A quiet-machine check cannot see a timer that has not fired yet

> `[measured 2026-09-22]` — boot D step D4 lost rounds 13 to 20 of 20 to
> `apt-daily-upgrade.service`. The check that guards every number in this repository was green
> before each of those rounds and green again after them. **`[to testing-skills]`**

## The shape

`scripts/check-machine.sh`'s *machine is quiet* row reads CPU busy **over one second, now**. It
is the right question for *"is something running?"* and it is no question at all about *"will
something start in the next four hours?"*

A campaign that runs for one hour never learns the difference. Boot C ran 20:37 → 02:45 and never
found out. Boot D ran 23:36 → 11:50, and at **06:51** `apt-daily-upgrade.timer` fired:

```text
round 12 arm w0  busy  0% ok
round 12 complete
round 13 arm w1s busy 12% DISQUALIFIED
round 14 arm w3  busy 13% DISQUALIFIED
…
round 20 incomplete
```

Every one of rounds 13–20 was thrown away — correctly, by the driver's own quiet rule, which is
the only reason the boot did not publish eight rounds of numbers taken against a machine running
`apt`. **The rule that saved it is per-arm and per-round, not the one-off check at the start.**

## The hour is the whole finding

These are not random daemons. They are **timers with a schedule**, and the schedule is knowable
before the campaign starts:

```text
$ systemctl list-timers --all
Tue 2026-09-22 07:22:31   fwupd-refresh.timer
Tue 2026-09-22 08:33:26   apt-daily.timer
Wed 2026-09-23 05:07:09   man-db.timer
Wed 2026-09-23 06:23:39   apt-daily-upgrade.timer     ← fired 06:51 today
```

`apt-daily` and `apt-daily-upgrade` carry `RandomizedDelaySec=` of up to 12 hours on Ubuntu, so
the *exact* minute is not predictable, but the window is, and `list-timers` prints the next one
outright. A campaign that will cross a listed firing either stops the timer or expects the loss.

## The rule

* **Before a campaign longer than an hour, read `systemctl list-timers --all` and stop anything
  that fires inside the window.** On this desk:

  ```sh
  for t in apt-daily.timer apt-daily-upgrade.timer fwupd-refresh.timer \
           motd-news.timer man-db.timer update-notifier-motd.timer; do
    sudo -n systemctl stop "$t"
  done
  sudo -n systemctl stop packagekit.service
  ```

  `stop` and not `disable`: the units stay `enabled`, so the next boot restores them and the desk
  does not quietly stop getting security updates because a benchmark ran once.
* **Read the quiet row per run, not per campaign**, and drop the *whole round* rather than the
  one arm that saw the load — otherwise the arms no longer share an `n`
  ([ADR-0090](../decisions/ADR-0090-a-measurement-boot-is-pre-built-shares-one-control-arm-and-a-five-step-drift-is-closed-one-named-segment-at-a-time.md)
  decision 3).
* **A green checklist is a statement about the second it was read in.** `pass 16 fail 0 unknown 0`
  was true at 21:16, true at 07:00, and true throughout the hour that produced nothing.

## What this cost, and what it did not

Five hours of wall clock and eight rounds, re-run afterwards with the timers stopped. It cost no
*wrong* number, because nothing from those rounds entered a summary: the driver counts only rows
from rounds marked `complete`. That is the difference between a gate that reads a condition
continuously and one that reads it once.

Related: [a-benchmark-that-measures-where-the-compiler-put-it](a-benchmark-that-measures-where-the-compiler-put-it.md),
[perf-record-exits-zero-when-sudo-cannot-find-the-workload](perf-record-exits-zero-when-sudo-cannot-find-the-workload.md),
[a-machine-check-narrowed-its-own-scope-when-the-link-bounced](a-machine-check-narrowed-its-own-scope-when-the-link-bounced.md).
