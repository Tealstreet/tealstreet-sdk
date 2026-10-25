// Collection remains strict. This separate policy can certify exactly two
// complete public measurements; it never rewrites either strict report.
import { createHash } from 'node:crypto';
import { isDeepStrictEqual } from 'node:util';
import { memoryMetricErrors } from './memory.mjs';
import { compareSnapshots, snapshotCoverageErrors } from './snapshot-compare.mjs';
import { sameCommandResults } from './tape.mjs';
import { CPU_FIELDS, performanceFailures, performanceMetricErrors, performanceObjectives, performanceOrder } from './performance-policy.mjs';
import { hasPairedBitgetSteady } from './bitget-paired-steady.mjs';

export const CPU_REMEASURE_RULE = 'all-objective-normalized-two-run-mean-v1';
const sha256 = (bytes) => createHash('sha256').update(bytes).digest('hex');
const slotKey = (value) => JSON.stringify([value?.impl, value?.mode, value?.run]);
const expectedSlots = () => [
  ...['ts', 'rust'].map((impl) => ({ impl, mode: 'correctness', run: 0 })),
  ...Array.from({ length: 9 }, (_, run) => performanceOrder(run).map((impl) => ({ impl, mode: 'performance', run }))).flat(),
];
const positiveId = (value) => typeof value === 'string' && /^[1-9]\d*$/.test(value);
const digest = (value) => typeof value === 'string' && /^[a-f0-9]{64}$/.test(value);
const same = isDeepStrictEqual;
const nonmetric = ({ metrics: _metrics, ...output }) => output;
const decode = (slot) => JSON.parse(slot.bytes);
const objectives = (report, paired, source) => performanceObjectives(report.samples.ts, report.samples.rust)
  .filter(({ field }) => CPU_FIELDS.includes(field) && !(field === 'steadyCpuMs' && hasPairedBitgetSteady(paired, report, source)));

/** Validate every raw slot before trusting report samples or failure strings. */
function validatePublicEvidenceContent(value) {
  const errors = [];
  const report = value?.report;
  if (!report || report.schema !== 1 || report.private !== false || report.runs !== 9 || report.correctnessRuns !== 1 ||
      report.requiredImprovement !== 0.10 || !['PASS', 'FAIL'].includes(report.status) ||
      !Array.isArray(report.failures) || report.failures.some((item) => typeof item !== 'string') ||
      (report.status === 'PASS') !== (report.failures.length === 0) || report.replayFailure ||
      typeof report.venue !== 'string' || !report.venue || typeof report.tape !== 'string' || !report.tape ||
      typeof report.books !== 'string' || !digest(report.tapeSha256) || !Array.isArray(report.fixers))
    return ['invalid complete public strict report'];
  if (value.reportBytes !== undefined && (typeof value.reportBytes !== 'string' || !same(JSON.parse(value.reportBytes), report)))
    return ['original strict report bytes disagree with parsed content'];
  if (!Array.isArray(value.raw) || value.raw.length !== 20 || !Array.isArray(report.artifacts) || report.artifacts.length !== 20)
    return ['missing complete audited/nine-pair raw inventory'];
  const paths = report.artifacts.map((slot) => slot?.path);
  if (paths.some((path) => typeof path !== 'string' || !path) || new Set(paths).size !== paths.length)
    return ['raw artifact paths are missing or duplicated'];
  const expected = expectedSlots();
  const audited = {};
  for (let index = 0; index < expected.length; index++) {
    const slot = value.raw[index];
    if (!slot || !same(slotKey(slot), slotKey(expected[index])) || !same(slotKey(report.artifacts[index]), slotKey(expected[index]))) {
      errors.push(`invalid interleaved raw slot ${index}`); continue;
    }
    if (typeof slot.bytes !== 'string' || !digest(slot.sha256) || sha256(slot.bytes) !== slot.sha256) {
      errors.push(`raw hash mismatch at slot ${index}`); continue;
    }
    let output;
    try { output = decode(slot); } catch { errors.push(`invalid raw JSON at slot ${index}`); continue; }
    if (!output || Array.isArray(output) || output.impl !== slot.impl || !Object.hasOwn(output, 'metrics')) {
      errors.push(`invalid raw output at slot ${index}`); continue;
    }
    errors.push(...snapshotCoverageErrors(output, `slot ${index}`, { isPrivate: false, venue: report.venue }));
    if (!Number.isSafeInteger(output.frames) || output.frames <= 0 || !Array.isArray(output.commands) || !output.transportReceipt ||
        !Array.isArray(output.protocolErrors) || output.protocolErrors.length || output.unserved || output.missingWrites ||
        output.unusedWrites || output.invalidSignatures || !output.markets?.length || !output.tickers?.length)
      errors.push(`non-CPU workload/protocol failure at slot ${index}`);
    for (const symbol of report.books.split(',').filter(Boolean)) if (!output.books?.[symbol])
      errors.push(`requested book ${symbol} missing at slot ${index}`);
    if (slot.mode === 'correctness') audited[slot.impl] = output;
    else if (audited[slot.impl]) {
      const original = audited[slot.impl];
      errors.push(...compareSnapshots(original, output, `performance slot ${index}`));
      if (output.frames !== original.frames || !sameCommandResults(original.commands, output.commands) || !same(original.transportReceipt, output.transportReceipt))
        errors.push(`incomplete performance workload at slot ${index}`);
    }
    errors.push(...memoryMetricErrors(output.metrics).map((error) => `slot ${index}: ${error}`),
      ...performanceMetricErrors(output.metrics, slot.mode).map((error) => `slot ${index}: ${error}`));
    if (slot.mode === 'performance' && !same(report.samples?.[slot.impl]?.[slot.run], output.metrics))
      errors.push(`raw samples disagree with report at slot ${index}`);
  }
  if (audited.ts && audited.rust) {
    if (audited.ts.frames !== audited.rust.frames) errors.push('audited frame counts differ');
  }
  for (const impl of ['ts', 'rust']) if (!Array.isArray(report.samples?.[impl]) || report.samples[impl].length !== 9)
    errors.push(`${impl}: missing nine complete samples`);
  return errors;
}

export function validatePublicEvidence(value) {
  try { return validatePublicEvidenceContent(value); }
  catch (error) { return [`malformed public evidence: ${error.message}`]; }
}

function cpuOnlyErrors(value, requireMiss, paired) {
  const errors = validatePublicEvidence(value);
  if (errors.length) return errors;
  const report = value.report, source = value.execution?.measuredCheckoutSha;
  if (paired !== undefined && !hasPairedBitgetSteady(paired, report, source)) return ['unissued or mismatched paired steady certificate'];
  const calculated = performanceFailures(report.samples.ts, report.samples.rust).sort();
  // Exact generated failures, not a performance-prefix allowlist. Memory never
  // becomes an averaged objective and declarations cannot excuse correctness.
  const cpuFailures = calculated.filter((failure) => objectives(report).some(({ label }) => failure.startsWith(`performance ${label}:`)));
  if (!same(calculated, cpuFailures) || !same([...report.failures].sort(), cpuFailures))
    errors.push('strict failures include non-CPU diagnostics or omit/repeat computed failures');
  if (requireMiss && !cpuFailures.some(failure => objectives(report, paired, source).some(({ label }) => failure.startsWith(`performance ${label}:`))))
    errors.push('first measurement has no remaining CPU miss');
  for (const objective of objectives(report, paired, source)) if (!Number.isFinite(objective.ratio) || objective.ratio >= 1.05)
    errors.push(`${objective.field}: CPU ratio must be strictly below 1.05`);
  return errors;
}

export function assessEligibility(first, paired) {
  const errors = cpuOnlyErrors(first, true, paired);
  return { eligible: errors.length === 0, errors, objectives: errors.length ? [] : objectives(first.report, paired, first.execution?.measuredCheckoutSha) };
}

export function validateSecondExecution(first, second) {
  const errors = [];
  const a = first?.execution, b = second?.execution;
  for (const [name, execution] of [['first', a], ['second', b]]) {
    if (!execution || !positiveId(execution.runId) || !positiveId(execution.jobId) || !positiveId(execution.runnerId) || execution.attempt !== 1) {
      errors.push(`${name}: invalid physical execution identity/attempt`); continue;
    }
    for (const field of ['nodeSha256', 'inputsSha256', 'policySha256', 'runtimeSha256', 'initialCacheSha256', 'finalCacheSha256'])
      if (!digest(execution[field])) errors.push(`${name}: missing ${field}`);
    for (const field of ['imageOS', 'imageVersion', 'architecture', 'nodeVersion'])
      if (typeof execution[field] !== 'string' || !execution[field]) errors.push(`${name}: missing ${field}`);
    const start = Date.parse(execution.startedAt), end = Date.parse(execution.completedAt);
    if (!Number.isFinite(start) || !Number.isFinite(end) || !(end > start)) errors.push(`${name}: invalid execution window`);
  }
  if (!a || !b) return errors;
  if (a.runId !== b.runId || a.jobId === b.jobId || a.runnerId === b.runnerId)
    errors.push('remeasurement must use a different physical job/runner in the original cohort');
  if (!(Date.parse(b.startedAt) >= Date.parse(a.completedAt))) errors.push('measurement execution windows overlap or are invalid');
  for (const field of ['imageOS', 'imageVersion', 'architecture', 'nodeVersion', 'nodeSha256', 'inputsSha256', 'policySha256',
    'runtimeSha256', 'initialCacheSha256', 'finalCacheSha256'])
    if (!same(a[field], b[field])) errors.push(`second execution changed ${field}`);
  return errors;
}

/** Caller must also authenticate live cohort/artifact provenance. Pure content
 * checks are deliberately insufficient authority to certify a workflow. */
export function decideRemeasurement(first, second, paired) {
  const errors = [...cpuOnlyErrors(first, true, paired), ...cpuOnlyErrors(second, false, paired), ...validateSecondExecution(first, second)];
  if (errors.length) return { accepted: false, errors, receipt: null };
  for (const field of ['venue', 'tape', 'books', 'tapeSha256', 'runs', 'correctnessRuns', 'requiredImprovement', 'fixers'])
    if (!same(first.report[field], second.report[field])) errors.push(`second strict report changed ${field}`);
  for (let index = 0; index < first.raw.length; index++)
    if (!same(nonmetric(decode(first.raw[index])), nonmetric(decode(second.raw[index]))))
      errors.push(`nonmetric output differs at slot ${index}`);
  const a = objectives(first.report, paired, first.execution?.measuredCheckoutSha), b = objectives(second.report, paired, second.execution?.measuredCheckoutSha);
  const aggregate = a.map((objective, index) => {
    const other = b[index], mean = (objective.ratio + other.ratio) / 2;
    if (objective.category !== other.category) errors.push(`${objective.field}: small-window category changed`);
    if (!(mean <= 1)) errors.push(`${objective.field}: two-run mean exceeds existing bound`);
    return { field: objective.field, category: objective.category, first: objective, second: other, mean };
  });
  if (errors.length) return { accepted: false, errors, receipt: null };
  return { accepted: true, errors: [], receipt: {
    schema: 1, rule: CPU_REMEASURE_RULE,
    case: { venue: first.report.venue, tape: first.report.tape, tapeSha256: first.report.tapeSha256, books: first.report.books },
    objectives: aggregate,
    executions: [first.execution, second.execution].map((value) => structuredClone(value)),
    strictReports: [first, second].map((value) => ({ status: value.report.status,
      contentSha256: sha256(JSON.stringify(value.report)),
      ...(typeof value.reportBytes === 'string' ? { sha256: sha256(value.reportBytes) } : {}),
    })),
    raw: [first.raw, second.raw].map((slots) => slots.map(({ impl, mode, run, sha256: hash }) => ({ impl, mode, run, sha256: hash }))),
  } };
}
