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
import { ExchangeError } from './host.mjs';
import { venueFamilies, venueFamily } from './venues.mjs';

const OUT = [
  'polymarket', 'txflow', 'weex',
];

test('the beta set is exactly the 45 IN venues in 24 families', () => {
  assert.equal(betaVenueNames.length, 45);
  assert.deepEqual(betaVenueFamilies, [
    'apexomni', 'asterdex', 'backpack', 'binance', 'bingx', 'bitget', 'bitrue', 'bitunix', 'blofin', 'bybit', 'coinbase',
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
    assert.throws(() => betaVenueFamily(venue), (error) => error instanceof BetaVenueUnavailableError && error.venue === venue
      && error instanceof ExchangeError && error.kind === 'unsupported' && error.outcome === 'notSent');
  }
});

test('retired venues still saved on accounts refuse typed, by name', () => {
  for (const venue of ['bitmex', 'bitmextestnet']) {
    for (const resolve of [venueFamily, betaVenueFamily]) {
      assert.throws(() => resolve(venue), (error) => error instanceof ExchangeError
        && error.kind === 'unsupported' && error.outcome === 'notSent'
        && error.message === `unsupported Rust venue: ${venue}`);
    }
  }
});

test('the beta loader refuses an OUT venue before importing any engine', async () => {
  const imports = [];
  const load = familyLoader((family) => {
    imports.push(family);
    return { default: async () => {} };
  }, betaVenueFamily);
  await assert.rejects(load('weex'), BetaVenueUnavailableError);
  await load('bitrue');
  await load('okxeu');
  await load('lighter-robinhood');
  assert.deepEqual(imports, ['bitrue', 'okx', 'lighter']);
});

test('engine.beta.web.mjs is generated from the table and imports only beta families', () => {
  const loader = readFileSync(BETA_LOADER, 'utf8');
  assert.equal(loader, betaLoaderSource(), 'run node js/generate-beta-loader.mjs');
  const imported = [...loader.matchAll(/import\('\.\.\/pkg\/web\/([a-z]+)\/safe_cex_wasm\.js'\)/g)].map((match) => match[1]);
  assert.deepEqual(imported, betaVenueFamilies);
});

test('BingX Spot belongs to the native family but stays outside beta admission until smoke', () => {
  assert.equal(venueFamily('bingxspot'), 'bingx');
  assert.throws(() => betaVenueFamily('bingxspot'), BetaVenueUnavailableError);
});
