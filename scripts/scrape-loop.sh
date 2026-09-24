#!/usr/bin/env bash
# Scrape a fixbolt-metrics exporter at a fixed rate for a fixed time, and say
# what came back. Phase 4 row 1 step 5 (docs/plans/2026-09-24-p4-metrics-exporter.md):
# the load the three mode scripts run under with `W2W_EXTRA="--metrics <addr>"`,
# and row 2's scrape-on arm on the §9 desk.
#
#   scripts/scrape-loop.sh <addr> <hz> <seconds>
#   scripts/scrape-loop.sh 127.0.0.1:19464 10 60 &
#
# WHAT IT COUNTS: every attempt is one of
#   ok          HTTP 200 from /metrics
#   bad         an HTTP answer that is not 200 — the exporter answered, wrongly
#   unanswered  no HTTP answer at all: nothing listening (a mode script between
#               two w2w runs), or no reply within the timeout
#
# EXIT: 0 when at least one scrape was `ok` and none was `bad`; 1 when any was
# `bad`, or none was `ok` — a loop that never reached an exporter scraped
# nothing and must not read as load; 2 on a usage error.
#
# WHAT IT CANNOT SEE: whether the body is right (that is
# `crates/metrics/tests/`), or what a scrape cost the engine (that is row 2's
# `w2w` pair).
#
# PACING: one attempt is STARTED every 1/<hz> seconds and not waited for, so a
# slow answer does not lower the rate. `[measured 2026-09-24]` the first
# version waited for each answer and then slept; against the exporter's
# default 100 ms `tick` — a scrape waits up to one tick to be accepted — it
# reached 4.2 Hz when asked for 10. The summary prints the rate that was sent.
set -uo pipefail

if [[ $# -ne 3 ]]; then
  echo "usage: $0 <addr> <hz> <seconds>" >&2
  exit 2
fi
addr="$1" hz="$2" seconds="$3"
[[ "${hz}" =~ ^[1-9][0-9]*$ && "${seconds}" =~ ^[1-9][0-9]*$ ]] || {
  echo "scrape-loop: <hz> and <seconds> are positive integers" >&2
  exit 2
}
command -v curl >/dev/null || { echo "scrape-loop: curl is not installed" >&2; exit 2; }

period="$(awk -v h="${hz}" 'BEGIN { printf "%.3f", 1 / h }')"
TMP="$(mktemp -d)"
trap 'rm -rf "${TMP}"' EXIT
attempts=$((hz * seconds))
start_ns="$(date +%s%N)"
for ((i = 0; i < attempts; i++)); do
  curl -s -o /dev/null -w '%{http_code}' --max-time 2 "http://${addr}/metrics" >"${TMP}/${i}" 2>/dev/null &
  sleep "${period}"
done
sent_ns="$(date +%s%N)"
wait
ok=0 bad=0 unanswered=0
for ((i = 0; i < attempts; i++)); do
  code="$(cat "${TMP}/${i}" 2>/dev/null || true)"
  case "${code}" in
    200) ok=$((ok + 1)) ;;
    000 | "") unanswered=$((unanswered + 1)) ;;
    *) bad=$((bad + 1)) ;;
  esac
done
took="$(awk -v a="${start_ns}" -v b="${sent_ns}" 'BEGIN { printf "%.1f", (b - a) / 1e9 }')"
rate="$(awk -v n="${attempts}" -v a="${start_ns}" -v b="${sent_ns}" 'BEGIN { printf "%.1f", n / ((b - a) / 1e9) }')"
echo "scrape-loop: ${addr} for ${took} s at ${rate} Hz asked ${hz}: ok ${ok} bad ${bad} unanswered ${unanswered}"
if ((bad > 0)); then
  echo "scrape-loop: FAIL — ${bad} answer(s) other than 200" >&2
  exit 1
fi
if ((ok == 0)); then
  echo "scrape-loop: FAIL — no scrape reached an exporter" >&2
  exit 1
fi
exit 0
