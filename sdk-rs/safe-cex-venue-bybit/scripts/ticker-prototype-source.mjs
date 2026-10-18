import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import vm from 'node:vm';
import ts from 'typescript';

const root = new URL('../../../packages/safe-cex/src/', import.meta.url);
const context = vm.createContext({ exports: {}, structuredClone });
function source(path) {
  return ts.createSourceFile(path, readFileSync(new URL(path, root), 'utf8'), ts.ScriptTarget.Latest, true);
}
function evaluate(text) {
  return vm.runInContext(ts.transpileModule(text, {
    compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS },
  }).outputText, context);
}
function module(path) {
  const ast = source(path);
  evaluate(ast.statements.filter(s => !ts.isImportDeclaration(s)).map(s => s.getText(ast)).join('\n'));
}
const types = source('types.ts');
const status = types.statements.filter(s => ts.isEnumDeclaration(s) && s.name.text === 'OrderStatus');
assert.equal(status.length, 1);
evaluate(status[0].getText(types));
module('utils/clone.ts');
context.clone = context.exports.clone;
module('store/store.base.ts');
const Store = context.exports.DefaultStore;
const initial = (id, symbol) => ({
  id, symbol, bid: 1, ask: 2, last: 3, mark: 4, index: 5, percentage: 6,
  openInterest: 7, fundingRate: 8, fundingTime: 9, volume: 10, quoteVolume: 11,
});
const base = [initial('BTCUSDT', 'BTCUSDT'), initial('undefined', 'UNDEFINED-ID')];
const rows = [];
for (const variant of ['bybit', 'bybitdemo', 'bybitinverse', 'bybitspot']) {
  const file = variant === 'bybitdemo' ? 'bybit' : variant;
  const ast = source(`exchanges/bybit/${file}.ws-public.ts`);
  const cls = ast.statements.find(ts.isClassDeclaration);
  const member = cls.members.filter(m => m.name?.getText(ast) === 'handleTickersEvent');
  assert.equal(member.length, 1);
  const make = evaluate(`(function() { return ${member[0].initializer.getText(ast)}; })`);
  const cases = [];
  for (const symbol of [...Object.getOwnPropertyNames(Object.prototype), 'BTCUSDT', 'UNKNOWN']) {
    for (const values of [{ lastPrice: { toString: null } }, { lastPrice: '42' }, {}]) {
      cases.push({ tickers: base, frame: { topic: `tickers.${symbol}`, data: { symbol, ...values } } });
    }
  }
  for (const [symbol, extra] of [
    ['toString', initial('shadow', 'toString')],
    ['__proto__', initial('prototype', '__proto__')],
    ['last', initial('prototype', '__proto__')],
  ]) {
    for (const lastPrice of ['42', { toString: null }]) {
      cases.push({ tickers: [...base, extra], frame: { topic: `tickers.${symbol}`, data: { symbol, lastPrice } } });
    }
  }
  assert.equal(cases.length, 48);
  for (const input of cases) {
    const store = new Store();
    store.update({ tickers: context.clone(input.tickers) }, false);
    let notifications = 0;
    store.subscribe(() => notifications++);
    let error;
    try {
      make.call({ parent: { store }, category: variant === 'bybitinverse' ? 'inverse' : 'linear' })(input.frame);
    } catch (e) {
      error = { name: e.name, message: e.message };
    }
    const result = store.tickers.map(t => ({ last: t.last, bid: t.bid }));
    if (input.frame.data.lastPrice?.toString === null && input.frame.data.symbol !== 'UNKNOWN') {
      assert.equal(error?.name, 'TypeError');
      assert.equal(error.message, 'Cannot convert object to primitive value');
      assert.equal(notifications, 0);
    }
    rows.push({ variant, ...input, result, notifications, error });
  }
}
assert.equal(rows.length, 192);
console.log(JSON.stringify(rows));
