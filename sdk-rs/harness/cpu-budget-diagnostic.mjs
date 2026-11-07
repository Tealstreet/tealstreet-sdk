// Non-certifying A/A experiment. Both arms execute identical source and WASM.
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { isDeepStrictEqual, parseArgs } from 'node:util';
import { observeCpu } from './cpu-observation.mjs';
import { median, performanceFailures, performanceMetricErrors, performanceOrder } from './performance-policy.mjs';
import { compareSnapshots } from './snapshot-compare.mjs';

export const AA_ORDER = Object.freeze(['A', 'B', 'B', 'A', 'A', 'B', 'B', 'A']);
export function budgetComparison(samples, budget) {
  if (![12, 16].includes(budget) || ['A', 'B'].some((arm) => ['ts', 'rust'].some((impl) => samples[arm][impl].length !== 16)))
    throw new Error('Complete fixed sixteen-pair arms required; prefixes are diagnostic only');
  const summaries = Object.fromEntries(['A', 'B'].map((arm) => [arm, Object.fromEntries(['ts', 'rust']
    .map((impl) => [impl, median(samples[arm][impl].slice(0, budget).map((value) => value.steadyCpuMs))]))]));
  const ratio = (summaries.B.rust / summaries.B.ts) / (summaries.A.rust / summaries.A.ts);
  return { budget, summaries, relativeRatio: ratio, stableWithinFivePercent: ratio < 1.05 && 1 / ratio < 1.05 };
}

function main() {
  const { values } = parseArgs({ options: { module: { type: 'string' }, out: { type: 'string' }, profile: { type: 'boolean', default: false } } });
  if (!values.module || !values.out || process.env.GITHUB_ACTIONS !== 'true' || process.env.GITHUB_RUN_ATTEMPT !== '1')
    throw new Error('Explicit module/output and an original CI attempt required');
  const directory = resolve(values.out); mkdirSync(directory, { recursive: true });
  const sha = (path) => createHash('sha256').update(readFileSync(path)).digest('hex');
  const module = resolve(values.module), wasm = module.replace(/\.js$/, '_bg.wasm');
  const hashes = { node: sha(process.execPath), glue: sha(module), wasm: sha(wasm), tape: sha('sdk-rs/harness/fixtures/txflow-public.jsonl.gz') };
  if (hashes.wasm !== 'b41482547cccd22a54e44abb91770aa3f1c6be5fa49ab5afa968ba720374bf27') throw new Error('Unexpected diagnostic WASM bytes');
  const samples = { A: { ts: [], rust: [] }, B: { ts: [], rust: [] } };
  const rows = [], originals = {};
  const start = performance.now();
  for (const [block, arm] of AA_ORDER.entries()) for (let pair = 0; pair < 4; pair++) for (const impl of performanceOrder(pair)) {
    const index = samples[arm][impl].length;
    const path = `${directory}/${arm}-${impl}-${index}.json`;
    const args = ['--expose-gc', ...(impl === 'ts' ? ['--import', 'tsx'] : []), 'sdk-rs/harness/replay.mjs',
      '--impl', impl, '--venue', 'txflow', '--tape', 'sdk-rs/harness/fixtures/txflow-public.jsonl.gz', '--books', 'BTC-USDC,ETH-USDC', '--performance', '--out', path,
      ...(impl === 'rust' ? ['--wasm', module] : [])];
    const before = observeCpu(), began = performance.now();
    const result = spawnSync(process.execPath, args, { encoding: 'utf8', timeout: 125000, env: { ...process.env, ESBUILD_WORKER_THREADS: '0' } });
    writeFileSync(`${path}.log`, `${result.stdout ?? ''}\n${result.stderr ?? ''}`);
    if (result.status !== 0) throw new Error(`Diagnostic child failed: ${arm}/${impl}/${index}; see ${path}.log`);
    const output = JSON.parse(readFileSync(path));
    if (performanceMetricErrors(output.metrics, 'performance').length || output.frames !== 527 || output.unserved || output.missingWrites || output.unusedWrites || output.invalidSignatures || output.protocolErrors?.length)
      throw new Error('Incomplete diagnostic replay');
    if (originals[impl] && (compareSnapshots(originals[impl], output, 'A/A').length || !isDeepStrictEqual(originals[impl].transportReceipt, output.transportReceipt)))
      throw new Error('A/A workload changed');
    originals[impl] ??= output;
    rows.push({ block, arm, pair: index, impl, before, after: observeCpu(), elapsedMs: performance.now() - began, path });
    samples[arm][impl].push(output.metrics);
    writeFileSync(`${directory}/progress.json`, JSON.stringify({ diagnosticOnly: true, hashes, rows, samples }, null, 2));
  }
  if (sha(module) !== hashes.glue || sha(wasm) !== hashes.wasm) throw new Error('Module changed during A/A');
  const analysis = [12, 16].map((budget) => ({ ...budgetComparison(samples, budget),
    originalFailures: Object.fromEntries(['A', 'B'].map((arm) => [arm, performanceFailures(samples[arm].ts.slice(0, budget), samples[arm].rust.slice(0, budget))])) }));
  writeFileSync(`${directory}/diagnostic.json`, JSON.stringify({ diagnosticOnly: true, order: AA_ORDER, hashes, cpu: observeCpu(), rows, samples, analysis, elapsedMs: performance.now() - start }, null, 2));
  if (values.profile) for (const impl of ['ts', 'rust']) {
    const args = ['--expose-gc', '--trace-gc-nvp', '--trace-opt', '--trace-deopt', ...(impl === 'ts' ? ['--import', 'tsx'] : []),
      'sdk-rs/harness/replay.mjs', '--impl', impl, '--venue', 'txflow', '--tape', 'sdk-rs/harness/fixtures/txflow-public.jsonl.gz',
      '--books', 'BTC-USDC,ETH-USDC', '--performance', '--out', `${directory}/profile-${impl}.json`, ...(impl === 'rust' ? ['--wasm', module] : [])];
    const result = spawnSync(process.execPath, args, { encoding: 'utf8', timeout: 125000, env: { ...process.env, ESBUILD_WORKER_THREADS: '0' } });
    writeFileSync(`${directory}/profile-${impl}.log`, `${result.stdout ?? ''}\n${result.stderr ?? ''}`);
    if (result.status !== 0) throw new Error('Profiling child failed');
  }
  console.log(JSON.stringify({ diagnosticOnly: true, analysis, elapsedMs: performance.now() - start }));
}
if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) main();
