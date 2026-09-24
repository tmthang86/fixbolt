# A `nosuid` mount shows file capabilities it does not grant

> `[measured 2026-09-24]` — the rehearsal of `scripts/boot-p4.sh` (row 7a.1 of
> [plans/2026-09-24-p4-bypass-and-s9-boot.md](../plans/2026-09-24-p4-bypass-and-s9-boot.md)).
> Sibling of [a-pre-built-w2w-needs-its-file-capabilities-and-a-rebuild-drops-them](a-pre-built-w2w-needs-its-file-capabilities-and-a-rebuild-drops-them.md).

## What happened

A `w2w` built under `/tmp` — on the desk a `tmpfs` mounted `nosuid`, and the agent's scratchpad lives
there — was given `cap_net_raw,cap_net_admin+ep`. `getcap` printed the capability. The binary still
failed `socket(AF_PACKET)` with `EPERM`. On a `nosuid` mount the kernel ignores file capabilities at
exec, exactly as it ignores the set-user-ID bit; the attribute is stored and readable, it is just
never applied. So `getcap` alone is a green that grants nothing.

The scratchpad is also where agents tend to stage copies of binaries, which makes this the likely
way to hit it again. (`/tmp` on the desk is tmpfs for a second reason too — its files do not survive
a shutdown.)

## The rule

A binary that needs a file capability is built and run from a normal mount — the worktrees under
`../fb-p4-boot/` — never from `/tmp` or the scratchpad. Check the mount, not only the attribute:

```
findmnt -no OPTIONS -T <binary>     # must not contain nosuid
```

## The guard

`scripts/boot-p4.sh`'s `has_caps` reads both `getcap` and `findmnt -no OPTIONS -T` and treats a
`nosuid` mount as "no capability". `run` refuses such a binary before anything runs —
`refused before running: … not on a nosuid mount such as /tmp`, **exit 2** — and the per-arm check
stops the boot the same way (exit 3) if it ever changes mid-boot.
