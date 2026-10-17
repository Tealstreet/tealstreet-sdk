import assert from 'node:assert/strict';
import test, { describe } from 'node:test';
import path from 'node:path';
import { readInputs, validateLayers } from './check-layers.mjs';

// Use live Cargo declarations; mutations retain every dependency kind and target.
const original = readInputs();
function addEdge(metadata, from, to, extra = {}) {
  const member = metadata.packages.find(pkg => pkg.name === to);
  metadata.packages.find(pkg => pkg.name === from).dependencies.push({ name: to, kind: null, optional: false, target: null, ...(member?.manifest_path ? { path: path.dirname(member.manifest_path) } : {}), ...extra });
}

function addSyntheticVenue(policy, metadata) {
  const name = 'safe-cex-venue-unbound-control';
  assert.ok(!metadata.packages.some(pkg => pkg.name === name));
  metadata.packages.push({ id: name, name, manifest_path: `/synthetic/${name}/Cargo.toml`, dependencies: [] });
  metadata.workspace_members.push(name);
  policy.layers.find(layer => layer.name === 'venues').packages.push(name);
  return name;
}

// Run every mutation against live declarations and a second graph with all
// venues populated. Empty scaffolds must not conceal competing failure paths.
for (const populated of [false, true]) describe(populated ? 'populated venues' : 'live workspace', () => {
const inputs = () => {
  const fixture = structuredClone(original);
  if (populated) {
    const targets = ['safe-cex-types', 'safe-cex-num', 'safe-cex-store', 'safe-cex-transport', 'safe-cex-crypto', 'safe-cex-candle-cache', 'safe-cex-exchange'];
    for (const venue of fixture.policy.layers.find(layer => layer.name === 'venues').packages) {
      for (const target of targets) addEdge(fixture.metadata, venue, target);
    }
  }
  return fixture;
};

test('every actual Cargo workspace crate obeys its declared layer', () => {
  const { policy, metadata } = inputs();
  assert.equal(validateLayers(policy, metadata), metadata.workspace_members.length);
});
test('Lighter protocol SDK stays below venue adapters and above crypto services', () => {
  const { policy, metadata } = inputs();
  assert.ok(policy.layers.find(layer => layer.name === 'exchange-core').packages.includes('safe-cex-lighter-sdk'));
  addEdge(metadata, 'safe-cex-venue-lighter', 'safe-cex-lighter-sdk');
  assert.equal(validateLayers(policy, metadata), metadata.workspace_members.length);
  addEdge(metadata, 'safe-cex-lighter-sdk', 'safe-cex-venue-lighter');
  assert.throws(() => validateLayers(policy, metadata), /(?:dependency cycle|upward dependency).*safe-cex-lighter-sdk -> safe-cex-venue-lighter/);
});
test('removing the protocol SDK classification fails closed', () => {
  const { policy, metadata } = inputs();
  policy.layers.find(layer => layer.name === 'exchange-core').packages = ['safe-cex-exchange'];
  assert.throws(() => validateLayers(policy, metadata), /unclassified dependency: safe-cex-venue-lighter -> safe-cex-lighter-sdk/);
});
for (const [name, from, to, reason] of [
  ['lateral edge', 'safe-cex-store', 'safe-cex-transport', 'lateral dependency'],
  ['skip edge', 'safe-cex-ffi', 'safe-cex-types', 'skip dependency'],
  ['upward edge', 'safe-cex-types', 'safe-cex-ffi', 'dependency cycle|upward dependency'],
  ['runtime-to-tooling edge', 'safe-cex-transport', 'safe-cex-parity', 'dependency cycle|non-runtime dependency'],
]) test(`${name} is rejected and names the edge`, () => {
  const { policy, metadata } = inputs();
  addEdge(metadata, from, to);
  assert.throws(() => validateLayers(policy, metadata), new RegExp(`(?:${reason}).*${from} -> ${to}`));
});
test('unclassified workspace crate is rejected and names its incoming edge', () => {
  const { policy, metadata } = inputs();
  metadata.packages.push({ id: 'new-member', name: 'safe-cex-new', dependencies: [] });
  metadata.workspace_members.push('new-member');
  addEdge(metadata, 'safe-cex', 'safe-cex-new');
  assert.throws(() => validateLayers(policy, metadata), /unclassified.*safe-cex -> safe-cex-new/);
});
test('unclassified isolated crate is also rejected', () => {
  const { policy, metadata } = inputs();
  metadata.packages.push({ id: 'new-member', name: 'safe-cex-new', dependencies: [] });
  metadata.workspace_members.push('new-member');
  assert.throws(() => validateLayers(policy, metadata), /unclassified crate: workspace -> safe-cex-new/);
});
for (const extra of [
  { optional: true },
  { kind: 'dev' },
  { kind: 'build' },
  { target: 'cfg(target_arch = "wasm32")' },
  { rename: 'innocent_name' },
]) test(`private derivations cannot leak through ${JSON.stringify(extra)}`, () => {
  const { policy, metadata } = inputs();
  addEdge(metadata, 'safe-cex', 'safe-cex-venue-data', extra);
  assert.throws(() => validateLayers(policy, metadata), /private derivations forbidden: safe-cex -> safe-cex-venue-data/);
});
test('transitive private derivations are rejected with the full path', () => {
  const { policy, metadata } = inputs();
  addEdge(metadata, 'safe-cex-crypto', 'venue-data', { optional: true, kind: 'build', rename: 'derivations' });
  assert.throws(() => validateLayers(policy, metadata), /private derivations forbidden: safe-cex -> safe-cex-crypto -> venue-data/);
});
test('unclassified path dependency cannot escape through workspace exclusion', () => {
  const { policy, metadata } = inputs();
  addEdge(metadata, 'safe-cex', 'outside-workspace', { path: '/excluded/private-crate' });
  assert.throws(() => validateLayers(policy, metadata), /unclassified dependency: safe-cex -> outside-workspace/);
});
test('an obsolete exception is rejected', () => {
  const { policy, metadata } = inputs();
  policy.exceptions.push({ from: 'safe-cex-ffi', to: 'safe-cex-types', reason: 'not an existing edge' });
  assert.throws(() => validateLayers(policy, metadata), /stale exception: safe-cex-ffi -> safe-cex-types/);
});
test('an exception cannot grant lateral access', () => {
  const { policy, metadata } = inputs();
  addEdge(metadata, 'safe-cex-store', 'safe-cex-transport');
  policy.exceptions.push({ from: 'safe-cex-store', to: 'safe-cex-transport', reason: 'invalid exception' });
  assert.throws(() => validateLayers(policy, metadata), /only existing downward skips can be excepted: safe-cex-store -> safe-cex-transport/);
});
test('private package classification cannot be weakened', () => {
  const { policy, metadata } = inputs();
  policy.privateDerivations = policy.privateDerivations.filter(name => name !== 'safe-cex-venue-data');
  assert.throws(() => validateLayers(policy, metadata), /safe-cex-venue-data must remain private derivations/);
});

test('a workspace name cannot disguise a private derivation path', () => {
  const { policy, metadata } = inputs();
  const dep = metadata.packages.find(pkg => pkg.name === 'safe-cex').dependencies.find(dep => dep.name === 'safe-cex-types');
  dep.path = '/repo/packages/venue-data';
  assert.throws(() => validateLayers(policy, metadata), /private derivation path forbidden: safe-cex -> safe-cex-types/);
});
test('a workspace name cannot disguise a different path', () => {
  const { policy, metadata } = inputs();
  const dep = metadata.packages.find(pkg => pkg.name === 'safe-cex').dependencies.find(dep => dep.name === 'safe-cex-types');
  dep.path = '/repo/private-other-name';
  assert.throws(() => validateLayers(policy, metadata), /workspace package identity mismatch: safe-cex -> safe-cex-types/);
});
test('a registry package cannot impersonate a workspace member', () => {
  const { policy, metadata } = inputs();
  const dep = metadata.packages.find(pkg => pkg.name === 'safe-cex').dependencies.find(dep => dep.name === 'safe-cex-types');
  delete dep.path;
  dep.source = 'registry+https://example.invalid/index';
  assert.throws(() => validateLayers(policy, metadata), /workspace package identity mismatch: safe-cex -> safe-cex-types/);
});

// BaseExchange belongs below venues; placing it in the facade made every venue edge upward.
const grantedTargets = ['safe-cex-types','safe-cex-num','safe-cex-store','safe-cex-transport','safe-cex-crypto','safe-cex-candle-cache'].sort();
function pinGrantDecision(policy) {
  assert.deepEqual(policy.layerGrants.map(grant => [grant.fromLayer, grant.to]).sort(),
    grantedTargets.map(target => ['venues', target]).sort(), 'seventh target requires explicit architecture decision');
}
test('layer grant pins exactly the six named targets', () => {
  pinGrantDecision(original.policy);
  const { policy } = inputs();
  policy.layerGrants.push({fromLayer:'venues',to:'safe-cex-new'});
  assert.throws(() => pinGrantDecision(policy), /seventh target/);
});
test('each venue may use each granted target and removal fails closed', () => {
  const venues = original.policy.layers.find(layer => layer.name === 'venues').packages;
  // Populate the fixture even while scaffolds are empty: removing a shared grant
  // invalidates every existing venue edge to that target, not just our injection.
  const populated = inputs();
  for (const venue of venues) for (const target of grantedTargets) {
    addEdge(populated.metadata, venue, target);
  }
  assert.equal(validateLayers(populated.policy, populated.metadata), populated.metadata.workspace_members.length);
  for (const venue of venues) for (const target of grantedTargets) {
    const { policy, metadata } = structuredClone(populated);
    // Isolate the selected target edge so another venue cannot mask its named
    // refusal. The unmodified live graph is checked independently above.
    for (const pkg of metadata.packages.filter(pkg => venues.includes(pkg.name))) {
      pkg.dependencies = pkg.dependencies.filter(dep => dep.name !== target);
    }
    addEdge(metadata, venue, target);
    assert.equal(validateLayers(policy, metadata), metadata.workspace_members.length);
    policy.layerGrants = policy.layerGrants.filter(grant => grant.to !== target);
    assert.throws(() => validateLayers(policy, metadata), new RegExp(`skip dependency: ${venue} -> ${target}`));
  }
});
test('removing all venue dependencies leaves grants valid without stale pair exceptions', () => {
  const { policy, metadata } = inputs();
  for (const venue of policy.layers.find(layer => layer.name === 'venues').packages) {
    metadata.packages.find(pkg => pkg.name === venue).dependencies = [];
  }
  assert.equal(validateLayers(policy, metadata), metadata.workspace_members.length);
});
for (const [fromLayer, to, diagnostic] of [
  ['services','safe-cex-types','restricted to venues'],
  ['venues','*','named downward runtime skip'],
  ['venues','safe-cex-venue-bybit','named downward runtime skip'],
  ['venues','safe-cex','named downward runtime skip'],
  ['venues','safe-cex-parity','named downward runtime skip'],
  ['venues','venue-data','named downward runtime skip'],
]) test(`invalid layer grant ${fromLayer} -> ${to} is rejected`, () => {
  const { policy, metadata } = inputs();
  policy.layerGrants.push({fromLayer,to,reason:'Mutation'});
  assert.throws(() => validateLayers(policy, metadata), new RegExp(diagnostic));
});
test('duplicate and empty-reason grants fail', () => {
  const { policy, metadata } = inputs();
  policy.layerGrants.push({...policy.layerGrants[0]});
  assert.throws(() => validateLayers(policy, metadata), /duplicate layer grant/);
  policy.layerGrants.pop(); policy.layerGrants[0].reason='';
  assert.throws(() => validateLayers(policy, metadata), /layer grant requires reason/);
});
test('venue grants do not authorize another layer to use a target', () => {
  const { policy, metadata } = inputs();
  addEdge(metadata, 'safe-cex-ffi', 'safe-cex-candle-cache');
  assert.throws(() => validateLayers(policy, metadata), /skip dependency: safe-cex-ffi -> safe-cex-candle-cache/);
});

for (const [from, to, reason] of [
  ['safe-cex-venue-bybit', 'safe-cex', 'upward'],
  ['safe-cex-exchange', 'safe-cex-venue-bybit', 'upward'],
  ['safe-cex', 'safe-cex-venue-bybit', 'skip'],
]) test(`architecture rejects ${from} -> ${to}`, () => {
  const { policy, metadata } = inputs();
  addEdge(metadata, from, to);
  assert.throws(() => validateLayers(policy, metadata), new RegExp(`(?:${reason} dependency: |dependency cycle: .*?)${from} -> ${to}`));
});
test('registry can construct a listed venue and facade can depend on registry without exceptions', () => {
  const { policy, metadata } = inputs();
  const venue = addSyntheticVenue(policy, metadata);
  policy.registryConstructors.venues.push(venue);
  addEdge(metadata, 'safe-cex-registry', venue);
  addEdge(metadata, 'safe-cex', 'safe-cex-registry');
  assert.equal(validateLayers(policy, metadata), metadata.workspace_members.length);
});
test('moving registry back beside venues rejects its constructor dependency', () => {
  const { policy, metadata } = inputs();
  const registry = policy.layers.find(layer => layer.name === 'registry');
  policy.layers = policy.layers.filter(layer => layer !== registry);
  // Core becomes adjacent after moving registry down; remove that now-stale skip.
  policy.exceptions = policy.exceptions.filter(edge => !(edge.from === 'safe-cex-registry' && edge.to === 'safe-cex-exchange'));
  policy.layers.find(layer => layer.name === 'venues').packages.push('safe-cex-registry');
  // Its first real constructor-table edge is the one rejected.
  assert.throws(() => validateLayers(policy, metadata), /lateral dependency: safe-cex-registry -> safe-cex-venue-apexomni$/);
});
test('registry constructs only the listed venues', () => {
  const { policy, metadata } = inputs();
  assert.deepEqual(policy.registryConstructors.venues, ['safe-cex-venue-apexomni', 'safe-cex-venue-asterdex', 'safe-cex-venue-backpack', 'safe-cex-venue-binance', 'safe-cex-venue-bingx', 'safe-cex-venue-bitget', 'safe-cex-venue-bitrue', 'safe-cex-venue-bitunix', 'safe-cex-venue-blofin', 'safe-cex-venue-bybit', 'safe-cex-venue-coinbase', 'safe-cex-venue-cryptocom', 'safe-cex-venue-orderly', 'safe-cex-venue-paradex', 'safe-cex-venue-phemex', 'safe-cex-venue-okx', 'safe-cex-venue-kraken', 'safe-cex-venue-grvt', 'safe-cex-venue-hyperliquid', 'safe-cex-venue-breakoutprop', 'safe-cex-venue-polymarket', 'safe-cex-venue-kucoin', 'safe-cex-venue-mexc', 'safe-cex-venue-lighter', 'safe-cex-venue-woo', 'safe-cex-venue-weex', 'safe-cex-venue-txflow']);
  const venue = addSyntheticVenue(policy, metadata);
  addEdge(metadata, 'safe-cex-registry', venue);
  assert.throws(() => validateLayers(policy, metadata), /^AssertionError \[ERR_ASSERTION\]: unlisted registry constructor: safe-cex-registry -> safe-cex-venue-unbound-control$/);
});
test('removing a listed registry constructor rejects its live edge', () => {
  const { policy, metadata } = inputs();
  policy.registryConstructors.venues = policy.registryConstructors.venues.filter(venue => venue !== 'safe-cex-venue-backpack');
  assert.throws(() => validateLayers(policy, metadata), /^AssertionError \[ERR_ASSERTION\]: unlisted registry constructor: safe-cex-registry -> safe-cex-venue-backpack$/);
});
test('a listed registry constructor without a live edge is stale', () => {
  const { policy, metadata } = inputs();
  const registry = metadata.packages.find(pkg => pkg.name === 'safe-cex-registry');
  registry.dependencies = registry.dependencies.filter(dep => dep.name !== 'safe-cex-venue-backpack');
  assert.throws(() => validateLayers(policy, metadata), /stale registry constructor: safe-cex-registry -> safe-cex-venue-backpack/);
});
test('venue cannot depend upward on constructor registry', () => {
  const { policy, metadata } = inputs();
  addEdge(metadata, 'safe-cex-venue-bybit', 'safe-cex-registry');
  assert.throws(() => validateLayers(policy, metadata), /upward dependency: safe-cex-venue-bybit -> safe-cex-registry/);
});

// Core's real facade edge changes which hard failure is encountered first.
test('core-to-venue injection is rejected as a cycle when facade imports core', () => {
  const { policy, metadata } = inputs();
  if (!metadata.packages.find(pkg => pkg.name === 'safe-cex').dependencies.some(dep => dep.name === 'safe-cex-exchange')) {
    addEdge(metadata, 'safe-cex', 'safe-cex-exchange');
    policy.exceptions.push({ from: 'safe-cex', to: 'safe-cex-exchange', reason: 'Fixture of actual facade re-export and health feature edge' });
  }
  addEdge(metadata, 'safe-cex-exchange', 'safe-cex-venue-bybit');
  addEdge(metadata, 'safe-cex-venue-bybit', 'safe-cex-exchange');
  assert.throws(() => validateLayers(policy, metadata), /dependency cycle: .*safe-cex-exchange -> safe-cex-venue-bybit -> safe-cex-exchange/);
});

test('a newly classified venue inherits only the named targets', () => {
  const { policy, metadata } = inputs();
  const name = 'safe-cex-venue-new';
  metadata.packages.push({id:name,name,manifest_path:'/workspace/new/Cargo.toml',dependencies:[]});
  metadata.workspace_members.push(name);
  policy.layers.find(layer => layer.name === 'venues').packages.push(name);
  for (const target of grantedTargets) addEdge(metadata, name, target);
  assert.equal(validateLayers(policy, metadata), metadata.workspace_members.length);
  addEdge(metadata, name, 'safe-cex-registry');
  assert.throws(() => validateLayers(policy, metadata), /upward dependency: safe-cex-venue-new -> safe-cex-registry/);
});

});
