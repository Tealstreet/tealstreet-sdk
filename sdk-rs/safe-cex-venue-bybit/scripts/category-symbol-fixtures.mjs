import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import vm from 'node:vm';
import ts from 'typescript';
const root = new URL('../../../packages/safe-cex/src/exchanges/', import.meta.url);
function methods(file, owner, names) {
  const source = ts.createSourceFile(file, readFileSync(new URL(file, root), 'utf8'), ts.ScriptTarget.Latest, true);
  const node = source.statements.find(n => ts.isClassDeclaration(n) && n.name?.text === owner);
  const selected = node.members.filter(n => names.includes(n.name?.getText(source)));
  assert.equal(selected.length, names.length, `source method census ${owner}`);
  return selected.map(n => n.getText(source)).join('\n');
}
// The routing predicates execute from their maintained modules (type-only imports).
const helpers = {};
for (const file of ['bybit/bybit.options.ts', 'bybit/bybit.inverse.ts']) {
  const exports = {};
  vm.runInNewContext(ts.transpileModule(readFileSync(new URL(file, root), 'utf8'), {
    compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS },
  }).outputText, { exports, require() { throw Error('no runtime imports expected'); } });
  Object.assign(helpers, exports);
}
const context = vm.createContext({ isBybitOptionSymbol: helpers.isBybitOptionSymbol, isBybitInverseSymbol: helpers.isBybitInverseSymbol });
const source = methods('bybit/bybit.exchange.ts', 'BybitExchange', ['getCategoryForSymbol', 'getTickerSymbolsForCategory', 'getSettleCoinsForCategory', 'accountType'])
  + methods('base.ts', 'BaseExchange', ['getActiveTickerSymbols', 'getTickerSymbolsForSubscription']);
const subject = vm.runInContext(ts.transpileModule(`new class { ${source} }`, {
  compilerOptions: { target: ts.ScriptTarget.ES2022 },
}).outputText, context);
const symbols = ['', 'BTCUSD', 'BTCUSDT', 'constructor', '__proto__', '😀', 'ÉTH', ' unknown ', 'ETHUSDZ26', 'BTC-25SEP26-130000-C-USDT']
  .map(symbol => ({ symbol, category: subject.getCategoryForSymbol(symbol) }));
const market = (symbol, quote, active, isInverse) => ({
  id: `id:${symbol}:${quote}`, symbol, base: 'base-only', quote, active, isInverse,
  precision: { amount: 0.01, price: 0.5 },
  limits: { amount: { min: 1, max: 7 }, leverage: { min: 2, max: 19 } },
});
const mixed = [market('BTCUSD', 'USDT', false, true), market('BTCUSDT', 'USD', true, false),
  market('DUP', 'USDC', true, true), market('DUP', 'USDT', false, false),
  market('😀', 'usd', true, false), market('ÉTH', '', false, true), market(' spaced ', 'USDC', false, false),
  { ...market('BTC-25SEP26-130000-C-USDT', 'USDT', true, false), option: { underlying: 'BTC', expiry: 1790323200000, strike: 130000,
    right: 'call', style: 'european', settlement: 'cash', premiumAsset: 'USDT', settleAsset: 'USDT' } }];
const lists = [[], mixed, [...mixed].reverse(), mixed.slice(0, 1), mixed.slice(1, 2), mixed.slice(2, 4)];
const activeSets = [[], ['BTCUSD'], ['BTCUSDT'], ['DUP', '😀'], ['MISSING'], [' spaced ']];
const cases = [];
for (const markets of lists) for (const active of activeSets) for (const category of ['linear', 'inverse']) {
  // These active inputs are already normalized; no setter behavior is approximated.
  subject.store = { markets }; subject._activeTickerSymbols = active;
  cases.push({ markets, active, category, expected: subject.getTickerSymbolsForCategory(category) });
}
const surrogate = market('\ud800', 'USDT', true, false);
subject.store = { markets: [surrogate] }; subject._activeTickerSymbols = [];
const actual = subject.getTickerSymbolsForCategory('linear');
assert.equal(actual[0].charCodeAt(0), 0xd800, 'source retains lone surrogate symbol');
const boundary = { wire: JSON.stringify(surrogate), outputUnits: Array.from(actual[0], s => s.charCodeAt(0)) };
assert.equal(cases.length, 72);
const settlements=[];
for(const status of [null,1,"1",0,2,true,{},[]]) for(const category of ['linear','inverse','spot','','LINEAR','constructor','😀']) for(const fail of [false,true]) {
  subject.unifiedMarginStatus=status;
  const reports=[];
  const sentinel=new Error('reporter failure');
  subject.emitAndLogError=message=>{reports.push(message);if(fail)throw sentinel;};
  let coins=null,error=null,original=false;
  try {coins=subject.getSettleCoinsForCategory(category);} catch(e){error=e.message;original=e===sentinel;}
  settlements.push({status,category,fail,reports,coins,error,original});
}
subject.emitAndLogError=message=>{assert.equal(message.charCodeAt(message.length-1),0xd800);};
let surrogateError;
try{subject.getSettleCoinsForCategory('\ud800');}catch(e){surrogateError=e.message;}
assert.equal(surrogateError.charCodeAt(surrogateError.length-1),0xd800);
const settlementBoundary=JSON.stringify('\ud800');
process.stdout.write(JSON.stringify({ settlements, settlementBoundary, capture: { architecture: process.arch, node: process.versions.node }, symbols, cases, boundary }));
