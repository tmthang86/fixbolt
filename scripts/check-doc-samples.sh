#!/usr/bin/env bash
# ADR-0206 decision 7: "Every Rust code block in a tutorial or how-to is
# either a verbatim copy of a file that CI compiles ... or is fenced `text`
# with the reason written beside it." This script is the machine check for
# the first half of that sentence: a `<!-- sample: <path> -->` marker claims
# the fenced code block right after it is a verbatim, byte-for-byte copy of
# a real file in this repository (or of a named region inside one), and this
# script is what makes that claim provable rather than a promise a page can
# quietly stop keeping.
#
# Marker forms, one per line, matched anywhere in docs/**/*.md, README.md,
# ARCHITECTURE.md and CONTRIBUTING.md (the last three only when they exist —
# ARCHITECTURE.md and CONTRIBUTING.md do not exist yet as of this script):
#
#   <!-- sample: crates/dict/examples/venue/src/main.rs -->
#   <!-- sample: crates/dict/examples/venue/src/main.rs#setup -->
#
# The path is repository-relative. The optional `#region` form compares only
# the lines strictly between a `// region:setup` line and the matching
# `// endregion:setup` line in the named file (both marker lines excluded);
# a source file that wants a region names it that way itself. This is the
# one comment style supported — `//`, not `#` or `/*`  — kept simple on
# purpose (the brief this script was written against: "only add regions if
# simple").
#
# The fenced block must be the very next non-blank line after the marker:
# a marker followed by prose, another marker, or nothing before EOF is red
# ("marker with no block"), not silently skipped. The marker must start at
# column 0: an indented marker (under a list item, say) is red, with a
# sentence saying so, rather than supported — comparing an indented block
# exactly would mean deciding how much indentation each block line loses,
# and a wrong guess there is a silent green. A marker-shaped line inside
# prose (text before or after it on the same line) is not a marker. Only plain ``` fences are
# recognised (this repository does not use ~~~ fences or 4-backtick fences
# today); a fence opened but never closed before EOF is red too.
#
# WHAT IT CANNOT SEE:
#   - a source file whose very last line has no trailing newline: the
#     reconstructed comparison always ends one, so such a file reads as
#     mismatched even if a human would call it identical. Every source file
#     this script has been pointed at so far ends with one (rustfmt enforces
#     it for .rs), so this has not bitten yet.
#   - whether the SURROUNDING PROSE still describes the code accurately —
#     only that the fenced bytes equal the file's (or region's) bytes right
#     now. A file and a doc page can both be right and still describe two
#     different points in time if a human edits the prose without the code
#     changing, or vice versa.
#   - a marker whose path exists but is a directory, a symlink to nowhere,
#     or otherwise unreadable: treated the same as a missing file.
#   - `#region` names that collide with a REGULAR-EXPRESSION metacharacter
#     in a way that matches an unrelated `// region:` line — the name is
#     matched literally, not as a substring, so this is not expected to
#     happen in practice, but a name is not sanitised beyond that.
#
# Runs standalone: scripts/check-doc-samples.sh
# Reversal: append a scratch marker to a throwaway file under docs/,
# pointing at a real tracked file with one byte changed relative to what is
# fenced after it — expect red naming the file, the marker's line, and the
# first differing line; then restore (remove the scratch file) — expect
# green. See the plan step this script was written for.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${ROOT}" || exit 2

# --- Which markdown files to scan. ------------------------------------------
FILES=()
if [[ -d docs ]]; then
  while IFS= read -r f; do
    FILES+=("${f}")
  done < <(find docs -type f -name '*.md' | sort)
fi
for extra in README.md ARCHITECTURE.md CONTRIBUTING.md; do
  if [[ -f "${extra}" ]]; then
    FILES+=("${extra}")
  fi
done

checked=0
fail=0

MARKER_RE='^<!--[[:space:]]*sample:[[:space:]]*([^[:space:]]+)[[:space:]]*-->[[:space:]]*$'
INDENTED_MARKER_RE='^[[:space:]]+<!--[[:space:]]*sample:[[:space:]]*([^[:space:]]+)[[:space:]]*-->[[:space:]]*$'
FENCE_OPEN_RE='^```'
FENCE_CLOSE_RE='^```[[:space:]]*$'

for md in "${FILES[@]}"; do
  # Load the whole file into an array, line by line, without mapfile — this
  # must run on macOS's bundled bash 3.2, which has no mapfile/readarray.
  lines=()
  while IFS= read -r line || [[ -n "${line}" ]]; do
    lines+=("${line}")
  done < "${md}"
  n=${#lines[@]}

  i=0
  while [[ ${i} -lt ${n} ]]; do
    this_line="${lines[${i}]}"
    if [[ "${this_line}" =~ ${INDENTED_MARKER_RE} ]]; then
      echo "check-doc-samples: FAIL — ${md}:$((i + 1)): marker '<!-- sample: ${BASH_REMATCH[1]} -->' is indented; indented markers are not supported — put the marker at column 0" >&2
      fail=1
      i=$((i + 1))
      continue
    fi
    if [[ "${this_line}" =~ ${MARKER_RE} ]]; then
      marker_ref="${BASH_REMATCH[1]}"
      marker_lineno=$((i + 1))

      path="${marker_ref%%#*}"
      region=""
      if [[ "${marker_ref}" == *"#"* ]]; then
        region="${marker_ref#*#}"
      fi

      # Find the next non-blank line — it must open a fence.
      j=$((i + 1))
      while [[ ${j} -lt ${n} && -z "${lines[${j}]}" ]]; do
        j=$((j + 1))
      done

      if [[ ${j} -ge ${n} || ! "${lines[${j}]}" =~ ${FENCE_OPEN_RE} ]]; then
        echo "check-doc-samples: FAIL — ${md}:${marker_lineno}: marker '<!-- sample: ${marker_ref} -->' has no fenced code block right after it" >&2
        fail=1
        i=$((i + 1))
        continue
      fi

      # Find the closing fence.
      k=$((j + 1))
      while [[ ${k} -lt ${n} && ! "${lines[${k}]}" =~ ${FENCE_CLOSE_RE} ]]; do
        k=$((k + 1))
      done

      if [[ ${k} -ge ${n} ]]; then
        echo "check-doc-samples: FAIL — ${md}:${marker_lineno}: fenced code block opened at line $((j + 1)) is never closed" >&2
        fail=1
        i=$((k + 1))
        continue
      fi

      checked=$((checked + 1))

      block_tmp="$(mktemp)"
      : > "${block_tmp}"
      bi=$((j + 1))
      while [[ ${bi} -lt ${k} ]]; do
        printf '%s\n' "${lines[${bi}]}" >> "${block_tmp}"
        bi=$((bi + 1))
      done

      if [[ ! -f "${path}" ]]; then
        echo "check-doc-samples: FAIL — ${md}:${marker_lineno}: marker names ${path}, which does not exist" >&2
        fail=1
        rm -f "${block_tmp}"
        i=$((k + 1))
        continue
      fi

      if [[ -z "${region}" ]]; then
        want_tmp="${path}"
        cmp_target="${path}"
      else
        want_tmp="$(mktemp)"
        cmp_target="${path}#${region}"
        start_re="^[[:space:]]*// region:${region}[[:space:]]*\$"
        end_re="^[[:space:]]*// endregion:${region}[[:space:]]*\$"

        src_lines=()
        while IFS= read -r sline || [[ -n "${sline}" ]]; do
          src_lines+=("${sline}")
        done < "${path}"
        sn=${#src_lines[@]}

        si=0
        start_idx=-1
        end_idx=-1
        while [[ ${si} -lt ${sn} ]]; do
          if [[ ${start_idx} -eq -1 && "${src_lines[${si}]}" =~ ${start_re} ]]; then
            start_idx=${si}
          elif [[ ${start_idx} -ne -1 && ${end_idx} -eq -1 && "${src_lines[${si}]}" =~ ${end_re} ]]; then
            end_idx=${si}
          fi
          si=$((si + 1))
        done

        if [[ ${start_idx} -eq -1 || ${end_idx} -eq -1 ]]; then
          echo "check-doc-samples: FAIL — ${md}:${marker_lineno}: region '${region}' not found in ${path} (expected // region:${region} ... // endregion:${region})" >&2
          fail=1
          rm -f "${block_tmp}" "${want_tmp}"
          i=$((k + 1))
          continue
        fi

        wi=$((start_idx + 1))
        while [[ ${wi} -lt ${end_idx} ]]; do
          printf '%s\n' "${src_lines[${wi}]}" >> "${want_tmp}"
          wi=$((wi + 1))
        done
      fi

      if ! cmp -s "${block_tmp}" "${want_tmp}"; then
        first_diff="$(diff "${want_tmp}" "${block_tmp}" 2>/dev/null | grep -m1 -E '^[<>]' || true)"
        echo "check-doc-samples: FAIL — ${md}:${marker_lineno}: fenced block does not match ${cmp_target} byte-for-byte; first differing line: ${first_diff}" >&2
        fail=1
      fi

      rm -f "${block_tmp}"
      if [[ -n "${region}" ]]; then
        rm -f "${want_tmp}"
      fi

      i=$((k + 1))
      continue
    fi
    i=$((i + 1))
  done
done

if [[ ${fail} -ne 0 ]]; then
  echo "check-doc-samples: FAIL" >&2
  exit 1
fi
echo "check-doc-samples: ok — ${checked} sample marker(s) checked"
