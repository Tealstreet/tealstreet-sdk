// Run once with the shipped family and once with the explicit fixture feature.
import assert from 'node:assert/strict';
import test from 'node:test';
import { loadEngine } from './engine.mjs';
const module = await loadEngine();
const fixtureBuild = process.env.BINGX_SPOT_FIXTURE === '1';

test('every normal Spot constructor still refuses before an engine exists', () => {
  for (const options of [{}, { publicDataOnly: true }, { testnet: true }, { key: 'replay-key', secret: 'replay-secret' }]) {
    assert.throws(() => new module.WasmEngine('bingxspot', JSON.stringify(options)), error => {
      assert.match(error.message, /unsupported venue: bingxspot/);
      return true;
    });
  }
});

test('only the explicit fixture build exports the fixed-credential constructor', () => {
  assert.equal(typeof module.bingxSpotReplayFixture, fixtureBuild ? 'function' : 'undefined');
  if (fixtureBuild) {
    for (const publicOnly of [false, true]) {
      const engine = module.bingxSpotReplayFixture(publicOnly);
      assert.equal(typeof engine.start, 'function');
      engine.free();
    }
  }
});
