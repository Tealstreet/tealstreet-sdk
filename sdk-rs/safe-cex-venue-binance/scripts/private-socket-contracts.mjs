// Exact typed owner classification; native effects live in private_owner_binding.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import {spawnSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';

const workspace = fileURLToPath(new URL('../..', import.meta.url));
const owner = 'safe_cex_venue_binance::private_classes::PrivateSocket';
const args = process.argv.slice(2);
assert.ok(args.length <= 1 && args.every(arg => arg === '--expect-unclassified'));
// Use the canonical driver for compiler provenance, actual Cargo features and
// transitive Deserialize discovery. Keep rustdoc outside the ordinary target.
const target = path.resolve(workspace, process.env.CARGO_TARGET_DIR ?? 'target/accepted-inventory');
const result = spawnSync(process.execPath, [
  'safe-cex-parity/scripts/accepted-set.mjs', '--package=safe-cex-venue-binance',
  '--jobs=1', '--measure', `--owner=${owner}`,
], {cwd: workspace, stdio: 'inherit', env: {...process.env, CARGO_TARGET_DIR: target}});
assert.equal(result.status, 0, `${result.error ?? 'canonical measurement failed'}`);
const results = JSON.parse(fs.readFileSync(path.join(target, 'accepted-set-results.json')));
assert.deepEqual(results.map(row => row.name), [owner]);
const [receipt] = results;
if (args.includes('--expect-unclassified')) {
  assert.ok(receipt.failures.some(failure => failure.includes('unclassified deserializable type')));
} else {
  assert.deepEqual(receipt.failures, [], owner);
  assert.equal(receipt.classificationOnly, true, owner);
  assert.equal(receipt.alternateOnly, true, owner);
  assert.equal(receipt.category, 'reviewed-api', owner);
}
console.log(`PrivateSocket: ${args.length ? 'named missing-contract RED' : 'typed classification GREEN'}; native execution is separate.`);
