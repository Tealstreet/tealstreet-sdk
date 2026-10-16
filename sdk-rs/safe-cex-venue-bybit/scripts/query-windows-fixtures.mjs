// Live capture architecture is recorded in captureArchitecture; NaN alone is canonicalized.
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import vm from 'node:vm';
import ts from 'typescript';
const path = new URL('../../../packages/safe-cex/src/exchanges/bybit/bybit.exchange.ts', import.meta.url);
const file = ts.createSourceFile(path.pathname, readFileSync(path, 'utf8'), ts.ScriptTarget.Latest, true);
const owner = file.statements.find(n => ts.isClassDeclaration(n) && n.name?.text === 'BybitExchange');
const names = ['EXECUTION_QUERY_MAX_RANGE_MS', 'DEFAULT_EXECUTIONS_LOOKBACK_MS', 'getExecutionQueryWindows'];
const members = names.map(name => {
  const member = owner.members.find(n => n.name?.getText(file) === name);
  assert.ok(member, `source member ${name}`);
  return member.getText(file);
});
const context = vm.createContext({});
vm.runInContext(ts.transpileModule(`class Subject { ${members.join('\n')} }; globalThis.subject = new Subject();`, {
  compilerOptions: {target:ts.ScriptTarget.ES2022},
}).outputText, context);
const bits = value => {
  if (value === undefined) return null;
  if (Number.isNaN(value)) return "7ff8000000000000";
  const buffer = Buffer.alloc(8); buffer.writeDoubleBE(value);
  return buffer.toString('hex');
};
assert.equal(bits(-0), '8000000000000000', 'capture preserves signed zero');
assert.equal(bits(1.25), '3ff4000000000000', 'capture preserves finite payload');
assert.equal(bits(-NaN), '7ff8000000000000', 'capture canonicalizes only NaN');
const pairs = [
  [null,null], [null,'0'], ['-0',null], [null,'Infinity'], ['NaN',null],
  ['0','0'], ['-0','0'], ['0','-0'], ['-0','-0'], ['2','1'],
  ['0','604799999'], ['0','604800000'], ['0','604800001'],
  ['0','1209600000'], ['0.25','604800000.5'], ['-604800000','0'],
  ['NaN','1'], ['1','NaN'], ['Infinity','Infinity'], ['Infinity','0'],
  ['0','-Infinity'], ['1e20','100000000001209600000'],
];
// Deterministic short and multi-window ranges, including reversed intervals.
for (let i=0;i<200;i++) {
  const from = (i-100)*123456789.125;
  const to = from + ((i%9)-2)*604800000 + (i%3)*0.25;
  pairs.push([String(from),String(to)]);
}
const cases = pairs.map(([from,to]) => {
  const result = context.subject.getExecutionQueryWindows(from === null ? undefined : Number(from), to === null ? undefined : Number(to));
  return {from,to,windows:Array.from(result, w => [bits(w.startTime),bits(w.endTime)])};
});
// ts-bug:singleton-window-omission: the strict loop guard skips the final
// singleton [from, from] after the one-millisecond step reaches from exactly.
assert.deepEqual(cases[11].windows, [[bits(1), bits(604800000)]], 'source singleton-window omission');
process.stdout.write(JSON.stringify({
  captureArchitecture:process.arch,
  maxRange:bits(context.subject.EXECUTION_QUERY_MAX_RANGE_MS),
  lookback:bits(context.subject.DEFAULT_EXECUTIONS_LOOKBACK_MS),
  cases,
}));
