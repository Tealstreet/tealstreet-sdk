// Every family package built by `yarn rust:build-wasm --families` loads in a
// fresh web initialization, constructs each of its venues, refuses every
// other family's venues and fits the single-venue size budget.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync, statSync } from 'node:fs';
import { venueFamilies, venueFamily } from '../hosts/wasm/js/venues.mjs';
import { wasmBudgetError } from './wasm-budget.mjs';
import { checkAdvertisedCommands, constructionProfiles } from './capability-conformance.mjs';
import { checkHyperliquidLocalSigning } from './hyperliquid-local-signing.mjs';
import { checkParadexLocalSigning } from './paradex-local-signing.mjs';

const pkg = new URL('../hosts/wasm/pkg/', import.meta.url);
const venues = readFileSync(new URL('../hosts/wasm/js/venues.mjs', import.meta.url), 'utf8')
  .match(/^\s+'?[a-z-]+'?: '[a-z]+',$/gm)
  .map((line) => line.trim().split(':')[0].replaceAll("'", ''));

test('the venue list is read from the family map', () => {
  assert.equal(venues.length, 50);
  for (const venue of venues) assert.doesNotThrow(() => venueFamily(venue), venue);
});

const selectedFamilies = process.env.RUST_TEST_FAMILY ? [process.env.RUST_TEST_FAMILY] : venueFamilies;
if (selectedFamilies.some(family => !venueFamilies.includes(family))) throw new Error('Unknown family test shard');
for (const family of selectedFamilies) {
  test(`${family} family package`, async () => {
    for (const target of ['web', 'node']) {
      const size = statSync(new URL(`${target}/${family}/safe_cex_wasm_bg.wasm`, pkg)).size;
      // One source of truth for the single-venue budget: wasm-budget.mjs.
      const error = wasmBudgetError(size, 'venue', family);
      assert.equal(error, undefined, `${target} ${family}: ${error}`);
    }
    const engine = await import(new URL(`web/${family}/safe_cex_wasm.js`, pkg));
    await engine.default({ module_or_path: readFileSync(new URL(`web/${family}/safe_cex_wasm_bg.wasm`, pkg)) });
    if (family === 'hyperliquid') {
      await checkHyperliquidLocalSigning(engine.WasmEngine, false);
      await checkHyperliquidLocalSigning(engine.WasmEngine, true);
    }
    if (family === 'paradex') await checkParadexLocalSigning(engine.WasmEngine);
    if (family === 'apexomni') {
      const corpus = JSON.parse(readFileSync(new URL('../crypto/tests/fixtures/zklink/npm-wasm.json', import.meta.url)));
      const seed = corpus.cases[0].seed;
      const primary = { key: 'api-key', secret: 'bXlzZWNyZXQ=', passphrase: 'pass' };
      for (const [options, setup] of [
        [{ ...primary, omniKeySeed: seed, publicMode: { requested: false } }, undefined],
        [{ publicMode: { requested: false } }, JSON.stringify({ primary: { ...primary, extra: { zklinkSeed: { value: seed, encoding: 'hex' } } } })],
      ]) {
        const instance = new engine.WasmEngine('apexomni', JSON.stringify(options), setup);
        try {
          const construction = JSON.parse(instance.construction());
          assert.equal(construction.canSign, true);
          assert.equal(construction.publicDataOnly, false);
          assert.equal(instance.requiresExternalSigner(), false);
          assert(JSON.parse(instance.start(1000)).some(action => action.op === 'managedHttp'));
        } finally {
          instance.free();
        }
      }
    }
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
