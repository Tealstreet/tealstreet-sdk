import { spawn, execFileSync } from 'node:child_process';
import { closeSync, mkdirSync, mkdtempSync, openSync, readFileSync, rmSync } from 'node:fs';
import { resolve } from 'node:path';
import { CPU_REMEASURE_SUITES } from './cpu-rollout.mjs';

export function cpuPartitions(count = 4) {
  if (!Number.isSafeInteger(count) || count < 1 || count > 4) throw new Error('CPU verification has at most four workers');
  return Array.from({ length: count }, (_, index) => CPU_REMEASURE_SUITES.filter((_, i) => i % count === index));
}

export function validateWorkerRoster(rows) {
  if (!Array.isArray(rows) || rows.length !== CPU_REMEASURE_SUITES.length ||
      new Set(rows.map(row => row.suite)).size !== rows.length ||
      rows.some(row => !CPU_REMEASURE_SUITES.includes(row.suite)))
    throw new Error('Incomplete, duplicate or foreign CPU worker roster');
  return CPU_REMEASURE_SUITES.map(suite => rows.find(row => row.suite === suite));
}

// Verification only. Each process owns an independent checkout and output tree;
// no measurement, build, shared out/ mutation or node_modules copy occurs here.
export async function runCpuWorkers(context, phase, destination) {
  if (!['plan', 'certify'].includes(phase)) throw new Error('Unknown CPU verification phase');
  const parent = mkdtempSync(resolve(context.env.RUNNER_TEMP, 'cpu-verifiers-'));
  mkdirSync(destination, { recursive: true });
  try {
    const settled = await Promise.allSettled(cpuPartitions().map(async (suites, index) => {
      const root = resolve(parent, String(index)), out = resolve(destination, `worker-${index}`);
      mkdirSync(out);
      execFileSync('git', ['clone', '--shared', '--no-checkout', '--quiet', context.root, root]);
      execFileSync('git', ['-C', root, 'checkout', '--detach', '--quiet', context.checkoutSha]);
      execFileSync('git', ['-C', root, 'remote', 'set-url', 'origin', 'https://github.com/Tealstreet/tealstreet-sdk.git']);
      const log = openSync(resolve(out, 'verification.log'), 'wx');
      try {
        await new Promise((accept, reject) => {
          const child = spawn(process.execPath, ['sdk-rs/harness/cpu-ci.mjs', '--command', `${phase}-worker`,
            '--worker-suites', JSON.stringify(suites), '--out', out], { cwd: root,
            env: { ...context.env, NODE_PATH: resolve(context.root, 'node_modules') }, stdio: ['ignore', log, log] });
          child.once('error', reject);
          child.once('exit', (code, signal) => code === 0 && !signal ? accept() : reject(new Error(`CPU verifier ${index} failed (${code ?? signal}); see ${out}/verification.log`)));
        });
      } finally { closeSync(log); }
      const result = JSON.parse(readFileSync(resolve(out, 'worker.json'), 'utf8'));
      if (result.runId !== context.runId || result.checkoutSha !== context.checkoutSha || result.phase !== phase ||
          !Array.isArray(result.bindings) || !Array.isArray(result.secondDispositions) ||
          JSON.stringify(result.rows?.map(row => row.suite)) !== JSON.stringify(suites))
        throw new Error('CPU verification worker source/run/partition differs');
      context.bindings.push(...result.bindings);
      context.secondDispositions ??= new Map();
      for (const [suite, cases] of result.secondDispositions) context.secondDispositions.set(suite, cases);
      return result.rows;
    }));
    const failures = settled.filter(result => result.status === 'rejected');
    if (failures.length) throw new AggregateError(failures.map(result => result.reason), 'CPU verification workers failed');
    return validateWorkerRoster(settled.flatMap(result => result.value));
  } finally { rmSync(parent, { recursive: true, force: true }); }
}
