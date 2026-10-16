const assert = require('node:assert/strict');
const fs = require('node:fs');
const {root, ts} = require('../../safe-cex-types/scripts/accounts-source.cjs');
const source = fs.readFileSync(root + '/packages/safe-cex/src/crossTrade/types.ts', 'utf8');
const exportsObject = {};
new Function('exports', ts.transpileModule(source, {compilerOptions: {
  target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS,
}}).outputText)(exportsObject);
assert.equal(exportsObject.defaultCrossTradeConfig.masterWeight, 1);
assert.equal(exportsObject.defaultCrossTradeCopier.weight, 1);
const cases = [
  ['1.5', '3ff8000000000000'], ['-0', '8000000000000000'],
  ['-0.0', '8000000000000000'], ['1e400', '7ff0000000000000'],
  ['-1e400', 'fff0000000000000'], ['-1e-999', '8000000000000000'],
  ['1e-999', '0000000000000000'], ['2', '4000000000000000'],
];
let count = 0;
for (const [token, expected] of cases) {
  const parsed = JSON.parse(`{"masterWeight":${token},"copiers":[{"weight":${token}}]}`);
  for (const value of [parsed.masterWeight, parsed.copiers[0].weight]) {
    const native = Buffer.alloc(8);
    native.writeDoubleBE(value);
    assert.equal(native.toString('hex'), expected, 'source decoded Number bits: ' + token);
  }
  count += 1;
}
assert.equal(count, 8, 'independent source numeric-profile floor');
console.log('CROSS-NUMERIC source assertions: 8 profiles');
