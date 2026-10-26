import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { familyLoader } from './family-loader.mjs';
import { venueFamilies, venueFamily } from './venues.mjs';

const fakeFamilies = () => {
  const imports = [];
  const inits = [];
  const importFamily = (family) => {
    imports.push(family);
    return { family, default: () => new Promise((resolve, reject) => inits.push({ family, resolve, reject })) };
  };
  return { imports, inits, load: familyLoader(importFamily) };
};

test('aliases and concurrent accounts share one initialization per family', async () => {
  const { imports, inits, load } = fakeFamilies();
  const loads = [load('bybit'), load('hyrotrader'), load('bybitspot'), load('okx')];
  await new Promise(setImmediate);
  assert.deepEqual(imports, ['bybit', 'okx']);
  for (const init of inits) init.resolve();
  const engines = await Promise.all(loads);
  assert.deepEqual(engines.map((engine) => engine.family), ['bybit', 'bybit', 'bybit', 'okx']);
  assert.equal(await load('bybitdemo'), engines[0]);
  assert.deepEqual(imports, ['bybit', 'okx']);
});

test('a failed initialization rejects its waiters and lets the next load retry', async () => {
  const { imports, inits, load } = fakeFamilies();
  const failure = new Error('compile failed');
  const first = [load('paradex'), load('paradex')];
  await new Promise(setImmediate);
  inits[0].reject(failure);
  for (const result of await Promise.allSettled(first)) assert.deepEqual(result, { status: 'rejected', reason: failure });
  const retry = load('paradex');
  await new Promise(setImmediate);
  inits[1].resolve();
  assert.equal((await retry).family, 'paradex');
  assert.deepEqual(imports, ['paradex', 'paradex']);
});

test('unknown venues reject before importing any engine', async () => {
  const { imports, load } = fakeFamilies();
  await assert.rejects(load('bitmex'), /unsupported Rust venue: bitmex/);
  assert.deepEqual(imports, []);
});

test('every family is a Cargo feature, a web import and a host venue name', () => {
  const cargo = readFileSync(new URL('../Cargo.toml', import.meta.url), 'utf8');
  const features = cargo.slice(cargo.indexOf('[features]'), cargo.indexOf('[dependencies]'));
  const web = readFileSync(new URL('./engine.web.mjs', import.meta.url), 'utf8');
  const host = readFileSync(new URL('../../../registry/src/lib.rs', import.meta.url), 'utf8');
  for (const family of venueFamilies) {
    assert.match(features, new RegExp(`^${family} = \\[`, 'm'), `${family} feature`);
    assert.ok(web.includes(`case '${family}': return import('../pkg/web/${family}/safe_cex_wasm.js');`), `${family} import`);
  }
  // Every venue the host constructs maps to a family, and every mapped venue is constructed.
  const constructed = [...host.matchAll(/^\s+((?:"[a-z-]+"\s*\|\s*)*"[a-z-]+")\s*=>/gm)]
    .flatMap((match) => match[1].match(/"[a-z-]+"/g).map((name) => name.slice(1, -1)));
  for (const venue of constructed) assert.doesNotThrow(() => venueFamily(venue), venue);
});
