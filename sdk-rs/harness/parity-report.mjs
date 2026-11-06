import { readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { parseArgs } from 'node:util';
import { assessGateReport, assessCorrectnessReport, diagnosticKey, ACCEPTED_DIFFERENCES } from './known-failure-policy.mjs';

export const ENTITIES = Object.freeze(['orders', 'positions', 'balance', 'fills', 'settings', 'flatPositionPnl', 'account',
  'commands', 'books', 'markets', 'tickers', 'transport', 'performance', 'other']);
const CLASSES = Object.freeze(['accepted', 'known', 'new']);
const SAMPLES = 3;

/** Map a gate diagnostic key to the normalized output it compares. */
export function entityOf(key) {
  const path = key.replace(/^checkpoint \d+ /, '');
  const account = /^account[ .](orders|positions|balance|fills|settings|flatPositionPnl)\b/.exec(path);
  if (account) return account[1];
  if (/^account[ .]/.test(path)) return 'account';
  if (path.startsWith('command results')) return 'commands';
  if (/^books?[ .]/.test(path)) return 'books';
  if (/^markets?[ .[]/.test(path)) return 'markets';
  if (/^tickers?[ .[]/.test(path)) return 'tickers';
  if (/^(?:requests|subscriptions) run |^HTTP write sequence|^websocket requests/.test(path)) return 'transport';
  if (/^performance |^(?:ts|rust) performance run /.test(path)) return 'performance';
  return 'other';
}

/** Classify every strict diagnostic of one gate report as accepted, known or new. */
export function reportCase(report, manifest, accepted = ACCEPTED_DIFFERENCES) {
  const assessment = (report?.correctnessOnly === true ? assessCorrectnessReport : assessGateReport)(report, manifest, accepted);
  // Accepted lines match exactly, so a changed value at an accepted path counts as new.
  const acceptedLines = new Set(assessment.accepted.map((item) => item.failure));
  const knownKeys = new Set(assessment.known.map((item) => item.key));
  const entities = {};
  for (const failure of Array.isArray(report?.failures) ? report.failures : []) {
    const key = typeof failure === 'string' ? diagnosticKey(failure) : String(failure);
    const kind = acceptedLines.has(failure) ? 'accepted' : knownKeys.has(key) ? 'known' : 'new';
    const row = entities[entityOf(key)] ??= Object.fromEntries(CLASSES.map((name) => [name, { count: 0, samples: [] }]));
    row[kind].count++;
    if (row[kind].samples.length < SAMPLES) row[kind].samples.push(failure);
  }
  const status = report?.status === 'PASS' && !assessment.errors.length ? 'PASS'
    : assessment.errors.length ? 'NEW DIVERGENCE OR INVALID'
      : assessment.known.length ? 'KNOWN FAILURE' : 'ACCEPTED DIFFERENCES';
  return {
    venue: report?.venue, tape: report?.tape, tapeSha256: report?.tapeSha256, private: report?.private, status,
    entities, errors: assessment.errors, staleKnown: assessment.resolved,
  };
}

export function buildParityReport(reports, manifest, accepted = ACCEPTED_DIFFERENCES) {
  const cases = reports.map((report) => reportCase(report, manifest, accepted))
    .sort((a, b) => `${a.venue}:${a.tape}`.localeCompare(`${b.venue}:${b.tape}`));
  const totals = Object.fromEntries(CLASSES.map((name) => [name, 0]));
  for (const item of cases) for (const row of Object.values(item.entities)) for (const name of CLASSES) totals[name] += row[name].count;
  return { schema: 1, cases, totals };
}

export function renderMarkdown({ cases, totals }) {
  const lines = ['# TS/Rust parity report', '',
    `Diagnostics: ${totals.accepted} accepted, ${totals.known} known (debt), ${totals.new} new or invalid.`, '',
    '| Venue | Tape | Status | Entity | Accepted | Known | New |', '| --- | --- | --- | --- | ---: | ---: | ---: |'];
  for (const item of cases) {
    const tape = item.tape?.split('/').pop() ?? '';
    const rows = ENTITIES.filter((name) => item.entities[name]);
    if (!rows.length) lines.push(`| ${item.venue} | ${tape} | ${item.status} | - | 0 | 0 | 0 |`);
    for (const name of rows) {
      const row = item.entities[name];
      lines.push(`| ${item.venue} | ${tape} | ${item.status} | ${name} | ${row.accepted.count} | ${row.known.count} | ${row.new.count} |`);
    }
  }
  for (const item of cases.filter((candidate) => candidate.errors.length)) {
    lines.push('', `## ${item.venue} ${item.tape?.split('/').pop() ?? ''}`, '');
    for (const error of item.errors.slice(0, 20)) lines.push(`- ${error.replaceAll('|', '\\|')}`);
    if (item.errors.length > 20) lines.push(`- ... ${item.errors.length - 20} more`);
  }
  return `${lines.join('\n')}\n`;
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const { values, positionals } = parseArgs({ allowPositionals: true, options: {
    ledger: { type: 'string' }, json: { type: 'string' }, markdown: { type: 'string' },
  } });
  const manifest = JSON.parse(readFileSync(new URL('./known-gate-failures.json', import.meta.url), 'utf8'));
  let reports = positionals.map((path) => JSON.parse(readFileSync(path, 'utf8')));
  if (values.ledger || !reports.length) {
    const { loadGateLedger } = await import('./verify-gates.mjs');
    reports = [...reports, ...loadGateLedger(values.ledger ? { directory: values.ledger } : {}).map((entry) => entry.report)];
  }
  const report = buildParityReport(reports, manifest);
  if (values.json) writeFileSync(values.json, `${JSON.stringify(report, null, 2)}\n`);
  const markdown = renderMarkdown(report);
  if (values.markdown) writeFileSync(values.markdown, markdown);
  else process.stdout.write(markdown);
}
