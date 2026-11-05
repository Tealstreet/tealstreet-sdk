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
  'polymarket', 'breakoutprop', 'txflow', 'weex',
  'bitrue',
];

test('the beta set is exactly the 44 IN venues in 23 families', () => {
  assert.equal(betaVenueNames.length, 44);
  assert.deepEqual(betaVenueFamilies, [
    'apexomni', 'asterdex', 'backpack', 'binance', 'bingx', 'bitget', 'bitunix', 'blofin', 'bybit', 'coinbase',
    'cryptocom', 'grvt', 'hyperliquid', 'kraken', 'kucoin', 'kucoinspot', 'lighter', 'mexc', 'okx', 'orderly',
    'paradex', 'phemex', 'woo',
  ]);
});

test('retired brokers are rejected while all live Orderly names keep their family', () => {
  for (const venue of ['salsadex', 'tealstreet-dex']) {
    assert.throws(() => venueFamily(venue), /unsupported Rust venue/);
    assert.throws(() => betaVenueFamily(venue), /unsupported Rust venue/);
  }
  for (const venue of ['orderly', 'woofi', 'kodiakfi', 'mode', 'what']) {
    assert.equal(venueFamily(venue), 'orderly');
    assert.equal(betaVenueFamily(venue), 'orderly');
  }
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
  await assert.rejects(load('bitrue'), BetaVenueUnavailableError);
  await load('okxeu');
  await load('lighter-robinhood');
  assert.deepEqual(imports, ['okx', 'lighter']);
});

test('engine.beta.web.mjs is generated from the table and imports only beta families', () => {
  const loader = readFileSync(BETA_LOADER, 'utf8');
  assert.equal(loader, betaLoaderSource(), 'run node js/generate-beta-loader.mjs');
  const imported = [...loader.matchAll(/import\('\.\.\/pkg\/web\/([a-z]+)\/safe_cex_wasm\.js'\)/g)].map((match) => match[1]);
  assert.deepEqual(imported, betaVenueFamilies);
});
