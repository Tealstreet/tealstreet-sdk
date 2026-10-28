import { readFileSync } from 'node:fs';
import { GATE_SUITES, expectedGateCases } from './ci-gate-suites.mjs';
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
  (item.keys ?? [item.key]).map((key) => ({ key, divergence: item.divergence, condition: item.condition })));

const lighterDailyCounter = /^checkpoint 0 tickers\[\d+\]\.(?:percentage|quoteVolume|volume)$/;
const matchesCondition = (declaration, failure) => {
  if (declaration?.condition === undefined) return true;
  const values = /^checkpoint 0 tickers\[\d+\]\.(?:percentage|quoteVolume|volume): ts=0 rust=(-?(?:\d+(?:\.\d*)?|\.\d+)(?:e[+-]?\d+)?)$/i.exec(failure);
  return declaration.condition === 'lighter-zero-bootstrap' && values !== null && Number.isFinite(Number(values[1]));
};

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
        if (diagnostic.condition !== undefined && (diagnostic.condition !== 'lighter-zero-bootstrap' ||
            entry.venue !== 'lighter' || !lighterDailyCounter.test(path)))
          errors.push(`invalid conditional diagnostic ${identity}`);
      }
    }
  }
  return errors;
}

export const ACCEPTED_CATEGORIES = Object.freeze({
  'rust-better': 'Rust is intentionally more correct or fresher than TS for this value.',
  'ts-unsupported': 'TS cannot express the native value that Rust reports.',
});
// Only TS/Rust output comparisons may be accepted. Performance, coverage,
// transport and harness integrity diagnostics never become accepted parity.
const acceptableKey = /^(?:checkpoint \d+ )?(?:account[ .]|command results|books?[ .]|markets?[ .[]|tickers?[ .[])/;

export const ACCEPTED_DIFFERENCES = JSON.parse(readFileSync(new URL('./accepted-differences.json', import.meta.url), 'utf8'));

export function validateAcceptedDifferences(accepted, manifest) {
  if (accepted?.schema !== 1 || !Array.isArray(accepted.differences)) return ['invalid accepted-difference manifest'];
  const errors = [];
  const seen = new Set();
  for (const entry of accepted.differences) {
    const identity = `${entry?.venue}:${entry?.tapeSha256}`;
    if (!entry || typeof entry.venue !== 'string' || !/^[a-f0-9]{64}$/.test(entry.tapeSha256 ?? '') || typeof entry.tape !== 'string' ||
        !Array.isArray(entry.diagnostics) || !entry.diagnostics.length || seen.has(identity)) {
      errors.push(`invalid or duplicate accepted-difference entry ${identity}`);
      continue;
    }
    seen.add(identity);
    const known = new Set(declaredDiagnostics(manifest?.failures?.find((candidate) =>
      candidate.venue === entry.venue && candidate.tapeSha256 === entry.tapeSha256)).map((item) => item.key));
    const keys = new Set();
    for (const diagnostic of entry.diagnostics) {
      const text = (field) => typeof diagnostic?.[field] === 'string' && diagnostic[field].trim() !== '';
      if (!Array.isArray(diagnostic?.failures) || !diagnostic.failures.length || !Object.hasOwn(ACCEPTED_CATEGORIES, diagnostic.category) ||
          !text('rationale') || !text('evidence') || ['key', 'keys', 'condition'].some((field) => Object.hasOwn(diagnostic, field))) {
        errors.push(`invalid accepted-difference declaration ${identity}`);
        continue;
      }
      // Each entry is the exact failure line, values included, so a later
      // change of either value at an accepted path is a new divergence.
      for (const failure of diagnostic.failures) {
        const path = typeof failure === 'string' ? diagnosticKey(failure) : '';
        if (!path || path === failure || path.includes('*') || keys.has(path) || !acceptableKey.test(path))
          errors.push(`invalid accepted-difference failure ${identity}: ${failure}`);
        else if (known.has(path)) errors.push(`accepted difference is also a known failure ${identity}: ${path}`);
        keys.add(path);
      }
    }
  }
  return errors;
}

export const assessGateReport = (report, manifest, accepted = ACCEPTED_DIFFERENCES) => assessReport(report, manifest, false, accepted);
export const assessCorrectnessReport = (report, manifest, accepted = ACCEPTED_DIFFERENCES) => assessReport(report, manifest, true, accepted);

const acceptedDiagnostics = (accepted, report) => new Map((accepted.differences.find((candidate) =>
  candidate.venue === report.venue && candidate.tapeSha256 === report.tapeSha256 && candidate.tape === report.tape)?.diagnostics ?? [])
  .flatMap((item) => item.failures.map((failure) => [diagnosticKey(failure),
    { key: diagnosticKey(failure), failure, category: item.category, rationale: item.rationale, evidence: item.evidence }])));

/** Every accepted entry must name a tape that a CI gate case replays. Private
 * tapes are generated in CI, so their bytes are checked per suite at verification. */
export function acceptedCoverageErrors(accepted, cases = GATE_SUITES.flatMap((suite) => expectedGateCases(suite))) {
  return (accepted?.differences ?? []).filter((entry) => !cases.some((item) => item.venue === entry.venue && item.tape === entry.tape))
    .map((entry) => `accepted difference ${entry.venue}:${entry.tapeSha256} names no CI gate case for ${entry.tape}`);
}

/** A suite's ledger must replay every accepted entry it owns at the recorded tape SHA. */
export function acceptedReplayErrors(accepted, cases, reports) {
  return (accepted?.differences ?? []).filter((entry) => cases.some((item) => item.venue === entry.venue && item.tape === entry.tape))
    .filter((entry) => !reports.some((report) => report?.venue === entry.venue && report.tape === entry.tape && report.tapeSha256 === entry.tapeSha256))
    .map((entry) => `accepted difference ${entry.venue}:${entry.tapeSha256} was not replayed at that tape SHA: ${entry.tape}`);
}

function assessReport(report, manifest, correctnessOnly, accepted) {
  const errors = [...validateKnownFailures(manifest), ...validateAcceptedDifferences(accepted, manifest)];
  if (errors.length) return { errors, known: [], accepted: [], resolved: [] };
  if (report?.schema !== 1 || !['PASS', 'FAIL'].includes(report.status) ||
      !Array.isArray(report.failures) || report.failures.some((failure) => typeof failure !== 'string') ||
      (report.status === 'PASS') !== (report.failures.length === 0) ||
      typeof report.private !== 'boolean' || !/^[a-f0-9]{64}$/.test(report.tapeSha256 ?? ''))
    return { errors: [...errors, 'invalid strict gate report'], known: [], accepted: [], resolved: [] };
  if (correctnessOnly !== (report.correctnessOnly === true))
    errors.push('correctness-only reports cannot substitute for measured gate evidence');
  if (correctnessOnly && (report.runs !== 0 || report.correctnessRuns !== 1 ||
      report.coverage?.performance !== 'not-run' || report.coverage?.correctness !== (report.replayFailure ? 'incomplete' : 'complete')))
    errors.push('invalid correctness-only replay coverage');
  const entry = manifest.failures.find((candidate) => candidate.venue === report.venue && candidate.tapeSha256 === report.tapeSha256);
  const declarations = new Map(declaredDiagnostics(entry).map((item) => [item.key, item]));
  const acceptances = acceptedDiagnostics(accepted, report);
  if (report.replayFailure) {
    const { kind, impl, command } = report.replayFailure;
    const key = `${impl} replay: command ${command} did not complete`;
    if (!report.private || kind !== 'command-timeout' || !['ts', 'rust'].includes(impl) ||
        typeof command !== 'string' || !/^[A-Za-z][A-Za-z0-9]*$/.test(command) ||
        report.status !== 'FAIL' || report.failures.length !== 1 || report.failures[0] !== key ||
        report.coverage?.correctness !== 'incomplete' || report.coverage?.performance !== 'not-run' || report.runs !== 0)
      errors.push('invalid incomplete command replay report');
    if (!declarations.has(key)) errors.push(`${report.venue}: unexpected ${key}`);
    // An incomplete replay cannot reproduce or retire accepted differences.
    return { errors, known: declarations.has(key) ? [{ key, divergence: declarations.get(key).divergence }] : [], accepted: [], resolved: [] };
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
  const acceptedSeen = [];
  const observed = new Set();
  for (const failure of report.failures) {
    const key = diagnosticKey(failure);
    observed.add(key);
    // Every implementation must reproduce its own audited replay. Snapshot
    // leaf and duplicate-row diagnostics have no common "workload" suffix.
    const ownReplayFailure = /^(?:ts|rust) performance run \d+(?:[ :]|$)/.test(failure);
    if (ownReplayFailure || failure.startsWith('checkpoint coverage ') || protectedFailure.test(failure))
      errors.push(`${report.venue}: unexpected ${failure}`);
    else if (acceptances.has(key)) {
      if (acceptances.get(key).failure === failure) acceptedSeen.push(acceptances.get(key));
      else errors.push(`${report.venue}: accepted difference changed value ${failure}`);
    }
    else if (!declarations.has(key) || !matchesCondition(declarations.get(key), failure)) errors.push(`${report.venue}: unexpected ${failure}`);
    else known.push({ key, divergence: declarations.get(key).divergence });
  }
  // A stale acceptance would silently cover a future regression at that path.
  for (const key of acceptances.keys()) if (!observed.has(key)) errors.push(`${report.venue}: stale accepted difference ${key}`);
  return { errors, known, accepted: acceptedSeen, resolved: [...declarations.keys()].filter((key) => !observed.has(key)) };
}
