import assert from 'node:assert/strict';
import { spawn, spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { chmodSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { createServer } from 'node:http';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';

const source = readFileSync(new URL('./install-binaryen.sh', import.meta.url), 'utf8');
const pinned = '195ddc94f9bc89f45abdabb0b9eea86023d727ba90eac8b35b80f2544fc30572';

async function fixture(t, { cached = false, corrupt = false, reset = false, badDownload = false } = {}) {
  const root = mkdtempSync(join(tmpdir(), 'binaryen-install-'));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const packageDir = join(root, 'binaryen-version_132'), bin = join(packageDir, 'bin');
  mkdirSync(bin, { recursive: true });
  writeFileSync(join(bin, 'wasm-opt'), '#!/usr/bin/env bash\nprintf "wasm-opt version 132\\n"\n');
  chmodSync(join(bin, 'wasm-opt'), 0o755);
  const tar = spawnSync('tar', ['-czf', join(root, 'release.tar.gz'), '-C', root, 'binaryen-version_132']);
  assert.equal(tar.status, 0, tar.stderr?.toString());
  const bytes = readFileSync(join(root, 'release.tar.gz'));
  const sha = createHash('sha256').update(bytes).digest('hex');
  const cache = join(root, 'tools/binaryen-archives'), archive = join(cache, `version_132-${sha}.tar.gz`);
  mkdirSync(cache, { recursive: true });
  if (cached) writeFileSync(archive, corrupt ? 'corrupt cache' : bytes);
  let requests = 0;
  const server = createServer((request, response) => {
    requests++;
    if (reset && requests === 1) request.socket.destroy();
    else response.end(badDownload ? 'substituted release' : bytes);
  });
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  t.after(() => new Promise(resolve => server.close(resolve)));
  const script = join(root, 'install.sh');
  // Replace only the pinned release location and digest with a local signed fixture.
  writeFileSync(script, source.replace(pinned, sha).replace(
    'https://github.com/WebAssembly/binaryen/releases/download/version_$version/binaryen-version_$version-x86_64-linux.tar.gz',
    `http://127.0.0.1:${server.address().port}/release.tar.gz`));
  const pathFile = join(root, 'path');
  const run = () => new Promise((resolve, reject) => {
    const child = spawn('bash', [script], { env: { ...process.env, RUNNER_TOOL_CACHE: join(root, 'tools'), GITHUB_PATH: pathFile } });
    let output = '';
    child.stdout.on('data', chunk => output += chunk);
    child.stderr.on('data', chunk => output += chunk);
    child.on('error', reject);
    child.on('close', status => resolve({ status, output }));
  });
  return { root, archive, pathFile, run, requests: () => requests };
}

test('warm checksum-verified cache installs without a network request and replaces an untrusted executable', async t => {
  const f = await fixture(t, { cached: true });
  const bin = join(f.root, 'tools/binaryen-version_132/bin');
  mkdirSync(bin, { recursive: true });
  writeFileSync(join(bin, 'wasm-opt'), '#!/usr/bin/env bash\nexit 99\n');
  chmodSync(join(bin, 'wasm-opt'), 0o755);
  const result = await f.run();
  assert.equal(result.status, 0, result.output);
  assert.equal(f.requests(), 0);
  assert.equal(readFileSync(f.pathFile, 'utf8'), `${bin}\n`);
});

test('a cold connection reset retries within curl before caching the verified release', async t => {
  const f = await fixture(t, { reset: true });
  const result = await f.run();
  assert.equal(result.status, 0, result.output);
  assert.equal(f.requests(), 2);
  assert.equal(existsSync(f.archive), true);
  assert.equal((await f.run()).status, 0);
  assert.equal(f.requests(), 2);
});

test('a corrupt cached archive is discarded and fetched again', async t => {
  const f = await fixture(t, { cached: true, corrupt: true });
  const result = await f.run();
  assert.equal(result.status, 0, result.output);
  assert.match(result.output, /Discarding corrupt Binaryen archive/);
  assert.equal(f.requests(), 1);
});

test('an unverified download never becomes an installed or cached tool', async t => {
  const f = await fixture(t, { badDownload: true });
  const result = await f.run();
  assert.notEqual(result.status, 0);
  assert.match(result.output, /FAILED/);
  assert.equal(existsSync(f.archive), false);
  assert.equal(existsSync(f.pathFile), false);
  assert.equal(existsSync(join(f.root, 'tools/binaryen-version_132')), false);
});

test('the fixed release and bounded transport retries are shared by every Binaryen consumer', () => {
  assert.match(source, new RegExp(`sha256=${pinned}`));
  assert.match(source, /--retry 3 --retry-all-errors --retry-delay 2 --retry-max-time 180/);
  assert.match(source, /--connect-timeout 15 --max-time 60/);
  const action = readFileSync(new URL('../../.github/actions/install-binaryen/action.yml', import.meta.url), 'utf8');
  assert.match(action, /actions\/cache@v6/);
  assert.match(action, /runner\.tool_cache.*binaryen-archives/);
  assert.match(action, /runner\.os.*runner\.arch.*hashFiles\('sdk-rs\/harness\/install-binaryen.sh'\)/);
  const workflow = readFileSync(new URL('../../.github/workflows/rust.yml', import.meta.url), 'utf8');
  assert.equal(workflow.split('uses: ./.github/actions/install-binaryen').length - 1, 4);
  assert.equal(workflow.split("- '.github/actions/install-binaryen/**'").length - 1, 2);
  assert.doesNotMatch(workflow, /run: bash sdk-rs\/harness\/install-binaryen\.sh/);
});
