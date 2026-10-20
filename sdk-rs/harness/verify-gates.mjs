import { readFileSync } from 'node:fs';
import { parseArgs } from 'node:util';
import { assessGateReport } from './known-failure-policy.mjs';
import { validateGateCoverage } from './ci-gate-suites.mjs';

const { values } = parseArgs({ options: { suite: { type: 'string' } } });
const ledger = readFileSync('sdk-rs/harness/out/ci/ledger.jsonl', 'utf8').trim().split('\n').map(JSON.parse);
const manifest = JSON.parse(readFileSync(new URL('./known-gate-failures.json', import.meta.url), 'utf8'));
const errors = [];
const cases = [];
const seen = new Set();
let passes = 0, knownFailures = 0;
for (const { reportPath, exitCode } of ledger) {
  const report = JSON.parse(readFileSync(reportPath, 'utf8'));
  const identity = `${report.venue}:${report.tapeSha256}`;
  cases.push({ venue: report.venue, tape: report.tape, books: report.books });
  if (seen.has(identity)) errors.push(`Duplicate gate cannot substitute for missing coverage: ${identity}`);
  seen.add(identity);
  if ((exitCode === 0) !== (report.status === 'PASS')) errors.push(`Invalid ledger exit code: ${identity}`);
  const assessment = assessGateReport(report, manifest);
  errors.push(...assessment.errors);
  if (report.status === 'PASS') passes++;
  else knownFailures++;
  console.log(`${report.status === 'PASS' ? 'PASS' : 'KNOWN FAILURE (not parity)'} ${report.venue} ${report.tape}: ${assessment.known.length} declared diagnostics`);
  for (const diagnostic of new Map(assessment.known.map((item) => [item.key, item])).values())
    console.log(`  ${diagnostic.key}: ${diagnostic.divergence}`);
  if (assessment.resolved.length) console.log(`  ${assessment.resolved.length} declared diagnostics not observed in this run`);
}
errors.push(...validateGateCoverage(values.suite, cases));
console.log(`${passes} strict passes, ${knownFailures} known-failure gates, ${errors.length} unexpected failures`);
if (errors.length) {
  for (const error of errors) console.error(error);
  process.exit(1);
}
