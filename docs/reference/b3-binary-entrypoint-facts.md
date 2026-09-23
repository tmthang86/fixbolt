# B3 Binary EntryPoint facts, and the traps building the FIXP spike hit

`[measured 2026-09-23, desk tmt-B450-I-AORUS-PRO-WIFI, phase 3 row 9, docs/plans/2026-09-23-p3-fixp-spike.md]`

The FIXP spike (`scripts/fixp-spike.sh`, `spikes/fixp-probe/`, ADR-0140) is `fixbolt-sbe`'s first
conversation with a Real-Logic-generated Binary EntryPoint decoder over a real socket. This page
is two things: what the protocol and Artio's 0.184 implementation of it actually do — each fact
paired with the arm or check that stands behind it, or a plain statement that nothing does — and
the traps this repository paid for while wiring it up. `docs/CONFORMANCE.md` §10 is the result;
this page is why each part of it reads the way it does.

## The protocol, as Artio speaks it

### 1. The framing is little-endian and compact, not the FIXP-standard header

B3's Simple Open Framing Header is 4 bytes: a `uint16` message length **including the header**,
little-endian, then `uint16` `0xEB50`, little-endian
(`artio-codecs/.../fixp/SimpleOpenFramingHeader.java:25-62`, cited in ADR-0140). The FIXP
Technical Standard's own SOFH is a 4-byte **big-endian** length followed by a big-endian encoding
tag — a different shape entirely, not merely a different tag value. `crates/sbe` does not know
about framing at all (ADR-0140 decision 1); `spikes/fixp-probe/src/main.rs` writes these 4 bytes
by hand on every message, citing the same file.

**Guarded by:** plan row 3 reversal A — flipping `0xEB50` to `0xEB51` in the probe's framing must
turn `accept: negotiate` into a `FAIL` and name whether Artio closed the socket or stayed silent.
This is a **manual reversal**, run once and restored (docs/plans/2026-09-23-p3-fixp-spike.md,
*Chia việc* row 3); no automated test re-runs it, so a regression here would only be caught by
`accept` itself failing for an unrelated-looking reason.

### 2. The referee's schema is three major versions behind the venue's

The referee decodes `binary_entrypoint.xml` (`id="1" version="5" semanticVersion="5.6"`) — the
copy Artio ships inside `artio-binary-entrypoint-codecs-0.184.jar`, dated 2022-08-24. B3 currently
publishes `b3-entrypoint-messages-8.4.2.xml` (`version="6" semanticVersion="8.4.2"`). A green spike
says fixbolt speaks **Artio's** dialect, not B3's production one; `docs/CONFORMANCE.md` §10 states
this plainly rather than letting the spike's summary line imply more.

**Guarded by:** the schema's own SHA-256 pin in `scripts/fixp-spike.sh`
(`c31fcd6228e613fa6ee3af441832393a4a35c30263bb5cd80929f16529af4a71`) — a `CHECKSUM MISMATCH`
before anything compiles or runs if the extracted file ever differs from the one this page and
ADR-0140 describe, whether from an Artio upgrade or a mistake in the extraction step. **Nothing
guards against silently targeting 8.4.2 instead** — that is a deliberate non-goal of this plan
(ADR-0140 decision 4), and would need a second `sbe-gen` fix (`presence` on a composite-typed
field) the FIXP ADR is left to schedule.

### 3. Artio's sending-time window is 2 minutes, and the referee says so out loud

`CommonConfiguration`'s default sending-time window is 120 000 ms. The referee is started with
`--sending-time-window-ms 120000` explicitly (not left to Artio's default, so a future Artio
release changing its own default cannot silently change what this spike tests) and prints
`referee: limits sending-time-window=120000ms …` on every run.

**Guarded by:** arm `reject-timestamp` — a Negotiate dated one hour stale must be refused with
`NegotiateReject INVALID_TIMESTAMP(7)`. Arm `accept` is the other side of the same fact: it sends
a Negotiate timestamped with the real clock at send time and must be accepted.

### 4. Artio's keep-alive bounds are `[1 ms, 60 000 ms]`, and only the accepting side is exercised

`InternalBinaryEntryPointConnection` refuses an `Establish.keepAliveInterval` outside
`[minKeepaliveMs, maxKeepaliveMs]` with an `EstablishReject`. The referee is started with
`--keepalive-min-ms 1 --keepalive-max-ms 60000 --acceptor-keepalive-ms 30000` and prints
`referee: limits … keepalive-min=1ms keepalive-max=60000ms acceptor-keepalive=30000ms`; the probe
sends `10000` (`PROBE_KEEPALIVE_MS`), comfortably inside the window, and asserts
`EstablishAck.keepAliveInterval == 30000` — the **acceptor's** value, not an echo of what the
probe sent.

**Guarded by:** arm `accept`'s `establish` step, for the inside-the-window case only.
**Nothing in this spike sends a keep-alive outside `[1, 60000]` to prove the `EstablishReject`
side fires** — the plan named this as a trap to watch for (*Bẫy đã lường trước*: "`keepAliveInterval`
ngoài giới hạn min/max của Artio → EstablishReject") and its only guard today is the referee
printing its limits so a human can compare them against whatever value a future change sends. A
fourth arm exercising the reject path is unbuilt.

## Traps hit building the spike

### 5. Artio binds without `SO_REUSEADDR`, so a fixed port dies in `TIME_WAIT`

`DefaultTcpChannelSupplier.bind` calls `ServerSocketChannel.bind` with no `SO_REUSEADDR` — JDK NIO
does not set it by default. Artio also closes its side of the connection first: after echoing a
client `Terminate`, or at its own authentication timeout following a library-level
`NegotiateReject` (trap 8 below). The side that closes first is the side whose port goes into
`TIME_WAIT` for roughly a minute. `[measured 2026-09-23 on this desk]`: the third arm of the first
run against one fixed port failed `FixEngine.launch` with
`BindException: Address already in use`.

**Guarded by:** every arm asks the kernel for a port that is bindable *now* (`pick_port()` in
`scripts/fixp-spike.sh`) rather than reusing one; the port is printed so a failure can be read
against it. `FIXP_REFEREE_PORT` overrides this with one fixed port for every arm, entirely at the
caller's risk — see trap 6.

### 6. A fixed referee port must not collide with `scripts/interop-qfj.sh`'s fixed ports

`spikes/fixp-probe/referee/Referee.java`'s `Args` class used to default `port` to `15660` when
`--port` was not given on its command line — dead code for `scripts/fixp-spike.sh`, which always
passes an explicit, freshly-picked port (trap 5), but a live landmine for anything that invokes
`Referee` directly. `scripts/interop-qfj.sh`, on the sibling branch `plan/p3-quickfixj-interop`,
fixes **exactly that number** as its own `PORT_ACCEPTOR_PLAIN` (with `PORT_INITIATOR_PLAIN`,
`PORT_ACCEPTOR_TLS`, `PORT_ACCEPTOR_TLS` at `15661`-`15663` beside it) for a QuickFIX/J acceptor.
`[reported 2026-09-23]` a parallel run of both scripts made one interop run fail with a bind
error that read as an interop regression rather than as two unrelated scripts reaching for the
same number.

**Guarded by:** `Referee.Args.parse` now refuses to start without `--port` (`--port is required`,
the same shape as the pre-existing `--aeron-dir is required` check) — there is no default port
left to collide with anything. `scripts/fixp-spike.sh`'s `FIXP_REFEREE_PORT` environment override
is documented, where it is read, as needing to avoid `15660`-`15663`; nothing enforces that at
run time beyond the comment, because the override exists precisely for a caller who has a reason
to pick a specific port and is trusted to pick one that is actually free.

**Reversal proving the fix, run and restored on this desk:**

```
$ java --add-opens java.base/sun.nio.ch=ALL-UNNAMED --add-opens java.base/jdk.internal.misc=ALL-UNNAMED \
    --add-opens java.base/java.util.zip=ALL-UNNAMED -cp vendor/fixp/classes:<jars> Referee \
    --aeron-dir /tmp/referee-no-port-test
Exception in thread "main" java.lang.IllegalArgumentException: --port is required
	at Referee$Args.parse(Referee.java:417)
	at Referee.main(Referee.java:79)
```

### 7. `sort` and `comm` need `LC_ALL=C` to agree on a `git status --porcelain` line

`[measured 2026-09-23, row 3, once the tree had untracked files]` Under `en_US.UTF-8`, `sort` and
`comm` disagreed about how to order porcelain lines that start with a space (`" M file"` beside
`"?? file"`), and `comm` reported `file 1 is not in sorted order` rather than comparing the two
snapshots reliably. `scripts/fixp-spike.sh` takes a `git status` snapshot before anything runs and
another after, and diffs them to prove the run added nothing `git` can see (ADR-0140 decision 3);
a locale-dependent collation order breaking that diff would either hide a real leak or invent one.

**Guarded by:** both the `sort` in the `BEFORE`/`AFTER` snapshot and the `comm -13` that diffs them
are run under `LC_ALL=C` explicitly (`scripts/fixp-spike.sh`, the snapshot lines and the comment
directly above the diff). The three runs quoted in `docs/CONFORMANCE.md` §10 each printed
`the run added nothing git can see`.

### 8. The two refusal arms are refused at different speeds, for different reasons

`reject-timestamp`'s `NegotiateReject INVALID_TIMESTAMP` comes from Artio's own library check
(`InternalBinaryEntryPointConnection.isInvalidTimestamp`), which runs **after** this repository's
authentication strategy has already accepted the connection (`referee: authentication accepted`
still prints). Artio does not tear the socket down at that point; it waits for its own
authentication timeout. `[measured 2026-09-23 on this desk]`:

```
referee: disconnected AUTHENTICATION_TIMEOUT
reject-timestamp: note: after the reject, EOF after 4999ms
```

`reject-credentials`, by contrast, is refused inside this repository's own authentication
strategy (`referee: authentication rejected`) before Artio's library ever accepts the connection,
and the socket closes promptly:

```
reject-credentials: note: after the reject, EOF after 500ms
```

**Guarded by:** nothing asserts either number. Both are printed by the probe as a `note:` line —
observational, the way `docs/DESIGN.md` §8 and CLAUDE.md non-negotiable 10 require a timing figure
to be treated when it is not a benchmark: read, not gated on. What the arms *do* assert is the
refusal's named code (`INVALID_TIMESTAMP(7)` / `CREDENTIALS(1)`) and, for `reject-timestamp`, that
all seven fields were accepted first. **A future FIXP session design has to account for the ~5
second gap on the timestamp path** — a malformed or hostile client that fails only the timestamp
check ties up a connection roughly ten times longer than one whose credentials are simply wrong —
and nothing here measures whether that gap is configurable.

### 9. A bare TCP connect-and-close reads as a malformed SOFH client, not as a readiness probe

An early version of the referee's "confirm the acceptor is listening" check opened a plain
loopback socket and closed it immediately, expecting a clean connect/disconnect to prove the bind
succeeded. Artio's framer instead read that bare connection as a corrupt Simple Open Framing
Header and logged `IllegalArgumentException: Unsupported Encoding Type` through the connection's
error handler — a real FIXP client never sends zero bytes before disconnecting, so nothing in
Artio expects that shape. See the class-level Javadoc on `Referee` in
`spikes/fixp-probe/referee/Referee.java` (the paragraph starting "The 'listening' line is
printed only after `FixEngine.launch` has returned") for the full account, including why the
fix — reading a normal return from `FixEngine.launch` as the observation, since `launch` binds
the `ServerSocketChannel` synchronously — needed no probe connection at all.

**Guarded by:** the doc comment alone. There is no regression test that reintroduces a bare
connect-and-close and asserts Artio logs that exact exception; the `referee-only` arm's own
`referee: listening on …` line comes from the synchronous return of `launch`, not from a socket
probe, so the class of trap is avoided by construction rather than caught if it recurs. A future
readiness check written the naive way would reintroduce this exact log line, unasserted.

## What this page does not cover

Session-layer questions — `Sequence`, `RetransmitRequest`, `NotApplied`,
`FinishedSending`/`FinishedReceiving`, reconnection — are out of scope for this spike
(ADR-0078 decision 2, ADR-0097 decision 5) and are not "facts" this page can state anything about
yet. They are the FIXP ADR's to take up.
