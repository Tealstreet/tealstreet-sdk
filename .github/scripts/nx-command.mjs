import { spawnSync } from 'node:child_process';
import { closeSync, fchmodSync, openSync } from 'node:fs';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';

// NX is private even though this workflow runs on a public mirror.
export function runNxCommand(root, label, command, args, directory = process.env.RUNNER_TEMP) {
  if (!directory || !/^[a-z][a-z0-9-]*$/.test(label)) throw new Error('Missing private log directory or invalid gate label');
  const log = openSync(join(directory, `nx-${label}.log`), 'w', 0o600);
  fchmodSync(log, 0o600);
  let child;
  try {
    child = spawnSync(command, args, { cwd: root, stdio: ['ignore', log, log] });
  } finally {
    closeSync(log);
  }
  if (child.error || child.status !== 0) {
    throw new Error(`NX ${label} failed (${child.error?.code ?? child.status ?? child.signal}); private output is retained only in the runner temporary directory`);
  }
  console.log(`NX ${label}: success`);
}

if (import.meta.url === pathToFileURL(process.argv[1] ?? '').href) {
  try {
    const [root, label, command, ...args] = process.argv.slice(2);
    runNxCommand(root, label, command, args);
  } catch (error) {
    console.error(error.message);
    process.exitCode = 1;
  }
}
