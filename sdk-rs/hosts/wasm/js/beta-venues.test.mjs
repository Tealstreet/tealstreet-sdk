import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import {
  BetaVenueUnavailableError,
  betaVenueFamilies,
  betaVenueFamily,
  betaVenueNames,
} from './beta-venues.mjs';
import { familyLoader } from './family-loader.mjs';
import { BETA_LOADER, betaLoaderSource } from './generate-beta-loader.mjs';
import { venueFamilies, venueFamily } from './venues.mjs';

const OUT = [
  'woo', 'mexc', 'grvt', 'backpack', 'coinbase', 'coinbasespot', 'orderly', 'kodiakfi', 'mode', 'salsadex',
  'tealstreet-dex', 'what', 'woofi', 'kucoin', 'kucoinspot', 'polymarket', 'breakoutprop', 'txflow', 'weex',
  'bitrue', 'hyrotrader', 'lighter-robinhood', 'bitgetinverse',
];

test('the beta set is exactly the 27 IN venues in 15 families', () => {
  assert.equal(betaVenueNames.length, 27);
  assert.deepEqual(betaVenueFamilies, [
    'apexomni', 'asterdex', 'binance', 'bingx', 'bitget', 'bitunix', 'blofin', 'bybit', 'cryptocom',
    'hyperliquid', 'kraken', 'lighter', 'okx', 'paradex', 'phemex',
  ]);
});

test('every beta venue keeps the family the full registry gives it', () => {
  for (const venue of betaVenueNames) assert.equal(betaVenueFamily(venue), venueFamily(venue), venue);
  for (const family of betaVenueFamilies) assert.ok(venueFamilies.includes(family), family);
});

test('OUT venues are refused by name and unknown ones as before', () => {
  for (const venue of OUT) {
    assert.doesNotThrow(() => venueFamily(venue), venue);
    assert.throws(() => betaVenueFamily(venue), (error) => error instanceof BetaVenueUnavailableError && error.venue === venue);
  }
  assert.throws(() => betaVenueFamily('bitmex'), /unsupported Rust venue: bitmex/);
});

test('the beta loader refuses an OUT venue before importing any engine', async () => {
  const imports = [];
  const load = familyLoader((family) => {
    imports.push(family);
    return { default: async () => {} };
  }, betaVenueFamily);
  await assert.rejects(load('woo'), BetaVenueUnavailableError);
  await load('okxeu');
  assert.deepEqual(imports, ['okx']);
});

test('engine.beta.web.mjs is generated from the table and imports only beta families', () => {
  const loader = readFileSync(BETA_LOADER, 'utf8');
  assert.equal(loader, betaLoaderSource(), 'run node js/generate-beta-loader.mjs');
  const imported = [...loader.matchAll(/import\('\.\.\/pkg\/web\/([a-z]+)\/safe_cex_wasm\.js'\)/g)].map((match) => match[1]);
  assert.deepEqual(imported, betaVenueFamilies);
});
