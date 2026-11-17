// Run once with the shipped family and once with the explicit fixture feature.
import assert from 'node:assert/strict';
import test from 'node:test';
import { loadEngine } from './engine.mjs';
const module = await loadEngine();
const fixtureBuild = process.env.BINGX_SPOT_FIXTURE === '1';

test('ordinary Spot factories construct public and local-key accounts', () => {
  for (const options of [{ publicDataOnly: true }, { key: 'replay-key', secret: 'replay-secret' }]) {
    const engine = new module.WasmEngine('bingxspot', JSON.stringify(options));
    assert.equal(typeof engine.start, 'function');
    engine.free();
  }
});
test('Spot testnet is refused before an engine exists', () => {
  assert.throws(() => new module.WasmEngine('bingxspot', JSON.stringify({ publicDataOnly: true, testnet: true })),
    error => error.kind === 'unsupported' && error.outcome === 'notSent' && /testnet/.test(error.message));
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
