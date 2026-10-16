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
if (process.env.GRVT_BUILDER_HELPER_MUTATION === 'round') {
  assert(source.includes('Math.round(parsed * 10_000)'), 'fee mutation matched source');
  source = source.replace('Math.round(parsed * 10_000)', 'Math.floor(parsed * 10_000)');
} else if (process.env.GRVT_BUILDER_HELPER_MUTATION === 'network') {
  const needle = 'testnet ? TEALSTREET_GRVT_BUILDER_ADDRESS_TESTNET';
  assert(source.includes(needle), 'network mutation matched source');
  source = source.replace(needle, '!testnet ? TEALSTREET_GRVT_BUILDER_ADDRESS_TESTNET');
} else assert(!process.env.GRVT_BUILDER_HELPER_MUTATION, 'unknown helper mutation');
const tree = ts.createSourceFile(file, source, ts.ScriptTarget.Latest, true);
const names = ['TEALSTREET_GRVT_BUILDER_ADDRESS_MAINNET', 'TEALSTREET_GRVT_BUILDER_ADDRESS_TESTNET', 'TEALSTREET_GRVT_BUILDER_ADDRESS', 'DEFAULT_GRVT_BUILDER_FEE_RATE', 'getGrvtBuilderAddress', 'grvtBuilderFeeRateToRaw'];
// Execute actual constants and helper declarations without importing browser
// authorization side effects. Built-in parseFloat/Math are the real VM runtime.
const declarations = names.map(name => {
  const declaration = tree.statements.find(s => ts.isVariableStatement(s) && s.declarationList.declarations.some(d => d.name.getText(tree) === name));
  assert(declaration, `actual builder constant ${name}`);
  return declaration.getText(tree);
});
const output = ts.transpileModule(declarations.join('\n'), {compilerOptions: {module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022},reportDiagnostics:true});
assert.equal(output.diagnostics.length, 0, 'builder constants transpile');
const exportsObject = {};
vm.runInNewContext(output.outputText, {exports:exportsObject}, {filename:file});

const bits = value => { const b = Buffer.alloc(8); b.writeDoubleBE(value); return b.toString('hex'); };
const cases = new Set(['', ' ', '0', '-0', '-0.00001', '-0.00005', '-0.00015', '0.00005', '0.03', '1e308', '-1e308', '1e400', '-1e400', '-1e-400', 'Infinity', '-Infinity', 'NaN', '0x10', '1_000', '12abc', '.', '+', '1e+', '1.25e2tail']);
for (const whitespace of [' ', '\t', '\r\n', '\uFEFF', '\u00A0', '\u2028', '\u0085', '\u200B']) {
  for (const text of ['0.03', '-0', 'Infinity', 'bad', '1e308']) { cases.add(whitespace + text); cases.add(text + whitespace); }
}
for (let i = -500; i <= 500; ++i) {
  cases.add(String((i + 0.5) / 10000));
  for (const exponent of [-400, -324, -308, -8, 0, 8, 304, 308, 400]) cases.add(`${i}e${exponent}`);
}
for (const text of ['é', '😀', '漢字', '\uFEFF']) { cases.add(text); cases.add('0.03' + text); }
const rows = [...cases].map(text => {
  try { return {text, bits: bits(exportsObject.grvtBuilderFeeRateToRaw(text))}; }
  catch (error) { return {text, error: {name:error.name, message:error.message}}; }
});
// JSON escapes containing lone surrogates are valid JSON but outside Rust str.
const refused = ['\uD800', '\uDC00', '0.03\uD800'].map(text => {
  try { return {wire: JSON.stringify(text), bits: bits(exportsObject.grvtBuilderFeeRateToRaw(text))}; }
  catch (error) { return {wire: JSON.stringify(text), error: {name:error.name,messageUnits:error.message.split('').map(c => c.charCodeAt(0))}}; }
});
process.stdout.write(JSON.stringify({addresses:[false,true].map(testnet => [testnet,exportsObject.getGrvtBuilderAddress(testnet)]), rows, refused}));
