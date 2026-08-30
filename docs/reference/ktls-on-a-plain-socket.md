# kTLS on a plain socket: what is known, and what blocked it twice

> **What this page is:** the record of a spike that was **blocked for two
> different wrong reasons in a row**, the second of which was a defect in the very
> script written to diagnose the first. `[measured 2026-08-30]` the blocker is now
> **gone**: the owner's Linux desktop accepts `setsockopt(TCP_ULP, "tls")`, so
> `STATUS.md` open item 10 can be answered. Nothing about `ktls-core` or ADR-0005
> is concluded here — the spike's steps 2–5 still have to happen.
> `docs/plans/2026-08-30-ktls-spike.md` is the plan; this is its result so far.

## The question, and why it is load-bearing

[ADR-0005](../decisions/ADR-0005-tls.md) makes TLS a transport and states the
hot-path guarantee per mode. It was accepted **on reasoning, not measurement**,
and its own open question 1 has never been answered:

> Can `ktls-core` be driven from a plain non-blocking socket with no async
> runtime?

Its documented usage is `tokio-rustls`-shaped. If the answer is **no**, ADR-0005's
central claim collapses to *userspace rustls only*, and the hot-path guarantee
goes with it — every TLS byte would be encrypted in this process rather than by
the kernel, on a path `DESIGN.md` D8 spends a core to keep clear.

## What was wrong with how this was blocked

`STATUS.md` recorded item 10 for days as *"cannot be checked here — needs the
Linux box of open item 6"*. `[measured 2026-08-30]` **that was the wrong blocker,
twice over.**

- **kTLS is a kernel feature, not a latency property.** It needs no machine
  matching `DESIGN.md` §9 — no isolated cores, no pinning, no frequency control.
  Answering it is a question about a `setsockopt` returning, not about a
  percentile. Blocking it behind the §9 machine kept it shut for no reason.
- **And "a Linux box" is not the requirement either.** It needs a Linux kernel
  **built with `CONFIG_TLS`**, which is a much narrower thing and is not implied
  by running Linux.

The second point is the one that stopped the spike, and it was found by running
the check rather than by reading a config line.

## What was observed

`[measured 2026-08-30]` Linux 6.18.44-fc-v22, x86_64 container:

```
kernel:    Linux 6.18.44-fc-v22
config:    # CONFIG_TLS is not set
tls_stat:  absent
setsockopt(TCP_ULP, "tls"): REFUSED errno=2 (ENOENT) No such file or directory
```

The `setsockopt` was issued on a **real, connected** TCP socket, on both ends of a
loopback pair, because on an unconnected socket it fails for an unrelated reason
and that result would prove nothing. `ENOENT` from `TCP_ULP` means the kernel has
no ULP registered under the name `tls` — the module does not exist, rather than
being unloaded or refused by policy. `/boot/config-$(uname -r)` agrees, and the
kernel exposes no `/proc/net/tls_stat`.

**The config line alone would not have been enough.** A kernel config says what
was compiled; it does not say what a container is permitted to do. The syscall
says both, which is why the check makes the call rather than grepping.

## What this does and does not conclude

**It does not conclude anything about `ktls-core` or about ADR-0005.** No
conclusion about a library can be drawn from a kernel that has no kernel TLS: the
experiment never got as far as the library. Recording "probably fine" or
"probably broken" here would be exactly the kind of claim `CLAUDE.md` §10 forbids.

What it does settle is **the requirement to provision**, which was previously
described wrongly:

| Needed | Not needed |
|---|---|
| A Linux kernel with **`CONFIG_TLS=y`** or `=m` with the module loadable | A machine matching `DESIGN.md` §9 |
| Permission to `setsockopt(TCP_ULP)` in the container or host | Isolated cores, pinning, frequency control |
| `rustls` for the handshake, and the key material it exposes | A load generator, or a second machine |

`scripts/check-ktls-available.sh` answers "can I start?" in one command, and
exits non-zero with the reason when the answer is no. Run it first on any machine
before picking this spike back up; the rest of the plan is unchanged and its
steps 2–5 are still what has to happen.

## The third blocker was the diagnostic script itself

`[measured 2026-08-30]` on the owner's Linux desktop — Linux 7.0.0-30-generic,
AMD Ryzen 7 3700X — the script printed this, and **both halves are from the same
run**:

```
config: CONFIG_TLS=m
setsockopt(TCP_ULP, "tls"): REFUSED errno=2 (ENOENT) No such file or directory
  ENOENT here means the kernel has no `tls` ULP at all: it was built
  without CONFIG_TLS.
```

The kernel it says was built without `CONFIG_TLS` reports `CONFIG_TLS=m` four
lines above. Open item 10 sat blocked on that sentence.

**The rule was already written down correctly on this page** — the provisioning
table says *"`CONFIG_TLS=y` **or `=m` with the module loadable**"* — and the script
did not implement it. A page that states a rule does not enforce it.

### What ENOENT from TCP_ULP actually means

`ENOENT` says **no ULP is registered under the name `tls`**. Registered is not the
same as compiled, and it is not the same as available:

| State | `setsockopt` unprivileged | What it means |
|---|---|---|
| No `CONFIG_TLS` | `ENOENT` | Genuinely cannot. Provision another kernel |
| `CONFIG_TLS=m`, module **not loaded** | `ENOENT` | **One `modprobe` away** |
| `CONFIG_TLS=m`, module loaded | accepted | Ready |
| `CONFIG_TLS=y` | accepted | Ready, nothing to load |

The middle row is the trap. The kernel autoloads a ULP through `request_module`
**only for a caller holding `CAP_NET_ADMIN`**, so an unprivileged probe gets
`ENOENT` while `tls.ko` sits on disk. Proven by reversal on the desktop: with the
module unloaded the fixed script reports `LOADABLE` and exits 2; after
`modprobe tls` the same script reports `READY` and exits 0.

### And a second defect, found while fixing the first

The replacement detected "is the module loaded" with `lsmod | grep -q '^tls '`
and reported `loaded=no` on a machine where `tls` was loaded. Under the script's
own `set -o pipefail`, `grep -q` exits at the first match, `lsmod` dies of
`SIGPIPE` with status 141, and **the pipeline reports failure precisely when the
thing was found**. `modinfo` escaped only because it is not in a pipeline.

`scripts/check-machine.sh` had the same construction in its kTLS row. There it was
masked by a `|| modinfo tls` fallback — but on a kernel with `CONFIG_TLS=y` the
module is built in, `modinfo` finds nothing, and that row would have reported
*"no tls module"* on the machine best equipped to run kTLS. Both now ask
`/sys/module/tls`, which covers loaded and built-in alike.

### The guard

`scripts/check-ktls-classify.sh` runs the verdict logic over eight
(syscall, config, loaded, on-disk) combinations and asserts the token for each. It
needs no kernel, no root and no kTLS, so CI runs it on every push — the job is
`script-logic` in `.github/workflows/ci.yml`.

Against the old logic it fails 5 of 8, including the desktop's own case; the three
it passes are the container case and the accepted case, which the old script did
get right. **A gate that nothing gated** is what let a script contradict itself in
its own output for a day.

## The generalisation

`[to testing-skills]` — **a blocker recorded once and never re-examined keeps
work shut for the wrong reason.** This one said "needs the fast machine" and
meant "needs a kernel option"; the two have nothing to do with each other, and
the item sat behind hardware that would not have helped. The cheap defence is a
one-command capability probe that *states its own requirement*, so the blocker is
re-tested rather than re-read. The expensive version is what happened here: an
item blocked on the wrong thing for days, in a repository whose own status page
is explicitly maintained against staleness.
