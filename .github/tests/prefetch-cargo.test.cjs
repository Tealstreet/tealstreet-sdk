const assert = require('node:assert/strict');
const { spawnSync } = require('node:child_process');
const { mkdtempSync, readFileSync, writeFileSync, mkdirSync, rmSync, chmodSync } = require('node:fs');
const { tmpdir } = require('node:os');
const { join, resolve } = require('node:path');
const test = require('node:test');

const root = resolve(__dirname, '../..');
const script = join(root, '.github/actions/prefetch-cargo/prefetch.sh');
const workflow = readFileSync(join(root, '.github/workflows/rust.yml'), 'utf8');

function runPrefetch(failures, attempts = '4') {
  const directory = mkdtempSync(join(tmpdir(), 'prefetch-cargo-test-'));
  try {
    const bin = join(directory, 'bin');
    const log = join(directory, 'calls.log');
    mkdirSync(bin);
    writeFileSync(log, '');
    writeFileSync(join(bin, 'cargo'), `#!/usr/bin/env bash
echo "$*" >> "${log}"
calls=$(wc -l < "${log}")
if [ "$calls" -le ${failures} ]; then echo 'SSL error: unknown error; class=Ssl (16)' >&2; exit 101; fi
`);
    chmodSync(join(bin, 'cargo'), 0o755);
    const result = spawnSync('bash', [script], {
      encoding: 'utf8',
      env: { ...process.env, PATH: `${bin}:${process.env.PATH}`, CARGO_PREFETCH_DELAY_SECONDS: '0', CARGO_PREFETCH_ATTEMPTS: attempts },
    });
    const calls = readFileSync(log, 'utf8').trim().split('\n').filter(Boolean);
    return { status: result.status, stderr: result.stderr, calls };
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
}

test('a transient fetch failure is retried until the locked fetch succeeds', () => {
  const { status, calls, stderr } = runPrefetch(2);
  assert.equal(status, 0, stderr);
  assert.deepEqual(calls, ['fetch --locked', 'fetch --locked', 'fetch --locked']);
  assert.match(stderr, /attempt 2\/4/);
});

test('a persistent fetch failure fails the step after the bounded attempts', () => {
  const { status, calls, stderr } = runPrefetch(10, '3');
  assert.equal(status, 1);
  assert.equal(calls.length, 3);
  assert.match(stderr, /failed after 3 attempts/);
});

test('every cargo-caching Rust job prefetches after its toolchain and fetches git deps through the CLI', () => {
  const env = workflow.slice(workflow.indexOf('\nenv:\n'), workflow.indexOf('\ndefaults:\n'));
  assert.match(env, /CARGO_NET_GIT_FETCH_WITH_CLI: 'true'/);
  assert.match(env, /CARGO_NET_RETRY: '\d+'/);
  const jobs = workflow.slice(workflow.indexOf('\njobs:\n')).split(/\n  (?=[a-z][a-z0-9-]*:\n)/).slice(1);
  const cargoJobs = jobs.filter((job) => job.includes('~/.cargo/git'));
  assert.deepEqual(cargoJobs.map((job) => job.split(':')[0]), ['native', 'wasm', 'mutation-barrier-size', 'families', 'gate', 'bitget-paired']);
  for (const job of cargoJobs) {
    const toolchain = job.indexOf('rustup show');
    const prefetch = job.indexOf('uses: ./.github/actions/prefetch-cargo');
    assert.ok(toolchain >= 0 && prefetch > toolchain, job.split(':')[0]);
  }
  assert.equal(workflow.split("- '.github/actions/prefetch-cargo/**'").length - 1, 2);
});
