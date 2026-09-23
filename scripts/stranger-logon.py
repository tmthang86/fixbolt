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

Each reply is checked three ways, not only for `35=`: its own `10=` checksum
is recomputed and compared (a wrong checksum this script's own substring
match on `35=` would never notice), and `56=` must read back `TW44` — this
script's own identity, echoed from the request's `49=` — proving the reply
was addressed to the stranger and not merely present on the wire.

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


def read_one(sock: socket.socket) -> bytes:
    """One read, raw bytes. Empty means the peer closed the socket without
    answering."""
    sock.settimeout(TIMEOUT_S)
    try:
        return sock.recv(8192)
    except socket.timeout:
        return b""
    except ConnectionResetError:
        return b""


def parse_fields(raw: bytes) -> dict:
    """`{tag: value}` over `SOH`-delimited `tag=value` pairs, first message
    only (this script never receives more than one at a time). The first
    occurrence of a repeated tag wins — good enough for the three tags this
    script reads (`35`, `56`, `10`); a `MessageView` this is not."""
    fields = {}
    for pair in raw.split(SOH.encode("ascii")):
        if b"=" not in pair:
            continue
        tag, _, value = pair.partition(b"=")
        tag_str = tag.decode("latin-1")
        if tag_str not in fields:
            fields[tag_str] = value.decode("latin-1")
    return fields


def checksum_ok(raw: bytes) -> bool:
    """Recomputes `10=` the same way `frame()` writes it, over the bytes
    that actually arrived — the one check a `35=` substring match can never
    make, because a corrupted body can still carry the right message type."""
    tenth = raw.rfind(b"10=")
    if tenth == -1 or tenth == 0:
        return False
    body = raw[:tenth]
    trailer = raw[tenth:].rstrip(SOH.encode("ascii"))
    try:
        claimed = int(trailer.split(b"=", 1)[1])
    except (IndexError, ValueError):
        return False
    return sum(body) % 256 == claimed


def fail(message: str) -> "None":
    print(f"FAIL: {message}", file=sys.stderr)
    sys.exit(1)


def expect(raw: bytes, msg_type: str, what: str) -> None:
    """All three checks together: an answer arrived, its `10=` is honest, its
    `35=` is what was asked for, and its `56=` names this script back — the
    reply was addressed to `TW44`, not merely present on the wire."""
    readable = raw.decode("latin-1").replace(SOH, "|")
    if not raw:
        fail(f"no {what} answer within {TIMEOUT_S:g} s (got {readable!r})")
    if not checksum_ok(raw):
        fail(f"the {what} answer's checksum does not match its own bytes (got {readable!r})")
    fields = parse_fields(raw)
    if fields.get("35") != msg_type:
        fail(f"no {what} answer within {TIMEOUT_S:g} s (got {readable!r})")
    if fields.get("56") != SENDER:
        fail(f"the {what} answer's 56= reads {fields.get('56')!r}, not {SENDER!r} (got {readable!r})")


def main() -> None:
    if len(sys.argv) != 3:
        fail("usage: stranger-logon.py <host> <port>")
    host = sys.argv[1]
    port = int(sys.argv[2])

    with socket.create_connection((host, port), timeout=TIMEOUT_S) as sock:
        sock.setsockopt(socket.IPPROTO_TCP, socket.TCP_NODELAY, 1)

        sock.sendall(logon(1))
        expect(read_one(sock), "A", "Logon")
        print("LOGON OK")

        sock.sendall(logout(2))
        expect(read_one(sock), "5", "Logout")
        print("LOGOUT OK")


if __name__ == "__main__":
    main()
