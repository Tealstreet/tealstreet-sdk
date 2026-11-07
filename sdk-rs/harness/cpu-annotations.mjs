// CPU warnings never block; they are surfaced as GitHub annotations and in the step summary.
import { appendFileSync } from 'node:fs';
import { CPU_SLOWER } from './performance-policy.mjs';

const escape = (text) => text.replaceAll('%', '%25').replaceAll('\r', '%0D').replaceAll('\n', '%0A');

/** The annotation lines and step-summary rows for one gate report. */
export function cpuAnnotations({ venue, tape, failures, warnings, remeasure }) {
  const slower = failures.filter((failure) => failure.includes(`: ${CPU_SLOWER} (`));
  const lines = [
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
