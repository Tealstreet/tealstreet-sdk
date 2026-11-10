// Temporary CI-only before/after evidence. These trials never certify a gate.
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { parseArgs, isDeepStrictEqual } from 'node:util';
import { validateCiWasm } from './ci-wasm.mjs';
import { observeCpu } from './cpu-observation.mjs';
import { median, performanceMetricErrors, performanceOrder, performanceVerdict } from './performance-policy.mjs';
import { compareSnapshots } from './snapshot-compare.mjs';

const { values } = parseArgs({ options: { suite: { type: 'string' }, out: { type: 'string' },
  baseline: { type: 'string' }, 'baseline-wasm': { type: 'string' } } });
if (process.env.GITHUB_ACTIONS !== 'true' || process.env.GITHUB_RUN_ATTEMPT !== '1' ||
    !/^\d+$/.test(process.env.GITHUB_RUN_ID ?? '') || !['asterdex', 'bybit'].includes(values.suite) || !values.out)
  throw new Error('This investigation requires an original CI attempt and an explicit supported suite/output');
const venue = values.suite === 'bybit' ? 'hyrotrader' : 'asterdex';
const tape = `sdk-rs/harness/fixtures/${venue}-public.jsonl.gz`;
const directory = resolve(values.out);
mkdirSync(directory, { recursive: false });
const module = validateCiWasm({ venue, directory: 'sdk-rs/hosts/wasm/pkg/ci-replay' });
const sha = path => createHash('sha256').update(readFileSync(path)).digest('hex');
const baselineRoot = resolve(values.baseline);
const baseline = validateCiWasm({ root: baselineRoot, venue, directory: resolve(values['baseline-wasm']) });
if (sha(resolve(baselineRoot, tape)) !== sha(tape)) throw new Error('Comparison tapes differ');
const arms = { A: { root: baselineRoot, module: baseline }, B: { root: process.cwd(), module } };
const seals = { node: sha(process.execPath), tape: sha(tape), module: module.metadata, baseline: baseline.metadata };
const samples = { A: { ts: [], rust: [] }, B: { ts: [], rust: [] } };
const originals = {}, rows = [];
const order = ['A', 'B', 'B', 'A', 'A', 'B', 'B', 'A'];
const environment = { ...process.env, ESBUILD_WORKER_THREADS: '0', TMPDIR: `${directory}/tmp` };
mkdirSync(environment.TMPDIR);
const probe = `${directory}/profile-markers.cjs`;
writeFileSync(probe, `
if (require('node:worker_threads').isMainThread) {
const { writeFileSync } = require('node:fs');
const original = process.threadCpuUsage.bind(process);
const seen = new WeakMap(), markers = [];
process.threadCpuUsage = previous => {
  const at = Number(process.hrtime.bigint() / 1000n);
  const value = original(previous), call = markers.length + 1;
  markers.push({ at, call, previousCall: seen.get(previous) ?? null, ...value });
  seen.set(value, call);
  return value;
};
process.once('exit', () => writeFileSync(process.env.RAW_CPU_MARKERS, JSON.stringify(markers)));
}
`);

function replay(arm, impl, path, profile) {
  const source = arms[arm];
  const args = ['--expose-gc', ...(profile ? ['--cpu-prof', '--cpu-prof-interval=100',
    `--cpu-prof-dir=${directory}`, `--cpu-prof-name=${profile}.cpuprofile`, '--require', probe] : []),
    ...(impl === 'ts' ? ['--import', 'tsx'] : []), 'sdk-rs/harness/replay.mjs',
    '--impl', impl, '--venue', venue, '--tape', tape, '--books', 'BTCUSDT,ETHUSDT',
    '--performance', '--out', path, ...(impl === 'rust' ? ['--wasm', source.module.path] : [])];
  const result = spawnSync(process.execPath, args, {
    cwd: source.root,
    env: { ...environment, ...(profile ? { RAW_CPU_MARKERS: `${directory}/${profile}.markers.json` } : {}) },
    encoding: 'utf8', timeout: 125000,
  });
  writeFileSync(`${path}.log`, `${result.stdout ?? ''}\n${result.stderr ?? ''}`);
  if (result.status !== 0 || result.error || result.signal) throw new Error(`Diagnostic replay failed: ${path}`);
  const output = JSON.parse(readFileSync(path));
  if (performanceMetricErrors(output.metrics, 'performance').length || output.unserved || output.missingWrites ||
      output.unusedWrites || output.invalidSignatures || output.protocolErrors?.length)
    throw new Error('Diagnostic replay has invalid metrics or incomplete transport work');
  if (originals[impl] && (output.frames !== originals[impl].frames ||
      compareSnapshots(originals[impl], output, 'A/A').length ||
      !isDeepStrictEqual(originals[impl].transportReceipt, output.transportReceipt)))
    throw new Error('Before/after replay workload changed');
  originals[impl] ??= output;
  return output.metrics;
}

for (const [block, arm] of order.entries()) for (let pair = 0; pair < 4; pair++) {
  for (const impl of performanceOrder(pair)) {
    const index = samples[arm][impl].length, path = `${directory}/${arm}-${impl}-${index}.json`;
    const before = observeCpu();
    samples[arm][impl].push(replay(arm, impl, path));
    rows.push({ block, arm, impl, pair: index, before, after: observeCpu(), path });
    writeFileSync(`${directory}/progress.json`, JSON.stringify({ diagnosticOnly: true, seals, rows, samples }, null, 2));
  }
}
if (sha(process.execPath) !== seals.node || sha(tape) !== seals.tape ||
    sha(module.path) !== seals.module.hashes['safe_cex_wasm.js'] ||
    sha(resolve(module.path, '..', 'safe_cex_wasm_bg.wasm')) !== seals.module.hashes['safe_cex_wasm_bg.wasm'] ||
    sha(baseline.path) !== seals.baseline.hashes['safe_cex_wasm.js'] ||
    sha(resolve(baseline.path, '..', 'safe_cex_wasm_bg.wasm')) !== seals.baseline.hashes['safe_cex_wasm_bg.wasm'])
  throw new Error('Before/after replay inputs changed');
const analysis = Object.fromEntries(['A', 'B'].map(arm => [arm, {
  medians: Object.fromEntries(['cpuMs', 'steadyCpuMs', 'processCpuMs'].map(field => [field,
    Object.fromEntries(['ts', 'rust'].map(impl => [impl, median(samples[arm][impl].map(row => row[field]))]))])),
  verdict: performanceVerdict(samples[arm].ts, samples[arm].rust),
}]));
writeFileSync(`${directory}/analysis.json`, JSON.stringify({ diagnosticOnly: true, venue, seals, order, rows, samples, analysis }, null, 2));
console.log(JSON.stringify({ diagnosticOnly: true, venue, analysis }));
for (const impl of ['ts', 'rust']) for (let trial = 0; trial < 8; trial++)
  replay('B', impl, `${directory}/profile-${impl}-${trial}.json`, `profile-${impl}-${trial}`);
