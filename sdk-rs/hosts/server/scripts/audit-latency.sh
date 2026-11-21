#!/usr/bin/env bash
# Per-venue p50/p95/p99 of each order-audit stage, from journal lines on stdin.
# Read-only; see DESIGN.md "Order audit stage timings".
#
#   journalctl CONTAINER_NAME=exchange-server -o cat --since '1 hour ago' | audit-latency.sh
set -euo pipefail

if (($#)); then
  sed -n '5p' "$0" | sed 's/^#   //' >&2
  exit 2
fi
command -v jq >/dev/null || {
  echo 'audit-latency: jq is required' >&2
  exit 1
}

jq -Rrn '
  # Nearest-rank percentile of a sorted array.
  def pct($p): .[([(($p * length) | ceil) - 1, 0] | max)];
  def ms: (. / 100 | round) / 10;
  def span($from; $to):
    if (.[$from] | type) == "number" and (.[$to] | type) == "number"
    then .[$to] - .[$from] else empty end;
  [
    ["admit", "received", "admitted"],
    ["queue", "admitted", "queued"],
    ["release", "queued", "sent"],
    ["venue", "sent", "responded"],
    ["dispatch", "queued", "responded"],
    ["reply", "responded", "replied"],
    ["total", "received", "replied"]
  ] as $spans
  | [inputs | fromjson? | select(type == "object" and .msg == "order audit"
      and (.stages_us | type) == "object")]
  | group_by(.venue) as $venues
  | (["venue", "stage", "n", "p50_ms", "p95_ms", "p99_ms"] | @tsv),
    ($venues[] as $lines
      | $spans[] as [$name, $from, $to]
      | [$lines[] | .stages_us | span($from; $to)] | sort
      | select(length > 0)
      | [$lines[0].venue, $name, length, (pct(0.50) | ms), (pct(0.95) | ms), (pct(0.99) | ms)]
      | @tsv),
    "",
    (["venue", "commands", "unknown"] | @tsv),
    ($venues[]
      | [.[0].venue, length,
          (map(select((.result | type) == "object" and .result.outcome == "unknown")) | length)]
      | @tsv)
'
