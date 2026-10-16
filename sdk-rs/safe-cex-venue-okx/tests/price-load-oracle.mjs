import fs from 'node:fs';
import vm from 'node:vm';
import assert from 'node:assert/strict';
import {ts} from '../../safe-cex-store/tests/source_loader.mjs';
const url = new URL('../../../packages/safe-cex/src/exchanges/okx/okx.exchange.ts', import.meta.url);
const ast = ts.createSourceFile(url.pathname,fs.readFileSync(url,'utf8'),ts.ScriptTarget.Latest,true);
const owner = ast.statements.find(n => ts.isClassDeclaration(n) && n.name.text === 'OKXExchange');
const names = ['markPriceMarketIds','indexPriceMarketIds','markPriceLoaded','indexPriceLoaded','hasMarkPriceLoaded','hasIndexPriceLoaded'];
const selected = names.map(name => {
  const member = owner.members.find(n => n.name?.getText(ast) === name);
  assert.ok(member,'maintained price-load member '+name);
  return member.getText(ast);
});
const code = ts.transpileModule('export class Selected {'+selected.join('\n')+'}',{
  compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS},
}).outputText;
const module = {exports:{}};
vm.runInThisContext('(function(module,exports){'+code+'\n})')(module,module.exports);
const cases = JSON.parse(fs.readFileSync(0,'utf8'));
assert.equal(cases.length,5,'five declared string profiles');
const receiver = new module.exports.Selected();
const captures = [];
for(const units of cases) {
  const id = String.fromCharCode(...units);
  const rows=[];
  const read = () => rows.push([receiver.hasMarkPriceLoaded(id),receiver.hasIndexPriceLoaded(id)]);
  read();receiver.markPriceLoaded(id);read();receiver.markPriceLoaded(id);read();receiver.indexPriceLoaded(id);read();
  assert.equal(id.length,units.length,'materialized code-unit length');
  assert.deepEqual(Array.from({length:id.length},(_,i)=>id.charCodeAt(i)),units,'materialized units before serialization');
  captures.push(rows);
}
assert.equal(receiver.markPriceMarketIds.size,5,'five independent mark entries');
assert.equal(receiver.indexPriceMarketIds.size,5,'five independent index entries');
process.stdout.write(JSON.stringify(captures));
