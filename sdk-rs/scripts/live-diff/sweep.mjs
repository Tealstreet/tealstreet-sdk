import { spawn } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { venueHosts } from './venues.mjs';

const selected = process.argv.slice(2);
let failed = false;
for (const venue of selected.length ? selected : Object.keys(venueHosts)) {
  const code = await new Promise((resolve) => {
    const child = spawn(process.execPath, [fileURLToPath(new URL('./run.mjs', import.meta.url)), '--venue', venue], {
      stdio: 'inherit',
    });
    child.on('exit', (code) => resolve(code));
  });
  if (code !== 0) failed = true;
}
process.exitCode = failed ? 1 : 0;
