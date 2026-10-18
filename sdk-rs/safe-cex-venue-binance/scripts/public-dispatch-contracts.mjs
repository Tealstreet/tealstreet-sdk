import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const workspace = fileURLToPath(new URL('../..', import.meta.url));
const names = ['public_dispatch::Handlers', 'public_dispatch::dispatch_binance_message',
  'ticker_capture::Capture', 'ticker_capture::lookup']
  .map(name => `safe_cex_venue_binance::${name}`).sort();
const args = process.argv.slice(2);
assert.ok(args.length <= 1 && args.every(arg => arg === '--expect-unclassified'));
const target = path.join(workspace, 'target/accepted-inventory');
const run = spawnSync(process.execPath, [
  path.join(workspace, 'safe-cex-parity/scripts/accepted-set.mjs'),
  '--package=safe-cex-venue-binance', '--jobs=1', '--measure',
  ...names.map(name => `--owner=${name}`),
], { cwd: workspace, encoding: 'utf8', maxBuffer: 64 * 1024 * 1024,
  env: { ...process.env, CARGO_BUILD_JOBS: '1', CARGO_TARGET_DIR: target },
});
assert.equal(run.status, 0, `${run.error ?? ''}\n${run.stdout}\n${run.stderr}`);
process.stdout.write(run.stdout);
const results = JSON.parse(fs.readFileSync(path.join(target, 'accepted-set-results.json'), 'utf8'));
assert.deepEqual(results.map(result => result.name).sort(), names);
if (args.includes('--expect-unclassified')) {
  assert.ok(results.every(result => result.failures.some(failure =>
    failure.includes('unclassified deserializable type'))));
  console.log('All four public dispatch/capture owners fail missing classification by name.');
} else {
  for (const result of results) assert.deepEqual(result.failures, [], result.name);
  console.log('All four public dispatch/capture owner contracts pass.');
}
