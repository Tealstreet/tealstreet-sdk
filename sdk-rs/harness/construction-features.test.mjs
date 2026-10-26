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
  asterdex: ['dispatch-signing', 'address-nonce', 'sign-meta'],
};

function requiredFeatures(rule) {
  const required = new Set(rule.bridge?.dispatchSigning ? ['dispatch-signing'] : []);
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

test('feature guard follows transitive local features, without borrowing combined defaults', () => {
  const graph = { wallet: ['bootstrap', 'address-nonce'], bootstrap: ['dispatch-signing'],
    'address-nonce': ['sign-meta'], default: ['unrelated'], unrelated: ['dispatch-signing'],
    'dispatch-signing': [], 'sign-meta': [], bare: [] };
  checkFeatures(graph, 'wallet', { setupMapping: 'asterdex' });
  assert.throws(() => checkFeatures(graph, 'bare', { bridge: { dispatchSigning: true } }), /dispatch-signing/);
});
