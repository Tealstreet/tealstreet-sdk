import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import test from 'node:test';
import { venueFamily } from '../hosts/wasm/js/venues.mjs';

const rules = JSON.parse(readFileSync(new URL('../../packages/safe-cex/src/exchanges/rust/construction-rules.json', import.meta.url)));
const manifest = fileURLToPath(new URL('../hosts/wasm/Cargo.toml', import.meta.url));
const features = JSON.parse(execFileSync('python3', ['-c',
  'import json,sys,tomllib; print(json.dumps(tomllib.load(open(sys.argv[1], "rb"))["features"]))', manifest], { encoding: 'utf8' }));

const setupFeatures = {
  apexomni: ['dispatch-signing'],
  mexc: ['dispatch-signing'],
  lighter: ['dispatch-signing'],
  hyperliquid: ['dispatch-signing', 'address-nonce'],
  paradex: ['dispatch-signing'],
  // Hosted V1 signing admits AsterDEX without a remoteSigning declaration.
  asterdex: ['dispatch-signing', 'address-nonce', 'sign-meta', 'hosted-signing'],
};

function requiredFeatures(rule) {
  const required = new Set(rule.bridge?.dispatchSigning ? ['dispatch-signing'] : []);
  // Every family that admits remote signing keeps the hosted-signing captures.
  if (rule.remoteSigning?.enabled) required.add('hosted-signing');
  if (rule.setupMapping) {
    assert(Object.hasOwn(setupFeatures, rule.setupMapping), `Unspecified setup mapping: ${rule.setupMapping}`);
    for (const feature of setupFeatures[rule.setupMapping]) required.add(feature);
  }
  return required;
}

function enabledFeatures(graph, root, enabled = new Set()) {
  assert(Object.hasOwn(graph, root), `Missing host feature: ${root}`);
  if (enabled.has(root)) return enabled;
  enabled.add(root);
  for (const feature of graph[root]) {
    if (Object.hasOwn(graph, feature)) enabledFeatures(graph, feature, enabled);
  }
  return enabled;
}

function checkFeatures(graph, family, rule) {
  const enabled = enabledFeatures(graph, family);
  for (const feature of requiredFeatures(rule)) {
    assert(enabled.has(feature), `${family}: construction requires host feature ${feature}`);
  }
}

for (const [venue, rule] of Object.entries(rules.venues)) {
  test(`${venue} construction requirements are enabled in its isolated WASM family`, () => {
    checkFeatures(features, venueFamily(venue), rule);
  });
}

test('a family that admits remote signing cannot drop hosted signing', () => {
  const remote = Object.entries(rules.venues).filter(([, rule]) => rule.remoteSigning?.enabled).map(([venue]) => venue);
  assert.ok(remote.length > 0);
  for (const venue of remote) assert.ok(enabledFeatures(features, venueFamily(venue)).has('hosted-signing'), venue);
  assert.throws(() => checkFeatures({ family: ['dispatch-signing'], 'dispatch-signing': [] }, 'family',
    { bridge: { dispatchSigning: true }, remoteSigning: { enabled: true } }), /hosted-signing/);
});

test('feature guard rejects missing bootstrap, address setup and signing metadata independently', () => {
  for (const required of ['dispatch-signing', 'address-nonce', 'sign-meta']) {
    const graph = { wallet: ['dispatch-signing', 'address-nonce', 'sign-meta'].filter(feature => feature !== required),
      'dispatch-signing': [], 'address-nonce': [], 'sign-meta': [] };
    assert.throws(() => checkFeatures(graph, 'wallet', { setupMapping: 'asterdex' }), new RegExp(required));
  }
  assert.throws(() => checkFeatures({ family: [] }, 'family', { bridge: { dispatchSigning: true } }), /dispatch-signing/);
  assert.throws(() => checkFeatures({ family: [] }, 'family', { setupMapping: 'lighter' }), /dispatch-signing/);
  assert.throws(() => requiredFeatures({ setupMapping: 'new-mapping' }), /Unspecified setup mapping/);
});

test('Hyperliquid ScalarOnly setup requires isolated dispatch signing, address setup and hosted signing', () => {
  const required = ['dispatch-signing', 'address-nonce', 'hosted-signing'];
  for (const missing of required) {
    const graph = { hyperliquid: required.filter(feature => feature !== missing),
      'dispatch-signing': [], 'address-nonce': [], 'hosted-signing': [] };
    assert.throws(() => checkFeatures(graph, 'hyperliquid', rules.venues.hyperliquid), new RegExp(missing));
    assert.throws(() => checkFeatures(graph, 'hyperliquid', rules.venues.hyperliquidspot), new RegExp(missing));
  }
});

test('feature guard follows transitive local features, without borrowing combined defaults', () => {
  const graph = { wallet: ['bootstrap', 'address-nonce'], bootstrap: ['dispatch-signing', 'hosted-signing'],
    'address-nonce': ['sign-meta'], default: ['unrelated'], unrelated: ['dispatch-signing'],
    'dispatch-signing': [], 'hosted-signing': [], 'sign-meta': [], bare: [] };
  checkFeatures(graph, 'wallet', { setupMapping: 'asterdex' });
  assert.throws(() => checkFeatures(graph, 'bare', { bridge: { dispatchSigning: true } }), /dispatch-signing/);
});
