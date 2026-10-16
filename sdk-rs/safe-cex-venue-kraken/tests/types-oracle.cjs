'use strict';
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const assert = require('node:assert/strict');
assert.ok(process.env.SAFE_CEX_TYPESCRIPT_PATH, 'SAFE_CEX_TYPESCRIPT_PATH is required');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const root = path.resolve(__dirname, '../../..');
const src = path.join(root, 'packages/safe-cex/src');
const cache = new Map();
function load(file) {
  if (cache.has(file)) return cache.get(file);
  assert.ok(file.startsWith(src + path.sep) || file === path.join(root, 'packages/logging/src/LogSeverity.ts'), `source outside safe-cex: ${file}`);
  const source = fs.readFileSync(file, 'utf8');
  const exports = {};
  cache.set(file, exports);
  const result = ts.transpileModule(source, {
    compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 },
    reportDiagnostics: true,
  });
  assert.equal((result.diagnostics || []).filter(d => d.category === ts.DiagnosticCategory.Error).length, 0);
  vm.runInNewContext(result.outputText, { exports, require(name) {
    if (name === '@tealstreet/logging/src/LogSeverity') return load(path.join(root, 'packages/logging/src/LogSeverity.ts'));
    assert.ok(name.startsWith('.'), `unexpected runtime dependency ${name}`);
    return load(path.resolve(path.dirname(file), name) + '.ts');
  } }, { filename: file });
  return exports;
}
function capture(value) {
  // Observe native type/bits BEFORE JSON; never collapse inherited functions to omission.
  if (typeof value === 'number') {
    const bytes = Buffer.alloc(8);
    bytes.writeDoubleBE(Number.isNaN(value) ? NaN : value);
    return { kind: 'number', bits: Number.isNaN(value) ? '7ff8000000000000' : bytes.toString('hex') };
  }
  if (typeof value === 'string') return { kind: 'string', units: Array.from({ length: value.length }, (_, i) => value.charCodeAt(i)) };
  if (prototypes.has(value)) return { kind: 'object', identity: 'Object.prototype' };
  if (value === undefined) return { kind: 'undefined' };
  if (typeof value === 'function') return { kind: 'function', text: String(value), length: value.length };
  if (Array.isArray(value)) return value.map(capture);
  assert.equal(typeof value, 'object');
  return Object.entries(value).map(([key, entry]) => [key, capture(entry)]);
}
const inheritedKeys = ['constructor', '__defineGetter__', '__defineSetter__', 'hasOwnProperty', '__lookupGetter__', '__lookupSetter__', 'isPrototypeOf', 'propertyIsEnumerable', 'toString', 'valueOf', '__proto__', 'toLocaleString'];
const futures = load(path.join(src, 'exchanges/kraken/kraken.types.ts'));
const spot = load(path.join(src, 'exchanges/kraken/krakenspot.types.ts'));
const prototypes = new Set([Object.getPrototypeOf(futures.BASE_URL), Object.getPrototypeOf(spot.BASE_URL)]);
const ordered = module => Object.fromEntries(Object.entries(module).filter(([, value]) => typeof value !== 'function').map(([name, value]) => [name, capture(value)]));
const probes = module => Object.fromEntries(Object.entries(module).filter(([, value]) => typeof value === 'object' && !Array.isArray(value)).map(([name, value]) => [name, [...Object.keys(value), ...inheritedKeys, 'missing', ''].map(key => [key, capture(value[key])])]));
const inputs = [...Object.keys(spot.ASSET_ALIASES), ...inheritedKeys, 'BTC', 'xbt', '', ' XBT', 'XBT ', '💰', '\u0000', '\ud800', '\udfff', 'a\ud800z'];
const aliasFile = path.join(src, 'exchanges/kraken/kraken.types.ts');
const ast = ts.createSourceFile(aliasFile, fs.readFileSync(aliasFile, 'utf8'), ts.ScriptTarget.Latest, true);
const alias = ast.statements.find(s => ts.isTypeAliasDeclaration(s) && s.name.text === 'KrakenBaseUrl');
assert.ok(alias && ts.isUnionTypeNode(alias.type), 'KrakenBaseUrl must remain a literal union');
const variants = alias.type.types.map(node => {
  assert.ok(ts.isLiteralTypeNode(node) && ts.isStringLiteral(node.literal), 'KrakenBaseUrl non-literal member');
  return node.literal.text;
});
process.stdout.write(JSON.stringify({
  header: { node: process.version, v8: process.versions.v8, arch: process.arch, platform: process.platform },
  futures: ordered(futures), spot: ordered(spot),
  probes: { futures: probes(futures), spot: probes(spot) },
  nestedProbes: { futures: probes(futures.BASE_WS_URL), spot: probes(spot.BASE_WS_URL) },
  normalize: inputs.map(asset => ({ input: capture(asset).units, output: capture(spot.normalizeAsset(asset)) })),
  variants,
}));
