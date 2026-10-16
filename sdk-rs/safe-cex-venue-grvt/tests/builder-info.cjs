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
const tree = ts.createSourceFile(file, source, ts.ScriptTarget.Latest, true);
const names = ['TEALSTREET_GRVT_BUILDER_ADDRESS_MAINNET', 'TEALSTREET_GRVT_BUILDER_ADDRESS_TESTNET', 'TEALSTREET_GRVT_BUILDER_ADDRESS', 'DEFAULT_GRVT_BUILDER_FEE_RATE', 'getGrvtBuilderAddress', 'grvtBuilderFeeRateToRaw', 'createGrvtBuilderInfo'];
// Execute actual constants and helper declarations without importing browser
// authorization side effects. Built-in parseFloat/Math are the real VM runtime.
const declarations = names.map(name => {
  const declaration = tree.statements.find(s => (ts.isVariableStatement(s) && s.declarationList.declarations.some(d => d.name.getText(tree) === name)) || (ts.isFunctionDeclaration(s) && s.name?.text === name));
  assert(declaration, `actual builder constant ${name}`);
  let text = declaration.getText(tree);
  if (name === 'createGrvtBuilderInfo' && process.env.GRVT_BUILDER_INFO_MUTATION) {
    assert.equal(process.env.GRVT_BUILDER_INFO_MUTATION, 'lowercase');
    assert(text.includes('builder.toLowerCase()'));
    text = text.replace('builder.toLowerCase()', 'builder');
  }
  return text;
});
const output = ts.transpileModule(declarations.join('\n'), {compilerOptions: {module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022},reportDiagnostics:true});
assert.equal(output.diagnostics.length, 0, 'builder constants transpile');
const exportsObject = {};
vm.runInNewContext(output.outputText, {exports:exportsObject}, {filename:file});

const bits = value => { const b = Buffer.alloc(8); b.writeDoubleBE(value); return b.toString('hex'); };
const addresses = [undefined, '', '0xABCDEF123', 'İΣΟΣ', 'AΣ', 'AΣA', 'AΣ\u0301', '漢字😀', 'ǅẞK'];
const fees = [undefined, '', '0.03', '-0', '-0.00005', '1e308', '-1e308', '1e400', '-1e-400', '12abc', 'NaN', '0x10'];
const rows = [];
function record(testnet, address, fee) {
  const input = {testnet, address, fee};
  try {
    const result = exportsObject.createGrvtBuilderInfo(testnet, address, fee);
    rows.push({input, builder:result.builder, fee:result.builderFee, bits:bits(result.builderFeeInt)});
  } catch (error) { rows.push({input, error:{name:error.name, message:error.message}}); }
}
for (const network of [undefined, false, true]) for (const address of addresses) for (const fee of fees) record(network, address, fee);
// Exercise every scalar in isolation and in sigma/case-ignorable context without
// creating a million JSON records. These are calls to the actual whole function.
let plain = [], contextual = [];
for (let cp = 0; cp <= 0x10ffff; ++cp) {
  if (cp >= 0xd800 && cp <= 0xdfff) continue;
  const c = String.fromCodePoint(cp);
  plain.push(c, '_'); contextual.push('A', c, 'Σ ');
}
record(false, plain.join(''), undefined);
record(true, contextual.join(''), '-0');
const refused = ['\uD800', '\uDC00', 'A\uD800Σ'].map(address => {
  const result = exportsObject.createGrvtBuilderInfo(false, address);
  return {wire:JSON.stringify(address), outputUnits:result.builder.split('').map(c => c.charCodeAt(0))};
});
process.stdout.write(JSON.stringify({rows, refused}));
