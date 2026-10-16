// Measurement only: enumerate every element access before classifying table reads.
const fs = require('node:fs');
const path = require('node:path');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const directory = path.resolve(__dirname, '../../../packages/safe-cex/src/exchanges/hyperliquid');
const accesses = [];
for (const filename of fs.readdirSync(directory).filter(name => name.endsWith('.ts')).sort()) {
  const file = ts.createSourceFile(filename, fs.readFileSync(path.join(directory, filename), 'utf8'), ts.ScriptTarget.Latest, true);
  function visit(node) {
    if (ts.isElementAccessExpression(node)) {
      accesses.push({file: filename, table: node.expression.getText(file), key: node.argumentExpression.getText(file), context: node.parent.getText(file)});
    }
    ts.forEachChild(node, visit);
  }
  visit(file);
}
const staticTables = accesses.filter(row => /^[A-Z][A-Z_]+$/.test(row.table));
console.log(JSON.stringify({elementAccessCount: accesses.length, staticTableCount: staticTables.length, staticTables, accesses}, null, 2));
