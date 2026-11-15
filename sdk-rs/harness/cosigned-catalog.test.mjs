import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import test from 'node:test';
import { venueFamily } from '../hosts/wasm/js/venues.mjs';

const read = path => readFileSync(new URL(path, import.meta.url), 'utf8');
const rules = JSON.parse(read('../../packages/safe-cex/src/exchanges/rust/construction-rules.json')).venues;
const metadata = JSON.parse(read('../safe-cex-types/src/exchange_metadata.json')).metadata;
const manifest = fileURLToPath(new URL('../registry/Cargo.toml', import.meta.url));
const features = JSON.parse(execFileSync('python3', ['-c',
  'import json,sys,tomllib; print(json.dumps(tomllib.load(open(sys.argv[1], "rb"))["features"]))',
  manifest], { encoding: 'utf8' }));
const catalogSource = read('../hosts/server/src/delegated/probe.rs');
const exclusionRecords = JSON.parse(read('../hosts/server/src/delegated/cosigned-proof-exclusions.json')).exclusions;

// Census/client refusesHostedInit gates, not a new runtime policy. Cover ALL
// declarations below, so a blocked/disabled venue cannot disappear from review.
const noServerOrigin = new Set(['hyrotrader', 'binanceoptions']);
const awaitingLiveTest = new Set(['paradex']);

function exactNames(names) {
  assert(Array.isArray(names) && names.length > 0, 'expected a nonempty venue list');
  for (const name of names) {
    assert.match(name, /^[a-z][a-z0-9-]*$/);
    assert(Object.hasOwn(rules, name), `unknown exact construction-rule venue: ${name}`);
  }
  assert.equal(new Set(names).size, names.length, 'duplicate exact venue');
  return new Set(names);
}

function catalog(source) {
  const matches = [...source.matchAll(/const PRIVATE_STARTUP_VENUES\s*:\s*&\[\s*&str\s*\]\s*=\s*&\[([\s\S]*?)\]\s*;/g)];
  assert.equal(matches.length, 1, 'expected exactly one literal PRIVATE_STARTUP_VENUES');
  const body = matches[0][1].replace(/\/\/[^\n]*/g, '').trim().replace(/,\s*$/, '');
  return exactNames(JSON.parse(`[${body}]`));
}

function exclusions(records) {
  assert(Array.isArray(records), 'expected named proof exclusions');
  const result = new Map();
  for (const entry of records) {
    exactNames([entry.venue]);
    assert.equal(typeof entry.reason, 'string', `${entry.venue}: missing exclusion reason`);
    assert(entry.reason.trim(), `${entry.venue}: empty exclusion reason`);
    assert(!result.has(entry.venue), `${entry.venue}: duplicate exclusion`);
    result.set(entry.venue, entry.reason);
  }
  return result;
}

function accounted(declared, ready, excluded) {
  const missing = [...declared].filter(name => !ready.has(name) && !excluded.has(name)).sort();
  assert.equal(missing.length, 0, `Unaccounted Hosted venues: ${missing.join(', ')}`);
  for (const name of ready) assert(!excluded.has(name), `${name}: remove its exclusion when its proof lands`);
  for (const name of excluded.keys()) assert(declared.has(name), `${name}: stale exclusion without remoteSigning`);
}

function enabledFeatures(root, seen = new Set()) {
  assert(Object.hasOwn(features, root), `missing registry feature: ${root}`);
  if (seen.has(root)) return seen;
  seen.add(root);
  for (const child of features[root]) {
    if (Object.hasOwn(features, child)) enabledFeatures(child, seen);
  }
  return seen;
}

const declared = exactNames(Object.entries(rules)
  .filter(([, rule]) => rule.remoteSigning?.enabled === true).map(([name]) => name));
const ready = catalog(catalogSource);
const excluded = exclusions(exclusionRecords);
const helperEligible = new Set([...declared].filter(name => !noServerOrigin.has(name) && !awaitingLiveTest.has(name)));
// This generated snapshot is checked by rust:metadata:check. It only reconciles
// the production cohort; the stronger all-declarations guard does not filter on it.
const productionOffered = new Set([...helperEligible].filter(name => {
  assert.equal(typeof metadata[name]?.enabled, 'boolean', `${name}: missing production metadata`);
  return metadata[name].enabled;
}));

test('every declared remoteSigning venue has an exact proof or a reason', () => {
  accounted(declared, ready, excluded);
});

test('production cohort reproduces the 41 / 38 / 33 census without hiding disabled venues', () => {
  assert.equal(declared.size, 41);
  assert.equal(helperEligible.size, 38);
  assert.equal(productionOffered.size, 33);
  assert.deepEqual([...helperEligible].filter(name => !productionOffered.has(name)).sort(),
    ['bingxspot', 'bitgetinverse', 'bitrue', 'lighter-robinhood', 'txflow']);
  for (const name of ready) assert(declared.has(name), `${name}: catalog has no remoteSigning declaration`);
  accounted(productionOffered, ready, new Map([...excluded].filter(([name]) => productionOffered.has(name))));
});

test('registry hosted-signing is a secondary family cross-check, not the offered roster', () => {
  const venueFeatures = Object.entries(features)
    .filter(([, values]) => values.some(value => value.startsWith('dep:safe-cex-venue-')))
    .map(([name]) => name);
  const hostedSigning = new Set(venueFeatures.filter(name => enabledFeatures(name).has('hosted-signing')));
  assert.equal(hostedSigning.size, 19);
  const declaredFamilies = new Set([...declared].map(venueFamily));
  for (const family of hostedSigning) assert(declaredFamilies.has(family), `${family}: registry codec has no declared Hosted venue`);
  for (const name of declared) {
    // Keep exact variants in primary accounting. Normalize ONLY this Cargo
    // comparison: e.g. bybitinverse -> bybit, okxeu -> okx, kucoinspot -> kucoinspot.
    const enabled = enabledFeatures(venueFamily(name));
    assert(enabled.has('hosted-signing') || enabled.has('hosted-wallet'), `${name}: missing registry Hosted codec`);
  }
});

test('base-family proof cannot account for an unproved variant', () => {
  assert.throws(() => accounted(new Set(['bybit', 'bybitdemo']), new Set(['bybit']), new Map()), /bybitdemo/);
  assert.throws(() => accounted(new Set(['kucoin', 'kucoinspot']), new Set(['kucoin']), new Map()), /kucoinspot/);
  assert.throws(() => accounted(new Set(['okx', 'okxeu']), new Set(['okx']), new Map()), /okxeu/);
});

test('removing a real offered exclusion or adding an unaccounted venue fails closed', () => {
  const withoutWoo = new Map(excluded);
  withoutWoo.delete('woo');
  assert.throws(() => accounted(new Set(['woo']), ready, withoutWoo), /Unaccounted Hosted venues: woo/);
  assert.throws(() => accounted(new Set([...declared, 'future-hosted']), ready, excluded), /future-hosted/);
});

test('empty reasons, duplicate exclusions, stale exclusions and overlapping readiness refuse', () => {
  assert.throws(() => exclusions([{ venue: 'woo', reason: ' ' }]), /empty exclusion reason/);
  assert.throws(() => exclusions([{ venue: 'woo' }]), /missing exclusion reason/);
  assert.throws(() => exclusions([{ venue: 'woo', reason: 'No proof' }, { venue: 'woo', reason: 'No proof' }]), /duplicate exclusion/);
  assert.throws(() => accounted(new Set(['woo']), new Set(['woo']), new Map([['woo', 'No proof']])), /remove its exclusion/);
  assert.throws(() => accounted(new Set(['bybit']), new Set(['bybit']), new Map([['woo', 'No proof']])), /stale exclusion/);
});

test('catalog parsing rejects ambiguous, duplicate, unknown or executable entries', () => {
  const source = 'const PRIVATE_STARTUP_VENUES: &[&str] = &["bybit", // "woo" is not ready\n];';
  assert.deepEqual([...catalog(source)], ['bybit']);
  assert.throws(() => catalog(source + source), /exactly one/);
  assert.throws(() => catalog(source.replace('"bybit",', '"bybit", "bybit",')), /duplicate exact venue/);
  assert.throws(() => catalog(source.replace('"bybit"', '"Bybit"')));
  assert.throws(() => catalog(source.replace('"bybit"', 'include!("other")')));
});
