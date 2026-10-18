// Maintained source constructor, URL selectors and local cancellation policy.
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const assert = require('node:assert/strict');
const root = path.resolve(__dirname, '../../..');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || path.join(root, 'node_modules/typescript'));
const dir = path.join(root, 'packages/safe-cex/src');
const cache = new Map();
function load(file) {
  if (cache.has(file)) return cache.get(file);
  const exports = {};
  cache.set(file, exports);
  let text = fs.readFileSync(path.join(dir, file), 'utf8');
  if (file === 'types.ts') {
    const source = ts.createSourceFile(file, text, ts.ScriptTarget.Latest, true);
    text = source.statements.filter(ts.isEnumDeclaration).map(n => n.getText(source)).join('\n');
  }
  vm.runInNewContext(ts.transpileModule(text, {compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText, {
    exports, process, require: name => load(path.normalize(path.join(path.dirname(file), name)) + '.ts'),
  });
  return exports;
}
function members(file, name, wanted) {
  const source = ts.createSourceFile(file, fs.readFileSync(path.join(dir, 'exchanges/bybit', file), 'utf8'), ts.ScriptTarget.Latest, true);
  const cls = source.statements.find(n => ts.isClassDeclaration(n) && n.name?.text === name);
  return wanted.map(key => {
    const found = cls.members.filter(n => key === 'constructor' ? ts.isConstructorDeclaration(n) : n.name?.getText(source) === key);
    assert.equal(found.length, 1, key);
    return found[0].getText(source);
  }).join('\n');
}
const body = members('hyrotrader.exchange.ts', 'HyrotraderExchange', [
  'constructor', 'hyrotraderMode', 'getConfiguredUrls', 'getBybitRestBaseUrl',
  'getBybitPublicWsUrl', 'getBybitPrivateWsUrl', 'shouldUseLocalRestOrderLifecycleFallback',
]);
const retirement = members('bybit.exchange.ts', 'BybitExchange', ['retireCancelledOrders']);
const Parent = class {
  constructor(options) {
    this.options = options;
    this.dispose = () => {};
    this.orderRecovery = {dispose() {}};
    this.activeOrderObservations = new Set();
    this.pendingOrderLists = new Map();
    this.retiredOrderSymbols = new Map();
    this.markOrderSeen = () => {};
    this.removed = [];
    this.store = {removeOrders: rows => this.removed.push(...rows)};
  }
};
const Source = vm.runInNewContext(ts.transpileModule(`class Source extends Parent { ${body}\n${retirement} }; Source;`, {
  compilerOptions:{target:ts.ScriptTarget.ES2022},
}).outputText, {Parent, ...load('exchanges/bybit/bybit.types.ts'), virtualClock:{getCurrentTimeMs:() => 1}});
const rows = [];
for (const mode of [undefined, 'demo', 'production']) {
  const venue = new Source(mode === undefined ? {} : {extra:{hyrotraderMode:mode}});
  const row = {mode:mode ?? 'absent', isDemo:venue.isDemo, fallback:venue.shouldUseLocalRestOrderLifecycleFallback()};
  for (const [key, method, args] of [
    ['rest', 'getBybitRestBaseUrl', ['livenet']],
    ['public', 'getBybitPublicWsUrl', ['linear', 'livenet']],
    ['private', 'getBybitPrivateWsUrl', ['livenet']],
    ['invalidCategory', 'getBybitPublicWsUrl', ['inverse', 'livenet']],
  ]) {
    try { row[key] = venue[method](...args); }
    catch (error) { row[key] = {error:error.message}; }
  }
  venue.retireCancelledOrders([{id:'mode-order', symbol:'BTCUSDT'}]);
  row.removed = venue.removed;
  rows.push(row);
}
assert.equal(rows.length, 3);
process.stdout.write(JSON.stringify(rows));
