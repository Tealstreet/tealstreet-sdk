import { createHash } from 'node:crypto';
import { isDeepStrictEqual } from 'node:util';
import { parseCohortJson } from './cpu-cohort.mjs';
import { validatePublicEvidence } from './cpu-remeasure.mjs';
import { assessGateReport, validateKnownFailures } from './known-failure-policy.mjs';
import { median, performanceFailures, performanceObjectives, REQUIRED_IMPROVEMENT } from './performance-policy.mjs';

export const BITGET_PAIRED_TOLERANCE = 1.023110;
export const BITGET_PAIRED_FREEZE_SHA256 = 'ee6b8fd762184969b2eacf7a5ef030bc8872acfbc474ca229377f1b181e99b62';
export const BITGET_CUMULATIVE_ANCHOR = '518d3953c09daf63c1345ea465bbc55bd313f53a';
export const BITGET_MEASUREMENT_FILES = Object.freeze(['sdk-rs/harness/gate.mjs', 'sdk-rs/harness/replay.mjs',
  'sdk-rs/harness/performance-policy.mjs', 'sdk-rs/harness/ci-wasm.mjs',
  'sdk-rs/harness/bitget-paired-steady.mjs', 'sdk-rs/harness/cpu-ci.mjs', 'sdk-rs/harness/revalidate-gate.mjs']);
export const BITGET_COHORT_ORDER = Object.freeze(['baseline', 'candidate', 'candidate', 'baseline',
  'candidate', 'baseline', 'baseline', 'candidate']);
export const BITGET_PAIRED_PROTOCOL_SHA256 = createHash('sha256').update(JSON.stringify({
  order: BITGET_COHORT_ORDER, pairsPerCohort: 9, correctnessPairsPerCohort: 1, attempt: 1,
  statistic: '(median(B Rust36)/median(B TS36))/(median(A Rust36)/median(A TS36))',
  scope: 'bitget:public:steadyCpuMs', freezeSha256: BITGET_PAIRED_FREEZE_SHA256,
  comparisons: ['tip', 'anchor'], cumulativeAnchor: BITGET_CUMULATIVE_ANCHOR,
  measurementCheckout: 'candidate', measurementFiles: BITGET_MEASUREMENT_FILES,
})).digest('hex');
const authorities = new WeakSet();
const certificates = new WeakSet();
const combinedCertificates = new WeakSet();
const same = isDeepStrictEqual;
const sha256 = value => createHash('sha256').update(value).digest('hex');
const oid = value => typeof value === 'string' && /^[a-f0-9]{40}$/.test(value);
const hash = value => typeof value === 'string' && /^[a-f0-9]{64}$/.test(value);
const positive = value => Number.isSafeInteger(value) && value > 0;
const insist = (ok, message) => { if (!ok) throw new Error(`Bitget paired steady: ${message}`); };
const freezeTree = value => {
  if (value && typeof value === 'object') { Object.values(value).forEach(freezeTree); Object.freeze(value); }
  return value;
};
const exactCase = report => report?.venue === 'bitget' && report.private === false &&
  report.tape === 'sdk-rs/harness/fixtures/bitget-public.jsonl.gz' && report.books === 'BTCUSDT,ETHUSDT';

/** Inputs come from authenticated GitHub API responses and a local merge-tree
 * recomputation. A report/PR configuration is never an authority source. */
export function bitgetPairedAuthority({ run, pull, landing, job, checkoutSha, checkoutTree, mergeTree, measurementFiles }) {
  insist(run?.repository?.full_name === 'Tealstreet/tealstreet-sdk' && positive(run.id) &&
    run.run_attempt === 1 && ['pull_request', 'workflow_dispatch', 'push'].includes(run.event), 'original repository run required');
  insist(run.event !== 'push' || run.head_branch === 'master', 'push must be the Rust landing branch');
  let baseline, pullNumber = null;
  if (run.event === 'push' && run.head_branch === 'master') {
    insist(landing?.sha === run.head_sha && checkoutSha === landing.sha && landing.tree?.sha === checkoutTree &&
      oid(landing.parents?.[0]?.sha), 'landing baseline must be the authenticated actual first parent');
    baseline = landing.parents[0].sha;
  } else {
    insist(pull?.base?.repo?.full_name === run.repository.full_name && pull.base.ref === 'master' &&
      oid(pull.base.sha) && oid(pull.head?.sha) && pull.head.sha === run.head_sha && positive(pull.number),
    'baseline must be the authenticated landing base of this exact PR head');
    baseline = pull.base.sha; pullNumber = pull.number;
  }
  insist(oid(checkoutSha) && oid(checkoutTree) && checkoutTree === mergeTree,
    'candidate must be the complete automatic composition on the landing base');
  insist(job?.run_id === run.id && job.run_attempt === 1 && job.head_sha === run.head_sha &&
    job.name === 'Rust / Bitget counterbalanced steady CPU' && positive(job.id) && positive(job.runner_id),
  'missing unique same-run original physical producer');
  insist(measurementFiles && same(Object.keys(measurementFiles).sort(), [...BITGET_MEASUREMENT_FILES].sort()) &&
    Object.values(measurementFiles).every(oid), 'missing trusted common measurement source seals');
  const value = Object.freeze({ measurementFiles: Object.freeze({ ...measurementFiles }), runId: String(run.id), attempt: 1, jobId: String(job.id),
    runnerId: String(job.runner_id), baseline, candidate: checkoutSha,
    candidateTree: checkoutTree, head: run.head_sha, pullNumber, witness: 'tip' });
  authorities.add(value);
  return value;
}

export function bitgetAnchorAuthority(tip) {
  insist(authorities.has(tip) && tip.witness === 'tip', 'anchor requires issued tip authority');
  const value = Object.freeze({ ...tip, baseline: BITGET_CUMULATIVE_ANCHOR, witness: 'anchor' });
  authorities.add(value);
  return value;
}

function checkArchive(archive, source, authority) {
  insist(archive?.source === source && archive.metadata?.sourceHead === source &&
    String(archive.metadata.runId) === authority.runId, 'archive source/run differs');
  insist(archive.files && Object.keys(archive.files).length >= 3 &&
    Object.entries(archive.files).every(([name, digest]) => /^[a-zA-Z0-9_.-]+$/.test(name) && hash(digest)),
  'incomplete archive file seals');
  insist(archive.metadata.hashes && Object.keys(archive.metadata.hashes).length > 0 &&
    Object.entries(archive.metadata.hashes).every(([name, digest]) => archive.files[name] === digest),
  'archive metadata disagrees with independent file seals');
  insist(hash(archive.files['safe_cex_wasm_bg.wasm']) && hash(archive.files['build.json']), 'missing module/metadata seals');
  insist(typeof archive.metadataBytes === 'string' && same(parseCohortJson(archive.metadataBytes), archive.metadata) &&
    sha256(archive.metadataBytes) === archive.files['build.json'], 'archive metadata content differs');
}

function assessContent(authority, evidence, manifest) {
  insist(authorities.has(authority), 'unissued baseline/producer authority');
  insist(validateKnownFailures(manifest).length === 0, 'invalid original known-failure policy');
  insist(evidence?.schema === 1 && evidence.freezeSha256 === BITGET_PAIRED_FREEZE_SHA256 &&
    evidence.attempt === 1 && evidence.runId === authority.runId && evidence.jobId === authority.jobId &&
    evidence.runnerId === authority.runnerId, 'evidence is not bound to the original physical producer');
  insist(!['T', 'tolerance', 'threshold', 'R0', 'r0', 'baselineOverride', 'anchorOverride', 'A0'].some(key => Object.hasOwn(evidence, key)),
    'PR-supplied acceptance authority is forbidden');
  insist(same(evidence.order, BITGET_COHORT_ORDER) && evidence.cohorts?.length === 8, 'complete fixed ABBA/BAAB required');
  insist(evidence.refs?.baseline === authority.baseline && evidence.refs?.candidate === authority.candidate &&
    evidence.candidateTree === authority.candidateTree, 'source/composition differs from authority');
  insist(typeof evidence.cpuModel === 'string' && evidence.cpuModel.trim() &&
    typeof evidence.imageOS === 'string' && evidence.imageOS && typeof evidence.imageVersion === 'string' &&
    evidence.imageVersion && hash(evidence.nodeSha256) && evidence.protocolSha256 === BITGET_PAIRED_PROTOCOL_SHA256,
  'missing or changed runner/runtime/protocol identity');
  insist(evidence.measurementSource === authority.candidate && evidence.measurementTree === authority.candidateTree &&
    same(evidence.measurementFiles, authority.measurementFiles), 'measurement harness differs from trusted candidate checkout');
  for (const arm of ['baseline', 'candidate']) checkArchive(evidence.archives?.[arm], authority[arm], authority);
  const semantics = new Map(), pooled = { baseline: { ts: [], rust: [] }, candidate: { ts: [], rust: [] } };
  const retained = [], unchangedFailures = [];
  let previousEnd = -Infinity, canonicalPath, tapeSha256;
  for (const [index, cohort] of evidence.cohorts.entries()) {
    const arm = BITGET_COHORT_ORDER[index], before = cohort?.before, value = cohort?.evidence, report = value?.report;
    insist(before?.number === index + 1 && before.arm === arm && before.source === authority[arm] &&
      same(before.hashes, evidence.archives[arm].files), 'cohort order/source/module seals differ');
    insist(typeof before.modulePath === 'string' && before.modulePath.startsWith('/'), 'missing canonical module path');
    insist(before.measurementSource === authority.candidate && before.measurementTree === authority.candidateTree &&
      same(before.measurementFiles, authority.measurementFiles) && before.nodeSha256 === evidence.nodeSha256,
    'cohort measurement harness/Node source changed');
    canonicalPath ??= before.modulePath;
    insist(before.modulePath === canonicalPath, 'module path changed between labels');
    const start = Date.parse(before.startedAt), end = Date.parse(cohort.finishedAt);
    insist(Number.isFinite(start) && Number.isFinite(end) && start >= previousEnd && end > start,
      'overlapping/nonfinite/out-of-order cohort windows');
    previousEnd = end;
    insist(exactCase(report), 'only canonical Bitget public steady is eligible');
    insist(typeof value.reportBytes === 'string' && same(parseCohortJson(value.reportBytes), report), 'ambiguous strict report bytes');
    insist(same(report.ciWasm, evidence.archives[arm].metadata), 'report consumed a different archive');
    const contentErrors = validatePublicEvidence(value);
    insist(contentErrors.length === 0, contentErrors.join('; '));
    insist(new Set(report.failures).size === report.failures.length, 'duplicate failure cannot be displaced');
    tapeSha256 ??= report.tapeSha256;
    insist(report.tapeSha256 === tapeSha256, 'tape changed between cohorts');
    const assessment = assessGateReport(report, manifest);
    const calculated = performanceFailures(report.samples.ts, report.samples.rust);
    insist(assessment.errors.every(error => calculated.some(failure => error === `bitget: unexpected ${failure}`)),
      assessment.errors.join('; '));
    const steady = calculated.find(failure => failure.startsWith('performance steady-state CPU:'));
    const residual = assessment.errors.filter(error => error !== `bitget: unexpected ${steady}`);
    // Other objectives remain attached to the original report. This certificate
    // grants no permission to discard them or to certify the complete suite.
    unchangedFailures.push(...residual.map(error => ({ cohort: index + 1, error })));
    for (const slot of value.raw) {
      const { metrics: _metrics, ...output } = parseCohortJson(slot.bytes);
      if (slot.mode === 'correctness') {
        if (semantics.has(slot.impl)) insist(same(output, semantics.get(slot.impl)), 'audited output changed between arms');
        else semantics.set(slot.impl, output);
      }
    }
    for (const impl of ['ts', 'rust']) {
      const samples = report.samples[impl].map(sample => sample.steadyCpuMs);
      insist(samples.length === 9 && samples.every(sample => Number.isFinite(sample) && sample > 0),
        'invalid or incomplete steady samples');
      pooled[arm][impl].push(...samples);
    }
    retained.push({ number: index + 1, arm, status: report.status, failures: [...report.failures],
      reportSha256: sha256(value.reportBytes), raw: value.raw.map(({ impl, mode, run, sha256 }) => ({ impl, mode, run, sha256 })) });
  }
  const summary = Object.fromEntries(Object.entries(pooled).map(([arm, samples]) => {
    insist(samples.ts.length === 36 && samples.rust.length === 36, 'missing pooled samples');
    const ts = median(samples.ts), rust = median(samples.rust);
    return [arm, { tsMedianMs: ts, rustMedianMs: rust, rustTsRatio: rust / ts }];
  }));
  const q = summary.candidate.rustTsRatio / summary.baseline.rustTsRatio;
  insist(Number.isFinite(q) && q > 0, 'nonfinite pooled contrast');
  return { schema: 1, rule: 'bitget-public-steady-counterbalanced-pooled', scope: 'bitget:public:steadyCpuMs',
    canonicalModulePath: canonicalPath,
    measurementIdentity: { source: evidence.measurementSource, tree: evidence.measurementTree, files: evidence.measurementFiles },
    runtimeIdentity: { cpuModel: evidence.cpuModel, imageOS: evidence.imageOS,
      imageVersion: evidence.imageVersion, nodeSha256: evidence.nodeSha256 },
    measurementWindow: { startedAt: evidence.cohorts[0].before.startedAt,
      completedAt: evidence.cohorts.at(-1).finishedAt },
    steadyAccepted: q <= BITGET_PAIRED_TOLERANCE, q, tolerance: BITGET_PAIRED_TOLERANCE,
    freezeSha256: BITGET_PAIRED_FREEZE_SHA256, authority, pooled: summary,
    readiness: { cpuModel: evidence.cpuModel, rustTsRatio: summary.candidate.rustTsRatio,
      limit: 1 - REQUIRED_IMPROVEMENT, ready: summary.candidate.rustTsRatio <= 1 - REQUIRED_IMPROVEMENT },
    unchangedFailures, originalReports: retained, noOverallSuiteCertification: true };
}

/** Fail closed; callers must independently authenticate uploaded bytes/logs
 * and revalidate original strict reports before using this supplement. */
export function assessBitgetPairedSteady({ authority, evidence, manifest }) {
  try {
    const certificate = freezeTree({ ...assessContent(authority, evidence, manifest),
      tapeSha256: evidence.cohorts[0].evidence.report.tapeSha256,
      moduleHashes: Object.freeze({ ...evidence.archives.candidate.metadata.hashes }) });
    certificates.add(certificate);
    return { valid: true, errors: [], certificate };
  }
  catch (error) { return { valid: false, errors: [error.message], certificate: null }; }
}

export function hasPairedBitgetSteady(certificate, report, source) {
  return combinedCertificates.has(certificate) && certificate.steadyAccepted === true && exactCase(report) &&
    report.tapeSha256 === certificate.tapeSha256 && (report.ciWasm?.sourceHead ?? source) === certificate.authority.candidate;
}

export function combineBitgetPairedCertificates(tip, anchor) {
  insist(certificates.has(tip) && certificates.has(anchor) && tip.authority.witness === 'tip' &&
    anchor.authority.witness === 'anchor' && anchor.authority.baseline === BITGET_CUMULATIVE_ANCHOR,
  'both independently issued tip and pinned-anchor witnesses are required');
  for (const field of ['runId', 'attempt', 'jobId', 'runnerId', 'candidate', 'candidateTree', 'head'])
    insist(tip.authority[field] === anchor.authority[field], 'witnesses belong to different candidates or physical jobs');
  insist(tip.tapeSha256 === anchor.tapeSha256 && same(tip.moduleHashes, anchor.moduleHashes) &&
    same(tip.runtimeIdentity, anchor.runtimeIdentity) && same(tip.measurementIdentity, anchor.measurementIdentity) && tip.canonicalModulePath === anchor.canonicalModulePath,
    'witness tape/module/CPU/canonical path differs');
  insist(Date.parse(tip.measurementWindow.completedAt) <= Date.parse(anchor.measurementWindow.startedAt),
    'tip and anchor measurement windows overlap or are reversed');
  const certificate = freezeTree({ schema: 1, rule: 'bitget-public-steady-tip-and-anchor',
    authority: tip.authority, tapeSha256: tip.tapeSha256, moduleHashes: tip.moduleHashes,
    steadyAccepted: tip.steadyAccepted && anchor.steadyAccepted, tolerance: BITGET_PAIRED_TOLERANCE,
    anchorSource: BITGET_CUMULATIVE_ANCHOR, tip, anchor,
    readinessByCpuModel: { cpuModel: tip.readiness.cpuModel,
      comparisons: { tip: tip.readiness, anchor: anchor.readiness } }, noOverallSuiteCertification: true });
  combinedCertificates.add(certificate);
  return certificate;
}

export function pairedSteadyAssessment(assessment, report, certificate, source) {
  if (!hasPairedBitgetSteady(certificate, report, source)) return assessment;
  const steady = performanceFailures(report.samples.ts, report.samples.rust)
    .find(failure => failure.startsWith('performance steady-state CPU:'));
  return { ...assessment, errors: assessment.errors.filter(error => error !== `bitget: unexpected ${steady}`) };
}

/** Collection can retain this unresolved miss; it cannot certify it. */
export function pendingBitgetSteady(evidence, manifest) {
  if (!exactCase(evidence?.report) || validatePublicEvidence(evidence).length) return false;
  const report = evidence.report, failures = performanceFailures(report.samples.ts, report.samples.rust);
  const steady = failures.find(failure => failure.startsWith('performance steady-state CPU:'));
  if (!steady) return false;
  const assessment = assessGateReport(report, manifest);
  const others = performanceObjectives(report.samples.ts, report.samples.rust)
    .filter(objective => ['cpuMs', 'processCpuMs'].includes(objective.field));
  return assessment.errors.length > 0 && others.every(objective => Number.isFinite(objective.ratio) && objective.ratio < 1.05) &&
    assessment.errors.every(error => failures.some(failure => error === `bitget: unexpected ${failure}` &&
      (failure === steady || others.some(({ label }) => failure.startsWith(`performance ${label}:`)))));
}
