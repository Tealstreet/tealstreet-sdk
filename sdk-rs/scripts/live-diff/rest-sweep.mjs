import { spawn } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { restVenueHosts as venueHosts } from './rest-venues.mjs';

const args = process.argv.slice(2),
  forwarded = [],
  venues = [];
for (let i = 0; i < args.length; i++) {
  if (['--addon', '--rust-revision', '--timeout-ms'].includes(args[i])) forwarded.push(args[i], args[++i]);
  else if (!Object.hasOwn(venueHosts, args[i])) throw new Error(`Unknown REST venue/option ${args[i]}`);
  else venues.push(args[i]);
}
let failed = false;
for (const venue of venues.length ? venues : Object.keys(venueHosts)) {
  const code = await new Promise((resolve, reject) => {
    const child = spawn(
      process.execPath,
      [fileURLToPath(new URL('./rest.mjs', import.meta.url)), '--venue', venue, ...forwarded],
      { stdio: 'inherit' },
    );
    child.on('error', reject);
    child.on('exit', resolve);
  });
  if (code !== 0) failed = true;
}
process.exitCode = failed ? 1 : 0;
