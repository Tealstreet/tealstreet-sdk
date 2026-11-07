import assert from 'node:assert/strict';
import test from 'node:test';
import { readFileSync, readdirSync } from 'node:fs';
import { selectCi, checkoutBase, ciOutputs, rustDocumentInputs, externallyImportedVenues } from './ci-selection.mjs';
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
  // The frozen TS replay oracle selects its venue exactly like the runtime helpers.
  assert.deepEqual(select('packages/safe-cex/src/__tests__/reference/kraken/kraken.exchange.ts').families, ['kraken']);
  assert.deepEqual(select('packages/safe-cex/src/__tests__/reference/kucoin/kucoinspot.exchange.ts').nativeCrates, ['kucoin']);
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

test('hosted-operation inputs and full landing coverage include the Kraken proof family', () => {
  for (const path of ['sdk-rs/hosts/wasm/js/hosted-operations.mjs',
    'sdk-rs/hosts/wasm/js/kraken-operation-codec.mjs', 'sdk-rs/hosts/wasm/src/lib.rs',
    'sdk-rs/venues/kraken/src/sign/hosted.rs']) {
    const result = select(path);
    assert.equal(result.sizeRequired, true, path);
    assert(result.families.includes('kraken'), path);
  }
  const landing = selectCi({ paths: [], manifests, full: true });
  assert.equal(landing.sizeRequired, true);
  assert(landing.families.includes('kraken'));
});

test('venue feature declarations retain complete engine combinations and wasm checks', () => {
  const result = select('sdk-rs/venues/bitget/Cargo.toml');
  assert.equal(result.nativeFull, true);
  // The native job keeps the complete legacy workspace/feature-mode matrix.
  assert.equal(result.sizeRequired, true);
  assert(result.families.includes('bitget'));
});

test('cross-venue and non-venue TS imports conservatively select the full matrix', () => {
  const sources = [
    ['packages/safe-cex/src/__tests__/reference/binance/binanceoptions.exchange.ts', "import { options } from '../bybit/bybit.options'"],
    ['packages/safe-cex/src/transport/engine.ts', "import { site } from '../exchanges/bybit/bybit.site'"],
    ['packages/safe-cex/src/exchanges/bitget/ticker.ts', "import { row } from './wire'"],
  ];
  const externalImports = externallyImportedVenues(sources);
  assert.equal(selectCi({ paths: ['packages/safe-cex/src/exchanges/bybit/bybit.site.ts'], manifests, externalImports }).full, true);
  assert.equal(selectCi({ paths: ['packages/safe-cex/src/exchanges/bitget/ticker.ts'], manifests, externalImports }).full, false);
  const oracle = externallyImportedVenues([
    ['packages/safe-cex/src/__tests__/reference/binance/binance.exchange.ts', "import { x } from '../okx/okx.wire'"],
    ['packages/safe-cex/src/__tests__/reference/okx/okx.exchange.ts', "import { y } from './okx.wire'"],
  ]);
  assert.deepEqual([...oracle], ['okx']);
  assert.equal(selectCi({ paths: ['packages/safe-cex/src/__tests__/reference/okx/okx.wire.ts'], manifests, externalImports: oracle }).full, true);
});

test('edited integration controls execute, and engine support shared with wire examples builds consumers', () => {
  for (const path of ['sdk-rs/harness/bitget-parity.integration.mjs', 'sdk-rs/harness/krakenspot-frame.integration.mts']) {
    const result = select(path);
    assert.equal(result.combinedRequired, true);
    assert.deepEqual(result.integrationTests, [path]);
    assert.equal(result.replayRequired, false);
    assert.equal(result.sizeRequired, false);
  }
  assert.equal(select('sdk-rs/engine/tests/support/mod.rs').combinedRequired, true);
  const familyChecker = select('sdk-rs/harness/families.integration.mjs');
  assert.deepEqual(familyChecker.families, venueFamilies);
  assert.deepEqual(familyChecker.integrationTests, []);
  assert.equal(familyChecker.full, true);
});

test('documentation included by Rust, and unresolved include macros, select full checks', () => {
  const sources = [['sdk-rs/engine/src/lib.rs', '#![doc = include_str!("../README.md")]\nconst DATA: &[u8] = include_bytes!("../rules.adoc");']];
  const documents = rustDocumentInputs(sources);
  for (const path of ['sdk-rs/engine/README.md', 'sdk-rs/engine/rules.adoc'])
    assert.equal(selectCi({ paths: [path], manifests, documents }).full, true);
  assert.equal(selectCi({ paths: ['README.md'], manifests, documents }).full, false);
  for (const text of ['include_str!(concat!("../", "README.md"))', 'include_bytes!(r"../README.md")'])
    assert.equal(selectCi({ paths: ['README.md'], manifests, documents: rustDocumentInputs([['src/lib.rs', text]]) }).full, true);
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
  const event = { pull_request: { head: { sha: b }, base: { sha: c, ref: 'master', repo: { full_name: 'Tealstreet/tealstreet-sdk' } } } };
  const value = { event, checkout: c, parents: [a, b], tree: t, mergeTree: t };
  assert.equal(checkoutBase(value), a);
  for (const change of [{ parents: [a] }, { parents: [a, c] }, { parents: [a, b, c] }, { mergeTree: a },
    { event: { pull_request: { ...event.pull_request, base: { ...event.pull_request.base, ref: 'other' } } } }])
    assert.throws(() => checkoutBase({ ...value, ...change }));
});
