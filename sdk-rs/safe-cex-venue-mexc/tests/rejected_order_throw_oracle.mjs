import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import vm from 'node:vm';
import {load,root,ts} from './source.mjs';

const filename=path.join(root,'packages/safe-cex/src/exchanges/mexc/mexc.exchange.ts');
const source=fs.readFileSync(filename,'utf8');
const ast=ts.createSourceFile(filename,source,ts.ScriptTarget.Latest,true);
let initializer,reasons;
for(const statement of ast.statements){
 if(ts.isVariableStatement(statement))for(const declaration of statement.declarationList.declarations){
  if(declaration.name.getText(ast)==='MEXC_ORDER_CANCEL_REASONS')reasons=declaration.initializer;
 }
 if(ts.isClassDeclaration(statement)&&statement.name?.text==='MexcExchange')for(const member of statement.members){
  if(member.name?.getText(ast)==='noteRejectedRegularOrder')initializer=member.initializer;
 }
}
assert(initializer,'MexcExchange.noteRejectedRegularOrder was not found');
assert(reasons,'MEXC_ORDER_CANCEL_REASONS was not found');
let method=initializer.getText(ast);
if(process.env.MEXC_REJECTED_THROW_TS_CONTROL==='suppress-update'){
 const before='this.store.updateOrderStatus({ id: cleanId }, OrderStatus.Canceled);';
 assert(method.includes(before),'missing Store update mutation anchor');
 method=method.replace(before,'');
}
const js=ts.transpileModule(`const MEXC_ORDER_CANCEL_REASONS=${reasons.getText(ast)}; class Receiver {
 constructor(test){this.store=test.store;this.recentlyRejectedRegularOrderIds=new Map();this.errors=[];}
 emitAndLogError(message){this.errors.push(message);}
 noteRejectedRegularOrder=${method};
} exports.run=async test=>{const receiver=new Receiver(test);let rejected=false,message='';try{receiver.noteRejectedRegularOrder(test.orderId,test.errorCode);}catch(error){rejected=true;message=error.message;}return {rejected,message,orders:receiver.store.orders.map(({id,status})=>({id,status})),shield:[...receiver.recentlyRejectedRegularOrderIds],subscriberCalls:test.subscriberCalls,errors:receiver.errors};};`,{
 compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS},
}).outputText;
const module={exports:{}};
vm.runInNewContext(`(function(exports){${js}\n})(module.exports)`,{
 module,Map,OrderStatus:load('packages/safe-cex/src/types.ts').OrderStatus,
 virtualClock:{getCurrentTimeMs:()=>123.25},
});
const {DefaultStore}=load('packages/safe-cex/src/store/store.base.ts');
const store=new DefaultStore();
store.update({orders:[{id:'reject-id',symbol:'BTCUSDT',type:'limit',status:'open',lastUpdate:1}]});
const subscriberCalls=[];
store.subscribe(()=>{subscriberCalls.push('first');throw Error('subscriber failed');});
store.subscribe(()=>subscriberCalls.push('second'));
const result=await module.exports.run({store,orderId:'reject-id__legacy',errorCode:6,subscriberCalls});
process.stdout.write(JSON.stringify(result));
