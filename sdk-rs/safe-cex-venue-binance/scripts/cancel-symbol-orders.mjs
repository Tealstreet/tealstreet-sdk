import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import vm from 'node:vm';
import {createRequire} from 'node:module';
const require=createRequire(import.meta.url),ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const runtimeRequire=process.env.SAFE_CEX_TYPESCRIPT_PATH?name=>require(path.resolve(process.env.SAFE_CEX_TYPESCRIPT_PATH,'..',name)):require;
const root=new URL('../../../packages/safe-cex/src/',import.meta.url);
const parse=file=>ts.createSourceFile(file,fs.readFileSync(new URL(file,root),'utf8'),ts.ScriptTarget.Latest,true);
function load(file,deps){
 const exports={};vm.runInNewContext(ts.transpileModule(fs.readFileSync(new URL(file,root),'utf8'),{compilerOptions:{module:ts.ModuleKind.CommonJS,esModuleInterop:true}}).outputText,{exports,require:name=>{assert.ok(name in deps,name);return deps[name];},structuredClone});return exports;
}
const types={};vm.runInNewContext(ts.transpileModule(parse('types.ts').statements.filter(ts.isEnumDeclaration).map(n=>n.getText()).join('\n'),{compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText,{exports:types});
const {DefaultStore}=load('store/store.base.ts',{'lodash/uniqBy':runtimeRequire('lodash/uniqBy'),'../types':types,'../utils/clone':load('utils/clone.ts',{}),'../utils/virtualClock':{virtualClock:{getCurrentTimeMs:()=>1000}}});
const row=(id,symbol,type)=>({id,symbol,type,status:types.OrderStatus.Open,side:types.OrderSide.Buy,price:12.3,amount:3.6,filled:0,remaining:3.6,reduceOnly:false,lastUpdate:100});
const initial=()=>[row('r','BTCUSDT',types.OrderType.Limit),row('a','BTCUSDT',types.OrderType.StopLoss),row('tp','BTCUSDT',types.OrderType.TakeProfit),row('trail','BTCUSDT',types.OrderType.TrailingStopLoss),row('conditional','BTCUSDT',types.OrderType.Conditional),row('market','BTCUSDT',types.OrderType.Market),row('other','ETHUSDT',types.OrderType.StopLoss)];
const results=[];
for(const [variant,klass,client] of [['binance','BinanceExchange','xhr'],['binancepm','BinancePMExchange','papiXhr'],['binancespot','BinanceSpotExchange','xhr']]){
 const ast=parse(`exchanges/binance/${variant}.exchange.ts`),decl=ast.statements.find(n=>ts.isClassDeclaration(n)&&n.name?.text===klass),method=decl.members.find(n=>n.name?.getText(ast)==='cancelSymbolOrders');assert.ok(method?.initializer);
 const constants=parse(`exchanges/binance/${variant}.types.ts`),endpoints=constants.statements.flatMap(n=>ts.isVariableStatement(n)?[...n.declarationList.declarations]:[]).find(n=>n.name.getText(constants)==='ENDPOINTS');assert.ok(endpoints);
 const classifier=variant==='binance'?decl.members.find(n=>n.name?.getText(ast)==='isAlgoOrderType'):null;
 const exports={};vm.runInNewContext(ts.transpileModule(`const ENDPOINTS=${endpoints.initializer.getText(constants)};exports.fn=function(symbol){return (${method.initializer.getText(ast)})(symbol);};${classifier?`exports.classify=${classifier.initializer.getText(ast)};`:''}`,{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText,{exports,OrderType:types.OrderType,OrderStatus:types.OrderStatus});
 const specs=[];
 if(variant==='binance'){
  for(const regular of ['ok','reject'])for(const algo of ['ok','reject'])for(const listenerFail of [0,1,2])for(const emitReject of [false,true])specs.push({regular,algo,listenerFail,emitReject,symbol:'BTCUSDT',mutation:'none',msg:'venue detail'});
  for(const fail of ['regular','algo'])for(const emitReject of [false,true])specs.push({regular:fail==='regular'?'sync-reject':'ok',algo:fail==='algo'?'sync-reject':'ok',listenerFail:0,emitReject,symbol:'BTCUSDT',mutation:'none',msg:'venue detail'});
 }else{
  for(const regular of ['ok','reject','sync-reject'])for(const listenerFail of [0,1])for(const emitReject of [false,true])specs.push({regular,algo:'unused',listenerFail,emitReject,symbol:'BTCUSDT',mutation:'none',msg:'venue detail'});
 }
 for(const msg of [undefined,'',null,false,0,[],{tag:'msg'},{toString:null}])specs.push({regular:'reject',algo:variant==='binance'?'ok':'unused',listenerFail:0,emitReject:false,symbol:'BTCUSDT',mutation:'none',...(msg===undefined?{}:{msg})});
 for(const symbol of ['','ETHUSDT','missing'])specs.push({regular:'ok',algo:variant==='binance'?'ok':'unused',listenerFail:0,emitReject:false,symbol,mutation:'none',msg:'venue detail'});
 for(const mutation of ['replace-before-resolve','rename-before-resolve','change-before-resolve',...(variant==='binance'?['rename-algo-in-first-status']:[])])specs.push({regular:'ok',algo:variant==='binance'?'ok':'unused',listenerFail:0,emitReject:false,symbol:'BTCUSDT',mutation,msg:'venue detail'});
 assert.equal(specs.length,variant==='binance'?43:26);
 for(let index=0;index<specs.length;index++){
  const spec=specs[index],store=new DefaultStore();store.update({orders:initial()});
  const initialUpdateId=store.updateIds.orders,trace=[],requests=[],updates=[],notifications=[],emissions=[];
  const sentinel=role=>Object.assign(RangeError(`fixture ${role}`),{code:`${role}-code`,response:{status:503,data:{...(spec.msg===undefined?{}:{msg:spec.msg})}}});
  const regularError=sentinel('regular'),algoError=sentinel('algo'),listenerError=sentinel('listener'),emitterError=sentinel('emitter');
  let stage='during-request',mutated=false;
  const updateStatus=store.updateOrdersStatus;
  store.updateOrdersStatus=(orders,status)=>{stage='status';updates.push({ids:orders.map(o=>o.id),status});trace.push(['update',orders.map(o=>o.id),status]);return updateStatus(orders,status);};
  const unsubscribe=store.subscribe(data=>{
   trace.push(['notify',stage]);notifications.push({stage,orders:data.orders.map(o=>({id:o.id,status:o.status}))});
   if(stage==='status'){
    const statusOrdinal=notifications.filter(n=>n.stage==='status').length;
    if(spec.listenerFail===statusOrdinal)throw listenerError;
    if(spec.mutation==='rename-algo-in-first-status'&&statusOrdinal===1)store.updateOrder({id:'a'},{id:'a-after-notify'});
   }
  });
  // A later subscriber proves first-listener throws abort propagation.
  const later=[];const unsubscribeLater=store.subscribe(()=>later.push(stage));
  const mutate=()=>{
   if(mutated)return;mutated=true;
   if(spec.mutation==='replace-before-resolve')store.update({orders:[row('new','BTCUSDT',types.OrderType.Limit),row('other','ETHUSDT',types.OrderType.StopLoss)]});
   else if(spec.mutation==='rename-before-resolve')store.updateOrder({id:'r'},{id:'r-renamed'});
   else if(spec.mutation==='change-before-resolve')store.updateOrder({id:'r'},{symbol:'ETHUSDT',type:types.OrderType.StopLoss});
  };
  const instance={store,isAlgoOrderType:exports.classify,emitAndLogError:message=>{trace.push(['emit',message]);emissions.push(message);if(spec.emitReject)throw emitterError;}};
  instance[client]={delete:(endpoint,options)=>{
   const role=endpoint.includes('algoOpenOrders')?'algo':'regular',mode=spec[role],error=role==='algo'?algoError:regularError;
   trace.push(['delete',role]);requests.push({client,endpoint,options,role});
   if(mode==='sync-reject')throw error;
   return Promise.resolve().then(()=>{mutate();if(mode==='reject')throw error;});
  }};
  let failure=null,returnedUndefined=false;
  try{returnedUndefined=(await exports.fn.call(instance,spec.symbol))===undefined;}
  catch(e){failure={name:e.name,message:e.message,code:e.code??null,response:e.response??null,identity:e===regularError?'regular':e===algoError?'algo':e===listenerError?'listener':e===emitterError?'emitter':'other'};}
  const expectedRequests=variant==='binance'&&spec.regular!=='sync-reject'?2:1;assert.equal(requests.length,expectedRequests);
  const native=store.orders.map(o=>({id:o.id,symbol:o.symbol,type:o.type,status:o.status,price:o.price,amount:o.amount}));
  results.push({variant,index,...spec,initialUpdateId,requests,updates,notifications,later,emissions,trace,error:failure,returnedUndefined,orders:native,updateId:store.updateIds.orders});
  unsubscribe();unsubscribeLater();
 }
}
assert.equal(results.length,95);
process.stdout.write(JSON.stringify(results));
