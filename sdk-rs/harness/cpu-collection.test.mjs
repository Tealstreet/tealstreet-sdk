import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { gzipSync } from 'node:zlib';
import { fileURLToPath } from 'node:url';
import { prepareCollection, beforeCpuCase, afterCpuCase, readSourceTree, selectCollectionRuntimeInputs, resetCollectionCache, capturePublicRaw } from './cpu-collection.mjs';
import { expectedGateCases } from './ci-gate-suites.mjs';
import { performanceOrder } from './performance-policy.mjs';

// Fixture workspaces deliberately have no ambient module/loader overrides.
const ambient = Object.fromEntries(['NODE_OPTIONS', 'NODE_PATH', 'TSX_DISABLE_CACHE', 'TSX_TSCONFIG_PATH'].map(key => [key, process.env[key]]));
for (const key of Object.keys(ambient)) delete process.env[key];
test.after(() => { for (const [key, value] of Object.entries(ambient)) { if (value === undefined) delete process.env[key]; else process.env[key] = value; } });

const sha = value => createHash('sha256').update(value).digest('hex');
const environment = { ImageOS: 'fixture-linux', ImageVersion: 'fixture-image', arch: 'x64', nodeVersion: 'v22.1.0', nodeVersions: { node: '22.1.0', v8: 'fixture-v8' } };
function write(root, name, data) { const filename = path.join(root, name); fs.mkdirSync(path.dirname(filename), { recursive: true }); fs.writeFileSync(filename, data); return filename; }
function git(root, ...args) { const result = spawnSync('git', args, { cwd: root, encoding: 'utf8' }); assert.equal(result.status, 0, result.stderr); return result.stdout.trim(); }
function fixture(t, suite = 'binance') {
  const temp = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), 'cpu-collection-test-'))), root = path.join(temp, 'workspace');
  fs.mkdirSync(root); t.after(() => fs.rmSync(temp, { recursive: true, force: true }));
  write(root, '.gitignore', 'node_modules/\nout/\n.cpu-runtime/\npkg/\n.turbo/\n');
  write(root, 'package.json', '{"private":true,"workspaces":["packages/*","sdk-rs/hosts/wasm","apps/cli"]}');
  write(root, 'sdk-rs/harness/cpu-remeasure.mjs', 'export const rule = "fixture-policy";');
  write(root, 'sdk-rs/harness/gate.mjs', '// tracked runtime');
  write(root, 'packages/fixture/package.json', '{"name":"fixture"}');
  write(root, 'packages/fixture/src/index.ts', 'export const value = 1;');
  write(root, 'sdk-rs/hosts/wasm/package.json', '{"name":"fixture-wasm-host"}');
  write(root, 'apps/cli/package.json', '{"name":"unused-workspace-link"}');
  write(root, 'packages/fixture/.turbo/ambient.log', 'ignored and deliberately not runtime');
  write(root, 'node_modules/dependency/index.js', 'module.exports = 1;');
  fs.symlinkSync('../packages/fixture', path.join(root, 'node_modules/fixture'));
  fs.symlinkSync('../sdk-rs/hosts/wasm', path.join(root, 'node_modules/wasm-host'));
  fs.symlinkSync('../apps/cli', path.join(root, 'node_modules/unused-app'));
  write(root, 'packages/fixture/node_modules/local/index.js', 'module.exports = 2;');
  for (const item of expectedGateCases(suite)) {
    const privateTape = item.tape.includes('-private.');
    if (!item.tape.includes('/fixtures/') && privateTape) continue;
    const data = Buffer.from(`${JSON.stringify({ type: 'meta', private: privateTape })}\n${JSON.stringify({ type: 'config', private: privateTape })}\n`);
    write(root, item.tape, item.tape.endsWith('.gz') ? gzipSync(data) : data);
  }
  for (const name of ['safe_cex_wasm.js', 'safe_cex_wasm_bg.wasm', 'package.json']) write(root, `sdk-rs/hosts/wasm/pkg/node/${name}`, `actual combined-or-family ${name}`);
  git(root, 'init', '-q'); git(root, 'add', '.'); git(root, '-c', 'user.name=Fixture', '-c', 'user.email=fixture@example.invalid', 'commit', '-qm', 'fixture');
  const head = git(root, 'rev-parse', 'HEAD');
  const run = { id: 39999999999, run_number: 6100, run_attempt: 1, head_sha: head, event: 'push' };
  const job = { id: 1234, runner_id: 5678, run_id: run.id, run_attempt: 1, head_sha: head, name: `Rust / replay measurement (${suite})` };
  return { root, suite, destination: path.join(temp, 'capsule'), environment, run, job };
}
function args(claim) { return ['--venue', claim.case.venue, '--tape', claim.case.tape, '--books', claim.case.books, '--runs', '9']; }
function reportFixture(state, claim, { exitCode = 0 } = {}) {
  const slots = [...['ts', 'rust'].map(impl => ({ impl, mode: 'correctness', run: 0 })), ...Array.from({ length: 9 }, (_, run) => performanceOrder(run).map(impl => ({ impl, mode: 'performance', run }))).flat()];
  const artifacts = slots.map((slot, index) => {
    const filename = `sdk-rs/harness/out/raw-${claim.key}/${index}.json`;
    write(state.root, filename, JSON.stringify({ impl: slot.impl, metrics: { fixture: true }, retained: { exact: index } }));
    return { ...slot, path: filename };
  });
  const report = { schema: 1, ...claim.case, requiredImprovement: .1, status: exitCode === 0 ? 'PASS' : 'FAIL', failures: exitCode === 0 ? [] : ['original CPU miss'], fixers: [], artifacts };
  const reportPath = write(state.root, `sdk-rs/harness/out/report-${claim.key}.json`, `${JSON.stringify(report, null, 2)}\n`);
  return { reportPath, report };
}
async function pause() { await new Promise(resolve => setTimeout(resolve, 2)); }

test('premeasurement claim bundle seals exact public roster, actual module, dependencies and shallow package roots', t => {
  const options = fixture(t), state = prepareCollection(options);
  assert.equal(state.claims.length, 5);
  assert.equal(state.claims.filter(claim => claim.case.venue === 'binanceoptions').length, 1);
  const bundle = JSON.parse(fs.readFileSync(path.join(options.root, 'sdk-rs/harness/out/cpu-collection/cohort/cohort.json')));
  assert.deepEqual(bundle.claims, state.claims);
  assert.equal(bundle.claims[0].measuredCheckoutSha, options.run.head_sha);
  assert.ok(bundle.claims.every(claim => claim.case.private === false && claim.case.runs === 9 && claim.case.correctnessRuns === 1));
  const manifest = JSON.parse(fs.readFileSync(path.join(options.destination, 'manifest.json')));
  assert.deepEqual(fs.readdirSync(path.join(options.root, 'sdk-rs/harness/out/cpu-collection/cohort')).sort(), ['cohort.json', 'manifest.json']);
  assert.deepEqual(manifest.directoryAnchors, ['apps/cli', 'packages/fixture', 'sdk-rs/hosts/wasm']);
  assert.ok(manifest.entries.some(entry => entry.path === 'node_modules/dependency/index.js'));
  assert.ok(manifest.entries.some(entry => entry.path === 'packages/fixture/node_modules/local/index.js'));
  assert.ok(!manifest.entries.some(entry => entry.path.includes('.turbo')));
  for (const name of ['safe_cex_wasm.js', 'safe_cex_wasm_bg.wasm', 'package.json']) {
    const entry = manifest.entries.find(entry => entry.path === `sdk-rs/hosts/wasm/pkg/node/${name}`);
    assert.equal(entry.sha256, sha(fs.readFileSync(path.join(options.root, entry.path))));
  }
  assert.ok(state.destination.startsWith(path.dirname(options.root)) && !state.destination.startsWith(`${options.root}/`));
  assert.throws(() => prepareCollection(options), /output collision/);
});

test('each public case starts empty, records immediate final cache, exact reports/raw and consumes one marker', async t => {
  const options = fixture(t), state = prepareCollection(options), first = state.claims[0], second = state.claims[1];
  const before = beforeCpuCase({ state, args: args(first), root: options.root });
  assert.equal(before.nodePath, path.join(options.root, '.cpu-runtime/node'));
  assert.deepEqual(fs.readdirSync(before.env.TMPDIR), []);
  write(options.root, 'sdk-rs/harness/out/cpu-collection/tmp/tsx-loader', 'generated transform');
  const original = reportFixture(state, first, { exitCode: 1 }); await pause();
  const result = afterCpuCase({ context: before, reportPath: original.reportPath, exitCode: 1 });
  assert.equal(result.evidence.reportBytes, fs.readFileSync(original.reportPath, 'utf8'));
  assert.equal(result.evidence.raw.length, 20); assert.equal(result.evidence.exitCode, 1);
  assert.notEqual(result.evidence.execution.finalCacheSha256, result.evidence.execution.initialCacheSha256);
  assert.equal(result.evidence.execution.jobId, String(options.job.id)); assert.equal(result.evidence.execution.runnerId, String(options.job.runner_id));
  assert.ok(result.evidence.raw.every(item => sha(item.bytes) === item.sha256));
  assert.throws(() => afterCpuCase({ context: before, reportPath: original.reportPath, exitCode: 1 }), /consumed/);
  assert.throws(() => beforeCpuCase({ state, args: args(first) }), /already started/);
  const sibling = beforeCpuCase({ state, args: args(second) });
  assert.deepEqual(fs.readdirSync(sibling.env.TMPDIR), []);
  assert.equal(sibling.initialCacheSha256, before.initialCacheSha256);
  const report = reportFixture(state, second); await pause(); afterCpuCase({ context: sibling, reportPath: report.reportPath, exitCode: 0 });
  const final = JSON.parse(fs.readFileSync(state.statePath)); assert.equal(final.records.length, 2); assert.equal(final.active, null);
});

test('private invocation remains unchanged and cannot clear a public cache or consume a claim', t => {
  const options = fixture(t), state = prepareCollection(options);
  const privateCase = expectedGateCases(options.suite).find(item => item.tape.includes('/tapes/'));
  write(options.root, privateCase.tape, '{"type":"config","private":true}\n');
  write(options.root, 'sdk-rs/harness/out/cpu-collection/tmp/prior', 'retain private behavior');
  assert.equal(beforeCpuCase({ state, args: { ...privateCase, runs: '3' } }), null);
  assert.equal(afterCpuCase({ context: null }), null);
  assert.equal(fs.readFileSync(path.join(options.root, 'sdk-rs/harness/out/cpu-collection/tmp/prior'), 'utf8'), 'retain private behavior');
  assert.equal(JSON.parse(fs.readFileSync(state.statePath)).records.length, 0);
});

test('public preflight rejects unknown/mutated case, active duplicate, runtime drift and CI module substitution', t => {
  const options = fixture(t), state = prepareCollection(options), claim = state.claims[0];
  for (const values of [{ ...claim.case, runs: 7 }, { ...claim.case, books: 'wrong' }, { ...claim.case, runs: 9, 'ci-wasm': 'family-archive' }, { ...claim.case, runs: 9, 'recorded-evidence': 'prior.json' }])
    assert.throws(() => beforeCpuCase({ state, args: values }));
  const originalTape = fs.readFileSync(path.join(options.root, claim.case.tape)); fs.appendFileSync(path.join(options.root, claim.case.tape), 'tamper');
  assert.throws(() => beforeCpuCase({ state, args: args(claim) })); fs.writeFileSync(path.join(options.root, claim.case.tape), originalTape);
  const filename = path.join(options.root, 'node_modules/dependency/index.js'), original = fs.readFileSync(filename); fs.writeFileSync(filename, 'changed');
  assert.throws(() => beforeCpuCase({ state, args: args(claim) }), /collision/); fs.writeFileSync(filename, original);
  beforeCpuCase({ state, args: args(claim) });
  assert.throws(() => beforeCpuCase({ state, args: args(state.claims[1]) }), /already started/);
});

test('after hook refuses report/exit, slot, path, duplicate inode, marker and runtime drift', async t => {
  const mutations = [
    (state, context, original) => { original.report.status = 'FAIL'; },
    (state, context, original) => { original.report.tapeSha256 = '0'.repeat(64); },
    (state, context, original) => { original.report.artifacts.pop(); },
    (state, context, original) => { original.report.artifacts[1] = { ...original.report.artifacts[0] }; },
    (state, context, original) => { original.report.artifacts[0].path = '../outside.json'; },
    (state, context, original) => { const target = path.join(state.root, original.report.artifacts[0].path); fs.unlinkSync(target); fs.symlinkSync(original.reportPath, target); },
    (state, context, original) => { const target = path.join(state.root, original.report.artifacts[1].path); fs.unlinkSync(target); fs.linkSync(path.join(state.root, original.report.artifacts[0].path), target); },
    (state, context) => { context.startedAt = '2000-01-01T00:00:00.000Z'; },
    state => { fs.writeFileSync(path.join(state.root, 'sdk-rs/hosts/wasm/pkg/node/safe_cex_wasm.js'), 'substituted'); },
  ];
  for (const mutate of mutations) {
    const options = fixture(t), state = prepareCollection(options), context = beforeCpuCase({ state, args: args(state.claims[0]) }), original = reportFixture(state, state.claims[0]);
    mutate(state, context, original); fs.writeFileSync(original.reportPath, JSON.stringify(original.report)); await pause();
    assert.throws(() => afterCpuCase({ context, reportPath: original.reportPath, exitCode: 0 }));
  }
});

test('source helper rejects dirty/untracked runtime inputs and preserves doc-only quota identity', t => {
  const options = fixture(t), source = readSourceTree(options.root);
  write(options.root, 'docs/new.md', 'untracked documentation'); assert.equal(readSourceTree(options.root).inputsSha256, source.inputsSha256);
  write(options.root, 'sdk-rs/harness/gate.mjs', 'dirty'); assert.throws(() => readSourceTree(options.root), /dirty/);
  git(options.root, 'checkout', '--', 'sdk-rs/harness/gate.mjs');
  write(options.root, 'sdk-rs/harness/unknown.mjs', 'untracked runtime'); assert.throws(() => readSourceTree(options.root), /untracked runtime/);
});

test('combined mode seals actual node module without family CI manifest or ignored ambient files', t => {
  const options = fixture(t, 'bitget');
  write(options.root, 'sdk-rs/hosts/wasm/pkg/ci-replay/build.json', '{"family":"wrong-but-ignored"}');
  const source = readSourceTree(options.root), cases = expectedGateCases('bitget').filter(item => item.tape.includes('/fixtures/'));
  const selection = selectCollectionRuntimeInputs({ root: options.root, source, cases, mode: 'combined' });
  assert.ok(selection.paths.includes('sdk-rs/hosts/wasm/pkg/node/safe_cex_wasm_bg.wasm'));
  assert.ok(!selection.paths.some(item => item.includes('ci-replay') || item.includes('.turbo')));
  const state = prepareCollection({ ...options, mode: 'combined' }); assert.equal(state.mode, 'combined');
});

test('metadata and output collisions fail before producing a claim', t => {
  for (const mutation of [options => { options.run.run_attempt = 2; }, options => { options.job.runner_id = 0; }, options => { options.job.head_sha = '0'.repeat(40); }, options => { options.destination = options.root; }]) {
    const options = fixture(t); mutation(options); assert.throws(() => prepareCollection(options));
    assert.equal(fs.existsSync(path.join(options.root, 'sdk-rs/harness/out/cpu-collection/cohort/cohort.json')), false);
  }
});

test('shared reset and raw capture enforce authenticated manifest paths and exact original slots', t => {
  const options = fixture(t, 'bitget'), state = prepareCollection(options);
  write(options.root, 'sdk-rs/harness/out/cpu-collection/tmp/old-transform', 'old');
  const reset = resetCollectionCache(state); assert.deepEqual(fs.readdirSync(reset.env.TMPDIR), []);
  assert.equal(reset.cacheDigest, reset.initialCacheSha256);
  for (const change of [{ cachePath: 'other-cache' }, { runtimeSha256: '0'.repeat(64) }, { nodePath: '/unsealed/node' }]) assert.throws(() => resetCollectionCache({ ...state, ...change }));
  const original = reportFixture(state, state.claims[0]); assert.equal(capturePublicRaw({ root: state.root, report: original.report }).length, 20);
  original.report.artifacts.reverse(); assert.throws(() => capturePublicRaw({ root: state.root, report: original.report }));
});

test('collection output symlink and rewritten state destination cannot redirect writes', t => {
  const options = fixture(t), external = path.join(path.dirname(options.root), 'outside'); fs.mkdirSync(external);
  // Ignore the link itself so this specifically reaches the output-ancestor
  // guard rather than the independent untracked-runtime rejection.
  fs.appendFileSync(path.join(options.root, '.gitignore'), 'sdk-rs/harness/out\n');
  fs.symlinkSync(external, path.join(options.root, 'sdk-rs/harness/out'));
  assert.throws(() => prepareCollection(options), /output ancestor/);
  assert.deepEqual(fs.readdirSync(external), []);
  fs.unlinkSync(path.join(options.root, 'sdk-rs/harness/out'));
  const state = prepareCollection(options), altered = JSON.parse(fs.readFileSync(state.statePath));
  altered.statePath = path.join(external, 'state.json'); fs.writeFileSync(state.statePath, JSON.stringify(altered));
  assert.throws(() => beforeCpuCase({ state, args: args(state.claims[0]) }), /state\/root path/);
  assert.deepEqual(fs.readdirSync(external), []);
});

test('absolute raw path cannot traverse an ignored symlink before dot-dot', t => {
  const options = fixture(t, 'bitget'), state = prepareCollection(options), original = reportFixture(state, state.claims[0]);
  const outside = path.join(path.dirname(options.root), 'outside-raw'); fs.mkdirSync(path.join(outside, 'inner'), { recursive: true });
  fs.writeFileSync(path.join(outside, 'raw0.json'), '{"impl":"ts","outside":true}');
  const out = path.join(options.root, 'sdk-rs/harness/out');
  fs.writeFileSync(path.join(out, 'raw0.json'), '{"impl":"ts","inside":true}');
  fs.symlinkSync(path.join(outside, 'inner'), path.join(out, 'link'));
  original.report.artifacts[0].path = `${out}/link/../raw0.json`;
  assert.throws(() => capturePublicRaw({ root: options.root, report: original.report }));
});

test('workspace CLI bin links seal their tracked app target and refuse later mutation', t => {
  const options = fixture(t);
  const relativeBin = 'apps/cli/bin/tealstreet.cjs';
  write(options.root, relativeBin, 'module.exports = "fixture CLI entry";\n');
  fs.mkdirSync(path.join(options.root, 'node_modules/.bin'));
  fs.symlinkSync('../../apps/cli/bin/tealstreet.cjs', path.join(options.root, 'node_modules/.bin/tealstreet'));
  git(options.root, 'add', relativeBin);
  git(options.root, '-c', 'user.name=Fixture', '-c', 'user.email=fixture@example.invalid', 'commit', '-qm', 'tracked workspace CLI bin');
  options.run.head_sha = options.job.head_sha = git(options.root, 'rev-parse', 'HEAD');
  const state = prepareCollection(options);
  const manifest = JSON.parse(fs.readFileSync(path.join(options.destination, 'manifest.json')));
  assert(manifest.entries.some(entry => entry.path === relativeBin && entry.type === 'file'));
  assert(manifest.entries.some(entry => entry.path === 'apps/cli/bin' && entry.type === 'directory'));
  const source = readSourceTree(options.root);
  assert(source.selected.some(entry => entry.path === relativeBin));
  fs.appendFileSync(path.join(options.root, relativeBin), '// changed\n');
  assert.throws(() => readSourceTree(options.root), /inputs are dirty/);
  assert.throws(() => beforeCpuCase({ state, args: args(state.claims[0]) }), /collision/);
});

function shallowPullRequest(t, { testedParent = 'second', parentCount = 2 } = {}) {
  const options = fixture(t, 'bitgetusdc'), { root } = options;
  const tree = git(root, 'rev-parse', 'HEAD^{tree}'), tested = options.run.head_sha;
  const commit = (parents, message) => git(root, '-c', 'user.name=Fixture', '-c', 'user.email=fixture@example.invalid',
    'commit-tree', tree, ...parents.flatMap(parent => ['-p', parent]), '-m', message);
  const base = commit([tested], 'base'), other = commit([base], 'other');
  const parents = parentCount === 1 ? [tested] : testedParent === 'first' ? [tested, other] :
    testedParent === 'absent' ? [base, other] : [base, tested];
  const merge = commit(parents, 'synthetic PR checkout');
  git(root, 'update-ref', 'HEAD', merge);
  fs.writeFileSync(path.join(root, '.git/shallow'), `${merge}\n`);
  options.run.event = 'pull_request';
  assert.equal(git(root, 'rev-list', '--parents', '-n', '1', 'HEAD'), merge);
  return { options, merge };
}

test('shallow synthetic PR checkout binds the exact tested second parent from its commit object', t => {
  const { options, merge } = shallowPullRequest(t);
  const state = prepareCollection(options);
  assert.equal(state.claims[0].measuredCheckoutSha, merge);
  assert.equal(state.run.head_sha, options.run.head_sha);
  assert.notEqual(merge, options.run.head_sha);
});

test('shallow PR checkout refuses a tested head present only as the base parent', t => {
  const { options } = shallowPullRequest(t, { testedParent: 'first' });
  assert.throws(() => prepareCollection(options), /measured PR checkout not bound to tested head/);
});

test('shallow PR checkout refuses absent tested parents and single-parent commits', t => {
  for (const value of [{ testedParent: 'absent' }, { parentCount: 1 }]) {
    const { options } = shallowPullRequest(t, value);
    assert.throws(() => prepareCollection(options), /measured PR checkout not bound to tested head/);
  }
});

// Exercise the shell, recorder and collector; only the measurement child is
// scripted, so this checks invocation rather than CPU acceptance.
test('actual post-merge workflow invocation enters and completes the real collector with explicit nine pairs', t => {
  const workflow = fs.readFileSync(new URL('../../.github/workflows/rust.yml', import.meta.url), 'utf8');
  const measured = /^  gate:\n([\s\S]*?)(?=^  bitget-paired:)/m.exec(workflow)[1];
  const publicCommands = measured.split('\n').filter(line => line.includes('rust:gate:record') && line.includes('-public.jsonl.gz'));
  assert.equal(publicCommands.length, 11);
  for (const command of publicCommands) assert.match(command, / --runs 9$/);
  const options = fixture(t, 'bitget');
  const gate = `import fs from 'node:fs'; import path from 'node:path';
    const write = ${write.toString()};
    const performanceOrder = ${performanceOrder.toString()};
    const reportFixture = ${reportFixture.toString()};
    const state = JSON.parse(fs.readFileSync(process.env.CPU_COLLECTION_STATE));
    const claim = state.claims.find(item => item.key === state.active.key);
    const result = reportFixture(state, claim);
    fs.copyFileSync(result.reportPath, process.argv[process.argv.indexOf('--report') + 1]);`;
  write(options.root, 'sdk-rs/harness/gate.mjs', gate);
  git(options.root, 'add', 'sdk-rs/harness/gate.mjs'); git(options.root, '-c', 'user.name=Fixture', '-c', 'user.email=fixture@example.invalid', 'commit', '-qm', 'scripted child report');
  options.run.head_sha = options.job.head_sha = git(options.root, 'rev-parse', 'HEAD');
  const state = prepareCollection(options), claim = state.claims[0];
  const bin = path.join(path.dirname(options.root), 'bin'); fs.mkdirSync(bin);
  const record = fileURLToPath(new URL('./record-gate.mjs', import.meta.url));
  write(path.dirname(options.root), 'bin/corepack', `#!${process.execPath}\nconst c = require('node:child_process').spawnSync(process.execPath, [${JSON.stringify(record)}, ...process.argv.slice(5)], {stdio:'inherit'}); process.exit(c.status ?? 2);\n`);
  fs.chmodSync(path.join(bin, 'corepack'), 0o755);
  const command = publicCommands.find(line => line.includes('matrix.venue')).trim().replace(/^run: /, '')
    .replaceAll('${{ matrix.venue }}', 'bitget').replaceAll('${{ matrix.books }}', claim.case.books);
  const invoke = value => spawnSync('bash', ['-e', '-o', 'pipefail', '-c', value], {
    cwd: options.root, encoding: 'utf8', env: { ...process.env, PATH: `${bin}:${process.env.PATH}`,
      CPU_COLLECTION_STATE: state.statePath, RUST_REPLAY_CORRECTNESS_ONLY: 'false', RUST_REPLAY_PUBLIC_RUNS: '9' },
  });
  for (const mutant of [command.replace(' --runs 9', ''), command.replace(' --runs 9', ' --runs 7')]) {
    const refused = invoke(mutant);
    assert.notEqual(refused.status, 0); assert.match(refused.stderr, /public case\/hash\/runs differ/);
    assert.equal(JSON.parse(fs.readFileSync(state.statePath)).active, null);
  }
  const accepted = invoke(command);
  assert.equal(accepted.status, 0, accepted.stderr);
  const complete = JSON.parse(fs.readFileSync(state.statePath));
  assert.equal(complete.active, null); assert.equal(complete.records.length, 1);
  const evidence = JSON.parse(fs.readFileSync(complete.records[0].evidencePath));
  assert.equal(evidence.report.runs, 9); assert.equal(evidence.raw.length, 20);
  const ledger = fs.readFileSync(path.join(options.root, 'sdk-rs/harness/out/ci/ledger.jsonl'), 'utf8').trim().split('\n');
  assert.equal(ledger.length, 1); assert.equal(JSON.parse(ledger[0]).exitCode, 0);
});

test('Binance Options public input is generated before CPU measurement inputs are sealed', () => {
  const workflow = fs.readFileSync(new URL('../../.github/workflows/rust.yml', import.meta.url), 'utf8');
  const measurement = workflow.slice(workflow.indexOf('\n  gate:\n'), workflow.indexOf('\n  bitget-paired:\n'));
  const generation = measurement.indexOf('corepack yarn rust:private-tape-binanceoptions --public');
  const sealing = measurement.indexOf('node sdk-rs/harness/cpu-ci.mjs --command prepare');
  assert.ok(generation >= 0 && generation < sealing, 'Options public tape must exist before the exact CPU roster is sealed');
  assert.equal(measurement.match(/corepack yarn rust:private-tape-binanceoptions --public/g).length, 1);
  assert.ok(measurement.indexOf('corepack yarn rust:private-tape-binanceoptions\n') > sealing);
});
