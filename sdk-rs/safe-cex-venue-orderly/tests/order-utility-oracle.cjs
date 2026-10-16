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
assert.equal(classes.length, 1, 'order-utility class census');
const fields = ['getCurrentMarginType', 'resolveMarginTypeForOrder', 'isAlgoOrder'].map(name => {
  const matches = classes[0].members.filter(m => ts.isPropertyDeclaration(m) && m.name?.getText(tree) === name);
  assert.equal(matches.length, 1, `actual utility initializer/${name}`);
  return `${name} = ${matches[0].initializer.getText(tree)};`;
});
const typesFile = path.join(root, 'packages/safe-cex/src/types.ts');
const types = ts.createSourceFile(typesFile, fs.readFileSync(typesFile, 'utf8'), ts.ScriptTarget.Latest, true);
const enums = ['MarginType', 'OrderSide', 'PositionSide', 'OrderType'].map(name => {
  const matches = types.statements.filter(s => ts.isEnumDeclaration(s) && s.name.text === name);
  assert.equal(matches.length, 1, `actual shared enum/${name}`);
  return matches[0].getText(types);
});
const source = enums.join('\n') + '\nexport class Fixture { constructor(public store: any, public marginTypeBySymbol: any) {} ' + fields.join('\n') + ' }';
const code = ts.transpileModule(source, { compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS } }).outputText;
const mod = { exports: {} };
new vm.Script(`(function(module,exports,uniqBy){${code}\n})`, { filename: file }).runInThisContext()(mod, mod.exports, external('lodash/uniqBy'));
const rows = [];
function record(label, side, reduceOnly, cache, specs, symbol = 'BTC') {
  const opts = { symbol, side, type: 'limit', amount: 3.25 };
  if (reduceOnly !== undefined) opts.reduceOnly = reduceOnly;
  const positions = specs.map(([positionSymbol, positionSide, contracts, margin], i) => {
    const value = { id: String(i), symbol: positionSymbol, side: positionSide, entryPrice: 7, notional: 11, leverage: 13, unrealizedPnl: 17, contracts, liquidationPrice: 19 };
    if (margin !== undefined) value.marginType = margin;
    return value;
  });
  const fixture = new mod.exports.Fixture({ positions }, new Map(cache));
  let expected;
  try { expected = { margin: fixture.resolveMarginTypeForOrder(opts) }; }
  catch (e) { expected = { name: e.name, message: e.message }; }
  rows.push({ label, optsJson: JSON.stringify(opts), cache, positionsJson: JSON.stringify(positions), expected });
}
record('buy-closes-short', 'buy', true, [], [['BTC', 'short', 1, 'isolated'], ['BTC', 'long', 2, 'cross']]);
record('sell-closes-long', 'sell', true, [], [['BTC', 'long', 1, 'isolated'], ['BTC', 'short', 2, 'cross']]);
record('ambiguous', 'buy', true, [], [['BTC', 'short', 1, 'isolated'], ['BTC', 'short', 2, 'cross']]);
record('cached-selection', 'buy', true, [['BTC', 'isolated']], [['BTC', 'short', 1, 'isolated'], ['BTC', 'short', 2, 'cross']]);
record('duplicate-cached', 'buy', true, [['BTC', 'isolated']], [['BTC', 'short', 1, 'isolated'], ['BTC', 'short', 2, 'isolated']]);
record('non-reduce', 'buy', false, [['BTC', 'isolated']], [['BTC', 'short', 1, 'cross']]);
record('zero-is-not-candidate', 'buy', true, [], [['BTC', 'short', 0, 'cross'], ['BTC', 'short', 1, 'isolated']]);
for (const side of ['buy', 'sell']) for (const reduceOnly of [undefined, false, true]) for (const cached of [undefined, 'cross', 'isolated']) {
  const cache = cached === undefined ? [] : [['BTC', cached]];
  for (const first of [undefined, 'cross', 'isolated']) for (const second of [undefined, 'cross', 'isolated']) {
    for (const contracts of [-1, 0, 0.5]) {
      record(`matrix/${side}/${reduceOnly}/${cached}/${first}/${second}/${contracts}`, side, reduceOnly, cache,
        [['BTC', 'short', contracts, first], ['BTC', 'long', 2, second], ['ETH', 'short', 3, 'isolated']]);
    }
  }
  record(`empty/${side}/${reduceOnly}/${cached}`, side, reduceOnly, cache, []);
}
record('ambiguous-non-btc', 'buy', true, [], [['ETH:日本語', 'short', 1, 'isolated'], ['ETH:日本語', 'short', 2, 'cross']], 'ETH:日本語');
const fixture = new mod.exports.Fixture({ positions: [] }, new Map());
const algo = Object.values(mod.exports.OrderType).map(input => ({ input, expected: fixture.isAlgoOrder(input) }));
const surrogate = '\ud800';
const boundary = new mod.exports.Fixture({ positions: [] }, new Map([[surrogate, 'isolated']]));
console.log(JSON.stringify({ runtime: { node: process.version, arch: process.arch }, rows, algo, boundary: { unit: surrogate.charCodeAt(0), margin: boundary.resolveMarginTypeForOrder({ symbol: surrogate, side: 'buy', reduceOnly: true }) } }));
