// Temporary CI-only A/A evidence. Profiling and these trials never certify a gate.
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
    !/^\d+$/.test(process.env.GITHUB_RUN_ID ?? '') || !['asterdex', 'bybit'].includes(values.suite) || !values.out)
  throw new Error('This investigation requires an original CI attempt and an explicit supported suite/output');
const venue = values.suite === 'bybit' ? 'hyrotrader' : 'asterdex';
const tape = `sdk-rs/harness/fixtures/${venue}-public.jsonl.gz`;
const directory = resolve(values.out);
mkdirSync(directory, { recursive: false });
const module = validateCiWasm({ venue, directory: 'sdk-rs/hosts/wasm/pkg/ci-replay' });
const sha = path => createHash('sha256').update(readFileSync(path)).digest('hex');
const seals = { node: sha(process.execPath), tape: sha(tape), module: module.metadata };
const samples = { A: { ts: [], rust: [] }, B: { ts: [], rust: [] } };
const originals = {}, rows = [];
const order = ['A', 'B', 'B', 'A', 'A', 'B', 'B', 'A'];
const environment = { ...process.env, ESBUILD_WORKER_THREADS: '0', TMPDIR: `${directory}/tmp` };
mkdirSync(environment.TMPDIR);

function replay(impl, path, profile) {
  const args = ['--expose-gc', ...(profile ? ['--cpu-prof', '--cpu-prof-interval=100',
    `--cpu-prof-dir=${directory}`, `--cpu-prof-name=${profile}.cpuprofile`] : []),
    ...(impl === 'ts' ? ['--import', 'tsx'] : []), 'sdk-rs/harness/replay.mjs',
    '--impl', impl, '--venue', venue, '--tape', tape, '--books', 'BTCUSDT,ETHUSDT',
    '--performance', '--out', path, ...(impl === 'rust' ? ['--wasm', module.path] : [])];
  const result = spawnSync(process.execPath, args, { env: environment, encoding: 'utf8', timeout: 125000 });
  writeFileSync(`${path}.log`, `${result.stdout ?? ''}\n${result.stderr ?? ''}`);
  if (result.status !== 0 || result.error || result.signal) throw new Error(`Diagnostic replay failed: ${path}`);
  const output = JSON.parse(readFileSync(path));
  if (performanceMetricErrors(output.metrics, 'performance').length || output.unserved || output.missingWrites ||
      output.unusedWrites || output.invalidSignatures || output.protocolErrors?.length)
    throw new Error('Diagnostic replay has invalid metrics or incomplete transport work');
  if (originals[impl] && (output.frames !== originals[impl].frames ||
      compareSnapshots(originals[impl], output, 'A/A').length ||
      !isDeepStrictEqual(originals[impl].transportReceipt, output.transportReceipt)))
    throw new Error('A/A replay workload changed');
  originals[impl] ??= output;
  return output.metrics;
}

for (const [block, arm] of order.entries()) for (let pair = 0; pair < 4; pair++) {
  for (const impl of performanceOrder(pair)) {
    const index = samples[arm][impl].length, path = `${directory}/${arm}-${impl}-${index}.json`;
    const before = observeCpu();
    samples[arm][impl].push(replay(impl, path));
    rows.push({ block, arm, impl, pair: index, before, after: observeCpu(), path });
    writeFileSync(`${directory}/progress.json`, JSON.stringify({ diagnosticOnly: true, seals, rows, samples }, null, 2));
  }
}
if (sha(process.execPath) !== seals.node || sha(tape) !== seals.tape ||
    sha(module.path) !== seals.module.hashes['safe_cex_wasm.js'] ||
    sha(resolve(module.path, '..', 'safe_cex_wasm_bg.wasm')) !== seals.module.hashes['safe_cex_wasm_bg.wasm'])
  throw new Error('A/A replay inputs changed');
const analysis = Object.fromEntries(['A', 'B'].map(arm => [arm, {
  medians: Object.fromEntries(['cpuMs', 'steadyCpuMs', 'processCpuMs'].map(field => [field,
    Object.fromEntries(['ts', 'rust'].map(impl => [impl, median(samples[arm][impl].map(row => row[field]))]))])),
  verdict: performanceVerdict(samples[arm].ts, samples[arm].rust),
}]));
writeFileSync(`${directory}/analysis.json`, JSON.stringify({ diagnosticOnly: true, venue, seals, order, rows, samples, analysis }, null, 2));
console.log(JSON.stringify({ diagnosticOnly: true, venue, analysis }));
for (const impl of ['ts', 'rust']) for (let trial = 0; trial < 3; trial++)
  replay(impl, `${directory}/profile-${impl}-${trial}.json`, `profile-${impl}-${trial}`);
