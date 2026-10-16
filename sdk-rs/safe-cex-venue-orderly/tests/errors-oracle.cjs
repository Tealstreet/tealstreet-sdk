const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const assert = require('node:assert/strict');
assert.ok(process.env.SAFE_CEX_TYPESCRIPT_PATH, 'SAFE_CEX_TYPESCRIPT_PATH required');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const file = path.resolve(__dirname, '../../../packages/safe-cex/src/exchanges/orderly/orderly.exchange.ts');
const text = fs.readFileSync(file, 'utf8');
const tree = ts.createSourceFile(file, text, ts.ScriptTarget.Latest, true);
const names = ['EXPECTED_ORDERLY_ORDER_REJECTION_PATTERNS', 'isExpectedOrderlyOrderRejection', 'getOrderlyOrderErrorLogOptions'];
// Select complete actual declarations, including the actual regex literals. No replacement bodies.
const pieces = [];
for (const name of names) {
  const declarations = tree.statements.filter(s => ts.isVariableStatement(s) && s.declarationList.declarations.some(d => ts.isIdentifier(d.name) && d.name.text === name));
  assert.equal(declarations.length, 1, `source declaration census/${name}`);
  pieces.push(declarations[0].getText(tree));
}
const source = pieces.join('\n') + '\nexport { isExpectedOrderlyOrderRejection, getOrderlyOrderErrorLogOptions };';
const code = ts.transpileModule(source, { compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS } }).outputText;
const mod = { exports: {} };
new vm.Script(`(function(module,exports){${code}\n})`, { filename: file }).runInThisContext()(mod, mod.exports);
const actual = mod.exports;
function capture(input) {
  const expected = actual.isExpectedOrderlyOrderRejection(input);
  const options = actual.getOrderlyOrderErrorLogOptions(input);
  assert.equal(typeof expected, 'boolean', 'source classification typeof');
  assert.ok(options === undefined || (Object.keys(options).length === 1 && options.suppressConsoleLog === true), 'source logging native shape');
  return { input: typeof input === 'string' ? input : null, expected, options: options ?? null };
}
const seeds = ['The order value should be greater or equal to 10.5', 'The margin will be insufficient after', 'The TP_SL side should be opposite to position side'];
const inputs = ['', 'unrelated rejection'];
for (const seed of seeds) {
  inputs.push(seed, seed.toUpperCase(), seed.toLowerCase(), '\n' + seed + '\n', 'prefix:' + seed + ':suffix');
  // Each position is independently perturbed, covering literal spelling and numeric wildcard behavior.
  for (let i = 0; i < seed.length; i++) inputs.push(seed.slice(0, i) + '#' + seed.slice(i + 1));
}
for (const suffix of ['', '.', '...', '0', '-1', '١', '１', '12abc', '\n1', ' 1']) inputs.push('The order value should be greater or equal to ' + suffix);
inputs.push('The margin will be inſufficient after', 'The margİn will be insufficient after', 'The margın will be insufficient after', 'The margin will be insufficient\nafter');
const corpus = [...inputs.map(capture), ...[undefined, null, 1, true, [], {}, new String(seeds[0])].map(capture)];
const surrogate = '\ud800' + seeds[1];
const boundary = { firstUnit: surrogate.charCodeAt(0), expected: actual.isExpectedOrderlyOrderRejection(surrogate), options: actual.getOrderlyOrderErrorLogOptions(surrogate) };
console.log(JSON.stringify({ runtime: { node: process.version, arch: process.arch }, corpus, boundary }));
