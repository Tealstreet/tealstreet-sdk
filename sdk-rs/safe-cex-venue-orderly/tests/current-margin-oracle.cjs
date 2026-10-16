const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const assert = require('node:assert/strict');
assert.ok(process.env.SAFE_CEX_TYPESCRIPT_PATH, 'SAFE_CEX_TYPESCRIPT_PATH required');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const external = require('node:module').createRequire(path.join(process.env.SAFE_CEX_TYPESCRIPT_PATH, 'package.json'));
const root = path.resolve(__dirname, '../../..');
const file = path.join(root, 'packages/safe-cex/src/exchanges/orderly/orderly.exchange.ts');
const tree = ts.createSourceFile(file, fs.readFileSync(file, 'utf8'), ts.ScriptTarget.Latest, true);
const classes = tree.statements.filter(s => ts.isClassDeclaration(s) && s.name?.text === 'OrderlyExchange');
assert.equal(classes.length, 1, 'current-margin source class census');
const fields = classes[0].members.filter(m => ts.isPropertyDeclaration(m) && m.name?.getText(tree) === 'getCurrentMarginType');
assert.equal(fields.length, 1, 'current-margin initializer census');
const typesFile = path.join(root, 'packages/safe-cex/src/types.ts');
const types = ts.createSourceFile(typesFile, fs.readFileSync(typesFile, 'utf8'), ts.ScriptTarget.Latest, true);
const enums = types.statements.filter(s => ts.isEnumDeclaration(s) && s.name.text === 'MarginType');
assert.equal(enums.length, 1, 'actual margin enum census');
// Test host supplies fields; the complete production arrow initializer is unchanged.
const source = enums[0].getText(types) + '\nexport class Fixture { constructor(public store: any, public marginTypeBySymbol: any) {} getCurrentMarginType = ' + fields[0].initializer.getText(tree) + '; }';
const code = ts.transpileModule(source, { compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS } }).outputText;
const mod = { exports: {} };
new vm.Script(`(function(module,exports,uniqBy){${code}\n})`, { filename: file }).runInThisContext()(mod, mod.exports, external('lodash/uniqBy'));
const rows = [];
function record(symbol, cache, specs, label) {
  const tokens = specs.map(([positionSymbol, contracts, margin], i) => {
    const value = { id: String(i), symbol: positionSymbol, side: 'long', entryPrice: 7, notional: 11, leverage: 13, unrealizedPnl: 17, contracts: '__contracts__', liquidationPrice: 19 };
    if (margin !== undefined) value.marginType = margin;
    return JSON.stringify(value).replace('"__contracts__"', contracts === 'NaN' ? '0' : contracts);
  });
  const positionsJson = '[' + tokens.join(',') + ']';
  const positions = JSON.parse(positionsJson);
  const nanIndices = specs.flatMap((spec, i) => spec[1] === 'NaN' ? [i] : []);
  for (const i of nanIndices) positions[i].contracts = NaN;
  const fixture = new mod.exports.Fixture({ positions }, new Map(cache));
  const bits = positions.map(p => { if (Number.isNaN(p.contracts)) return '7ff8000000000000'; const b = Buffer.alloc(8); b.writeDoubleBE(p.contracts); return b.toString('hex'); });
  rows.push({ label, symbol, cache, positionsJson, nanIndices, bits, expected: fixture.getCurrentMarginType(symbol) });
}
record('BTC', [], [], 'empty');
record('BTC', [], [['BTC', '1', 'isolated']], 'one-isolated');
record('BTC', [['BTC', 'cross']], [['BTC', '1', 'isolated']], 'cache-wins');
record('BTC', [], [['BTC', '0', 'isolated']], 'zero-is-inactive');
record('BTC', [], [['BTC', '1', 'isolated'], ['BTC', '2', 'isolated']], 'deduplicate');
record('BTC', [], [['BTC', '1', 'isolated'], ['BTC', '2', undefined]], 'missing-is-distinct');
record('BTC', [], [['BTC', '1', 'isolated'], ['ETH', '2', 'cross']], 'other-symbol');
for (const contracts of ['NaN', '-1', '-0', '0', '5e-324', '-5e-324', '1', '1e999', '-1e999', '-1e-999', '9007199254740993']) {
  for (const first of [undefined, 'cross', 'isolated']) for (const second of [undefined, 'cross', 'isolated']) {
    record('BTC', [], [['BTC', contracts, first], ['BTC', '2', second]], `pair/${contracts}/${first}/${second}`);
  }
}
for (const symbol of ['constructor', '__proto__', '日本語😀', '']) {
  record(symbol, [[symbol, 'isolated']], [], 'cache-key/' + symbol);
  record(symbol, [['BTC', 'cross']], [[symbol, '1', 'isolated']], 'symbol-key/' + symbol);
}
const surrogate = '\ud800';
const boundary = new mod.exports.Fixture({ positions: [] }, new Map([[surrogate, 'isolated']]));
console.log(JSON.stringify({ runtime: { node: process.version, arch: process.arch }, rows, boundary: { unit: surrogate.charCodeAt(0), expected: boundary.getCurrentMarginType(surrogate) } }));
