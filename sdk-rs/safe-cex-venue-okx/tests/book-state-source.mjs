import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import {ts} from '../../safe-cex-store/tests/source_loader.mjs';
import {loadSource} from './source-loader.mjs';
const url = new URL('../../../packages/safe-cex/src/exchanges/okx/okx.ws-public.ts', import.meta.url);
const ast = ts.createSourceFile(url.pathname, fs.readFileSync(url, 'utf8'), ts.ScriptTarget.Latest, true);
const owner = ast.statements.find(node => ts.isClassDeclaration(node) && node.name.text === 'OKXPublicWebsocket');
const names = ['acceptOrderBookSequence', 'recoverOrderBook', 'onMessage', 'dispatchMessageHandler'];
const members = names.map(name => {
  const member = owner.members.find(node => node.name?.getText(ast) === name);
  assert.ok(member, name);
  return member.getText(ast);
});
const module = {exports:{}};
const code = ts.transpileModule('export class Selected {' + members.join('\n') + '}', {
  compilerOptions:{target:ts.ScriptTarget.ES2022, module:ts.ModuleKind.CommonJS},
}).outputText;
const {bigIntAsStringJsonParse} = loadSource(new URL('../../../packages/json-parse/src/jsonParse.ts', import.meta.url));
vm.runInThisContext('(function(module,exports,bigIntAsStringJsonParse){'+code+'\n})')(module,module.exports,bigIntAsStringJsonParse);
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
const aliased = scene(), sent = [];
aliased.isConnected = true;
const topic = {channel:'books', instId:'BTC', extra:'before'};
aliased.topics['books.BTC'] = [topic];
aliased.send = function(wire) {
  assert.equal(this,aliased);
  sent.push(wire);
  topic.extra = 'after';
  aliased.topics['books.BTC'] = [{instId:'replacement'}];
};
aliased.recoverOrderBook('books.BTC');
assert.deepEqual(sent,[
  '{"op":"unsubscribe","args":[{"channel":"books","instId":"BTC","extra":"before"}]}',
  '{"op":"subscribe","args":[{"channel":"books","instId":"BTC","extra":"after"}]}',
]);
const publicFrame = scene();
publicFrame.trackLastReceived = () => {};
publicFrame.directFallback = {onMessage(){}};
publicFrame.recordMessageHandlerStats = () => {};
Object.defineProperty(publicFrame.topics,'unrelated',{enumerable:true,get(){throw original;}});
let callbacks = 0;
const map = publicFrame.orderBookSequenceByHandler;
publicFrame.messageHandlers = {'books.BTC':[()=>{
  assert.equal(map.get('books.BTC'),'10'); callbacks++;
}]};
publicFrame.onMessage({data:JSON.stringify({arg:{channel:'books',instId:'BTC'},action:'snapshot',data:[{seqId:'10'}]})});
assert.equal(callbacks,1);
assert.equal(publicFrame.orderBookSequenceByHandler,map);
process.stdout.write('6 maintained source state observations PASS\n');
