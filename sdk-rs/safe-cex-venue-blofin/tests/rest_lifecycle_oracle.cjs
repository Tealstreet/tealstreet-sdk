const assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path');
const {root,load,ts,dependencies}=require('../../safe-cex-types/scripts/accounts-source.cjs');
const constants=load(path.join(root,'packages/safe-cex/src/exchanges/blofin/blofin.types.ts'));
const enums=load(path.join(root,'packages/safe-cex/src/types.ts'));
const {DefaultStore}=load(path.join(root,'packages/safe-cex/src/store/store.base.ts'));
const file=path.join(root,'packages/safe-cex/src/exchanges/blofin/blofin.exchange.ts');
const ast=ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true);
const klass=ast.statements.find(s=>ts.isClassDeclaration(s)&&s.name?.text==='BlofinExchange');
const helpers={...load(path.join(root,'packages/safe-cex/src/utils/safe-math.ts')),...load(path.join(root,'packages/safe-cex/src/utils/roundUSD.ts'))};
const decls=ast.statements.filter(ts.isVariableStatement).flatMap(s=>s.declarationList.declarations);
const normalized=new Function(ts.transpileModule('return '+decls.find(d=>d.name.text==='getNormalizedInstType').initializer.getText(ast),{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText)();
const linear=new Function('getNormalizedInstType','BLOFIN_SUPPORTED_INST_TYPE','BLOFIN_SUPPORTED_CONTRACT_TYPE',ts.transpileModule('return '+decls.find(d=>d.name.text==='isBlofinLinearSwapMarket').initializer.getText(ast),{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText)(normalized,'SWAP','linear');
const context={...constants,...enums,...helpers,sumBy:dependencies('lodash/sumBy'),nanoid:()=> 'cache-id',isBlofinLinearSwapMarket:linear};
let activeTrace;
context.loop=(callback,delay)=>activeTrace.push(['schedule',delay]);
const RealDayjs=dependencies('dayjs');context.dayjs=value=>RealDayjs(value===undefined?1710000000123:value);
const names=['validateAccount','dispose','start','tick','fetchMarkets','fetchOHLCV'];
const factories={};
// dispose's super requires a real method HomeObject; the synthetic subclass below
// retains the exact maintained body, while its base/socket boundaries are injected.
for(const name of names.filter(n=>n!=='dispose')){
 const member=klass.members.find(m=>m.name?.text===name);assert(member&&ts.isArrowFunction(member.initializer),'method source shape '+name);
 factories[name]=new Function(...Object.keys(context),ts.transpileModule('return function(){return '+member.initializer.getText(ast)+'}',{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText)(...Object.values(context));
}
const market={id:'BTC-USDT',symbol:'BTCUSDT',base:'BTC',quote:'USDT',active:true,precision:{amount:0.01,price:0.25},limits:{amount:{min:0.01,max:100},leverage:{min:1,max:50}},contractSize:0.01,info:{stored:true}};
const position={id:'p1',symbol:'BTCUSDT',side:'long',entryPrice:5,notional:13,leverage:7,unrealizedPnl:2,contracts:3,liquidationPrice:1,marginType:'cross',positionMode:'oneway',info:{keep:'open'}};
const closed={...position,id:'p-closed',symbol:'ETHUSDT',contracts:0,leverage:19,info:{keep:'closed-context'}};
const order={id:'o1',symbol:'BTCUSDT',status:'open',type:'limit',side:'buy',price:11,amount:2,filled:0,remaining:2,reduceOnly:false,lastUpdate:17};
const balance={total:15,free:12,used:3,upnl:2};
const ticker={id:'BTC-USDT',symbol:'BTCUSDT',bid:1,ask:2,last:3,mark:4,index:5,percentage:6,fundingRate:7,fundingTime:12,volume:8,quoteVolume:9,openInterest:10};
const rawMarket={instType:'swap',contractType:'LINEAR',instId:'BTC-USDT-SWAP',baseCurrency:'BTC',quoteCurrency:'USDT',state:'live',maxLimitSize:'17.5',maxMarketSize:'12.75',contractValue:'0.01',lotSize:'3',minSize:'2',tickSize:'0.25',maxLeverage:'50'};
function observe(v){if(v===undefined)return{undefined:true};if(typeof v==='number'){const b=Buffer.alloc(8);b.writeDoubleBE(v);return{number:Number.isNaN(v)?'nan':b.toString('hex')}};if(Array.isArray(v))return v.map(observe);if(v&&typeof v==='object')return Object.fromEntries(Object.entries(v).map(([k,v])=>[k,observe(v)]));return v;}
const fixtures=[];const add=(method,input={})=>fixtures.push({method,input});
add('validateAccount',{body:{code:'0',data:[{available:'100.124',frozen:'2.236'},{available:'0.115',frozen:'0.015'}]}});
add('validateAccount',{body:{code:'0',data:[]}});add('validateAccount',{body:{code:'0',data:{length:1,0:{available:'2',frozen:'3'}}}});
add('validateAccount',{body:{code:'0',data:[{available:'bad',frozen:'1'}]}});add('validateAccount',{body:{code:0,msg:'numeric code'}});
add('validateAccount',{body:{code:'8'}});add('validateAccount',{throw:'network',hint:{reason:'blocked'}});add('validateAccount',{throw:'',hint:0});
add('fetchMarkets',{body:{data:[rawMarket,{...rawMarket,instId:'ETH-USDT',lotSize:'',minSize:'5',state:'halted'},{...rawMarket,instType:'SPOT'},{...rawMarket,contractType:'inverse'}]}});
add('fetchMarkets',{body:{data:[]}});add('fetchMarkets',{body:{data:{}}});add('fetchMarkets',{throw:'market failure',hint:'instrument hint'});add('fetchMarkets',{body:{data:[null]}});add('fetchMarkets',{body:{data:[{...rawMarket,instId:17},null]}});
for(const interval of ['1m','3m','5m','15m','30m','1h','2h','4h','6h','12h','1d','1w'])add('fetchOHLCV',{options:{symbol:'BTCUSDT',interval,limit:2,to:1710000000123},body:{data:[['3000.5','1.5','2.5','0.5','1.75','IGNORE','IGNORE','9.25'],['1000','5','7','4','6','999','888','11']]}});
add('fetchOHLCV',{options:{symbol:'BTCUSDT',interval:'1m',limit:1,to:-0.5},body:{data:[]}});
add('fetchOHLCV',{options:{symbol:'BTCUSDT',interval:'1m',from:1000,to:5000},body:{data:[]}});
add('fetchOHLCV',{options:{symbol:'BTCUSDT',interval:'1m',limit:2000,from:1000,to:5000},body:{data:[['bad','-0','bad','Infinity','2e400','x','y','-0']]}});
add('fetchOHLCV',{options:{symbol:'BTCUSDT',interval:'1d',limit:0.5,to:1710000000123},body:{data:[]}});
add('fetchOHLCV',{options:{symbol:'BTCUSDT',interval:'1w',limit:-0.5,to:1710000000123},body:{data:[]}});
add('fetchOHLCV',{options:{symbol:'BTCUSDT',interval:'1m',limit:0,from:0,to:0},body:{data:[]}});
add('fetchOHLCV',{options:{symbol:'MISSING',interval:'1m'},body:{data:[]}});add('fetchOHLCV',{options:{symbol:'BTCUSDT',interval:'1m'},body:{data:null}});add('fetchOHLCV',{options:{symbol:'BTCUSDT',interval:'1m'},throw:'candles',hint:'candle hint'});
add('fetchOHLCV',{options:{symbol:'BTCUSDT',interval:'1m'},body:{data:[{0:'2000',1:'3',2:'4',3:'1',4:'2',7:'8'}]}});
for(const input of [{},{publicOnly:true},{external:true,publicOnly:true},{external:true},{disposed:true},{auth:2,interval:40000},{external:true,disposed:true,auth:1,interval:0},{external:true,fail:'margin'},{external:true,fail:'positions'},{external:true,fail:'orders'}])add('start',input);
for(const input of [{},{disposed:true},{fail:'positions'},{fail:'orders'},{fail:'positions',failOrders:true},{disposeOn:'positions'},{disposeOn:'orders'},{ordersEmpty:true},{ordersEmpty:true,storedOrdersEmpty:true},{interval:0,count:7},{interval:40000,count:9007199254740992}])add('tick',input);
for(const stop of ['', 'super', 'public', 'private'])add('dispose',{stop});
async function run(f){
 const i=f.input,trace=[];activeTrace=trace;const store=new DefaultStore();store.update({markets:structuredClone([market]),positions:structuredClone([position,closed]),orders:i.storedOrdersEmpty?[]:structuredClone([order]),balance:{total:77,free:66,used:11,upnl:4},loaded:{markets:false,tickers:false,balance:false,positions:false,orders:false}});
 const unsubscribe=store.subscribe(data=>{trace.push(['store',observe({markets:data.markets,tickers:data.tickers,positions:data.positions,orders:data.orders,balance:data.balance,loaded:data.loaded})]);});
 let disposed=!!i.disposed,releaseMarkets;
 const fail=(kind)=>{if(i.disposeOn===kind)disposed=true;if(i.fail===kind||(kind==='orders'&&i.failOrders))throw{message:kind+' failure',response:{data:{msg:kind+' hint'}}};};
 const host={store,get isDisposed(){return disposed;},get isPublicDataOnly(){return !!i.publicOnly},get authFailureStreak(){return i.auth??0},
 getTickInterval(){trace.push(['interval']);return i.interval;},emitAndLogError(v){trace.push(['emit',observe(v)]);},
 xhr:{async get(url,config){trace.push(['get',url,config?.params??null]);if(Object.hasOwn(i,'throw'))throw{message:i.throw,response:{data:{msg:i.hint}}};return{data:i.body};}},
 publicWebsocket:{connectAndSubscribe(){trace.push(['public']);}},privateWebsocket:{connectAndSubscribe(){trace.push(['private']);}},
 async initExternalPublicData(){trace.push(['external']);return !!i.external;},
 async fetchMarkets(){trace.push(['markets']);if(i.deferMarkets)return await new Promise(resolve=>releaseMarkets=()=>resolve([market]));return [market];},async fetchTickersRaw(){trace.push(['raw']);fail('raw');return ['raw'];},formatTickers(raw){assert.deepEqual(raw,['raw']);trace.push(['format']);return[ticker];},
 async getOrFetchMarginType(symbol,refetch){assert.equal(symbol,undefined);assert.equal(refetch,true);trace.push(['margin']);fail('margin');},
 async getOrFetchPositionMode(symbol,refetch){assert.equal(symbol,undefined);assert.equal(refetch,true);trace.push(['mode']);fail('mode');},
 async fetchBalanceAndPositions(){trace.push(['positions']);fail('positions');return{balance,positions:[{...position,contracts:5,leverage:8}]};},
 async fetchOrders(){trace.push(['orders']);fail('orders');return i.ordersEmpty?[]:[{...order,price:13}];}};
 const method=factories[f.method];if(method)host[f.method]=method.call(host);
 if(f.method==='start'){host.start=factories.start.call(host);host.tick=factories.tick.call(host);}
 let result,returned;
 if(f.method==='dispose'){
  class Base{dispose(){trace.push(['super']);if(i.stop==='super')throw Error('super failed');}}
  const member=klass.members.find(m=>m.name?.text==='dispose');const SourceClass=new Function('Base',ts.transpileModule('return class extends Base {'+member.getText(ast)+'}',{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText)(Base);
  const instance=new SourceClass();for(const kind of ['public','private'])instance[kind+'Websocket']={dispose(){trace.push([kind]);if(i.stop===kind)throw Error(kind+' failed');}};
  try{instance.dispose();result={ok:observe(undefined)}}catch(e){result={error:e.message}}
 }else try{const v=await host[f.method](...(f.method==='fetchOHLCV'?[i.options]:f.method==='tick'?[i.count??0]:[]));returned=v;result={ok:observe(v)}}catch(e){result={error:e.message}}
 if(releaseMarkets){releaseMarkets();for(let k=0;k<4;k++)await Promise.resolve();}
 unsubscribe();return{...f,trace,result,state:observe({markets:store.markets,tickers:store.tickers,positions:store.positions,orders:store.orders,balance:store.balance,loaded:store.loaded}),errorAlias:f.method==='validateAccount'&&!!returned&&typeof returned.error==='object'&&returned.error===i.hint,fallbackAlias:f.method==='fetchMarkets'&&returned===store.markets};
}
async function apiCases(){
 const axios=dependencies('axios'),retry=dependencies('axios-retry').default??dependencies('axios-retry');const apiFile=path.join(root,'packages/safe-cex/src/exchanges/blofin/blofin.api.ts');const apiAst=ts.createSourceFile(apiFile,fs.readFileSync(apiFile,'utf8'),ts.ScriptTarget.Latest,true);const initializer=apiAst.statements.filter(ts.isVariableStatement).flatMap(s=>s.declarationList.declarations).find(d=>d.name.text==='createAPI').initializer.getText(apiAst);
 const output=[];
 const inputs=[{options:{key:'k',secret:'s'}},{options:{key:'',secret:'',publicDataOnly:true}},{options:{key:'k',secret:'s',passphrase:'p'}},{options:{key:'k',secret:'s',passphrase:'p',testnet:true}},{options:{key:'k',secret:'s',passphrase:'p',remoteSigner:{http:true}}},{options:{key:'k',secret:'s',passphrase:'p',publicDataOnly:true}},{options:{key:'',secret:'s',passphrase:'p'}},{options:{key:'k',secret:'',passphrase:'p'}}];
 for(const url of ['/api/v1/asset/balances','/api/v1/market/instruments','/public/foo','/PUBLIC/foo'])inputs.push({options:{key:'k',secret:'s',passphrase:'p'},config:{url,method:'get',params:{a:'x&=? é',list:['a b','c'],obj:{z:null,n:7}},data:{z:1,a:'é'},headers:{'X-Existing':'keep'}},code:'152401'});
 for(const code of ['152401','152406','0',0,152401,'other',null])inputs.push({options:{key:'k',secret:'s',passphrase:'p'},code});
 inputs.push({options:{key:'k',secret:'s',passphrase:'p'},config:{url:'/account',method:'post',data:'{"x":1}',headers:{'access-sign':'old'}},code:'0'});
 inputs.push({options:{key:'k',secret:'s',passphrase:'p'},config:{url:'/account',method:'',data:false,params:{}},code:'0'});
 for(const [method,failures] of [['get',3],['head',9],['post',9]])inputs.push({options:{key:'k',secret:'s',passphrase:'p'},retry:{method,failures}});
 inputs.push({options:{key:'k',secret:'s',passphrase:'p'},retry:{method:'get',failures:3,code:'152406'}});
 for(const timeout of [undefined,0,5000])inputs.push({options:{key:'k',secret:'s',passphrase:'p'},retry:{method:'get',failures:3,elapsed:7000,timeout}});
 inputs.push({options:{key:'k',secret:'s',passphrase:'p'},mutate:{key:'changed',secret:'changed',passphrase:'changed'}});
 inputs.push({options:{key:'k',secret:'s',passphrase:'p'},retry:{method:'',failures:3}});
 for(const input of inputs){const originalInput=structuredClone(input);const trace=[],runtime={n:0,auth:0};let xhr,defaults;
  const factory=new Function('AxiosHeaders','retry','createHmac','qs','uuid','safeCexAxiosCreate','virtualClock','networkErrorOnRead','BASE_URL','BLOFIN_AUTH_ERROR_CODES',ts.transpileModule('return '+initializer,{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText)(axios.AxiosHeaders,retry,dependencies('create-hmac'),dependencies('qs'),()=> 'nonce-'+runtime.n++,(config)=>{defaults=config;xhr=axios.create(config);return xhr;},{getCurrentTime:()=>1710000000123},load(path.join(root,'packages/safe-cex/src/utils/retryReadsOnly.ts')).networkErrorOnRead,constants.BASE_URL,new Set(['152401','152406']));
  let result;
  try{factory(input.options,{recordAuthFailure(){runtime.auth++;trace.push(['failure']);},recordAuthSuccess(){runtime.auth=0;trace.push(['success']);}});
   if(input.retry){let attempts=0;const request=input.retry;const originalNow=Date.now;let clock=1710000000123;Date.now=()=>clock;xhr.defaults.adapter=async config=>{trace.push(['attempt',config.method,config.url,Object.fromEntries(Object.entries(config.headers)),config.timeout]);clock+=request.elapsed??0;if(attempts++<request.failures){const e=Error('network');e.code='ECONNRESET';e.config=config;throw e;}return{config,status:200,statusText:'OK',headers:{},data:{code:request.code??'0'}};};try{await xhr.request({url:'/account',method:request.method,timeout:request.timeout});result={ok:true}}catch(e){result={error:e.message}}finally{Date.now=originalNow;}}
   else{if(input.mutate)Object.assign(input.options,input.mutate);const config={url:'/account',method:'get',headers:{},...input.config};const signed=xhr.interceptors.request.handlers.filter(h=>h?.fulfilled).at(-1).fulfilled(config);const response={config:signed,data:{code:Object.hasOwn(input,'code')?input.code:'0'}};xhr.interceptors.response.handlers.find(h=>h?.fulfilled).fulfilled(response);result={ok:observe({method:signed.method,url:signed.url,params:signed.params,data:signed.data,headers:Object.fromEntries(Object.entries(signed.headers))})};}
  }catch(e){result={error:e.message}}
  output.push({input:originalInput,defaults,trace,result,auth:runtime.auth,nonceCalls:runtime.n});
 }
 assert.equal(output.length,30,'API independent fixture floor');assert.equal(output[2].nonceCalls,1,'API actual venue interceptor runs');assert.equal(output[2].auth,0,'API signed success');assert.equal(output[8].auth,1,'API signed auth failure');assert.equal(output[9].nonceCalls,0,'API market skips signing');assert.equal(output[21].trace.filter(t=>t[0]==='attempt').length,4,'API GET three retries');assert.equal(output[23].trace.filter(t=>t[0]==='attempt').length,1,'API writes never retried');assert.equal(output[24].auth,4,'API retried auth failure unwinds four times');assert.equal(output[28].result.ok.headers['ACCESS-KEY'],'changed','source reads live key');assert.equal(output[28].result.ok.headers['ACCESS-PASSPHRASE'],'p','source captures destructured passphrase');assert.equal(output[25].trace.filter(t=>t[0]==='attempt').length,3,'source20s budget stops before fourth7s attempt');assert.equal(output[26].trace.filter(t=>t[0]==='attempt').length,4,'source explicit0 timeout stays unlimited');assert.equal(output[27].trace.filter(t=>t[0]==='attempt').length,1,'source explicit5s budget stops after first7s attempt');return output;
}
function constructorCases(){
 const ctor=klass.members.find(ts.isConstructorDeclaration);const fields=klass.members.filter(m=>ts.isPropertyDeclaration(m)&&m.initializer&&!ts.isArrowFunction(m.initializer));const output=[];
 for(const stop of ['', 'super', 'xhr', 'public', 'private']){const trace=[],opts={key:'k',secret:'s',passphrase:'p'},store=new DefaultStore();let result,instance;
  class Base{constructor(name,o,s){trace.push(['super',name]);if(stop==='super')throw Error('super failed');this.store=s;this.options=o;}createXhrWithSecondaryKey(o,factory){assert.equal(o,opts);assert.equal(factory,apiMarker);trace.push(['xhr']);if(stop==='xhr')throw Error('xhr failed');return 'xhr';}}
  function apiMarker(){}
  const socket=kind=>class{constructor(parent){assert.equal(parent.store,store);trace.push([kind]);if(stop===kind)throw Error(kind+' failed');this.kind=kind;}};
  const C=new Function('BaseExchange','createAPI','BlofinPublicWebsocket','BlofinPrivateWebsocket','patternClientOrderIdRule','BROKER_ID','uuid','GRAFANA_ENDPOINTS_PERIODIC','GRAFANA_ENDPOINTS_EVERY',ts.transpileModule('return class extends BaseExchange {'+fields.map(m=>m.getText(ast)).join('\n')+ctor.getText(ast)+'}',{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText)(Base,apiMarker,socket('public'),socket('private'),load(path.join(root,'packages/safe-cex/src/exchanges/clientOrderId.ts')).patternClientOrderIdRule,constants.BROKER_ID,()=> '0123456789abcdef0123456789abcdef',constants.GRAFANA_ENDPOINTS_PERIODIC,constants.GRAFANA_ENDPOINTS_EVERY);
  try{instance=new C('blofin',opts,store);result={ok:{xhr:instance.xhr,public:instance.publicWebsocket.kind,private:instance.privateWebsocket.kind,margin:instance._accountMarginType,mode:instance._accountPositionMode,clientLabel:instance.clientOrderIdRule.label,clientMint:instance.clientOrderIdRule.mint(),periodic:instance.grafanaEndpointsPeriodic,every:instance.grafanaEndpointsEvery}}}catch(e){result={error:e.message}}
  output.push({stop,trace,result});
 }assert.equal(output.length,5,'constructor independent floor');return output;
}
(async()=>{
 assert.equal(fixtures.length,61,'REST/lifecycle independent fixture floor');const cases=[];for(const f of fixtures)cases.push(await run(f));
 assert.equal(cases[6].errorAlias,true,'source validation error retains original body hint alias');
 assert.equal(cases[0].result.ok.balance.total.number,'40599f5c28f5c28f','validation collateral anchor 102.49');
 assert.equal(cases.find(c=>c.method==='fetchMarkets').result.ok[0].limits.amount.min.number,'3f947ae147ae147b','market min in base units 0.02');
 const tickCase=cases.find(c=>c.method==='tick'&&!Object.keys(c.input).length);assert.equal(tickCase.state.positions.length,2,'tick preserves closed-symbol context');assert.equal(tickCase.state.positions[1].leverage.number,'4033000000000000','closed leverage19 retained');
 const boundaries=[];
 for(const [kind,input] of [['nonfinite market',{body:{data:[{...rawMarket,contractValue:'bad'}]}}],['missing base',{body:{data:[{...rawMarket,baseCurrency:undefined}]}}]]){const f=await run({method:'fetchMarkets',input});assert(!f.trace.some(t=>t[0]==='emit'),'source admits '+kind);boundaries.push(f);}
 const invalid=await run({method:'fetchOHLCV',input:{options:{symbol:'BTCUSDT',interval:'1m',to:9e15},body:{data:[]}}});assert(Number.isNaN(invalid.trace.find(t=>t[0]==='get')[2].after),'source sends invalid date');boundaries.push(invalid);
 const sortWitness=await run({method:'fetchOHLCV',input:{options:{symbol:'BTCUSDT',interval:'1m'},body:{data:['3000','2000','bad','1000'].map(t=>[t,'1','2','0','1','x','y','3'])}}});assert.equal(sortWitness.result.ok[0].timestamp.number,'3ff0000000000000','V8 NaN sort witness first timestamp1');boundaries.push(sortWitness);
 const oldTZ=process.env.TZ;process.env.TZ='America/New_York';const dst=await run({method:'fetchOHLCV',input:{options:{symbol:'BTCUSDT',interval:'1d',limit:1,to:1710129600000},body:{data:[]}}});process.env.TZ=oldTZ;assert.equal(dst.trace.find(t=>t[0]==='get')[2].before,1710046800000,'source local calendar subtracts23h across DST');boundaries.push(dst);const cancellation=await run({method:'start',input:{fail:'raw',deferMarkets:true}});assert.equal(cancellation.state.loaded.markets,true,'source market promise continues after peer rejection');boundaries.push(cancellation);
 const audit=[await run({method:'validateAccount',input:{body:{code:'0',data:[{available:'1',frozen:{toString:null}},null]}}})];
 process.stdout.write(JSON.stringify({header:{nodeVersion:process.version,architecture:process.arch,TZ:process.env.TZ},audit,cases,api:await apiCases(),constructors:constructorCases(),boundaries:observe(boundaries),models:{market,position,closed,order,balance,ticker},stoppedAfter:61}));
})().catch(e=>{console.error(e);process.exitCode=1});
