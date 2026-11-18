import assert from 'node:assert/strict';
import { existsSync } from 'node:fs';
import { createRequire } from 'node:module';
import test from 'node:test';

const built = new URL('../hosts/wasm/pkg/node/safe_cex_wasm.js', import.meta.url);
const module = existsSync(built) ? createRequire(import.meta.url)(built.pathname) : undefined;

test('BingX Spot byte ingress survives memory growth and later small inputs', {
  skip: typeof module?.bingxSpotReplayFixture !== 'function',
}, () => {
  const engine = module.bingxSpotReplayFixture(true);
  const encoder = new TextEncoder();
  try {
    assert.equal(engine.wsMessageBytes(7, encoder.encode('{}'), 1), undefined);
    const before = module.wasmMemoryBytes();
    const large = new Uint8Array(before + 65_536).fill(32);
    assert.equal(engine.wsMessageBytes(7, large, 1), undefined);
    assert.ok(module.wasmMemoryBytes() > before, 'the control must actually grow linear memory');
    for (const text of ['', '{}', '{"note":"é漢🦀"}']) {
      const bytes = encoder.encode(text);
      const backing = new Uint8Array(bytes.length + 2).fill(255);
      backing.set(bytes, 1);
      assert.equal(engine.wsMessageBytes(7, backing.subarray(1, -1), 1), undefined);
    }
    assert.equal(engine.wsMessageBytes(7, new Uint8Array([255]), 1), undefined);
  } finally {
    engine.free();
  }
});
