# A pre-built `w2w` needs its file capabilities, and a rebuild drops them

> `[measured 2026-09-24]` — the first rehearsal of `scripts/boot-p4.sh` (row 7a.1 of
> [plans/2026-09-24-p4-bypass-and-s9-boot.md](../plans/2026-09-24-p4-bypass-and-s9-boot.md)).
> Sibling of [a-traced-process-gets-no-file-capabilities](a-traced-process-gets-no-file-capabilities.md)
> and [a-nosuid-mount-shows-file-capabilities-it-does-not-grant](a-nosuid-mount-shows-file-capabilities-it-does-not-grant.md).

## What happened

A measurement boot runs binaries built before the reboot (ADR-0090 decision 2). The pre-build
procedure — as ADR-0090 and the plan's row 7a.2 first wrote it — built `w2w`, hashed it into
`MANIFEST.txt`, and stopped there. In the rehearsal every arm that takes the acceptor's NIC stamps
(`WIRE_NIC`/`OBSERVER_CORE`, which gives the listen half `--wire-timestamps`) failed with
`socket(AF_PACKET): Operation not permitted`: the NIC tap opens an `AF_PACKET` socket and sets the
NIC's hardware stamping, which need `CAP_NET_RAW` and `CAP_NET_ADMIN`, and a freshly built binary
carries no file capability. Every arm without NIC stamps (`standard`, the benches) ran, so a partial
rehearsal would have looked mostly green.

A file capability is an extended attribute on the file, so it does not change the binary's sha256 —
`MANIFEST.txt` cannot see it. Cargo writes a new file on a rebuild, and the attribute is gone.

## The rule

The pre-build sets the capability on every `w2w` that will run a NIC-stamped arm, **after** the last
build and before the manifest is read back:

```
sudo -n /usr/sbin/setcap cap_net_raw,cap_net_admin+ep <tree>/target/release/w2w
getcap <tree>/target/release/w2w     # must print cap_net_admin,cap_net_raw=ep
```

`scripts/boot-p4.sh build` does both for each worktree and dies if `getcap` does not read it back.

## The guard

`scripts/boot-p4.sh run` checks the capability with `getcap` (and the mount, see the sibling page)
**before running anything** — a missing one is `refused before running`, exit 2 — and again
**before every arm**: a binary that lost it mid-boot stops the boot, exit 3, with
`lost its cap_net_raw,cap_net_admin file capability`. The check lives in the driver's `has_caps`.
