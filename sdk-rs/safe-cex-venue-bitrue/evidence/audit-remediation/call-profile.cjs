const fs = require('node:fs');
const crypto = require('node:crypto');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const file = 'packages/safe-cex/src/exchanges/bitrue/bitrue.exchange.ts';
const source = fs.readFileSync(file, 'utf8');
const tree = ts.createSourceFile(file, source, ts.ScriptTarget.Latest, true);
const wanted = new Set(['isSuccessCode', 'parseMaxLeverageFromErrorMessage']);
const calls = [];
function visit(node) {
 if (ts.isCallExpression(node) && ts.isPropertyAccessExpression(node.expression) && node.expression.expression.kind === ts.SyntaxKind.ThisKeyword && wanted.has(node.expression.name.text)) {
  calls.push({symbol:node.expression.name.text, arguments:node.arguments.map(a=>a.getText(tree)), line:tree.getLineAndCharacterOfPosition(node.getStart(tree)).line+1});
 }
 ts.forEachChild(node, visit);
}
visit(tree);
const assert = require('node:assert/strict');
assert.equal(calls.filter(c=>c.symbol==='isSuccessCode').length,8);
assert(calls.filter(c=>c.symbol==='isSuccessCode').every(c=>c.arguments.length===1 && c.arguments[0]==='data?.code'));
assert.equal(calls.filter(c=>c.symbol==='parseMaxLeverageFromErrorMessage').length,1);
assert.deepEqual(calls.find(c=>c.symbol==='parseMaxLeverageFromErrorMessage').arguments,['errorMessage']);
console.log(JSON.stringify({source:file,source_sha256:crypto.createHash('sha256').update(source).digest('hex'),expected_success_calls:8,expected_error_calls:1,calls,stop:calls.at(-1),scope:'Direct this.property calls in this one file only; indirect calls and dynamic values not classified'},null,2));
