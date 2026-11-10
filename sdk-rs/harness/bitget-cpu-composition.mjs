// CI-only diagnostic composition; this cannot issue a certification.
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { cpSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { basename, resolve } from 'node:path';
import { parseArgs } from 'node:util';
import { BITGET_COHORT_ORDER, BITGET_CUMULATIVE_ANCHOR, BITGET_MEASUREMENT_FILES } from './bitget-paired-steady.mjs';
import { PUBLIC_REPLAY_PAIRS, median } from './performance-policy.mjs';

const { values } = parseArgs({ options: { out: { type: 'string' } } });
if (process.env.GITHUB_ACTIONS !== 'true' || process.env.GITHUB_RUN_ATTEMPT !== '1' || !values.out)
  throw new Error('Composition replay is original CI attempt1 only; never run on a development host');
const out = resolve(values.out);
mkdirSync(out, { recursive: false });
const write = (name, value) => writeFileSync(`${out}/${name}`, JSON.stringify(value, null, 2));
function tool(command, args, log, allowed = [0], env = process.env) {
  const child = spawnSync(command, args, { encoding: 'utf8', timeout: 900000, maxBuffer: 32 * 1024 * 1024, env });
  if (log) writeFileSync(`${out}/${log}`, `${child.stdout ?? ''}${child.stderr ?? ''}`);
  if (child.error || child.signal || !allowed.includes(child.status))
    throw new Error(`${command} ${args[0]} failed (${child.status}); see ${log ?? child.stderr}`);
  return child.stdout.trim();
}
const git = (...args) => tool('git', args);
const original = git('rev-parse', 'HEAD');
const prior = '49a782a20c0b3838a9a87098fa9796464dd16060';
if (git('status', '--porcelain')) throw new Error('Dirty original source');
git('fetch', '--no-tags', 'origin', prior);
const tree = git('merge-tree', '--write-tree', original, prior);
if (!/^[a-f0-9]{40}$/.test(tree)) throw new Error('Composition must merge without resolution');
const composition = tool('git', ['commit-tree', tree, '-p', original, '-p', prior, '-m',
  'Diagnostic-only automatic composition of Bitget CPU fix and pinned PR 3426'], null, [0], {
  ...process.env, GIT_AUTHOR_NAME: 'Bitget CPU diagnostic', GIT_AUTHOR_EMAIL: 'diagnostic@example.invalid',
  GIT_COMMITTER_NAME: 'Bitget CPU diagnostic', GIT_COMMITTER_EMAIL: 'diagnostic@example.invalid',
});
const refs = { baseline: BITGET_CUMULATIVE_ANCHOR, candidate: composition };
const sourceSeals = Object.fromEntries(BITGET_MEASUREMENT_FILES.map(path => [path, git('rev-parse', `${original}:${path}`)]));
for (const [path, seal] of Object.entries(sourceSeals))
  if (git('rev-parse', `${composition}:${path}`) !== seal) throw new Error(`Composition changes measurement source: ${path}`);
writeFileSync(`${out}/composition.patch`, tool('git', ['diff', original, composition]));
write('plan.json', { diagnosticOnly: true, certified: false, runId: process.env.GITHUB_RUN_ID,
  original, prior, composition, compositionTree: tree, refs, sourceSeals, order: BITGET_COHORT_ORDER,
  pairs: PUBLIC_REPLAY_PAIRS, cpuInfo: readFileSync('/proc/cpuinfo', 'utf8') });
try {
  const modules = {};
  for (const [arm, ref] of Object.entries(refs)) {
    git('checkout', '--detach', ref);
    tool('bash', ['sdk-rs/harness/build.sh', '--venue', 'bitget'], `${arm}-build.log`);
    tool('node', ['sdk-rs/harness/wasm-budget.mjs', '--venue', 'bitget'], `${arm}-budget.log`);
    modules[arm] = `${out}/${arm}`;
    tool('node', ['sdk-rs/harness/ci-wasm.mjs', '--venue', 'bitget', '--archive', modules[arm]], `${arm}-archive.log`);
  }
  git('checkout', '--detach', composition);
  // Restored types enable arbitrary_precision; CI keeps that feature closure
  // separate from venue tests. Run both full suites without unifying them.
  tool('cargo', ['test', '--locked', '--manifest-path', 'sdk-rs/Cargo.toml', '-p', 'safe-cex-types', '-j2'], 'composition-types-tests.log');
  tool('cargo', ['test', '--locked', '--manifest-path', 'sdk-rs/Cargo.toml', '-p', 'safe-cex-venue-bitget', '-j2'], 'composition-native-tests.log');
  const pooled = { baseline: { ts: [], rust: [] }, candidate: { ts: [], rust: [] } };
  const active = `${out}/active-module`;
  for (const [index, arm] of BITGET_COHORT_ORDER.entries()) {
    if (git('status', '--porcelain') || git('rev-parse', 'HEAD^{tree}') !== tree) throw new Error('Measurement source changed');
    rmSync(active, { recursive: true, force: true });
    cpSync(modules[arm], active, { recursive: true });
    const report = `${out}/${index + 1}-${arm}.json`;
    tool('node', ['sdk-rs/harness/gate.mjs', '--venue', 'bitget', '--tape', 'sdk-rs/harness/fixtures/bitget-public.jsonl.gz',
      '--books', 'BTCUSDT,ETHUSDT', '--runs', String(PUBLIC_REPLAY_PAIRS), '--ci-wasm', active,
      '--paired-source', refs[arm], '--report', report], `${index + 1}-${arm}.log`, [0, 1]);
    const result = JSON.parse(readFileSync(report));
    if (result.runs !== PUBLIC_REPLAY_PAIRS || result.correctnessRuns !== 1 ||
      result.failures.some(failure => !failure.startsWith('performance '))) throw new Error('Incomplete or incorrect replay');
    const artifacts = `${out}/cohort-${index + 1}-raw`;
    mkdirSync(artifacts);
    for (const artifact of result.artifacts) cpSync(artifact.path, `${artifacts}/${basename(artifact.path)}`);
    for (const implementation of ['ts', 'rust'])
      for (const sample of result.samples[implementation]) {
        if (!Number.isFinite(sample.steadyCpuMs) || sample.steadyCpuMs <= 0) throw new Error('Invalid steady sample');
        pooled[arm][implementation].push(sample.steadyCpuMs);
      }
    write(`cohort-${index + 1}-proof.json`, { arm, ref: refs[arm], composition, tree,
      reportSha256: createHash('sha256').update(readFileSync(report)).digest('hex'),
      module: JSON.parse(readFileSync(`${active}/build.json`)) });
  }
  const medians = Object.fromEntries(Object.entries(pooled).map(([arm, samples]) => [arm,
    { ts: median(samples.ts), rust: median(samples.rust), ratio: median(samples.rust) / median(samples.ts) }]));
  const result = { diagnosticOnly: true, certified: false, original, prior, composition, tree, medians,
    anchorQ: medians.candidate.ratio / medians.baseline.ratio,
    note: 'Automatic synthetic composition, not an authenticated PR candidate. This issues no certificate; PR 3426 must run its own unchanged certification after incorporating the fix.' };
  write('summary.json', result);
  console.log(JSON.stringify(result));
} finally {
  git('checkout', '--detach', original);
}
