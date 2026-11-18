#!/usr/bin/env bash
# Restarts this box's exchange server without dropping a Hosted session or
# cutting a chase or TWAP short. Runs on the box itself; see DESIGN.md
# "Drain and roll".
#
#   roll.sh [--now] [--ceiling SECONDS] [--unit NAME] [ADMIN_URL]
#
# ADMIN_URL defaults to http://127.0.0.1:${TEALSTREET_PORT:-23456}. The admin
# routes answer only on-box requests (loopback or the Docker bridge) that no proxy forwarded.
set -euo pipefail

usage() {
  sed -n '6p' "$0" | sed 's/^#   //' >&2
  exit 2
}

now=0
ceiling=1800
unit=exchange-server
url="http://127.0.0.1:${TEALSTREET_PORT:-23456}"
while (($#)); do
  case $1 in
    --now) now=1 ;;
    --ceiling)
      [[ ${2:-} =~ ^[0-9]+$ ]] || usage
      ceiling=$2
      shift
      ;;
    --unit)
      [[ -n ${2:-} ]] || usage
      unit=$2
      shift
      ;;
    -h | --help) usage ;;
    -*) usage ;;
    *) url=${1%/} ;;
  esac
  shift
done

log() { printf '%s roll: %s\n' "$(date -u +%FT%TZ)" "$*" >&2; }
fail() {
  log "FAILED: $*"
  exit 1
}

# Prints "<chases> <twaps>" from a /health body, or fails.
algos_of() {
  local counts
  counts=$(sed -nE 's/.*"algos":\{"chases":([0-9]+),"twaps":([0-9]+)\}.*/\1 \2/p' <<<"$1")
  [[ -n $counts ]] || return 1
  printf '%s\n' "$counts"
}

health_body() {
  # A draining box answers 503 on purpose; the body still carries the counts.
  curl -sS --max-time 5 "$url/health"
}

admin() {
  local path=$1 max_time=$2 body
  body=$(curl -sS --fail-with-body --max-time "$max_time" -X POST "$url$path") ||
    fail "POST $path: ${body:-no answer}"
  log "POST $path: $body"
}

if ((now)); then
  log "break-glass: stopping every chase and TWAP, then moving every socket"
  admin '/admin/drain?now=1' 60
else
  admin /admin/drain 10
  log "waiting up to ${ceiling}s for chases and TWAPs to end"
  deadline=$((SECONDS + ceiling))
  while :; do
    body=$(health_body) || fail "GET /health: no answer"
    read -r chases twaps < <(algos_of "$body") || fail "GET /health: no algo counts in $body"
    if ((chases == 0 && twaps == 0)); then
      break
    fi
    if ((SECONDS >= deadline)); then
      fail "after ${ceiling}s, $chases chase(s) and $twaps TWAP(s) still run; the box stays drained. Rerun with --now to stop them, or POST $url/admin/undrain"
    fi
    log "$chases chase(s), $twaps TWAP(s) still running"
    sleep 5
  done
  log "no chase or TWAP runs here"
fi

log "restarting $unit"
systemctl restart "$unit" || fail "systemctl restart $unit"

deadline=$((SECONDS + 120))
until curl -fsS --max-time 5 "$url/health" 2>/dev/null | grep -q '"ok":true'; do
  ((SECONDS < deadline)) || fail "$unit did not report /health ok within 120s"
  sleep 2
done
log "$unit is healthy"

# A fresh process starts undrained; this only confirms it.
admin /admin/undrain 10
log "done"
