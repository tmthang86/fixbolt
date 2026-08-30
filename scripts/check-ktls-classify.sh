#!/usr/bin/env bash
# Does check-ktls-available.sh classify a refusal correctly?
#
# `[measured 2026-08-30]` It did not, and the failure was silent in the way that
# matters: on the owner's Linux desktop the script printed `config: CONFIG_TLS=m`
# and then, four lines later, "the kernel has no `tls` ULP at all: it was built
# without CONFIG_TLS". Both lines came out of the same run. The machine could in
# fact run kTLS after one `modprobe`, and STATUS.md open item 10 sat blocked on a
# sentence the script had contradicted in its own output.
#
# The cause is that the old script printed one fixed ENOENT paragraph for EVERY
# OSError and never consulted the config line it had already read. So the guard
# here is not "does the syscall work" — no test can promise a kernel — it is
# "given a syscall result and a config, does the script reach the right verdict".
# That is pure logic, it runs on any machine, and it is what was wrong.
#
# Referenced by docs/reference/ktls-on-a-plain-socket.md, which stated the correct
# rule ("CONFIG_TLS=y or =m with the module loadable") a day before the script
# failed to implement it.
set -uo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=check-ktls-available.sh
KTLS_SOURCE_ONLY=1 . "$here/check-ktls-available.sh"

pass=0
fail=0

# expect <want-token> <syscall> <config-line> <module-loaded> <module-on-disk> <what it means>
expect() {
  local want="$1" syscall="$2" cfg="$3" loaded="$4" ondisk="$5" what="$6"
  local got
  got="$(ktls_verdict "$syscall" "$cfg" "$loaded" "$ondisk" | head -1)"
  if [[ "$got" == "$want" ]]; then
    pass=$((pass + 1))
    printf 'ok    %-12s %s\n' "$want" "$what"
  else
    fail=$((fail + 1))
    printf 'FAIL  want %-12s got %-12s %s\n' "$want" "$got" "$what"
  fi
}

echo "=== ktls_verdict"

# The case that was wrong on the owner's desktop, 2026-08-30. The module is on
# disk and simply not loaded; the kernel refuses TCP_ULP with ENOENT because
# autoloading a ULP needs CAP_NET_ADMIN, which an unprivileged process lacks.
expect LOADABLE   ENOENT "CONFIG_TLS=m" no  yes \
  "=m, module on disk, not loaded — one modprobe away, NOT unbuilt"

# The case the container was in, and the only one the old script got right.
expect NOT_BUILT  ENOENT "# CONFIG_TLS is not set" no no \
  "config says not set, no module — genuinely cannot answer item 10"

expect NOT_BUILT  ENOENT "" no no \
  "CONFIG_TLS absent from the config entirely, no module"

# Built in, no module file to find, and the ULP still missing. Nothing in the
# inputs explains that, so the script must say so rather than pick a story.
expect SURPRISE   ENOENT "CONFIG_TLS=y" no  no \
  "=y yet no ULP — unexplained, must not be reported as unbuilt"

# Loaded and still ENOENT is equally unexplained.
expect SURPRISE   ENOENT "CONFIG_TLS=m" yes yes \
  "module loaded yet ENOENT — unexplained"

# A refusal that is not ENOENT says nothing about CONFIG_TLS at all. The old
# script printed the ENOENT paragraph for these too.
expect OTHER      EPERM  "CONFIG_TLS=m" yes yes \
  "EPERM — a policy refusal, not a missing feature"

expect OTHER      ENOPROTOOPT "CONFIG_TLS=y" yes yes \
  "ENOPROTOOPT — the kernel does not know TCP_ULP"

expect READY      ACCEPTED "CONFIG_TLS=m" yes yes \
  "the syscall was accepted"

echo
echo "=== summary"
echo "pass $pass   fail $fail"
[[ "$fail" -eq 0 ]]
