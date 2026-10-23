// Feed retained bytes through the existing strict gate's validators. This path
// has no transport/engine fallback and cannot collect another timing sample.
import { createHash } from 'node:crypto';
import { readFileSync, realpathSync, statSync } from 'node:fs';
import { isAbsolute, relative, resolve } from 'node:path';
import { isDeepStrictEqual } from 'node:util';
import { performanceOrder } from './performance-policy.mjs';
const sha256 = (value) => createHash('sha256').update(value).digest('hex');
export const readEvidenceText = (path) => new TextDecoder('utf-8', { fatal: true, ignoreBOM: true }).decode(readFileSync(path));
const key = (slot) => JSON.stringify([slot?.impl, slot?.mode, slot?.run]);
export const recordedReportFields = Object.freeze(['schema', 'venue', 'tape', 'tapeSha256', 'private', 'books', 'runs',
  'correctnessRuns', 'requiredImprovement', 'status', 'failures', 'fixers', 'samples', 'summary']);

export function compareRecordedReport(original, rederived) {
  const errors = [];
  for (const field of recordedReportFields) if (!isDeepStrictEqual(original?.[field], rederived?.[field]))
    errors.push(`offline strict report changed ${field}`);
  return errors;
}

export function createRecordedReplay({ evidence, root = process.cwd(), venue, tape, books }) {
  const report = evidence?.report;
  if (!report || report.schema !== 1 || report.private !== false || report.runs !== 9 || report.correctnessRuns !== 1 ||
      report.venue !== venue || report.tape !== tape || report.books !== books ||
      !Array.isArray(evidence.raw) || evidence.raw.length !== 20 || !Array.isArray(report.artifacts) || report.artifacts.length !== 20)
    throw new Error('invalid recorded public case/inventory');
  const outputRoot = realpathSync(resolve(root, 'sdk-rs/harness/out'));
  const actualTape = readFileSync(resolve(root, tape));
  if (report.tapeSha256 !== sha256(actualTape)) throw new Error('recorded tape hash differs');
  const slots = [...['ts', 'rust'].map((impl) => ({ impl, mode: 'correctness', run: 0 })),
    ...Array.from({ length: 9 }, (_, run) => performanceOrder(run).map((impl) => ({ impl, mode: 'performance', run }))).flat()];
  const paths = new Set(), inodes = new Set();
  const outputs = evidence.raw.map((slot, index) => {
    if (key(slot) !== key(slots[index]) || key(report.artifacts[index]) !== key(slots[index])) throw new Error('recorded slots are missing, reordered or duplicated');
    if (typeof slot.bytes !== 'string' || sha256(slot.bytes) !== slot.sha256) throw new Error('recorded raw byte hash differs');
    const path = realpathSync(resolve(root, report.artifacts[index].path));
    const suffix = relative(outputRoot, path);
    const stat = statSync(path);
    const inode = `${stat.dev}:${stat.ino}`;
    if (!stat.isFile() || paths.has(path) || inodes.has(inode)) throw new Error('recorded raw files are duplicated or substituted');
    paths.add(path); inodes.add(inode);
    if (!suffix || suffix.startsWith('..') || isAbsolute(suffix)) throw new Error('recorded artifact escapes output root');
    if (readEvidenceText(path) !== slot.bytes) throw new Error('recorded raw file differs');
    const output = JSON.parse(slot.bytes);
    if (output?.impl !== slot.impl) throw new Error('recorded raw implementation differs');
    return output;
  });
  let consumed = 0;
  return {
    read(impl, run, performanceRun) {
      const slot = { impl, run, mode: performanceRun ? 'performance' : 'correctness' };
      if (key(slot) !== key(slots[consumed])) throw new Error('recorded strict replay requested a missing or reordered slot; no replay fallback');
      return structuredClone(outputs[consumed++]);
    },
    finish() { if (consumed !== slots.length) throw new Error('recorded strict replay did not consume the complete inventory'); },
  };
}
