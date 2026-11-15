// CPU warnings never block; they are surfaced as GitHub annotations and in the step summary.
import { appendFileSync } from 'node:fs';
import { CPU_FIELDS, CPU_SLOWER, performanceObjectives } from './performance-policy.mjs';

const escape = (text) => text.replaceAll('%', '%25').replaceAll('\r', '%0D').replaceAll('\n', '%0A');

/** Exact CPU observations, annotation lines and step-summary rows for one gate report. */
export function cpuAnnotations({ venue, tape, failures, warnings, remeasure, samples }) {
  const slower = failures.filter((failure) => failure.includes(`: ${CPU_SLOWER} (`));
  const lines = [
    ...(samples ? performanceObjectives(samples.ts, samples.rust).filter(({ field }) => CPU_FIELDS.includes(field))
      .map(({ field, reference, candidate }) => `CPU_METRICS ${JSON.stringify({ venue, tape, field,
        tsMs: reference, rustMs: candidate, rustTsRatio: candidate / reference })}`) : []),
    ...warnings.map((warning) => `::warning title=CPU ${venue}::${escape(`${tape}: ${warning}`)}`),
    ...slower.map(
      (failure) =>
        `::error title=CPU ${venue}::${escape(`${tape}: ${failure}${remeasure ? ' (persisted after the one re-measure)' : ''}`)}`,
    ),
  ];
  const rows = [
    ...(remeasure ? remeasure.first.map((first) => ['first measurement', first]) : []),
    ...warnings.map((warning) => ['warning', warning]),
    ...slower.map((failure) => ['failure', failure]),
  ];
  const summary = rows.length
    ? [
        `#### CPU ${venue} \`${tape}\``,
        '',
        '| kind | detail |',
        '| --- | --- |',
        ...rows.map(([kind, detail]) => `| ${kind} | ${detail.replaceAll('|', '\\|')} |`),
        '',
      ].join('\n') + '\n'
    : '';
  return { lines, summary };
}

export function writeCpuAnnotations(report, summaryPath = process.env.GITHUB_STEP_SUMMARY, log = console.log) {
  const { lines, summary } = cpuAnnotations(report);
  for (const line of lines) log(line);
  if (summary && summaryPath) appendFileSync(summaryPath, summary);
}
