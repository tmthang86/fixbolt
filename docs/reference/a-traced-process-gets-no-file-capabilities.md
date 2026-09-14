# A traced process gets no file capabilities, and the way round it depends on the terminal

`[measured 2026-09-14]` — found while building step A3b (`tools/w2w --wire-timestamps`) of
[the second Linux desk plan](../plans/2026-09-04-the-second-linux-desk.md), written up in that
plan's Sửa 2, Điều 1. `[to testing-skills]`

## What happened

A gate traces a benchmark binary with `strace -f` to read which syscalls one thread makes. A new
arm of that gate needed the binary to open an `AF_PACKET` socket, so the binary was given
`cap_net_raw,cap_net_admin=ep` with `setcap`. Run directly, it worked (exit 0). Run by the gate,
the same binary failed with `socket(AF_PACKET): Operation not permitted`, exit 1.

**The kernel strips file capabilities from a program exec'd under a tracer that lacks
`CAP_SYS_PTRACE`** (`security/commoncap.c`, `cap_bprm_creds_from_file`: with `LSM_UNSAFE_PTRACE`
set and `!ptracer_capable`, the new permitted set is intersected with the parent's). An
unprivileged `strace` or `gdb` is such a tracer. So `setcap` is correct for a direct run and
useless for a traced one, and nothing prints why.

The way round it chosen here — run the arm inside `unshare -Urn`, where the process is root of
its own user and network namespace and has `CAP_NET_RAW` over its own `lo` — hit a second,
quieter trap: **Ubuntu ≥ 24.04's AppArmor refuses unprivileged user namespaces to a process
without a profile that allows them** (`kernel.apparmor_restrict_unprivileged_userns = 1`). The
first successful run worked only because the shell was a VS Code terminal, whose `vscode`
profile allows `userns`; from `aa-exec -p unconfined` the same command failed with
`unshare: write failed /proc/self/uid_map: Operation not permitted`. The GitHub Ubuntu runner
refused it too: CI run [`34842390918`](https://github.com/tmthang86/fixbolt/actions/runs/34842390918)
on `f9abc1a` took the step's fallback (`echo 0 | sudo tee
/proc/sys/kernel/apparmor_restrict_unprivileged_userns` printed `0`) before the arm went green.

## The rule

- **A capability on a file is not a capability of a traced process.** A gate that traces a
  binary must not depend on `setcap`; give it the privilege some other way (a user namespace, or
  a root tracer that drops to the user with `strace -u`), or do not trace.
- **Whether a user namespace is allowed is a property of the calling process's AppArmor label,
  not of the machine.** A result from one terminal says nothing about another terminal, a
  service, or CI. Probe (`unshare -Urn true`) in the same process that will run the gate, and
  treat a refusal as *not run*, never as green.

## What guards it

- `scripts/check-no-kernel-sleep.sh` and `scripts/check-standard-gives-the-core-back.sh`: with
  `--wire-timestamps` in `W2W_EXTRA` they re-run themselves under `unshare -Urn`, and when the
  probe is refused print `SKIPPED, NOT PASSED` and **exit 2** — red in CI, never a pass.
- The reversal, run by the manager on `f9abc1a`: the same command under `aa-exec -p unconfined`
  exits 2 with the SKIP sentence. **It is a manual reversal, not an automated test** — no gate
  runs the refused branch.
- CI: one step per mode job flips the sysctl only when the probe is refused.
- `docs/hft-playbook.md` §6 item 4: tracing the engine on a real NIC, which a namespace cannot
  see, is `sudo -n strace -f -u "$USER" …` at the desk.
