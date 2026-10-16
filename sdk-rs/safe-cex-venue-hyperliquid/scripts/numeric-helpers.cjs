const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const assert = require('node:assert/strict');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const file = path.resolve(__dirname, '../../../packages/safe-cex/src/exchanges/hyperliquid/hyperliquid.exchange.ts');
const source = ts.createSourceFile(file, fs.readFileSync(file, 'utf8'), ts.ScriptTarget.Latest, true);
const names = ['parseFiniteNumber', 'calculatePriceChangePercentage', 'isFinitePositiveNumber'];
const declarations = source.statements.filter(node => ts.isVariableStatement(node) && node.declarationList.declarations.some(d => names.includes(d.name.text)));
assert.equal(declarations.length, names.length, 'actual exchange numeric helpers required');
const printer = ts.createPrinter();
let code = declarations.map(node => printer.printNode(ts.EmitHint.Unspecified, node, source)).join('\n');
if (process.argv.includes('--control')) code = code.replace('parseFloat(String(value ?? \'\'))', 'Number(value)');
const out = {};
vm.runInNewContext(ts.transpileModule(code, {compilerOptions: {target: ts.ScriptTarget.ES2022}}).outputText + names.map(name => `\nout.${name} = ${name};`).join(''), {out});
const cases = [];
function capture(operation, raw, representationRefusal = false) {
  const row = {operation, raw};
  if (representationRefusal) row.representationRefusal = true;
  try {
    const value = out[operation](...JSON.parse(raw));
    row.value = value;
    if (typeof value === 'number') row.numberString = String(value);
  } catch (error) { row.errorName = error.name; row.error = error.message; }
  cases.push(row);
}
for (const raw of ['null', 'false', 'true', '0', '-0', '1e400', '-1e400', '9007199254740993', '0.000001', '1e20', '"Infinity"', '"12abc"', '"0x10"', '""', '"1e+tail"', '"-.25x"', '"\\ufeff12tail"', '"\\u008512"', '"１２"', '"雪"', '"😀12"', '[]', '[12]', '[12,"34"]', '{}', '{"toString":null}', '{"toString":123,"valueOf":0}']) {
  capture('parseFiniteNumber', `[${raw}]`);
  capture('isFinitePositiveNumber', `[${raw}]`);
}
for (const raw of ['[110,100]', '[0,100]', '[10,0]', '[10,-1]', '[1e400,1]', '[1,1e400]', '[1e308,1e-308]', '[-1e308,1e-308]', '[-0,100]']) capture('calculatePriceChangePercentage', raw);
for (const raw of ['["\\ud800"]', '["\\udfff12"]']) capture('parseFiniteNumber', raw, true);
if (process.argv.includes('--refusals')) {
  cases.length = 0;
  for (const operation of ['parseFiniteNumber', 'isFinitePositiveNumber']) {
    for (const raw of ['["\\ud800"]', '["\\udfff12"]']) capture(operation, raw, true);
  }
}
const fixture = path.resolve(__dirname, '../tests/fixtures/' + (process.argv.includes('--refusals') ? 'numeric-string-refusals.json' : 'numeric-helpers.json'));
const text = JSON.stringify(cases, null, 2) + '\n';
if (process.argv.includes('--capture')) fs.writeFileSync(fixture, text, {flag: 'wx'});
else assert.equal(text, fs.readFileSync(fixture, 'utf8'), 'actual exchange numeric helper drift');
