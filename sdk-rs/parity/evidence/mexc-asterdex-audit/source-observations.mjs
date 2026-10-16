// Source-only observations. Production/native code is never mutated.
import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import {root,runtimeRequire,once,declaration,execute,loader} from './source-core.mjs';
const high=JSON.parse('"\\ud800"'),low=JSON.parse('"\\udc00"');
const units=s=>Array.from({length:s.length},(_,i)=>s.charCodeAt(i));
const selected=process.argv[2],control=process.argv.includes('--control');
const prefix='packages/safe-cex/src/exchanges/mexc/';
const profiles={};
for(const [name,callee] of [['fetchMarkets','xhr'],['fetchTickers','xhr'],['fetchAndSetOrders','fetchOrders'],['fetchAndSetRegularOrders','fetchRegularOrders']]){
 profiles['mexc-'+name]=()=>{
  let member=declaration(fs.readFileSync(path.join(root,prefix+'mexc.exchange.ts'),'utf8'),name,'MexcExchange');
  if(control){const prefix=member.slice(0,member.indexOf('=> {')+4);member=once(member,prefix,prefix+' await Promise.resolve();');}
  const receiver=new (execute('class Selected {'+member+'}\nexports.Selected=Selected;',{ENDPOINTS:{CONTRACT_DETAIL:'/fixture-markets',TICKER:'/fixture-tickers'}}).Selected)();
  const calls=[];const callback=(...args)=>{calls.push(args);return new Promise(()=>{});};
  if(callee==='xhr')receiver.xhr={get:callback};else receiver[callee]=callback;
  const pending=receiver[name]();pending.catch(()=>{});
  assert.equal(calls.length,1,'MEXC-PREFIX-'+name+' immediate callback');
  return {profile:'mexc-'+name,immediateCalls:1,scope:'actual source prefix, continuation not reached; native async laziness inspected only'};
 };
}
for(const name of ['normalizeMarket','normalizeTicker','normalizeOrder','normalizePlanOrder','normalizePosition','normalizeStopOrder','normalizeExecution']){
 profiles['mexc-'+name]=()=>{
  const load=loader((text,file)=>{
   if(!control||!file.endsWith('/mexc.utils.ts'))return text;
   const original=declaration(text,name);
   const changed=name==='normalizeMarket'?once(original,'symbol: fromMexcSymbol(id),',"symbol: 'replacement',"):
    name==='normalizeTicker'?once(original,'symbol: market.symbol,',"symbol: 'replacement',"):
    once(original,'    symbol,',"    symbol: 'replacement',");
   return once(text,original,changed);
  });
  const api=load(prefix+'mexc.utils.ts');
  const raw={symbol:high,settleCoin:'USDT',id:'fixture-id',orderId:'fixture-order',side:1,positionType:1,vol:1};
  // Ticker market identity comes from the actual source market producer.
  const result=name==='normalizeTicker'?api[name]({},api.normalizeMarket(raw)):api[name](raw);
  assert.ok(result,'MEXC-'+name+' admitted result');
  assert.deepEqual(units(result.symbol),[0xd800],'MEXC-UNIT-'+name+' returned symbol');
  return {profile:'mexc-'+name,units:[0xd800],scope:'actual returned native JS field before serialization, no Rust decode run'};
 };
}
function factory(mode){
 const axios=runtimeRequire('axios');
 const boundaries=new Map([
  [path.join(root,'packages/safe-cex/src/safeCexAxiosCreate.ts'),{safeCexAxiosCreate:config=>axios.create(config)}],
  [path.join(root,'packages/safe-cex/src/utils/virtualClock.ts'),{virtualClock:{getCurrentTimeMs:()=>500}}],
 ]);
 const load=loader((text,file)=>{
  if(!control||!file.endsWith('/mexc.api.ts'))return text;
  return mode==='request'?once(text,'data: bodyString || nextConfig.data',"data: 'replacement'"):
   once(text,'    return response;',"    response.data.symbol = 'replacement'; return response;");
 },boundaries);
 // Public synthetic fixture values; never real credentials or live transport.
 return load(prefix+'mexc.api.ts').createAPI({key:'public-test-key',secret:'public-test-bytes'},{});
}
profiles['mexc-normalize-request-consumer']=()=>{
 const xhr=factory('request');const callback=xhr.interceptors.request.handlers.at(-1).fulfilled;
 const config=callback({method:'post',url:'/private-fixture',data:high});
 assert.deepEqual(units(config.data),[0xd800],'MEXC-REQUEST actual returned config data unit');
 return {profile:'mexc-normalize-request-consumer',units:[0xd800],scope:'actual factory request callback calls private normalizer; actual HMAC/headers with public test bytes, no request sent'};
};
profiles['mexc-factory-response']=()=>{
 const xhr=factory('response');const callback=xhr.interceptors.response.handlers.at(-1).fulfilled;
 const response={data:{symbol:high}};const returned=callback(response);
 assert.strictEqual(returned,response,'MEXC-RESPONSE returned response identity');
 assert.deepEqual(units(returned.data.symbol),[0xd800],'MEXC-RESPONSE actual returned field unit');
 return {profile:'mexc-factory-response',units:[0xd800],scope:'actual installed response callback, native Value/String refusal inspected only'};
};
function apiDeclarations(change){
 const file=path.join(root,prefix+'mexc.api.ts');let source=fs.readFileSync(file,'utf8');
 source=['serializeMexcParams','normalizeRequestData','getQueryString','signMexcRequest'].map(name=>declaration(source,name)).join('\n');
 return execute(change(source)+'\nObject.assign(exports,{serializeMexcParams,normalizeRequestData,getQueryString,signMexcRequest});',{qs:runtimeRequire('qs'),createHmac:runtimeRequire('create-hmac')});
}
profiles['mexc-private-query-erasure']=()=>{
 const api=apiDeclarations(text=>control?once(text,'return query;',"return 'replacement';"):text);
 const value=api.getQueryString({url:'/?q='+high});
 assert.deepEqual(units(value),[0x71,0x3d,0xd800],'MEXC-QUERY private returned units');
 const signature=api.signMexcRequest('public-test-bytes','public-test-key','500',value);
 assert.equal(signature,api.signMexcRequest('public-test-bytes','public-test-key','500','q=\ufffd'),'MEXC-QUERY actual HMAC erases retained unit');
 return {profile:'mexc-private-query-erasure',units:[0x71,0x3d,0xd800],scope:'private return only; actual HMAC UTF8 erases it, no forced public-output inference'};
};
profiles['mexc-query-encoding']=()=>{
 const api=apiDeclarations(text=>control?once(text,'qs.stringify(params,','qs.stringify({},'):text);
 const value=api.serializeMexcParams({a:high,z:low});
 assert.equal(value.split('&').filter(Boolean).length,2,'MEXC-QS two encoded fields');
 assert.ok([...value].every(c=>c.charCodeAt(0)<128),'MEXC-QS ASCII encoded result');
 return {profile:'mexc-query-encoding',fieldCount:2,scope:'encoder erases input units; not a retained-output surrogate witness'};
};
assert.equal(Object.keys(profiles).length,15,'AUDIT independent fifteen-profile floor');
const observations=[];
if(selected){assert.ok(Object.hasOwn(profiles,selected));observations.push(await profiles[selected]());}
else{assert.equal(control,false);for(const run of Object.values(profiles))observations.push(await run());}
process.stdout.write(JSON.stringify({node:process.version,arch:process.arch,observations})+'\n');
