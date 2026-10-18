// Execute the real native Auth/PrivateState methods; never classify raw APIs typed.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import {spawnSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
const root = fileURLToPath(new URL('../..', import.meta.url));
const args = process.argv.slice(2);
assert.ok(args.every(arg => ['--native', '--expect-unclassified', '--expect-mismatch'].includes(arg)
  || /^--only=(Auth|PrivateState)$/.test(arg)));
assert.equal(new Set(args).size, args.length);
const only = args.filter(arg => arg.startsWith('--only='));
assert.ok(only.length <= 1);
assert.ok(!(args.includes('--expect-unclassified') && args.includes('--expect-mismatch')));
const native = args.includes('--native') || args.includes('--expect-unclassified') || args.includes('--expect-mismatch');
const owners = (only.length ? [only[0].slice(7)] : ['Auth', 'PrivateState'])
  .map(name => `safe_cex_venue_binance::private_classes::${name}`).sort();
const target = path.resolve(root, process.env.CARGO_TARGET_DIR ?? 'target/accepted-inventory');
const run = spawnSync(process.execPath, [
  'safe-cex-parity/scripts/accepted-set.mjs', '--package=safe-cex-venue-binance',
  '--jobs=1', '--measure', ...(native ? ['--features=native'] : []),
  ...owners.map(owner => `--owner=${owner}`),
], {cwd: root, stdio: 'inherit', env: {...process.env, CARGO_TARGET_DIR: target,
  // Keep all compiler commands consistent inside the isolated rustdoc target.
  RUSTC_BOOTSTRAP: '1'}});
assert.equal(run.status, 0, `${run.error ?? 'canonical native measurement failed'}`);
const rows = JSON.parse(fs.readFileSync(path.join(target, 'accepted-set-results.json')));
assert.deepEqual([...new Set(rows.map(row => row.name))].sort(), owners);
for (const owner of owners) {
  const own = rows.filter(row => row.name === owner);
  const executed = own.filter(row => row.configuration.target === 'native'
    && row.compilerFeatures.includes('native') && !row.classificationOnly);
  assert.equal(executed.length, 1, `${owner}: real native receipt required`);
  if (args.includes('--expect-unclassified')) {
    assert.ok(executed[0].failures.some(failure => failure.includes('unclassified deserializable type')), owner);
  } else if (args.includes('--expect-mismatch')) {
    assert.ok(executed[0].failures.some(failure => failure.includes('function output mismatch')), owner);
  } else {
    assert.deepEqual(executed[0].failures, [], owner);
    assert.ok(own.every(row => row.failures.length === 0), owner);
    if (!native) {
      assert.equal(own.length, 2, owner);
      assert.equal(own.filter(row => row.status === 'native-gated' && row.observed === false).length, 1, owner);
    } else assert.equal(own.length, 1, owner);
  }
}
console.log(`${owners.length} real native private owners: ${args.includes('--expect-unclassified') ? 'named missing-contract RED' : args.includes('--expect-mismatch') ? 'compiled output-mismatch RED' : 'executed GREEN'}${native ? '' : '; explicit default native-gated receipts retained'}.`);
