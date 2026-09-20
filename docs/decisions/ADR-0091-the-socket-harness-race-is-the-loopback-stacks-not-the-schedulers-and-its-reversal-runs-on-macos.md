# ADR-0091 — The socket harness race is the loopback stack's, not the scheduler's: it is reachable on macOS, unreachable on Linux, and its reversal runs on the machine that has it

- **Status**: Accepted — 2026-09-20, under the owner's delegation of
  [the-desk-free-residue](../plans/2026-09-20-the-desk-free-residue.md)'s technical decisions.
  **Supersedes ADR-0087 in part**: its *Context* premise (a descheduled process) and its
  decision 5's reversal clause. ADR-0087 decisions 1–4 **stand**, are built in `3233032`, and
  are not restated here.
- **Date**: 2026-09-20
- **Deciders**: Tran Manh Thang (mandate); written by the architect after the builder's report.
- **Related**: [ADR-0087](ADR-0087-a-socket-harness-settles-on-counted-records-and-the-clock-waits-for-the-engine.md),
  [ADR-0013](ADR-0013-two-modes-standard-and-hft.md) (proven in one place is
  proven in neither — the same shape, for platforms),
  [the-same-commit-went-red-and-green-in-the-same-minute](../reference/the-same-commit-went-red-and-green-in-the-same-minute.md)
  (the counts, and now the correction), `CLAUDE.md` §2 non-negotiables 3 and 10.
- **Touches**: this file, ADR-0087's status line, the reference page above, the plan's *Sửa 1*.
  **Not** `crates/`, not `scripts/`, not `.github/`.

## Context

ADR-0087 was written this morning from one recorded observation:
`the_sixty_fix50sp2_definitions_pass_through_a_real_socket` read **0 red in 40** sequential runs,
**11 red in 50** as ten concurrent copies, **8 red in 50** the same way on `main` at `64ea6c2`.
It named a mechanism — the runner ticks the simulated clock a whole `HeartBtInt` when `pending`
is empty, and over a socket "empty" can mean "late" — and it required, as the step's reversal,
at least one red in fifty on the unfixed tree.

The step (`3233032`, worktree `fb-prb`) built decisions 1–4 and then could not produce the red.
`[measured 2026-09-20]` on the desk (`tmt-B450-I-AORUS-PRO-WIFI`, Ryzen 7 3700X, 16 logical
CPUs, Linux `7.0.0-31-generic`, ordinary desktop boot line, three other agents compiling):
about 670 runs over nine contention shapes — 5×10, 2×48 and 2×96 copies of `wire_fixt`; 5×10 of
`wire`; ten copies pinned to two cores, then to one; ten copies against six spinners on one core
(12.85 s a run against 0.87 s idle); sixteen copies against 32 spinners; ten copies under a
`CPUQuota=100%` cgroup — **zero red**. A temporary counting log on the *unfixed* harness,
printing whenever a `Tick` arrived while the engine still owed a read or the harness still owed
a drain, recorded **0 debt events in 122 contended runs**, with 365 `Tick`s per run whether idle
or squeezed onto one CPU.

So the question put to the architect was: is the mechanism wrong, or the reversal?

### What the evidence says, in order

**1. The original figures were not taken on this desk, and the bullet did not say where.**
`journalctl --list-boots`: the previous boot ended `2026-09-19 16:23 +07` and the current one
began `2026-09-20 12:33 +07`; the commit that recorded the counts, `caaf14e`, is dated
`2026-09-20 08:51 +07`. The neighbouring commits of that session say where it ran: `8e81aae`
and `6e84ef1` — "Gates on the laptop"; `9fdbeaa` — "A/B on this laptop (Apple M5 …)". **The
11 / 50 and 8 / 50 were measured on a macOS laptop.** The bullet, the reference page and
ADR-0087 all carried the numbers without the machine — the exact omission `CLAUDE.md` §2 item
10 exists to forbid, and the reason a day was spent hunting a race on the wrong kernel.

**2. The recorded failure output admits only one mechanism, and it is ADR-0087's.** The extra
message has 8 fields and the displaced `Logout` carries `34=5`, so the engine really numbered
five outbound messages: `Logon` 1, `ResendRequest` 2, **X** 3, the `35=D` echo 4, `Logout` 5.
X is an 8-field message the engine sent on its own; `crates/session/src/lib.rs:2247-2248` and
`:2510` say one tick of `HeartBtInt` after the engine last spoke produces a `Heartbeat`
(`8,9,35,34,49,52,56,10` — eight fields) and two ticks a `TestRequest` (nine). The engine's
clock is `ManualClock` and moves only on `Input::Tick`; the runner sends a `Tick` only at an
`E` line with `pending` empty. Therefore a `Tick` reached the engine at line 16 or 18 of
`3b_InvalidChecksum.def` while an answer was in flight. That is decision 2's premise, and the
output cannot be explained without it. **The mechanism is confirmed by the output**, not by the
desk.

**3. The lateness is not the scheduler's; it is the loopback stack's, and the two kernels
differ.** The harness is single-threaded: the same thread writes the client socket and turns
the engine. On Linux, `write()` on a loopback socket runs the receive path in the caller's
context — `loopback_xmit` → `netif_rx` → `NET_RX_SOFTIRQ`, processed on the way out of the
syscall — so by the time `write_all` returns the bytes are already in the peer's receive
queue, and the pump's first `turn()` reads them. No amount of CPU contention changes that
order, which is why nine shapes read zero and the counting log read zero. On XNU the same
`write()` ends in `lo_output` → `ifnet_input_extended` → `ifnet_input_common`, which enqueues
the packets to `lo_rcvq_pkts` and calls `dlil_input_wakeup`; **the main DLIL input thread** —
one thread, shared by every `lo0` packet on the machine — delivers them later. The bytes are
not readable when `write()` returns, and under load the wait is bounded by that one thread's
scheduling.

**4. Measured, on both machines, with a probe outside the repository.** The probe (appendix B)
writes 96 bytes to a loopback TCP socket with `TCP_NODELAY`, reads the peer non-blocking at
once, counts a first-read `EAGAIN`, and times the wait until the bytes arrive; 20 000
round trips per copy. `[measured 2026-09-20]`:

| Machine | Copies | First read `EAGAIN` | Delay p50 / p99 / p999 / max (µs) | Waits ≥ 1 ms per 20 000 |
|---|---|---|---|---|
| Desk, Linux `7.0.0-31-generic`, 16 CPUs | 1 | **0** (0.00 %) | 1.3 / 1.9 / 6.1 / 23.0 | **0** |
| Desk, Linux, same | 10 concurrent | **0** in every copy | 1.3–2.0 / 1.7–3.7 / 5.2–8.0 / 9.3–19.2 | **0** in every copy |
| Mac mini `Mac16,10`, macOS 26.6.2 (Darwin 25.6.0), 10 CPUs, `net.link.loopback.sched_model=0` | 1 | **19 181** (95.91 %) | 7.4 / 23.6 / 63.8 / 85.4 | 0 |
| Mac mini, same | 10 concurrent | 19 969–19 987 (99.84–99.94 %) | 53.8–54.1 / 71.2–73.9 / **1 871–2 675** / **5 216–9 437** | **34–55** in every copy |

`Wire::pump` gives up after `STEP_QUIET = 1 ms` of nothing moving. On the Mac under ten copies
the delivery crosses 1 ms roughly 0.2 % of the time; a run of the corpus is a few hundred
steps, and the rate of 11 red in 50 runs is the order of magnitude that predicts. On Linux the
worst delivery in 220 000 was 23 µs, forty times inside the window. **The race ADR-0087
described is real, reachable on macOS, and unreachable on Linux.** The desk was the wrong
instrument, not the wrong answer.

**5. "Concurrent copies" and "synthetic CPU load" are different experiments, and the
difference is a shared resource.** The builder's spinners loaded CPUs; ten copies of the suite
load the one DLIL input thread that every loopback packet on a Mac passes through. That is
the same shape as the ephemeral-port and `TIME_WAIT` exhaustion that makes concurrent socket
suites flake where CPU load does not (see *Sources*): the contended resource is in the kernel's
network path, and only traffic contends for it. On Linux the loopback path has no such
serialising thread in the common case, so there was nothing for ten copies to contend for.

### What the search found

- **XNU**: `bsd/net/dlil.c` — "Main input thread: a) handles all inbound packets for lo0
  b) handles all inbound packets for interfaces with no dedicated input thread … c) protocol
  registrations d) packet injections" (comment above `dlil_main_input_thread_cont`,
  ~line 3545); `ifnet_input_common` (~line 3159) enqueues to `lo_rcvq_pkts` and calls
  `dlil_input_wakeup`. `bsd/net/if_loop.c` — `lo_output` ends in
  `return ifnet_input_extended(ifp, m_list, m_tail, &s);`; the sysctl
  `net.link.loopback.sched_model` exists and reads `0` on the Mac mini.
- **Linux**: the receive path for a loopback frame is `netif_rx` → backlog → `NET_RX_SOFTIRQ`,
  run on softirq exit in the sending context; the loopback case is special-cased in the stack
  walk ("a poor man's longjmp … to keep the depth in check"). Deferral to `ksoftirqd` exists
  when the softirq budget is exhausted, and the probe under ten copies did not reach it.
- **QuickFIX and QuickFIX/J**: no documented flake of the acceptance runners was found. Their
  runners do not simulate time (ADR-0087, *What the search found*), so they have no `Tick` to
  send early; the only timing knob is QuickFIX/J's `atest.timeout` (10 s), a wait for the
  expected message, and the mailing-list and JIRA traffic about "timed out waiting for
  heartbeat" is about production sessions, not the test runner.
- **Concurrent socket suites**: the reports found (miniflare, morph, cryptos, bindex-rs) all
  name a kernel-side shared resource — ephemeral ports, `TIME_WAIT`, a just-closed port
  handed to two processes — and all note that CPU load alone does not reproduce them. That is
  the distinction the builder's nine shapes could not have separated on Linux, because the
  resource this race contends for does not exist there.

## Decision

### 1. ADR-0087 decisions 1–4 stand, as built in `3233032`

The counted settle, the tick that waits, the 5 s lifeline that is reported, the untouched
runner and oracle. They are the fix by construction for the race this ADR confirms: a step no
longer ends on a wall-clock quiet at all, so the DLIL thread's lateness can no longer be read
as silence. Nothing in this ADR reopens them.

### 2. The reversal moves to the machine that has the race, and it is the step's closing evidence

ADR-0087 decision 5's clause "on the tree before step 4 it must read at least one red in
fifty" is **withdrawn for Linux** and **kept for macOS**. The reversal is executed on a macOS
machine with `net.link.loopback.sched_model` reading `0` — the Mac mini over the cable or the
M5 laptop — as follows, and the developer quotes every line:

1. Record the machine: `sysctl -n hw.model hw.ncpu; sw_vers -productVersion;
   sysctl net.link.loopback.sched_model; uptime`.
2. On the tree **before** `3233032` (its parent, or `main` at `64ea6c2`), build
   `cargo test -p fixbolt-engine --features fix50sp2 --test wire_fixt --no-run` and run
   `scripts/check-socket-corpus-under-contention.sh 5 10` (the script from `3233032`, copied
   beside the old tree if needed). Expected: `N red in 50`, N ≥ 1, and in at least one red the
   pair `FieldCount { expected: 14, actual: 8 }` / `FieldCount { expected: 8, actual: 14 }`.
   The builder's temporary counting log, if re-applied, is expected to print ≥ 1 debt event.
3. On `3233032` or later, the same command: expected `0 red in 50`, `lifeline hit: 0` in every
   run.
4. Both counts, the machine line and the commands go into the plan's *Nhật ký giao hàng* and
   into `docs/CONFORMANCE.md` §9 beside the 60 / 60 line.

If step 2 reads **0 red in 50 on the Mac too**, this ADR is falsified in its turn: the item is
then recorded as *not reproducible on either platform on 2026-09-20*, the harness fix stays
(it is correct by construction and cheap), and the item stays open with the reopening
condition of decision 4. The architect is told, with the output.

### 3. `assert_eq!(lifeline_hits, 0)` stays, and a lifeline red is an environment red, said so

The builder asked whether turning a desperately slow runner from a wrong score into a red is
the right call. It is, for three reasons that the builder's own reversal supplies:

- **The score cannot see it.** With the lifeline cut to 1 µs the suite scored 60 / 60 while
  settling on a timeout 1 087 times. A gate that stays green while its settle is broken is a
  gate that has stopped measuring; the count is the only thing that can tell those apart.
- **A hit is five seconds.** The worst loopback delivery measured on either machine under ten
  copies is 9.4 ms — five hundred times inside the lifeline; the one-CPU cgroup read 0 hits.
  A step that waits 5 s for a fact is a broken engine or an unusable machine, and both must be
  red rather than quietly green.
- **A red that is counted beats a green that is wrong** — the lesson the reference page
  already carries. A lifeline red names the file and the unmet fact; the operator's rule is:
  one re-run, recorded; two in a row is a machine investigation, not a code one.

Two consequences are accepted with it: the assertion is a new way for CI to be red that the
engine did not cause, and `E` line numbers do not reach the harness (`SessionUnderTest::step`
takes one `Input`; reaching them would edit `crates/conformance`, frozen by ADR-0087 decision
4), so the lifeline names file and fact, not line. File and fact are enough to find the step.

### 4. What the item's status is, and what reopens it

Until decision 2's reversal is quoted, the item stays under *Not proven* as **"fixed by
construction, reversal owed on macOS"** — the harness can no longer take the race, the race is
shown reachable on macOS by the probe, and shown unreachable on Linux by the probe and 670
runs; what is not yet shown is the corpus itself going red then green on the fixed harness.
After the reversal the bullet is struck.

It reopens on any of: a red of `wire.rs` or `wire_fixt.rs` whose output carries the shifted
pair, on any platform; a `lifeline hit` above zero in CI or on a recorded run; or a change to
`STEP_QUIET`, the lifeline, or the counting settle. Each reopening records the machine, the
kernel and — on macOS — `net.link.loopback.sched_model`.

### 5. The Linux contention step stays in CI, as a counter and not as the reversal

`scripts/check-socket-corpus-under-contention.sh` in the `gates` job runs on a Linux runner
and, by this ADR, is not expected to see the race. It stays because it counts — a red there is
a new mechanism, not this one — and because it costs a minute. **No macOS CI job is added**:
the race is a property of the old harness on XNU and the harness is fixed by construction; a
macOS runner would spend ten times the minutes watching a fixed harness. If a reopening ever
comes from macOS, that decision is revisited.

## Alternatives considered

| Alternative | Why not |
|---|---|
| Amend ADR-0087 in place | It is `Accepted`; `CLAUDE.md` §5 forbids editing its substance, and its premise sentence is exactly what was wrong |
| Refuse the item as unreproducible | It reproduced eleven times in fifty on the laptop, and the output admits one mechanism; "unreproducible" would mean "not on the machine we happened to use" |
| Declare the mechanism unknown and start a bisection of the environment | The output already fixes the mechanism (a `Tick` while an answer is in flight); what was unknown was the platform, and the probe answers that |
| Add a macOS CI job as the permanent reversal | Ten times the minutes to watch a harness that can no longer take the race; decision 5 says when to revisit |
| Make the lifeline a warning rather than an assertion | The 1 µs reversal shows a warning nobody reads leaves a 60 / 60 that is false |
| Raise `STEP_QUIET` to cover the Mac's p999 | The move `tests/wire.rs` records as measuring the scheduler; and ADR-0087 decisions 1–2 already removed the quiet from the decision |

## Consequences

**Good**

- The mechanism is confirmed by two independent readings — the failure output, and a probe
  on the kernel that shows it — rather than inferred from code. The fix is not "it went away".
- The reversal is executable and cheap: one macOS machine over the cable, two builds, one
  script, a table of numbers to quote.
- A platform is now a variable this repository names in a flake report. The rule that a
  number carries its machine (§2 item 10) is re-learned with a day's price attached.
- The lifeline assertion has its rationale on record, so the first CI red it causes is
  answered by a page and not by a debate.

**Bad — and accepted**

- The reversal is owed, not done: this ADR closes the *decision*, and the item stays under
  *Not proven* until a developer quotes decision 2's two counts from the Mac.
- The Linux contention step in CI costs a minute per run to count a race it cannot have.
- ADR-0087 carries a wrong sentence in its *Context* that this ADR can only point at, not
  remove. Readers of ADR-0087 must read its status line.
- The probe is in an ADR appendix, not in `scripts/`, because the architect does not touch
  `scripts/`; if it is ever wanted as a gate, a developer moves it and this appendix points
  there.

## Sources

- XNU, `bsd/net/dlil.c` — the main input thread comment and `ifnet_input_common`:
  <https://github.com/apple/darwin-xnu/blob/main/bsd/net/dlil.c>. `bsd/net/if_loop.c` —
  `lo_output` → `ifnet_input_extended`: <https://github.com/apple/darwin-xnu/blob/main/bsd/net/if_loop.c>.
  Read 2026-09-20.
- Linux receive path, softirq on the sending CPU, and the loopback special case:
  <https://blog.packagecloud.io/illustrated-guide-monitoring-tuning-linux-networking-stack-receiving-data/>,
  <https://www.privateinternetaccess.com/blog/linux-networking-stack-from-the-ground-up-part-3/>,
  and the RFC that would defer NET_RX to `ksoftirqd` (not merged as such):
  <https://lkml.iu.edu/hypermail/linux/kernel/1801.2/02301.html>.
- Concurrent socket suites flaking on a kernel-side shared resource rather than on CPU load:
  <https://github.com/cloudflare/workers-sdk/issues/15716>,
  <https://github.com/LASTRADA-Software/morph/issues/559>,
  <https://github.com/CryptOS-PKI/cryptos/issues/189>,
  <https://tech.bluesmoon.info/2011/09/limits-of-network-load-testing-ports.html>.
- QuickFIX/J `AcceptanceTestSuite.java` (the `atest.timeout` knob):
  <https://github.com/quickfix-j/quickfixj/blob/master/quickfixj-core/src/test/java/quickfix/test/acceptance/AcceptanceTestSuite.java>;
  QuickFIX acceptance-test documentation:
  <https://quickfixengine.org/c/documentation/testing/acceptance-tests.html>. **No documented
  flake of either runner was found.**
- This repository: `crates/session/src/lib.rs:2247-2248, 2510` (one tick → `Heartbeat`, two →
  `TestRequest`); `crates/conformance/src/runner.rs:441-447` (the tick on empty `pending`);
  `git show 64ea6c2:crates/engine/tests/wire_fixt.rs` lines 160–185 and 254–258 (the 1 ms
  quiet and the tick); commit `3233032` (the builder's nine shapes and the counting log);
  `journalctl --list-boots` on the desk; commits `8e81aae`, `6e84ef1`, `9fdbeaa` (the laptop).

---

## Appendix A — replacement text for `STATUS.md` *Not proven* bullet (a)

The manager owns `STATUS.md`; this is the bullet to paste in place of the one that begins
"**`the_sixty_fix50sp2_definitions_pass_through_a_real_socket` is load-dependent, and it is
pre-existing.**" (`STATUS.md` line 117 on 2026-09-20). **When pasting, prefix the ADR-0091 link's
target with `docs/decisions/`** — it is written relative to this directory so that
`scripts/check-links.py` can resolve it here.

```markdown
* **`the_sixty_fix50sp2_definitions_pass_through_a_real_socket` was load-dependent on macOS,
  is fixed by construction, and its reversal is owed on macOS.** `[measured 2026-09-20]` The
  original counts — 0 red in 40 sequential, **11 red in 50** as ten concurrent copies, **8 red in
  50** on `main` at `64ea6c2` — were taken on the **Apple M5 macOS laptop** (commits `8e81aae`,
  `6e84ef1`, `9fdbeaa` name it; the bullet that first carried them did not), while the desk was
  powered off (`journalctl --list-boots`). On the desk (Linux `7.0.0-31-generic`, 16 CPUs) the
  builder ran ~670 runs over nine contention shapes and a counting log on the unfixed harness:
  **0 red, 0 debt events in 122 runs** (`3233032`). The mechanism is ADR-0087's — a `Tick`
  reaching the engine while an answer is in flight; the output admits no other — but the
  lateness is the loopback stack's, not the scheduler's: XNU delivers `lo0` bytes through one
  shared DLIL input thread, Linux delivers them inside the sender's `write()`. A probe outside
  the repository measured it: Mac mini, ten concurrent copies, **34–55 deliveries ≥ 1 ms per
  20 000** (max 9.4 ms) against the harness's 1 ms quiet; desk, **0 in 220 000** (max 23 µs).
  [ADR-0091](ADR-0091-the-socket-harness-race-is-the-loopback-stacks-not-the-schedulers-and-its-reversal-runs-on-macos.md).
  **Fixed by construction** in `3233032` (ADR-0087 decisions 1–4: counted settle, the tick
  waits, a 5 s lifeline asserted to be hit 0 times — with the lifeline at 1 µs the suite still
  scored 60 / 60 while timing out 1 087 times, so the count is the only assertion that can see
  it). **Owed**: ADR-0091 decision 2 — on a macOS machine with `net.link.loopback.sched_model`
  = 0, `scripts/check-socket-corpus-under-contention.sh 5 10` on the tree before `3233032`
  reading ≥ 1 red in 50 with the shifted `FieldCount` pair, then 0 red in 50 and `lifeline hit:
  0` on the tree after; machine line and both counts quoted here. **Reopens on**: a red of
  `wire.rs`/`wire_fixt.rs` carrying the shifted pair on any platform, a `lifeline hit` above
  zero anywhere, or a change to `STEP_QUIET`, the lifeline or the counting settle — each
  recorded with the machine and kernel. The Linux CI contention step stays as a counter; it is
  not expected to see this race.
```

## Appendix B — the loopback-delivery probe, verbatim

Not project code; kept here so the table above can be re-run. Save as `loopback_probe.py`,
run `python3 loopback_probe.py 20000` once, then ten copies with `&` and `wait`. Needs only the
standard library (Python 3.9 on the Mac mini, 3.x on the desk).

```python
# Research probe, not project code: does a write to a loopback TCP socket make the
# bytes readable on the peer before write() returns?  Counts first-read EAGAIN and
# the wall time until the bytes arrive.  Usage: python3 loopback_probe.py [iters]
import socket, sys, time, platform
n = int(sys.argv[1]) if len(sys.argv) > 1 else 20000
srv = socket.socket(); srv.bind(("127.0.0.1", 0)); srv.listen(1)
cli = socket.create_connection(srv.getsockname())
cli.setsockopt(socket.IPPROTO_TCP, socket.TCP_NODELAY, 1)
acc, _ = srv.accept(); acc.setblocking(False); cli.setblocking(False)
payload = b"8=FIXT.1.1\x019=0\x01" + b"x" * 80
eagain = 0; delays = []
for _ in range(n):
    cli.send(payload)
    t0 = time.perf_counter()
    first = True
    while True:
        try:
            acc.recv(4096); break
        except BlockingIOError:
            if first: eagain += 1
            first = False
    delays.append(time.perf_counter() - t0)
    # answer back the other way and drain, so both directions are exercised
    acc.send(payload)
    while True:
        try:
            cli.recv(4096); break
        except BlockingIOError:
            pass
delays.sort()
p = lambda q: delays[min(n - 1, int(q * n))] * 1e6
print(f"{platform.system()} {platform.release()} iters={n} first_read_EAGAIN={eagain} "
      f"({100.0*eagain/n:.2f}%) delay_us p50={p(0.5):.1f} p99={p(0.99):.1f} p999={p(0.999):.1f} max={delays[-1]*1e6:.1f} over_1ms={sum(1 for d in delays if d >= 1e-3)}")
```

Output on the desk, one copy, `[measured 2026-09-20]`:

```
Linux 7.0.0-31-generic iters=20000 first_read_EAGAIN=0 (0.00%) delay_us p50=1.3 p99=1.9 p999=6.1 max=23.0 over_1ms=0
```

Output on the Mac mini, one copy, then the worst of ten concurrent copies:

```
Darwin 25.6.0 iters=20000 first_read_EAGAIN=19181 (95.91%) delay_us p50=7.4 p99=23.6 p999=63.8 max=85.4 over_1ms=0
Darwin 25.6.0 iters=20000 first_read_EAGAIN=19987 (99.94%) delay_us p50=53.8 p99=72.4 p999=2236.9 max=8104.7 over_1ms=55
```
