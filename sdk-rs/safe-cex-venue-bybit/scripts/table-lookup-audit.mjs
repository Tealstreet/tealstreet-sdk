// Inventory, not a conformance gate: inspect all element accesses before selecting
// static protocol/configuration tables. Dynamic stores/handler maps are separate.
import ts from 'typescript';
import {readdirSync, readFileSync} from 'node:fs';
const root = new URL('../../../packages/safe-cex/src/', import.meta.url);
const files = readdirSync(new URL('exchanges/bybit/', root))
  .filter(name => name.endsWith('.ts') && !name.includes('.test.'))
  .map(name => `exchanges/bybit/${name}`)
  .concat('journal/bybit.position-adapter.ts').sort();
const accesses = [];
for (const file of files) {
  const source = ts.createSourceFile(file, readFileSync(new URL(file, root), 'utf8'), ts.ScriptTarget.Latest, true);
  function visit(node) {
    if (ts.isElementAccessExpression(node)) {
      const base = node.expression.getText(source).replace(/\s+/g, ' ');
      const table = /\b[A-Z][A-Z_]+\b/.test(base.split('[')[0]) ||
        /^(inverseObj\(|rateLimitEntry\[|rateLimitEntry$|typeMap$|\{ 0:)/.test(base);
      let owner = node.parent;
      while (owner && !(ts.isMethodDeclaration(owner) || ts.isPropertyDeclaration(owner) ||
        (ts.isVariableDeclaration(owner) && owner.initializer &&
          (ts.isArrowFunction(owner.initializer) || ts.isFunctionExpression(owner.initializer))))) owner = owner.parent;
      accesses.push({file, line: source.getLineAndCharacterOfPosition(node.getStart(source)).line + 1,
        owner: owner?.name?.getText(source) ?? '(top-level)',
        expression: node.getText(source).replace(/\s+/g, ' '), table});
    }
    ts.forEachChild(node, visit);
  }
  visit(source);
}
console.log(JSON.stringify({files: files.length, elementAccesses: accesses.length,
  tables: accesses.filter(row => row.table),
  excluded: accesses.filter(row => !row.table)}, null, 2));
