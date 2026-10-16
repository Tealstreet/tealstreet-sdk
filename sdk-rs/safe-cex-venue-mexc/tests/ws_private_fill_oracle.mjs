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
let functionText=initializer.getText(ast);
if(process.env.MEXC_WS_PRIVATE_FILL_CONTROL==='fee-value') {
  const changed=functionText.replace('fee: fill.commission','fee: fill.price');
  assert.notEqual(changed,functionText,'named in-memory fill mutation did not apply');
  functionText=changed;
}
const js=ts.transpileModule(`class Receiver { constructor(store,parent){this.store=store;this.parent=parent;} handleFill=${functionText}; } exports.create=(store,parent)=>new Receiver(store,parent);`,{
  compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS},
}).outputText;
const utils=load('packages/safe-cex/src/exchanges/mexc/mexc.utils.ts');
const module={exports:{}};
const fromMexcSymbol=utils.fromMexcSymbol,normalizeExecution=utils.normalizeExecution;
const emitter={emit:(name,event)=>events.push([name,event])};
const events=[];
vm.runInNewContext(`(function(exports,fromMexcSymbol,normalizeExecution){${js}\n})(module.exports,fromMexcSymbol,normalizeExecution)`,{
  module,fromMexcSymbol,normalizeExecution,
});
const store={marketsBySymbol:{}};
const parent={emitter};
module.exports.create(store,parent).handleFill({data:[{
  tradeId:'fill-1',orderId:'order-1',symbol:'BTC_USDT',side:1,price:'12.5',vol:'2',fee:'0.25',
  feeCurrency:'USDT',profit:'1.5',createTime:1700000000000,role:2,
}]});
const event=events[0][1];
assert.equal(events[0][0],'fill');
process.stdout.write(JSON.stringify([event]));
