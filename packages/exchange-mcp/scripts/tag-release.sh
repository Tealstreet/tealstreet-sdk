#!/usr/bin/env bash
# Tags the tealstreet-sdk mirror commit that carries this package's current
# version, which fires .github/workflows/exchange-mcp-release.yml there.
# Run after the version bump has merged to monorepo master and Copybara has
# exported it (within ~20 minutes). Needs `gh` with write access to the mirror.
set -euo pipefail

REPO=Tealstreet/tealstreet-sdk
PACKAGE_JSON=packages/exchange-mcp/package.json
cd "$(dirname "$0")/.."

version=$(node -p "require('./package.json').version")
tag="exchange-mcp-v${version}"

if gh api "repos/${REPO}/git/ref/tags/${tag}" >/dev/null 2>&1; then
  echo "${tag} already exists on ${REPO}" >&2
  exit 1
fi

sha=$(gh api "repos/${REPO}/commits/master" --jq .sha)
mirror_version=$(gh api "repos/${REPO}/contents/${PACKAGE_JSON}?ref=${sha}" -H 'Accept: application/vnd.github.raw' |
  node -e 'let s="";process.stdin.on("data",d=>s+=d).on("end",()=>console.log(JSON.parse(s).version))')
if [ "$mirror_version" != "$version" ]; then
  echo "mirror master ${sha:0:10} has ${mirror_version}, not ${version}; wait for the Copybara export" >&2
  exit 1
fi

gh api "repos/${REPO}/git/refs" -f ref="refs/tags/${tag}" -f sha="$sha" >/dev/null
echo "tagged ${REPO}@${sha:0:10} as ${tag}; follow it at https://github.com/${REPO}/actions"
