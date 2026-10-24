// Every family package built by `yarn rust:build-wasm --families` loads in a
// fresh web initialization, constructs each of its venues, refuses every
// other family's venues and fits the single-venue size budget.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync, statSync } from 'node:fs';
import { venueFamilies, venueFamily } from '../hosts/wasm/js/venues.mjs';
import { wasmBudgetError } from './wasm-budget.mjs';
import { checkAdvertisedCommands, constructionProfiles } from './capability-conformance.mjs';

const pkg = new URL('../hosts/wasm/pkg/', import.meta.url);
const venues = readFileSync(new URL('../hosts/wasm/js/venues.mjs', import.meta.url), 'utf8')
  .match(/^\s+'?[a-z-]+'?: '[a-z]+',$/gm)
  .map((line) => line.trim().split(':')[0].replaceAll("'", ''));

test('the venue list is read from the family map', () => {
  assert.equal(venues.length, 50);
  for (const venue of venues) assert.doesNotThrow(() => venueFamily(venue), venue);
});

for (const family of venueFamilies) {
  test(`${family} family package`, async () => {
    for (const target of ['web', 'node']) {
      const size = statSync(new URL(`${target}/${family}/safe_cex_wasm_bg.wasm`, pkg)).size;
      // One source of truth for the single-venue budget: wasm-budget.mjs.
      const error = wasmBudgetError(size, 'venue', family);
      assert.equal(error, undefined, `${target} ${family}: ${error}`);
    }
    const engine = await import(new URL(`web/${family}/safe_cex_wasm.js`, pkg));
    await engine.default({ module_or_path: readFileSync(new URL(`web/${family}/safe_cex_wasm_bg.wasm`, pkg)) });
    for (const venue of venues) {
      if (venueFamily(venue) === family) {
        for (const [profile, options] of constructionProfiles(venue).entries()) {
          const label = `${family}/${venue}/profile${profile}`;
          const proofs = checkAdvertisedCommands(() => new engine.WasmEngine(venue, JSON.stringify(options)), label);
          // Descriptive coverage only: preconditions/deferred responses are not
          // positive valid-state handler proof (covered separately by native fixtures).
          console.log(JSON.stringify({ conformance: label, ingress: proofs.map(({ field, op, evidence }) => ({ field, op, evidence })) }));
        }
      } else {
        assert.throws(() => new engine.WasmEngine(venue, JSON.stringify({ publicDataOnly: true })),
          /unsupported venue/, `${family} also contains ${venue}`);
      }
    }
  });
}
