'use strict';
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const compiler = process.env.SAFE_CEX_TYPESCRIPT_PATH;
assert(compiler && path.isAbsolute(compiler), 'absolute isolated compiler required');
const ts = require(compiler);
const file = path.resolve(__dirname, '../../../packages/safe-cex/src/exchanges/grvt/grvt.builder.ts');
let source = fs.readFileSync(file, 'utf8');
if (process.env.GRVT_DOMAIN_MUTATION === 'network') {
  const needle = 'chainId: testnet ? 326 : 325';
  assert(source.includes(needle), 'domain source mutation matched');
  source = source.replace(needle, 'chainId: testnet ? 325 : 326');
} else assert(!process.env.GRVT_DOMAIN_MUTATION, 'unknown domain source mutation');
const tree = ts.createSourceFile(file, source, ts.ScriptTarget.Latest, true);
const statement = tree.statements.find(s => ts.isVariableStatement(s) && s.declarationList.declarations.some(d => d.name.getText(tree) === 'getEIP712Domain'));
assert(statement, 'actual builder domain declaration');
const declaration = statement.declarationList.declarations.find(d => d.name.getText(tree) === 'getEIP712Domain');
assert(ts.isArrowFunction(declaration.initializer), 'domain arrow shape');
assert.equal(declaration.initializer.parameters.length, 1, 'domain parameter count');
assert.equal(declaration.initializer.parameters[0].type.kind, ts.SyntaxKind.BooleanKeyword, 'domain boolean parameter');
const calls = [];
function visit(node) {
  if (ts.isCallExpression(node) && node.expression.getText(tree) === 'getEIP712Domain') calls.push(node);
  ts.forEachChild(node, visit);
}
visit(tree);
assert.equal(calls.length, 1, 'domain caller census');
assert.equal(calls[0].arguments[0].getText(tree), 'testnet', 'domain caller argument');
const compiled = ts.transpileModule(statement.getText(tree) + '\nexports.domain = getEIP712Domain;', { compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS }, reportDiagnostics: true });
assert.equal(compiled.diagnostics.length, 0, 'domain transpile');
const context = { exports: {} };
vm.runInNewContext(compiled.outputText, context, { filename: file });
const rows = [false, true].map(testnet => {
  const value = context.exports.domain(testnet);
  return { testnet, value, keys: Object.keys(value) };
});
process.stdout.write(JSON.stringify(rows));
