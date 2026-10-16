import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import {load,root,ts} from './source.mjs';
const filename='packages/safe-cex/src/exchanges/mexc/mexc.ws-private.ts';
const source=fs.readFileSync(`${root}/${filename}`,'utf8');
const ast=ts.createSourceFile(filename,source,ts.ScriptTarget.Latest,true);
const initializers={};
const find=node=>{
  if(ts.isPropertyDeclaration(node)) {
    const name=node.name.getText(ast);
    if(['applyStopLikeFrame','handlePlanOrder','handleStopOrder'].includes(name)) initializers[name]=node.initializer;
  }
  ts.forEachChild(node,find);
};
find(ast);
for(const name of ['applyStopLikeFrame','handlePlanOrder','handleStopOrder']) assert(initializers[name],`${name} was not found`);
let applyText=initializers.applyStopLikeFrame.getText(ast);
if(process.env.MEXC_WS_PRIVATE_STOP_CONTROL==='disable-cancel-shield') {
  const changed=applyText.replace('this.parent.isRecentlyCanceledStopLikeOrderId(order.id)','false');
  assert.notEqual(changed,applyText,'named in-memory stop shield mutation did not apply');
  applyText=changed;
}
const js=ts.transpileModule(`class Receiver { constructor(store,parent){this.store=store;this.parent=parent;} applyStopLikeFrame=${applyText}; handlePlanOrder=${initializers.handlePlanOrder.getText(ast)}; handleStopOrder=${initializers.handleStopOrder.getText(ast)}; } exports.create=(store,parent)=>new Receiver(store,parent);`,{
  compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS},
}).outputText;
const utils=load('packages/safe-cex/src/exchanges/mexc/mexc.utils.ts');
const module={exports:{}};
const fromMexcSymbol=utils.fromMexcSymbol,normalizePlanOrder=utils.normalizePlanOrder,
  normalizeStopOrder=utils.normalizeStopOrder,toIntegerString=utils.toIntegerString;
const OrderStatus={Open:'open',Closed:'closed',Canceled:'canceled'};
vm.runInNewContext(`(function(exports,fromMexcSymbol,normalizePlanOrder,normalizeStopOrder,toIntegerString,OrderStatus){${js}\n})(module.exports,fromMexcSymbol,normalizePlanOrder,normalizeStopOrder,toIntegerString,OrderStatus)`,{
  module,fromMexcSymbol,normalizePlanOrder,normalizeStopOrder,toIntegerString,OrderStatus,
});
const seen=[],writes=[];
const store={marketsBySymbol:{},addOrUpdateOrders:orders=>writes.push(...orders.map(order=>order.id))};
const parent={markStopLikeOrderSeen:id=>seen.push(id),isRecentlyCanceledStopLikeOrderId:id=>id==='cancel-1'};
const receiver=module.exports.create(store,parent);
receiver.handlePlanOrder({data:[
  {id:'plan-1',symbol:'BTC_USDT',side:2,state:1,vol:3,price:10,triggerPrice:9,triggerType:4},
  {id:'cancel-1',symbol:'BTC_USDT',side:2,state:1,vol:3,price:10,triggerPrice:9,triggerType:4},
]});
receiver.handleStopOrder({data:[{id:'stop-1',symbol:'BTC_USDT',side:0,state:1,vol:4,stopLossPrice:9,positionType:2,triggerSide:3}]});
process.stdout.write(JSON.stringify({seen,orders:writes}));
