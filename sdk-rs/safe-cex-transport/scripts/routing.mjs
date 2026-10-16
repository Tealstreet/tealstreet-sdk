import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import vm from 'node:vm';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';
import { createRequire } from 'node:module';
const require = createRequire(import.meta.url);
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH ?? 'typescript');

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../../..');
const routing = 'packages/safe-cex/src/transport/tealstreet/routing/';
const output = path.join(root, 'sdk-rs/safe-cex-transport/src/routing_generated.rs');
const read = (p) => fs.readFileSync(path.join(root, p), 'utf8');
// Evaluate only repository-owned pure modules; unknown dependencies fail closed.
function load(p, imports = {}, extra = '') {
  const exports = {};
  const js = ts.transpileModule(read(p) + extra, { compilerOptions: { module: ts.ModuleKind.CommonJS } }).outputText;
  vm.runInNewContext(js, { exports, URL, require: (name) => {
    assert.ok(Object.hasOwn(imports, name), `unexpected import ${name} in ${p}`);
    return imports[name];
  } }, { filename: p });
  return exports;
}
const snapshotExports = {};
vm.runInNewContext(read(`${routing}__tests__/__snapshots__/routing.equivalence.test.ts.snap`), { exports: snapshotExports });
assert.equal(Object.keys(snapshotExports).length, 1);
const snapshot = JSON.parse(Object.values(snapshotExports)[0].replace(/,\s*([}\]])/g, '$1'));
const names = Object.keys(snapshot).sort();
const egress = load('packages/http-over-websocket/src/egressRegion.ts');
assert.ok(egress.TOKYO_EGRESS_EXCHANGES.every((token) => /^[a-z0-9-]+$/.test(token)), 'HOW tokens must be literal ASCII substrings; regex semantics require an explicit port');
const endpoints = load('packages/http-over-websocket/src/tunnelEndpoints.ts');
const pools = load('packages/safe-cex/src/transport/tealstreet/transportResolution.ts');
const table = load(`${routing}routingTable.ts`, {
  '@tealstreet/http-over-websocket/src/egressRegion': egress,
  '../../../exchangeMetadata': { exchangeMetadata: Object.fromEntries(names.map((name) => [name, {}])) },
});
const resolver = load(`${routing}resolveExchangeRouting.ts`, {
  '@tealstreet/http-over-websocket/src/tunnelEndpoints': endpoints,
  '../transportResolution': pools,
  './routingTable': table,
}).resolveExchangeRouting;
const enumText = read('sdk-rs/safe-cex-types/src/lib.rs').split('pub enum ExchangeName {')[1].split('\n}')[0];
const variants = new Map([...enumText.matchAll(/#\[serde\(rename = "([^"]+)"\)\]\s*(\w+),/g)].map((m) => [m[1], m[2]]));
assert.deepEqual([...variants.keys()].sort(), names, 'snapshot must cover every Rust exchange');
// Check real metadata keys without importing adapter constructors and their side effects.
const metadata = ts.createSourceFile('metadata.ts', read('packages/safe-cex/src/exchangeMetadata.ts'), ts.ScriptTarget.Latest, true);
const metadataKeys = [];
function visit(node) {
  if (ts.isVariableDeclaration(node) && node.name.getText(metadata) === 'exchangeMetadata') {
    let value = node.initializer;
    while (ts.isAsExpression(value) || ts.isSatisfiesExpression(value)) value = value.expression;
    assert.ok(ts.isObjectLiteralExpression(value));
    for (const entry of value.properties) metadataKeys.push(entry.name.text);
  }
  ts.forEachChild(node, visit);
}
visit(metadata);
assert.deepEqual(metadataKeys.sort(), names, 'snapshot must cover every TS exchange');
// Run the golden test's assertions, including Brazil literals and baseURL/name egress.
const plain = (value) => value === undefined ? undefined : JSON.parse(JSON.stringify(value));
function matchObject(actual, expected) {
  for (const [key, value] of Object.entries(expected)) {
    if (value && typeof value === 'object') matchObject(actual[key], value);
    else assert.deepEqual(actual[key], value);
  }
}
let currentTest;
const vitest = {
  describe: (_name, run) => run(),
  it: (name, run) => {
    currentTest = name;
    assert.equal(run()?.then, undefined, 'async golden tests require an async harness');
  },
  expect: (actual, message = currentTest) => ({
    toBe: (expected) => assert.equal(actual, expected, message),
    toEqual: (expected) => assert.deepEqual(plain(actual), plain(expected), message),
    toMatchObject: (expected) => matchObject(plain(actual), plain(expected)),
    toBeUndefined: () => assert.equal(actual, undefined, message),
    toBeDefined: () => assert.notEqual(actual, undefined, message),
    toContain: (expected) => assert.ok(actual.includes(expected), message),
    toMatchSnapshot: () => assert.deepEqual(plain(actual), snapshot),
  }),
};
const goldenTests = load(`${routing}__tests__/routing.equivalence.test.ts`, {
  vitest,
  '@tealstreet/http-over-websocket/src/egressRegion': egress,
  '../../../../exchangeMetadata': { exchangeMetadata: Object.fromEntries(names.map((name) => [name, {}])) },
  '../../transportResolution': pools,
  '../resolveExchangeRouting': { resolveExchangeRouting: resolver },
}, '\nexport { REST_BASE_URLS };');
const expectedKeys = [];
for (const proxy of [false, true]) for (const wsRoute of [false, true]) for (const region of ['NONE', 'JA', 'SG']) for (const site of ['NONE', 'BRA_BTL'])
  expectedKeys.push(`proxy=${Number(proxy)} wsRoute=${Number(wsRoute)} region=${region} site=${site}`);
for (const name of names) assert.deepEqual(Object.keys(snapshot[name]).sort(), expectedKeys.toSorted(), `snapshot option coverage ${name}`);
const cases = [];
const routes = [];
const indexes = new Map();
function intern(route) {
  // HOW stays derived at runtime; relay follows that same derived region.
  const { howEgress, ...data } = route;
  for (const channel of ['wsPublic', 'wsPrivate', 'wsBusiness']) {
    data[channel] = { ...data[channel] };
    if (data[channel].relay) data[channel].relay = endpoints.parseRelayUrl(data[channel].relay).upstream;
  }
  const key = JSON.stringify(data, (_key, value) =>
    value && typeof value === 'object' && !Array.isArray(value)
      ? Object.fromEntries(Object.keys(value).sort().map((key) => [key, value[key]]))
      : value);
  if (!indexes.has(key)) { indexes.set(key, routes.length); routes.push(data); }
  return indexes.get(key);
}
const rows = [];
for (const name of names) {
  const indices = [];
  assert.equal(Object.keys(snapshot[name]).length, 24);
  const brazil = [];
  for (const site of [undefined, 'BRA_BTL']) {
    for (const proxy of [false, true]) for (const wsRoute of [false, true]) for (const region of [undefined, 'JA', 'SG']) {
      const options = { tealstreetProxy: proxy, tealstreetWsRoute: wsRoute, ...(region ? { proxyRegion: region } : {}), ...(site ? { bybitSiteId: site } : {}) };
      const key = `proxy=${Number(proxy)} wsRoute=${Number(wsRoute)} region=${region ?? 'NONE'} site=${site ?? 'NONE'}`;
      const golden = snapshot[name][key];
      assert.deepEqual(plain(resolver(name, options)), golden, `TS vs golden ${name} ${key}`);
      const index = intern(golden);
      assert.equal(intern(Object.fromEntries(Object.entries(golden).reverse())), index, 'routing interning must ignore object key order');
      (site ? brazil : indices).push(index);
      cases.push({ exchange: name, options: { key: "oracle", secret: "oracle", ...options }, expected: golden });
    }
  }
  const conditional = snapshot[name]['proxy=0 wsRoute=0 region=JA site=NONE'].howEgress !== snapshot[name]['proxy=0 wsRoute=0 region=SG site=NONE'].howEgress;
  for (const c of cases.filter((c) => c.exchange === name)) {
    const derived = conditional ? (c.options.proxyRegion === 'JA' ? 'jp' : 'sg') : (egress.isTokyoEgress(name) ? 'jp' : 'sg');
    assert.equal(derived, c.expected.howEgress, `derived HOW ${name}`);
  }
  rows.push({ name, indices, brazil, conditional });
}
const q = JSON.stringify;
const opt = (x) => x === undefined ? 'None' : `Some(${q(x)})`;
const ws = (s) => `StaticWs { tunnelled: ${s.tunnelled}, host: ${opt(s.host)}, fallback: ${opt(s.fallback)}, relay_upstream: ${opt(s.relay)}, readonly: ${opt(s.readonly)} }`;
let generated = '// Generated by scripts/routing.mjs from routing.equivalence.test.ts snapshot. Do not edit.\n';
generated += 'use safe_cex_types::ExchangeName;\nuse super::{StaticRoute, StaticWs};\n';
generated += `pub(crate) const TOKYO_KEYS: &[&str] = &[${egress.TOKYO_EGRESS_EXCHANGES.map(q).join(',')}];\n`;
generated += `pub(crate) const TUNNEL_JP: &str = ${q(endpoints.TOKYO_HOW_EGRESS)};\npub(crate) const TUNNEL_SG: &str = ${q(endpoints.SINGAPORE_HOW_EGRESS)};\n`;
generated += `pub(crate) const ROUTES: &[StaticRoute] = &[\n${routes.map((r) => `StaticRoute { rest_proxy_host: ${q(r.restProxyHost)}, ws_public: ${ws(r.wsPublic)}, ws_private: ${ws(r.wsPrivate)}, ws_business: ${ws(r.wsBusiness)} },`).join('\n')}\n];\n`;
generated += 'pub(crate) fn row(exchange: &ExchangeName, brazil: bool) -> ([usize; 12], bool, &\'static str) {\nmatch exchange {\n';
for (const r of rows) {
  const indices = q(r.brazil) === q(r.indices) ? `[${r.indices}]` : `if brazil { [${r.brazil}] } else { [${r.indices}] }`;
  generated += `ExchangeName::${variants.get(r.name)} => (${indices}, ${r.conditional}, ${q(r.name)}),\n`;
}
generated += '}\n}\n';
const cargo = process.env.CARGO ?? 'cargo';
const formatted = spawnSync(process.env.RUSTFMT ?? 'rustfmt', ['--edition', '2024'], { input: generated, encoding: 'utf8' });
assert.equal(formatted.status, 0, formatted.error?.message ?? formatted.stderr);
if (process.argv.includes('--check')) assert.equal(fs.readFileSync(output, 'utf8'), formatted.stdout, 'generated routing is stale');
else if (!process.argv.includes('--equivalence-only')) fs.writeFileSync(output, formatted.stdout);
if (process.argv.includes('--equivalence') || process.argv.includes('--equivalence-only')) {
  const result = spawnSync(cargo, ['run', '--locked', '--quiet', '--manifest-path', path.join(root, 'sdk-rs/Cargo.toml'), '-p', 'safe-cex-transport', '--example', 'routing'], { input: JSON.stringify({ cases, egressKeys: [...Object.values(goldenTests.REST_BASE_URLS), ...names].flatMap((key) => [key, key.toUpperCase()]) }), encoding: 'utf8', maxBuffer: 8 * 1024 * 1024 });
  assert.equal(result.status, 0, result.error?.message ?? result.stderr);
  const response = JSON.parse(result.stdout);
  const keys = [...Object.values(goldenTests.REST_BASE_URLS), ...names].flatMap((key) => [key, key.toUpperCase()]);
  assert.deepEqual(response.egress, keys.map(egress.isTokyoEgress), 'Rust vs HOW predicate');
  const actual = response.routes;
  assert.equal(actual.length, cases.length);
  actual.forEach((route, i) => assert.deepEqual(route, cases[i].expected, `Rust vs TS ${cases[i].exchange} ${q(cases[i].options)}`));
}
console.log(`Routing: ${names.length} exchanges, ${cases.length} cross-language cases, ${routes.length} unique routes; ${process.argv.includes('--check') ? 'fresh' : 'generated'}.`);
