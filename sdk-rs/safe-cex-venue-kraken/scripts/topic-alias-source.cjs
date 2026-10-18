// Execute unchanged live onOpen arrows selected by the maintained TS compiler AST.
'use strict';
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
assert.ok(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const root = path.resolve(__dirname, '../../..');
function ast(file) {
  const source = fs.readFileSync(path.join(root, 'packages/safe-cex/src', file), 'utf8');
  return ts.createSourceFile(file, source, ts.ScriptTarget.Latest, true);
}
function compile(source, globals = {}) {
  const result = ts.transpileModule(source, {compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS},reportDiagnostics:true});
  assert.equal((result.diagnostics || []).filter(d => d.category === ts.DiagnosticCategory.Error).length, 0);
  return vm.runInNewContext(result.outputText, globals);
}
function arrow(file, name) {
  const source = ast('exchanges/kraken/' + file);
  const cls = source.statements.filter(n => ts.isClassDeclaration(n) && n.name.text === name);
  assert.equal(cls.length, 1);
  const selected = cls[0].members.filter(n => n.name?.getText(source) === 'onOpen');
  assert.equal(selected.length, 1);
  return `function capture(){return (${selected[0].initializer.getText(source)});} capture;`;
}
const spot = ast('exchanges/kraken/krakenspot.types.ts');
const intervals = spot.statements.filter(ts.isVariableStatement).flatMap(s=>s.declarationList.declarations).filter(n=>n.name.getText(spot)==='INTERVAL');
assert.equal(intervals.length, 1);
const INTERVAL = compile(`(${intervals[0].initializer.getText(spot)});`);
const futures = compile(arrow('kraken.ws-public.ts', 'KrakenPublicWebsocket'));
const spotOpen = compile(arrow('krakenspot.ws-public.ts', 'KrakenSpotPublicWebsocket'), {INTERVAL});
const inputs = [[66,84,67,47,85,83,68], [0xd800], [0xdc00], [65,0xd800,66,0xdc00], [0xd83d,0xde00], [0,34,92,10]];
const rows = inputs.map(units => {
  const symbol = String.fromCharCode(...units), sent = [];
  function host(markets, books, candles) {return {isDisposed:false,parent:{store:{markets}},getTickerProductIds(){return [];},orderBookHandlers:books,ohlcvHandlers:candles,send(text){sent.push(text);}};}
  futures.call(host([], {[symbol]:[()=>{}]}, {}))();
  assert.equal(sent.length, 1); const future = sent.pop();
  spotOpen.call(host([{symbol}], {[symbol]:[()=>{}]}, {[symbol+'::1m']:[()=>{}]}))();
  assert.equal(sent.length, 3);
  return {units, futures:future, ticker:sent[0], book:sent[1], ohlc:sent[2]};
});
console.log(JSON.stringify(rows));
