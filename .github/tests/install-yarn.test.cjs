const assert = require('node:assert/strict');
const { spawnSync } = require('node:child_process');
const { mkdtempSync, readFileSync, writeFileSync, mkdirSync, rmSync, existsSync } = require('node:fs');
const { tmpdir } = require('node:os');
const { join, resolve } = require('node:path');
const test = require('node:test');
const { InstallEvidence, retrySafeSources } = require('../actions/install-yarn/install.cjs');

const root = resolve(__dirname, '../..');
const step = message => ({ type: 'step', data: { message, current: 2, total: 4 } });
const error = data => ({ type: 'error', data });
const network = 'Error: https://registry.npmjs.org/cosmiconfig/-/cosmiconfig-8.3.6.tgz: Request failed "503 Service Unavailable"\n    at Request.callback (yarn.js:1:2)';
function retryable(events, exit = 1, manifest = {}) {
  const evidence = new InstallEvidence(manifest);
  for (const event of events) evidence.line(typeof event === 'string' ? event : JSON.stringify(event));
  return evidence.retryable(exit);
}

test('only structured transient network errors during fetch/resolve qualify', () => {
  assert.ok(retryable([step('Validating package.json'), step('Resolving packages'), step('Fetching packages'), error(network)]));
  assert.equal(retryable([step('Validating package.json'), error(network)]), false);
  for (const phase of ['Resolving packages', 'Fetching packages']) {
    for (const status of ['408 Request Timeout', '429 Too Many Requests', '500 Internal Server Error', '502 Bad Gateway', '503 Service Unavailable', '504 Gateway Timeout']) {
      assert.ok(retryable([step(phase), error(`Error: https://registry.npmjs.org/pkg/-/pkg.tgz: Request failed "${status}"`)]));
    }
    for (const message of ['https://registry.npmjs.org/pkg: read ECONNRESET', 'getaddrinfo EAI_AGAIN registry.npmjs.org', 'connect ETIMEDOUT 1.2.3.4:443', 'https://registry.npmjs.org/pkg: socket hang up', 'ESOCKETTIMEDOUT']) {
      assert.ok(retryable([step(phase), error(message)]), message);
    }
  }
  assert.ok(retryable([step('Fetching packages'), error(network)]));
});

test('deterministic errors and fatal mixtures never retry', () => {
  for (const message of [
    'https://registry.npmjs.org/pkg: Request failed "404 Not Found"',
    'https://registry.npmjs.org/pkg: Request failed "401 Unauthorized"',
    'Your lockfile needs to be updated, but yarn was run with --frozen-lockfile.',
    'Integrity check failed for cosmiconfig (computed integrity does not match our records)',
    'The engine "node" is incompatible with this module.',
    'Command failed with exit code 1.',
    'A script mentioning HTTP 503 timed out',
    'Error: certificate has expired',
    'Something unknown happened',
  ]) {
    assert.equal(retryable([step('Fetching packages'), error(message)]), false, message);
    assert.equal(retryable([step('Fetching packages'), error(network), error(message)]), false, message);
  }
});

test('no retry before phases, after linking/building or with root preinstall/lifecycle', () => {
  assert.equal(retryable([error(network)]), false);
  for (const phase of ['Linking dependencies', 'Building fresh packages', 'Unexpected phase', 'constructor', 'toString', 123, null]) {
    assert.equal(retryable([step('Fetching packages'), step(phase), error(network)]), false);
    assert.equal(retryable([step(phase), step('Fetching packages'), error(network)]), false);
  }
  assert.equal(retryable([step('Fetching packages'), error(network)], 1, { scripts: { preinstall: 'echo do-not-replay' } }), false);
  assert.equal(retryable([step('Fetching packages'), { type: 'command', data: 'preinstall' }, error(network)]), false);
});

test('unknown/unstructured errors and nonstandard exits fail closed', () => {
  for (const unknown of ['error unknown failure', 'Error: a plain script failure', '{}', 'null', '123', error({ message: network })]) {
    assert.equal(retryable([step('Fetching packages'), unknown, error(network)]), false);
  }
  for (const exit of [0, 2, 77, 124, 137, 143]) assert.equal(retryable([step('Fetching packages'), error(network)], exit), false);
  assert.equal(retryable([step('Fetching packages'), { type: 'info', data: '503' }]), false);
  assert.ok(retryable([step('Fetching packages'), '(node:123) [DEP0169] DeprecationWarning: URL parsing', error(network)]));
});

function fixture({ failures = 0, mode = 'network', exit = 1, manifest = {}, lockfile = '  resolved "https://registry.npmjs.org/pkg/-/pkg-1.0.0.tgz"\n' } = {}, control = false) {
  const directory = mkdtempSync(join(tmpdir(), 'install-yarn-test-'));
  try {
    const action = join(directory, '.github/actions/install-yarn');
    const bin = join(directory, 'bin');
    const calls = join(directory, 'calls');
    const sleeps = join(directory, 'sleeps');
    mkdirSync(action, { recursive: true });
    mkdirSync(bin);
    writeFileSync(join(action, 'install.cjs'), readFileSync(join(root, '.github/actions/install-yarn/install.cjs')));
    writeFileSync(join(directory, 'package.json'), JSON.stringify(manifest));
    writeFileSync(join(directory, 'yarn.lock'), lockfile);
    writeFileSync(join(bin, 'sleep'), '#!/bin/sh\nprintf "%s\\n" "$1" >> "$CI_INSTALL_TEST_SLEEPS"\n', { mode: 0o755 });
    writeFileSync(join(bin, 'corepack'), `#!/usr/bin/env node
const fs = require('node:fs');
const args = process.argv.slice(2);
fs.appendFileSync(process.env.CI_INSTALL_TEST_CALLS, JSON.stringify(args)+'\\n');
const n = fs.readFileSync(process.env.CI_INSTALL_TEST_CALLS,'utf8').trim().split('\\n').length;
console.log(JSON.stringify({type:'step',data:{message:'Validating package.json',current:1,total:5}}));
console.log(JSON.stringify({type:'step',data:{message:'Resolving packages',current:2,total:5}}));
console.log(JSON.stringify({type:'step',data:{message:'Fetching packages',current:2,total:4}}));
if(n <= Number(process.env.CI_INSTALL_TEST_FAILURES)) {
 const mode = process.env.CI_INSTALL_TEST_MODE;
 if(mode==='script') console.log(JSON.stringify({type:'step',data:{message:'Building fresh packages',current:4,total:4}}));
 // Yarn's reporter writes step events to stdout and errors to stderr.
 console.error(JSON.stringify({type:'error',data: mode==='integrity' ? 'Integrity check failed' : ${JSON.stringify(network)}}));
 process.exit(Number(process.env.CI_INSTALL_TEST_EXIT));
}
console.log(JSON.stringify({type:'success',data:'Saved lockfile.'}));
`, { mode: 0o755 });
    const env = {
      ...process.env, PATH: `${bin}:${process.env.PATH}`,
      CI_INSTALL_TEST_CALLS: calls, CI_INSTALL_TEST_SLEEPS: sleeps,
      CI_INSTALL_TEST_FAILURES: String(failures), CI_INSTALL_TEST_MODE: mode,
      CI_INSTALL_TEST_EXIT: String(exit),
    };
    const result = control
      ? spawnSync('corepack', ['yarn', 'install', '--frozen-lockfile', '--network-timeout', '600000'], { cwd: directory, env, encoding: 'utf8', timeout: 10_000 })
      : spawnSync(process.execPath, [join(action, 'install.cjs')], { cwd: directory, env, encoding: 'utf8', timeout: 10_000 });
    assert.ifError(result.error);
    return {
      ...result,
      calls: readFileSync(calls, 'utf8').trim().split('\n').map(JSON.parse),
      sleeps: existsSync(sleeps) ? readFileSync(sleeps, 'utf8').trim().split('\n') : [],
    };
  } finally { rmSync(directory, { recursive: true, force: true }); }
}

test('single-shot original is red; shared retry recovers the same injected 503', () => {
  const original = fixture({ failures: 1 }, true);
  assert.equal(original.status, 1);
  const fixed = fixture({ failures: 1 });
  assert.equal(fixed.status, 0, fixed.stderr);
  assert.equal(fixed.calls.length, 2);
  assert.deepEqual(fixed.sleeps, ['5']);
  assert.ok(fixed.stdout.includes('cosmiconfig'));
  assert.ok(fixed.calls.every(c => JSON.stringify(c) === JSON.stringify(['yarn', 'install', '--frozen-lockfile', '--network-timeout', '600000', '--prefer-offline', '--json'])));
});

test('immediate success does not retry; network exhaustion is bounded to three attempts', () => {
  const success = fixture();
  assert.equal(success.status, 0, success.stderr);
  assert.equal(success.calls.length, 1);
  assert.deepEqual(success.sleeps, []);
  const exhausted = fixture({ failures: 8 });
  assert.equal(exhausted.status, 1);
  assert.equal(exhausted.calls.length, 3);
  assert.deepEqual(exhausted.sleeps, ['5', '10']);
});

test('real runner does not retry checksum, lifecycle, preinstall or nonstandard failures', () => {
  for (const options of [{ mode: 'integrity' }, { mode: 'script' }, { manifest: { scripts: { preinstall: 'echo once' } } }, { exit: 77 }, { lockfile: '  resolved "git+https://github.com/example/repo.git#abc"\n' }]) {
    const r = fixture({ failures: 8, ...options });
    assert.equal(r.status, options.exit ?? 1, r.stderr);
    assert.equal(r.calls.length, 1);
    assert.deepEqual(r.sleeps, []);
  }
});

test('new source kinds cannot replay Git prepare scripts during fetch', () => {
  assert.ok(retrySafeSources(readFileSync(join(root, 'yarn.lock'), 'utf8')));
  for (const source of ['git+https://github.com/example/repo.git#abc', 'https://github.com/example/repo.git#abc', 'https://codeload.github.com/example/repo/tar.gz/abc', 'file:../local']) {
    assert.equal(retrySafeSources(`  resolved "${source}"\n`), false, source);
  }
});

test('all installs follow restore and save cache immediately only on install success', () => {
  const prepare = readFileSync(join(root, '.github/actions/prepare-yarn/action.yml'), 'utf8');
  const install = readFileSync(join(root, '.github/actions/install-yarn/action.yml'), 'utf8');
  assert.ok(prepare.includes('uses: actions/cache/restore@v6'));
  assert.ok(!prepare.includes('uses: actions/cache@'));
  assert.ok(install.indexOf('install.cjs') < install.indexOf('uses: actions/cache/save@v6'));
  assert.ok(install.includes("if: inputs.cache-hit != 'true' && inputs.cache-key != ''"));
  assert.ok(!install.includes('always()'));
  const callers = {
    'ci.yml': ['ci'],
    'rust.yml': ['families', 'browser', 'gate', 'bitget-paired', 'cpu-plan', 'cpu-certify'],
    'exchange-mcp-release.yml': ['release'],
  };
  for (const [file, expected] of Object.entries(callers)) {
    const source = readFileSync(join(root, '.github/workflows', file), 'utf8');
    assert.ok(!source.includes('run: corepack yarn install'));
    const jobs = source.split(/^  ([\w-]+):\n/m), installed = [];
    for (let index = 1; index < jobs.length; index += 2) {
      const job = jobs[index + 1];
      if (!job.includes('uses: ./.github/actions/install-yarn')) continue;
      installed.push(jobs[index]);
      assert.ok(job.indexOf('uses: ./.github/actions/prepare-yarn') < job.indexOf('uses: ./.github/actions/install-yarn'));
      for (const field of ['cache-dir', 'cache-key', 'cache-hit']) assert.ok(job.includes(`${field}: \${{ steps.yarn.outputs.${field} }}`));
    }
    assert.deepEqual(installed, expected, file);
  }
  const rust = readFileSync(join(root, '.github/workflows/rust.yml'), 'utf8');
  assert.equal(rust.match(/'\.github\/actions\/install-yarn\/\*\*'/g)?.length, 2);
});
