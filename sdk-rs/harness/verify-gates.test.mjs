import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { test } from 'node:test';
import { verifyGateSuite, loadGateLedger } from './verify-gates.mjs';
import { expectedGateCases } from './ci-gate-suites.mjs';
import { performanceFailures, performanceOrder } from './performance-policy.mjs';
import { createHash } from 'node:crypto';
const manifest = { schema: 1, failures: [] };
const metric = (amount, mode) => ({
  baselineKind: 'idle-loaded', measuredMode: mode, cpuMs: amount, steadyCpuMs: amount, processCpuMs: amount,
  idleLoadCpuMs: 1, idleLoadProcessCpuMs: 1, idleLoadWallMs: 1,
  runtimeCpuMs: amount - 1, runtimeProcessCpuMs: amount - 1, runtimeWallMs: amount - 1, wallMs: amount,
  heapBeforeBytes: 100, heapUsedBytes: 150, heapRetainedBytes: 50,
  buffersBefore: 0, buffersAfter: 0, bufferRetainedBytes: 0,
  wasmBeforeBytes: 0, wasmBytes: 0, wasmRetainedBytes: 0, retainedBytes: 50,
});
const entries = () => expectedGateCases('mexc').map((item, index) => {
  const privateCase = !item.books;
  const samples = Object.fromEntries(['ts', 'rust'].map((impl) => [impl, Array.from({ length: privateCase ? 1 : 9 }, () => ({
    ...metric(impl === 'ts' ? 200 : 180, privateCase ? 'correctness' : 'performance'),
    ...(impl === 'ts' ? { heapUsedBytes: 200, heapRetainedBytes: 100, retainedBytes: 100 } : {}),
  }))]));
  const report = { ...item, schema: 1, tapeSha256: String(index + 1).repeat(64), private: privateCase,
    runs: privateCase ? 0 : 9, correctnessRuns: 1, requiredImprovement: 0.1, samples, status: 'PASS', failures: [] };
  return { report, exitCode: 0 };
});

test('default verifier preserves strict complete suite coverage', () => {
  const result = verifyGateSuite({ suite: 'mexc', entries: entries(), manifest });
  assert.deepEqual(result.errors, []); assert.equal(result.passes, 2); assert.equal(result.pending.length, 0);
  assert.ok(verifyGateSuite({ suite: 'mexc', entries: entries().slice(1), manifest }).errors.length);
  assert.ok(verifyGateSuite({ suite: 'mexc', entries: [...entries(), entries()[0]], manifest }).errors.length);
});

test('default strict mode refuses CPU failure and collection needs full raw evidence', () => {
  const values = entries(); const entry = values[0];
  for (const sample of entry.report.samples.rust) {
    sample.steadyCpuMs = 183.6;
  }
  entry.report.failures = performanceFailures(entry.report.samples.ts, entry.report.samples.rust).sort();
  entry.report.status = 'FAIL'; entry.exitCode = 1;
  assert.ok(verifyGateSuite({ suite: 'mexc', entries: values, manifest }).errors.length);
  assert.ok(verifyGateSuite({ suite: 'mexc', entries: values, manifest, mode: 'collect' }).errors.length);
});

test('ledger status mismatch, unknown mode and malformed inputs cannot bypass verification', () => {
  const values = entries(); values[0].exitCode = 1;
  assert.ok(verifyGateSuite({ suite: 'mexc', entries: values, manifest }).errors.length);
  assert.throws(() => verifyGateSuite({ suite: 'mexc', entries: entries(), manifest, mode: 'anything' }), /mode/);
  for (const value of [undefined, null, [], [{}], [{ report: null }]])
    assert.ok(verifyGateSuite({ suite: 'mexc', entries: value, manifest }).errors.length);
});

test('invalid or undeclared reports are never labelled as known failures or certified passes', (context) => {
  const directory = mkdtempSync(join(tmpdir(), 'gate-verifier-'));
  context.after(() => rmSync(directory, { recursive: true, force: true }));
  const ledger = join(directory, 'sdk-rs/harness/out/ci');
  mkdirSync(ledger, { recursive: true });
  const reportPath = join(ledger, 'report.json');
  for (const status of ['FAIL', 'PASS']) {
    writeFileSync(reportPath, JSON.stringify({
      schema: 1, venue: 'unregistered-test-venue', tape: 'unknown.jsonl', books: '',
      tapeSha256: 'a'.repeat(64), private: true, status,
      failures: status === 'FAIL' ? ['unlisted divergence'] : [],
      correctnessRuns: 1, runs: 0, samples: { ts: [], rust: [] },
    }));
    writeFileSync(join(ledger, 'ledger.jsonl'), JSON.stringify({ reportPath, exitCode: status === 'PASS' ? 0 : 1 }) + '\n');
    const result = spawnSync(process.execPath, [fileURLToPath(new URL('./verify-gates.mjs', import.meta.url)), '--suite', 'binance'], {
      cwd: directory, encoding: 'utf8', timeout: 10000,
    });
    assert.ifError(result.error);
    assert.equal(result.status, 1);
    assert.match(result.stdout, /UNEXPECTED \/ INVALID RESULT unregistered-test-venue/);
    assert.match(result.stdout, /0 strict passes/);
    assert.match(result.stdout, /0 declared known-failure gates/);
    assert.doesNotMatch(result.stdout, /KNOWN FAILURE \(not parity\)/);
  }
});


test('collecting a declared private incomplete replay does not demand nonexistent raw results', (context) => {
  const directory = mkdtempSync(join(tmpdir(), 'gate-private-incomplete-'));
  context.after(() => rmSync(directory, { recursive: true, force: true }));
  const ledger = join(directory, 'sdk-rs/harness/out/ci'); mkdirSync(ledger, { recursive: true });
  const reportPath = join(ledger, 'private.json');
  const report = { schema: 1, venue: 'fixture', tapeSha256: 'a'.repeat(64), private: true, status: 'FAIL',
    failures: ['ts replay: command Orders did not complete'], runs: 0,
    replayFailure: { kind: 'command-timeout', impl: 'ts', command: 'Orders' },
    coverage: { correctness: 'incomplete', performance: 'not-run' },
    artifacts: [{ impl: 'ts', mode: 'correctness', run: 0, path: join(ledger, 'missing.json') }] };
  writeFileSync(reportPath, JSON.stringify(report));
  writeFileSync(join(ledger, 'ledger.jsonl'), JSON.stringify({ reportPath, exitCode: 1 }) + '\n');
  const loaded = loadGateLedger({ root: directory, includeRaw: true });
  assert.equal(loaded[0].report.status, 'FAIL'); assert.equal(loaded[0].raw, undefined);
  report.private = false; writeFileSync(reportPath, JSON.stringify(report));
  assert.throws(() => loadGateLedger({ root: directory, includeRaw: true }), /ENOENT/);
});

test('enabled Bitget collection retains an eligible miss as unresolved without rewriting strict evidence', () => {
  const item = expectedGateCases('bitget')[0];
  const raw = [];
  const add = (impl, mode, run) => {
    const output = { impl, frames: 10, commands: [], transportReceipt: { frames: 10 }, protocolErrors: [],
      markets: [{ symbol: 'BTCUSDT' }], tickers: [{ symbol: 'BTCUSDT', last: 1 }],
      books: Object.fromEntries(item.books.split(',').map((symbol) => [symbol, { bids: [[1, 1]] }])),
      metrics: metric(impl === 'ts' ? 200 : 183.6, mode) };
    if (impl === 'ts') Object.assign(output.metrics, { heapUsedBytes: 200, heapRetainedBytes: 100, retainedBytes: 100 });
    const bytes = JSON.stringify(output);
    raw.push({ impl, mode, run, bytes, sha256: createHash('sha256').update(bytes).digest('hex') });
  };
  for (const impl of ['ts', 'rust']) add(impl, 'correctness', 0);
  for (let run = 0; run < 9; run++) for (const impl of performanceOrder(run)) add(impl, 'performance', run);
  const samples = Object.fromEntries(['ts', 'rust'].map((impl) => [impl,
    raw.filter((value) => value.impl === impl && value.mode === 'performance').map((value) => JSON.parse(value.bytes).metrics)]));
  const report = { ...item, schema: 1, tapeSha256: 'a'.repeat(64), private: false, runs: 9, correctnessRuns: 1,
    requiredImprovement: .1, samples, fixers: [], status: 'FAIL', failures: performanceFailures(samples.ts, samples.rust).sort(),
    artifacts: raw.map(({ impl, mode, run }) => ({ impl, mode, run, path: `out/${impl}-${mode}-${run}` })) };
  const values = [{ report, raw, exitCode: 1 }, ...expectedGateCases('bitget').slice(1).map((value, index) => ({
    report: { ...entries()[1].report, ...value, tapeSha256: String(index + 1).repeat(64) }, exitCode: 0,
  }))];
  const before = JSON.stringify(values);
  const collected = verifyGateSuite({ suite: 'bitget', entries: values, manifest, mode: 'collect' });
  assert.deepEqual(collected.errors, []);
  assert.equal(collected.pending.length, 1);
  assert.equal(collected.passes, 2, 'only the two ordinary strict private passes count');
  assert.equal(collected.strictFailures, 1);
  assert.equal(collected.rows[0].label, 'UNRESOLVED CPU MISS (not certification)');
  assert.equal(JSON.stringify(values), before, 'strict FAIL, exit1 and original raw remain unchanged');
  assert.ok(verifyGateSuite({ suite: 'bitget', entries: values, manifest }).errors.length, 'default strict still rejects');
  assert.ok(verifyGateSuite({ suite: 'bitgetusdc', entries: values, manifest, mode: 'collect' }).errors.length, 'wrong suite cannot substitute Bitget workload identities');
});
