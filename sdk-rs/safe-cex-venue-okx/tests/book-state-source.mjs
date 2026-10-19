import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import {ts} from '../../safe-cex-store/tests/source_loader.mjs';
const url = new URL('../../../packages/safe-cex/src/exchanges/okx/okx.ws-public.ts', import.meta.url);
const ast = ts.createSourceFile(url.pathname, fs.readFileSync(url, 'utf8'), ts.ScriptTarget.Latest, true);
const owner = ast.statements.find(node => ts.isClassDeclaration(node) && node.name.text === 'OKXPublicWebsocket');
const names = ['acceptOrderBookSequence', 'recoverOrderBook'];
const members = names.map(name => {
  const member = owner.members.find(node => node.name?.getText(ast) === name);
  assert.ok(member, name);
  return member.getText(ast);
});
const module = {exports:{}};
const code = ts.transpileModule('export class Selected {' + members.join('\n') + '}', {
  compilerOptions:{target:ts.ScriptTarget.ES2022, module:ts.ModuleKind.CommonJS},
}).outputText;
vm.runInThisContext('(function(module,exports){'+code+'\n})')(module,module.exports);
const frame = (action, seqId, prevSeqId) => ({action, data:[{seqId,prevSeqId}]});
function scene() {
  return Object.assign(new module.exports.Selected(), {
    orderBookSequenceByHandler:new Map(), recoveringOrderBooks:new Set(), topics:{}, isConnected:false, isDisposed:false,
  });
}
const original = {opaque:true};
const poison = {toString(){throw original;}};
const snapshot = scene();
snapshot.orderBookSequenceByHandler.set('unrelated', poison);
Object.defineProperty(snapshot.topics, 'unrelated', {enumerable:true,get(){throw original;}});
assert.equal(snapshot.acceptOrderBookSequence('books.BTC',frame('snapshot','10','')),true);
assert.equal(snapshot.orderBookSequenceByHandler.get('unrelated'),poison);
assert.equal(snapshot.orderBookSequenceByHandler.get('books.BTC'),'10');
const recovery = scene();
recovery.orderBookSequenceByHandler.set('books.BTC','10');
Object.defineProperty(recovery.topics, 'books.BTC', {enumerable:true,get(){throw original;}});
assert.throws(()=>recovery.recoverOrderBook('books.BTC'), error=>error===original);
assert.equal(recovery.orderBookSequenceByHandler.has('books.BTC'),false);
assert.equal(recovery.recoveringOrderBooks.has('books.BTC'),true);
const live = scene(), calls=[];
live.orderBookSequenceByHandler = {set(...args){calls.push({receiver:this,args});}};
assert.equal(live.acceptOrderBookSequence('books.BTC',frame('snapshot','10','')),true);
assert.equal(calls.length,1);
assert.equal(calls[0].receiver,live.orderBookSequenceByHandler);
assert.deepEqual(calls[0].args,['books.BTC','10']);
const repeated = scene();
let reads = 0;
const row = {get seqId(){return ++reads === 1 ? '10' : '11';}};
assert.equal(repeated.acceptOrderBookSequence('books.BTC',{action:'snapshot',data:[row]}),true);
assert.equal(reads,2);
assert.equal(repeated.orderBookSequenceByHandler.get('books.BTC'),'11');
process.stdout.write('4 maintained source state observations PASS\n');
