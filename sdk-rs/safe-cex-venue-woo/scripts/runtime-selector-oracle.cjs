// Source-first capture; stdout only, one process for the full corpus.
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const assert = require('node:assert/strict');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const bits = require('./number-bits.cjs');
const root = path.resolve(__dirname, '../../..');
function load(name, imports = {}, extra = '') {
  const exports = {};
  let source = fs.readFileSync(path.join(root, 'packages/safe-cex/src/exchanges/woo', name), 'utf8');
  if (process.argv.includes('--break-route-alias') && name === 'woo.v3.runtime-selector.ts') {
    assert.ok(source.includes('  return route;'));
    source = source.replace('  return route;', '  return { ...route };');
  }
  vm.runInNewContext(ts.transpileModule(source + extra, {
    compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 },
  }).outputText, { exports, require: key => {
    assert.ok(Object.hasOwn(imports, key), key);
    return imports[key];
  } });
  return exports;
}
const contracts = load('woo.v3.contracts.ts');
const api = load('woo.v3.runtime-selector.ts', { './woo.v3.contracts': contracts },
  '\nexport { requireRoute, target };');
function observe(value) {
  if (typeof value === 'number') return { numberBits: bits(value) };
  if (Array.isArray(value)) return value.map(observe);
  if (value && typeof value === 'object') return Object.fromEntries(Object.entries(value).map(([k, v]) => [k, observe(v)]));
  return value;
}
const routes = contracts.WOO_V3_MIGRATION_ROUTES;
const modes = ['"legacy"', '"shadow-v3"', '"v3"', '"LEGACY"', '""', 'null', 'false', 'true',
  '0', '-0', '1e400', '-1e-400', '[]', '{}', '{"mode":"legacy"}', '{"nested":[-0,1e400]}'];
// Post-capture supplement: distinct nonzero finite payload identity.
modes.push('1.25');
const cases = [];
for (const routeId of [...routes.map(route => route.id), '', 'constructor', '__proto__', 'unknown', '😀', '\ufeff']) {
  for (const mode of modes) {
    let outcome;
    try {
      const result = api.selectWooV3RuntimeRoute(routeId, JSON.parse(mode));
      assert.equal(Object.isFrozen(result), true, 'source selector outer freeze');
      if (result.authoritative) assert.equal(Object.isFrozen(result.authoritative), true, 'source target freeze');
      if (result.observer) assert.equal(Object.isFrozen(result.observer), true, 'source observer freeze');
      outcome = { ok: observe(result) };
    } catch (error) { outcome = { error: { name: error.name, message: error.message } }; }
    cases.push({ routeId, mode, ...outcome });
  }
}
const targets = [];
for (const generation of ['legacy', 'v3']) for (const method of ['GET', 'POST', 'PUT', 'DELETE']) {
  const routePath = '/probe/' + generation + '/' + method;
  const result = api.target(generation, method, routePath);
  assert.ok(Object.isFrozen(result), 'source standalone target freeze');
  targets.push({ generation, method, path: routePath, result });
}
const copiedRoutes = JSON.parse(JSON.stringify(routes));
const route = api.requireRoute('markets');
assert.equal(route, routes[0], 'source requireRoute returns registry alias');
const before = api.selectWooV3RuntimeRoute('markets', 'legacy').authoritative.path;
route.legacyPath = '/mutated-source-route';
const after = api.selectWooV3RuntimeRoute('markets', 'legacy').authoritative.path;
assert.equal(after, '/mutated-source-route', 'source route mutation reaches later selector');
route.legacyPath = before;
const mode = { version: 'before' };
const selection = api.selectWooV3RuntimeRoute('markets', mode);
assert.equal(selection.requestedMode, mode, 'source invalid mode retains alias');
mode.version = 'after';
assert.equal(selection.requestedMode.version, 'after', 'source invalid mode sees later mutation');
const lone = JSON.parse('"\\ud800"');
let surrogateMessage;
try { api.requireRoute(lone); } catch (error) { surrogateMessage = error.message; }
assert.equal(surrogateMessage.charCodeAt(surrogateMessage.length - 1), 0xd800, 'source route diagnostic retains surrogate');
const surrogateTarget = api.target('legacy', 'GET', lone);
assert.equal(surrogateTarget.path.charCodeAt(0), 0xd800, 'source target retains surrogate');
const surrogateSelection = api.selectWooV3RuntimeRoute('markets', lone);
assert.equal(surrogateSelection.requestedMode.charCodeAt(0), 0xd800, 'source mode retains surrogate');
const undefinedMode = api.selectWooV3RuntimeRoute('markets', undefined);
assert.equal(Object.hasOwn(undefinedMode, 'requestedMode'), true, 'source retains present undefined mode');
assert.equal(undefinedMode.requestedMode, undefined);
const nativeNaNMode = api.selectWooV3RuntimeRoute('markets', NaN);
assert.ok(Number.isNaN(nativeNaNMode.requestedMode), 'source retains native NaN mode');
const openMethodTarget = api.target('legacy', 'PATCH', '/probe');
assert.equal(openMethodTarget.method, 'PATCH', 'source annotation does not validate target method');
process.stdout.write(JSON.stringify({ capture: { platform: process.platform, architecture: process.arch, node: process.version },
  routes: copiedRoutes, cases, targets, routeAlias: { before, after },
  modeAlias: { after: selection.requestedMode.version }, representation: { input: JSON.stringify(lone) },
  excluded: { undefinedMode: Object.hasOwn(undefinedMode, 'requestedMode'), nanMode: bits(nativeNaNMode.requestedMode), openMethod: openMethodTarget.method } }));
