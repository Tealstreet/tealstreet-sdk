#!/usr/bin/env bash
# Git dependencies (franklin-crypto for ApeX Omni) have failed mid-run on transient
# TLS errors. Fetching once up front, with retries, keeps every later cargo step off the network.
set -uo pipefail

attempts=${CARGO_PREFETCH_ATTEMPTS:-4}
delay_seconds=${CARGO_PREFETCH_DELAY_SECONDS:-15}
for ((attempt = 1; attempt <= attempts; attempt++)); do
  if cargo fetch --locked; then
    exit 0
  fi
  if ((attempt < attempts)); then
    delay=$((attempt * delay_seconds))
    echo "cargo fetch failed (attempt $attempt/$attempts); retrying in ${delay}s" >&2
    sleep "$delay"
  fi
done
echo "cargo fetch failed after $attempts attempts" >&2
exit 1
