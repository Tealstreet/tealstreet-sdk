import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { readFileSync, readdirSync, lstatSync, realpathSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { relative, isAbsolute, sep } from 'node:path';
import test from 'node:test';
import { venueFamily } from '../hosts/wasm/js/venues.mjs';

const read = path => readFileSync(new URL(path, import.meta.url), 'utf8');
const rules = JSON.parse(read('../../packages/safe-cex/src/exchanges/rust/construction-rules.json')).venues;
const metadata = JSON.parse(read('../safe-cex-types/src/exchange_metadata.json')).metadata;
const manifest = fileURLToPath(new URL('../registry/Cargo.toml', import.meta.url));
const features = JSON.parse(execFileSync('python3', ['-c',
  'import json,sys,tomllib; print(json.dumps(tomllib.load(open(sys.argv[1], "rb"))["features"]))',
  manifest], { encoding: 'utf8' }));
const server = new URL('../hosts/server/', import.meta.url);
const directory = new URL('src/delegated/cosigned-proof-catalog/', server);
const catalogFiles = readdirSync(directory).map(name => {
  assert(lstatSync(new URL(name, directory)).isFile(), `${name}: expected a regular record`);
  return [name, readFileSync(new URL(name, directory), 'utf8')];
});

// Census of the Hosted helper and its remaining live-test hold. Cover ALL
// declarations below, so a blocked/disabled venue cannot disappear from review.
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

// Hide comments and string contents before checking real test declarations.
function rustSource(source) {
  const code = source.split('');
  const hide = (from, to) => {
    for (let i = from; i < to; i++) {
      code[i] = ' ';
    }
  };
  for (let i = 0; i < source.length;) {
    const start = i;
    if (source.startsWith('//', i)) {
      i = source.indexOf('\n', i);
      if (i < 0) i = source.length;
      hide(start, i);
    } else if (source.startsWith('/*', i)) {
      let depth = 1;
      i += 2;
      while (i < source.length && depth) {
        if (source.startsWith('/*', i)) { depth++; i += 2; }
        else if (source.startsWith('*/', i)) { depth--; i += 2; }
        else i++;
      }
      assert.equal(depth, 0, 'unterminated Rust comment');
      hide(start, i);
    } else {
      const raw = /^r(#{0,255})"/.exec(source.slice(i));
      if (raw) {
        const end = '"' + raw[1];
        i = source.indexOf(end, i + raw[0].length);
        assert(i >= 0, 'unterminated Rust raw string');
        i += end.length;
        hide(start, i);
      } else if (source[i] === '"') {
        i++;
        while (i < source.length && source[i] !== '"') {
          i += source[i] === '\\' ? 2 : 1;
        }
        assert(i < source.length, 'unterminated Rust string');
        hide(start, ++i);
      } else {
        const character = /^'(?:\\(?:u\{[0-9a-fA-F_]+\}|x[0-9a-fA-F]{2}|.)|[^'\\\n])'/u.exec(source.slice(i));
        if (character) { i += character[0].length; hide(start, i); }
        else i++;
      }
    }
  }
  return code.join('');
}

function catalog(files) {
  const ready = new Set(), excluded = [], records = new Map();
  for (const [file, raw] of files) {
    assert(file.endsWith('.json'), 'expected a per-venue JSON file');
    const venue = file.slice(0, -5);
    exactNames([venue]);
    assert(!records.has(venue), `${venue}: duplicate exact venue`);
    const entry = JSON.parse(raw);
    assert(entry && !Array.isArray(entry) && typeof entry === 'object', 'expected a closed record');
    const keys = [...raw.matchAll(/"(?:\\.|[^"\\])*"/g)]
      .filter(match => raw.slice(match.index + match[0].length).trimStart().startsWith(':'))
      .map(match => JSON.parse(match[0]));
    assert.equal(new Set(keys).size, keys.length, 'duplicate record field');
    if (entry.status === 'ready') {
      assert.deepEqual(Object.keys(entry).sort(), ['probeTests', 'sessionTests', 'status']);
      for (const field of ['probeTests', 'sessionTests']) {
        assert.equal(typeof entry[field], 'string', `missing ${field}`);
        assert.match(entry[field], /^src\/(?:[a-zA-Z0-9_-]+\/)*[a-zA-Z0-9_-]+\.rs$/);
      }
      ready.add(venue);
    } else {
      assert.equal(entry.status, 'excluded', 'unknown proof status');
      assert.deepEqual(Object.keys(entry).sort(), ['reason', 'status']);
      excluded.push({ venue, reason: entry.reason });
    }
    records.set(venue, entry);
  }
  return { ready, excluded: exclusions(excluded), records };
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
const { ready, excluded, records } = catalog(catalogFiles);
const helperEligible = new Set([...declared].filter(name => !awaitingLiveTest.has(name)));
// This generated snapshot is checked by rust:metadata:check. It only reconciles
// the production cohort; the stronger all-declarations guard does not filter on it.
const productionOffered = new Set([...helperEligible].filter(name => {
  assert.equal(typeof metadata[name]?.enabled, 'boolean', `${name}: missing production metadata`);
  return metadata[name].enabled;
}));

test('every declared remoteSigning venue has an exact proof or a reason', () => {
  accounted(declared, ready, excluded);
});

test('production cohort reproduces the 41 / 40 / 33 census without hiding disabled venues', () => {
  assert.equal(declared.size, 41);
  // NX #9737 removed the origin gate after SDK #3567/#3570 supplied origins.
  // Retain all seven disabled metadata entries in the declared accounting.
  assert.equal(helperEligible.size, 40);
  assert.equal(productionOffered.size, 33);
  for (const venue of ['hyrotrader', 'binanceoptions']) {
    assert(helperEligible.has(venue), `${venue}: live origin must remain in review`);
    assert(!productionOffered.has(venue), `${venue}: disabled metadata must remain disabled`);
  }
  assert.deepEqual([...helperEligible].filter(name => !productionOffered.has(name)).sort(),
    ['binanceoptions', 'bingxspot', 'bitgetinverse', 'bitrue', 'hyrotrader', 'lighter-robinhood', 'txflow']);
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
  const [unproved] = excluded.keys();
  assert(unproved, 'control requires a real excluded venue');
  const withoutProof = new Map(excluded);
  withoutProof.delete(unproved);
  assert.throws(() => accounted(declared, ready, withoutProof), new RegExp(`Unaccounted Hosted venues: ${unproved}`));
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
  const source = JSON.stringify({ status: 'ready', probeTests: 'src/delegated.rs', sessionTests: 'src/session.rs' });
  assert.deepEqual([...catalog([['bybit.json', source]]).ready], ['bybit']);
  assert.throws(() => catalog([['bybit.json', source], ['bybit.json', source]]), /duplicate exact venue/);
  assert.throws(() => catalog([['Bybit.json', source]]));
  assert.throws(() => catalog([['bybit.json', 'include!("other")']]));
  assert.throws(() => catalog([['bybit.json', '{"status":"ready","status":"excluded","reason":"ambiguous"}']]), /duplicate record field/);
  assert.throws(() => catalog([['bybit.json', JSON.stringify({ status: 'ready', reason: 'still excluded' })]]));
  assert.throws(() => catalog([['bybit.json', JSON.stringify({ status: 'unknown' })]]));
  assert.throws(() => catalog([['bybit.json', source.replace('src/delegated.rs', '../outside.rs')]]));
});

test('commented catalog cannot shadow a live alias or a second live definition', () => {
  const literal = JSON.stringify({ status: 'excluded', reason: 'no proof' });
  for (const commented of [`// ${literal}\n`, `/* ${literal} */`, `/* outer /* ${literal} */ end */`]) {
    assert.throws(() => catalog([['bybit.json', `${commented}${literal}`]]));
  }
  assert.throws(() => catalog([['bybit.json', `${literal}\n${literal}`]]));
  assert.throws(() => catalog([['bybit.json', `const ACTIVE = ${literal};`]]));
});

test('each declared venue has its own independent catalog record', () => {
  const names = readdirSync(new URL('../hosts/server/src/delegated/cosigned-proof-catalog/', import.meta.url));
  assert.deepEqual(names.sort(), [...declared].map(name => `${name}.json`).sort());
});


test('ready records name existing Rust proof suites within the server crate', () => {
  const root = realpathSync(server);
  for (const [venue, entry] of records) {
    if (entry.status !== 'ready') continue;
    for (const field of ['probeTests', 'sessionTests']) {
      const file = new URL(entry[field], server);
      const within = relative(root, realpathSync(file));
      assert(within && !isAbsolute(within) && !within.split(sep).includes('..'), `${venue}: escaped proof path`);
      assert(lstatSync(file).isFile(), `${venue}: missing proof suite`);
      assert.match(rustSource(readFileSync(file, 'utf8')), /#\[\s*(?:tokio::)?test\b/, `${venue}: no test declarations`);
    }
  }
});

test('Bybit standalone probe names its native private-read possession-proof suite', () => {
  assert.equal(records.get('bybit').probeTests, 'src/session/tests/cosigned_negatives.rs');
});

test('independent venue promotions compose without editing a shared catalog', () => {
  const promoted = [...excluded.keys()].sort().slice(0, 2);
  assert.equal(promoted.length, 2);
  const changed = catalogFiles.map(([file, text]) => [file, promoted.some(name => file === `${name}.json`)
    ? JSON.stringify({ status: 'ready', probeTests: 'src/delegated.rs', sessionTests: 'src/session/tests/cosigned_venues.rs' }) : text]);
  const combined = catalog(changed);
  accounted(declared, combined.ready, combined.excluded);
  assert.equal(combined.ready.size, ready.size + 2);
  for (const name of promoted) assert(combined.ready.has(name));
  assert.equal(combined.excluded.size, excluded.size - 2);
});
