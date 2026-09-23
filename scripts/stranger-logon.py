#!/usr/bin/env python3
"""A FIX 4.4 counterparty that shares no code with fixbolt.

Plan row 8a (docs/plans/2026-09-23-p3-packaging-and-first-release.md): the
"stranger" that `scripts/stranger-check.sh` runs against a freshly built
`fixbolt` acceptor has to be an implementation that never imported anything
from this repository — otherwise a bug shared by both sides would stay
green. Python's standard library only: `socket`, `time`, `sys`. Nothing
here reads `fixbolt_codec` or any other crate, and nothing here is built by
`cargo`.

Talks to the acceptor's `TW44` counterparty (`crates/library/examples/acceptor.cfg`
and the config block `docs/GETTING-STARTED.md` pastes verbatim carry the same
`[SESSION] TargetCompID=TW44`, `SenderCompID=ISLD`): sends a `Logon` as
`49=TW44`, `56=ISLD`, `98=0`, `108=30`, waits for a `Logon` back, sends a
`Logout`, waits for a `Logout` back, then closes its side of the socket. The
acceptor process itself is stopped by `stranger-check.sh`, not by this
script — a peer `Logout` ends that one session, not the process listening
for more.

Usage: stranger-logon.py <host> <port>

Exit 0 and "LOGON OK" then "LOGOUT OK" on stdout when both round trips
answer within the timeout; exit 1 and a line starting "FAIL: " on stderr
otherwise. "no Logon answer within 5 s" and "no Logout answer within 5 s"
are the two sentences this script commits to before anything runs, so a
change to the timeout or the failure text is a change to the plan's written
reversal, not a free edit.
"""

import socket
import sys
import time

SOH = "\x01"
TIMEOUT_S = 5.0
SENDER = "TW44"  # this script, the "stranger"
TARGET = "ISLD"  # the acceptor, from acceptor.cfg's SenderCompID


def now_stamp() -> str:
    """`YYYYMMDD-HH:MM:SS`, 17 bytes, no fractional digits — the width
    `crates/library/tests/end_to_end.rs::now_stamp` uses for an inbound `52=`."""
    return time.strftime("%Y%m%d-%H:%M:%S", time.gmtime())


def frame(body: str) -> bytes:
    """`8=`, `9=` and `10=` around `body` — the same three lines every FIX
    sender writes, computed here rather than borrowed from anywhere."""
    prefix = f"8=FIX.4.4{SOH}9={len(body)}{SOH}"
    whole = (prefix + body).encode("ascii")
    checksum = sum(whole) % 256
    return whole + f"10={checksum:03d}{SOH}".encode("ascii")


def logon(seq: int, target: str = TARGET) -> bytes:
    return frame(
        f"35=A{SOH}34={seq}{SOH}49={SENDER}{SOH}52={now_stamp()}{SOH}"
        f"56={target}{SOH}98=0{SOH}108=30{SOH}"
    )


def logout(seq: int, target: str = TARGET) -> bytes:
    return frame(
        f"35=5{SOH}34={seq}{SOH}49={SENDER}{SOH}52={now_stamp()}{SOH}56={target}{SOH}"
    )


def read_one(sock: socket.socket) -> str:
    """One read, SOH rendered as `|` for a readable `in` check. Empty string
    means the peer closed the socket without answering."""
    sock.settimeout(TIMEOUT_S)
    try:
        data = sock.recv(8192)
    except socket.timeout:
        return ""
    except ConnectionResetError:
        return ""
    return data.decode("latin-1").replace(SOH, "|")


def fail(message: str) -> "None":
    print(f"FAIL: {message}", file=sys.stderr)
    sys.exit(1)


def main() -> None:
    if len(sys.argv) != 3:
        fail("usage: stranger-logon.py <host> <port>")
    host = sys.argv[1]
    port = int(sys.argv[2])

    with socket.create_connection((host, port), timeout=TIMEOUT_S) as sock:
        sock.setsockopt(socket.IPPROTO_TCP, socket.TCP_NODELAY, 1)

        sock.sendall(logon(1))
        reply = read_one(sock)
        if "|35=A|" not in reply:
            fail(f"no Logon answer within {TIMEOUT_S:g} s (got {reply!r})")
        print("LOGON OK")

        sock.sendall(logout(2))
        reply = read_one(sock)
        if "|35=5|" not in reply:
            fail(f"no Logout answer within {TIMEOUT_S:g} s (got {reply!r})")
        print("LOGOUT OK")


if __name__ == "__main__":
    main()
