import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { test } from 'node:test';
import { venueFamily } from './venues.mjs';

const require = createRequire(import.meta.url);
const aliases = {
  binance: ['binance', 'binancepm'],
  bybit: ['bybit'],
  cryptocom: ['cryptocom'],
  okx: ['okx', 'okxeu', 'okxdemo', 'okxeudemo'],
  weex: ['weex'],
};

for (const [family, names] of Object.entries(aliases)) {
  test(`${family} package accepts its aliases and excludes other venues`, () => {
    const { WasmEngine } = require(`../pkg/node/${family}/safe_cex_wasm.js`);
    for (const venue of names) {
      assert.equal(venueFamily(venue), family);
      const engine = new WasmEngine(venue, '{"publicDataOnly":true}');
      engine.free();
    }
    for (const other of Object.keys(aliases).filter((name) => name !== family)) {
      assert.throws(() => new WasmEngine(other, '{}'), /unsupported venue/);
    }
  });
}

test('unknown aliases fail before resolving a module', () => {
  for (const name of ['', 'unknown', 'constructor', '__proto__', 'okx/../../binance']) {
    assert.throws(() => venueFamily(name), /unsupported Rust venue/);
  }
});
