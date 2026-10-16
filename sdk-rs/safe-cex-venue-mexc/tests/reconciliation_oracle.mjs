import fs from 'node:fs';
import path from 'node:path';
import vm from 'node:vm';
import {createRequire} from 'node:module';
import {load,root,ts,boundaries} from './source.mjs';
let now=1000;
const virtualClock={getCurrentTimeMs:()=>now};
boundaries.set(path.join(root,'packages/safe-cex/src/utils/virtualClock.ts'),{virtualClock});
const {DefaultStore}=load('packages/safe-cex/src/store/store.base.ts');
const require=createRequire(path.join(path.resolve(process.env.SAFE_CEX_TYPESCRIPT_PATH),'../mexc-reconciliation.cjs'));
const filename=path.join(root,'packages/safe-cex/src/exchanges/mexc/mexc.exchange.ts');
const source=fs.readFileSync(filename,'utf8');const ast=ts.createSourceFile(filename,source,ts.ScriptTarget.Latest,true);
const names=['fetchAndSetOrders','fetchAndSetRegularOrders','markStopLikeOrderSeen','isRecentlyPlacedStopLikeOrder','isRecentlyCanceledStopLikeOrder','isRecentlyCanceledStopLikeOrderId','isRecentlyRejectedRegularOrder','shouldPreserveStopLikeOrder','shouldPreserveRecentRegularOrder','isStopLikeOrder'];
const constants=['MEXC_RECENT_STOP_LIKE_PLACE_SHIELD_MS','MEXC_RECENT_STOP_CANCEL_SHIELD_MS','MEXC_RECENT_REJECTED_ORDER_SHIELD_MS','MEXC_RECENT_REGULAR_ORDER_PRESERVE_MS'];
const selected=[];
for(const node of ast.statements){
 if(ts.isVariableStatement(node)&&node.declarationList.declarations.some(d=>constants.includes(d.name.getText(ast))))selected.push(node.getText(ast));
 if(ts.isClassDeclaration(node)&&node.name?.text==='MexcExchange')for(const member of node.members)if(names.includes(member.name?.getText(ast)))selected.push(`this.${member.name.getText(ast)}=${member.initializer.getText(ast)};`);
}
const bindings={...load('packages/safe-cex/src/types.ts'),...load('packages/safe-cex/src/exchanges/mexc/mexc.utils.ts'),virtualClock,uniqBy:require('lodash/uniqBy')};
let input=selected.join('\n');
if(process.env.MEXC_TS_CONTROL==='snapshot-disposal'){
 const before='if (this.isDisposed) return orders;';if(!input.includes(before))throw Error('missing snapshot disposal mutation anchor');input=input.replace(before,'if (false) return orders;');
}
const js=ts.transpileModule(input,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;
const row=(id,type='limit',symbol='BTCUSDT',extra={})=>({id,type,symbol,status:'open',lastUpdate:1,...extra});
const initial=[row('fresh'),row('old','limit','BTCUSDT',{lastUpdate:-100000}),row('placed','stop_market'),row('unseen','stop_market'),row('canceled','stop_market'),row('rejected'),row('terminal','limit','BTCUSDT',{status:'canceled'}),row('eth','limit','ETHUSDT'),row('empty','limit','')];
const fetched=[row('fetched'),row('seen','stop_market'),row('fetched','limit','BTCUSDT',{lastUpdate:999}),row('eth-fetched','limit','ETHUSDT')];
const cases=[];
for(const method of ['fetchAndSetOrders','fetchAndSetRegularOrders'])for(const symbols of [undefined,[],['BTCUSDT'],['ETHUSDT'],['BTCUSDT','ETHUSDT'],['BTCUSDT','BTCUSDT'],['']])for(const disposed of [false,true])for(const time of [1000,15001,20001,30002]){
 now=time;const store=new DefaultStore();store.update({orders:structuredClone(initial)});
 const placed=new Map([['placed',1],['seen',1]]),canceled=new Map([['canceled',1]]),rejected=new Map([['rejected',1]]);
 const calls=[];let notifications=0;store.subscribe(()=>notifications++);
 const context=vm.createContext({...bindings,store,isDisposed:false,recentlyPlacedStopLikeOrderIds:placed,recentlyCanceledStopLikeOrderIds:canceled,recentlyRejectedRegularOrderIds:rejected});
 for(const name of ['fetchOrders','fetchRegularOrders'])context[name]=async scope=>{calls.push({name,symbols:scope});await Promise.resolve();context.isDisposed=disposed;if(disposed)store.reset();return structuredClone(fetched);};
 vm.runInContext(js,context,{filename});
 const result=await context[method](symbols);
 const observation=structuredClone({method,symbols,disposed,time,initial,fetched,result,orders:store.orders,loaded:store.loaded,notifications,updateIds:store.updateIds,calls,placed:Object.fromEntries(placed),canceled:Object.fromEntries(canceled),rejected:Object.fromEntries(rejected)});
 result[0].lastUpdate=4242;
 observation.alias={id:result[0].id,returnedLastUpdate:result[0].lastUpdate,storedLastUpdate:store.orders.find(o=>o.id===result[0].id)?.lastUpdate??null};
 cases.push(observation);
}
process.stdout.write(JSON.stringify(cases));
