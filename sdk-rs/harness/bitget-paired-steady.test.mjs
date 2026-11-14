import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import test from 'node:test';
import { BITGET_COHORT_ORDER, BITGET_PAIRED_FREEZE_SHA256, BITGET_PAIRED_TOLERANCE, BITGET_PAIRED_PROTOCOL_SHA256,
  assessBitgetPairedSteady, bitgetPairedAuthority, bitgetAnchorAuthority,
  BITGET_CUMULATIVE_ANCHOR, BITGET_MEASUREMENT_FILES, combineBitgetPairedCertificates,
  assessBitgetUnchangedBytes, readBitgetSteadyCertificate, hasPairedBitgetSteady, bitgetSteadyWarning,
  bitgetCpuRejectionRecord, revalidateBitgetCpuRejection, bitgetBaselineObservation, BITGET_STEADY_NOISE, BITGET_STEADY_FAIL_RATIO } from './bitget-paired-steady.mjs';
import { PUBLIC_REPLAY_PAIRS, performanceFailures, performanceOrder } from './performance-policy.mjs';
import { classifyInitialCase, pairedCertificateForRuntime, certifySuites } from './cpu-ci.mjs';
import { readBitgetOracleRefusal } from './bitget-oracle-refusal.mjs';
import { BitgetOracleChanged } from './bitget-paired-oracle.mjs';
import { Frame, SECTION } from '../hosts/wasm/js/wire.mjs';
import { replayPositions } from './bitget-anchor-positions.mjs';

const sha = value => createHash('sha256').update(value).digest('hex');
const A = 'a'.repeat(40), B = 'b'.repeat(40), tree = 'c'.repeat(40), head = 'd'.repeat(40);
const manifest = { schema: 1, failures: [] };
const inputs = () => ({ run: { id: 10, run_attempt: 1, event: 'pull_request', head_sha: head,
  repository: { full_name: 'Tealstreet/tealstreet-sdk' } },
pull: { number: 20, base: { sha: A, ref: 'master', repo: { full_name: 'Tealstreet/tealstreet-sdk' } }, head: { sha: head } },
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
    for (let run = 0; run < PUBLIC_REPLAY_PAIRS; run++) for (const impl of performanceOrder(run)) add(impl, 'performance', run);
    const samples = Object.fromEntries(['ts', 'rust'].map(impl => [impl, raw.filter(slot => slot.impl === impl &&
      slot.mode === 'performance').map(slot => JSON.parse(slot.bytes).metrics)]));
    const failures = performanceFailures(samples.ts, samples.rust);
    const report = { schema: 1, venue: 'bitget', tape: 'sdk-rs/harness/fixtures/bitget-public.jsonl.gz',
      tapeSha256: sha('tape'), books: 'BTCUSDT,ETHUSDT', private: false, runs: PUBLIC_REPLAY_PAIRS, correctnessRuns: 1,
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

/** One strict cohort report with Rust's metrics patched and its failures recomputed. */
function patchedRust(evidence, patch) {
  const value = structuredClone(evidence);
  for (const slot of value.raw) {
    const output = JSON.parse(slot.bytes);
    if (slot.impl === 'rust') Object.assign(output.metrics, patch);
    slot.bytes = JSON.stringify(output); slot.sha256 = sha(slot.bytes);
    if (slot.mode === 'performance') value.report.samples[slot.impl][slot.run] = output.metrics;
  }
  value.report.failures = performanceFailures(value.report.samples.ts, value.report.samples.rust).sort();
  value.report.status = value.report.failures.length ? 'FAIL' : 'PASS'; value.reportBytes = JSON.stringify(value.report);
  value.execution = { measuredCheckoutSha: B };
  return value;
}
// Rust steady 205 ms vs TS 200 ms: a strict steady miss for the paired certificate to decide.
const steadyMiss = (evidence, patch = {}) => patchedRust(evidence, { steadyCpuMs: 205, ...patch });

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
      if (slot.impl === 'rust') { output.metrics.cpuMs = 210; output.metrics.runtimeCpuMs = 209; }
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
  const first = steadyMiss(data.evidence.cohorts[1].evidence);
  delete first.report.ciWasm;
  first.reportBytes = JSON.stringify(first.report);
  assert.equal(classifyInitialCase(first, manifest).decision, 'cpu-refused');
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

test('the steady certificate never waives another CPU objective', () => {
  const data = fixture(1, 1), certificate = certificateFor(data);
  assert.equal(classifyInitialCase(steadyMiss(data.evidence.cohorts[1].evidence, { cpuMs: 190, runtimeCpuMs: 189 }), manifest, certificate).decision, 'first',
    'a main-thread CPU warning (0.95x) beside a certified steady miss certifies');
  assert.equal(classifyInitialCase(steadyMiss(data.evidence.cohorts[1].evidence, { cpuMs: 202, runtimeCpuMs: 201 }), manifest, certificate).decision, 'cpu-refused',
    'main-thread CPU over 1.00x still refuses the CPU gate');
});

test('Bitget paired steady fails beyond measured noise; creep only adds rejections', () => {
  const warn = assessBitgetPairedSteady(fixture(1, .95)).certificate;
  assert.equal(warn.steadyAccepted, true, '0.95x with q in tolerance is accepted');
  assert.equal(warn.readiness.ready, false);
  const combined = certificateFor(fixture(1, .95));
  assert.match(bitgetSteadyWarning(combined), /missed the 10%-faster target \(rust\/ts=0\.950x, target 0\.90x\)/);
  const slower = assessBitgetPairedSteady(fixture(1, 1.02)).certificate;
  assert.equal(slower.notSlowerThanTs, false);
  assert.equal(slower.steadyAccepted, false, '1.02x exceeds measured noise');
  assert.equal(bitgetSteadyWarning(slower), null, 'a failure is not downgraded to a warning');
  const creep = assessBitgetPairedSteady(fixture(1.05, .89 / 1.05)).certificate;
  assert.equal(creep.notSlowerThanTs, true, '0.89x is faster than TS');
  assert.equal(creep.steadyAccepted, false, 'but q over the frozen creep tolerance still fails');
  assert.equal(bitgetSteadyWarning(certificateFor(fixture(1, .85))), null, '0.85x meets the target cleanly');
});

test('measured and unchanged family certificates cannot authorize a different module or combined build', () => {
  for (const certificate of [certificateFor(fixture()), assessBitgetUnchangedBytes(unchangedFixture()).certificate]) {
  const entries = new Map([['manifest.json', Buffer.from(JSON.stringify({ entries: Object.entries(certificate.moduleHashes)
    .map(([name, sha256]) => ({ path: `sdk-rs/hosts/wasm/pkg/node/${name}`, type: 'file', sha256 })) }))]]);
  assert.equal(pairedCertificateForRuntime({ certificate }, { mode: 'family' }, entries), certificate);
  assert.equal(pairedCertificateForRuntime({ certificate }, { mode: 'combined' }, entries), undefined);
  entries.set('manifest.json', Buffer.from(JSON.stringify({ entries: [] })));
  assert.throws(() => pairedCertificateForRuntime({ certificate }, { mode: 'family' }, entries));
  }
});

test('both independent comparisons are mandatory and cumulative creep blocks a passing tip', () => {
  assert.equal(BITGET_CUMULATIVE_ANCHOR, '38b69912e461e27198f5252c98cef73b7521b169');
  for (const [tipQ, anchorQ, expected] of [[1, 1, true], [1, 1.05, false], [1.05, 1, false]]) {
    const data = fixture(tipQ), tip = assessBitgetPairedSteady(data).certificate;
    const anchor = assessBitgetPairedSteady(anchorFixture(fixture(anchorQ))).certificate;
    assert.equal(combineBitgetPairedCertificates(tip, anchor).steadyAccepted, expected);
  }
  const data = fixture(1, .92), tip = assessBitgetPairedSteady(data).certificate;
  const first = steadyMiss(data.evidence.cohorts[1].evidence);
  assert.throws(() => classifyInitialCase(first, manifest, tip), 'single witness cannot displace steady');
  const anchor = assessBitgetPairedSteady(anchorFixture(data)).certificate;
  for (const absent of [null, tip, { ...anchor }]) assert.throws(() => combineBitgetPairedCertificates(tip, absent));
  assert.throws(() => bitgetAnchorAuthority({ ...data.authority }));
});

test('reset changes the protocol seal and refuses pre-reset evidence', () => {
  const previousProtocol = '1b5ac9e04a188ffe6add61f3e9d45a3bdbbb7119ab79820bb538660a1f50bb9d';
  assert.notEqual(BITGET_PAIRED_PROTOCOL_SHA256, previousProtocol);
  const data = anchorFixture(fixture());
  data.evidence.protocolSha256 = previousProtocol;
  const result = assessBitgetPairedSteady(data);
  assert.equal(result.valid, false);
  assert.match(result.errors.join('\n'), /protocol identity/);
});

test('pre-reset anchor cannot replace the newly issued anchor authority', () => {
  const data = anchorFixture(fixture());
  data.evidence.refs.baseline = '518d3953c09daf63c1345ea465bbc55bd313f53a';
  const result = assessBitgetPairedSteady(data);
  assert.equal(result.valid, false);
  assert.match(result.errors.join('\n'), /source\/composition differs/);
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


function unchangedFixture() {
  const data = fixture(1.0343978501725235);
  const original = data.evidence.cohorts[1].evidence;
  const evidence = data.evidence;
  const baseline = evidence.archives.baseline;
  baseline.files['safe_cex_wasm_bg.wasm'] = evidence.archives.candidate.files['safe_cex_wasm_bg.wasm'];
  baseline.metadata.hashes['safe_cex_wasm_bg.wasm'] = baseline.files['safe_cex_wasm_bg.wasm'];
  baseline.metadataBytes = JSON.stringify(baseline.metadata) + '\n';
  baseline.files['build.json'] = sha(baseline.metadataBytes);
  evidence.disposition = 'unchanged-bytes';
  evidence.cohorts = [];
  evidence.case = { venue: 'bitget', tape: original.report.tape, books: original.report.books, private: false };
  evidence.tapeSha256 = original.report.tapeSha256;
  return { ...data, original };
}

test('identical sealed WASM and glue yield an unmeasured disposition without either CPU witness', async () => {
  const data = unchangedFixture(), reads = [];
  const certificate = await readBitgetSteadyCertificate({ authority: data.authority, manifest,
    readEvidence: async (name, selected) => {
      reads.push(name); assert.equal(name, 'tip'); assert.equal(selected, data.authority);
      return data.evidence;
    } });
  assert.deepEqual(reads, ['tip']);
  assert.equal(certificate.disposition, 'unchanged bytes, not measured');
  assert.equal(certificate.measured, false);
  assert.equal(certificate.steadyAccepted, true);
  assert.equal(certificate.noOverallSuiteCertification, true);
  assert.equal(certificate.q, undefined);
  assert.equal(certificate.anchor, undefined);
  assert.equal(certificate.readinessByCpuModel, undefined);
  assert.equal(classifyInitialCase(data.original, manifest, certificate).decision, 'first');
  assert.equal(hasPairedBitgetSteady({ ...certificate }, data.original.report, B), false);
  assert.equal(hasPairedBitgetSteady(certificate, data.original.report, A), true);
  const foreign = structuredClone(data.original.report); foreign.ciWasm.sourceHead = A;
  assert.equal(hasPairedBitgetSteady(certificate, foreign, B), false);
  assert.throws(() => { certificate.moduleHashes['safe_cex_wasm.js'] = sha('changed'); });
});

test('unchanged byte proof fails closed on mismatched packages, seals, source, case or producer', () => {
  const reseal = archive => {
    archive.metadataBytes = JSON.stringify(archive.metadata) + '\n';
    archive.files['build.json'] = sha(archive.metadataBytes);
  };
  for (const mutate of [
    x => { x.authority = { ...x.authority }; },
    x => { x.evidence.attempt = 2; }, x => { x.evidence.runId = '11'; },
    x => { x.evidence.jobId = '31'; }, x => { x.evidence.runnerId = '41'; },
    x => { x.evidence.refs.baseline = B; }, x => { x.evidence.measurementTree = A; },
    x => { x.evidence.measurementFiles = { ...x.evidence.measurementFiles, 'sdk-rs/harness/gate.mjs': A }; },
    x => { x.evidence.cohorts = [{}]; }, x => { x.evidence.case.venue = 'bitgetusdc'; },
    x => { x.evidence.tapeSha256 = ''; },
    x => { delete x.evidence.archives.baseline.files['safe_cex_wasm.js']; },
    x => { x.evidence.archives.baseline.metadata.sourceHead = B; reseal(x.evidence.archives.baseline); },
    ...['safe_cex_wasm_bg.wasm', 'safe_cex_wasm.js'].map(name => x => {
      const archive = x.evidence.archives.baseline;
      archive.files[name] = sha('different bytes'); archive.metadata.hashes[name] = archive.files[name]; reseal(archive);
    }),
    x => { x.evidence.archives.candidate.metadataBytes += ' '; },
  ]) {
    const data = unchangedFixture(); mutate(data);
    assert.equal(assessBitgetUnchangedBytes(data).valid, false);
  }
});

test('unchanged disposition cannot waive correctness, memory or process CPU failures', () => {
  const data = unchangedFixture(), result = assessBitgetUnchangedBytes(data);
  assert.equal(result.valid, true);
  for (const mutate of [
    x => { x.report.failures.push('snapshot mismatch'); x.report.status = 'FAIL'; },
    x => { for (const sample of x.report.samples.rust) sample.retainedBytes = 10000; },
    x => { for (const sample of x.report.samples.rust) sample.processCpuMs = 10000; },
  ]) {
    const invalid = structuredClone(data.original); mutate(invalid);
    assert.throws(() => classifyInitialCase(invalid, manifest, result.certificate));
  }
});

test('changed bytes still require both measured witnesses and the unchanged frozen bound', async () => {
  for (const q of [1, 1.023]) {
    const data = fixture(q), reads = [];
    const certificate = await readBitgetSteadyCertificate({ authority: data.authority, manifest,
      readEvidence: async name => { reads.push(name); return name === 'tip' ? data.evidence : anchorFixture(data).evidence; } });
    assert.deepEqual(reads, ['tip', 'anchor']);
    assert.equal(certificate.tolerance, 1.023110);
    assert.equal(certificate.steadyAccepted, q <= 1.023110);
    assert.equal(certificate.measured, undefined);
    assert.equal(certificate.tip.originalReports.length, 8);
    assert.equal(certificate.anchor.originalReports.length, 8);
  }
});

test('a complete rejected tip never collects an anchor that cannot rescue it', async () => {
  for (const [q, rA] of [[1.024, .85], [1.0343978501725235, .85],
    [1.0278628752799848 / 1.0225747088093846, 1.0225747088093846]]) {
    const data = fixture(q, rA), reads = [];
    await assert.rejects(readBitgetSteadyCertificate({ authority: data.authority, manifest,
      readEvidence: async name => {
        reads.push(name);
        assert.equal(name, 'tip', 'rejected tip must not start an anchor build or replay');
        return data.evidence;
      } }), error => {
      assert.equal(error.disposition, 'REJECTED');
      assert.equal(error.certificate, null, 'a lone rejected witness is never a certificate');
      assert.equal(error.tip.steadyAccepted, false);
      assert.equal(error.tip.originalReports.length, 8, 'validate the complete tip before rejecting');
      return true;
    });
    assert.deepEqual(reads, ['tip']);
  }
});

test('a byte-changed build certifies against the original anchor with its empty legacy positions', async () => {
  const data = fixture(1.01), reads = [];
  assert.notEqual(data.evidence.archives.baseline.files['safe_cex_wasm_bg.wasm'],
    data.evidence.archives.candidate.files['safe_cex_wasm_bg.wasm']);
  const certificate = await readBitgetSteadyCertificate({ authority: data.authority, manifest,
    readEvidence: async name => {
      reads.push(name);
      // The collector must decode the selected module before it can return
      // measured evidence. This used to throw for the pinned anchor even with
      // zero positions; changed bytes could never reach certification.
      const buffer = new ArrayBuffer(32), view = new DataView(buffer);
      view.setUint32(0, 0x46584353, true);
      view.setUint16(4, 1, true); view.setUint16(6, 1, true);
      view.setUint16(16, SECTION.POSITIONS, true);
      view.setUint16(18, name === 'anchor' ? 1 : 2, true);
      view.setUint32(20, 8, true);
      assert.equal(replayPositions(new Frame(buffer), 'bitget', false).count, 0);
      return name === 'tip' ? data.evidence : anchorFixture(data).evidence;
    } });
  assert.deepEqual(reads, ['tip', 'anchor']);
  assert.equal(certificate.anchorSource, BITGET_CUMULATIVE_ANCHOR);
  assert.equal(certificate.steadyAccepted, true);
  assert.equal(certificate.tip.originalReports.length, 8);
  assert.equal(certificate.anchor.originalReports.length, 8);
  assert.equal(hasPairedBitgetSteady(certificate, data.evidence.cohorts[1].evidence.report, B), true);
});

test('an unchanged TS oracle still certifies, while baseline Rust corruption still fails', async () => {
  const { assertBitgetOracleUnchanged } = await import('./bitget-paired-oracle.mjs');
  const data = fixture();
  const rawOracle = JSON.parse(data.evidence.cohorts[0].evidence.raw.find(slot => slot.impl === 'ts' && slot.mode === 'correctness').bytes);
  const oracle = JSON.stringify({ ...rawOracle, checkpoints: [] });
  assertBitgetOracleUnchanged({ baseline: oracle, candidate: oracle });
  assert.equal(certificateFor(data).steadyAccepted, true);
  const slot = data.evidence.cohorts[0].evidence.raw.find(slot => slot.impl === 'rust' && slot.mode === 'correctness');
  const corrupted = JSON.parse(slot.bytes);
  corrupted.markets[0].limits = { amount: { max: 999999 } };
  slot.bytes = JSON.stringify(corrupted); slot.sha256 = sha(slot.bytes);
  const result = assessBitgetPairedSteady(data);
  assert.equal(result.valid, false);
  assert.equal(result.certificate, null);
});

function refusalFixture(witness = 'tip') {
  const data = fixture(), authority = data.authority;
  const selected = witness === 'anchor' ? bitgetAnchorAuthority(authority) : authority;
  const value = JSON.parse(data.evidence.cohorts[0].evidence.raw[0].bytes);
  value.checkpoints = [];
  const baseline = JSON.stringify({ ...value, markets: [{ symbol: 'BTCUSDT', max: 1200 }] });
  const candidate = JSON.stringify({ ...value, markets: [{ symbol: 'BTCUSDT', max: 220 }] });
  const sourceTrees = { [selected.baseline]: 'f'.repeat(40), [selected.candidate]: tree };
  const receipt = { runId: authority.runId, jobId: authority.jobId, runnerId: authority.runnerId, witness,
    sources: { baseline: { source: selected.baseline, tree: sourceTrees[selected.baseline], outputSha256: sha(baseline) },
      candidate: { source: selected.candidate, tree, outputSha256: sha(candidate) } },
    tape: 'sdk-rs/harness/fixtures/bitget-public.jsonl.gz', tapeSha256: sha('tape'),
    books: 'BTCUSDT,ETHUSDT', nodeSha256: sha('node') };
  const result = { valid: false, disposition: 'NOT-CERTIFIED', certified: false, certificate: null,
    errors: [new BitgetOracleChanged().message], oracle: receipt };
  const entries = new Map(Object.entries({ 'steady-assessment.json': result,
    [`${witness}/oracle-receipt.json`]: receipt,
    [`${witness}/oracle-baseline.json`]: baseline, [`${witness}/oracle-candidate.json`]: candidate })
    .map(([name, value]) => [name, Buffer.from(typeof value === 'string' ? value : JSON.stringify(value))]));
  return { data, entries, authority, sourceTrees, tapeSha256: sha('tape') };
}

test('issued tip and anchor oracle refusals stay non-certified and cannot supply steady authority', () => {
  for (const witness of ['tip', 'anchor']) {
    const x = refusalFixture(witness), refusal = readBitgetOracleRefusal(x);
    assert.equal(refusal.disposition, 'NOT-CERTIFIED');
    assert.equal(refusal.certified, false);
    assert.equal(refusal.certificate, null);
    assert.equal(refusal.witness, witness);
    assert.equal(pairedCertificateForRuntime({ refusal }, { mode: 'family' }, new Map()), undefined);
  }
});

test('refusal content binds actual oracle difference, exact receipt, source tree, run and raw bytes', () => {
  for (const mutate of [x => { x.authority = { ...x.authority }; },
    x => { x.sourceTrees[x.authority.candidate] = '9'.repeat(40); },
    x => { x.entries.set('tip/oracle-candidate.json', x.entries.get('tip/oracle-baseline.json')); },
    x => { x.entries.set('tip/oracle-receipt.json', Buffer.from('{}')); },
    x => { x.entries.set('tip/records/plan.json', Buffer.from('{}')); },
    x => { const v = JSON.parse(x.entries.get('steady-assessment.json')); v.certified = true; x.entries.set('steady-assessment.json', Buffer.from(JSON.stringify(v))); },
    x => { const v = JSON.parse(x.entries.get('steady-assessment.json')); v.oracle.runId = '99'; x.entries.set('steady-assessment.json', Buffer.from(JSON.stringify(v))); },
    x => { x.entries.set('tip/oracle-candidate.json', Buffer.from('{"impl":"ts","impl":"rust"}')); }]) {
    const x = refusalFixture(); mutate(x); assert.throws(() => readBitgetOracleRefusal(x));
  }
});

test('a sealed metrics-only oracle difference is not a legitimate refusal', () => {
  const x = refusalFixture(), raw = JSON.parse(x.entries.get('tip/oracle-baseline.json'));
  raw.metrics.cpuMs += 1;
  const bytes = Buffer.from(JSON.stringify(raw));
  x.entries.set('tip/oracle-candidate.json', bytes);
  const result = JSON.parse(x.entries.get('steady-assessment.json'));
  result.oracle.sources.candidate.outputSha256 = sha(bytes);
  x.entries.set('steady-assessment.json', Buffer.from(JSON.stringify(result)));
  x.entries.set('tip/oracle-receipt.json', Buffer.from(JSON.stringify(result.oracle)));
  assert.throws(() => readBitgetOracleRefusal(x), /no actual published oracle difference/);
});

test('a valid refusal plans no second but never hides corruption or memory failure', () => {
  const x = refusalFixture(), refusal = readBitgetOracleRefusal(x);
  const evidence = structuredClone(x.data.evidence.cohorts[0].evidence);
  evidence.execution = { measuredCheckoutSha: x.authority.candidate };
  assert.equal(classifyInitialCase(evidence, manifest, undefined, undefined, refusal).decision, 'not-certified');
  assert.equal(classifyInitialCase(evidence, manifest).decision, 'first', 'ordinary success stays unchanged');
  assert.throws(() => classifyInitialCase(evidence, manifest, undefined, undefined, { ...refusal }));
  const corrupted = structuredClone(evidence); corrupted.raw[0].bytes += ' ';
  assert.throws(() => classifyInitialCase(corrupted, manifest, undefined, undefined, refusal));
  const memory = structuredClone(evidence);
  for (const slot of memory.raw.filter(slot => slot.impl === 'rust')) {
    const output = JSON.parse(slot.bytes); output.metrics.heapUsedBytes = 250;
    output.metrics.heapRetainedBytes = output.metrics.retainedBytes = 150;
    slot.bytes = JSON.stringify(output); slot.sha256 = sha(slot.bytes);
    if (slot.mode === 'performance') memory.report.samples.rust[slot.run] = output.metrics;
  }
  memory.report.failures = performanceFailures(memory.report.samples.ts, memory.report.samples.rust);
  memory.report.status = 'FAIL'; memory.reportBytes = JSON.stringify(memory.report);
  assert.throws(() => classifyInitialCase(memory, manifest, undefined, undefined, refusal));
});

test('Bitget steady distinguishes measured baseline noise from a slowdown beyond that noise', () => {
  for (const ratio of [1, 1.01, 1.015369697275649, 1.01725]) {
    const result = assessBitgetPairedSteady(fixture(1, ratio));
    assert.equal(result.valid, true, result.errors.join('\n'));
    assert.equal(result.certificate.steadyAccepted, true, `${ratio} is within observed baseline variance`);
  }
  for (const ratio of [1.01726, 1.0203509277368097, 1.03]) {
    assert.equal(assessBitgetPairedSteady(fixture(1, ratio)).certificate.steadyAccepted, false, `${ratio} exceeds noise`);
  }
});

test('master push and nightly cannot use the unmeasured unchanged-bytes shortcut', () => {
  for (const event of ['push', 'workflow_dispatch']) {
    const data = unchangedFixture(), source = inputs();
    Object.assign(source.run, { event, head_branch: 'master', head_sha: B });
    source.job.head_sha = B;
    source.landing = { sha: B, tree: { sha: tree }, parents: [{ sha: A }] };
    data.authority = bitgetPairedAuthority(source);
    const result = assessBitgetUnchangedBytes(data);
    assert.equal(result.valid, false, 'master baseline must actually be measured');
    assert.match(result.errors.join('\n'), /master.*measur/i);
  }
});

for (const witness of ['tip', 'anchor']) test(`a complete failed ${witness} yields a recomputed CPU verdict, never an oracle refusal`, async () => {
  const data = fixture(witness === 'tip' ? 1.05 : 1, .85);
  const anchor = anchorFixture(data);
  if (witness === 'anchor') {
    for (const cohort of anchor.evidence.cohorts.filter(row => row.before.arm === 'baseline')) {
      cohort.evidence = patchedRust(cohort.evidence, { steadyCpuMs: 150 });
    }
  }
  const options = { authority: data.authority, manifest,
    readEvidence: async name => name === 'tip' ? data.evidence : anchor.evidence };
  let original;
  try { await readBitgetSteadyCertificate(options); } catch (error) { original = error; }
  assert.equal(original.disposition, 'REJECTED');
  const record = bitgetCpuRejectionRecord(original);
  const args = { ...options, record: structuredClone(record), message: original.message };
  const result = await revalidateBitgetCpuRejection(args);
  assert.equal(result.message, original.message);
  assert.equal(record.certified, false);
  assert.equal(record.certificate, null);
  assert.equal(record.tip.steadyAccepted, witness === 'anchor');
  const { results } = await certifySuites(['bitget', 'okx'], async suite => {
    if (suite === 'bitget') throw await revalidateBitgetCpuRejection(args);
    return { suite };
  });
  assert.deepEqual(results.suites, { bitget: false, okx: true });
  assert.equal(results.certified, false);
  assert.equal(results.reasons.bitget, original.message, 'the final CPU gate carries the actual failed comparison');

  for (const change of [
    x => { x.record.tip.q += .01; },
    x => { x.record.certified = true; },
    x => { x.message += ' modified'; },
    x => { x.record.errors = ['not a CPU verdict']; },
  ]) {
    const bad = { ...args, record: structuredClone(record) }; change(bad);
    await assert.rejects(revalidateBitgetCpuRejection(bad), /differs from recomputed/);
  }
  // A log line/payload cannot supply a verdict if any raw cohort is missing or corrupt.
  const incomplete = structuredClone(data.evidence); incomplete.cohorts.pop();
  await assert.rejects(revalidateBitgetCpuRejection({ ...args, readEvidence: async () => incomplete }), /did not reproduce/);
  const corrupt = structuredClone(data.evidence); corrupt.cohorts[0].evidence.raw[0].bytes += 'x';
  await assert.rejects(revalidateBitgetCpuRejection({ ...args, readEvidence: async () => corrupt }), /did not reproduce/);
  const accepted = fixture();
  await assert.rejects(revalidateBitgetCpuRejection({ ...args, authority: accepted.authority,
    readEvidence: async name => name === 'tip' ? accepted.evidence : anchorFixture(accepted).evidence }), /recomputed as accepted/);
});

test('the fixed Bitget allowance records its observed range without changing global strict objectives', () => {
  assert.equal(BITGET_STEADY_NOISE, 458.32 / 450.66 - 469.5715 / 469.6945);
  assert.equal(BITGET_STEADY_FAIL_RATIO, 1 + BITGET_STEADY_NOISE);
  const data = fixture(1, 1.01), cert = certificateFor(data);
  assert.equal(cert.steadyAccepted, true);
  assert.equal(cert.tip.notSlowerThanTs, false, 'retain the literal comparison separately from noise');
  assert.equal(cert.tip.withinSteadyNoise, true);
  const report = data.evidence.cohorts[0].evidence.report;
  assert(performanceFailures(report.samples.ts, report.samples.rust).some(x => x.startsWith('performance steady-state CPU:')),
    'raw reports and every other venue keep strict policy');
});

test('master telemetry records the landed and parent ratios even on a failed tip, never certifies', () => {
  for (const ratio of [.99, 1.03]) {
    const data = fixture(1, ratio), source = inputs();
    Object.assign(source.run, { event: 'push', head_branch: 'master', head_sha: B });
    source.job.head_sha = B;
    source.landing = { sha: B, tree: { sha: tree }, parents: [{ sha: A }] };
    data.authority = bitgetPairedAuthority(source);
    const tip = assessBitgetPairedSteady(data).certificate;
    const observation = bitgetBaselineObservation(data.authority, tip);
    assert.equal(observation.masterSource, B);
    assert.equal(observation.baselineSource, A);
    assert.equal(observation.masterRustTsRatio, tip.pooled.candidate.rustTsRatio);
    assert.equal(observation.baselineRustTsRatio, tip.pooled.baseline.rustTsRatio);
    assert.equal(observation.steadyAccepted, ratio < 1);
    assert.equal(observation.certified, false);
    assert.equal(observation.runId, data.authority.runId);
    assert(Object.isFrozen(observation));
    assert.throws(() => bitgetBaselineObservation(data.authority, structuredClone(tip)), /requires measured master/);
  }
  const pr = fixture();
  assert.throws(() => bitgetBaselineObservation(pr.authority, assessBitgetPairedSteady(pr).certificate), /requires measured master/);
});
