# A saved iptables-nft ruleset does not load back through `nft`

`[measured 2026-09-23]` on the `DESIGN.md` §9 desktop, boot F step F9 of
[plans/2026-09-23-boot-f-closes-the-open-items-and-powers-off.md](../plans/2026-09-23-boot-f-closes-the-open-items-and-powers-off.md)
(`STATUS.md` item 51's flush arm). Evidence: `target/boot-f-evidence/f9.sh`, `f9.txt`, `f9b.sh`,
`f9b.txt`, `nft-before.txt`, `nft-before-2.txt` (gitignored, this desk only).

## What happened

The plan's restore step for the flush arm was the obvious pair:

```text
sudo -n nft list ruleset > nft-before.txt     # save
sudo -n nft flush ruleset                     # measure with nothing loaded
sudo -n nft -f nft-before.txt                 # restore
```

The restore failed:

```text
nft-before.txt:62:66-87: Error: unsupported xtables compat expression, use iptables-nft with this ruleset
		meta mark & 0x00ff0000 == 0x00040000 counter packets 0 bytes 0 xt target "MASQUERADE"
```

Every table on this desk (`ip`/`ip6` × `filter`, `nat`, `mangle`) was created by tailscaled
through **iptables-nft**, and the listing says so on its first lines — `# Warning: table ip
filter is managed by iptables-nft, do not touch!` and `# Warning: XT target MASQUERADE not
found`. iptables-nft stores some rules as **xtables compat expressions** (`xt target
"MASQUERADE"`); `nft list` can print them, `nft -f` cannot parse them back. So `nft list
ruleset` is a **listing, not a backup**, for any ruleset another tool built through the
iptables-nft compatibility layer.

What restored it was the owner of the rules: `systemctl start tailscaled` rebuilt all six
tables. The second attempt (`f9b.sh`) dropped `nft -f` altogether, restarted tailscaled, and
compared the listing with the counters normalised (`sed -E 's/counter packets [0-9]+ bytes
[0-9]+/counter/'`, comment lines dropped): **identical**, and identical to the listing saved
before the first attempt. A plain `diff` of two listings is never empty, because the packet and
byte counters move; the first script printed `RULESET DIFFERS` for that reason alone.

The same first attempt lost all fifteen of its timing readings to a second, unrelated slip: it
extracted the figure with `awk '{print $6}'`, and `TCP loopback, 8 in 8 out` is **six**
whitespace-separated fields before the number, so field 6 is the word `out`. The script printed
`out out out out out` for every phase and exited 0. The rerun used `$7`; the shape is the one in
[reading-the-output-you-grepped-for](reading-the-output-you-grepped-for.md) — a field number is a
count of the case name's spaces, and a script that extracts it verifies nothing unless the
extracted thing is read as a number.

## The rule

1. **Restore a ruleset through the tool that built it.** If the listing carries `managed by
   iptables-nft`, the restore is `iptables-restore` from an `iptables-save` taken *before*, or —
   when a daemon owns the rules, as tailscaled does here — restarting that daemon. `nft -f` is
   only a restore for a ruleset written in native `nft` syntax.
2. **Compare rulesets modulo counters.** Normalise `counter packets N bytes M` and drop the
   comment lines before diffing; anything else reports a difference that is only traffic.
3. **Check which tables a flush will take.** `nft flush ruleset` removes every table of every
   family, including the ones a VPN or a container runtime needs; on this desk that was all of
   tailscaled's, and the arm ran only after `ss -tnp` and `who` showed no controlling session on
   a `100.x` address (the plan's own condition).

## What guards it

**No automated guard.** It is a host operation on the §9 desk, not repository code, and no CI
runner carries tailscaled's ruleset. The guard is procedural: the restore step in the plan's
flush-arm row names the owning daemon and the counter-normalised diff, as `f9b.sh` does. A
future plan that flushes the ruleset copies `f9b.sh`'s `norm` and restart lines, not the plan
text of boot F step F9.

## Related

- [a-loopback-write-costs-thirty-two-syscalls](a-loopback-write-costs-thirty-two-syscalls.md) —
  the item the flush arm was measuring; its result (−22.6 % with the ruleset flushed) is there
  and in [measured-costs](measured-costs.md) *Boot F, item 51*.
- `nft`'s own error text is the authority used here: *"unsupported xtables compat expression,
  use iptables-nft with this ruleset"*. No web search was made for this page.
