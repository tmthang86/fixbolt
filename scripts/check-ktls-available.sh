#!/usr/bin/env bash
# Can THIS machine answer STATUS.md open item 10?
#
# Item 10 asks whether `ktls-core` can be driven from a plain non-blocking socket
# with no async runtime — the load-bearing open question in ADR-0005, because if
# the answer is no then "TLS is a transport with the hot-path guarantee" collapses
# to "userspace rustls only".
#
# It was recorded for days as "cannot be checked here — needs the Linux box of
# open item 6". `[measured 2026-08-30]` that was the wrong blocker twice over.
# kTLS is a KERNEL FEATURE, not a latency property, so it needs no §9 machine at
# all. And a Linux box is not enough either: it needs a kernel BUILT WITH
# CONFIG_TLS, which the box this was first tried on is not.
#
# So this script exists to answer "can I even start?" in one command, rather than
# leaving the next person to rediscover ENOENT.
set -uo pipefail

echo "kernel: $(uname -sr)"

cfg="not readable"
if [[ -r "/boot/config-$(uname -r)" ]]; then
  cfg="$(grep -E '^#? *CONFIG_TLS[ =]' "/boot/config-$(uname -r)" | head -1)"
elif [[ -r /proc/config.gz ]]; then
  cfg="$(zcat /proc/config.gz | grep -E '^#? *CONFIG_TLS[ =]' | head -1)"
fi
echo "config: ${cfg:-CONFIG_TLS absent from the config}"
echo "tls_stat: $([[ -r /proc/net/tls_stat ]] && echo present || echo absent)"

# The config line is a claim; this is the observation. TCP_ULP is set on a REAL
# connected socket, because on an unconnected one it fails for a different reason
# and that would prove nothing.
python3 - <<'PY'
import errno, socket, sys
SOL_TCP, TCP_ULP = 6, 31
srv = socket.socket(); srv.bind(("127.0.0.1", 0)); srv.listen(1)
c = socket.create_connection(srv.getsockname()); s, _ = srv.accept()
try:
    c.setsockopt(SOL_TCP, TCP_ULP, b"tls")
    print("setsockopt(TCP_ULP, \"tls\"): ACCEPTED — this machine can answer item 10")
    rc = 0
except OSError as e:
    name = errno.errorcode.get(e.errno, e.errno)
    print(f'setsockopt(TCP_ULP, "tls"): REFUSED errno={e.errno} ({name}) {e.strerror}')
    print("  ENOENT here means the kernel has no `tls` ULP at all: it was built")
    print("  without CONFIG_TLS. Item 10 CANNOT be answered on this machine, and")
    print("  nothing about ADR-0005 should be concluded from that.")
    rc = 1
finally:
    c.close(); s.close(); srv.close()
sys.exit(rc)
PY
