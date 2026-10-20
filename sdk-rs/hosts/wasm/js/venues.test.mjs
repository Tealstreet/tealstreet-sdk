import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { test } from 'node:test';
import { venueFamily, venueFamilies } from './venues.mjs';
import { loadEngine } from './engine.node.mjs';

const require = createRequire(import.meta.url);
const aliases = {
  txflow: ['txflow'],
  coinbase: ['coinbase', 'coinbasespot'],
  polymarket: ['polymarket'],
  grvt: ['grvt'],
  breakoutprop: ['breakoutprop'],
  phemex: ['phemex'],
  paradex: ['paradex'],
  apexomni: ['apexomni'],
  kraken: ['kraken', 'krakenspot'],
  asterdex: ['asterdex'],
  kucoin: ['kucoin'],
  kucoinspot: ['kucoinspot'],
  backpack: ['backpack'],
  binance: ['binance', 'binancepm', 'binancespot', 'binanceinverse'],
  bingx: ['bingx'],
  bitget: ['bitget', 'bitgetusdc', 'bitgetinverse'],
  bitrue: ['bitrue'],
  blofin: ['blofin'],
  bybit: ['bybit', 'bybitinverse', 'bybitspot', 'bybitdemo', 'hyrotrader'],
  cryptocom: ['cryptocom'],
  hyperliquid: ['hyperliquid', 'hyperliquidspot'],
  lighter: ['lighter', 'lighter-robinhood'],
  mexc: ['mexc'],
  orderly: ['orderly', 'tealstreet-dex', 'kodiakfi', 'mode', 'salsadex', 'what', 'woofi'],
  okx: ['okx', 'okxeu', 'okxdemo', 'okxeudemo'],
  weex: ['weex'],
  woo: ['woo'],
};

test('canonical family inventory contains every family once and stays immutable', () => {
  assert.deepEqual(venueFamilies, Object.keys(aliases));
  assert(Object.isFrozen(venueFamilies));
  assert.throws(() => venueFamilies.push('unknown'), TypeError);
});

for (const [family, names] of Object.entries(aliases)) {
  test(`${family} package accepts its aliases and excludes other venues`, () => {
    const { WasmEngine } = require(`../pkg/node/${family}/safe_cex_wasm.js`);
    for (const method of ['payloadSigned', 'payloadSignFailed'])
      assert.equal(typeof WasmEngine.prototype[method], family === 'paradex' ? 'function' : 'undefined',
        `${family} delegated signing ABI: ${method}`);
    for (const venue of names) {
      assert.equal(venueFamily(venue), family);
      const options = venue === 'hyrotrader'
        ? { publicDataOnly: true, endpoints: { rest: 'https://example.test', publicWs: 'wss://example.test/public', privateWs: 'wss://example.test/private' } }
        : { publicDataOnly: true, ...(venue === 'txflow' ? { testnet: true } : {}) };
      const engine = new WasmEngine(venue, JSON.stringify(options));
      engine.free();
    }
    for (const other of Object.keys(aliases).filter((name) => name !== family)) {
      assert.throws(() => new WasmEngine(other, '{}'), /unsupported venue/);
    }
  });
}

test('default Node loader shares each selected family with explicit requires', async () => {
  const modules = new Set();
  for (const [family, names] of Object.entries(aliases)) {
    const explicit = require(`../pkg/node/${family}/safe_cex_wasm.js`);
    const defaults = await Promise.all(names.map(loadEngine));
    for (const selected of defaults) assert.equal(selected, explicit);
    modules.add(explicit);
  }
  assert.equal(modules.size, Object.keys(aliases).length);
});

test('unknown aliases fail before resolving a module', () => {
  for (const name of ['', 'unknown', 'constructor', '__proto__', 'okx/../../binance']) {
    assert.throws(() => venueFamily(name), /unsupported Rust venue/);
  }
});
