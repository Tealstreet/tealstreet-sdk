const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH),root=path.resolve(__dirname,'../../..');
let source=fs.readFileSync(path.join(root,'packages/safe-cex/src/exchanges/bybit/bybit.exchange.ts'),'utf8');
// Discrimination controls mutate the maintained source in memory only.
const control=process.env.BYBIT_STARTUP_TS_CONTROL;
if(control==='inverse-skip'){const before='this.inversePublicWebsocket?.connectAndSubscribe();';assert(source.includes(before),'STARTUP inverse mutation anchor');source=source.replace(before,'');}
if(control==='probe-message'){const before="String(err?.message || err) || 'request failed'";assert(source.includes(before),'STARTUP probe mutation anchor');source=source.replace(before,"String(err) || 'request failed'");}
const a=ts.createSourceFile('bybit.exchange.ts',source,99,true);
const c=a.statements.find(s=>ts.isClassDeclaration(s)&&s.name.text==='BybitExchange');assert(c);
const hits=c.members.filter(m=>m.name?.getText(a)==='start');assert.equal(hits.length,1,'STARTUP single start member');
const code=ts.transpileModule(`class Subject {${hits[0].getText(a)}}globalThis.api=new Subject()`,{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText;
const units=s=>Array.from({length:s.length},(_,i)=>s.charCodeAt(i));const bits=n=>{const b=Buffer.alloc(8);b.writeDoubleBE(n);return Number.isNaN(n)?'nan':b.toString('hex')};
function atom(v,refs){if(v===undefined)return{u:true};if(v===null)return null;if(typeof v==='number')return{n:bits(v)};if(typeof v==='string')return{s:units(v)};if(typeof v==='boolean')return v;if(refs.has(v))return{ref:refs.get(v)};if(v instanceof Error)return{error:true,name:units(v.name),message:units(v.message)};if(Array.isArray(v))return{a:Array.from(v,x=>atom(x,refs))};if(typeof v==='function')return{fn:v.startupCode??'unknown'};return{o:Object.entries(v).map(([k,v])=>[units(k),atom(v,refs)])};}
async function run(input){
 const refs=new WeakMap(),events=[],started=[];const ref=(v,name)=>{if(v&&typeof v==='object')refs.set(v,name);return v};
 const primary=ref(new Error('primary-\ud800'),'primary');
 const rejection=input.probeReject==='empty-string'?'':input.probeReject==='empty-error'?ref(new Error(''),'emptyError'):primary;
 function rec(name,args=[]){events.push({name,args:atom(args,refs)});if(input.fail===name)throw primary}
 const markets=ref([{symbol:'BTCUSDT'},{symbol:'ETHUSDT'}],'markets'),rawTickers=ref([{symbol:'BTCUSDT',lastPrice:'1'}],'rawTickers'),tickers=ref([{symbol:'BTCUSDT',last:1}],'tickers'),fees=ref({linear:true},'fees');
 let loaded={sentinel:7.5,markets:false,tickers:false,balance:false,positions:false,orders:false};
 const store=ref({update(data){rec('update',[data]);Object.assign(store,data);if('loaded' in data)loaded=data.loaded},get loaded(){return loaded},set loaded(v){loaded=v}},'store');
 const values={virtualClockStart:undefined,fetchFees:fees,loadAccountMarketOverlay:undefined,initExternalPublicData:!!input.external,fetchMarginAccountInfos:undefined,fetchAndCacheApiKeyInfo:undefined,fetchMarkets:markets,fetchTickersWithoutParse:rawTickers,fetchOrdersPositionsAndBalance:undefined};
 let api;
 async function dependency(name,args=[]){rec(name,args);const delay=input.delays?.[name]??1;for(let i=0;i<delay;i++)await Promise.resolve();rec('resolved:'+name);if(input.disposeAfter===name)api.isDisposed=true;
  if(name==='loadAccountMarketOverlay'){const inner=args[0]();started.push(inner);await inner;}
  if(name==='fetchMarginAccountInfos'&&input.probeReject)throw rejection;
  if(name==='fetchAndCacheApiKeyInfo'&&input.cacheFail)throw primary;
  if(input.failAsync===name)throw primary;return values[name]}
 const call=name=>(...args)=>{const p=dependency(name,args);started.push(p);p.catch(()=>{});return p};
 const socket=label=>ref({connectAndSubscribe(){rec('connect:'+label)}},label);
 const globals={virtualClock:{start:call('virtualClockStart')},bybitFeeOverlay(v){rec('feeOverlay',[v])},LogSeverity:{Warning:'warning'}};
 vm.runInNewContext(code,globals);api=globals.api;const start=api.start;
 Object.assign(api,{batchLimit:3,isPublicDataOnly:!!input.publicOnly,needsAccountMarketOverlay:!!input.overlay,isDisposed:false,store,publicWebsocket:socket('public'),privateWebsocket:socket('private'),inversePublicWebsocket:input.inverse?socket('inverse'):undefined,
  options:{key:input.noKey?'':'key',secret:input.noSecret?'':'secret',remoteSigner:input.remoteSigner?{signer:true}:undefined,payloadSigner:undefined}});
 for(const name of ['fetchFees','loadAccountMarketOverlay','initExternalPublicData','fetchMarginAccountInfos','fetchAndCacheApiKeyInfo','fetchMarkets','fetchTickersWithoutParse','fetchOrdersPositionsAndBalance'])api[name]=call(name);
 api.log=(...args)=>rec('log',args);api.parseTickers=raw=>{rec('parseTickers',[raw]);return tickers};api.tick=()=>rec('tick');api.emitAndLogError=e=>rec('emit',[e]);
 let result,error;try{result=await start()}catch(e){error=e}
 for(let i=0;i<128;i++)await Promise.resolve();await Promise.allSettled(started);
 return{input,events,result:atom(result,refs),error:error?atom(error,refs):null,primary:error===primary,batchLimit:api.batchLimit,loaded:atom(loaded,refs),storeMarkets:atom(store.markets,refs),storeTickers:atom(store.tickers,refs)};
}
const inputs=[{},{publicOnly:true},{overlay:true},{overlay:true,disposeAfter:'loadAccountMarketOverlay'},{external:true},{publicOnly:true,external:true},
 {probeReject:'primary'},{probeReject:'empty-string'},{probeReject:'empty-error'},{cacheFail:true},{disposeAfter:'fetchMarkets'},{disposeAfter:'fetchTickersWithoutParse'},
 {noSecret:true},{noSecret:true,remoteSigner:true},{noKey:true},{inverse:true},{failAsync:'fetchOrdersPositionsAndBalance'},{failAsync:'fetchMarkets'},
 {delays:{fetchTickersWithoutParse:6,fetchMarkets:1}},{delays:{fetchMarginAccountInfos:12}},{fail:'log'},{cacheFail:true,fail:'emit'}];
assert.equal(inputs.length,22,'STARTUP independent scenario floor');
(async()=>{const cases=[];for(const input of inputs)cases.push(await run(input));process.stdout.write(JSON.stringify(cases))})().catch(e=>{console.error(e);process.exitCode=1});
