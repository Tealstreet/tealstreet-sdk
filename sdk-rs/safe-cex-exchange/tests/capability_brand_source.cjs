const fs = require('node:fs');
const path = require('node:path');
const assert = require('node:assert/strict');
const ts = require('typescript');

const file = path.resolve(__dirname, '../../../packages/safe-cex/src/exchanges/base.ts');
const text = fs.readFileSync(file, 'utf8');
const source = ts.createSourceFile(file, text, ts.ScriptTarget.Latest, true);
const names = new Set(['baseCapabilityFallback', 'markBaseCapabilityFallback', 'isBaseCapabilityFallback']);
const declarations = source.statements.filter(node => ts.isVariableStatement(node) &&
  node.declarationList.declarations.some(declaration => names.has(declaration.name.getText(source))));
assert.equal(declarations.length, 3);
const methodType = source.statements.find(node => ts.isTypeAliasDeclaration(node) && node.name.text === 'BaseCapabilityMethod');
assert(ts.isUnionTypeNode(methodType.type));
const methods = methodType.type.types.map(node => {
  assert(ts.isLiteralTypeNode(node) && ts.isStringLiteral(node.literal));
  return node.literal.text;
});
const compiled = ts.transpileModule(declarations.map(node => node.getText(source)).join('\n'), {
  compilerOptions: {module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022},
}).outputText;
const {mark, check, symbol} = new Function('exports', `${compiled}
return {mark:exports.markBaseCapabilityFallback,check:exports.isBaseCapabilityFallback,symbol:baseCapabilityFallback};`)({});

let calls = 0;
const matrix = methods.map(method => {
  const fn = () => { calls++; return []; };
  assert.equal(mark(method, fn), fn);
  const descriptor = Object.getOwnPropertyDescriptor(fn, symbol);
  assert.deepEqual(descriptor, {value: method, configurable: false, enumerable: false, writable: false});
  assert.equal(Reflect.set(fn, symbol, 'start'), false);
  assert.equal(Reflect.deleteProperty(fn, symbol), false);
  assert.equal(Object.hasOwn({...fn}, symbol), false);
  return methods.map(query => check(fn, query));
});
assert.equal(calls, 0, 'branding and inspection never execute the callback');
const original = mark('fetchIncomeHistory', () => []);
const exchange = {fetchIncomeHistory: original};
const alias = exchange.fetchIncomeHistory;
exchange.fetchIncomeHistory = () => [];
const replacement = {
  retainedAlias: check(alias, 'fetchIncomeHistory'),
  replacementBranded: check(exchange.fetchIncomeHistory, 'fetchIncomeHistory'),
  originalEmpty: original().length === 0,
  replacementEmpty: exchange.fetchIncomeHistory().length === 0,
};
const marker = {};
const callback = () => { calls++; return marker; };
const implemented = {
  brand: methods.map(method => check(callback, method)),
  callsBefore: calls,
  originalResult: callback() === marker,
  callsAfter: calls,
};
for (const value of [undefined, null, 0, '', {}, mark('start', {})]) {
  assert.equal(check(value, 'start'), false);
}
process.stdout.write(JSON.stringify({methods, matrix, replacement, implemented}));
