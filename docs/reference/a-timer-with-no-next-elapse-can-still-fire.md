# A timer with no next elapse can still fire

> `[measured 2026-09-27]` — the phase-4 §9 boot. The procedure stopped every timer whose `next`
> was set. `anacron.timer`'s `next` was empty at that moment, so it was left running. Fifteen
> minutes later it had a `next`, and the driver stopped the boot before its second block (exit 3).
> **`[to testing-skills]`**

## The shape

`systemctl list-timers --all` shows a `NEXT` column. The boot plan's step 5 (row 7b, *Sửa 4*
item 3) read it as the list of timers that can fire and stopped exactly those:

```bash
systemctl list-timers --all --output=json | jq -r '.[] | select(.next != null) | .unit' |
  xargs -r sudo -n systemctl stop
```

A timer whose `next` is empty is not a timer that cannot fire. It is a timer with no elapse
scheduled **at the moment of reading**. A timer that is still `active` can get one later.
`anacron.timer` (`OnCalendar=hourly`) did exactly that. Its `LAST` was 11:05:49 local time, and the
anacron job it starts runs for a while. Our reading, **inferred and not proven**: while the
triggered service runs, the timer has no next elapse, and systemd computes one when the service
exits.

## What happened, in order

All times are local (+07), from `target/boot-p4-evidence/first-actions.txt` and the two attempt logs:

1. **Step 5, 11:12.** The system timers with a `next` were stopped. The list had 17 names, and
   `anacron.timer` was not among them.
2. **Attempt 1, 11:12:18. Refused, exit 2.** The driver's own check covers both managers, and it
   found two **user** timers due inside its window. Step 5 had stopped system timers only:
   `user: aura-glass-update-check.timer next 2026-09-27T04:22Z (in 9m51s),
   snap.firmware-updater.firmware-notifier.timer next 2026-09-27T05:00Z (in 47m42s)`. The five user
   timers were then stopped by hand.
3. **Attempt 2, 11:12:36 to 11:27:49. Stopped, exit 3, before p1 B.** The gate before the bench
   block read `FAIL   no timer due           anacron.timer next 2026-09-27T04:31Z (in 3m14s)`. The
   io_uring block had already run six arms, which were thrown away.
4. **The fix that worked:** stop **every** timer unit, system and `--user`, whatever its `next`
   says. Attempt 3 started at 11:28:32 and exited 0 at 13:04:36.

The driver's refusal before attempt 2 read `next` too (`timers_verdict`, the `no timer due` row's
own function). So it could not see `anacron.timer` either. Both checks asked *which timers are
due?*, and the question that protects a boot is *which timers are armed?*

## The fix

- **Procedure** (boot plan *Sửa 5*): step 5 stops every active timer unit of both managers:

  ```bash
  systemctl list-units --type=timer --all --output=json | jq -r '.[] | select(.active != "inactive" and .active != "failed") | .unit' | xargs -r sudo -n systemctl stop
  systemctl --user list-units --type=timer --all --output=json | jq -r '.[] | select(.active != "inactive" and .active != "failed") | .unit' | xargs -r systemctl --user stop
  ```

  `stop`, not `disable`, so the next boot gets them back.
- **Driver**: `scripts/boot-p4.sh run` refuses (exit 2) while any timer unit of either manager is
  active. `active_timers_verdict` reads `systemctl list-units --type=timer --all --output=json` and
  names each unit and the command that stops it. The older *due in the window* refusal stays in
  place. `check-machine.sh`'s `no timer due` row is unchanged: it answers a different question
  for every other script that reads it.

## The guard

`scripts/check-boot-p4-driver.sh` (CI job *script-logic*) feeds `active_timers_verdict` a set of
fixtures. They include `anacron.timer` `active`/`running` with no `next` (the 2026-09-27 case) and
the two user timers of attempt 1. A fake `systemctl` first on `PATH` also exercises
`active_timers_check` for both managers. Reversals, 2026-09-27:

- The filter narrowed to timers with a scheduled elapse (`select(.sub == "waiting")`, the
  `next != null` question in `list-units` terms) went red on three assertions: `want FAIL with
  'anacron.timer' — anacron.timer active with no next (sub running) — the 2026-09-27 case; got:
  PASS`, the fix-command assertion, and `system manager: PASS`.
- `--user` dropped from `active_timers_check` went red on `user manager: FAIL … user: anacron.timer active` (the user fixture was never read).

Both were restored to `pass 28 fail 0`.

**What the guard cannot see:** the refusal's top-level loop in the driver. It also cannot see
whether a unit that is not a timer (a `.path` unit, a cron daemon) starts work mid-boot. The
per-block `check-machine.sh` gate, whose *machine is quiet* row is re-read per run, is still what
catches that. See [a-quiet-machine-check-cannot-see-a-timer-that-has-not-fired](a-quiet-machine-check-cannot-see-a-timer-that-has-not-fired.md).
