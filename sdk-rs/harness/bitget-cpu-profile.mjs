// Diagnostic CI profiles only. These samples never certify or alter a CPU gate.
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { parseArgs } from 'node:util';
import { validateSameRunCiWasm } from './ci-wasm.mjs';

const { values } = parseArgs({ options: { out: { type: 'string' } } });
if (process.env.GITHUB_ACTIONS !== 'true' || process.env.GITHUB_RUN_ATTEMPT !== '1' ||
    !/^\d+$/.test(process.env.GITHUB_RUN_ID ?? '') || !values.out)
  throw new Error('Bitget profiling requires original CI attempt1; never run on a development host');
const out = resolve(values.out);
mkdirSync(out, { recursive: false });
const sha = path => createHash('sha256').update(readFileSync(path)).digest('hex');
function tool(command, args, options = {}) {
  const result = spawnSync(command, args, { encoding: 'utf8', timeout: 240000,
    maxBuffer: 32 * 1024 * 1024, ...options });
  if (result.status !== 0 || result.error || result.signal)
    throw new Error(`${command} failed: ${result.stderr ?? result.error ?? result.signal}`);
  return result.stdout;
}
const current = validateSameRunCiWasm({ venue: 'bitget', directory: 'sdk-rs/hosts/wasm/pkg/ci-replay' });
const old = `${out}/prior`;
tool('gh', ['run', 'download', '42776495878', '-n', 'bitget-paired-steady', '-D', old]);
const certificate = JSON.parse(readFileSync(`${old}/steady-assessment.json`)).certificate;
if (certificate.authority.head !== '49a782a20c0b3838a9a87098fa9796464dd16060' ||
    certificate.authority.runId !== '42776495878') throw new Error('Wrong pinned prior certification');
const labels = {
  current: { directory: resolve(current.path, '..'), raw: resolve(process.env.CARGO_TARGET_DIR || 'sdk-rs/target', 'wasm32-unknown-unknown/release/safe_cex_wasm.wasm') },
  priorMaster: { directory: `${old}/tip/records/baseline/node`, raw: `${old}/tip/records/baseline/raw.wasm` },
  pr3426: { directory: `${old}/tip/records/candidate/node`, raw: `${old}/tip/records/candidate/raw.wasm` },
};
const seals = {};
for (const [label, files] of Object.entries(labels)) {
  const metadata = JSON.parse(readFileSync(`${files.directory}/build.json`));
  for (const [name, hash] of Object.entries(metadata.hashes)) {
    if (sha(`${files.directory}/${name}`) !== hash) throw new Error(`Wrong ${label} archive bytes`);
  }
  if (label !== 'current' && metadata.sourceHead !== certificate.authority[label === 'pr3426' ? 'candidate' : 'baseline'])
    throw new Error(`Wrong ${label} source`);
  seals[label] = { metadata, raw: sha(files.raw) };
}
const probe = `${out}/markers.cjs`;
writeFileSync(probe, `
if (require('node:worker_threads').isMainThread) {
const { writeFileSync } = require('node:fs');
const original = process.threadCpuUsage.bind(process), seen = new WeakMap(), markers = [];
process.threadCpuUsage = previous => {
  const at = Number(process.hrtime.bigint() / 1000n), value = original(previous), call = markers.length + 1;
  markers.push({ at, call, previousCall: seen.get(previous) ?? null, ...value });
  seen.set(value, call); return value;
};
process.once('exit', () => writeFileSync(process.env.BITGET_PROFILE_MARKERS, JSON.stringify(markers)));
}
`);
const tape = 'sdk-rs/harness/fixtures/bitget-public.jsonl.gz';
const trials = [];
for (let trial = 0; trial < 4; trial++) for (const label of ['current', 'pr3426', 'priorMaster']) {
  const prefix = `${out}/${label}-${trial}`;
  const args = ['--expose-gc', '--cpu-prof', '--cpu-prof-interval=100', `--cpu-prof-dir=${out}`,
    `--cpu-prof-name=${label}-${trial}.cpuprofile`, '--require', probe, 'sdk-rs/harness/replay.mjs',
    '--impl', 'rust', '--venue', 'bitget', '--tape', tape, '--books', 'BTCUSDT,ETHUSDT', '--performance',
    '--wasm', `${labels[label].directory}/safe_cex_wasm.js`, '--out', `${prefix}.json`];
  const log = tool(process.execPath, args, { env: { ...process.env, ESBUILD_WORKER_THREADS: '0',
    BITGET_PROFILE_MARKERS: `${prefix}.markers.json` } });
  writeFileSync(`${prefix}.log`, log);
  const result = JSON.parse(readFileSync(`${prefix}.json`));
  if (result.unserved || result.missingWrites || result.unusedWrites || result.invalidSignatures || result.protocolErrors?.length)
    throw new Error('Profile replay left incomplete transport work');
  trials.push({ label, trial, frames: result.frames, metrics: result.metrics, transportReceipt: result.transportReceipt });
}
// Names are advisory unless optimized non-custom sections match production exactly.
function codeHash(path) {
  const bytes = readFileSync(path), sections = [bytes.subarray(0, 8)];
  let cursor = 8;
  while (cursor < bytes.length) {
    const start = cursor, id = bytes[cursor++];
    let size = 0, shift = 0, byte;
    do { byte = bytes[cursor++]; size += (byte & 127) * 2 ** shift; shift += 7; } while (byte & 128);
    cursor += size;
    if (id !== 0) sections.push(bytes.subarray(start, cursor));
  }
  return createHash('sha256').update(Buffer.concat(sections)).digest('hex');
}
for (const [label, files] of Object.entries(labels)) {
  const named = `${out}/${label}-named`; mkdirSync(named);
  tool('wasm-bindgen', ['--target', 'nodejs', '--remove-producers-section', '--out-name', 'safe_cex_wasm', '--out-dir', named, files.raw]);
  const path = `${named}/safe_cex_wasm_bg.wasm`;
  for (let pass = 0; pass < 2; pass++) tool('wasm-opt', ['-Oz', '-g', '--enable-bulk-memory', '--enable-bulk-memory-opt',
    '--enable-mutable-globals', '--enable-sign-ext', '--enable-nontrapping-float-to-int', '--enable-reference-types',
    '--enable-multivalue', path, '-o', path]);
  const productionCode = codeHash(`${files.directory}/safe_cex_wasm_bg.wasm`);
  if (codeHash(path) !== productionCode) throw new Error(`${label} named code differs from production`);
  writeFileSync(`${out}/${label}.function-map.txt`, tool('wasm-opt', ['--print-function-map', path, '-o', `${named}/mapped.wasm`]));
  seals[label].functionMapCodeHash = productionCode;
}
writeFileSync(`${out}/profiles.json`, JSON.stringify({ diagnosticOnly: true, runId: process.env.GITHUB_RUN_ID,
  node: sha(process.execPath), tape: sha(tape), priorAnchorQ: certificate.anchor.q, seals, trials }, null, 2));
console.log('Retained separate diagnostic profiles for current master composition, prior master and PR #3426');
