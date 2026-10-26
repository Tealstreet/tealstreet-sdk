import { readFileSync, realpathSync } from 'node:fs';
import { isAbsolute, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { parseArgs } from 'node:util';
import { createHash } from 'node:crypto';
import { assessGateReport } from './known-failure-policy.mjs';
import { validateGateCoverage } from './ci-gate-suites.mjs';
import { assessEligibility } from './cpu-remeasure.mjs';
import { cpuRemeasureEnabled } from './cpu-rollout.mjs';
import { readEvidenceText } from './recorded-replay.mjs';
import { revalidateRecordedEvidence } from './revalidate-gate.mjs';
import { pendingBitgetSteady } from './bitget-paired-steady.mjs';

/** Strict remains the default. Collection may retain an eligible unresolved
 * CPU miss, but cannot label it PASS or supply final certification. */
export function verifyGateSuite({ suite, entries, manifest, mode = 'strict' }) {
  if (!['strict', 'collect'].includes(mode)) throw new Error('unknown gate verifier mode');
  const errors = [], cases = [], rows = [], pending = [], seen = new Set();
  let passes = 0, strictFailures = 0, knownFailures = 0;
  if (!Array.isArray(entries) || entries.length === 0) return { errors: ['missing gate ledger'], rows, pending, passes, strictFailures, knownFailures };
  for (const entry of entries) {
    if (!entry?.report || typeof entry.report !== 'object' || ![0, 1].includes(entry.exitCode)) {
      errors.push('invalid gate ledger entry'); continue;
    }
    const { report, exitCode } = entry;
    const identity = `${report.venue}:${report.tapeSha256}`;
    cases.push({ venue: report.venue, tape: report.tape, books: report.books });
    if (seen.has(identity)) errors.push(`Duplicate gate cannot substitute for missing coverage: ${identity}`);
    seen.add(identity);
    const ledgerMismatch = (exitCode === 0) !== (report.status === 'PASS');
    if (ledgerMismatch) errors.push(`Invalid ledger exit code: ${identity}`);
    const assessment = assessGateReport(report, manifest);
    const eligible = mode === 'collect' && cpuRemeasureEnabled(suite) && !ledgerMismatch && assessment.errors.length > 0 &&
      (assessEligibility({ report, raw: entry.raw }).eligible || suite === 'bitget' && pendingBitgetSteady(entry, manifest));
    if (eligible) pending.push({ venue: report.venue, tape: report.tape, books: report.books, tapeSha256: report.tapeSha256 });
    else errors.push(...assessment.errors);
    if (report.status === 'PASS' && !assessment.errors.length) passes++;
    if (report.status === 'FAIL') {
      strictFailures++;
      if (!assessment.errors.length) knownFailures++;
    }
    const label = eligible ? 'UNRESOLVED CPU MISS (not certification)' : assessment.errors.length ? 'UNEXPECTED / INVALID RESULT'
      : report.status === 'PASS' ? 'PASS' : 'KNOWN FAILURE (not parity)';
    rows.push({ label, identity, assessment });
  }
  errors.push(...validateGateCoverage(suite, cases));
  return { errors, rows, pending, passes, strictFailures, knownFailures };
}

export function loadGateLedger({ root = process.cwd(), directory = 'sdk-rs/harness/out/ci', includeRaw = false } = {}) {
  const outputRoot = realpathSync(resolve(root, 'sdk-rs/harness/out'));
  const file = (path) => {
    if (typeof path !== 'string') throw new Error('invalid gate evidence path');
    const actual = realpathSync(resolve(root, path));
    const suffix = relative(outputRoot, actual);
    if (!suffix || suffix.startsWith('..') || isAbsolute(suffix)) throw new Error('gate evidence escapes output root');
    return readEvidenceText(actual);
  };
  const ledger = file(resolve(root, directory, 'ledger.jsonl')).trim().split('\n').map(JSON.parse);
  return ledger.map(({ reportPath, exitCode }) => {
    const reportBytes = file(reportPath);
    const report = JSON.parse(reportBytes);
    const raw = includeRaw && report.private === false && Array.isArray(report.artifacts) ? report.artifacts.map(({ impl, mode, run, path }) => {
      const bytes = file(path);
      return { impl, mode, run, bytes, sha256: createHash('sha256').update(bytes).digest('hex') };
    }) : undefined;
    const evidence = { report, reportBytes, exitCode, ...(raw ? { raw } : {}) };
    if (includeRaw && report.private === false && assessEligibility(evidence).eligible)
      evidence.revalidation = revalidateRecordedEvidence(evidence, { root });
    return evidence;
  });
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const { values } = parseArgs({ options: { suite: { type: 'string' }, mode: { type: 'string', default: 'strict' } } });
  const manifest = JSON.parse(readFileSync(new URL('./known-gate-failures.json', import.meta.url), 'utf8'));
  const result = verifyGateSuite({ suite: values.suite, entries: loadGateLedger({ includeRaw: values.mode === 'collect' }), manifest, mode: values.mode });
  for (const { label, identity, assessment } of result.rows) {
    console.log(`${label} ${identity}: ${assessment.known.length} declared diagnostics`);
    for (const diagnostic of new Map(assessment.known.map((item) => [item.key, item])).values())
      console.log(`  ${diagnostic.key}: ${diagnostic.divergence}`);
    if (assessment.resolved.length) console.log(`  ${assessment.resolved.length} declared diagnostics not observed in this run`);
  }
  console.log(`${result.passes} strict passes, ${result.strictFailures} strict failures (${result.knownFailures} declared known-failure gates), ${result.pending.length} unresolved CPU cases, ${result.errors.length} unexpected failures`);
  if (result.errors.length) {
    for (const error of result.errors) console.error(error);
    process.exitCode = 1;
  }
}
