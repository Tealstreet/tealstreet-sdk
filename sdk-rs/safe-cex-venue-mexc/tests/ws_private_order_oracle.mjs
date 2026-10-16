import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import {load,root,ts} from './source.mjs';
const filename='packages/safe-cex/src/exchanges/mexc/mexc.ws-private.ts';
const source=fs.readFileSync(`${root}/${filename}`,'utf8');
const ast=ts.createSourceFile(filename,source,ts.ScriptTarget.Latest,true);
let initializer;
const find=node=>{
  if(ts.isPropertyDeclaration(node) && node.name.getText(ast)==='handleOrder') initializer=node.initializer;
  ts.forEachChild(node,find);
};
find(ast);
assert(initializer,'MexcPrivateWebsocket#handleOrder was not found');
let functionText=initializer.getText(ast);
if(process.env.MEXC_WS_PRIVATE_ORDER_CONTROL==='rejection-state') {
  const changed=functionText.replace('state === 4 && errorCode > 0','state === 3 && errorCode > 0');
  assert.notEqual(changed,functionText,'named in-memory order rejection mutation did not apply');
  functionText=changed;
}
const js=ts.transpileModule(`class Receiver { constructor(store,parent){this.store=store;this.parent=parent;} handleOrder=${functionText}; } exports.create=(store,parent)=>new Receiver(store,parent);`,{
  compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS},
}).outputText;
const utils=load('packages/safe-cex/src/exchanges/mexc/mexc.utils.ts');
const module={exports:{}};
const fromMexcSymbol=utils.fromMexcSymbol,normalizeOrder=utils.normalizeOrder,
  toNumber=utils.toNumber,toIntegerString=utils.toIntegerString;
const OrderStatus={Closed:'closed',Canceled:'canceled'};
vm.runInNewContext(`(function(exports,fromMexcSymbol,normalizeOrder,toNumber,toIntegerString,OrderStatus){${js}\n})(module.exports,fromMexcSymbol,normalizeOrder,toNumber,toIntegerString,OrderStatus)`,{
  module,fromMexcSymbol,normalizeOrder,toNumber,toIntegerString,OrderStatus,
});
const events=[],rejected=[];
const store={marketsBySymbol:{},addOrUpdateOrders:orders=>events.push(['orders',...orders.map(order=>order.id)]),
  updateOrderStatus:({id},status)=>events.push(['status',id,status])};
const parent={noteRejectedRegularOrder:(id,code)=>rejected.push([id,code])};
module.exports.create(store,parent).handleOrder({data:[{orderId:'123',symbol:'BTC_USDT',side:1,state:4,errorCode:510,dealVol:0,vol:2,price:10}]});
process.stdout.write(JSON.stringify({events,rejected}));
