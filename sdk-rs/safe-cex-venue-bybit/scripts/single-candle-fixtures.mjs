import ts from 'typescript';
import vm from 'node:vm';
import {readFileSync} from 'node:fs';
import assert from 'node:assert/strict';

const root = new URL('../../../packages/safe-cex/src/', import.meta.url);
const load = path => ts.createSourceFile(path, readFileSync(new URL(path, root), 'utf8'), ts.ScriptTarget.Latest, true);
const compile = code => ts.transpileModule(code, {compilerOptions: {target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS}}).outputText;
const types = load('types.ts');
const timeframe = types.statements.flatMap(s => ts.isVariableStatement(s) ? [...s.declarationList.declarations] : []).find(d => d.name.getText(types) === 'timeframeToSeconds');
assert.ok(timeframe?.initializer);
const timeframeToSeconds = vm.runInNewContext(compile(`(${timeframe.initializer.getText(types)})`));
const calendarFile = load('utils/calendarCandles.ts');
const calendarSource = calendarFile.statements.filter(s => ts.isFunctionDeclaration(s) || ts.isVariableStatement(s)).map(s => s.getText(calendarFile)).join('\n').replaceAll('export ', '');
const createLiveCandleStart = vm.runInNewContext(compile(`(function(){ ${calendarSource}; return createLiveCandleStart; })()`), {timeframeToSeconds});
const encode = value => typeof value === 'number' && !Number.isFinite(value) ? String(value) : Object.is(value, -0) ? '-0' : Array.isArray(value) ? value.map(encode) : value && typeof value === 'object' ? Object.fromEntries(Object.entries(value).map(([k,v]) => [k, encode(v)])) : value;
const cases = [];
for (const variant of ['bybit', 'bybitspot', 'bybitinverse']) {
  const file = load(`exchanges/bybit/${variant}.ws-public.ts`);
  const cls = file.statements.find(ts.isClassDeclaration);
  const member = cls.members.find(m => m.name?.getText(file) === 'listenOHLCV');
  const parsers = new Map();
  const visit = node => { if (ts.isVariableDeclaration(node) && ['parser', 'secondaryParser'].includes(node.name.getText(file))) parsers.set(node.name.getText(file), node.initializer.getText(file)); ts.forEachChild(node, visit); };
  visit(member);
  assert.equal(parsers.size, 2);
  const make = vm.runInNewContext(compile(`(function(opts,callback){ const topic='kline',secondaryTopic='trade'; const candleStartOf=createLiveCandleStart(opts.interval),isSeconds=opts.interval.endsWith('s');let lastCandle;return {kline:${parsers.get('parser')},trade:${parsers.get('secondaryParser')}};})`), {createLiveCandleStart, OrderSide: {Buy:'buy',Sell:'sell'}, console:{warn(){}}});
  for (const interval of ['1m', '1w']) for (const invalid of [false, true]) {
    const now = Date.UTC(2023,10,14,22,14,0);
    const start = createLiveCandleStart(interval)(now/1000)*1000;
    const width = timeframeToSeconds[interval]*1000;
    const rows = [
      {kind:'trade',frame:{data:[{T:now,p:'104',v:'208',S:'Sell'}]}},
      {kind:'kline',frame:{data:[{start,open:'100',high:'105',low:'99',close:'103',volume:'7'}]}},
      {kind:'trade',frame:{data:[{T:now+45000,p:invalid?'NaN':'104',v:'208',S:'Sell'}]}},
      {kind:'trade',frame:{data:[{T:start+width+1000,p:'95',v:'190',S:'Buy'}]}},
      {kind:'trade',frame:{data:[{T:now,p:'110',v:'220',S:'Sell'}]}},
      {kind:'kline',frame:{data:[{start,open:'1',high:'2',low:'0',close:'1',volume:'1'}]}},
    ];
    let emitted;
    const handlers = make.call({armOhlcvNoDataTimeout(){}}, {interval}, candle => {emitted=encode(structuredClone(candle));});
    const outcomes = rows.map(({kind,frame}) => {emitted=undefined;handlers[kind](frame);return emitted ?? null;});
    cases.push({variant,interval,rows,outcomes});
  }
}
assert.equal(cases.length, 12);
process.stdout.write(JSON.stringify(cases));
