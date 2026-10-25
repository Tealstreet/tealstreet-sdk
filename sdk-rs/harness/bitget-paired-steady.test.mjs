import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import test from 'node:test';
import { BITGET_COHORT_ORDER, BITGET_PAIRED_FREEZE_SHA256, BITGET_PAIRED_TOLERANCE, BITGET_PAIRED_PROTOCOL_SHA256,
  assessBitgetPairedSteady, bitgetPairedAuthority, bitgetAnchorAuthority,
  BITGET_CUMULATIVE_ANCHOR, BITGET_MEASUREMENT_FILES, combineBitgetPairedCertificates } from './bitget-paired-steady.mjs';
import { performanceFailures, performanceOrder } from './performance-policy.mjs';
import { assessEligibility, decideRemeasurement } from './cpu-remeasure.mjs';
import { classifyInitialCase, pairedCertificateForRuntime } from './cpu-ci.mjs';

const sha = value => createHash('sha256').update(value).digest('hex');
const A = 'a'.repeat(40), B = 'b'.repeat(40), tree = 'c'.repeat(40), head = 'd'.repeat(40);
const manifest = { schema: 1, failures: [] };
const inputs = () => ({ run: { id: 10, run_attempt: 1, event: 'pull_request', head_sha: head,
  repository: { full_name: 'Tealstreet/tealstreet-sdk' } },
pull: { number: 20, base: { sha: A, ref: 'feat/rust-rewrite', repo: { full_name: 'Tealstreet/tealstreet-sdk' } }, head: { sha: head } },
job: { id: 30, runner_id: 40, run_id: 10, run_attempt: 1, head_sha: head, name: 'Rust / Bitget counterbalanced steady CPU' },
checkoutSha: B, checkoutTree: tree, mergeTree: tree,
measurementFiles: Object.fromEntries(BITGET_MEASUREMENT_FILES.map(path => [path, 'e'.repeat(40)])) });
const metrics = (mode, steady, rust) => ({ baselineKind: 'idle-loaded', measuredMode: mode,
  cpuMs: rust ? 160 : 200, processCpuMs: rust ? 160 : 200, steadyCpuMs: steady,
  idleLoadCpuMs: 1, idleLoadProcessCpuMs: 1, idleLoadWallMs: 1,
  runtimeCpuMs: rust ? 159 : 199, runtimeProcessCpuMs: rust ? 159 : 199, runtimeWallMs: 199, wallMs: 200,
  heapBeforeBytes: 100, heapUsedBytes: rust ? 180 : 200, heapRetainedBytes: rust ? 80 : 100,
  buffersBefore: 0, buffersAfter: 0, bufferRetainedBytes: 0, wasmBeforeBytes: 0, wasmBytes: 0,
  wasmRetainedBytes: 0, retainedBytes: rust ? 80 : 100 });
function fixture(q = 1, rA = .85, candidate = B) {
  const authority = bitgetPairedAuthority({ ...inputs(), checkoutSha: candidate });
  const archives = Object.fromEntries(['baseline', 'candidate'].map(arm => {
    const source = arm === 'baseline' ? A : candidate;
    const files = { 'safe_cex_wasm_bg.wasm': sha(source), 'safe_cex_wasm.js': sha('js'), 'package.json': sha('{}') };
    const metadata = { schema: 1, sourceHead: source, runId: '10', hashes: { ...files } };
    const metadataBytes = JSON.stringify(metadata) + '\n';
    return [arm, { source, metadata, metadataBytes, files: { ...files, 'build.json': sha(metadataBytes) } }];
  }));
  const cohorts = BITGET_COHORT_ORDER.map((arm, index) => {
    const raw = [];
    const add = (impl, mode, run) => {
      const output = { impl, frames: 10, commands: [], transportReceipt: { frames: 10 }, protocolErrors: [],
        markets: [{ symbol: 'BTCUSDT' }], tickers: [{ symbol: 'BTCUSDT', last: 1 }],
        books: { BTCUSDT: { bids: [[1, 1]] }, ETHUSDT: { bids: [[1, 1]] } },
        metrics: metrics(mode, impl === 'ts' ? 200 : 200 * rA * (arm === 'candidate' ? q : 1), impl === 'rust') };
      const bytes = JSON.stringify(output); raw.push({ impl, mode, run, bytes, sha256: sha(bytes) });
    };
    for (const impl of ['ts', 'rust']) add(impl, 'correctness', 0);
    for (let run = 0; run < 9; run++) for (const impl of performanceOrder(run)) add(impl, 'performance', run);
    const samples = Object.fromEntries(['ts', 'rust'].map(impl => [impl, raw.filter(slot => slot.impl === impl &&
      slot.mode === 'performance').map(slot => JSON.parse(slot.bytes).metrics)]));
    const failures = performanceFailures(samples.ts, samples.rust);
    const report = { schema: 1, venue: 'bitget', tape: 'sdk-rs/harness/fixtures/bitget-public.jsonl.gz',
      tapeSha256: sha('tape'), books: 'BTCUSDT,ETHUSDT', private: false, runs: 9, correctnessRuns: 1,
      requiredImprovement: .1, status: failures.length ? 'FAIL' : 'PASS', failures, samples, fixers: [],
      ciWasm: archives[arm].metadata,
      artifacts: raw.map(({ impl, mode, run }) => ({ impl, mode, run, path: `/out/${index}/${impl}-${mode}-${run}` })) };
    return { before: { number: index + 1, arm, source: arm === 'baseline' ? A : candidate,
      hashes: structuredClone(archives[arm].files), modulePath: '/runner/active-module',
      measurementSource: candidate, measurementTree: tree, measurementFiles: authority.measurementFiles, nodeSha256: sha('node'),
      startedAt: new Date(Date.UTC(2026, 9, 26, 0, index * 2)).toISOString() },
    finishedAt: new Date(Date.UTC(2026, 9, 26, 0, index * 2 + 1)).toISOString(),
    evidence: { report, reportBytes: JSON.stringify(report), raw } };
  });
  return { authority, manifest, evidence: { schema: 1, freezeSha256: BITGET_PAIRED_FREEZE_SHA256,
    runId: '10', jobId: '30', runnerId: '40', attempt: 1, refs: { baseline: A, candidate }, candidateTree: tree,
    measurementSource: candidate, measurementTree: tree, measurementFiles: authority.measurementFiles,
    order: [...BITGET_COHORT_ORDER], cpuModel: 'fixture EPYC', imageOS: 'ubuntu24', imageVersion: 'fixture-image',
    nodeSha256: sha('node'), protocolSha256: BITGET_PAIRED_PROTOCOL_SHA256, archives, cohorts } };
}

function anchorFixture(data, mutate = () => {}) {
  const evidence = structuredClone(data.evidence), authority = bitgetAnchorAuthority(data.authority);
  evidence.refs.baseline = BITGET_CUMULATIVE_ANCHOR;
  const archive = evidence.archives.baseline;
  archive.source = BITGET_CUMULATIVE_ANCHOR;
  archive.metadata.hashes['safe_cex_wasm_bg.wasm'] = sha(BITGET_CUMULATIVE_ANCHOR);
  archive.files['safe_cex_wasm_bg.wasm'] = sha(BITGET_CUMULATIVE_ANCHOR);
  archive.metadata.sourceHead = BITGET_CUMULATIVE_ANCHOR;
  archive.metadataBytes = JSON.stringify(archive.metadata) + '\n';
  archive.files['build.json'] = sha(archive.metadataBytes);
  for (const cohort of evidence.cohorts) {
    cohort.before.startedAt = new Date(Date.parse(cohort.before.startedAt) + 3_600_000).toISOString();
    cohort.finishedAt = new Date(Date.parse(cohort.finishedAt) + 3_600_000).toISOString();
    if (cohort.before.arm === 'baseline') {
      cohort.before.source = BITGET_CUMULATIVE_ANCHOR;
      cohort.before.hashes = structuredClone(archive.files);
      cohort.evidence.report.ciWasm = structuredClone(archive.metadata);
      cohort.evidence.reportBytes = JSON.stringify(cohort.evidence.report);
    }
  }
  mutate(evidence);
  return { authority, evidence, manifest };
}
function certificateFor(data) {
  const tip = assessBitgetPairedSteady(data), anchor = assessBitgetPairedSteady(anchorFixture(data));
  assert.equal(tip.valid, true, tip.errors.join('; '));
  assert.equal(anchor.valid, true, anchor.errors.join('; '));
  return combineBitgetPairedCertificates(tip.certificate, anchor.certificate);
}

test('fixed counterbalanced pooled q certifies only steady; .90 readiness is separate by CPU model', () => {
  for (const ratio of [.85, .92]) {
    const data = fixture(1.01, ratio), before = JSON.stringify(data.evidence);
    const result = assessBitgetPairedSteady(data);
    assert.deepEqual(result.errors, []);
    assert.equal(result.certificate.steadyAccepted, true);
    assert.equal(result.certificate.readiness.ready, ratio === .85);
    assert.equal(result.certificate.readiness.cpuModel, 'fixture EPYC');
    assert.equal(result.certificate.noOverallSuiteCertification, true);
    assert.equal(JSON.stringify(data.evidence), before);
    assert.equal(result.certificate.originalReports.length, 8);
  }
});

test('frozen tolerance detects a five-percent slowdown without refitting', () => {
  assert.equal(BITGET_PAIRED_TOLERANCE, 1.023110);
  assert.equal(assessBitgetPairedSteady(fixture(1.05)).certificate.steadyAccepted, false);
  assert.equal(assessBitgetPairedSteady(fixture(1.023)).certificate.steadyAccepted, true);
  assert.equal(assessBitgetPairedSteady(fixture(1.024)).certificate.steadyAccepted, false);
});

test('PR evidence cannot supply baseline authority, R0 or a tolerance override', () => {
  const forged = fixture(); forged.authority = { ...forged.authority };
  assert.equal(assessBitgetPairedSteady(forged).valid, false);
  for (const key of ['T', 'tolerance', 'threshold', 'R0', 'r0', 'baselineOverride', 'anchorOverride', 'A0']) {
    const data = fixture(); data.evidence[key] = key === 'baselineOverride' ? B : 2;
    assert.equal(assessBitgetPairedSteady(data).valid, false, key);
  }
  for (const mutate of [x => { x.run.run_attempt = 2; }, x => { x.pull.head.sha = A; },
    x => { x.pull.base.ref = 'attacker'; }, x => { x.checkoutTree = A; }, x => { x.job.runner_id = 0; },
    x => { x.job.run_attempt = 2; }, x => { x.job.head_sha = A; }]) {
    const data = inputs(); mutate(data); assert.throws(() => bitgetPairedAuthority(data));
  }
});

test('missing, reordered, duplicated, stale or overlapping observations fail closed', () => {
  for (const mutate of [x => { x.cohorts.pop(); }, x => { x.order.reverse(); },
    x => { x.cohorts[2] = x.cohorts[1]; }, x => { x.cohorts[0].before.source = B; },
    x => { x.cohorts[1].before.modulePath += '-B'; }, x => { x.cohorts[1].before.startedAt = x.cohorts[0].before.startedAt; },
    x => { x.measurementSource = A; }, x => { x.measurementTree = A; },
    x => { x.cohorts[0].before.measurementSource = A; }, x => { x.cohorts[0].before.nodeSha256 = sha('wrong'); },
    x => { x.measurementFiles = { ...x.measurementFiles, 'sdk-rs/harness/replay.mjs': A }; },
    x => { x.attempt = 2; }, x => { x.runId = '11'; }, x => { x.runnerId = '41'; },
    x => { x.refs.baseline = B; }, x => { x.candidateTree = A; }, x => { x.freezeSha256 = sha('wrong'); },
    x => { x.cpuModel = ''; }, x => { x.archives.baseline.metadataBytes += ' '; },
    x => { x.protocolSha256 = sha('wrong'); },
    x => { x.cohorts[0].before.hashes['safe_cex_wasm_bg.wasm'] = sha('wrong'); },
    x => { x.cohorts[0].evidence.raw.pop(); }, x => { x.cohorts[0].evidence.report.samples.rust[0].steadyCpuMs = NaN; },
    x => { x.cohorts[0].evidence.report.venue = 'bitgetusdc'; },
    x => { x.cohorts[0].evidence.report.private = true; }]) {
    const data = fixture(); mutate(data.evidence);
    const result = assessBitgetPairedSteady(data);
    assert.equal(result.valid, false, String(mutate));
    assert.equal(result.certificate, null);
  }
});

test('other objectives retain their original failures and are never granted a steady exception', () => {
  const data = fixture();
  for (const cohort of data.evidence.cohorts) {
    for (const slot of cohort.evidence.raw) {
      const output = JSON.parse(slot.bytes);
      if (slot.impl === 'rust') { output.metrics.cpuMs = 190; output.metrics.runtimeCpuMs = 189; }
      slot.bytes = JSON.stringify(output); slot.sha256 = sha(slot.bytes);
      if (slot.mode === 'performance') cohort.evidence.report.samples[slot.impl][slot.run] = output.metrics;
    }
    const report = cohort.evidence.report;
    report.failures = performanceFailures(report.samples.ts, report.samples.rust); report.status = 'FAIL';
    cohort.evidence.reportBytes = JSON.stringify(report);
  }
  const result = assessBitgetPairedSteady(data);
  assert.equal(result.valid, true, result.errors.join("; "));
  assert.equal(result.certificate.steadyAccepted, true);
  assert.equal(result.certificate.unchangedFailures.length, 8);
  assert.ok(result.certificate.unchangedFailures.every(row => row.error.includes('main-thread CPU')));
});

test('certifier consumes an immutable issued certificate; a copied/edited receipt grants nothing', () => {
  const data = fixture(1, .92), certificate = certificateFor(data);
  const first = data.evidence.cohorts[1].evidence;
  delete first.report.ciWasm;
  first.reportBytes = JSON.stringify(first.report);
  first.execution = { measuredCheckoutSha: B };
  assert.equal(classifyInitialCase(first, manifest).decision, 'second');
  assert.equal(classifyInitialCase(first, manifest, certificate).decision, 'first');
  assert.throws(() => classifyInitialCase(first, manifest, { ...certificate }));
  assert.throws(() => { certificate.steadyAccepted = false; });
  assert.throws(() => { certificate.anchor.authority.baseline = B; });
  assert.throws(() => { certificate.anchor.measurementWindow.startedAt = '2020-01-01'; });
  assert.throws(() => { certificate.tip.runtimeIdentity.cpuModel = 'other'; });
  const usdc = structuredClone(first); usdc.report.venue = 'bitgetusdc';
  assert.throws(() => classifyInitialCase(usdc, manifest, certificate));
  first.execution.measuredCheckoutSha = A;
  assert.throws(() => classifyInitialCase(first, manifest, certificate));
});

test('other CPU objectives still require their original two-run mean after steady replacement', () => {
  const data = fixture(1, 1), certificate = certificateFor(data);
  const make = (cpu, second) => {
    const value = structuredClone(data.evidence.cohorts[1].evidence);
    for (const slot of value.raw) {
      const output = JSON.parse(slot.bytes);
      if (slot.impl === 'rust') { output.metrics.cpuMs = cpu; output.metrics.runtimeCpuMs = cpu - 1; }
      slot.bytes = JSON.stringify(output); slot.sha256 = sha(slot.bytes);
      if (slot.mode === 'performance') value.report.samples[slot.impl][slot.run] = output.metrics;
    }
    value.report.failures = performanceFailures(value.report.samples.ts, value.report.samples.rust).sort();
    value.report.status = value.report.failures.length ? 'FAIL' : 'PASS'; value.reportBytes = JSON.stringify(value.report);
    value.execution = { measuredCheckoutSha: B, runId: '10', attempt: 1, jobId: second ? '52' : '51', runnerId: second ? '62' : '61',
      imageOS: 'ubuntu', imageVersion: 'fixture', architecture: 'x64', nodeVersion: 'v22.23.3',
      ...Object.fromEntries(['nodeSha256', 'inputsSha256', 'policySha256', 'runtimeSha256', 'initialCacheSha256', 'finalCacheSha256']
        .map(key => [key, sha(key)])), startedAt: `2026-10-26T00:0${second ? 3 : 1}:00Z`,
      completedAt: `2026-10-26T00:0${second ? 4 : 2}:00Z` };
    return value;
  };
  const first = make(185, false);
  assert.equal(assessEligibility(first).eligible, false, 'steady miss exceeds old eligibility cap');
  assert.equal(assessEligibility(first, certificate).eligible, true);
  assert.equal(classifyInitialCase(first, manifest, certificate).decision, 'second');
  const accepted = decideRemeasurement(first, make(173, true), certificate);
  assert.deepEqual(accepted.errors, []);
  assert.deepEqual(accepted.receipt.objectives.map(row => row.field), ['cpuMs', 'processCpuMs']);
  assert.equal(decideRemeasurement(first, make(185, true), certificate).accepted, false);
});

test('a family certificate cannot authorize a different module or a combined build', () => {
  const certificate = certificateFor(fixture());
  const entries = new Map([['manifest.json', Buffer.from(JSON.stringify({ entries: Object.entries(certificate.moduleHashes)
    .map(([name, sha256]) => ({ path: `sdk-rs/hosts/wasm/pkg/node/${name}`, type: 'file', sha256 })) }))]]);
  assert.equal(pairedCertificateForRuntime({ certificate }, { mode: 'family' }, entries), certificate);
  assert.equal(pairedCertificateForRuntime({ certificate }, { mode: 'combined' }, entries), undefined);
  entries.set('manifest.json', Buffer.from(JSON.stringify({ entries: [] })));
  assert.throws(() => pairedCertificateForRuntime({ certificate }, { mode: 'family' }, entries));
});

test('both independent comparisons are mandatory and cumulative creep blocks a passing tip', () => {
  assert.equal(BITGET_CUMULATIVE_ANCHOR, '518d3953c09daf63c1345ea465bbc55bd313f53a');
  for (const [tipQ, anchorQ, expected] of [[1, 1, true], [1, 1.05, false], [1.05, 1, false]]) {
    const data = fixture(tipQ), tip = assessBitgetPairedSteady(data).certificate;
    const anchor = assessBitgetPairedSteady(anchorFixture(fixture(anchorQ))).certificate;
    assert.equal(combineBitgetPairedCertificates(tip, anchor).steadyAccepted, expected);
  }
  const data = fixture(1, .92), tip = assessBitgetPairedSteady(data).certificate;
  const first = data.evidence.cohorts[1].evidence;
  assert.throws(() => classifyInitialCase(first, manifest, tip), 'single witness cannot displace steady');
  const anchor = assessBitgetPairedSteady(anchorFixture(data)).certificate;
  for (const absent of [null, tip, { ...anchor }]) assert.throws(() => combineBitgetPairedCertificates(tip, absent));
  assert.throws(() => bitgetAnchorAuthority({ ...data.authority }));
});

test('witnesses must share the physical job and canonical path with disjoint ordered windows', () => {
  const data = fixture(), tip = assessBitgetPairedSteady(data).certificate;
  for (const mutate of [x => {
    for (const cohort of x.cohorts) {
      cohort.before.startedAt = new Date(Date.parse(cohort.before.startedAt) - 3_600_000).toISOString();
      cohort.finishedAt = new Date(Date.parse(cohort.finishedAt) - 3_600_000).toISOString();
    }
  }, x => { for (const cohort of x.cohorts) cohort.before.modulePath += '-anchor'; }]) {
    const anchor = assessBitgetPairedSteady(anchorFixture(data, mutate));
    assert.equal(anchor.valid, true, anchor.errors.join('; '));
    assert.throws(() => combineBitgetPairedCertificates(tip, anchor.certificate));
  }
  const foreign = fixture(); foreign.authority = bitgetPairedAuthority({ ...inputs(), job: { ...inputs().job, id: 31 } });
  foreign.evidence.jobId = '31';
  const anchor = assessBitgetPairedSteady(anchorFixture(foreign));
  assert.equal(anchor.valid, true, anchor.errors.join('; '));
  assert.throws(() => combineBitgetPairedCertificates(tip, anchor.certificate));
});

test('identical pinned anchor and candidate passes without advancing the anchor', () => {
  const data = fixture(1, .85, BITGET_CUMULATIVE_ANCHOR);
  const result = assessBitgetPairedSteady(anchorFixture(data));
  assert.equal(result.valid, true, result.errors.join('; '));
  assert.equal(result.certificate.q, 1);
  assert.deepEqual(result.certificate.moduleHashes, anchorFixture(data).evidence.archives.baseline.metadata.hashes);
  assert.equal(data.evidence.archives.candidate.files['safe_cex_wasm_bg.wasm'], sha(BITGET_CUMULATIVE_ANCHOR));
  assert.equal(result.certificate.authority.baseline, result.certificate.authority.candidate);
  assert.equal(certificateFor(data).steadyAccepted, true);
});
