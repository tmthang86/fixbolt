# kTLS on a plain socket: what is known, and what blocked it twice

> **What this page is:** the record of a spike that was **blocked for two
> different wrong reasons in a row**, the second of which was a defect in the very
> script written to diagnose the first — and then, once unblocked, the answer it
> was written to get.
>
> **The answer is yes, with conditions**, and the conditions are the valuable
> part. `[measured 2026-08-31]` `ktls-core` drives a plain non-blocking socket
> with no async runtime, and the data path afterwards is `read(2)` and `write(2)`
> with no blocking call at all. `STATUS.md` open item 10 is **closed** and
> [ADR-0018](../decisions/ADR-0018-ktls-on-a-plain-socket-answers-adr-0005.md)
> supplements ADR-0005 with what was found.
> `docs/plans/2026-08-30-ktls-spike.md` is the plan; this is its result.

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

## The answer

`[measured 2026-08-31]` **Yes, with conditions.** AMD Ryzen 7 3700X, Linux
7.0.0-30-generic, `CONFIG_TLS=m` loaded; `ktls-core` 0.0.5, `rustls` 0.23.43 with
the `ring` provider, `rcgen` 0.14.10; TLS 1.3, `TLS13_AES_128_GCM_SHA256`.

The program is [`spikes/ktls`](../../spikes/ktls) and the gate that runs it is
`scripts/check-ktls-on-a-plain-socket.sh`. **15 assertions, `fail 0`.**

```
PASS handshake-no-runtime-server — rustls handshake on a non-blocking socket: 2 reads, 0 bytes left unprocessed
PASS negotiated — TLSv1_3 / TLS13_AES_128_GCM_SHA256
PASS plaintext-round-trip-server — wrote plaintext with write(2), read plaintext back with read(2)
PASS steady-state — 1000 plaintext round trips over an offloaded socket
PASS wouldblock-unchanged — read(2) on an empty offloaded socket returns EAGAIN
PASS session-tickets-survived — 1 kernel EIO seen on the client, 1 recovered
PASS kernel-counts-the-sockets — /proc/net/tls_stat TlsTxSw went 9 -> 11, expected +2
PASS wire-is-a-tls-record — first bytes off the wire: [17, 03, 03, 00, 34, ...]; 57 bytes total, record declares 52, needle+type+tag is 52
PASS wire-carries-no-plaintext — 57 bytes read raw off the socket; the plaintext needle is absent
PASS reversal-breaks-it — sender did not hand its keys to the kernel; receiver: read failed with errno Some(90)
PASS hand-draining-desyncs-the-kernel — 346 bytes read by hand before the handover; the next record then: read failed with errno Some(74)
SPIKE pass 15 fail 0
```

**The `tokio` impression came from the wrong crate.** ADR-0005 cited two
families at once. `ktls` 6.0.2, under the rustls organisation, genuinely is
`tokio-rustls`-specific. `ktls-core` 0.0.5, which is the one open question 1
names, depends on `bitfield-struct`, `libc`, `nix` and `zeroize` and **has no
async feature at all** — the `tokio` dependency lives one crate up, in
`ktls-stream`, behind its default `async-io-tokio` feature. Every entry point
`ktls-core` exposes is synchronous and generic over `AsFd`:

```rust
ktls_core::setup_ulp<S: AsFd>(&S) -> Result<()>
ktls_core::TlsCryptoInfoTx::set<S: AsFd>(&self, &S) -> Result<()>
ktls_core::Context::handle_io_error<S: AsFd>(&mut self, &S, io::Error) -> io::Result<()>
```

`handle_io_error` in particular is exactly the shape a spin loop wants: call
`read`, and when it fails, hand the error back and try again. Nothing asks for a
readiness API, a waker or an executor.

**The data path has no blocking call.** `strace -f`, syscalls attributed to the
thread that wrote the steady-state marker, over 1000 round trips:

| Arm | Syscalls in the region |
|---|---|
| the spin loop the engine uses | `recvfrom` 3033, `sendto` 1000 — **nothing else** |
| the same loop with `poll(2)` in front of the read | `recvfrom` 1000, `sendto` 1000, `poll` 1000 |

The second row is the gate's red half, and it exists because a check nobody has
seen fail is not known to work. It also priced the trade in passing: **spinning
costs about 3.0 `recvfrom` per message where blocking costs 1.0** — two `EAGAIN`
returns per message paying for never leaving user space. That is
[ADR-0013](../decisions/ADR-0013-two-modes-standard-and-hft.md)'s bargain,
unchanged by TLS being on.

### The conditions

Each was measured, not reasoned. They are the reason the answer is "yes, with
conditions" rather than "yes".

| # | Condition | What happens if you ignore it |
|---|---|---|
| 1 | **Hand every `read` error to `ktls_core::Context`.** The kernel refuses to decode TLS control records and returns `EIO` instead | `[measured]` one `EIO` per connection with rustls's default session tickets. A read loop that treats an error as fatal kills the session seconds after the handshake |
| 2 | **Never drain the socket by hand before the handover** | `[measured]` `EBADMSG` (errno 74) on the very next record, and `/proc/net/tls_stat` `TlsDecryptError` 0 → 1. The kernel starts at the sequence number rustls hands it, and that number counts only the records *rustls* processed |
| 3 | **No unprocessed bytes may be left in your own buffer at the handover** | Those bytes are ciphertext the kernel will never see, and its receive sequence number has already counted past them. The spike asserts `leftover == 0`; what a non-zero value does was not tested |
| 4 | **`setup_ulp` needs the socket `ESTABLISHED`** | `ENOTCONN` (errno 107), which reads like a kTLS problem and is not one — see the testing note below |
| 5 | **`TLS_RX` was accepted with a ticket record already queued** on 7.0.0-30. Observed, not guaranteed | Unknown on other kernels. `ktls-core`'s own compatibility table starts at 5.4 |

Two failure modes are worth telling apart because they look alike from the
outside and are not:

- **Sender never offloaded, receiver did** — `EMSGSIZE` (errno 90), and
  `TlsDecryptError` does **not** move. The kernel rejected the framing before it
  ever tried to decrypt.
- **Both offloaded, receive sequence desynchronised** — `EBADMSG` (errno 74), and
  `TlsDecryptError` **does** move.

### What this still does not answer

None of these were touched, and none may be inferred from the above.

- **No latency number.** The spike publishes none, deliberately: `DESIGN.md` §8's
  TLS row stays empty until `tools/w2w` runs the same load three ways on one box.
- **Key update / rekey under kTLS.** ADR-0005 open question 6. `ktls-core` has a
  `tls13-key-update` feature and `Context::refresh_traffic_keys`; neither ran here.
- **TLS 1.2, mutual TLS, SNI, multiple certificates.** ADR-0005 open questions 4
  and 5.
- **The cipher-suite and kernel floor.** ADR-0005 open question 2. This spike
  pinned one suite on one kernel *so that* the result would be attributable.
- **What asserts which mode is live.** ADR-0005 open question 3 — still no gate.

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

## What step 1 did and did not conclude, on the day it ran

**It concluded nothing about `ktls-core` or about ADR-0005** — that came a day
later, above. No
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
exits non-zero with the reason when the answer is no. Run it first on any machine;
`scripts/check-ktls-on-a-plain-socket.sh` calls it before doing anything else, and
skips with exit 2 rather than reporting a green it did not earn.

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

`[to testing-skills]` — **a diagnostic that contradicts its own output, and nothing
tests the diagnostic.** The script printed the evidence (`CONFIG_TLS=m`) and the
opposite conclusion ("built without CONFIG_TLS") four lines apart, in one run, and
that ran for a day. Two properties made it survivable. First, it was a **gate that
nothing gated**: every other check in this repository is exercised by CI, and this
one, being the thing that reads the machine, was assumed to be the thing that
tells the truth. Second, its failure mode was **a confident sentence, not an
error** — exit codes and output were both well-formed, so nothing looked broken.

The cheap defence generalises past kernels and past FIX: **when a checker's output
includes both an observation and a verdict, the mapping between them is pure logic
and can be tested without the system under test.** Eight synthetic cases, no
kernel, no root, and the old logic scores 3/8. The three it passes matter as much
as the five it fails — they are what shows the test is not simply asserting the
new behaviour.

The second defect is the same shape one level down: `lsmod | grep -q` under
`set -o pipefail` reports **failure on the run where the thing is found**, because
`grep -q` exits early and SIGPIPEs the producer. A guard whose success path is the
path that reports failure is invisible until something reads its answer against a
known state — here, unloading the module on purpose and requiring the answer to
change.

### Two more, from writing the spike itself

`[to testing-skills]` — **an assertion on a message's *shape* passes for any
message of that shape, including one your test never caused.** The wire check
started as *"the first bytes are a TLS 1.3 application-data header"*, and it went
green while reading a **session-ticket record the test had not sent**: the sender's
key handover had failed outright, and the receiver was looking at leftover
handshake traffic that has the identical header. Two things were wrong at once and
the transcript showed one `PASS`.

The fix was to assert the size the payload *implies* rather than the type it
*is* — 35 bytes of plaintext, one inner content-type byte, a 16-byte tag, so the
record must declare exactly 52. Proven by reversal: put the tickets back and the
strengthened check reads `record declares 341, needle+type+tag is 52` and fails,
where the shape check had passed. Note what did **not** save it — the companion
assertion *"the plaintext is absent from these bytes"* stayed green throughout,
because a payload that was never sent is trivially absent. **A negative assertion
cannot detect that the thing under test never happened**; it needs a positive one
beside it that only the real event can satisfy.

`[to testing-skills]` — **when two halves cooperate, join both before propagating
either.** The spike runs a peer on a second thread. The first version used `?` on
the main half, so when the worker failed, its error was discarded *and* the socket
it owned was closed — and the main half then reported `ENOTCONN` from
`setsockopt`, a syscall that was entirely healthy. The transcript named the
symptom and hid the cause, and the obvious next move — reading kernel source about
when `TCP_ULP` returns `ENOTCONN` — was research into the wrong question. Joining
first turned one misleading line into the worker's actual error on the next run.
This generalises past threads to any fixture with a peer process, a container, or
a mock server: **the half that fails first is usually not the half that reports.**

