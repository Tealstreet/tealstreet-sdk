const assert = require('node:assert/strict');
const { readFileSync } = require('node:fs');
const { join } = require('node:path');
const test = require('node:test');

const workflowRoot = join(__dirname, '../workflows');
const read = (name) => readFileSync(join(workflowRoot, name), 'utf8');
const candidate = read('docker-native-server-pr.yml');
const publishingWorkflows = [
  ['docker-exchange-server-native.yml', 'sdk-rs/hosts/server/Dockerfile', 'exchange-server-native'],
  ['docker-candle-server-native.yml', 'sdk-rs/hosts/candle-server/Dockerfile', 'candle-server-native'],
];

const paths = (source) => [...source.matchAll(/^      - '([^']+)'$/gm)].map((match) => match[1]);

test('PR image checks cover every production image trigger and their own definition', () => {
  assert.match(candidate, /^on:\n  pull_request:\n    paths:/m);
  const selected = new Set(paths(candidate));
  for (const [workflow] of publishingWorkflows) {
    for (const path of paths(read(workflow))) {
      assert.ok(selected.has(path), `missing ${workflow} trigger: ${path}`);
    }
  }
  assert.ok(selected.has('.github/workflows/docker-native-server-pr.yml'));
  assert.ok(selected.has('.github/tests/native-server-pr-build.test.cjs'));
});

test('both full production Dockerfiles build from the same repository context and cache', () => {
  for (const [workflow, dockerfile, image] of publishingWorkflows) {
    const production = read(workflow);
    assert.ok(production.includes(`--file ${dockerfile}`));
    assert.ok(candidate.includes(`dockerfile: ${dockerfile}`));
    assert.ok(candidate.includes(`- image: ${image}`));
    assert.match(production, /docker buildx build \. \\/);
    assert.match(production, /--cache-from "type=registry,ref=\$IMAGE:buildcache-sdk"/);
  }
  assert.match(candidate, /docker buildx build \. \\/);
  assert.match(candidate, /--file "\$DOCKERFILE"/);
  assert.match(candidate, /--platform linux\/amd64/);
  assert.match(candidate, /--cache-from "type=registry,ref=\$IMAGE:buildcache-sdk"/);
  assert.match(candidate, /--output type=cacheonly/);
  assert.doesNotMatch(candidate, /--target\b|--no-cache\b|continue-on-error/);
});

test('PR builds have read-only checkout authority and cannot publish or export cache', () => {
  assert.match(candidate, /^permissions:\n  contents: read\n  packages: read\n/m);
  assert.equal([...candidate.matchAll(/\bpermissions:/g)].length, 1);
  assert.match(candidate, /persist-credentials: false/);
  assert.match(candidate, /REGISTRY_TOKEN: \$\{\{ github\.token \}\}/);
  assert.match(candidate, /docker login ghcr\.io -u "\$REGISTRY_USER" --password-stdin/);
  assert.doesNotMatch(candidate, /--secret\b|--build-arg\b/);
  assert.doesNotMatch(candidate, /pull_request_target|workflow_dispatch|^  push:|secrets\.|: write\b/m);
  assert.doesNotMatch(candidate, /--push\b|--cache-to\b|cosign|id-token:/);
  assert.match(candidate, /cancel-in-progress: true/);
  assert.match(candidate, /fail-fast: false/);
  assert.match(candidate, /timeout-minutes: 30/);
});
