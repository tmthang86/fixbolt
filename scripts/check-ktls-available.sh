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

# ktls_verdict <syscall> <config-line> <module-loaded> <module-on-disk>
#
# Prints a verdict token on the first line and the human explanation after it.
# Split out so scripts/check-ktls-classify.sh can exercise it on any machine:
# what was wrong here was never the syscall, it was the story told about the
# syscall's result.
ktls_verdict() {
  local syscall="$1" cfg="$2" loaded="$3" ondisk="$4"

  if [[ "$syscall" == ACCEPTED ]]; then
    echo "READY"
    echo "  This machine can answer STATUS.md open item 10."
    return 0
  fi

  # Anything that is not ENOENT says nothing whatever about CONFIG_TLS. Reporting
  # it as "not built" is how this script sent open item 10 to the wrong blocker.
  if [[ "$syscall" != ENOENT ]]; then
    echo "OTHER"
    echo "  This refusal is not ENOENT, so it is NOT a statement about CONFIG_TLS."
    echo "  Read the errno above before concluding anything about kTLS or ADR-0005."
    return 1
  fi

  # ENOENT from TCP_ULP means only "no ULP is registered under the name tls".
  # Registered is not the same as compiled: a module that exists on disk and has
  # not been loaded gives exactly this, because the kernel autoloads a ULP only
  # for a caller holding CAP_NET_ADMIN.
  if [[ "$loaded" == yes ]]; then
    echo "SURPRISE"
    echo "  The tls module reports as loaded and TCP_ULP still says ENOENT."
    echo "  Nothing in the config or the module state explains that. Do not"
    echo "  conclude anything about ADR-0005 from it — find out why first."
    return 1
  fi

  if [[ "$ondisk" == yes ]]; then
    echo "LOADABLE"
    echo "  The tls module is present on disk and simply not loaded. This machine"
    echo "  CAN answer item 10 — it is one command away:"
    echo
    echo "      sudo modprobe tls && scripts/check-ktls-available.sh"
    echo
    echo "  ENOENT alone does not mean unbuilt: the kernel autoloads a ULP only"
    echo "  for a caller with CAP_NET_ADMIN, so an unprivileged probe sees ENOENT"
    echo "  even when the module is right there."
    return 2
  fi

  if [[ "$cfg" == *CONFIG_TLS=y* ]]; then
    echo "SURPRISE"
    echo "  The config says CONFIG_TLS=y — built in, nothing to load — and TCP_ULP"
    echo "  still says ENOENT. Nothing here explains that. Find out why before"
    echo "  concluding anything about ADR-0005."
    return 1
  fi

  echo "NOT_BUILT"
  echo "  No tls ULP, no module on disk, and the config does not claim one. This"
  echo "  kernel was built without CONFIG_TLS. Item 10 CANNOT be answered on this"
  echo "  machine, and nothing about ADR-0005 should be concluded from that."
  return 1
}

# Sourced by the classification test, which wants the functions and none of the
# probing below.
if [[ "${KTLS_SOURCE_ONLY:-0}" == 1 ]]; then
  return 0 2>/dev/null || exit 0
fi

echo "kernel: $(uname -sr)"

cfg="not readable"
if [[ -r "/boot/config-$(uname -r)" ]]; then
  cfg="$(grep -E '^#? *CONFIG_TLS[ =]' "/boot/config-$(uname -r)" | head -1)"
elif [[ -r /proc/config.gz ]]; then
  cfg="$(zcat /proc/config.gz | grep -E '^#? *CONFIG_TLS[ =]' | head -1)"
fi
echo "config: ${cfg:-CONFIG_TLS absent from the config}"
echo "tls_stat: $([[ -r /proc/net/tls_stat ]] && echo present || echo absent)"

# Both are inputs to the verdict, and the distinction between them is the whole
# point: loaded is what the kernel is running, on disk is what it could run.
#
# `loaded` asks /sys rather than `lsmod | grep -q`, and not for tidiness:
# `[measured 2026-08-30]` under this script's own `set -o pipefail`, grep -q
# exits at the first match, lsmod dies of SIGPIPE with status 141, and the
# pipeline reports FAILURE on the very run where the module was found. The first
# version of this line printed `loaded=no` on a machine with tls loaded — the
# same class of wrong answer this script is being fixed for.
loaded=no; [[ -d /sys/module/tls ]] && loaded=yes
ondisk=no; modinfo tls >/dev/null 2>&1 && ondisk=yes
echo "module: loaded=$loaded on_disk=$ondisk"

# The config line is a claim; this is the observation. TCP_ULP is set on a REAL
# connected socket, because on an unconnected one it fails for a different reason
# and that would prove nothing. The syscall's own verdict is deliberately NOT
# decided here — this block reports what happened and nothing else.
syscall="$(python3 - <<'PY'
import errno, socket, sys
SOL_TCP, TCP_ULP = 6, 31
srv = socket.socket(); srv.bind(("127.0.0.1", 0)); srv.listen(1)
c = socket.create_connection(srv.getsockname()); s, _ = srv.accept()
try:
    c.setsockopt(SOL_TCP, TCP_ULP, b"tls")
    sys.stderr.write('setsockopt(TCP_ULP, "tls"): ACCEPTED\n')
    print("ACCEPTED")
except OSError as e:
    name = errno.errorcode.get(e.errno, str(e.errno))
    sys.stderr.write(
        f'setsockopt(TCP_ULP, "tls"): REFUSED errno={e.errno} ({name}) {e.strerror}\n'
    )
    print(name)
finally:
    c.close(); s.close(); srv.close()
PY
)"

ktls_verdict "$syscall" "$cfg" "$loaded" "$ondisk"
rc=$?

# 0 = ready now, 2 = one modprobe away, 1 = cannot, or cannot be explained.
exit "$rc"
