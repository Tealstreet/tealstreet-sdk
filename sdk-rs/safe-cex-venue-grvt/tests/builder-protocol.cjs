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
if (process.env.GRVT_BUILDER_PROTOCOL_MUTATION === 'address') {
  const original = '0xee8f3559f298fffa79b8253437969d15b5dccff3';
  assert(source.includes(original), 'builder address mutation matched');
  source = source.replace(original, '0x0000000000000000000000000000000000000000');
} else assert(!process.env.GRVT_BUILDER_PROTOCOL_MUTATION, 'unknown builder mutation');
const tree = ts.createSourceFile(file, source, ts.ScriptTarget.Latest, true);
const names = ['TEALSTREET_GRVT_BUILDER_ADDRESS_MAINNET', 'TEALSTREET_GRVT_BUILDER_ADDRESS_TESTNET', 'TEALSTREET_GRVT_BUILDER_ADDRESS', 'DEFAULT_GRVT_BUILDER_FEE_RATE'];
// Execute the actual declarations, preserving the legacy export's reference to
// the mainnet constant. No ethers, wallet, network or package mocks are involved.
const declarations = names.map(name => {
  const declaration = tree.statements.find(s => ts.isVariableStatement(s) && s.declarationList.declarations.some(d => d.name.getText(tree) === name));
  assert(declaration, `actual builder constant ${name}`);
  return declaration.getText(tree);
});
const output = ts.transpileModule(declarations.join('\n'), {compilerOptions: {module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022},reportDiagnostics:true});
assert.equal(output.diagnostics.length, 0, 'builder constants transpile');
const exportsObject = {};
vm.runInNewContext(output.outputText, {exports:exportsObject}, {filename:file});
for (const name of names) assert.equal(typeof exportsObject[name], 'string', `builder string ${name}`);
process.stdout.write(JSON.stringify(names.map(name => [name, exportsObject[name]])));
