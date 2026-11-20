const assert = require('node:assert/strict');
const { existsSync, readFileSync } = require('node:fs');
const { join } = require('node:path');
const test = require('node:test');

const workflowPath = join(__dirname, '../workflows/nx-integration.yml');
const workflow = existsSync(workflowPath) ? readFileSync(workflowPath, 'utf8') : '';

test('SDK PRs run the NX caller gate on free hosted runners without publication authority', () => {
  assert.match(workflow, /^  pull_request:/m);
  assert.match(workflow, /runs-on: ubuntu-latest/);
  assert.match(workflow, /cancel-in-progress: true/);
  assert.doesNotMatch(workflow, /pull_request_target|self-hosted|: write\b|continue-on-error/);
});

test('NX checkout has an immutable resolved revision and no persistent credentials', () => {
  assert.match(workflow, /repository: Tealstreet\/tealstreet-next/);
  assert.match(workflow, /ref: \$\{\{ steps\.nx\.outputs\.sha \}\}/);
  assert.match(workflow, /persist-credentials: false/);
  assert.match(workflow, /NX_INTEGRATION_READ_TOKEN/);
});

test('caller validation includes reader/runbook checks and actual NX gates', () => {
  for (const command of ['unrolled-server-changes.test.mjs', 'rust-release-runbook.test.mjs',
    'yarn typecheck:ci', 'yarn lint:ci', 'yarn test:ci', 'nx-integration.mjs build']) {
    assert.ok(workflow.includes(command), `missing actual gate: ${command}`);
  }
});

test('native production Dockerfiles already have a cache-only PR build', () => {
  const image = readFileSync(join(__dirname, '../workflows/docker-native-server-pr.yml'), 'utf8');
  assert.match(image, /dockerfile: sdk-rs\/hosts\/server\/Dockerfile/);
  assert.match(image, /dockerfile: sdk-rs\/hosts\/candle-server\/Dockerfile/);
  assert.match(image, /--output type=cacheonly/);
  assert.doesNotMatch(image, /--push\b|--cache-to\b|: write\b/);
});
