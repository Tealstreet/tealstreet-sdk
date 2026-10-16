const assert = require('node:assert/strict');
const path = require('node:path');
const { execFileSync } = require('node:child_process');
const run = (file, args = []) => execFileSync(process.execPath, [file, ...args], { encoding: 'utf8' });
const output = run(path.resolve(__dirname, '../utils_final_oracle.cjs'));
assert.ok(output.trim().startsWith('{'), 'UTILITY SOURCE producer emits JSON');
const value = JSON.parse(output);
assert.deepEqual(value.secondary, [
  'apexomni', 'breakoutprop', 'grvt', 'hyperliquid', 'hyperliquidspot',
  'lighter', 'lighter-robinhood', 'paradex', 'polymarket', 'woofi',
  'kodiakfi', 'mode', 'salsadex', 'tealstreet-dex', 'what',
], 'UTILITY SOURCE fifteen source exclusions');
assert.deepEqual(value.unknown, [
  false, false, false, true, false, true, true, false,
  true, false, true, false, true, true, false, true,
], 'UTILITY SOURCE sixteen source classifications');
assert.equal(value.uuid, 'useandom26T198340PX7', 'UTILITY SOURCE deterministic nanoid value');
assert.deepEqual(value.nativeBlob, [[0, 65, 127, 128, 255, 13], [0, 65, 127, 128, 255, 13]],
  'UTILITY SOURCE two wrapped byte projections');
const clock = run(path.resolve(__dirname, '../../safe-cex-store/tests/clock_oracle.mjs'), ['--check']);
assert.equal(clock.trim(), '19 clock source-oracle cases verified', 'UTILITY CLOCK completed nineteen-case marker');
console.log('UTILITY SOURCE guarded two producers: 15 exclusions, 16 classifiers, 19 clock cases');
