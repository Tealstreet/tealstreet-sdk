'use strict';
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const compiler = process.env.SAFE_CEX_TYPESCRIPT_PATH;
assert(compiler && path.isAbsolute(compiler), 'absolute compiler installation required');
const ts = require(compiler);
const text = fs.readFileSync(path.resolve(__dirname, '../../../packages/safe-cex/src/exchanges/grvt/grvt.exchange.ts'), 'utf8');
const ast = ts.createSourceFile('grvt.exchange.ts', text, ts.ScriptTarget.Latest, true);
const cls = ast.statements.find(node => ts.isClassDeclaration(node) && node.name.text === 'GrvtExchange');
const member = cls.members.find(node => node.name?.getText(ast) === 'refreshMarkets');
assert(member && ts.isArrowFunction(member.initializer), 'REFRESH actual source method');
const compiled = ts.transpileModule('exports.call = async function() ' + member.initializer.body.getText(ast), {
  compilerOptions: {target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS}, reportDiagnostics: true,
});
assert.equal(compiled.diagnostics.length, 0, 'REFRESH source compiles');
const context = {exports: {}, LogSeverity: {Info: 'info'}};
vm.runInNewContext(compiled.outputText, context);
function market(id, hash) {
  return {id, symbol: id.toUpperCase(), base: 'B', quote: 'Q', active: true,
    precision: {amount: 0.01, price: 0.2}, limits: {amount: {min: 0.01, max: 20}, leverage: {min: 1, max: 100}},
    isSpot: false, isPerpetual: true, info: {ih: hash, bd: 3, qd: 6}};
}
(async () => {
  const rows = [];
  for (const name of ['metadata', 'membership', 'reorder', 'fetch-error', 'store-error', 'emit-error', 'log-error']) {
    const effects = [];
    const store = {markets: [market('a', 'old-a'), market('b', 'old-b')], update({markets}) {
      this.markets = markets;
      effects.push(['store', markets.map(market => market.id)]);
      if (name === 'store-error') throw Error('store-fail');
    }};
    const ids = name === 'metadata' ? ['a', 'b'] : name === 'reorder' ? ['b', 'a'] : ['b', 'c'];
    const exchange = {store, async fetchMarkets() {
      effects.push(['fetch']);
      if (name === 'fetch-error') throw Error('fetch-fail');
      return ids.map(id => market(id, 'new-' + id));
    }, emitter: {emit(event, changes) {
      effects.push(['emit', event, changes.added.map(m => m.id), changes.removed.map(m => m.id)]);
      if (name === 'emit-error') throw Error('emit-fail');
    }}, log(message, severity) {
      effects.push(['log', message, severity]);
      if (name === 'log-error') throw Error('log-fail');
    }};
    let result = null, error = null;
    try { result = await context.exports.call.call(exchange); } catch (cause) { error = String(cause); }
    rows.push({name, result, error, markets: store.markets, effects});
  }
  assert.equal(rows.length, 7, 'REFRESH independent source floor');
  assert.equal(rows[0].markets[0].info.ih, 'new-a', 'REFRESH metadata replacement');
  assert.equal(rows[1].result.removed[0].info.ih, 'old-a', 'REFRESH removed old metadata');
  let finish;
  const aliasStore = {markets: [market('a', 'old-a')], update({markets}) { this.markets = markets; }};
  const aliasExchange = {store: aliasStore, fetchMarkets: () => new Promise(resolve => { finish = resolve; }),
    emitter: {emit() {}}, log() {}};
  const pending = context.exports.call.call(aliasExchange);
  aliasStore.markets[0].info.ih = 'mutated-a';
  finish([]);
  const aliases = await pending;
  assert.equal(aliases.removed[0].info.ih, 'mutated-a', 'REFRESH source old object alias');
  process.stdout.write(JSON.stringify({rows, aliases}));
})().catch(error => { console.error(error); process.exitCode = 1; });
