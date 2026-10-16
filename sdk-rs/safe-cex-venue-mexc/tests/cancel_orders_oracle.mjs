import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import vm from 'node:vm';
import {createRequire} from 'node:module';
import {load,root,ts} from './source.mjs';

const require=createRequire(path.join(path.resolve(process.env.SAFE_CEX_TYPESCRIPT_PATH),'../mexc-cancel.cjs'));
const filename=path.join(root,'packages/safe-cex/src/exchanges/mexc/mexc.exchange.ts');
const ast=ts.createSourceFile(filename,fs.readFileSync(filename,'utf8'),ts.ScriptTarget.Latest,true);
const names=['isStopOrder','isPlanOrder','isStopLikeOrder','assertCommonSuccess','assertRegularCancelSucceeded','noteRecentlyCanceledStopLikeOrders','cancelOrders'];
const selected=[];
for(const node of ast.statements){
 if(ts.isVariableStatement(node)&&node.declarationList.declarations.some(d=>['extractData','extractRows'].includes(d.name.getText(ast))))selected.push(node.getText(ast));
 if(ts.isClassDeclaration(node)&&node.name?.text==='MexcExchange')for(const member of node.members)if(names.includes(member.name?.getText(ast)))selected.push(`this.${member.name.getText(ast)}=${member.initializer.getText(ast)};`);
}
let source=selected.join('\n');
if(process.env.MEXC_TS_CONTROL==='cancel-stop-kind'){
 const anchor='const stopOrders = orders.filter((order) => this.isStopOrder(order));';
 if(!source.includes(anchor))throw Error('missing cancel stop classification mutation anchor');
 source=source.replace(anchor,'const stopOrders = orders.filter((order) => this.isStopLikeOrder(order));');
}
const js=ts.transpileModule(source,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;
const types=load('packages/safe-cex/src/types.ts');
const mexcTypes=load('packages/safe-cex/src/exchanges/mexc/mexc.types.ts');
const utils=load('packages/safe-cex/src/exchanges/mexc/mexc.utils.ts');
const {DefaultStore}=load('packages/safe-cex/src/store/store.base.ts');
const virtualClock={getCurrentTimeMs:()=>123456};
const cases=[
 {name:'empty input is a no-op',orders:[],responses:[]},
 {name:'all order classes cancel in source order',orders:[
  {id:'regular__buy',symbol:'BTCUSDT',type:'limit',amount:3,remaining:2},
  {id:'plan__sell',symbol:'ETHUSDT',type:'take_profit_market',amount:1,remaining:1,info:{__mexcPlanOrder:true}},
  {id:'stop__long',symbol:'BTCUSDT',type:'stop_market',amount:0,remaining:0}
 ],responses:[{code:0},{code:0},{code:0}]},
 {name:'plan cancellation failure stops later writes',orders:[
  {id:'plan__sell',symbol:'ETHUSDT',type:'take_profit_market',amount:1,remaining:1,info:{__mexcPlanOrder:true}},
  {id:'regular',symbol:'BTCUSDT',type:'limit',amount:1,remaining:1}
 ],responses:[{success:false,message:'plan denied',code:511}]},
 {name:'regular per-order errors preserve source rejection',orders:[
  {id:'regular',symbol:'BTCUSDT',type:'limit',amount:1,remaining:1}
 ],responses:[{code:0,data:[{orderId:'regular',errorCode:9,errorMsg:'already closed'}]}]},
 {name:'notification failure follows committed status update',orders:[
  {id:'plan__sell',symbol:'ETHUSDT',type:'take_profit_market',amount:1,remaining:1,info:{__mexcPlanOrder:true}}
 ],responses:[{code:0}],throwSubscriber:true},
 {name:'batches preserve ids and stop at fifty',orders:Array.from({length:51},(_,i)=>({id:`regular-${i}__side`,symbol:'BTCUSDT',type:'limit',amount:1,remaining:1})),responses:[{code:0},{code:0}]}
];
function makeOrder(raw){return {status:types.OrderStatus.Open,info:null,...raw};}
if(process.argv.includes('--transport-finding')) {
 const order=makeOrder({id:'regular__buy',symbol:'BTCUSDT',type:'limit',amount:1,remaining:1});
 const store=new DefaultStore();store.update({orders:[order]},false);
 const error=Object.assign(new Error('transport failure'),{response:{data:{message:'response detail'}},code:'ERR_NETWORK'});
 const calls=[];
 const context={...types,...mexcTypes,...utils,virtualClock,store,
  recentlyCanceledStopLikeOrderIds:new Map(),recentlyPlacedStopLikeOrderIds:new Map(),
  chunk:require('lodash/chunk'),xhr:{post:async(url,data)=>{calls.push({url,data});throw error;}}};
 vm.runInContext(js,vm.createContext(context),{filename});
 let actual;
 const pending=context.cancelOrders([order]);const prefixCalls=calls.length;assert.equal(prefixCalls,1,'SOURCE cancellation eager POST prefix');
 try{await pending;assert.fail('MEXC transport rejection required');}catch(caught){actual=caught;}
 assert.strictEqual(actual,error,'MEXC cancel transport error identity');
 process.stdout.write(JSON.stringify({captureArchitecture:process.arch,captureNodeMajor:Number(process.versions.node.split('.')[0]),message:actual.message,name:actual.name,code:actual.code,response:actual.response,prefixCalls,calls,status:store.orders[0].status}));
 process.exit(0);
}
const results=[];
for(const scenario of cases){
 const store=new DefaultStore();
 store.update({orders:scenario.orders.map(makeOrder)},false);
 const calls=[],responses=[...scenario.responses],canceled=new Map(),placed=new Map([['plan',1],['stop',1]]);
 let subscriberCalls=[];
 if(scenario.throwSubscriber){
  store.subscribe(()=>{subscriberCalls.push('first');throw Error('subscriber failed');});
  store.subscribe(()=>subscriberCalls.push('second'));
 }
 const xhr={post:async(url,data)=>{calls.push({url,data});return {data:responses.shift()??{code:0}};}};
 const context={
  ...types,...mexcTypes,...utils,virtualClock,store,xhr,canceled,placed,
  recentlyCanceledStopLikeOrderIds:canceled,recentlyPlacedStopLikeOrderIds:placed,
  chunk:require('lodash/chunk'),
  isStopOrder:undefined,isPlanOrder:undefined,isStopLikeOrder:undefined,
  assertCommonSuccess:undefined,assertRegularCancelSucceeded:undefined,noteRecentlyCanceledStopLikeOrders:undefined,
 };
 vm.runInContext(js,vm.createContext(context),{filename});
 let error;
 try{await context.cancelOrders(scenario.orders.map(makeOrder));}catch(e){error={name:e.name,message:e.message};}
 results.push({name:scenario.name,sourceOrders:scenario.orders,orders:scenario.orders.map(o=>o.id),responses:scenario.responses,calls,
  statuses:store.orders.map(o=>[o.id,o.status]),canceled:[...canceled],placed:[...placed],error,subscriberCalls});
}
if(process.env.MEXC_TS_CONTROL==='cancel-stop-kind'){
 assert.deepEqual(results[1].calls[0].data,[{stopPlanOrderId:'stop'}],'MEXC cancel stop-order payload identity');
}
process.stdout.write(JSON.stringify(results.map(item=>({captureArchitecture:process.arch,captureNodeMajor:Number(process.versions.node.split('.')[0]),...item}))));
