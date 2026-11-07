const assert = require('node:assert/strict');
const { spawnSync } = require('node:child_process');
const { mkdtempSync, readFileSync, writeFileSync, mkdirSync, rmSync, existsSync } = require('node:fs');
const { tmpdir } = require('node:os');
const { join, resolve } = require('node:path');
const test = require('node:test');

const root = resolve(__dirname, '../..');
const script = readFileSync(join(root, '.github/actions/prepare-yarn/prepare.sh'), 'utf8');

function runPreparation(options = {}) {
  const directory = mkdtempSync(join(tmpdir(), 'prepare-yarn-test-'));
  try {
    const action = join(directory, '.github/actions/prepare-yarn');
    const bin = join(directory, 'bin');
    mkdirSync(action, { recursive: true });
    mkdirSync(bin);
    writeFileSync(join(action, 'prepare.sh'), script);
    writeFileSync(join(directory, 'package.json'), JSON.stringify({ packageManager: options.spec ?? 'yarn@1.22.21' }));
    writeFileSync(join(bin, 'corepack'), `#!/usr/bin/env node
const fs = require('node:fs');
const path = process.env.CI_YARN_TEST_LOG;
const args = process.argv.slice(2);
fs.appendFileSync(path, JSON.stringify({args, network: process.env.COREPACK_ENABLE_NETWORK, registry: process.env.COREPACK_NPM_REGISTRY, latest: process.env.COREPACK_DEFAULT_TO_LATEST})+'\\n');
if (args[0] === 'enable') process.exit(Number(process.env.CI_YARN_TEST_ENABLE_EXIT || 0));
if (args[0] === 'prepare') {
 const calls = fs.readFileSync(path, 'utf8').trim().split('\\n').map(JSON.parse).filter(c => c.args[0] === 'prepare').length;
 if (calls <= Number(process.env.CI_YARN_TEST_FAILURES || 0)) {
   console.error('HTTP 503 while fetching tarball');
   process.exit(Number(process.env.CI_YARN_TEST_FAILURE_EXIT || 1));
 }
 process.exit(0);
}
if (args.join(' ') === 'yarn --version') console.log(process.env.CI_YARN_TEST_VERSION || '1.22.21');
else if (args.join(' ') === 'yarn cache dir') {
 process.exitCode = Number(process.env.CI_YARN_TEST_CACHE_EXIT || 0);
 console.log('/tmp/mock yarn cache');
} else process.exit(99);
`, { mode: 0o755 });
    writeFileSync(join(bin, 'sleep'), '#!/bin/sh\nprintf "%s\\n" "$1" >> "$CI_YARN_TEST_SLEEPS"\n', { mode: 0o755 });
    // Exercise production timeout arguments without adding real minutes to unit tests.
    writeFileSync(join(bin, 'timeout'), '#!/bin/sh\n[ "$1" = "--kill-after=5s" ] && [ "$2" = "60s" ] || exit 98\nshift 2\nexec "$@"\n', { mode: 0o755 });
    const log = join(directory, 'commands');
    const sleeps = join(directory, 'sleeps');
    const output = join(directory, 'outputs');
    const result = spawnSync('bash', ['-e', join(action, 'prepare.sh')], {
      cwd: directory, encoding: 'utf8', timeout: 10_000,
      env: {
        ...process.env, PATH: `${bin}:${process.env.PATH}`, GITHUB_OUTPUT: output,
        COREPACK_NPM_REGISTRY: 'https://metadata-endpoint-must-not-be-used.invalid',
        CI_YARN_TEST_LOG: log, CI_YARN_TEST_SLEEPS: sleeps,
        CI_YARN_TEST_FAILURES: String(options.failures ?? 0),
        CI_YARN_TEST_FAILURE_EXIT: String(options.failureExit ?? 1),
        CI_YARN_TEST_ENABLE_EXIT: String(options.enableExit ?? 0),
        CI_YARN_TEST_CACHE_EXIT: String(options.cacheExit ?? 0),
        CI_YARN_TEST_VERSION: options.version ?? '1.22.21',
      },
    });
    assert.ifError(result.error);
    return {
      ...result,
      commands: existsSync(log) ? readFileSync(log, 'utf8').trim().split('\n').map(JSON.parse) : [],
      sleeps: existsSync(sleeps) ? readFileSync(sleeps, 'utf8').trim().split('\n') : [],
      output: existsSync(output) ? readFileSync(output, 'utf8') : '',
    };
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
}

test('prepares the integrity-pinned tarball once before offline version and cache probes', () => {
  const r = runPreparation();
  assert.equal(r.status, 0, r.stderr);
  assert.deepEqual(r.commands.map(c => c.args[0]), ['enable', 'prepare', 'yarn', 'yarn']);
  assert.match(r.commands[1].args[1], /^yarn@1\.22\.21\+sha512\.[a-f0-9]{128}$/);
  assert.ok(r.commands.every(c => c.registry === undefined && c.latest === '0'));
  assert.ok(r.commands.slice(2).every(c => c.network === '0'));
  assert.deepEqual(r.sleeps, []);
  assert.equal(r.output, 'version=1.22.21\ncache-dir=/tmp/mock yarn cache\n');
});

test('transient download 503s back off and recover before any cache lookup', () => {
  const r = runPreparation({ failures: 2 });
  assert.equal(r.status, 0, r.stderr);
  assert.deepEqual(r.sleeps, ['2', '4']);
  assert.deepEqual(r.commands.map(c => c.args[0]), ['enable', 'prepare', 'prepare', 'prepare', 'yarn', 'yarn']);
});

for (const failureExit of [1, 124, 137]) {
  test(`exhausted download/timeout failures stop after five attempts (exit ${failureExit})`, () => {
    const r = runPreparation({ failures: 9, failureExit });
    assert.equal(r.status, failureExit, r.stderr);
    assert.equal(r.commands.filter(c => c.args[0] === 'prepare').length, 5);
    assert.ok(r.commands.every(c => c.args[0] !== 'yarn'));
    assert.deepEqual(r.sleeps, ['2', '4', '8', '16']);
    assert.equal(r.output, '');
  });
}

test('a timed-out attempt can recover', () => {
  const r = runPreparation({ failures: 1, failureExit: 124 });
  assert.equal(r.status, 0, r.stderr);
  assert.deepEqual(r.sleeps, ['2']);
});

test('wrong executable version fails without publishing a cache path', () => {
  const r = runPreparation({ version: '1.22.22' });
  assert.equal(r.status, 1, r.stderr);
  assert.ok(!r.commands.some(c => c.args.includes('cache')));
  assert.deepEqual(r.sleeps, []);
  assert.equal(r.output, '');
});

for (const spec of ['yarn@latest', 'yarn@1.22.22', 'pnpm@1.22.21']) {
  test(`unreviewed packageManager ${spec} fails before Corepack`, () => {
    const r = runPreparation({ spec });
    assert.notEqual(r.status, 0);
    assert.deepEqual(r.commands, []);
    assert.equal(r.output, '');
  });
}

test('shim and cache failures propagate without install retry or success outputs', () => {
  for (const options of [{ enableExit: 77 }, { cacheExit: 22 }]) {
    const r = runPreparation(options);
    assert.equal(r.status, options.enableExit ?? options.cacheExit, r.stderr);
    assert.deepEqual(r.sleeps, []);
    assert.equal(r.output, '');
  }
});

test('all Yarn jobs prepare before probing caches or installing', () => {
  const jobs = [];
  for (const file of ['ci.yml', 'rust.yml', 'exchange-mcp-release.yml']) {
    const source = readFileSync(join(root, '.github/workflows', file), 'utf8');
    assert.doesNotMatch(source, /cache: yarn/);
    assert.doesNotMatch(source, /(?:run: |^\s+)yarn\s/m);
    for (const match of source.matchAll(/^  ([\w-]+):\n([\s\S]*?)(?=^  [\w-]+:\n|$(?![\s\S]))/gm)) {
      const [, name, job] = match;
      if (!job.includes('corepack yarn')) continue;
      jobs.push(`${file}:${name}`);
      const prep = job.indexOf('uses: ./.github/actions/prepare-yarn');
      assert.ok(prep > job.indexOf('uses: actions/setup-node@'), file);
      assert.ok(prep < job.indexOf('corepack yarn'), file);
      assert.equal(job.match(/uses: \.\/\.github\/actions\/prepare-yarn/g)?.length, 1, file);
    }
  }
  assert.deepEqual(jobs, ['ci.yml:ci', 'rust.yml:harness', 'rust.yml:families',
    'rust.yml:combined', 'rust.yml:browser', 'rust.yml:replay', 'rust.yml:gate', 'rust.yml:cpu-plan',
    'rust.yml:cpu-certify', 'exchange-mcp-release.yml:release']);
  const rust = readFileSync(join(root, '.github/workflows/rust.yml'), 'utf8');
  assert.equal(rust.match(/'\.github\/actions\/prepare-yarn\/\*\*'/g)?.length, 1, 'helper changes must be in the Rust push and PR scope');
  const action = readFileSync(join(root, '.github/actions/prepare-yarn/action.yml'), 'utf8');
  assert.ok(action.indexOf('prepare.sh') < action.indexOf('uses: actions/cache/restore@'));
  for (const key of ['runner.os', 'runner.arch', 'steps.yarn.outputs.version', "hashFiles('yarn.lock')"]) {
    assert.ok(action.includes(key), `dependency cache must include ${key}`);
  }
});
