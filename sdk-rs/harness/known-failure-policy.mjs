import { memoryMetricErrors } from './memory.mjs';
import { performanceFailures, performanceMetricErrors } from './performance-policy.mjs';

// Preserve checkpoint/row/field identities. Only strip observed values and the
// performance sample ordinal; every sample still runs and validates separately.
export const diagnosticKey = (failure) => failure
  .replace(/ performance run \d+/g, ' performance run')
  .split(': ts=')[0].split(' (ts=')[0];

// Group exact paths sharing a rationale without repeating prose thousands of
// times for catalogue/depth rows. This never introduces patterns or ranges.
const declaredDiagnostics = (entry) => (entry?.diagnostics ?? []).flatMap((item) =>
  (item.keys ?? [item.key]).map((key) => ({ key, divergence: item.divergence })));

export function validateKnownFailures(manifest) {
  const errors = [];
  if (manifest?.schema !== 1 || !Array.isArray(manifest.failures)) return ['invalid known-failure manifest'];
  const seen = new Set();
  for (const entry of manifest.failures) {
    if (!entry || typeof entry !== 'object' || !Array.isArray(entry.diagnostics)) {
      errors.push('invalid known-failure entry');
      continue;
    }
    const identity = `${entry.venue}:${entry.tapeSha256}`;
    if (typeof entry.venue !== 'string' || !/^[a-f0-9]{64}$/.test(entry.tapeSha256 ?? '') ||
        !Array.isArray(entry.diagnostics) || !entry.diagnostics.length || seen.has(identity))
      errors.push(`invalid or duplicate known-failure entry ${identity}`);
    seen.add(identity);
    const keys = new Set();
    for (const diagnostic of entry.diagnostics ?? []) {
      if (!diagnostic || typeof diagnostic !== 'object') { errors.push(`invalid diagnostic declaration ${identity}`); continue; }
      const grouped = Object.hasOwn(diagnostic, 'keys');
      const paths = grouped ? diagnostic.keys : [diagnostic.key];
      if ((grouped && Object.hasOwn(diagnostic, 'key')) || !Array.isArray(paths) || !paths.length ||
          typeof diagnostic.divergence !== 'string' || !diagnostic.divergence.trim()) {
        errors.push(`invalid diagnostic declaration ${identity}`);
        continue;
      }
      for (const path of paths) {
        if (typeof path !== 'string' || !path || path.includes('*') || keys.has(path))
          errors.push(`invalid diagnostic declaration ${identity}`);
        keys.add(path);
      }
    }
  }
  return errors;
}

export const assessGateReport = (report, manifest) => assessReport(report, manifest, false);
export const assessCorrectnessReport = (report, manifest) => assessReport(report, manifest, true);

function assessReport(report, manifest, correctnessOnly) {
  const errors = [...validateKnownFailures(manifest)];
  if (errors.length) return { errors, known: [], resolved: [] };
  if (report?.schema !== 1 || !['PASS', 'FAIL'].includes(report.status) ||
      !Array.isArray(report.failures) || report.failures.some((failure) => typeof failure !== 'string') ||
      (report.status === 'PASS') !== (report.failures.length === 0) ||
      typeof report.private !== 'boolean' || !/^[a-f0-9]{64}$/.test(report.tapeSha256 ?? ''))
    return { errors: [...errors, 'invalid strict gate report'], known: [], resolved: [] };
  if (correctnessOnly !== (report.correctnessOnly === true))
    errors.push('correctness-only reports cannot substitute for measured gate evidence');
  if (correctnessOnly && (report.runs !== 0 || report.correctnessRuns !== 1 ||
      report.coverage?.performance !== 'not-run' || report.coverage?.correctness !== (report.replayFailure ? 'incomplete' : 'complete')))
    errors.push('invalid correctness-only replay coverage');
  const entry = manifest.failures.find((candidate) => candidate.venue === report.venue && candidate.tapeSha256 === report.tapeSha256);
  const declarations = new Map(declaredDiagnostics(entry).map((item) => [item.key, item]));
  if (report.replayFailure) {
    const { kind, impl, command } = report.replayFailure;
    const key = `${impl} replay: command ${command} did not complete`;
    if (!report.private || kind !== 'command-timeout' || !['ts', 'rust'].includes(impl) ||
        typeof command !== 'string' || !/^[A-Za-z][A-Za-z0-9]*$/.test(command) ||
        report.status !== 'FAIL' || report.failures.length !== 1 || report.failures[0] !== key ||
        report.coverage?.correctness !== 'incomplete' || report.coverage?.performance !== 'not-run' || report.runs !== 0)
      errors.push('invalid incomplete command replay report');
    if (!declarations.has(key)) errors.push(`${report.venue}: unexpected ${key}`);
    return { errors, known: declarations.has(key) ? [{ key, divergence: declarations.get(key).divergence }] : [], resolved: [] };
  }
  // Missing performance work and harness integrity cannot become exemptions.
  // TS-versus-Rust frame/protocol differences may be declared venue failures;
  // their strict result remains FAIL and never certifies complete parity.
  for (const impl of ['ts', 'rust']) {
    const samples = report.samples?.[impl];
    const correctness = report.private || correctnessOnly;
    const expected = correctness ? report.correctnessRuns : report.runs;
    if (!Number.isSafeInteger(expected) || expected < (correctness ? 1 : 7) || !Array.isArray(samples) || samples.length !== expected) {
      errors.push(`${impl}: incomplete gate samples`);
      continue;
    }
    for (const sample of samples) errors.push(...memoryMetricErrors(sample),
      ...performanceMetricErrors(sample, correctness ? 'correctness' : 'performance'));
  }
  const protectedFailure = /baseline is invalid|invalid (?:cpuMs|steadyCpuMs|processCpuMs)|incomplete replay workload|workload differs|checkpoint snapshots missing|invalid tape boundary|empty public seed|requested book .* is missing|fixer modified Rust|fixer changed undeclared/;
  if (!report.private && !correctnessOnly) {
    if (report.requiredImprovement !== 0.10) errors.push('performance improvement contract differs from 10%');
    if (!errors.length) for (const missing of performanceFailures(report.samples.ts, report.samples.rust))
      if (!report.failures.includes(missing)) errors.push(`strict report omitted ${missing}`);
  }
  const known = [];
  const observed = new Set();
  for (const failure of report.failures) {
    const key = diagnosticKey(failure);
    observed.add(key);
    // Every implementation must reproduce its own audited replay. Snapshot
    // leaf and duplicate-row diagnostics have no common "workload" suffix.
    const ownReplayFailure = /^(?:ts|rust) performance run \d+(?:[ :]|$)/.test(failure);
    if (ownReplayFailure || failure.startsWith('checkpoint coverage ') || protectedFailure.test(failure) || !declarations.has(key)) errors.push(`${report.venue}: unexpected ${failure}`);
    else known.push({ key, divergence: declarations.get(key).divergence });
  }
  return { errors, known, resolved: [...declarations.keys()].filter((key) => !observed.has(key)) };
}
