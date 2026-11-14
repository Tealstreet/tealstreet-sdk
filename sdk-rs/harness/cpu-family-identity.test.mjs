import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import test from 'node:test';
import { collectFamilyIdentity, familyComparisonAuthority, readFamilyIdentity, readFamilyIdentityDirectory } from './cpu-family-identity.mjs';
import { classifyInitialCase } from './cpu-ci.mjs';
import { expectedGateCases } from './ci-gate-suites.mjs';
import { verifyGateSuite } from './verify-gates.mjs';
import { PUBLIC_REPLAY_PAIRS, performanceFailures, performanceOrder } from './performance-policy.mjs';

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
function evidence(metadata, second = false, steady) {
  const raw = [], publicCase = expectedGateCases(metadata.venue)[0];
  const symbols = publicCase.books.split(',');
  const add = (impl, mode, run) => {
    const output = { impl, frames: 10, commands: [], transportReceipt: { frames: 10 }, protocolErrors: [],
      markets: [{ symbol: symbols[0] }], tickers: [{ symbol: symbols[0], last: 1 }],
      books: Object.fromEntries(symbols.map(symbol => [symbol, { bids: [[1, 1]] }])),
      metrics: { ...metric(mode, (impl === 'ts' ? 200 : 170) * (metadata.venue === 'okx' ? 10 : 1), impl === 'ts' ? 100 : 80),
        steadyCpuMs: steady ? steady[impl] : metadata.venue === 'okx'
          ? second ? (impl === 'ts' ? 325.7 : 289.496) : (impl === 'ts' ? 336.515 : 312.985)
          : second ? (impl === 'ts' ? 18.836 : 20.195) : (impl === 'ts' ? 18.745 : 19.896) } };
    const bytes = JSON.stringify(output); raw.push({ impl, mode, run, bytes, sha256: sha(bytes) });
  };
  for (const impl of ['ts', 'rust']) add(impl, 'correctness', 0);
  for (let run = 0; run < PUBLIC_REPLAY_PAIRS; run++) for (const impl of performanceOrder(run)) add(impl, 'performance', run);
  const samples = Object.fromEntries(['ts', 'rust'].map(impl => [impl, raw.filter(slot => slot.impl === impl && slot.mode === 'performance').map(slot => JSON.parse(slot.bytes).metrics)]));
  const failures = performanceFailures(samples.ts, samples.rust).sort();
  const report = { ...publicCase, schema: 1, tapeSha256: sha('tape'), private: false,
    runs: PUBLIC_REPLAY_PAIRS, correctnessRuns: 1, requiredImprovement: .1, status: failures.length ? 'FAIL' : 'PASS', failures, samples, fixers: [], ciWasm: metadata,
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

test('original txflow steady CPU (Rust 1.061x TS) is a CPU refusal unless identical sealed bytes need no measurement', async () => {
  const data = await fixture(), first = evidence(data.metadata);
  assert.equal(classifyInitialCase(first, declarations).decision, 'cpu-refused');
  const certificate = readFamilyIdentity(data);
  assert.equal(certificate.disposition, 'unchanged bytes, not measured');
  assert.equal(certificate.measured, false);
  assert.equal(certificate.q, undefined);
  assert.equal(classifyInitialCase(first, declarations, undefined, certificate).decision, 'first');
  assert.equal(first.report.status, 'FAIL', 'strict diagnostic remains unchanged');
});

test('master 41618845764 OKX inverse steady at 0.930x TS is a warning now, not a miss', async () => {
  const data = await fixture('okx'), first = evidence(data.metadata);
  assert.equal(data.authority.baseline, A, 'master compares to its actual first parent');
  assert.equal(first.report.status, 'PASS', 'missing the 10%-faster target no longer fails');
  assert.equal(classifyInitialCase(first, declarations).decision, 'first');
  assert.equal(classifyInitialCase(first, declarations, undefined, readFamilyIdentity(data)).decision, 'first');
});

test('txflow first measurement 17.924/20.579 bypasses CPU eligibility only on identical sealed bytes', async () => {
  const data = await fixture(), first = evidence(data.metadata, false, { ts: 17.924, rust: 20.579 });
  const before = JSON.stringify(first);
  // Beyond remeasurement the miss refuses only the CPU gate; it is not a correctness error.
  assert.equal(classifyInitialCase(first, declarations).decision, 'cpu-refused');
  const mixed = structuredClone(first); mixed.report.failures = [...mixed.report.failures, 'private snapshot mismatch'].sort();
  assert.throws(() => classifyInitialCase(mixed, declarations), 'a correctness failure is never routed to the CPU gate');
  const certificate = readFamilyIdentity(data);
  assert.equal(classifyInitialCase(first, declarations, undefined, certificate).decision, 'first');
  assert.equal(JSON.stringify(first), before);
  for (const name of ['safe_cex_wasm_bg.wasm', 'safe_cex_wasm.js']) {
    const changed = await fixture();
    changed.entries.set(`cpu-family/baseline/${name}`, Buffer.from('changed bytes'));
    changeMetadata(changed, 'baseline', metadata => { metadata.hashes[name] = sha('changed bytes'); });
    assert.equal(classifyInitialCase(first, declarations, undefined, readFamilyIdentity(changed)).decision, 'cpu-refused');
  }
});

test('first replay collection accepts a sealed CPU-only miss with complete public and private coverage', async () => {
  const data = await fixture(), first = evidence(data.metadata, false, { ts: 17.924, rust: 20.579 });
  const privateReport = { ...first.report, ...expectedGateCases('txflow')[1], tapeSha256: sha('private tape'), private: true,
    status: 'PASS', failures: [], runs: 0, samples: Object.fromEntries(['ts', 'rust'].map(impl =>
      [impl, [{ ...first.report.samples[impl][0], measuredMode: 'correctness' }]])) };
  const entries = [{ ...first, exitCode: 1 }, { report: privateReport, exitCode: 0 }];
  const options = { suite: 'txflow', entries, manifest: declarations, mode: 'collect', sourceHead: data.state.sourceHead };
  const old = verifyGateSuite(options);
  assert.deepEqual(old.errors, [], 'a large CPU-only miss is not a measurement (correctness) error');
  assert.equal(old.cpuMisses.length, 1, 'it is left to the CPU gate');
  assert.equal(old.rows[0].label, 'CPU MISS (judged by the CPU gate)');
  const mixed = structuredClone(entries); mixed[0].report.failures = [...mixed[0].report.failures, 'private snapshot mismatch'].sort();
  assert.ok(verifyGateSuite({ ...options, entries: mixed }).errors.length, 'a correctness failure still fails the measurement');
  const family = readFamilyIdentity(data), accepted = verifyGateSuite({ ...options, family });
  assert.deepEqual(accepted.errors, []);
  assert.deepEqual(accepted.pending, []);
  assert.equal(accepted.rows[0].label, 'UNCHANGED BYTES, NOT MEASURED');
  assert.ok(verifyGateSuite({ ...options, mode: 'strict', family }).errors.length);
  for (const mutate of [
    rows => { rows.pop(); }, rows => { rows[0].raw.pop(); }, rows => { rows[0].exitCode = 0; },
    rows => { rows[1].report.status = 'FAIL'; rows[1].report.failures = ['private snapshot mismatch']; rows[1].exitCode = 1; },
    rows => { for (const sample of rows[0].report.samples.rust) sample.retainedBytes = 10000; },
  ]) {
    const invalid = structuredClone(entries); mutate(invalid);
    assert.ok(verifyGateSuite({ ...options, entries: invalid, family }).errors.length);
  }
});

test('sealed-runtime reports without ciWasm metadata use the issued runtime proof and authenticated source', async () => {
  for (const suite of ['bybit', 'okx', 'txflow']) {
    // OKX's fixture is otherwise faster than TS; make its steady CPU slower so identity has a miss to waive.
    const data = await fixture(suite), first = evidence(data.metadata, false, suite === 'okx' ? { ts: 312.985, rust: 336.515 } : undefined);
    delete first.report.ciWasm;
    const before = JSON.stringify(first), certificate = readFamilyIdentity(data);
    assert.equal(classifyInitialCase(first, declarations, undefined, certificate).decision, 'first');
    assert.equal(JSON.stringify(first), before, 'do not invent ciWasm metadata on sealed-runtime reports');
    const foreign = structuredClone(first); foreign.execution.measuredCheckoutSha = A;
    assert.throws(() => classifyInitialCase(foreign, declarations, undefined, certificate));
    assert.throws(() => classifyInitialCase(first, declarations, undefined, { ...certificate }));
    for (const ciWasm of [null, { ...data.metadata, sourceHead: A }, { ...data.metadata, hashes: {} }]) {
      const invalid = structuredClone(first); invalid.report.ciWasm = ciWasm;
      assert.throws(() => classifyInitialCase(invalid, declarations, undefined, certificate));
    }
    for (const name of ['safe_cex_wasm_bg.wasm', 'safe_cex_wasm.js']) {
      const changed = await fixture(suite);
      changed.entries.set(`cpu-family/baseline/${name}`, Buffer.from('changed base bytes'));
      changeMetadata(changed, 'baseline', metadata => { metadata.hashes[name] = sha('changed base bytes'); });
      const proof = readFamilyIdentity(changed);
      assert.equal(proof, undefined);
      assert.equal(classifyInitialCase(first, declarations, undefined, proof).decision, 'cpu-refused');
    }
  }
});

test('first collection binds metadata-free sealed reports to the authenticated collection source', async () => {
  const data = await fixture(), first = evidence(data.metadata, false, { ts: 17.924, rust: 20.579 });
  delete first.report.ciWasm;
  const privateReport = { ...first.report, ...expectedGateCases('txflow')[1], tapeSha256: sha('private tape'), private: true,
    status: 'PASS', failures: [], runs: 0, samples: Object.fromEntries(['ts', 'rust'].map(impl =>
      [impl, [{ ...first.report.samples[impl][0], measuredMode: 'correctness' }]])) };
  const entries = [{ ...first, exitCode: 1 }, { report: privateReport, exitCode: 0 }];
  const options = { suite: 'txflow', entries, manifest: declarations, mode: 'collect',
    sourceHead: data.state.sourceHead, family: readFamilyIdentity(data) };
  const before = JSON.stringify(entries);
  const result = verifyGateSuite(options);
  assert.deepEqual(result.errors, []); assert.deepEqual(result.pending, []);
  assert.equal(result.rows[0].label, 'UNCHANGED BYTES, NOT MEASURED');
  assert.equal(JSON.stringify(entries), before);
  for (const sourceHead of [undefined, A]) assert.throws(() => verifyGateSuite({ ...options, sourceHead }));
  for (const name of ['safe_cex_wasm_bg.wasm', 'safe_cex_wasm.js']) {
    const changed = await fixture();
    changed.entries.set(`cpu-family/baseline/${name}`, Buffer.from('changed base bytes'));
    changeMetadata(changed, 'baseline', metadata => { metadata.hashes[name] = sha('changed base bytes'); });
    const measured = evidence(data.metadata); delete measured.report.ciWasm;
    const family = readFamilyIdentity(changed); assert.equal(family, undefined);
    const normal = verifyGateSuite({ ...options, family, entries: [{ ...measured, exitCode: 1 }, entries[1]] });
    assert.deepEqual(normal.errors, []); assert.equal(normal.pending.length, 0); assert.equal(normal.cpuMisses.length, 1);
    assert.equal(normal.rows[0].label, 'CPU MISS (judged by the CPU gate)');
    const large = verifyGateSuite({ ...options, family });
    assert.deepEqual(large.errors, []); assert.equal(large.cpuMisses.length, 1, 'changed bytes leave the large miss to the CPU gate');
  }
});

test('changed WASM or glue leaves a CPU miss to the CPU gate', async () => {
  for (const name of ['safe_cex_wasm_bg.wasm', 'safe_cex_wasm.js']) {
    const data = await fixture();
    data.entries.set(`cpu-family/baseline/${name}`, Buffer.from('different bytes'));
    changeMetadata(data, 'baseline', metadata => { metadata.hashes[name] = sha('different bytes'); });
    assert.equal(readFamilyIdentity(data), undefined);
    const first = evidence(data.metadata);
    assert.equal(classifyInitialCase(first, declarations, undefined, readFamilyIdentity(data)).decision, 'cpu-refused');
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

test('BingX fixture workflow archives the measured candidate before CPU-miss collection', async t => {
  const { execFileSync, spawnSync } = await import('node:child_process');
  const fs = await import('node:fs');
  const { tmpdir } = await import('node:os');
  const { resolve } = await import('node:path');
  const root = fs.realpathSync(fs.mkdtempSync(resolve(tmpdir(), 'cpu-bingx-archive-')));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  const put = (name, bytes) => { const path = resolve(root, name); fs.mkdirSync(resolve(path, '..'), { recursive: true }); fs.writeFileSync(path, bytes); };
  const git = (...args) => execFileSync('git', args, { cwd: root, encoding: 'utf8' }).trim();
  const workflow = fs.readFileSync(new URL('../../.github/workflows/rust.yml', import.meta.url), 'utf8');
  const measurement = workflow.slice(workflow.indexOf('\n  gate:\n'), workflow.indexOf('\n  bitget-paired:\n'));
  const names = ['Build the venue wasm engine', 'Archive the matching CI replay engine',
    'Check the wasm size budget', 'Build the nondefault BingX Spot measurement engine'];
  const steps = measurement.split(/^      - /m).filter(step => names.some(name => step.startsWith(`name: ${name}\n`)));
  assert.equal(steps.length, names.length);
  git('init', '-q', '-b', 'master'); git('config', 'user.name', 'Fixture'); git('config', 'user.email', 'fixture@example.invalid');
  put('.gitignore', 'sdk-rs/hosts/wasm/pkg/\nsdk-rs/harness/out/\nrunner/\n');
  // Only compilation and archival tool metadata are scripted; workflow shell,
  // production collector, byte hashes and identity validation remain real.
  put('sdk-rs/harness/build.sh', `set -eu
variant=ordinary
for arg in "$@"; do if [ "$arg" = --bingx-spot-replay ]; then variant=fixture; fi; done
mkdir -p sdk-rs/hosts/wasm/pkg/node
printf '%s wasm' "$variant" > sdk-rs/hosts/wasm/pkg/node/safe_cex_wasm_bg.wasm
printf '%s glue' "$variant" > sdk-rs/hosts/wasm/pkg/node/safe_cex_wasm.js
printf '{"type":"commonjs"}' > sdk-rs/hosts/wasm/pkg/node/package.json
`);
  put('sdk-rs/harness/ci-wasm.mjs', `import fs from 'node:fs'; import {createHash} from 'node:crypto'; import {execFileSync} from 'node:child_process';
const destination=process.argv.at(-1), hashes={}; fs.mkdirSync(destination,{recursive:true});
for(const name of ['safe_cex_wasm_bg.wasm','safe_cex_wasm.js','package.json']) {
 const bytes=fs.readFileSync('sdk-rs/hosts/wasm/pkg/node/'+name); hashes[name]=createHash('sha256').update(bytes).digest('hex'); fs.writeFileSync(destination+'/'+name,bytes);
}
fs.writeFileSync(destination+'/build.json',JSON.stringify({schema:1,sourceHead:execFileSync('git',['rev-parse','HEAD'],{encoding:'utf8'}).trim(),runId:'10',family:'bingx',venue:'bingx',target:'nodejs',profile:'release',tools:{rustc:'pinned'},buildFlags:{wasmOptLevel:'-Oz'},hashes}));
`);
  fs.copyFileSync(new URL('./collect-cpu-family.py', import.meta.url), resolve(root, 'sdk-rs/harness/collect-cpu-family.py'));
  git('add', '.'); git('commit', '-qm', 'ordinary baseline'); const baseline = git('rev-parse', 'HEAD');
  put('candidate.txt', 'fixture build is available'); git('add', '.'); git('commit', '-qm', 'candidate');
  const candidate = git('rev-parse', 'HEAD'), tree = git('rev-parse', 'HEAD^{tree}');
  put('runner/bin/corepack', `#!/bin/sh
set -eu
test "$1" = yarn; shift
case "$1" in
  rust:build-wasm) shift; exec bash sdk-rs/harness/build.sh "$@" ;;
  -s) shift; if [ "$1" = node ]; then shift; exec '${process.execPath}' "$@"; fi
      test "$1" = rust:wasm-budget
      test "$(cat sdk-rs/hosts/wasm/pkg/node/safe_cex_wasm_bg.wasm)" = 'ordinary wasm'
      cp -r sdk-rs/hosts/wasm/pkg/node sdk-rs/hosts/wasm/pkg/shipped-size-evidence ;;
  *) exit 2 ;;
esac
`);
  fs.chmodSync(resolve(root, 'runner/bin/corepack'), 0o755);
  const env = { ...process.env, PATH: `${resolve(root, 'runner/bin')}:${process.env.PATH}`,
    RUNNER_TEMP: resolve(root, 'runner'), GITHUB_ACTIONS: 'true', GITHUB_RUN_ATTEMPT: '1', GITHUB_RUN_ID: '10' };
  for (const step of steps) {
    const command = step.split('        run: ')[1].trim().replace(/^\|\n/, '')
      .replaceAll('${{ matrix.venue }}', 'bingx').replaceAll('${{ inputs.replay_artifact }}', 'family');
    const child = spawnSync('bash', ['-e', '-o', 'pipefail', '-c', command], { cwd: root, encoding: 'utf8', env });
    assert.equal(child.status, 0, child.stderr);
  }
  assert.equal(fs.readFileSync(resolve(root, 'sdk-rs/hosts/wasm/pkg/shipped-size-evidence/safe_cex_wasm_bg.wasm'), 'utf8'), 'ordinary wasm');
  const data = await fixture('bingx');
  data.state.run.head_sha = data.state.job.head_sha = candidate;
  data.state.sourceHead = candidate;
  const options = { ...data.options, root, env, gitRead: (...args) => args[1] === 'HEAD' ? candidate : tree,
    reader: { ...data.options.reader, getCommit: async () => ({ sha: candidate, tree: { sha: tree }, parents: [{ sha: baseline }] }) } };
  const authority = await collectFamilyIdentity(options, 'bingx');
  const directory = resolve(root, 'sdk-rs/harness/out/cpu-family');
  const files = ['safe_cex_wasm_bg.wasm', 'safe_cex_wasm.js', 'package.json'];
  const runtimeManifest = { entries: files.map(name => ({ path: `sdk-rs/hosts/wasm/pkg/node/${name}`,
    type: 'file', sha256: sha(fs.readFileSync(resolve(root, `sdk-rs/hosts/wasm/pkg/node/${name}`))) })) };
  const read = () => readFamilyIdentityDirectory({ directory, authority, state: data.state, runtimeManifest });
  assert.equal(read(), undefined, 'different fixture bytes must not earn ordinary-family byte equivalence');
  const metadata = JSON.parse(fs.readFileSync(resolve(directory, 'candidate/build.json')));
  const miss = evidence(metadata); Object.assign(miss.report, expectedGateCases('bingx').find(c => c.venue === 'bingxspot' && c.tape.includes('/fixtures/')));
  miss.execution.measuredCheckoutSha = candidate;
  assert.equal(classifyInitialCase(miss, declarations, undefined, read()).decision, 'cpu-refused');
  for (const name of ['safe_cex_wasm_bg.wasm', 'safe_cex_wasm.js']) {
    assert.equal(fs.readFileSync(resolve(directory, `candidate/${name}`), 'utf8'), `fixture ${name.endsWith('.wasm') ? 'wasm' : 'glue'}`);
    const original = fs.readFileSync(resolve(directory, `candidate/${name}`));
    fs.writeFileSync(resolve(directory, `candidate/${name}`), fs.readFileSync(resolve(directory, `baseline/${name}`)));
    const changed = structuredClone(metadata); changed.hashes[name] = sha(fs.readFileSync(resolve(directory, `candidate/${name}`)));
    fs.writeFileSync(resolve(directory, 'candidate/build.json'), JSON.stringify(changed));
    assert.throws(read, /candidate differs from original family runtime/, `ordinary ${name} cannot replace the measured fixture`);
    fs.writeFileSync(resolve(directory, `candidate/${name}`), original);
    fs.writeFileSync(resolve(directory, 'candidate/build.json'), JSON.stringify(metadata));
  }
});

test('the production base collector builds a clean separate parent with a dirty candidate and preserves it on failure', async t => {
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
  put('.gitignore', 'sdk-rs/hosts/wasm/pkg/\nsdk-rs/harness/out/\nsdk-rs/target/\n');
  put('sdk-rs/harness/build.sh', `set -eu
if [ "\u0024{FAIL_FAMILY_BUILD:-}" = true ]; then exit 17; fi
test ! -e candidate.txt
test ! -e generated.json
test ! -e sdk-rs/target/candidate-cache
test "\u0024CARGO_TARGET_DIR" = "\u0024PWD/sdk-rs/target"
mkdir -p sdk-rs/hosts/wasm/pkg/node sdk-rs/hosts/wasm/pkg/web
printf 'baseline glue' > sdk-rs/hosts/wasm/pkg/node/safe_cex_wasm.js
printf '{}' > sdk-rs/hosts/wasm/pkg/node/package.json
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
  const authority = { candidate, baseline, candidateTree: git('rev-parse', 'HEAD^{tree}'), runId: '10', suite: 'txflow' };
  put('candidate.txt', 'generated tracked change');
  put('generated.json', '{"replayGenerated":true}');
  put('sdk-rs/target/candidate-cache', 'original target cache');
  const before = git('status', '--porcelain'), diffBefore = git('diff', '--binary', 'HEAD');
  assert.match(before, /candidate.txt/);
  assert.match(before, /generated.json/);
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
      env: { ...process.env, GITHUB_ACTIONS: 'true', GITHUB_RUN_ATTEMPT: '1', GITHUB_RUN_ID: '10', FAIL_FAMILY_BUILD: String(fail) } });
    assert.equal(child.status, fail ? 1 : 0, child.stderr);
    assert.equal(git('rev-parse', 'HEAD'), candidate);
    assert.equal(git('status', '--porcelain'), before);
    assert.equal(git('diff', '--binary', 'HEAD'), diffBefore);
    assert.equal(fs.readFileSync(resolve(root, 'generated.json'), 'utf8'), '{"replayGenerated":true}');
    assert.equal(fs.readFileSync(resolve(root, 'sdk-rs/target/candidate-cache'), 'utf8'), 'original target cache');
    const base = resolve(temporary, 'base-checkout');
    assert.equal(fs.existsSync(resolve(base, 'candidate.txt')), false);
    assert.equal(fs.existsSync(resolve(base, 'generated.json')), false);
    assert.equal(git('-C', base, 'rev-parse', 'HEAD'), baseline);
    assert.equal(git('-C', base, 'status', '--porcelain'), '');
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

// #3246 review: only the exact computed CPU failures are deferred to the CPU gate.
async function cpuOnlyCollection() {
  const data = await fixture(), first = evidence(data.metadata, false, { ts: 17.924, rust: 20.579 });
  const privateReport = { ...first.report, ...expectedGateCases('txflow')[1], tapeSha256: sha('private tape'), private: true,
    status: 'PASS', failures: [], runs: 0, samples: Object.fromEntries(['ts', 'rust'].map(impl =>
      [impl, [{ ...first.report.samples[impl][0], measuredMode: 'correctness' }]])) };
  return { suite: 'txflow', entries: [{ ...first, exitCode: 1 }, { report: privateReport, exitCode: 0 }], manifest: declarations,
    mode: 'collect', sourceHead: data.state.sourceHead };
}
test('a valid CPU-only miss beyond remeasurement is collection success', async () => {
  const result = verifyGateSuite(await cpuOnlyCollection());
  assert.deepEqual(result.errors, []);
  assert.equal(result.cpuMisses.length, 1);
});
test('a correctness-only report with a CPU miss still fails the measurement', async () => {
  const options = await cpuOnlyCollection();
  options.entries[0].report.correctnessOnly = true;
  const result = verifyGateSuite(options);
  assert.ok(result.errors.some(error => error.includes('correctness-only reports cannot substitute')), JSON.stringify(result.errors));
  assert.ok(!result.errors.some(error => error.includes('performance ')), 'the CPU failure itself is still deferred');
});
test('a stale accepted difference with a CPU miss still fails the measurement', async () => {
  const options = await cpuOnlyCollection(), report = options.entries[0].report;
  options.accepted = { schema: 1, differences: [{ venue: report.venue, tape: report.tape, tapeSha256: report.tapeSha256, diagnostics: [{
    failures: ['checkpoint 0 markets[0].limits.amount.min: ts=1 rust=2'], category: 'rust-better',
    rationale: 'Previously accepted market limit difference', evidence: 'Pinned old limits' }] }] };
  const result = verifyGateSuite(options);
  assert.ok(result.errors.some(error => error.includes('stale accepted difference')), JSON.stringify(result.errors));
});
