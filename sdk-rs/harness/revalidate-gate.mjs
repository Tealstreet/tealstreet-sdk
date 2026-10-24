import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { mkdirSync, mkdtempSync, readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { compareRecordedReport, readEvidenceText } from './recorded-replay.mjs';

/** Offline revalidation, not another replay. gate.mjs's recorded reader must
 * consume all retained slots; it has no subprocess/transport fallback. */
export function revalidateRecordedEvidence(evidence, { root = process.cwd() } = {}) {
  const report = evidence.report;
  if (report.private !== false) throw new Error('CPU supplement requires a public report');
  const name = createHash('sha256').update(JSON.stringify(evidence)).digest('hex');
  const parent = resolve(root, 'sdk-rs/harness/out/cpu-revalidation');
  mkdirSync(parent, { recursive: true });
  const directory = mkdtempSync(resolve(parent, `${name}-`));
  const input = resolve(directory, `${name}.input.json`), output = resolve(directory, `${name}.strict.json`);
  writeFileSync(input, JSON.stringify(evidence));
  const child = spawnSync(process.execPath, [resolve(root, 'sdk-rs/harness/gate.mjs'),
    '--venue', report.venue, '--tape', report.tape, '--books', report.books, '--runs', '9',
    '--recorded-evidence', input, '--report', output], { cwd: root, encoding: 'utf8', maxBuffer: 16 * 1024 * 1024 });
  writeFileSync(resolve(directory, `${name}.stdout.log`), child.stdout ?? '');
  writeFileSync(resolve(directory, `${name}.stderr.log`), child.stderr ?? '');
  if (![0, 1].includes(child.status) || child.error || child.signal) throw new Error(`offline strict revalidation did not complete (${child.status})`);
  let rederived;
  try { rederived = JSON.parse(readEvidenceText(output)); } catch { throw new Error('offline strict report is missing or invalid'); }
  if ((child.status === 0) !== (rederived.status === 'PASS')) throw new Error('offline strict report exit/status mismatch');
  const errors = compareRecordedReport(report, rederived);
  if (errors.length) throw new Error(errors.join('\n'));
  return { schema: 1, evidenceSha256: name, strictPath: output,
    strictSha256: createHash('sha256').update(readFileSync(output)).digest('hex') };
}
