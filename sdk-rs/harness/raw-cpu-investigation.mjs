// Temporary CI-only A/A evidence. Profiling and these trials never certify a gate.
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
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

function replay(impl, path, profile) {
  const args = ['--expose-gc', ...(profile ? ['--cpu-prof', '--cpu-prof-interval=100',
    `--cpu-prof-dir=${directory}`, `--cpu-prof-name=${profile}.cpuprofile`, '--require', probe] : []),
    ...(impl === 'ts' ? ['--import', 'tsx'] : []), 'sdk-rs/harness/replay.mjs',
    '--impl', impl, '--venue', venue, '--tape', tape, '--books', 'BTCUSDT,ETHUSDT',
    '--performance', '--out', path, ...(impl === 'rust' ? ['--wasm', module.path] : [])];
  const result = spawnSync(process.execPath, args, {
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
for (const impl of ['ts', 'rust']) for (let trial = 0; trial < 8; trial++)
  replay(impl, `${directory}/profile-${impl}-${trial}.json`, `profile-${impl}-${trial}`);
// Keep production bytes in every replay. Names are usable only if their
// independently bound module has identical non-custom sections.
const named = `${directory}/named`;
mkdirSync(named);
const raw = resolve(process.env.CARGO_TARGET_DIR || 'sdk-rs/target', 'wasm32-unknown-unknown/release/safe_cex_wasm.wasm');
if (!existsSync(raw)) throw new Error('Original compiler artifact is unavailable for named profiling');
function tool(command, args) {
  const result = spawnSync(command, args, { encoding: 'utf8', maxBuffer: 16 * 1024 * 1024 });
  if (result.status !== 0 || result.error || result.signal) throw new Error(`${command}: ${result.stderr}`);
  return result.stdout;
}
tool('wasm-bindgen', ['--target', 'nodejs', '--remove-producers-section', '--out-name', 'safe_cex_wasm', '--out-dir', named, raw]);
const namedWasm = `${named}/safe_cex_wasm_bg.wasm`;
for (let pass = 0; pass < 2; pass++) tool('wasm-opt', ['-Oz', '-g', '--enable-bulk-memory', '--enable-bulk-memory-opt',
  '--enable-mutable-globals', '--enable-sign-ext', '--enable-nontrapping-float-to-int', '--enable-reference-types',
  '--enable-multivalue', namedWasm, '-o', namedWasm]);
const codeHash = path => {
  const bytes = readFileSync(path), sections = [bytes.subarray(0, 8)];
  let cursor = 8;
  while (cursor < bytes.length) {
    const begin = cursor, id = bytes[cursor++];
    let size = 0, shift = 0, byte;
    do { byte = bytes[cursor++]; size += (byte & 127) * 2 ** shift; shift += 7; } while (byte & 128);
    cursor += size;
    if (id !== 0) sections.push(bytes.subarray(begin, cursor));
  }
  return createHash('sha256').update(Buffer.concat(sections)).digest('hex');
};
const productionCode = codeHash(resolve(module.path, '..', 'safe_cex_wasm_bg.wasm'));
if (codeHash(namedWasm) !== productionCode) throw new Error('Named WASM code differs; refuse an unverified function map');
writeFileSync(`${directory}/function-map.txt`, tool('wasm-opt', ['--print-function-map', namedWasm, '-o', `${named}/mapped.wasm`]));
writeFileSync(`${directory}/function-map-proof.json`, JSON.stringify({ productionCode, productionWasm: seals.module.hashes['safe_cex_wasm_bg.wasm'], namedWasm: sha(namedWasm) }));
