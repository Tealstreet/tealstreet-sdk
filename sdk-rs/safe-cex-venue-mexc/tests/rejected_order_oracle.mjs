import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import {root,ts} from './source.mjs';

const filename='packages/safe-cex/src/exchanges/mexc/mexc.exchange.ts';
const source=fs.readFileSync(`${root}/${filename}`,'utf8');
const ast=ts.createSourceFile(filename,source,ts.ScriptTarget.Latest,true);
let initializer, reasons;
const visit=node=>{
  if(ts.isPropertyDeclaration(node)&&node.name.getText(ast)==='noteRejectedRegularOrder')initializer=node.initializer;
  if(ts.isVariableDeclaration(node)&&node.name.getText(ast)==='MEXC_ORDER_CANCEL_REASONS')reasons=node.initializer;
  ts.forEachChild(node,visit);
};
visit(ast);
assert(initializer,'MexcExchange.noteRejectedRegularOrder source method was not found');
assert(reasons,'MEXC_ORDER_CANCEL_REASONS source table was not found');
let method=initializer.getText(ast);
if(process.env.MEXC_REJECTED_TS_CONTROL==='keep-suffix'){
  const before="const cleanId = orderId.split('__')[0];";
  assert(method.includes(before),'missing clean-id mutation anchor');
  method=method.replace(before,'const cleanId = orderId;');
}
const js=ts.transpileModule(`const MEXC_ORDER_CANCEL_REASONS=${reasons.getText(ast)}; class Receiver {
 constructor(test){this.recentlyRejectedRegularOrderIds=new Map();this.store=test.store;this.now=test.now;this.errors=[];}
 emitAndLogError(message){this.errors.push(message);}
 noteRejectedRegularOrder=${method};
} exports.run=test=>{const receiver=new Receiver(test);receiver.noteRejectedRegularOrder(test.orderId,test.errorCode);return {orders:receiver.store.orders,shield:[...receiver.recentlyRejectedRegularOrderIds],errors:receiver.errors};};`,{
 compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS},
}).outputText;
const module={exports:{}};
vm.runInNewContext(`(function(exports){${js}\n})(module.exports)`,{
 module,Map,virtualClock:{getCurrentTimeMs:()=>123.25},OrderStatus:{Canceled:'canceled'},
});
const cases=[
 {orderId:'',errorCode:6,store:{orders:[{id:'',status:'open'}]}},
 {orderId:'order__legacy',errorCode:6,store:{orders:[{id:'order',status:'open'}]}},
 {orderId:'already__suffix',errorCode:10,store:{orders:[{id:'already',status:'canceled'}]}},
 {orderId:'missing__suffix',errorCode:77,store:{orders:[]}},
 {orderId:'filled',errorCode:8,store:{orders:[{id:'filled',status:'closed'}]}},
];
for(const test of cases){
 const store={orders:test.store.orders.map(row=>({...row})),updateOrderStatus({id},status){const found=this.orders.find(row=>row.id===id);if(found)found.status=status;}};
 test.expected=module.exports.run({...test,store});
 test.expected.orders=store.orders;
}
process.stdout.write(JSON.stringify(cases));
