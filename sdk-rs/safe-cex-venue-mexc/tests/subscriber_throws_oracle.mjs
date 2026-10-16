import fs from 'node:fs';
import path from 'node:path';
import vm from 'node:vm';
import {createRequire} from 'node:module';
import {load,root,ts,boundaries} from './source.mjs';

let now=1000;
const virtualClock={getCurrentTimeMs:()=>now};
boundaries.set(path.join(root,'packages/safe-cex/src/utils/virtualClock.ts'),{virtualClock});
const {DefaultStore}=load('packages/safe-cex/src/store/store.base.ts');
const require=createRequire(path.join(path.resolve(process.env.SAFE_CEX_TYPESCRIPT_PATH),'../mexc-subscriber-throws.cjs'));
const filename=path.join(root,'packages/safe-cex/src/exchanges/mexc/mexc.exchange.ts');
const source=fs.readFileSync(filename,'utf8');
const ast=ts.createSourceFile(filename,source,ts.ScriptTarget.Latest,true);
const names=['fetchAndSetOrders','fetchAndSetRegularOrders','markStopLikeOrderSeen','isRecentlyPlacedStopLikeOrder','isRecentlyCanceledStopLikeOrder','isRecentlyCanceledStopLikeOrderId','isRecentlyRejectedRegularOrder','shouldPreserveStopLikeOrder','shouldPreserveRecentRegularOrder','isStopLikeOrder'];
const constants=['MEXC_RECENT_STOP_LIKE_PLACE_SHIELD_MS','MEXC_RECENT_STOP_CANCEL_SHIELD_MS','MEXC_RECENT_REJECTED_ORDER_SHIELD_MS','MEXC_RECENT_REGULAR_ORDER_PRESERVE_MS'];
const selected=[];
for(const node of ast.statements){
 if(ts.isVariableStatement(node)&&node.declarationList.declarations.some(d=>constants.includes(d.name.getText(ast))))selected.push(node.getText(ast));
 if(ts.isClassDeclaration(node)&&node.name?.text==='MexcExchange')for(const member of node.members)if(names.includes(member.name?.getText(ast)))selected.push(`this.${member.name.getText(ast)}=${member.initializer.getText(ast)};`);
}
let input=selected.join('\n');
if(process.env.MEXC_TS_CONTROL==='order-snapshot-write'){
 const writes=input.split('this.store.update({').length-1;
 if(writes!==2)throw Error(`expected two order snapshot Store writes, found ${writes}`);
 input=input.replaceAll('this.store.update({','if (false) this.store.update({');
}
const js=ts.transpileModule(input,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;
const bindings={...load('packages/safe-cex/src/types.ts'),...load('packages/safe-cex/src/exchanges/mexc/mexc.utils.ts'),virtualClock,uniqBy:require('lodash/uniqBy')};
const row=(id,type='limit',symbol='BTCUSDT',extra={})=>({id,type,symbol,status:'open',lastUpdate:1,...extra});
const initial=[row('fresh'),row('old','limit','BTCUSDT',{lastUpdate:-100000}),row('placed','stop_market'),row('unseen','stop_market'),row('canceled','stop_market'),row('rejected'),row('terminal','limit','BTCUSDT',{status:'canceled'}),row('eth','limit','ETHUSDT'),row('empty','limit','')];
const fetched=[row('fetched'),row('seen','stop_market'),row('eth-fetched','limit','ETHUSDT')];
const results=[];
for(const method of ['fetchAndSetOrders','fetchAndSetRegularOrders']){
 now=1000;const store=new DefaultStore();store.update({orders:structuredClone(initial)});
 const subscriberCalls=[];
 store.subscribe(()=>{subscriberCalls.push('first');throw Error('subscriber failed');});
 store.subscribe(()=>subscriberCalls.push('second'));
 const context=vm.createContext({...bindings,store,isDisposed:false,recentlyPlacedStopLikeOrderIds:new Map(),recentlyCanceledStopLikeOrderIds:new Map(),recentlyRejectedRegularOrderIds:new Map()});
 context.fetchOrders=async()=>structuredClone(fetched);
 context.fetchRegularOrders=async()=>structuredClone(fetched);
 vm.runInContext(js,context,{filename});
 let rejected=false,message='';
 try{await context[method]();}catch(error){rejected=true;message=error.message;}
 results.push({method,rejected,message,subscriberCalls,orderIds:store.orders.map(order=>order.id),loaded:store.loaded.orders});
}
process.stdout.write(JSON.stringify(results));
