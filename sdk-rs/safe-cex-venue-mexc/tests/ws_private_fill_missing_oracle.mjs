import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import {load,root,ts} from './source.mjs';
const filename='packages/safe-cex/src/exchanges/mexc/mexc.ws-private.ts';
const source=fs.readFileSync(`${root}/${filename}`,'utf8');
const ast=ts.createSourceFile(filename,source,ts.ScriptTarget.Latest,true);
let initializer;
const find=node=>{
  if(ts.isPropertyDeclaration(node) && node.name.getText(ast)==='handleFill') initializer=node.initializer;
  ts.forEachChild(node,find);
};
find(ast);
assert(initializer,'MexcPrivateWebsocket#handleFill was not found');
const js=ts.transpileModule(`class Receiver { constructor(store,parent){this.store=store;this.parent=parent;} handleFill=${initializer.getText(ast)}; } exports.create=(store,parent)=>new Receiver(store,parent);`,{
  compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS},
}).outputText;
const utils=load('packages/safe-cex/src/exchanges/mexc/mexc.utils.ts');
const module={exports:{}};
const fromMexcSymbol=utils.fromMexcSymbol,normalizeExecution=utils.normalizeExecution;
vm.runInNewContext(`(function(exports,fromMexcSymbol,normalizeExecution){${js}\n})(module.exports,fromMexcSymbol,normalizeExecution)`,{
  module,fromMexcSymbol,normalizeExecution,
});
const events=[];
const store={marketsBySymbol:{}};
const parent={emitter:{emit:(name,event)=>events.push([name,event])}};
module.exports.create(store,parent).handleFill({data:[{
  tradeId:'fill-2',symbol:'BTC_USDT',side:1,price:12.5,vol:2,createTime:1700000000000,
}]});
const event=events[0][1];
assert.equal(event.orderId,'','FILL Source empty orderId before JSON');
assert.equal(event.fee,0,'FILL Source fee before JSON');
assert.equal(event.realizedPnl,0,'FILL Source realizedPnl before JSON');
const serialized=JSON.parse(JSON.stringify(event));
const ownProperties=Object.fromEntries(['orderId','fee','realizedPnl'].map(key=>[
  key,Object.prototype.hasOwnProperty.call(event,key),
]));
assert.deepEqual(ownProperties,{orderId:true,fee:true,realizedPnl:true});
assert.equal(serialized.orderId,'');
assert.equal(serialized.fee,0);
assert.equal(serialized.realizedPnl,0);
process.stdout.write(JSON.stringify({ownProperties,serialized}));
