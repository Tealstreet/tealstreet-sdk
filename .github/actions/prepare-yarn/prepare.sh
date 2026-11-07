#!/usr/bin/env bash
set -euo pipefail

cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../../.."
yarn_version="$(node -e '
  const spec = require("./package.json").packageManager;
  const match = /^yarn@(\d+\.\d+\.\d+)$/.exec(spec);
  if (!match) throw new Error("packageManager must pin an exact Yarn version");
  process.stdout.write(match[1]);
')"

# Pin the release digest so Corepack can verify the tarball without fetching
# the persistently failing /yarn/1.22.21 version-metadata endpoint. The digest
# matches npm's whole-package metadata and the npm/Yarn registry tarballs.
# Update this reviewed pin together with packageManager when upgrading Yarn.
if [[ "$yarn_version" != '1.22.21' ]]; then
  echo "No reviewed Yarn release integrity for $yarn_version" >&2
  exit 1
fi
yarn_integrity='sha512.ca75da26c00327d26267ce33536e5790f18ebd53266796fbb664d2a4a5116308042dd8ee7003b276a20eace7d3c5561c3577bdd71bcb67071187af124779620a'
export COREPACK_DEFAULT_TO_LATEST=0
# A custom registry makes Corepack request version metadata even with a hash.
# Use its built-in Yarn tarball URL for this verified release instead.
unset COREPACK_NPM_REGISTRY

# Install shims without fetching Yarn. Even setup-node's Yarn cache probe must
# wait until the pinned executable is present in Corepack's local cache.
corepack enable yarn
max_attempts=5
delay=2
for ((attempt = 1; attempt <= max_attempts; attempt++)); do
  if timeout --kill-after=5s 60s corepack prepare "yarn@${yarn_version}+${yarn_integrity}"; then
    actual_version="$(COREPACK_ENABLE_NETWORK=0 timeout --kill-after=5s 60s corepack yarn --version)"
    if [[ "$actual_version" != "$yarn_version" ]]; then
      echo "Expected Yarn $yarn_version, got $actual_version" >&2
      exit 1
    fi
    echo "Prepared Yarn $yarn_version through Corepack (attempt $attempt/$max_attempts)"
    break
  else
    status=$?
  fi
  if ((attempt == max_attempts)); then
    echo "Corepack Yarn preparation failed after $max_attempts attempts (last exit $status)" >&2
    exit "$status"
  fi
  echo "Corepack Yarn preparation attempt $attempt/$max_attempts failed (exit $status); retrying in ${delay}s" >&2
  sleep "$delay"
  delay=$((delay * 2))
done

# This cannot fetch a different executable: packageManager and the checked
# version are unchanged. Only the dependency cache is restored by the action.
cache_dir="$(COREPACK_ENABLE_NETWORK=0 timeout --kill-after=5s 60s corepack yarn cache dir)"
test -n "$cache_dir"
printf 'version=%s\ncache-dir=%s\n' "$yarn_version" "$cache_dir" >> "$GITHUB_OUTPUT"
