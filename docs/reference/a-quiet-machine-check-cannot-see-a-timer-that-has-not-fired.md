# A quiet-machine check cannot see a timer that has not fired yet

> `[measured 2026-09-22]` — boot D step D4 lost rounds 13 to 20 of 20 to
> `apt-daily-upgrade.service`. The check that guards every number in this repository was green
> before each of those rounds and green again after them. **`[to testing-skills]`**

## The shape

`scripts/check-machine.sh`'s *machine is quiet* row reads CPU busy **over one second, now**. It
is the right question for *"is something running?"* and it is no question at all about *"will
something start in the next four hours?"*

A campaign that runs for one hour never learns the difference. Boot C ran 20:37 → 02:45 and never
found out. Boot D ran 23:36 → 11:50, and at **06:51:31** `apt-daily-upgrade.service` started:

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

Two details the first draft of this page got wrong, both fixed by reading the evidence instead of
remembering it. **Round 13 was not uniformly loaded**: its first three arms (`w0`, `wa`, `w1`) ran
clean and were dropped *as a round*, which is exactly the rule that keeps every arm's `n` equal;
only from round 14 did every arm read over the limit. And the load was **7% to 24%**, not the
10–13% the first three lines of the log happened to show. **`apt-daily-upgrade.service` itself ran
only 06:51:31 → 06:53:32** — `packagekit.service` stayed up until **06:58:34**, which is what the
last five wasted rounds actually ran against. Naming one unit for a seven-minute window was a cause
accepted because a knob moved with it (`CLAUDE.md` §10); the window is the finding, the unit list
is the remedy.

The `journalctl` window and the timer table were captured to
`target/boot-d-evidence/d4-timer-evidence.txt` **after** the boot, because the first version of
this page quoted a terminal that nothing had saved, and then **reconstructed a `list-timers` table
from memory that disagreed with the real one in two of its four rows** — it showed `fwupd-refresh`
and `apt-daily` as future firings when both had already fired that morning. A page about reading
the output instead of remembering it had, in its own first draft, a remembered output. The block
above is now the captured one.

## The hour is the whole finding

These are not random daemons. They are **timers with a schedule**, and the schedule is knowable
before the campaign starts:

```text
$ systemctl list-timers --all          # captured 2026-09-22 12:04, after the boot
NEXT                        LAST                         UNIT
-                           Tue 2026-09-22 06:48:31 +07  fwupd-refresh.timer
-                           Tue 2026-09-22 03:35:22 +07  apt-daily.timer
-                           Tue 2026-09-22 06:37:37 +07  man-db.timer
-                           Tue 2026-09-22 06:51:31 +07  apt-daily-upgrade.timer   ← the one that cost the rounds
```

`NEXT` is empty on every row because these six were **stopped** once the loss was understood; the
`LAST` column is what a campaign needed to read *beforehand*, when those rows still carried a
`NEXT` inside the night. The full capture, with the `journalctl` window, is
`target/boot-d-evidence/d4-timer-evidence.txt`.

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

## Guarded by

`check-machine.sh`'s `no timer due` row
([ADR-0093](../decisions/ADR-0093-a-campaign-driver-is-committed-sudo-in-a-committed-script-names-what-root-can-find-and-a-timer-due-inside-the-window-is-a-fail-row.md)
decision 3), reading `systemctl list-timers --all --output=json` through the pure function
`timers_verdict <now_usec> <window_sec> <json>`, tested in `scripts/check-machine-verdicts.sh`
section `=== timers_verdict` against a fixture with a timer due inside the window, one outside
it, one already past `next`, and `[]`. The window is `FIXBOLT_TIMER_WINDOW` hours, **default
12** — the longest campaign on record at the time this row was written. `ab-rotation.sh` refuses
to start a run on a `FAIL` from this row. **The row prints the window it used, so a 14-hour
campaign under a 12-hour window is still unseen.**

`[seen live 2026-09-22]` Both shapes on the desk (`tmt-B450-I-AORUS-PRO-WIFI`, systemd, desktop
grub line): `FAIL   no timer due   sysstat-collect.timer next 2026-09-22T14:10Z (in 7m59s), …
apt-daily-upgrade.timer next 2026-09-22T23:18Z (in 9h16m) [window 12h]` with twelve `stop`
commands on its `fix:` line; after running that line verbatim, `PASS   no timer due   no timer
due inside the window [window 12h]`; after `systemctl start` of the same twelve, `list-timers
--all` counted 20 `.timer` units before and after. Until then only `UNKNOWN` had been seen on a
live host (the CI container has no PID 1).

## What this cost, and what it did not

Five hours of wall clock and eight rounds, re-run afterwards with the timers stopped. It cost no
*wrong* number, because nothing from those rounds entered a summary: the driver counts only rows
from rounds marked `complete`. That is the difference between a gate that reads a condition
continuously and one that reads it once.

Related: [a-benchmark-that-measures-where-the-compiler-put-it](a-benchmark-that-measures-where-the-compiler-put-it.md),
[perf-record-exits-zero-when-sudo-cannot-find-the-workload](perf-record-exits-zero-when-sudo-cannot-find-the-workload.md),
[a-machine-check-narrowed-its-own-scope-when-the-link-bounced](a-machine-check-narrowed-its-own-scope-when-the-link-bounced.md).
