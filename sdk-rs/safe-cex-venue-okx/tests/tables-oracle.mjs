import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import {ts} from '../../safe-cex-store/tests/source_loader.mjs';
const source = new URL('../../../packages/safe-cex/src/exchanges/okx/okx.types.ts', import.meta.url);
const domain = new URL('../../../packages/safe-cex/src/types.ts', import.meta.url);
const domainText = fs.readFileSync(domain, 'utf8');
const parsed = ts.createSourceFile('types.ts', domainText, ts.ScriptTarget.Latest, true);
const wanted = new Set(['OrderStatus', 'OrderType', 'OrderSide', 'PositionSide']);
const enums = parsed.statements.filter(node => ts.isEnumDeclaration(node) && wanted.has(node.name.text));
assert.equal(enums.length, wanted.size);
const enumSource = enums.map(node => node.getText(parsed)).join('\n');
// Evaluate maintained enum declarations; unrelated logging re-exports need no host.
const tablesSource = fs.readFileSync(source, 'utf8').replace(/^import .*?;\n/gm, '');
const code = ts.transpileModule(enumSource+'\n'+tablesSource, {compilerOptions: {
  target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS,
}}).outputText;
const module = {exports: {}};
vm.runInThisContext('(function(module,exports){'+code+'\n})', {filename: source.pathname})(module, module.exports);
const tables = module.exports;
for (const name of wanted) delete tables[name];
const tableAst = ts.createSourceFile('okx.types.ts', fs.readFileSync(source, 'utf8'), ts.ScriptTarget.Latest, true);
for (const name of ['OkxRegion', 'OkxInstrumentType']) {
  const alias = tableAst.statements.find(node => ts.isTypeAliasDeclaration(node) && node.name.text === name);
  assert.ok(alias && ts.isUnionTypeNode(alias.type), `missing literal union ${name}`);
  tables[name] = alias.type.types.map(node => {
    assert.ok(ts.isLiteralTypeNode(node) && ts.isStringLiteral(node.literal));
    return node.literal.text;
  });
}
assert.equal(Object.keys(tables.INTERVAL).length, 13);
assert.equal(Object.keys(tables.ENDPOINTS).length, 28);
assert.equal(tables.INTERVAL['1M'], '1Mutc');
process.stdout.write(JSON.stringify(tables));
