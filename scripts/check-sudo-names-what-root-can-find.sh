#!/usr/bin/env bash
# ADR-0093 decision 2: a `sudo` line in a committed script hands root a
# command by NAME, and root resolves that name against `secure_path`
# (/etc/sudoers), never the caller's PATH — `~/.cargo/bin` is not on it.
# `docs/reference/perf-record-exits-zero-when-sudo-cannot-find-the-workload.md`
# is the trap this gate exists for: `sudo -n perf record … -- cargo bench …`
# ran five times, wrote a header-only .data file and exited 0, because `perf`
# itself IS on secure_path (it passes) and the workload after `--` is what
# root could not find. A gate that reads only the word right after `sudo`
# passes exactly that line.
#
# THREE RULES, applied to every occurrence of the word `sudo` on a live
# logical line — in command position, after a pipe, or inside a quoted
# advisory string handed to `row()` alike. A string is not a false positive:
# it is the line a human pastes, so it is read by the same rule as an
# invocation (ADR-0093 fact 3).
#
#   R1 — the command word (the first token after sudo's own -n/-E/-u
#        <user>/--/VAR=value options) passes when it contains a `/`
#        (resolved by path, not PATH), starts with `$` (a variable — its
#        value is unread; gap G1, the intended fix pattern), or is on ALLOW
#        below. Anything else: FAIL R1.
#   R2 — any BARE token (no `/`) anywhere after `sudo` that is `cargo`,
#        `cargo-*`, `rustc`, `rustup`, `rustdoc` or `w2w`: FAIL R2. This is
#        the rule that reads boot D's own line — `perf` passes R1, `cargo`
#        after `--` is caught here regardless of where it sits.
#   R3 — when the command word is `perf` and a `--` token follows, the token
#        right after `--` is a command word too and must also pass R1 (this
#        is what catches an UNLISTED workload, e.g. `sudo perf record --
#        mytool`, that R2 has no name for).
#
# ALLOW is the pinned set of names this project has checked live under the
# default Debian/Ubuntu secure_path (a gate that must run on a CI runner with
# no root and no `perf` cannot resolve names against a live PATH — ADR-0093
# fact, "sudo -V" as root). A name is added only with `dpkg -S "$(command -v
# X)"` evidence in a commit, recorded here:
#   apt, apt-get, bash — package management / shell, /usr/bin
#   cat, dmesg, env, kill, nice, sh, tee — coreutils/util-linux, /usr/bin or /bin
#   chrt, taskset — util-linux, /usr/bin
#   cpupower — linux-tools, /usr/bin
#   ethtool, ip — iproute2/ethtool, /usr/sbin
#   journalctl, systemctl — systemd, /usr/bin or /bin
#   modprobe, sysctl — kmod/procps, /usr/sbin or /sbin
#   nft — nftables, /usr/sbin
#   perf — linux-tools, /usr/bin
#   pkill — procps, /usr/bin
#   setcap — libcap2-bin, /usr/sbin
#   update-grub — grub2-common, /usr/sbin
# Over-matching (a real tool not yet on ALLOW reads FAIL) is the safe
# direction, on purpose — ADR-0061's precedent, generalised here.
#
# KNOWN GAPS, stated rather than papered over (ADR-0093 decision 2):
#   G1 — `sudo -n "$BIN"` passes R1 unread: the value of a variable is not
#        visible to a line-level gate, and this is the intended fix shape
#        (a manifest-pinned absolute path in the variable) — decision 1's
#        driver rule, and the script's own sha256 re-check, carry that
#        weight, not this gate.
#   G2 — inside `sh -c '…'`, `bash -c '…'`, `env …`, `nice …` only R2 is
#        applied to the quoted body, not R1: this gate reads tokens, not a
#        shell inside a shell.
#   G3 — prose (a plan cell, a comment) is not a script this gate scans;
#        decision 1 is what moves a driver into a file this gate reads.
#   G4 — ALLOW says nothing about whether the package is actually installed
#        on the machine that runs the line; that failure is loud (`command
#        not found`, exit 127) and is not the trap this gate guards.
#
# Sourced by scripts/check-sudo-verdicts.sh with SUDO_GATE_SOURCE_ONLY=1 for
# the pure verdict functions and none of the scanning — the same shape
# check-machine.sh (MACHINE_SOURCE_ONLY) and ab-rotation.sh
# (AB_ROTATION_SOURCE_ONLY) already use.
#
# SCOPE: with no arguments, every file directly in scripts/ that is named
# *.sh or whose first line is a `#!` naming an interpreter ending in `sh` —
# copied verbatim from check-scratch-fixtures.sh's own SCOPE comment and
# CANDIDATES_IN_SCRIPTS loop, same reasoning: a glob on *.sh alone misses a
# script that forgot the extension but kept the shebang, and an extension
# list is a thing somebody must remember to grow (STATUS.md items 62, 68).
#
# NO FILE IS EXCLUDED FROM THAT DEFAULT SCOPE (ADR-0093 decision 2, gap G5,
# closed). check-sudo-verdicts.sh used to be named out of scope by hand —
# the same way check-scratch-fixtures.sh names check-links.py out of ITS own
# scope — because its fixtures are deliberately byte-identical to a real bad
# line (boot D's own line among them), passed as a quoted STRING ARGUMENT to
# sudo_verdict(), never executed. `[measured 2026-09-22]` this gate's own
# first run against scripts/ found 9 "findings" there, every one of them a
# test fixture reading itself back. The consequence was never written down:
# a REAL bad sudo line added to check-sudo-verdicts.sh was never caught by
# the default scan, because nothing structural tells a deliberately-bad
# fixture string apart from a real advisory one — both are a quoted literal
# handed to a function (row() for the six real ones; sudo_verdict() here).
#
# The fix: a per-line trailing marker, `# check-sudo: fixture` — see
# is_marked_fixture() below — HONOURED ONLY when the file being scanned is
# check-sudo-verdicts.sh (matched by basename, so it holds however the path
# argument is spelled). The same marker text sitting in any other file does
# nothing at all: is_marked_fixture() is pure and answers only "does this
# text end in the marker", and the scanning loop below is the sole place
# that decides whether to ask it, keyed on the file's own name — so this is
# not a general escape hatch. Every fixture line in check-sudo-verdicts.sh
# that would otherwise read as a finding — a real bad line quoted as data,
# or plain English prose that happens to contain the word `sudo` followed by
# an unlisted word, since R1 cannot tell a comment from a command — carries
# the marker; an UNMARKED bad line added to that same file is caught exactly
# as it would be in any other file (the reversal in check-sudo-verdicts.sh's
# own commit history is what proves this). The count of lines the marker
# skipped is printed on the gate's own `ok` line, so it is visible in review
# instead of hidden inside a silent exclusion.
#
# An explicit path argument always reaches whatever file it names (nothing
# here can hide a file from a caller who names it on purpose). With one or
# more path arguments, exactly those files are scanned instead — used by the
# reversal fixtures under target/check-sudo/, which sit outside scripts/ on
# purpose (evidence, not a driver — ADR-0093 decision 1).
#
# A live logical line joins a trailing single backslash onto the next
# physical line first, so a workload hidden on the continuation is still
# read (plan trap table, "dòng nối \ giấu workload sang dòng sau"). A line
# whose first non-whitespace character is `#` is a comment and is skipped —
# is_live(), same rule check-scratch-fixtures.sh already uses.
#
# Zero scripts scanned is a FAIL, same reasoning as check-scratch-fixtures.sh
# B4: a count of nothing is a broken invocation, not a clean tree.
set -uo pipefail

ALLOW=(apt apt-get bash cat chrt cpupower dmesg env ethtool ip journalctl
       kill modprobe nft nice perf pkill setcap sh sysctl systemctl taskset
       tee update-grub)

# The word itself, assembled rather than spelled out here. `[measured
# 2026-09-22]` every OTHER live spot below that spelled it out directly — the
# regex literal and the two FAIL messages — was a line THIS GATE itself read
# as an occurrence of the word, on its own first run against scripts/: not a
# real invocation, the gate naming itself. A comment is exempt (is_live()
# skips it); a format string and a regex literal are not. Concatenating two
# quoted pieces keeps the SOURCE line from spelling the word contiguously
# while the built value is the word itself, used everywhere below via `%s`
# or variable interpolation instead of a literal.
SUDO_WORD="sud""o"

# A "live" line: its first non-whitespace character is not `#`.
is_live() {
  [[ ! "$1" =~ ^[[:space:]]*# ]]
}

# join_logical_lines <file> — emits one "<start-lineno>\t<0|1 live>\t<text>"
# per logical line, joining a trailing single backslash onto the next
# physical line (the backslash itself is dropped, a space takes its place so
# two tokens either side of the join never fuse).
join_logical_lines() {
  local file="$1" line lineno=0 buf="" buf_start=0 buf_live=1 pending=0
  while IFS= read -r line || [[ -n "$line" ]]; do
    lineno=$((lineno + 1))
    if [[ "$pending" -eq 0 ]]; then
      buf_start=$lineno
      if is_live "$line"; then buf_live=1; else buf_live=0; fi
      buf=""
      pending=1
    fi
    if [[ "$line" == *\\ ]]; then
      buf+="${line%\\} "
      continue
    fi
    buf+="$line"
    printf '%d\t%d\t%s\n' "$buf_start" "$buf_live" "$buf"
    pending=0
  done < "$file"
  if [[ "$pending" -eq 1 ]]; then
    printf '%d\t%d\t%s\n' "$buf_start" "$buf_live" "$buf"
  fi
}

# sudo_rests <line> — for every word-bounded occurrence of `sudo` in <line>,
# print the text immediately following it (one per output line), searching
# forward from each match so a second `sudo` on the same line (the
# irqbalance advisory: "sudo systemctl stop … && sudo systemctl disable …")
# is read as its own occurrence, not folded into the first.
#
# The boundary class excludes `-` as well as [A-Za-z0-9_]. `[measured
# 2026-09-22]` a plain `\b`-shaped boundary (alnum/underscore only) reads
# THIS SCRIPT'S OWN `"check-sudo: FAIL — …"` strings as an occurrence of the
# word `sudo` — the hyphen in `check-sudo` satisfies a boundary that stops at
# alnum/underscore, and this repository spells identifiers with hyphens
# throughout (`check-sudo`, `ab-rotation`, `no-kernel-sleep`). Widening the
# excluded class to `[^A-Za-z0-9_-]` reads `check-sudo` as one token and
# stops the false hit, without narrowing what a REAL `sudo X` still matches:
# every fixture below still sits after whitespace, a quote or shell syntax,
# never a hyphen.
sudo_rests() {
  local s="$1" pat="(^|[^A-Za-z0-9_-])${SUDO_WORD}([^A-Za-z0-9_-]|\$)(.*)\$"
  while [[ "$s" =~ $pat ]]; do
    printf '%s\n' "${BASH_REMATCH[3]}"
    s="${BASH_REMATCH[3]}"
  done
}

# sudo_cmdword <rest> — the command word: the first token after skipping
# sudo's own leading -n / -E / -u <user> / -- / VAR=value options.
sudo_cmdword() {
  local rest="$1" t
  local -a toks
  read -ra toks <<< "$rest"
  local i=0 n=${#toks[@]}
  while (( i < n )); do
    t="${toks[$i]}"
    case "$t" in
      --) i=$((i + 1)); break ;;
      -u|--user) i=$((i + 2)); continue ;;
      -*) i=$((i + 1)); continue ;;
      *)
        if [[ "$t" =~ ^[A-Za-z_][A-Za-z0-9_]*=.*$ ]]; then
          i=$((i + 1)); continue
        fi
        break
        ;;
    esac
  done
  printf '%s' "${toks[$i]:-}"
}

# perf_workload_after_dashdash <rest> — R3: the token right after a `--` on
# the same logical line, or empty when there is none.
perf_workload_after_dashdash() {
  local rest="$1" i
  local -a toks
  read -ra toks <<< "$rest"
  for (( i = 0; i < ${#toks[@]}; i++ )); do
    if [[ "${toks[$i]}" == "--" ]]; then
      printf '%s' "${toks[$((i + 1))]:-}"
      return
    fi
  done
}

# r1_pass <word> — R1's test on a single command word.
r1_pass() {
  local w="$1" stripped a
  [[ -z "$w" ]] && return 1
  [[ "$w" == *"/"* ]] && return 0
  stripped="${w#\"}"
  stripped="${stripped#\'}"
  [[ "$stripped" == '$'* ]] && return 0
  for a in "${ALLOW[@]}"; do
    [[ "$w" == "$a" ]] && return 0
  done
  return 1
}

# r2_hit_word <rest> — R2: the first bare toolchain word anywhere in <rest>,
# or nothing (and a failing exit) when there is none.
r2_hit_word() {
  local rest="$1" t
  local -a toks
  read -ra toks <<< "$rest"
  for t in "${toks[@]}"; do
    [[ "$t" == *"/"* ]] && continue
    case "$t" in
      cargo|cargo-*|rustc|rustup|rustdoc|w2w)
        printf '%s' "$t"
        return 0
        ;;
    esac
  done
  return 1
}

# sudo_occurrence_verdict <rest> — one occurrence's verdict: "ok",
# "FAIL R1" (no word — the reporting layer recomputes it when it needs one,
# via sudo_r1_offender) or "FAIL R2 <word>" (the word IS the finding).
sudo_occurrence_verdict() {
  local rest="$1" word cmdword workload
  word="$(r2_hit_word "$rest")"
  if [[ -n "$word" ]]; then
    printf 'FAIL R2 %s' "$word"
    return
  fi
  cmdword="$(sudo_cmdword "$rest")"
  if ! r1_pass "$cmdword"; then
    printf 'FAIL R1'
    return
  fi
  if [[ "$cmdword" == perf ]]; then
    workload="$(perf_workload_after_dashdash "$rest")"
    if [[ -n "$workload" ]] && ! r1_pass "$workload"; then
      printf 'FAIL R1'
      return
    fi
  fi
  printf 'ok'
}

# sudo_r1_offender <rest> — assumes sudo_occurrence_verdict returned
# "FAIL R1" for this <rest>; recomputes which word it was, for the message.
sudo_r1_offender() {
  local rest="$1" cmdword
  cmdword="$(sudo_cmdword "$rest")"
  if ! r1_pass "$cmdword"; then
    printf '%s' "$cmdword"
    return
  fi
  perf_workload_after_dashdash "$rest"
}

# sudo_verdict <line> — PURE, the function scripts/check-sudo-verdicts.sh
# tests directly: the first non-ok occurrence verdict on <line>, else "ok".
sudo_verdict() {
  local line="$1" rest v
  while IFS= read -r rest; do
    v="$(sudo_occurrence_verdict "$rest")"
    if [[ "$v" != ok ]]; then
      printf '%s' "$v"
      return
    fi
  done < <(sudo_rests "$line")
  printf 'ok'
}

# sudo_offending_rest <line> — the REST text of the first non-ok occurrence
# on <line>, for the reporting layer to recover the offending word from.
sudo_offending_rest() {
  local line="$1" rest
  while IFS= read -r rest; do
    if [[ "$(sudo_occurrence_verdict "$rest")" != ok ]]; then
      printf '%s' "$rest"
      return
    fi
  done < <(sudo_rests "$line")
}

# The per-line fixture marker (ADR-0093 gap G5). A plain string, not
# assembled like SUDO_WORD: it does not spell the word `sudo` contiguously
# (it is inside the hyphenated token `check-sudo`, which sudo_rests' own
# boundary class already reads as one token, not a match — see sudo_rests
# below), so this literal does not trip the gate reading itself.
FIXTURE_MARKER_PATTERN='(^|[[:space:]])# check-sudo: fixture[[:space:]]*$'

# is_marked_fixture <text> — prints "skip" when <text> ends with the marker
# (optional leading whitespace before the `#`, optional trailing
# whitespace), prints nothing otherwise. PURE: whether this is consulted at
# all, and for which file, is entirely the scanning loop's choice below —
# this function does not know or care what file its argument came from.
is_marked_fixture() {
  if [[ "$1" =~ $FIXTURE_MARKER_PATTERN ]]; then
    printf 'skip'
  fi
}

# Sourced by the verdict test, which wants the pure functions and none of
# the scanning below — the same shape check-machine.sh (MACHINE_SOURCE_ONLY)
# and ab-rotation.sh (AB_ROTATION_SOURCE_ONLY) already use, `return` at top
# level failing (not inside a function) when this file is executed rather
# than sourced, caught by `|| exit 0` instead.
# shellcheck disable=SC2317 # invoked indirectly: `return` here is reached
# only when this file is SOURCED (SUDO_GATE_SOURCE_ONLY=1), never when it is
# run as a script — shellcheck's static reachability check does not see that
# the branch runs under a different execution mode.
if [[ "${SUDO_GATE_SOURCE_ONLY:-0}" = 1 ]]; then
  return 0 2>/dev/null || exit 0
fi

declare -a FILES=()
if (( $# > 0 )); then
  FILES=("$@")
else
  ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
  cd "$ROOT" || exit 1
  shopt -s nullglob
  CANDIDATES=(scripts/*)
  shopt -u nullglob
  for cand in "${CANDIDATES[@]}"; do
    [[ -f "$cand" ]] || continue
    if [[ "$cand" == *.sh ]]; then
      FILES+=("$cand")
      continue
    fi
    shebang=""
    IFS= read -r shebang < "$cand" || true
    case "$shebang" in
      '#!'*sh | '#!'*sh[[:space:]]*) FILES+=("$cand") ;;
    esac
  done
fi

if [[ "${#FILES[@]}" -eq 0 ]]; then
  echo "check-sudo: FAIL — 0 scripts scanned" >&2
  exit 1
fi

findings=0
sudo_lines=0
marker_skipped=0

for f in "${FILES[@]}"; do
  while IFS=$'\t' read -r lineno live text; do
    [[ "$live" -eq 1 ]] || continue
    # The marker is honoured for exactly one file, by basename — see the
    # SCOPE comment above. Anywhere else this text does nothing.
    if [[ "$(basename -- "$f")" == "check-sudo-verdicts.sh" ]] \
      && [[ "$(is_marked_fixture "$text")" == skip ]]; then
      marker_skipped=$((marker_skipped + 1))
      continue
    fi
    mapfile -t occ < <(sudo_rests "$text")
    [[ "${#occ[@]}" -eq 0 ]] && continue
    sudo_lines=$((sudo_lines + 1))

    verdict="$(sudo_verdict "$text")"
    case "$verdict" in
      ok) ;;
      "FAIL R1")
        rest="$(sudo_offending_rest "$text")"
        word="$(sudo_r1_offender "$rest")"
        findings=$((findings + 1))
        printf "check-sudo: FAIL — %s:%s: %s runs '%s' by name, and this script cannot say root's secure_path finds it (R1) — add it to ALLOW with dpkg -S evidence, or give a path\n" \
          "$f" "$lineno" "$SUDO_WORD" "$word" >&2
        ;;
      "FAIL R2 "*)
        word="${verdict#FAIL R2 }"
        findings=$((findings + 1))
        case "$word" in
          cargo|cargo-*|rustc|rustup|rustdoc) hint="root's secure_path has no ~/.cargo/bin" ;;
          w2w) hint="root's secure_path has no build output directory for it" ;;
          *) hint="root's secure_path does not resolve it" ;;
        esac
        printf "check-sudo: FAIL — %s:%s: %s hands '%s' to root by name; %s (R2) — give an absolute path\n" \
          "$f" "$lineno" "$SUDO_WORD" "$word" "$hint" >&2
        ;;
    esac
  done < <(join_logical_lines "$f")
done

if [[ "$findings" -eq 0 ]]; then
  echo "check-sudo: ok — ${#FILES[@]} scripts scanned, ${sudo_lines} ${SUDO_WORD} lines read, ${marker_skipped} fixture lines skipped by marker, 0 findings"
  exit 0
fi
echo "check-sudo: ${findings} finding(s) — see the FAIL lines above" >&2
exit 1
