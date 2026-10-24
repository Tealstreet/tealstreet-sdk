// Collection is not certification: the mandatory verify-gates step decides CI.
// Keeping structured failures lets every product run before the job fails.
import { spawnSync } from 'node:child_process';
import { beforeCpuCase, afterCpuCase } from './cpu-collection.mjs';
import { formatMeasurementMarker } from './cpu-cohort.mjs';
import { appendFileSync, existsSync, mkdirSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';

const directory = resolve('sdk-rs/harness/out/ci');
mkdirSync(directory, { recursive: true });
const reportPath = `${directory}/gate-${process.pid}.json`;
const context = process.env.CPU_COLLECTION_STATE ? beforeCpuCase({ state: process.env.CPU_COLLECTION_STATE, args: process.argv.slice(2) }) : null;
if (context) console.log(formatMeasurementMarker('BEGIN', context.key, context.startedAt));
const capsule = context ? JSON.parse(readFileSync(context.statePath, 'utf8')).destination : null;
const child = spawnSync(context?.nodePath ?? process.execPath, ['sdk-rs/harness/gate.mjs', ...process.argv.slice(2),
  ...(capsule ? ['--runtime-capsule', capsule] : []), '--report', reportPath],
  { stdio: 'inherit', env: { ...process.env, ...context?.env } });
const measured = context && [0, 1].includes(child.status) ? afterCpuCase({ context, reportPath, exitCode: child.status }) : null;
if (measured) console.log(formatMeasurementMarker('END', context.key, measured.evidence.execution.completedAt));
if (![0, 1].includes(child.status) || !existsSync(reportPath))
  throw new Error(`Strict replay did not produce a complete report (exit=${child.status}, signal=${child.signal})`);
const report = JSON.parse(readFileSync(reportPath, 'utf8'));
if ((report.status === 'PASS') !== (child.status === 0)) throw new Error('Strict replay exit/report disagree');
appendFileSync(`${directory}/ledger.jsonl`, JSON.stringify({ reportPath, exitCode: child.status }) + '\n');
console.log(`${report.status} recorded; CI certification remains pending rust:gate:verify`);
