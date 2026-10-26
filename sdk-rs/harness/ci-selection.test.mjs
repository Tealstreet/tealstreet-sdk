import assert from 'node:assert/strict';
import test from 'node:test';
import { readFileSync, readdirSync } from 'node:fs';
import { selectCi, checkoutBase, ciOutputs } from './ci-selection.mjs';
import { GATE_SUITES } from './ci-gate-suites.mjs';
import { venueFamilies } from '../hosts/wasm/js/venues.mjs';

const manifests = readdirSync(new URL('../venues/', import.meta.url)).map(crate =>
  [crate, readFileSync(new URL(`../venues/${crate}/Cargo.toml`, import.meta.url), 'utf8')]);
const select = (...paths) => selectCi({ paths, manifests });

test('each leaf selects its whole family, including aliases and dependent crates', () => {
  for (const [crate] of manifests) {
    const result = select(`sdk-rs/venues/${crate}/src/lib.rs`);
    assert.equal(result.full, false, crate);
    assert(result.replay.length > 0, crate);
    assert(result.nativeCrates.includes(crate), crate);
  }
  assert.deepEqual(select('sdk-rs/venues/binance/src/sign.rs').families, ['asterdex', 'binance']);
  assert.deepEqual(select('sdk-rs/venues/hyperliquid/src/lib.rs').families, ['hyperliquid', 'txflow']);
  assert.deepEqual(select('sdk-rs/venues/bitget/src/lib.rs').replay.map(x => x.venue), ['bitget', 'bitgetusdc']);
  assert.deepEqual(select('sdk-rs/venues/kucoin/src/lib.rs').families, ['kucoin', 'kucoinspot']);
  assert.deepEqual(select('packages/safe-cex/src/exchanges/kraken/message.ts').families, ['kraken']);
  for (const [venue, crate] of [['bitgetusdc', 'bitget'], ['bybitinverse', 'bybit'], ['kucoinspot', 'kucoin'], ['krakenspot', 'kraken']]) {
    const result = select(`packages/safe-cex/src/exchanges/${venue}/message.ts`);
    assert.deepEqual(result.nativeCrates, [crate]);
    assert(result.families.includes(venue === 'kucoinspot' ? venue : crate));
  }
});

test('checkout outputs are literal SHAs while matrices remain JSON', () => {
  const sha = 'a'.repeat(40);
  assert.equal(ciOutputs({ checkoutSha: sha, full: false, native: [{ check: 'fmt' }] }),
    `checkoutSha=${sha}\nfull=false\nnative=[{"check":"fmt"}]\n`);
});

test('docs and inert tests skip replay/size, but unknown and shared inputs select everything', () => {
  for (const path of ['docs/plans/X.md', 'sdk-rs/venues/bitget/tests/ticker.rs',
    'sdk-rs/engine/tests/http.rs', 'sdk-rs/harness/gate.test.mjs', '.github/tests/x.test.cjs']) {
    const result = select(path);
    assert.equal(result.replayRequired, false, path); assert.equal(result.sizeRequired, false, path);
  }
  for (const path of ['sdk-rs/engine/src/engine.rs', 'sdk-rs/crypto/src/lib.rs',
    'sdk-rs/registry/src/lib.rs', 'sdk-rs/hosts/wasm/src/lib.rs', 'sdk-rs/Cargo.lock',
    'sdk-rs/venues/unknown/src/lib.rs', 'sdk-rs/venues/bitget/Cargo.toml',
    'yarn.lock', '.github/workflows/rust.yml', 'sdk-rs/harness/gate.mjs',
    'sdk-rs/engine/src/not_really_tests.rs']) {
    const result = select(path);
    if (path === 'sdk-rs/venues/bitget/Cargo.toml') assert.equal(result.families.includes('bitget'), true);
    else { assert.deepEqual(result.families, venueFamilies, path); assert.equal(result.replay.length, GATE_SUITES.length, path); }
  }
  assert.equal(selectCi({ paths: ['docs/X.md'], manifests, full: true }).replay.length, GATE_SUITES.length);
  for (const path of ['sdk-rs/hosts/wasm/js/workers.test.mjs', 'sdk-rs/harness/frame-wire.integration.mjs']) {
    const result = select(path);
    assert.equal(result.combinedRequired, true, path);
    assert.equal(result.sizeRequired, false, path);
    assert.equal(result.replayRequired, false, path);
  }
});

test('base dependency edges and transitive users survive removed dependencies', () => {
  const result = selectCi({ paths: ['sdk-rs/venues/binance/src/lib.rs'], manifests: [
    ['asterdex', 'safe-cex-venue-binance.workspace = true'],
    ['asterdex', ''], ['txflow', 'safe-cex-venue-asterdex.workspace = true'],
  ] });
  assert.deepEqual(result.families, ['asterdex', 'binance', 'txflow']);
  assert.equal(selectCi({ paths: [], manifests: [['bitget', 'safe-cex-venue-unknown.workspace = true']] }).full, true);
});

test('selection uses actual checkout parents, never a stale event base SHA', () => {
  const a = 'a'.repeat(40), b = 'b'.repeat(40), c = 'c'.repeat(40), t = 'd'.repeat(40);
  const event = { pull_request: { head: { sha: b }, base: { sha: c, ref: 'feat/rust-rewrite', repo: { full_name: 'Tealstreet/tealstreet-sdk' } } } };
  const value = { event, checkout: c, parents: [a, b], tree: t, mergeTree: t };
  assert.equal(checkoutBase(value), a);
  for (const change of [{ parents: [a] }, { parents: [a, c] }, { parents: [a, b, c] }, { mergeTree: a },
    { event: { pull_request: { ...event.pull_request, base: { ...event.pull_request.base, ref: 'other' } } } }])
    assert.throws(() => checkoutBase({ ...value, ...change }));
});
