import ts from 'typescript';
import vm from 'node:vm';
import {readFileSync} from 'node:fs';
import chunk from 'lodash/chunk.js';
import assert from 'node:assert/strict';

const root = new URL('../../../packages/safe-cex/src/', import.meta.url);
const load = path => ts.createSourceFile(path, readFileSync(new URL(path, root), 'utf8'), ts.ScriptTarget.Latest, true);
const compile = code => ts.transpileModule(code, {compilerOptions: {target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS}}).outputText;
const tableFile = load('exchanges/bybit/bybit.types.ts');
const table = tableFile.statements.flatMap(s => ts.isVariableStatement(s) ? [...s.declarationList.declarations] : []).find(d => d.name.getText(tableFile) === 'INTERVAL');
assert.ok(table?.initializer);
const INTERVAL = vm.runInNewContext(compile(`(${table.initializer.getText(tableFile)})`));
const helpers = {exports: {}};
vm.runInNewContext(compile(readFileSync(new URL('utils/inverseObj.ts', root), 'utf8')), helpers);
const INTERVAL_INVERSE = helpers.exports.inverseObj(INTERVAL);
const encode = value => value === undefined ? '$undefined' : typeof value === 'number' && !Number.isFinite(value) ? String(value) : Object.is(value, -0) ? '-0' : Array.isArray(value) ? value.map(encode) : value && typeof value === 'object' ? Object.fromEntries(Object.entries(value).map(([k,v]) => [k, encode(v)])) : value;
const cases = [];
for (const variant of ['bybit', 'bybitspot', 'bybitinverse']) {
  const file = load(`exchanges/bybit/${variant}.ws-public.ts`);
  const cls = file.statements.find(ts.isClassDeclaration);
  const member = cls.members.find(m => m.name?.getText(file) === 'listenOHLCVs');
  assert.ok(member?.initializer);
  const make = vm.runInNewContext(compile(`(function(){return ${member.initializer.getText(file)};})`), {INTERVAL, INTERVAL_INVERSE, KLINE_TOPIC_PREFIX_LEN: 6, chunk});
  for (const wire of ['1', '15', '60', 'D', 'W', 'M', '999', 'null', 'object', 'boolean', 'number', 'absent']) {
    const host = {parent: {supportsTradeDerivedCandles: true}, topics: {}, messageHandlers: {}, isConnected: true, isDisposed: false, send() {}};
    let emitted;
    make.call(host)({symbols: ['BTCUSDT'], interval: '1m'}, candle => {emitted = encode(candle);});
    const frame = {topic: `kline.${wire}.BTCUSDT`, data: [{start: '1700000040000', open: '100', high: '105', low: '99', close: '103', volume: '7'}]};
    if (['null', 'object', 'boolean', 'number', 'absent'].includes(wire)) {
      frame.data = {null: null, object: {}, boolean: false, number: 42, absent: undefined}[wire];
      let error;
      try { host.messageHandlers.kline[0](frame); } catch (caught) { error = {name: caught.name, message: caught.message}; }
      assert.equal(error?.name, 'TypeError');
      cases.push({variant, frame, error});
      continue;
    }
    host.messageHandlers.kline[0](frame);
    assert.equal(emitted.interval, wire === '999' ? '$undefined' : INTERVAL_INVERSE[wire]);
    cases.push({variant, frame, emitted});
  }
}
assert.equal(cases.length, 36);
process.stdout.write(JSON.stringify(cases));
