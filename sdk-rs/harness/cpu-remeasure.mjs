// Public strict-evidence validation and the CPU-only split: a valid report whose only
// unexpected failures are computed CPU misses is judged by the CPU gate, not correctness.
import { createHash } from 'node:crypto';
import { isDeepStrictEqual } from 'node:util';
import { memoryMetricErrors } from './memory.mjs';
import { compareSnapshots, snapshotCoverageErrors } from './snapshot-compare.mjs';
import { sameCommandResults } from './tape.mjs';
import { PUBLIC_REPLAY_PAIRS, CPU_FIELDS, performanceFailures, performanceMetricErrors, performanceObjectives, performanceOrder } from './performance-policy.mjs';
import { hasPairedBitgetSteady } from './bitget-paired-steady.mjs';

const sha256 = (bytes) => createHash('sha256').update(bytes).digest('hex');
const slotKey = (value) => JSON.stringify([value?.impl, value?.mode, value?.run]);
const expectedSlots = () => [
  ...['ts', 'rust'].map((impl) => ({ impl, mode: 'correctness', run: 0 })),
  ...Array.from({ length: PUBLIC_REPLAY_PAIRS }, (_, run) => performanceOrder(run).map((impl) => ({ impl, mode: 'performance', run }))).flat(),
];
const digest = (value) => typeof value === 'string' && /^[a-f0-9]{64}$/.test(value);
const same = isDeepStrictEqual;
const decode = (slot) => JSON.parse(slot.bytes);
const objectives = (report, paired, source) => performanceObjectives(report.samples.ts, report.samples.rust)
  .filter(({ field }) => CPU_FIELDS.includes(field) && !(field === 'steadyCpuMs' && hasPairedBitgetSteady(paired, report, source)));

/** Validate every raw slot before trusting report samples or failure strings. */
function validatePublicEvidenceContent(value) {
  const errors = [];
  const report = value?.report;
  if (!report || report.schema !== 1 || report.private !== false || report.runs !== PUBLIC_REPLAY_PAIRS || report.correctnessRuns !== 1 ||
      report.requiredImprovement !== 0.10 || !['PASS', 'FAIL'].includes(report.status) ||
      !Array.isArray(report.failures) || report.failures.some((item) => typeof item !== 'string') ||
      (report.status === 'PASS') !== (report.failures.length === 0) || report.replayFailure ||
      typeof report.venue !== 'string' || !report.venue || typeof report.tape !== 'string' || !report.tape ||
      typeof report.books !== 'string' || !digest(report.tapeSha256) || !Array.isArray(report.fixers))
    return ['invalid complete public strict report'];
  if (value.reportBytes !== undefined && (typeof value.reportBytes !== 'string' || !same(JSON.parse(value.reportBytes), report)))
    return ['original strict report bytes disagree with parsed content'];
  if (!Array.isArray(value.raw) || value.raw.length !== (2 + 2 * PUBLIC_REPLAY_PAIRS) || !Array.isArray(report.artifacts) || report.artifacts.length !== (2 + 2 * PUBLIC_REPLAY_PAIRS))
    return ['missing complete audited/fixed-pair raw inventory'];
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
  for (const impl of ['ts', 'rust']) if (!Array.isArray(report.samples?.[impl]) || report.samples[impl].length !== PUBLIC_REPLAY_PAIRS)
    errors.push(`${impl}: missing complete fixed samples`);
  return errors;
}

export function validatePublicEvidence(value) {
  try { return validatePublicEvidenceContent(value); }
  catch (error) { return [`malformed public evidence: ${error.message}`]; }
}

function cpuOnlyErrors(value, paired) {
  const errors = validatePublicEvidence(value);
  if (errors.length) return errors;
  const report = value.report, source = value.execution?.measuredCheckoutSha;
  if (paired !== undefined && !hasPairedBitgetSteady(paired, report, source)) return ['unissued or mismatched paired steady certificate'];
  const calculated = performanceFailures(report.samples.ts, report.samples.rust, report.venue).sort();
  // Exact generated failures, not a performance-prefix allowlist. Memory never
  // becomes an averaged objective and declarations cannot excuse correctness.
  const cpuFailures = calculated.filter((failure) => objectives(report).some(({ label }) => failure.startsWith(`performance ${label}:`)));
  if (!same(calculated, cpuFailures) || !same([...report.failures].sort(), cpuFailures))
    errors.push('strict failures include non-CPU diagnostics or omit/repeat computed failures');
  if (!cpuFailures.some(failure => objectives(report, paired, source).some(({ label }) => failure.startsWith(`performance ${label}:`))))
    errors.push('first measurement has no remaining CPU miss');
  return errors;
}

/** Split assessment errors of a valid CPU-only strict miss: only the exact computed
 * CPU failures are deferred to the CPU gate; every other error is kept. */
export function splitCpuErrors(value, errors, paired) {
  if (cpuOnlyErrors(value, paired).length) return { kept: errors, deferred: [] };
  const report = value.report;
  const cpu = new Set(performanceFailures(report.samples.ts, report.samples.rust, report.venue)
    .filter((failure) => objectives(report).some(({ label }) => failure.startsWith(`performance ${label}:`)))
    .map((failure) => `${report.venue}: unexpected ${failure}`));
  return { kept: errors.filter((error) => !cpu.has(error)), deferred: errors.filter((error) => cpu.has(error)) };
}
