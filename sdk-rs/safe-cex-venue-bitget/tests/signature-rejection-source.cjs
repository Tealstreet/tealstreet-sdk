// Execute the current venue predicate, including live coercion and getters.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const file = path.resolve(__dirname, '../../../packages/safe-cex/src/exchanges/bitget/bitget.api.ts');
const tree = ts.createSourceFile(file, fs.readFileSync(file, 'utf8'), ts.ScriptTarget.Latest, true);
assert.equal(tree.parseDiagnostics.length, 0);
const declarations = tree.statements.filter(ts.isVariableStatement).flatMap(n => [...n.declarationList.declarations]);
const declaration = declarations.find(n => n.name.getText(tree) === 'isBitgetSignatureRejection');
assert.ok(declaration);
const scope = {};
vm.runInNewContext(ts.transpileModule(`globalThis.predicate = ${declaration.initializer.getText(tree)};`, {compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText, scope);
const cases = [
  ['undefined', undefined], ['null', null], ['bool', true], ['number', 40009], ['empty', {}],
  ['numeric-code', {code:40009}], ['string-code', {code:'40009'}], ['array-code', {code:[40009]}],
  ['bigint-code', {code:40009n}], ['other-code', {code:'400090'}],
  ['message', {msg:'prefix SIGN SIGNATURE ERROR suffix'}], ['numeric-message', {msg:40009}],
  ['null-message', {msg:null}], ['array-message', {msg:['sign signature error']}],
  ['long-s', {msg:'ſign signature error'}], ['surrogate-positive', {msg:'\ud800sign signature error'}],
  ['surrogate-break', {msg:'sign signa\ud800ture error'}],
  ['coerced-code', {code:{toString(){return '40009';}}}],
  ['coerced-message', {msg:{toString(){return 'sign signature error';}}}],
  ['short-circuit', {code:'40009', get msg(){throw new RangeError('unreached');}}],
  ['code-throws', {get code(){throw new RangeError('code refused');}}],
  ['message-throws', {get msg(){throw new RangeError('message refused');}}],
];
const rows = cases.map(([label,value]) => {
  try { return {label,result:scope.predicate(value)}; }
  catch (error) { return {label,error:error.name}; }
});
process.stdout.write(JSON.stringify(rows));
