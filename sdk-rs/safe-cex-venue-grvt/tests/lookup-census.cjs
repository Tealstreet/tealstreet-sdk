'use strict';
// Inventory only: classification is reviewed in LOOKUP_CENSUS.md, not inferred here.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const compiler = process.env.SAFE_CEX_TYPESCRIPT_PATH;
assert(compiler && path.isAbsolute(compiler), 'absolute isolated compiler required');
const ts = require(compiler);
const directory = path.resolve(__dirname, '../../../packages/safe-cex/src/exchanges/grvt');
let count = 0;
for (const file of fs.readdirSync(directory).filter(file => file.endsWith('.ts')).sort()) {
  const tree = ts.createSourceFile(file, fs.readFileSync(path.join(directory, file), 'utf8'), ts.ScriptTarget.Latest, true);
  function visit(node) {
    if (ts.isElementAccessExpression(node)) {
      count++;
      const line = tree.getLineAndCharacterOfPosition(node.getStart()).line + 1;
      process.stdout.write(`${file}:${line}\t${node.getText(tree)}\n`);
    }
    ts.forEachChild(node, visit);
  }
  visit(tree);
}
process.stderr.write(`${count} element-access expressions\n`);
