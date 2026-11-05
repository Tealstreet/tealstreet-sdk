// One-off reset diagnostic: raw Rust CPU only; never certification evidence.
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { cpSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { isDeepStrictEqual, parseArgs } from 'node:util';
import { observeCpu } from './cpu-observation.mjs';
import { BITGET_PAIRED_TOLERANCE } from './bitget-paired-steady.mjs';

export const OLD_ANCHOR = '518d3953c09daf63c1345ea465bbc55bd313f53a';
export const NEW_ANCHOR = '38b69912e461e27198f5252c98cef73b7521b169';
export const DRIFT_ORDER = Object.freeze(['old', 'new', 'new', 'old', 'new', 'old', 'old', 'new']);
const insist = (ok, message) => { if (!ok) throw new Error(`Anchor drift: ${message}`); };
const median = values => { const sorted = [...values].sort((a, b) => a - b);
  return (sorted[(sorted.length - 1) >> 1] + sorted[sorted.length >> 1]) / 2; };
const hash = path => createHash('sha256').update(readFileSync(path)).digest('hex');
const write = (path, value) => writeFileSync(path, JSON.stringify(value, null, 2) + '\n');

export function assessAnchorDrift(rows) {
  insist(rows.length === DRIFT_ORDER.length, 'eight complete cohorts required');
  const pooled = { old: [], new: [] };
  rows.forEach((row, index) => {
    insist(row.arm === DRIFT_ORDER[index] && row.samples.length === 16 &&
      row.samples.every(x => Number.isFinite(x) && x > 0), 'invalid fixed cohort');
    pooled[row.arm].push(...row.samples);
  });
  const oldMedianMs = median(pooled.old), newMedianMs = median(pooled.new);
  const ratio = newMedianMs / oldMedianMs;
  return { oldMedianMs, newMedianMs, ratio, driftPercent: 100 * (ratio - 1),
    tolerance: BITGET_PAIRED_TOLERANCE, withinTolerance: ratio <= BITGET_PAIRED_TOLERANCE,
    certified: false, correctness: 'known-changed', samplesPerArm: 64 };
}

function main() {
  const { values } = parseArgs({ options: { packages: { type: 'string' }, out: { type: 'string' } }, strict: true });
  insist(process.env.GITHUB_ACTIONS === 'true' && process.env.GITHUB_RUN_ATTEMPT === '1', 'original CI attempt required');
  insist(values.packages && values.out, 'package and output paths required');
  const packages = resolve(values.packages), directory = resolve(values.out);
  mkdirSync(directory, { recursive: false });
  const tape = 'sdk-rs/harness/fixtures/bitget-public.jsonl.gz';
  const seals = { node: hash(process.execPath), tape: hash(tape), script: hash(fileURLToPath(import.meta.url)) };
  const initialCpu = observeCpu(), rows = [], semantics = {};
  const sources = { old: OLD_ANCHOR, new: NEW_ANCHOR };
  const moduleSeals = Object.fromEntries(Object.keys(sources).map(arm => [arm,
    Object.fromEntries(['safe_cex_wasm.js', 'safe_cex_wasm_bg.wasm'].map(name => [name, hash(`${packages}/${arm}/${name}`)]))]));
  for (const [arm, source] of Object.entries(sources)) {
    const metadata = JSON.parse(readFileSync(`${packages}/${arm}/build.json`));
    insist(metadata.sourceHead === source && String(metadata.runId) === process.env.GITHUB_RUN_ID &&
      Object.entries(moduleSeals[arm]).every(([name, digest]) => metadata.hashes[name] === digest),
    'package source, run or module seals differ');
  }
  const active = `${directory}/active-module`;
  write(`${directory}/plan.json`, { diagnosticOnly: true, certified: false, correctness: 'known-changed',
    runId: process.env.GITHUB_RUN_ID, sourceHead: process.env.GITHUB_SHA,
    sources, order: DRIFT_ORDER, pairsPerCohort: 16, seals, moduleSeals, initialCpu,
    imageOS: process.env.ImageOS, imageVersion: process.env.ImageVersion });
  for (const [cohort, arm] of DRIFT_ORDER.entries()) {
    rmSync(active, { recursive: true, force: true });
    cpSync(`${packages}/${arm}`, active, { recursive: true });
    const samples = [];
    for (let sample = 0; sample < 16; sample++) {
      const path = `${directory}/${cohort}-${arm}-${sample}.json`;
      const before = observeCpu(), started = Date.now();
      const result = spawnSync(process.execPath, ['--expose-gc', 'sdk-rs/harness/replay.mjs', '--impl', 'rust',
        '--venue', 'bitget', '--tape', tape, '--books', 'BTCUSDT,ETHUSDT', '--performance',
        '--wasm', `${active}/safe_cex_wasm.js`, '--out', path],
      { encoding: 'utf8', timeout: 125000, env: { ...process.env, ESBUILD_WORKER_THREADS: '0' } });
      writeFileSync(`${path}.log`, `${result.stdout ?? ''}\n${result.stderr ?? ''}`);
      insist(result.status === 0, `replay failed: ${arm}/${cohort}/${sample}; ` +
        `${result.error?.message ?? `exit ${result.status}, signal ${result.signal}`}`);
      const value = JSON.parse(readFileSync(path));
      insist(value.frames > 0 && value.unserved === 0 && value.missingWrites === 0 &&
        value.unusedWrites === 0 && value.invalidSignatures === 0 && value.protocolErrors?.length === 0,
      'incomplete replay is not accepted as known-changed correctness');
      const cpu = value.metrics?.steadyCpuMs;
      insist(Number.isFinite(cpu) && cpu > 0, 'invalid Rust CPU measurement');
      const after = observeCpu();
      insist(isDeepStrictEqual(before.models, initialCpu.models) && isDeepStrictEqual(after.models, initialCpu.models), 'CPU model changed');
      delete value.metrics;
      if (semantics[arm]) insist(isDeepStrictEqual(value, semantics[arm]), 'same-arm workload changed');
      else semantics[arm] = value;
      samples.push(cpu);
      write(`${path}.receipt.json`, { cohort, arm, sample, before, after, started, finished: Date.now(), cpu });
    }
    rows.push({ arm, samples });
    write(`${directory}/progress.json`, rows);
    for (const [name, digest] of Object.entries(moduleSeals[arm])) insist(hash(`${active}/${name}`) === digest, 'module changed');
    insist(hash(process.execPath) === seals.node && hash(tape) === seals.tape, 'runtime or tape changed');
  }
  const comparison = assessAnchorDrift(rows);
  write(`${directory}/comparison.json`, comparison);
  console.log(JSON.stringify(comparison));
  insist(comparison.withinTolerance, 'drift exceeds tolerance: stop, do not reset anchor');
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) main();
