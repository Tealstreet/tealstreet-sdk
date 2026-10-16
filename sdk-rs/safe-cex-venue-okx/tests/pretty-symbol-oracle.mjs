import fs from 'node:fs';
import vm from 'node:vm';
import assert from 'node:assert/strict';
import {ts} from '../../safe-cex-store/tests/source_loader.mjs';
const capture = {platform:process.platform, architecture:process.arch, nodeMajor:Number(process.versions.node.split('.')[0])};
const url = new URL('../../../packages/safe-cex/src/exchanges/okx/okx.exchange.ts', import.meta.url);
let source = fs.readFileSync(url, 'utf8');
if (process.argv.includes('--mutate-base-quote')) source = source.replace('`${base}/${quote} XPERP`', '`${quote}/${base} XPERP`');
const ast = ts.createSourceFile(url.pathname, source, ts.ScriptTarget.Latest, true);
const owner = ast.statements.find(n => ts.isClassDeclaration(n) && n.name.text === 'OKXExchange');
const member = owner.members.find(n => n.name?.getText(ast) === 'formatMarketPrettySymbol');
assert.ok(member, 'maintained pretty-symbol member');
const code = ts.transpileModule('export class Selected {' + member.getText(ast) + '}', {
  compilerOptions: {target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS},
}).outputText;
const module = {exports: {}};
vm.runInThisContext('(function(module,exports){' + code + '\n})')(module, module.exports);
const receiver = new module.exports.Selected();
if (process.argv.includes('--surrogate-witness')) {
  const baseRaw = '"BASE\\ud800"';
  const quoteRaw = '"QUOTE\\udfff"';
  const value = receiver.formatMarketPrettySymbol({instType:'FUTURES', ruleType:'xperp'}, JSON.parse(baseRaw), JSON.parse(quoteRaw));
  assert.equal(value.charCodeAt(4), 0xd800, 'source pretty-symbol lone high surrogate');
  assert.equal(value.charCodeAt(11), 0xdfff, 'source pretty-symbol lone low surrogate');
  process.stdout.write(JSON.stringify({capture, baseRaw, quoteRaw, wire:JSON.stringify(value)}));
  process.exit(0);
}
const cases = [];
function run(marketRaw, baseRaw, quoteRaw) {
  const decode = raw => raw === undefined ? undefined : JSON.parse(raw);
  let value, error;
  try { value = receiver.formatMarketPrettySymbol(decode(marketRaw), decode(baseRaw), decode(quoteRaw)); }
  catch (e) { error = {name:e.name, message:e.message}; }
  const record = {marketRaw, baseRaw, quoteRaw, defined:value !== undefined, value, error};
  cases.push(record);
  return record;
}
const types = ['FUTURES','SWAP','futures',null,0,false,[],{},'constructor'];
const rules = ['xperp','XPERP',null,0,false,[],{},'__proto__'];
const pairs = [['BTC','USDT'],['ETH','USD'],['','EUR'],['BASE\uFEFF','QUOTE😀'],['BASE-α','QUOTE-β']];
for (const instType of types) for (const ruleType of rules) for (const [base,quote] of pairs) {
  const result = run(JSON.stringify({instType, ruleType}), JSON.stringify(base), JSON.stringify(quote));
  const expected = instType === 'FUTURES' && ruleType === 'xperp';
  assert.equal(result.defined, expected, 'pretty-symbol exact branch and absence');
  if (expected) assert.equal(result.value, `${base}/${quote} XPERP`, 'pretty-symbol base quote identity');
}
for (const marketRaw of [undefined,'null','false','0','"scalar"','[]','{}']) run(marketRaw,'"BASE"','"QUOTE"');
const raw = [undefined,'null','false','0','-0','1.25','1e400','"BASE"','[]','[1,"x"]','{}','{"toString":null}'];
for (const baseRaw of raw) for (const quoteRaw of raw) {
  run('{"instType":"FUTURES","ruleType":"xperp"}',baseRaw,quoteRaw);
  const cold = run('{"instType":"SWAP","ruleType":"xperp"}',baseRaw,quoteRaw);
  assert.equal(cold.defined,false,'pretty-symbol cold absence');
  assert.equal(cold.error,undefined,'pretty-symbol cold branch must not coerce');
}
assert.equal(cases.length,655);
process.stdout.write(JSON.stringify({capture,cases}));
