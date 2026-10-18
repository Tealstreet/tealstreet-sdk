const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH ?? 'typescript');
const file = path.resolve(__dirname, '../../../packages/safe-cex/src/exchanges/apexomni/apexomni.exchange.ts');
const ast = ts.createSourceFile(file, fs.readFileSync(file, 'utf8'), ts.ScriptTarget.Latest, true);
const functions = ast.statements.filter(node => ts.isFunctionDeclaration(node) && node.name?.text === 'd8');
assert.equal(functions.length, 1, 'unique actual d8 source');
const declarations = functions[0].body.statements.filter(ts.isVariableStatement)
  .flatMap(node => [...node.declarationList.declarations]);
const picks = declarations.filter(node => node.name.getText(ast) === 'l');
assert.equal(picks.length, 1, 'unique first character source expression');
assert.ok(ts.isElementAccessExpression(picks[0].initializer));
const expression = picks[0].initializer.getText(ast);
const fixtures = JSON.parse(fs.readFileSync(path.resolve(__dirname, '../tests/support/accepted_scalar_helper_fixtures.json'), 'utf8'));
const cases = fixtures['exchange_utils::character_at'];
assert.equal(cases.length, 11);
for (const row of cases) {
  const index = row.input.index === 'inf' ? Infinity : Number(row.input.index);
  const value = vm.runInNewContext(expression, {
    i: row.input.alphabet, c: 1, Math: { floor: Math.floor, random: () => index },
  });
  assert.deepEqual(value ?? null, row.output, `actual d8 character expression: ${JSON.stringify(row.input)}`);
  assert.match(row.input.alphabet, /^[\x00-\x7f]*$/, 'source-serving character fixtures must stay in the ASCII alphabet domain');
}
console.log('Actual d8 character expression: 11 ASCII-domain expectations pass.');
