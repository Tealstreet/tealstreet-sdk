import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import vm from 'node:vm';
import {createRequire} from 'node:module';
import {load,root,ts,boundaries} from './source.mjs';
let now=1791800000123;
const virtualClock={getCurrentTimeMs:()=>now};
boundaries.set(path.join(root,'packages/safe-cex/src/utils/virtualClock.ts'),{virtualClock});
const {DefaultStore}=load('packages/safe-cex/src/store/store.base.ts');
const require=createRequire(path.join(path.resolve(process.env.SAFE_CEX_TYPESCRIPT_PATH),'../mexc-fetch.cjs'));
const filename=path.join(root,'packages/safe-cex/src/exchanges/mexc/mexc.exchange.ts');
const ast=ts.createSourceFile(filename,fs.readFileSync(filename,'utf8'),ts.ScriptTarget.Latest,true);
const names=['fetchOrders','fetchRegularOrders','orderListEndpoint','filterRecentlyCanceledStopLikeOrders','isRecentlyCanceledStopLikeOrder','isRecentlyCanceledStopLikeOrderId','isStopLikeOrder'];
const selected=[];
for(const node of ast.statements){
 if(ts.isVariableStatement(node)&&node.declarationList.declarations.some(d=>['extractData','extractRows','MEXC_PLAN_ORDER_LOOKBACK_MS','MEXC_RECENT_STOP_CANCEL_SHIELD_MS'].includes(d.name.getText(ast))))selected.push(node.getText(ast));
 if(ts.isClassDeclaration(node)&&node.name?.text==='MexcExchange')for(const member of node.members)if(names.includes(member.name?.getText(ast)))selected.push(`this.${member.name.getText(ast)}=${member.initializer.getText(ast)};`);
}
let source=selected.join('\n');
if(process.env.MEXC_TS_CONTROL==='fetch-dedupe'){
 const anchor="return uniqBy(orders, 'id');";if(!source.includes(anchor))throw Error('missing fetch-dedupe anchor');source=source.replace(anchor,'return orders;');
}
const js=ts.transpileModule(source,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;
const utils=load('packages/safe-cex/src/exchanges/mexc/mexc.utils.ts');
const bindings={...load('packages/safe-cex/src/types.ts'),...load('packages/safe-cex/src/exchanges/mexc/mexc.types.ts'),...utils,...load('packages/safe-cex/src/utils/omitUndefined.ts'),virtualClock,uniqBy:require('lodash/uniqBy')};
const number=n=>{if(Number.isNaN(n))return {$number:'7ff8000000000000'};const b=Buffer.alloc(8);b.writeDoubleBE(n);return {$number:b.toString('hex')};};
const encode=v=>v===undefined?{$undefined:true}:typeof v==='number'?number(v):Array.isArray(v)?v.map(encode):v&&typeof v==='object'?Object.fromEntries(Object.entries(v).map(([k,x])=>[k,encode(x)])):v;
const raw=(id,symbol='BTC_USDT')=>({id,orderId:id,symbol,side:1,positionType:2,vol:5,price:123,createTime:1700000000000});
const marketRaw={symbol:'BTC_USDT',contractSize:0.01,volUnit:3,minVol:2,maxVol:100000,priceScale:5,minLeverage:4,maxLeverage:100};
const cases=[];
for(const method of ['fetchOrders','fetchRegularOrders'])for(const symbols of [undefined,[],['BTCUSDT'],['BTCUSDT','ETHUSDT'],['']])for(const publicOnly of [false,true])for(const failure of [null,'open','plan','stop','row-null','row-number','row-array','row-coercion','error-open','error-plan','error-stop','identity']){
 const store=new DefaultStore();store.update({markets:[utils.normalizeMarket(marketRaw)]});
 const calls=[],logs=[];const canceled=new Map([['201',now]]);
 const context=vm.createContext({...bindings,store,isPublicDataOnly:publicOnly,recentlyCanceledStopLikeOrderIds:canceled,log:(message,severity)=>logs.push({message,severity}),xhr:{get:async(url,config)=>{
  calls.push({url,params:config.params});
  const kind=url.includes('open_orders')?'open':url.includes('planorder')?'plan':'stop';
  if(failure===kind || failure===`error-${kind}`){const error=new Error(`failed ${kind}`);error.response={data:{message:failure===kind?`venue ${kind}`:{toString:null}}};throw error;}
  const data=kind==='open'?[raw('101'),{...raw('101'),price:999},raw('102','ETH_USDT')]:kind==='plan'?[raw('201'),raw('202')]:[raw('301')];
  if(kind==='open' && failure==='identity') data[0].orderId='wire-101';
  if(kind==='open' && failure?.startsWith('row-')) data.push(failure==='row-null'?null:failure==='row-number'?7:failure==='row-array'?[]:{...raw('bad'),symbol:{toString:null}});
  return {data:{data:{items:data}}};
 }}});
 vm.runInContext(js,context,{filename});
 let result,error;
 try {result=await context[method](symbols);} catch(e){error={name:e.name,message:e.message};}
 if(failure==='identity' && calls.some(call=>call.url.includes('open_orders'))) assert.equal(result?.[0]?.id,'wire-101','MexcOrder.id uses the distinct raw orderId value');
 cases.push({method,symbols,publicOnly,failure,marketRaw,time:now,result:encode(result),error,calls:encode(calls),logs,canceled:Object.fromEntries(canceled)});
}
process.stdout.write(JSON.stringify(cases.map(item=>({captureArchitecture:process.arch,captureNodeMajor:Number(process.versions.node.split('.')[0]),...item}))));
