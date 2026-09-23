# A dial sitting in its reconnect wait does not see Admin::shutdown until the reconnect timer fires

`[measured 2026-09-23]` found building the `qfj-initiator-plain` arm of
`scripts/interop-qfj.sh`,
[docs/plans/2026-09-23-p3-quickfixj-interop.md](../plans/2026-09-23-p3-quickfixj-interop.md)
row 2. **This is an engine finding, not fixed by this plan** — `crates/**` is outside this
plan's bounds, and the fix, if one is wanted, belongs to `fixbolt_engine::reconnect`.

**`--role dial` uses `fixbolt::connect_and_serve`, whose reconnect-wait branch calls
`engine.idle_with` and loops straight back to the top without checking
`engine.shutdown_finished()`.** That check only runs once the branch falls through to
`Next::Now`. So once the judge's acceptor closes the connection (after `logout`), a dial that
lands in the reconnect wait does not observe `Admin::shutdown` until the next scheduled
reconnect attempt actually arrives — bounded by whatever `ReconnectInterval` the arm's settings
file carries, not by anything the calling script asks for.

## What was seen

Reproduced standalone, outside `scripts/interop-qfj.sh`: `Shutdown {` printed at **+26 s** after
the stop line was sent, every time, on a settings file with `ReconnectInterval=30`.

## Why the settings are not simply tightened

A shorter `ReconnectInterval` would close this window, but the plan's own *Cấu hình* section
pins it at 30 s **on purpose** — "để không quay số lại trong lúc Judge đang tắt sau logout"
(so fixbolt does not redial while the judge is shutting down after logout, which would log a
second, spurious `Logon` into the same transcript `assert_arm` reads). Shortening the interval
here would trade the hazard this page describes for the one the plan already named and guarded
against — not a net improvement.

## The fix that was made

`scripts/interop-qfj.sh`'s `run_initiator` widens its own wait for the dial to exit after
`Admin::shutdown`, from the 10 s the acceptor arms use to **40 s** — wider than the engine's
observed +26 s, with the cause commented at the call site rather than left as a bare number:

```bash
# **Not 10 s here — measured 2026-09-23, and it is a trap, not a typo.**
# `fixbolt_engine::dial`'s reconnect-wait branch (`policy.next() == At(_)`)
# calls `engine.idle_with` and `continue`s straight back to the top of the
# loop; `engine.shutdown_finished()` is only checked once that branch falls
# through to `Next::Now`. So once Judge's acceptor closes the connection,
# `Admin::shutdown` on a dial sitting in that wait is not observed until the
# scheduled reconnect attempt actually arrives — bounded by this arm's own
# `ReconnectInterval=30`, not by anything this script asks for.
```

(`scripts/interop-qfj.sh`, `run_initiator`.) The script widens its own patience to match the
engine's real, observed behaviour instead of silently reinterpreting a pinned setting.

## What is not known

Whether this is the intended shape of the reconnect-wait branch, or an oversight that should
check `shutdown_finished()` on every turn rather than only at `Next::Now`. Not decided here —
that is a design question for `crates/engine`, and this plan's own bounds keep it out.

## What guards it

The comment at the call site, and the arm itself: `qfj-initiator-plain` and `qfj-initiator-tls`
both exercise the reconnect-wait path on every run of `scripts/interop-qfj.sh` (three
consecutive green runs on the desk, `docs/CONFORMANCE.md` §10). No dedicated reversal was
written — the 40 s bound is a widened wait, not an assertion with a red/green pair of its own —
so a regression here (the wait growing past 40 s) would show as the `qfj-initiator-*` arms
timing out rather than as a named, isolated failure. A test that measures the reconnect-wait
delay directly, in `crates/engine`, would close that gap; it belongs to whichever plan next
touches `fixbolt_engine::reconnect` or `dial`.
