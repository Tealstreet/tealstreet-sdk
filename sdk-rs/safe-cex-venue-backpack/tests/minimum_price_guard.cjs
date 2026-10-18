// Execute the actual portable pricing guard on the real native binding's output.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const root = path.resolve(__dirname, '../../..');
const pricingPath = path.join(root, 'packages/script-api-core/src/order/orderPricing.ts');
const source = fs.readFileSync(pricingPath, 'utf8');
const ast = ts.createSourceFile(pricingPath, source, ts.ScriptTarget.Latest, true);
const guards = ast.statements.flatMap(statement => ts.isVariableStatement(statement)
  ? statement.declarationList.declarations.filter(declaration => declaration.name.getText(ast) === 'assertMinimumPrice')
  : []);
assert.equal(guards.length, 1, 'source guard must exist exactly once');
const errorSource = fs.readFileSync(path.join(root, 'packages/script-api-core/src/order/ScriptValidationError.ts'), 'utf8');
const moduleSource = `${errorSource}\nexport const assertMinimumPrice = ${guards[0].initializer.getText(ast)};`;
const compiled = ts.transpileModule(moduleSource, {
  reportDiagnostics: true,
  compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 },
});
assert.equal(compiled.diagnostics.length, 0);
const exportsObject = {};
vm.runInNewContext(compiled.outputText, { exports: exportsObject });
const markets = JSON.parse(fs.readFileSync(0, 'utf8'));
assert.equal(markets.length, 2);
for (const [index, min] of [5.25, 0.5].entries()) {
  const market = markets[index];
  assert.throws(() => exportsObject.assertMinimumPrice(min - 0.25, market),
    { name: 'ScriptValidationError', message: `price ${min - 0.25} is below the minimum ${min} for '${market.symbol}'` });
  assert.doesNotThrow(() => exportsObject.assertMinimumPrice(min, market));
  assert.doesNotThrow(() => exportsObject.assertMinimumPrice(min + 0.25, market));
}
