# ADR-0042 — Operator intent is an API, and a second implementation is the only independent opinion

- **Status**: Accepted — 2026-09-02
- **Date**: 2026-09-02
- **Deciders**: Tran Manh Thang
- **Related**: [ADR-0004](ADR-0004-bidirectional-engine.md) — decision 5 asked for this gate
  and decision 7 bounded where C++ may live · [ADR-0006](ADR-0006-mirrored-corpus-is-fifty.md)
  — the mirrored corpus this replaces as the primary initiator gate ·
  [ADR-0001](ADR-0001-relationship-to-quickfix.md) — QuickFIX as data and oracle, never as
  source · [DESIGN.md D1](../DESIGN.md) — the purity this had to be built inside ·
  [plan](../plans/2026-09-02-the-initiator-and-its-second-opinion.md)

## Context

Phase 1 exit criterion 4 has two halves. The acceptor half was met on 2026-08-29 at 59 / 59.
The initiator half asks for *"interop-green against `libquickfix` in CI"*, and it had been open
since then behind two problems that turned out to be one problem.

**The first problem is that a pure state machine cannot originate.** `[measured 2026-08-30]`
46 of the 50 mirrorable acceptance definitions require this end to send a message that nothing
on the wire asks for and no clock produces — 42 of them a `Logout`. `DESIGN.md` D1 says the
session layer has no socket and no clock; time arrives as `Input::Tick`. Neither a tick nor an
inbound message can produce a `Logout`, so the mirrored corpus sat at **0 / 50** and the plan
was paused rather than forced.

**The second problem is that the mirrored corpus cannot check its own reading.** Mirroring is
this repository's interpretation of a suite written for the other direction. Its ceiling moved
**51 → 50 → 45** across two readings before a line of it was green, each time because somebody
read the corpus more carefully — which is precisely the failure mode a gate built on that
reading cannot report.

## Decision

### 1. An operator's intent is an API, and there is no back door for bytes

Six functions on `Session<R, N>` — three that existed and three added here:

| | Caller supplies | Session keeps |
|---|---|---|
| `send_heartbeat` | nothing | `8`, `9`, `34`, `49`, `52`, `56`, `10` |
| `send_test_request(id)` | `112` | as above |
| `send_resend_request(from, to)` | `7`, `16` | as above |
| `send_sequence_reset(n)` | `36` | as above |
| `begin_logout(text)` | `58` | as above |
| `send_application(msg, journal)` | the body | as above |

**No function takes whole message bytes.** That was the cheaper design and it is rejected: a
harness that could hand the session a finished message would let the mirrored gate compare the
corpus against itself, and a *user* holding that door could write `34=` by hand, which
`DESIGN.md` D9 and non-negotiable 5 both exist to prevent.

The session remains pure. Each function is a `Template` patch through the one code path that
writes `34=` and `52=`, and `benches/alloc.rs` case `ordered` reads 0 — proven by injecting a
`to_vec()`, which reads 10 000.

### 2. The interop gate is the primary initiator gate; the mirrored corpus is secondary

`scripts/interop.sh` builds `libquickfix` from source at the **same pinned commit**
`scripts/fetch-quickfix-assets.sh` uses, runs it as an acceptor, drives this engine's initiator
through seven steps, and reads the printed transcript.

Two pins that can drift apart would be one pin: a disagreement between the acceptance corpus
and the C++ counterparty would be unattributable. The script refuses to run if they differ.

### 3. C++ lives in `scripts/` and `tools/`, and nowhere else

ADR-0004 decision 7, unchanged and now load-bearing. `tools/interop` is an ordinary Rust crate;
its `Cargo.toml` mentions no C++. No `build.rs` changes. `cargo test --all
--no-default-features` on a machine with no CMake is unaffected, and
`scripts/check-no-optional-deps.sh` still asks per crate.

### 4. The interop tool drives the **session**, not the engine

`Session<Initiator, 256>` over a blocking `TcpStream`, not `fixbolt_engine::Engine`. Criterion
4 is about the protocol, and the engine's polling loop already has `crates/engine/tests/wire.rs`
and `tools/w2w` over the same kernel sockets. **The consequence is stated below rather than
left to be discovered.**

## What it found on its first run

Before the gate had ever been green, it failed five of seven steps — and none of the five was
the broken one.

**This engine's initiator answered a `Logon` with a `Logon`.** One line in the inbound-Logon
handler, shared by both roles, correct for the acceptor, unconditioned. `libquickfix` took the
second Logon, dropped the connection **without a word**, and everything after that failed
because the socket was gone.

Green on that defect, at the moment it was found:

| Gate | Reading |
|---|---|
| `--test score` | 59 / 59 — the line is *correct* for an acceptor |
| `--test mirror` | 0 / 50, exactly as asserted — a gate pinned at a constant cannot fall |
| `cargo test --all` | 430 passed, 0 failed |
| `clippy -D warnings`, `fmt` | clean |
| `benches/alloc.rs` | `logon_out 0` — the wrong message allocated nothing |

Six green gates, and one of the two roles could not complete a handshake with anybody real.
[reference/a-role-can-be-wrong-in-a-direction-no-gate-runs.md](../reference/a-role-can-be-wrong-in-a-direction-no-gate-runs.md).

## Consequences

### Good

- **Criterion 4 has a command that can fail.** `PRD.md` §2: *"a criterion nobody can run is not
  one"*. It is blocking in CI and it went red, twice on purpose and once by accident.
- **One class of defect this repository could not see is now visible**: code shared between two
  protocol roles, correct in the direction the corpus covers. Nothing else here could have found
  it, and the write-up names the coverage axis so it can be asked about deliberately.
- **The initiator is usable.** It logs on, keeps a session alive, asks for and answers resends,
  gap-fills a hole it opened itself, and says goodbye — against QuickFIX, not against this
  project's own reading of QuickFIX.
- **The mirrored corpus can now be worked on honestly.** The origination API is what it was
  missing, and the interop gate stands in front of it, which is what ADR-0004 decision 5 said
  it was for.

### Bad, and accepted

- **C++ is in CI.** CMake, a ~3-minute build of somebody else's library, and a job that can go
  red for reasons that are not Rust. ADR-0004 weighed this and the plan's risk table re-stated
  it before the work began. Mitigation: its own job, blocking but isolated; the other nine jobs
  do not wait on it.
- **The gate is one scenario, not a suite.** Seven steps against one counterparty in one
  configuration. It is not a conformance corpus and must not be described as one — what it is
  is an *independent* opinion, which the 59 files are not.
- **It does not cover the engine's polling loop, reconnect, backoff, or schedules.** An
  initiator that reconnects after a drop is untested by anything here, and `.def` files cover
  none of it either (ADR-0004 already records that as debt). `STATUS.md` carries it.
- **A second pin to maintain.** Moving the QuickFIX ref now means moving it in two scripts and
  re-reading two sets of numbers. The script fails loudly rather than drifting, which is the
  cheapest form of this cost but not zero.
- **The counterparty is a maintenance dependency.** If upstream QuickFIX changes behaviour at a
  future pin, this job goes red for a reason that is not a regression here. That is the price of
  an opinion that is not ours, and it is the reason the opinion is worth having.

### What is explicitly not claimed

- **Nothing about latency.** Interop is a correctness gate and prints no timing.
- **Nothing about phase 1 exit criterion 6.** That needs a machine matching `DESIGN.md` §9 and
  is untouched by any of this.
