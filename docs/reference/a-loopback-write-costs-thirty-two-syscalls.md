# A loopback write costs thirty-two syscalls

`[measured 2026-09-05]` on the `DESIGN.md` §9 desktop — AMD Ryzen 7 3700X, governor
`performance`, boost off, SMT off, `isolcpus=6,7`, `processor.max_cstate=1`.

| What | ns |
|---|---|
| `getppid` — the bare syscall floor | **170.5** |
| `recv` on a quiet socket, non-blocking, returns `EAGAIN` (committed baseline) | **418.5** |
| `pipe` write **and** read of 8 bytes | **778.9** |
| UNIX `socketpair` write **and** read of 8 bytes | **1 924.9** |
| TCP `127.0.0.1` write **and** read of 8 bytes | **10 228.6** |
| TCP `127.0.0.1` write of 8 bytes alone, drained once per 1 024 | **5 450.0** |

**One TCP loopback write is thirty-two bare syscalls.** The round trip is thirteen pipes and
five UNIX sockets, for the same eight bytes.

## What it is not

**Not the read waiting.** A non-blocking version of the same loop reads **0.00 `EAGAIN` per
operation** — the data is always already there — and costs the same 10 276 ns. The blocking
version is not sleeping; there is nothing to sleep for.

**Not scheduling or migration.** `taskset -c 3` reads 10 265.4 and 5 449.7, which is the same
figure to within 0.1%.

**Not this project's code.** The measurement is thirty lines of `std::net` compiled with
`rustc -O`. No FIX, no engine, no crate from this workspace.

**Not the syscall boundary.** `getppid` is 170.5 ns on the same box in the same run, so entry
and exit are cheap. Whatever costs 5 µs is inside the network stack.

## What might explain it, and none of this is claimed

Two candidates were found and **neither was tested**, so neither is a cause:

* **Netfilter.** `nf_tables`, `nf_conntrack`, `nft_compat`, `xt_conntrack`, `xt_connmark` and
  `xt_MASQUERADE` are all loaded, `nf_conntrack_count` reads 66, and `tailscale0` is up. Every
  loopback packet traverses whatever chains those installed. Tailscale and Docker each install
  their own.
* **Speculation mitigations.** This kernel reports retpolines, `IBPB: conditional`, `STIBP:
  always-on`, RSB filling, `Safe RET` for SRSO, and `IBPB before exit to userspace` for
  vmscape. The TCP receive path is thousands of instructions with many indirect calls, which is
  the shape retpolines tax hardest.

Testing either means changing the machine — an `iptables -t raw -j NOTRACK` rule, or a boot
with `mitigations=off` — and both are the owner's call rather than a benchmark's.

## Why it was worth writing down anyway

**Every wire-to-wire figure this project has published went over `127.0.0.1`.** `DESIGN.md` §8
carries a 10–20 µs wire-to-wire budget and attributes ~1.4 µs of it to user space. If four
syscalls of the round trip cost ten microseconds *on this machine, in this configuration*, then
the budget's largest term is one nothing in this repository owns, and open item 40 — NIC to NIC
— is measuring something quite different from what it will be compared against.

**And it changes what a benchmark of this kind may be asked.** The step that found this was
pricing the *difference* between two message sizes. A difference is safe: both sides pay the
same constant and it cancels — `[measured 2026-09-05]` the two `tools/w2w` paths make 44 002
`sendto` calls each, so they differ in bytes and not in syscall count. **An absolute figure from
the same benchmark is not safe**, and the module doc of `crates/engine/benches/payload.rs` says
so where a reader will meet the number.

## The general shape

`[to testing-skills]`

**A benchmark's absolute figure can be dominated by something that is not the code under test,
while its differences stay perfectly sound.** The two are not the same measurement and they do
not have the same warranty.

The instinct on finding an absolute figure thirty times larger than expected is to fix the
benchmark or to discard the run. Both are wrong here. The figure is correct — that *is* what a
loopback round trip costs on this machine — and the benchmark's actual job, a subtraction, is
unaffected. What has to happen instead is narrower and less satisfying:

1. **Say which of the two the number is.** Publish the difference; label the absolute as
   environment-bound and name the environment.
2. **Prove the constant really is constant** before relying on the cancellation. Here that was
   a syscall count taken from `strace`: equal on both sides, so a per-syscall cost cannot
   appear in the difference. Without that check, "it cancels" is an assumption wearing the
   clothes of an argument.
3. **Bracket the anomaly with instruments that do not share its path.** A pipe, a UNIX socket
   and a bare `getppid` cost nothing to measure and turn "this seems slow" into a table with a
   32x in it.

The cheap comparisons are the whole method. Any suite timing something that crosses a kernel,
a driver, a container boundary or a virtual network should be able to answer *"and what does
the simplest possible version of this cost, right now, on this machine?"* — because when the
answer is thirty-two times smaller, everything absolute in the suite has just changed meaning.
