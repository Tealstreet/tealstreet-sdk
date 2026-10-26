import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import test from 'node:test';
import { cpuPartitions, validateWorkerRoster, runCpuWorkers } from './cpu-workers.mjs';
import { CPU_REMEASURE_SUITES } from './cpu-rollout.mjs';

test('four verification partitions cover the exact full roster, with no omission or overlap', () => {
  const rows = cpuPartitions().flat().map(suite => ({ suite }));
  assert.deepEqual(validateWorkerRoster(rows).map(x => x.suite), CPU_REMEASURE_SUITES);
  for (const invalid of [rows.slice(1), [...rows, rows[0]], [...rows.slice(1), { suite: 'foreign' }]])
    assert.throws(() => validateWorkerRoster(invalid));
  for (const count of [0, 5, 1.5]) assert.throws(() => cpuPartitions(count));
});

test('verification processes have isolated checkouts/output roots and retain all bindings', async t => {
  const parent = mkdtempSync(resolve(tmpdir(), 'cpu-worker-test-'));
  t.after(() => rmSync(parent, { recursive: true, force: true }));
  const root = resolve(parent, 'source'), out = resolve(parent, 'results'); mkdirSync(root);
  const git = (...args) => execFileSync('git', args, { cwd: root, encoding: 'utf8' }).trim();
  git('init', '-q', '-b', 'master'); git('config', 'user.name', 'fixture'); git('config', 'user.email', 'fixture@example.test');
  writeFileSync(resolve(root, '.gitignore'), 'node_modules/\n');
  mkdirSync(resolve(root, 'node_modules/fixture-dependency'), { recursive: true });
  writeFileSync(resolve(root, 'node_modules/fixture-dependency/index.js'), "module.exports = 'locked parent dependency';\n");
  mkdirSync(resolve(root, 'sdk-rs/harness'), { recursive: true });
  writeFileSync(resolve(root, 'sdk-rs/harness/cpu-ci.mjs'), `
import {mkdirSync,writeFileSync,existsSync} from 'node:fs';
import {execFileSync} from 'node:child_process';
import {createRequire} from 'node:module';
if(createRequire(import.meta.url)('fixture-dependency')!=='locked parent dependency')throw new Error('wrong parent dependency');
const args=process.argv.slice(2), value=k=>args[args.indexOf(k)+1];
const suites=JSON.parse(value('--worker-suites'));
if(existsSync('own-output'))throw new Error('shared output tree');
mkdirSync('own-output');
const result={phase:value('--command').split('-')[0],runId:42,
checkoutSha:execFileSync('git',['rev-parse','HEAD'],{encoding:'utf8'}).trim(),
rows:suites.map(suite=>({suite,workspace:process.cwd()})),bindings:[{fixture:suites[0]}],secondDispositions:[]};
writeFileSync(value('--out')+'/worker.json',JSON.stringify(result));
`);
  git('add', '.'); git('commit', '-qm', 'worker fixture');
  const context = { root, checkoutSha: git('rev-parse', 'HEAD'), runId: 42, env: { ...process.env, RUNNER_TEMP: parent }, bindings: [] };
  const rows = await runCpuWorkers(context, 'plan', out);
  assert.equal(new Set(rows.map(x => x.workspace)).size, 4);
  assert.equal(rows.length, CPU_REMEASURE_SUITES.length); assert.equal(context.bindings.length, 4);
  assert.equal(readdirSync(parent).filter(x => x.startsWith('cpu-verifiers-')).length, 0);
  for (let i = 0; i < 4; i++) assert(readFileSync(resolve(out, `worker-${i}/worker.json`)).length);
});
