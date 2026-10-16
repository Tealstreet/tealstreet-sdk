'use strict';
const fs = require('node:fs'), path = require('node:path'), vm = require('node:vm'), assert = require('node:assert/strict');
const compiler = process.env.SAFE_CEX_TYPESCRIPT_PATH;
assert(compiler && path.isAbsolute(compiler), 'absolute isolated compiler required');
const ts = require(compiler);
process.stderr.write(JSON.stringify({captureArchitecture:process.arch, node:process.version})+'\n');
const root = path.resolve(__dirname, '../../../packages/safe-cex/src');
const read = name => ts.createSourceFile(name, fs.readFileSync(path.join(root, name), 'utf8'), ts.ScriptTarget.Latest, true);
const source = read('exchanges/grvt/grvt.exchange.ts');
const cls = source.statements.find(s => ts.isClassDeclaration(s) && s.name.text === 'GrvtExchange');
const method = cls.members.find(s => s.name?.getText(source) === 'convertInterval');
assert(method?.body, 'actual interval method');
const types = read('exchanges/grvt/grvt.types.ts');
const enumeration = types.statements.find(s => ts.isEnumDeclaration(s) && s.name.text === 'GrvtCandlestickInterval');
assert(enumeration, 'actual interval enum');
const shared = read('types.ts');
const timeframe = shared.statements.find(s => ts.isTypeAliasDeclaration(s) && s.name.text === 'Timeframe');
assert(timeframe && ts.isUnionTypeNode(timeframe.type), 'closed Timeframe union');
const options = shared.statements.find(s => ts.isTypeAliasDeclaration(s) && s.name.text === 'OHLCVOptions');
assert(options && ts.isTypeLiteralNode(options.type), 'closed OHLCVOptions shape');
const intervalField = options.type.members.find(m => m.name?.getText(shared) === 'interval');
assert(intervalField && ts.isPropertySignature(intervalField) && !intervalField.questionToken && intervalField.type?.getText(shared) === 'Timeframe', 'OHLCV interval retains closed Timeframe');
const caller = cls.members.find(s => s.name?.getText(source) === 'fetchOHLCV')?.initializer;
assert(caller && ts.isArrowFunction(caller) && caller.parameters[0]?.type?.getText(source) === 'OHLCVOptions', 'fetchOHLCV typed caller boundary');
const keys = timeframe.type.types.map(t => { assert(ts.isLiteralTypeNode(t) && ts.isStringLiteral(t.literal)); return t.literal.text; });
const table = method.body.statements.find(ts.isVariableStatement).declarationList.declarations[0].initializer;
assert(ts.isObjectLiteralExpression(table), 'source interval table');
assert.deepEqual(table.properties.map(p => { assert(ts.isPropertyAssignment(p) && ts.isStringLiteral(p.name)); return p.name.text; }), keys, 'typed caller covers exactly own interval keys');
let calls = 0;
function walk(node) {
  if (ts.isCallExpression(node) && ts.isPropertyAccessExpression(node.expression) && node.expression.name.text === 'convertInterval') {
    assert.equal(node.arguments.length, 1);
    assert.equal(node.arguments[0].getText(source), 'opts.interval');
    calls++;
  }
  ts.forEachChild(node, walk);
}
walk(source);
assert.equal(calls, 1, 'sole convertInterval caller census');
let body = method.body.getText(source);
if (process.env.GRVT_INTERVAL_CONTROL === 'own-only') {
  assert(body.includes('mapping[interval] ||'));
  body = body.replace('mapping[interval] ||', '(Object.hasOwn(mapping, interval) ? mapping[interval] : undefined) ||');
}
const context = vm.createContext({exports:{}});
vm.runInContext(ts.transpileModule(enumeration.getText(types)+'\nexport function measured(interval:string) '+body,
  {compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText, context);
const protoKeys = vm.runInContext('Object.getOwnPropertyNames(Object.prototype)', context);
assert.equal(protoKeys.length, 12, 'stock prototype member census');
const inherited = vm.runInContext('Object.fromEntries(Object.getOwnPropertyNames(Object.prototype).map(key=>[key,({})[key]]))', context);
const inputs = [...keys, ...protoKeys, '', '8h', '3d', '5d', '2w', '3w', '4w', '😀', '漢字'];
for (const key of keys) inputs.push(key.toUpperCase(), ' '+key, key+' ', key+'\n', key+'\0');
const rows = inputs.map(input => {
  const value = context.exports.measured(input);
  if (typeof value === 'string') return {input, mapped:value};
  assert.strictEqual(value, inherited[input], 'inherited source identity');
  return {input, inherited:input, type:typeof value, string:String(value), length:value.length ?? null};
});
const refusals = ['"\\ud800"', '"\\udfff"', '"1m\\ud800"'].map(raw => ({raw, output:context.exports.measured(JSON.parse(raw))}));
process.stdout.write(JSON.stringify({rows, refusals}));
