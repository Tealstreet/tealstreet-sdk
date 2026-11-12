import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { resolve } from 'node:path';
import test, { after } from 'node:test';
import { runBitgetPaired } from './bitget-paired-run.mjs';

const parent = resolve('sdk-rs/harness/out');
mkdirSync(parent, { recursive: true });
const scratch = mkdtempSync(resolve(parent, 'bitget-paired-run-test-'));
after(() => rmSync(scratch, { recursive: true, force: true }));

test('deadline fails closed and kills a TERM-resistant collector and its descendant', async () => {
  const out = resolve(scratch, 'timeout'), marker = resolve(scratch, 'heartbeat');
  const descendant = `process.on('SIGTERM', () => {});
    setInterval(() => require('node:fs').appendFileSync(${JSON.stringify(marker)}, 'x'), 20);`;
  const code = `const fs = require('node:fs'); process.on('SIGTERM', () => {});
    fs.mkdirSync(${JSON.stringify(out)}, {recursive:true});
    fs.writeFileSync(${JSON.stringify(resolve(out, 'steady-assessment.json'))}, '{"valid":true}');
    require('node:child_process').spawn(process.execPath, ['-e', ${JSON.stringify(descendant)}], {stdio:'inherit'});
    setInterval(() => {}, 100);`;
  const result = await runBitgetPaired({ out, command: process.execPath, args: ['-e', code],
    timeoutMs: 1000, killGraceMs: 100 });
  assert.equal(result, 124, 'deadline must fail, never cancel or succeed');
  const failure = JSON.parse(readFileSync(resolve(out, 'steady-assessment.json')));
  assert.equal(failure.valid, false);
  assert.equal(failure.certified, false);
  assert.equal(failure.certificate, null);
  assert.equal(failure.disposition, 'TIMED-OUT');
  const stopped = readFileSync(marker, 'utf8');
  assert(stopped.length > 0, 'working control: descendant ran before the deadline');
  await new Promise(resolve => setTimeout(resolve, 150));
  assert.equal(readFileSync(marker, 'utf8'), stopped, 'no orphan continues collecting after timeout');
});

test('normal collector success and failure keep their exit codes without a timeout receipt', async () => {
  for (const status of [0, 7]) {
    const out = resolve(scratch, `exit-${status}`);
    assert.equal(await runBitgetPaired({ out, command: process.execPath,
      args: ['-e', `process.exit(${status})`], timeoutMs: 1000 }), status);
    assert.throws(() => readFileSync(resolve(out, 'steady-assessment.json')), { code: 'ENOENT' });
  }
});

test('workflow uses the internal deadline without enlarging the 45-minute job limit', () => {
  const workflow = readFileSync(new URL('../../.github/workflows/rust.yml', import.meta.url), 'utf8');
  const job = /^  bitget-paired:\n([\s\S]*?)(?=^  [a-z][a-z0-9-]*:\n)/m.exec(workflow)?.[1];
  assert(job);
  assert.match(job, /timeout-minutes: 45/);
  assert.match(job, /run: node sdk-rs\/harness\/bitget-paired-run\.mjs --out/);
  assert.doesNotMatch(job, /continue-on-error/);
  assert.match(job, /if: success\(\) \|\| failure\(\)/);
});

test('an external cancellation is forwarded to the detached process group', () => {
  const out = resolve(scratch, 'interrupt');
  const child = `setTimeout(() => process.kill(process.ppid, 'SIGTERM'), 50); setInterval(() => {}, 100);`;
  const code = `import {runBitgetPaired} from ${JSON.stringify(new URL('./bitget-paired-run.mjs', import.meta.url).href)};
    process.exitCode = await runBitgetPaired({out:${JSON.stringify(out)}, timeoutMs:5000,
      args:['-e', ${JSON.stringify(child)}]});`;
  const result = spawnSync(process.execPath, ['--input-type=module', '-e', code], { timeout: 2000, encoding: 'utf8' });
  assert.equal(result.error, undefined);
  assert.equal(result.signal, null, 'wrapper must forward the signal and wait for its child');
  assert.equal(result.status, 143);
  assert.match(result.stderr, /SIGTERM/);
});
