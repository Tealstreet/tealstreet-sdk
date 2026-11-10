// Temporary CI tiering diagnostic; none of these arms certifies a CPU gate.
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { parseArgs, isDeepStrictEqual } from 'node:util';
import { validateCiWasm } from './ci-wasm.mjs';
import { observeCpu } from './cpu-observation.mjs';
import { median, performanceMetricErrors, performanceOrder, performanceVerdict } from './performance-policy.mjs';
import { compareSnapshots } from './snapshot-compare.mjs';

const { values } = parseArgs({ options: { suite: { type: 'string' }, out: { type: 'string' } } });
if (process.env.GITHUB_ACTIONS !== 'true' || process.env.GITHUB_RUN_ATTEMPT !== '1' ||
    !/^\d+$/.test(process.env.GITHUB_RUN_ID ?? '') || values.suite !== 'bybit' || !values.out)
  throw new Error('Original CI attempt and explicit Bybit suite/output required');
const venue = 'hyrotrader', tape = `sdk-rs/harness/fixtures/${venue}-public.jsonl.gz`;
const directory = resolve(values.out);
mkdirSync(directory, { recursive: false });
const module = validateCiWasm({ venue, directory: 'sdk-rs/hosts/wasm/pkg/ci-replay' });
const sha = path => createHash('sha256').update(readFileSync(path)).digest('hex');
const seals = { node: sha(process.execPath), tape: sha(tape), module: module.metadata };
const flags = { A: [], B: ['--no-liftoff'], C: ['--liftoff-only'] };
const samples = Object.fromEntries(Object.keys(flags).map(arm => [arm, { ts: [], rust: [] }]));
const originals = {}, rows = [], order = ['A', 'B', 'C', 'C', 'B', 'A', 'C', 'A', 'B', 'B', 'A', 'C'];
const environment = { ...process.env, ESBUILD_WORKER_THREADS: '0', TMPDIR: `${directory}/tmp` };
mkdirSync(environment.TMPDIR);
const probe = `${directory}/tier-markers.cjs`;
writeFileSync(probe, `
if (require('node:worker_threads').isMainThread) {
const { writeFileSync } = require('node:fs');
const original = process.threadCpuUsage.bind(process);
const seen = new WeakMap(), markers = [];
process.threadCpuUsage = previous => {
  const at = Number(process.hrtime.bigint() / 1000n);
  const value = original(previous);
  markers.push({ at, call: markers.length + 1, previousCall: seen.get(previous) ?? null, ...value });
  seen.set(value, markers.length);
  return value;
};
process.once('exit', () => writeFileSync(process.env.RAW_CPU_MARKERS, JSON.stringify(markers)));
}
`);

function replay(arm, impl, path) {
  const args = ['--expose-gc', ...flags[arm], '--require', probe,
    ...(impl === 'ts' ? ['--import', 'tsx'] : []), 'sdk-rs/harness/replay.mjs',
    '--impl', impl, '--venue', venue, '--tape', tape, '--books', 'BTCUSDT,ETHUSDT',
    '--performance', '--out', path, ...(impl === 'rust' ? ['--wasm', module.path] : [])];
  const markersPath = `${path}.markers.json`;
  const result = spawnSync(process.execPath, args, {
    env: { ...environment, RAW_CPU_MARKERS: markersPath },
    encoding: 'utf8', timeout: 125000,
  });
  writeFileSync(`${path}.log`, `${result.stdout ?? ''}\n${result.stderr ?? ''}`);
  if (result.status !== 0 || result.error || result.signal) throw new Error(`Tiering diagnostic replay failed: ${path}`);
  const output = JSON.parse(readFileSync(path)), markers = JSON.parse(readFileSync(markersPath));
  if (markers.length !== 6 || markers[5].previousCall !== 4 ||
      Math.abs((markers[5].user + markers[5].system) / 1000 - output.metrics.steadyCpuMs) > 1e-8)
    throw new Error('Tiering diagnostic did not align with the exact measured steady boundary');
  if (performanceMetricErrors(output.metrics, 'performance').length || output.unserved || output.missingWrites ||
      output.unusedWrites || output.invalidSignatures || output.protocolErrors?.length)
    throw new Error('Method replay has invalid metrics or incomplete transport work');
  if (originals[impl] && (output.frames !== originals[impl].frames ||
      compareSnapshots(originals[impl], output, 'tiering-control').length ||
      !isDeepStrictEqual(originals[impl].transportReceipt, output.transportReceipt)))
    throw new Error('Tiering diagnostic changed the workload');
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
    sha(resolve(module.path, '..', 'safe_cex_wasm_bg.wasm')) !== seals.module.hashes['safe_cex_wasm_bg.wasm'])
  throw new Error('Tiering diagnostic inputs changed');
const analysis = Object.fromEntries(Object.keys(flags).map(arm => [arm, {
  medians: Object.fromEntries(['cpuMs', 'steadyCpuMs', 'processCpuMs'].map(field => [field,
    Object.fromEntries(['ts', 'rust'].map(impl => [impl, median(samples[arm][impl].map(row => row[field]))]))])),
  verdict: performanceVerdict(samples[arm].ts, samples[arm].rust),
}]));
writeFileSync(`${directory}/analysis.json`, JSON.stringify({ diagnosticOnly: true,
  methodExperiment: 'A uses default V8, B disables Liftoff, C uses Liftoff only; exact same production module, quarter-tape boundary and workload in all arms',
  flags,
  venue, seals, order, rows, samples, analysis }, null, 2));
console.log(JSON.stringify({ diagnosticOnly: true, venue, analysis }));
