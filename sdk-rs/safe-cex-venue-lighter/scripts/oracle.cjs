// Evaluate live repository-owned protocol data; unknown runtime imports fail closed.
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const assert = require('node:assert/strict');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const root = path.resolve(__dirname, '../../..');
const read = p => fs.readFileSync(path.join(root, p), 'utf8');
function evaluate(source, imports = {}) {
  const exports = {};
  const js = ts.transpileModule(source, { compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 } }).outputText;
  vm.runInNewContext(js, { exports, require(name) {
    assert.ok(Object.hasOwn(imports, name), `Unexpected oracle import: ${name}`);
    return imports[name];
  } });
  return exports;
}
const domain = ts.createSourceFile('types.ts', read('packages/safe-cex/src/types.ts'), ts.ScriptTarget.Latest, true);
const enums = evaluate(domain.statements.filter(ts.isEnumDeclaration).map(n => n.getText(domain)).join('\n'));
const source = read('packages/safe-cex/src/exchanges/lighter/lighter.types.ts');
const lighter = evaluate(source, { '../../types': enums, '../../transport/registry': { withPublicCorsProxy() { throw Error('not part of table oracle'); } } });
const tables = Object.fromEntries(Object.entries(lighter).filter(([, value]) => typeof value !== 'function'));
const ast = ts.createSourceFile('lighter.types.ts', source, ts.ScriptTarget.Latest, true);
const aliases = Object.fromEntries(ast.statements.filter(ts.isTypeAliasDeclaration).map(n => [n.name.text, n]));
function literals(node) {
  assert.ok(ts.isUnionTypeNode(node), 'expected literal union');
  return node.types.map(n => { assert.ok(ts.isLiteralTypeNode(n) && ts.isStringLiteral(n.literal)); return n.literal.text; });
}
const profile = aliases.LighterDeploymentProfile.type;
assert.ok(ts.isTypeLiteralNode(profile));
const fields = profile.members.map(n => {
  assert.ok(ts.isPropertySignature(n) && !n.questionToken, 'profile fields must remain required');
  return [n.name.getText(ast), n.type.getText(ast)];
});
process.stdout.write(JSON.stringify({ tables, networks: literals(aliases.LighterDeploymentNetwork.type), quoteCurrencies: literals(profile.members.find(n => n.name.getText(ast) === 'quoteCurrency').type), fields }));
