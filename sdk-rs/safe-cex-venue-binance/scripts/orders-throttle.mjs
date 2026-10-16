import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import {createRequire} from 'node:module';
const ts=createRequire(import.meta.url)(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const throttleNames=['normal','unset','null','false','zero','negative-zero','empty-string','nan','negative','object-zero','clear-fail','set-fail','late-callback','late-field-set-fail','get-callback-fail','disposed','empty-array','return-unset'];
const fetchNames=['normal','disposed','disposed-during-fetch','stale','stale-during-fetch','stale-cleared-during-fetch','fetch-sync-fail','fetch-reject','stale-getter-fail','loaded-getter-fail','update-fail','late-loaded','existing-orders-key','missing-orders-key','stale-getter-disposes','loaded-getter-disposes','fetch-replaces-timer','update-replaces-timer','update-getter-fail','update-getter-changes-store'];
const encode=t=>t===undefined?'undefined':typeof t==='number'?(Number.isNaN(t)?'nan':Object.is(t,-0)?'-zero':t):t?.handle!==undefined?{handle:t.handle}:t;
const results=[];let clear,set;
for(const [variant,klass] of [['binance','BinancePrivateWebsocket'],['binancepm','BinancePMPrivateWebsocket']]){
 const file=new URL(`../../../packages/safe-cex/src/exchanges/binance/${variant}.ws-private.ts`,import.meta.url),ast=ts.createSourceFile(file.pathname,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true),decl=ast.statements.find(n=>ts.isClassDeclaration(n)&&n.name?.text===klass);
 const members=['throttledFetchOrders','fetchThrottledOrders'].map(name=>{const member=decl.members.find(m=>m.name?.getText(ast)===name);assert.ok(member);return member.getText(ast);});
 const exports={};vm.runInNewContext(ts.transpileModule(`exports.Subject=class {${members.join('\n')}}`,{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText,{exports,clearTimeout:t=>clear(t),setTimeout:(fn,delay)=>set(fn,delay)});
 for(const name of throttleNames){
  const instance=new exports.Subject(),trace=[],calls=[],error=RangeError('throttle fixture'),old={handle:11},created={handle:42},replacement={handle:33};
  const initial=name==='unset'?undefined:name==='null'?null:name==='false'?false:name==='zero'?0:name==='negative-zero'?-0:name==='empty-string'?'':name==='nan'?NaN:name==='negative'?-1:name==='object-zero'?{handle:0}:name==='empty-array'?[]:old;
  instance.ordersThrottleTimeout=initial;instance.isDisposed=name==='disposed';
  const callbacks=[1,2,3].map(id=>()=>{calls.push(id);});let current=callbacks[0],callback,delay,failure=null;
  Object.defineProperty(instance,'fetchThrottledOrders',{get(){trace.push(['get-callback']);if(name==='get-callback-fail')throw error;return current;}});
  clear=t=>{assert.equal(t,initial);trace.push(['clear',encode(t)]);if(name==='clear-fail')throw error;if(name==='late-callback')current=callbacks[1];if(name==='late-field-set-fail')instance.ordersThrottleTimeout=replacement;};
  set=(fn,ms)=>{trace.push(['set',ms]);callback=fn;delay=ms;if(name==='set-fail'||name==='late-field-set-fail')throw error;return name==='return-unset'?undefined:created;};
  try{instance.throttledFetchOrders();}catch(e){assert.equal(e,error);failure='original';}
  let identity=null;
  if(callback){assert.equal(callback,name==='late-callback'?callbacks[1]:callbacks[0]);identity=true;if(failure===null){current=callbacks[2];callback();}}
  if(failure===null&&name!=='return-unset')assert.equal(instance.ordersThrottleTimeout,created);
  results.push({variant,method:'throttle',name,trace,calls,delay:delay??null,identity,timer:encode(instance.ordersThrottleTimeout),disposed:instance.isDisposed,failure});
 }
 for(const name of fetchNames){
  const instance=new exports.Subject(),trace=[],error=RangeError('refresh fixture'),marker={id:91},orders=[{id:'first',price:12.3,amount:3.6},{id:'second',price:14.2,amount:2.1}];
  instance.ordersThrottleTimeout={handle:11};instance.isDisposed=name==='disposed';let stale=name==='stale'||name==='stale-cleared-during-fetch',updates=0,patch=null,receiver=null;
  let primaryLoaded={balance:true,orders:name==='existing-orders-key',markets:false,tickers:true,positions:false,marker};
  if(name==='missing-orders-key')delete primaryLoaded.orders;
  const secondaryLoaded={balance:false,orders:false,markets:true,tickers:false,positions:true,marker};
  const makeStore=(id,getLoaded)=>({id,get loaded(){trace.push(['get-loaded',id]);if(name==='loaded-getter-fail')throw error;if(name==='loaded-getter-disposes')instance.isDisposed=true;return getLoaded();},get update(){trace.push(['get-update',id]);if(name==='update-getter-fail')throw error;if(name==='update-getter-changes-store')instance.parent.store=secondary;return function(p){trace.push(['update',this.id]);assert.equal(p.orders,orders);assert.equal(p.loaded.marker,marker);assert.notEqual(p.loaded,getLoaded());patch=p;receiver=this.id;updates++;if(name==='update-replaces-timer')instance.ordersThrottleTimeout={handle:44};if(name==='update-fail')throw error;};}});
  const primary=makeStore(10,()=>primaryLoaded),secondary=makeStore(20,()=>secondaryLoaded);
  instance.parent={store:primary,fetchOrders:()=>{trace.push(['fetch']);assert.equal(instance.ordersThrottleTimeout,undefined);if(name==='fetch-sync-fail')throw error;return Promise.resolve().then(()=>{if(name==='disposed-during-fetch')instance.isDisposed=true;if(name==='stale-during-fetch')stale=true;if(name==='stale-cleared-during-fetch')stale=false;if(name==='late-loaded')primaryLoaded={...primaryLoaded,balance:false,positions:true};if(name==='fetch-replaces-timer')instance.ordersThrottleTimeout={handle:33};if(name==='fetch-reject')throw error;return orders;});},get ordersSnapshotStale(){trace.push(['stale']);if(name==='stale-getter-fail')throw error;if(name==='stale-getter-disposes')instance.isDisposed=true;return stale;}};
  await instance.fetchThrottledOrders();
  results.push({variant,method:'fetch',name,trace,updates,receiver,timer:encode(instance.ordersThrottleTimeout),disposed:instance.isDisposed,loaded:patch?.loaded??null,keys:patch?Object.keys(patch.loaded):[],orders:patch?patch.orders.map(o=>({id:o.id,price:o.price,amount:o.amount})):null,sourceLoadedOrders:Object.hasOwn(primaryLoaded,'orders')?primaryLoaded.orders:'undefined',currentStore:instance.parent.store.id,identity:patch?true:null});
 }
}
assert.equal(results.length,76,'independently authored 2*(18+20) floor');
process.stdout.write(JSON.stringify(results));
