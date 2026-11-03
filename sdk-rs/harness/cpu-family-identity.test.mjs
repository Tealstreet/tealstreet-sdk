import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import test from 'node:test';
import { familyComparisonAuthority, readFamilyIdentity } from './cpu-family-identity.mjs';
import { classifyInitialCase } from './cpu-ci.mjs';
import { decideRemeasurement } from './cpu-remeasure.mjs';
import { performanceFailures, performanceOrder } from './performance-policy.mjs';

const A = 'a'.repeat(40), B = 'b'.repeat(40), TREE = 'c'.repeat(40);
const sha = bytes => createHash('sha256').update(bytes).digest('hex');
const declarations = { schema: 1, failures: [] };
async function fixture(suite = 'txflow') {
  const run = { id: 10, run_attempt: 1, head_sha: B, head_branch: 'master', event: 'push', repository: { full_name: 'Tealstreet/tealstreet-sdk' } };
  const job = { id: 20, run_id: 10, runner_id: 30, run_attempt: 1, head_sha: B, name: `Rust / replay measurement (${suite})` };
  const options = { sleep: async () => {}, suite, runId: 10, root: '/fixture', reader: { getRun: async () => run,
    listJobs: async () => [job], getCommit: async () => ({ sha: B, tree: { sha: TREE }, parents: [{ sha: A }] }) },
    gitRead: (...args) => args[1] === 'HEAD' ? B : TREE };
  const authority = await familyComparisonAuthority(options);
  const state = { mode: 'family', suite, sourceHead: B, run, job };
  const entries = new Map([['cpu-family/proof.json', Buffer.from(JSON.stringify(authority))]]);
  const put = (arm, name, bytes) => entries.set(`cpu-family/${arm}/${name}`, Buffer.from(bytes));
  for (const arm of ['baseline', 'candidate']) {
    const bytes = { 'safe_cex_wasm_bg.wasm': Buffer.from('real independent WASM bytes'),
      'safe_cex_wasm.js': Buffer.from('real independent glue bytes'), 'package.json': Buffer.from('{"type":"commonjs"}') };
    const metadata = { schema: 1, sourceHead: authority[arm], runId: '10', family: suite, venue: suite,
      target: 'nodejs', profile: 'release', tools: { rustc: 'pinned' }, buildFlags: { wasmOptLevel: '-Oz' },
      hashes: Object.fromEntries(Object.entries(bytes).map(([name, value]) => [name, sha(value)])) };
    for (const [name, value] of Object.entries(bytes)) put(arm, name, value);
    put(arm, 'build.json', JSON.stringify(metadata));
  }
  const metadata = JSON.parse(entries.get('cpu-family/candidate/build.json'));
  const runtimeManifest = { entries: Object.entries(metadata.hashes).map(([name, sha256]) => ({ path: `sdk-rs/hosts/wasm/pkg/node/${name}`, type: 'file', sha256 })) };
  return { authority, entries, state, runtimeManifest, options, metadata };
}
const metric = (mode, cpu, retained) => ({ baselineKind: 'idle-loaded', measuredMode: mode,
  cpuMs: cpu, processCpuMs: cpu, steadyCpuMs: cpu, idleLoadCpuMs: 1, idleLoadProcessCpuMs: 1, idleLoadWallMs: 1,
  runtimeCpuMs: cpu - 1, runtimeProcessCpuMs: cpu - 1, runtimeWallMs: cpu - 1, wallMs: cpu,
  heapBeforeBytes: 100, heapUsedBytes: 100 + retained, heapRetainedBytes: retained, buffersBefore: 0,
  buffersAfter: 0, bufferRetainedBytes: 0, wasmBeforeBytes: 0, wasmBytes: 0, wasmRetainedBytes: 0, retainedBytes: retained });
function evidence(metadata, second = false) {
  const raw = [];
  const add = (impl, mode, run) => {
    const output = { impl, frames: 10, commands: [], transportReceipt: { frames: 10 }, protocolErrors: [],
      markets: [{ symbol: 'BTC' }], tickers: [{ symbol: 'BTC', last: 1 }], books: { BTC: { bids: [[1, 1]] } },
      metrics: { ...metric(mode, (impl === 'ts' ? 200 : 170) * (metadata.venue === 'okx' ? 10 : 1), impl === 'ts' ? 100 : 80),
        steadyCpuMs: metadata.venue === 'okx'
          ? second ? (impl === 'ts' ? 325.7 : 289.496) : (impl === 'ts' ? 336.515 : 312.985)
          : second ? (impl === 'ts' ? 18.836 : 20.195) : (impl === 'ts' ? 18.745 : 19.896) } };
    const bytes = JSON.stringify(output); raw.push({ impl, mode, run, bytes, sha256: sha(bytes) });
  };
  for (const impl of ['ts', 'rust']) add(impl, 'correctness', 0);
  for (let run = 0; run < 9; run++) for (const impl of performanceOrder(run)) add(impl, 'performance', run);
  const samples = Object.fromEntries(['ts', 'rust'].map(impl => [impl, raw.filter(slot => slot.impl === impl && slot.mode === 'performance').map(slot => JSON.parse(slot.bytes).metrics)]));
  const failures = performanceFailures(samples.ts, samples.rust).sort();
  const report = { schema: 1, venue: metadata.venue, tape: `${metadata.venue}-public.jsonl.gz`, tapeSha256: sha('tape'), books: 'BTC', private: false,
    runs: 9, correctnessRuns: 1, requiredImprovement: .1, status: failures.length ? 'FAIL' : 'PASS', failures, samples, fixers: [], ciWasm: metadata,
    artifacts: raw.map(({ impl, mode, run }) => ({ impl, mode, run, path: `out/${impl}-${mode}-${run}` })) };
  return { report, raw, execution: { runId: '10', attempt: 1, jobId: second ? '21' : '20', runnerId: second ? '31' : '30',
    measuredCheckoutSha: B, imageOS: 'ubuntu24', imageVersion: 'pinned', architecture: 'x64', nodeVersion: 'v22',
    nodeSha256: sha('node'), inputsSha256: sha('inputs'), policySha256: sha('policy'), runtimeSha256: sha('runtime'),
    initialCacheSha256: sha('cache'), finalCacheSha256: sha('cache'), startedAt: second ? '2026-11-03T01:02:00Z' : '2026-11-03T01:00:00Z',
    completedAt: second ? '2026-11-03T01:03:00Z' : '2026-11-03T01:01:00Z' } };
}
function changeMetadata(data, arm, change) {
  const name = `cpu-family/${arm}/build.json`, metadata = JSON.parse(data.entries.get(name));
  change(metadata); data.entries.set(name, Buffer.from(JSON.stringify(metadata)));
}

test('original txflow steady CPU values fail the frozen bound but identical sealed bytes need no remeasurement', async () => {
  const data = await fixture(), first = evidence(data.metadata), second = evidence(data.metadata, true);
  assert.equal(classifyInitialCase(first, declarations).decision, 'second');
  const measured = decideRemeasurement(first, second);
  assert.equal(measured.accepted, false);
  assert.ok(measured.errors.some(error => error.includes('two-run mean exceeds existing bound')));
  const certificate = readFamilyIdentity(data);
  assert.equal(certificate.disposition, 'unchanged bytes, not measured');
  assert.equal(certificate.measured, false);
  assert.equal(certificate.q, undefined);
  assert.equal(classifyInitialCase(first, declarations, undefined, certificate).decision, 'first');
  assert.equal(first.report.status, 'FAIL', 'strict diagnostic remains unchanged');
});

test('master 41618845764 OKX inverse miss skips remeasurement only with authenticated identical bytes', async () => {
  const data = await fixture('okx'), first = evidence(data.metadata), second = evidence(data.metadata, true);
  assert.equal(data.authority.baseline, A, 'master compares to its actual first parent');
  assert.equal(second.report.status, 'PASS', 'the second measurement alone passed in the original run');
  const result = decideRemeasurement(first, second);
  assert.equal(result.accepted, false);
  assert.deepEqual(result.errors, ['steadyCpuMs: two-run mean exceeds existing bound']);
  assert.equal(classifyInitialCase(first, declarations).decision, 'second');
  assert.equal(classifyInitialCase(first, declarations, undefined, readFamilyIdentity(data)).decision, 'first');
});

test('changed WASM or glue keeps the existing second runner and numeric bound', async () => {
  for (const name of ['safe_cex_wasm_bg.wasm', 'safe_cex_wasm.js']) {
    const data = await fixture();
    data.entries.set(`cpu-family/baseline/${name}`, Buffer.from('different bytes'));
    changeMetadata(data, 'baseline', metadata => { metadata.hashes[name] = sha('different bytes'); });
    assert.equal(readFamilyIdentity(data), undefined);
    const first = evidence(data.metadata);
    assert.equal(classifyInitialCase(first, declarations, undefined, readFamilyIdentity(data)).decision, 'second');
    assert.equal(decideRemeasurement(first, evidence(data.metadata, true)).accepted, false);
  }
});

test('family byte identity rejects false seals, foreign sources, runtime or producer, and combined builds', async () => {
  for (const change of [
    x => { x.authority = { ...x.authority }; },
    x => { x.state.mode = 'combined'; }, x => { x.state.suite = 'bitget'; },
    x => { x.state.job = { ...x.state.job, id: 21 }; }, x => { x.state.run = { ...x.state.run, id: 11 }; },
    x => { x.state.job = { ...x.state.job, runner_id: 31 }; }, x => { x.state.sourceHead = A; },
    x => { x.entries.delete('cpu-family/baseline/safe_cex_wasm.js'); },
    x => { x.entries.set('cpu-family/baseline/safe_cex_wasm_bg.wasm', Buffer.from('corruption')); },
    x => changeMetadata(x, 'baseline', m => { m.sourceHead = B; }),
    x => changeMetadata(x, 'candidate', m => { m.runId = '11'; }),
    x => changeMetadata(x, 'baseline', m => { m.family = 'bitget'; }),
    x => changeMetadata(x, 'baseline', m => { m.tools.rustc = 'other'; }),
    x => changeMetadata(x, 'baseline', m => { m.buildFlags.wasmOptLevel = '-O3'; }),
    x => { x.runtimeManifest.entries[0].sha256 = sha('other module'); },
    x => { x.runtimeManifest.entries.push(x.runtimeManifest.entries[0]); },
  ]) {
    const data = await fixture(); change(data);
    assert.throws(() => readFamilyIdentity(data));
  }
});

test('identity cannot waive correctness, memory, incomplete evidence, foreign cases or forged certificates', async () => {
  const data = await fixture(), certificate = readFamilyIdentity(data);
  for (const change of [
    x => { x.report.failures.push('snapshot mismatch'); },
    x => { for (const sample of x.report.samples.rust) sample.retainedBytes = 10000; },
    x => { x.raw.pop(); }, x => { x.report.venue = 'bitget'; },
    x => { x.execution.measuredCheckoutSha = A; },
  ]) {
    const first = evidence(data.metadata); change(first);
    assert.throws(() => classifyInitialCase(first, declarations, undefined, certificate));
  }
  assert.throws(() => classifyInitialCase(evidence(data.metadata), declarations, undefined, { ...certificate }));
});

test('family comparison source authority refuses retries and a different physical producer', async () => {
  for (const change of [x => { x.state.run.run_attempt = 2; }, x => { x.state.job.run_attempt = 2; },
    x => { x.state.job.head_sha = A; }, x => { x.state.job.name = 'Rust / replay measurement (bitget)'; }]) {
    const data = await fixture(); change(data);
    await assert.rejects(familyComparisonAuthority(data.options));
  }
});

test('the production base collector restores the exact candidate packages on success and build failure', async t => {
  const { execFileSync, spawnSync } = await import('node:child_process');
  const fs = await import('node:fs');
  const { tmpdir } = await import('node:os');
  const { resolve } = await import('node:path');
  const script = resolve('sdk-rs/harness/collect-cpu-family.py');
  const root = fs.realpathSync(fs.mkdtempSync(resolve(tmpdir(), 'cpu-family-collector-')));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  const put = (name, contents) => { const path = resolve(root, name); fs.mkdirSync(resolve(path, '..'), { recursive: true }); fs.writeFileSync(path, contents); };
  const git = (...args) => execFileSync('git', args, { cwd: root, encoding: 'utf8' }).trim();
  git('init', '-q', '-b', 'master'); git('config', 'user.name', 'Fixture'); git('config', 'user.email', 'fixture@example.invalid');
  put('.gitignore', 'sdk-rs/hosts/wasm/pkg/\nsdk-rs/harness/out/\n');
  put('sdk-rs/harness/build.sh', `set -eu
if [ "\u0024{FAIL_FAMILY_BUILD:-}" = true ]; then exit 17; fi
printf 'baseline wasm' > sdk-rs/hosts/wasm/pkg/node/safe_cex_wasm_bg.wasm
printf 'baseline web' > sdk-rs/hosts/wasm/pkg/web/safe_cex_wasm_bg.wasm
`);
  put('sdk-rs/harness/ci-wasm.mjs', `import {readFileSync,writeFileSync,mkdirSync} from 'node:fs';
import {createHash} from 'node:crypto';
import {execFileSync} from 'node:child_process';
const out=process.argv.at(-1);mkdirSync(out,{recursive:true});const hashes={};
for(const name of ['safe_cex_wasm_bg.wasm','safe_cex_wasm.js','package.json']) {
 const bytes=readFileSync('sdk-rs/hosts/wasm/pkg/node/'+name);hashes[name]=createHash('sha256').update(bytes).digest('hex');writeFileSync(out+'/'+name,bytes);
}
writeFileSync(out+'/build.json',JSON.stringify({sourceHead:execFileSync('git',['rev-parse','HEAD'],{encoding:'utf8'}).trim(),hashes}));
`);
  git('add', '.'); git('commit', '-qm', 'base'); const baseline = git('rev-parse', 'HEAD');
  put('candidate.txt', 'candidate'); git('add', '.'); git('commit', '-qm', 'candidate'); const candidate = git('rev-parse', 'HEAD');
  const tools = fs.mkdtempSync(resolve(tmpdir(), 'cpu-family-tools-'));
  t.after(() => fs.rmSync(tools, { recursive: true, force: true }));
  fs.writeFileSync(resolve(tools, 'cargo'), '#!/bin/sh\ntest "$1" = clean\n'); fs.chmodSync(resolve(tools, 'cargo'), 0o755);
  const authority = { candidate, baseline, candidateTree: git('rev-parse', 'HEAD^{tree}'), runId: '10', suite: 'txflow' };
  for (const fail of [false, true]) {
    for (const directory of ['node', 'web', 'ci-replay']) {
      put(`sdk-rs/hosts/wasm/pkg/${directory}/safe_cex_wasm_bg.wasm`, 'candidate wasm');
      put(`sdk-rs/hosts/wasm/pkg/${directory}/safe_cex_wasm.js`, 'candidate glue');
      put(`sdk-rs/hosts/wasm/pkg/${directory}/package.json`, '{}');
      put(`sdk-rs/hosts/wasm/pkg/${directory}/build.json`, '{}');
    }
    const temporary = fs.mkdtempSync(resolve(tmpdir(), 'cpu-family-build-'));
    t.after(() => fs.rmSync(temporary, { recursive: true, force: true }));
    const path = resolve(temporary, 'authority.json'); fs.writeFileSync(path, JSON.stringify(authority));
    const child = spawnSync('python3', [script, '--authority', path, '--temporary', temporary], { cwd: root, encoding: 'utf8',
      env: { ...process.env, PATH: `${tools}:${process.env.PATH}`, GITHUB_ACTIONS: 'true', GITHUB_RUN_ATTEMPT: '1', GITHUB_RUN_ID: '10', FAIL_FAMILY_BUILD: String(fail) } });
    assert.equal(child.status, fail ? 1 : 0, child.stderr);
    assert.equal(git('rev-parse', 'HEAD'), candidate);
    for (const directory of ['node', 'web']) {
      assert.equal(fs.readFileSync(resolve(root, `sdk-rs/hosts/wasm/pkg/${directory}/safe_cex_wasm_bg.wasm`), 'utf8'), 'candidate wasm');
      assert.equal(fs.readFileSync(resolve(root, `sdk-rs/hosts/wasm/pkg/${directory}/safe_cex_wasm.js`), 'utf8'), 'candidate glue');
    }
    const proof = resolve(root, 'sdk-rs/harness/out/cpu-family');
    assert.equal(fs.existsSync(resolve(proof, 'proof.json')), !fail);
    if (!fail) assert.equal(fs.readFileSync(resolve(proof, 'baseline/safe_cex_wasm_bg.wasm'), 'utf8'), 'baseline wasm');
    fs.rmSync(proof, { recursive: true });
  }
});
