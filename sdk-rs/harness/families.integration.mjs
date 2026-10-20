// Every family package built by `yarn rust:build-wasm --families` loads in a
// fresh web initialization, constructs each of its venues, refuses every
// other family's venues and fits the single-venue size budget.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync, statSync } from 'node:fs';
import { venueFamilies, venueFamily } from '../hosts/wasm/js/venues.mjs';

const pkg = new URL('../hosts/wasm/pkg/', import.meta.url);
const budget = Number(process.env.WASM_BUDGET_BYTES);
const venues = readFileSync(new URL('../hosts/wasm/js/venues.mjs', import.meta.url), 'utf8')
  .match(/^\s+'?[a-z-]+'?: '[a-z]+',$/gm)
  .map((line) => line.trim().split(':')[0].replaceAll("'", ''));

test('the venue list is read from the family map', () => {
  assert.equal(venues.length, 49);
  for (const venue of venues) assert.doesNotThrow(() => venueFamily(venue), venue);
});

for (const family of venueFamilies) {
  test(`${family} family package`, async () => {
    for (const target of ['web', 'node']) {
      const size = statSync(new URL(`${target}/${family}/safe_cex_wasm_bg.wasm`, pkg)).size;
      if (budget) assert.ok(size <= budget, `${target} ${family}: ${size} bytes exceeds ${budget}`);
    }
    const engine = await import(new URL(`web/${family}/safe_cex_wasm.js`, pkg));
    await engine.default({ module_or_path: readFileSync(new URL(`web/${family}/safe_cex_wasm_bg.wasm`, pkg)) });
    for (const venue of venues) {
      let error;
      try {
        new engine.WasmEngine(venue, JSON.stringify({ publicDataOnly: true })).free();
      } catch (thrown) {
        error = thrown;
      }
      if (venueFamily(venue) === family)
        assert.doesNotMatch(String(error?.message ?? ''), /unsupported venue/, `${venue} is missing from ${family}`);
      else assert.match(String(error?.message), /unsupported venue/, `${family} also contains ${venue}`);
    }
  });
}
