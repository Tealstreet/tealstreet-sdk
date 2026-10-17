const assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
const {createRequire}=require('node:module');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const dependency=createRequire(path.join(process.env.SAFE_CEX_TYPESCRIPT_PATH,'package.json'));
const root=path.resolve(__dirname,'../../../packages/safe-cex/src');
const cache=new Map();
function load(file){if(cache.has(file))return cache.get(file);const module={exports:{}};cache.set(file,module.exports);const code=ts.transpileModule(fs.readFileSync(file,'utf8'),{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;new vm.Script(`(function(require,exports){${code}\n})`,{filename:file}).runInNewContext()((name)=>name==='@tealstreet/logging/src/LogSeverity'?load(path.resolve(root,'../../logging/src/LogSeverity.ts')):name.startsWith('.')?load(path.resolve(path.dirname(file),name+'.ts')):dependency(name),module.exports);return module.exports;}
const enums=load(path.join(root,'types.ts'));
const tables=load(path.join(root,'exchanges/apexomni/apexomni.types.ts'));
const file=path.join(root,'exchanges/apexomni/apexomni.exchange.ts');
const tree=ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true);
const declarations=tree.statements.filter(n=>ts.isFunctionDeclaration(n)||ts.isVariableStatement(n)||ts.isClassDeclaration(n)).map(n=>n.getText(tree)).join('\n');
const compile=ts.transpileModule(declarations,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;
let operations=[],responses={},clock=1700001234567;
const record=(name,...args)=>operations.push([name,...args]);
let external=false,publicOnly=false,signerFailure=false,tasks=[],listeners=[];
class Base {constructor(name,opts,store){this.name=name;this.options=opts;this.store=store;this.isDisposed=false;this.isPublicDataOnly=publicOnly;}log(message,severity='info'){record('log',message,String(severity).toLowerCase());}dispose(){this.isDisposed=true;}async initExternalPublicData(){record('external:init');return external;}getTickInterval(){return 321;}emitAndLogError(error){record('error',error instanceof Error?error.message:error);}}
const makeApi=()=>Object.fromEntries(['get','post'].map(method=>[method,async(endpoint,dataOrConfig,config)=>{record('request',method,endpoint,method==='get'?dataOrConfig?.params:undefined,method==='post'?dataOrConfig:undefined,method==='post'?config?.headers?.['Content-Type']:undefined);if(responses[endpoint] instanceof Error)throw responses[endpoint];return {data:responses[endpoint]};}]));
class PublicSocket{constructor(parent){assert(parent instanceof Base);record('socket:create',false);}dispose(){record('socket:dispose',false);}connectAndSubscribe(){record('socket:connect',false);}listenOHLCV(opts,callback){record('listen:candle',opts);listeners.push(callback);return ()=>record('cleanup:candle');}listenOrderBook(symbol,callback){record('listen:book',symbol);listeners.push(callback);return ()=>record('cleanup:book');}}
class PrivateSocket{constructor(parent){assert(parent instanceof Base);record('socket:create',true);}dispose(){record('socket:dispose',true);}connectAndSubscribe(){record('socket:connect',true);}}
const actualV=load(path.join(root,'utils/get-key.ts'));
// get-key imports lodash, so execute its unchanged functions with actual dependencies.
function v(obj,key){return actualV.v(obj,key);}
const actualMathFile=path.join(root,'utils/safe-math.ts');
const mathModule={exports:{}};const mathCode=ts.transpileModule(fs.readFileSync(actualMathFile,'utf8'),{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;
new vm.Script(`(function(require,exports){${mathCode}\n})`).runInNewContext()(name=>{assert.equal(name,'@tealstreet/math/src/isNotNumber');return load(path.resolve(root,'../../math/src/isNotNumber.ts'));},mathModule.exports);
const sourceModel=load(path.join(root,'utils/createPositionId.ts'));
const context={exports:{},...enums,...tables,BaseExchange:Base,ApexOmniPublicWebsocket:PublicSocket,ApexOmniPrivateWebsocket:PrivateSocket,TextEncoder,
  rateLimit:(api,opts)=>{record('rateLimit',opts.maxRPS);return api;},createAPI:(opts,parent)=>{record('api:create');return makeApi();},patternClientOrderIdRule:(...args)=>args,
  virtualClock:{getCurrentTimeMs:()=>clock},Date:class extends Date{static now(){return clock;}},v,
  createPositionId:sourceModel.createPositionId,...mathModule.exports,
  uniqBy:dependency('lodash/uniqBy'),chunk:dependency('lodash/chunk'),forEachSeries:dependency('p-iteration').forEachSeries,sleep:async ms=>{record('sleep',ms);},
  omitUndefined:load(path.join(root,'utils/omitUndefined.ts')).omitUndefined,
  initSdk:async()=>record('sdk:init'),sdk:{newRpcSignerWithProvider:opts=>{record('signer:create',opts);return {initZklinkSigner:async seed=>{record('signer:init',seed);if(signerFailure)throw new Error('bad seed');}};}},
  loop:(callback,interval)=>{record('loop',interval);tasks.push(callback);},dayjs:(date)=>dependency('dayjs')(date===undefined?clock:date),orderBy:dependency('lodash/orderBy'),
  mergeUpdatesUsingLastUpdate:undefined,
  console:{warn:(...args)=>record('console','warn',...args),error:(...args)=>record('console','error',...args)},
};

const mergeFile=path.join(root,'utils/mergeUpdatesUsingLastUpdate.ts');const mergeCode=ts.transpileModule(fs.readFileSync(mergeFile,'utf8'),{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;const mergeExports={};new vm.Script(`(function(require,exports){${mergeCode}\n})`).runInNewContext()(name=>name==='./virtualClock'?{virtualClock:context.virtualClock}:dependency(name),mergeExports);context.mergeUpdatesUsingLastUpdate=mergeExports.mergeUpdatesUsingLastUpdate;
const Class=new vm.Script(compile+'\nApexOmniExchange').runInNewContext(context);
function native(value){if(value===undefined)return {kind:'undefined'};if(value===null)return {kind:'null'};if(typeof value==='number'){const b=Buffer.alloc(8);b.writeDoubleBE(value);return {kind:'number',bits:Number.isNaN(value)?'7ff8000000000000':b.toString('hex')};}if(Array.isArray(value))return {kind:'array',values:value.map(native)};if(typeof value==='object')return {kind:'object',values:Object.entries(value).map(([k,v])=>[k,native(v)])};return {kind:typeof value,value};}
const market={id:'BTC-USDC',symbol:'BTCUSDC',base:'BTC',quote:'USDC',active:true,precision:{amount:0.003,price:0.25},limits:{amount:{min:0.006,max:12},leverage:{min:1,max:50}},info:{initialMarginRate:'0.1',defaultInitialMarginRate:'0.05'}};
const ticker={id:'BTC-USDC',symbol:'BTCUSDC',bid:101,ask:102,last:103,mark:104,index:105,percentage:106,volume:107,quoteVolume:108,openInterest:109,fundingRate:110,fundingTime:111};
const position={id:'BTC-USDC:long',symbol:'BTCUSDC',side:'long',entryPrice:91,notional:182,contracts:2,leverage:7,liquidationPrice:23,unrealizedPnl:24};
const rawPosition={symbol:'BTC-USDC',side:'SHORT',avgPrice:'87.25',entryPrice:'89.25',size:'3.25',unrealisedPnl:'4.25',curRealisedPnl:'5.25',liqPrice:'6.25',customImr:'0.2'};
const order={id:'order-1',symbol:'BTC-USDC',type:'LIMIT',status:'OPEN',side:'BUY',price:'101.25',triggerPrice:'0',size:'7.25',cumSuccessFillSize:'2.25',reduceOnly:true,timeInForce:'GOOD_TIL_CANCEL',openSlParams:{triggerPrice:'88.25',triggerPriceType:'INDEX'},openTpParam:{triggerPrice:'122.25',triggerPriceType:'MARKET'}};
const balance={used:31,free:32,total:33,upnl:34};
const storeFile=path.join(root,'store/store.base.ts');const storeExports={};const storeCode=ts.transpileModule(fs.readFileSync(storeFile,'utf8'),{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;new vm.Script(`(function(require,exports){${storeCode}\n})`).runInNewContext()(name=>name==='../utils/virtualClock'?{virtualClock:context.virtualClock}:name==='../types'?enums:name.startsWith('.')?load(path.resolve(path.dirname(storeFile),name+'.ts')):dependency(name),storeExports);
function store(){const s=new storeExports.DefaultStore();s.update({markets:[structuredClone(market)],tickers:[structuredClone(ticker)],positions:[structuredClone(position)],balance:structuredClone(balance),zkLinkConfig:{id:'old',l2Key:'old-key'}});return s;}

const contract={symbol:'ETH-USDC',crossSymbolName:'ETHUSDC',baseTokenId:'ETH',settleAssetId:'USDC',enableTrade:true,stepSize:'0.007',tickSize:'0.5',minOrderSize:'0.014',maxOrderSize:'28',displayMinLeverage:'2',displayMaxLeverage:'25'};
const account={id:'account',positions:[rawPosition],l2key:'new-key',ethereumAddress:'addr',spotAccount:{a:1},spotWallets:[],contractAccount:{b:2},contractWallets:[]};
let socketPrivate=false,connected=true,disposed=false,nextTimer=1,timers=new Map(),pingTimer,route,remoteMode=0,socket,socketListeners=new Map();
function timer(delay,callback){const id=nextTimer++;timers.set(id,callback);record('timeout',delay,id);return id;}
function clear(id){record('clear',id);timers.delete(id);}
function decodedFrame(frame){const json=JSON.parse(frame);if(json.op==='login')json.args=json.args.map(arg=>JSON.parse(arg));return json;}
class SocketBase {
 constructor(parent){this.parent=parent;this.store=parent.store;this.emitter={emit:(event,data)=>record('emit',event,data)};this.pingTimeoutId=undefined;}
 get isDisposed(){return disposed;}get isConnected(){return connected;}
 set pingAt(at){record('pingAt',at);}
 createWebsocket(url){record('dial',url);return {addEventListener:(name,callback)=>{record('listener',name);socketListeners.set(name,callback);}};}
 onClose=()=>{};
 send(frame){record('send',decodedFrame(frame),false);}
 sendWithLog(frame){record('send',decodedFrame(frame),true);}
 trackLastReceived(){record('received');}
 emitWsDiagnosticReceive(frame){record('diagnostic',frame);}
 recordMessageHandlerStats(topic){record('stats',topic);}
 cleanupMessageHandlerStats(topic){record('cleanup',topic);}
 handlePongEvent(delay){record('pong',delay);}
 emitWsReconnected(){record('reconnected');}
 reconnect(){record('reconnect');}
 emitAndLogError(error){record('error',error instanceof Error?error.message:String(error));}
 dispose(){record('base:dispose');disposed=true;if(this.pingTimeoutId){clear(this.pingTimeoutId);this.pingTimeoutId=undefined;}}
}
const wsContext={exports:{},BaseWebSocket:SocketBase,...enums,...tables,ENABLE_RECONNECTING_WEBSOCKET:false,
 virtualClock:context.virtualClock,performance:{now:()=>222.25},Date:context.Date,Buffer,
 nanoid:()=>{record('nanoid');return 'request-id';},chunk:dependency('lodash/chunk'),flatten:dependency('lodash/flatten'),
 jsonParse:load(path.resolve(root,'../../json-parse/src/jsonParse.ts')).jsonParse,
 getCandleStartTime:load(path.join(root,'utils/getCandleStartTime.ts')).getCandleStartTime,
 calcOrderBookTotal:load(path.join(root,'utils/orderbook.ts')).calcOrderBookTotal,
 updateOrderBook:load(path.join(root,'utils/updateOrderBook.ts')).updateOrderBook,...mathModule.exports,v,
 createHmac:dependency('create-hmac'),resolveTransportRouting:(name,options)=>{record('route',name,{key:options.key,secret:options.secret,passphrase:options.passphrase,testnet:options.testnet,publicDataOnly:options.publicDataOnly});return {wsPrivate:{tunnelled:!!route,host:route}};},
 setTimeout:(callback,delay)=>timer(delay,callback),clearTimeout:clear,
 console:{warn:(message,arg)=>record('warn',message,arg)},
};
function wsClass(name){const file=path.join(root,'exchanges/apexomni/apexomni.ws-'+name+'.ts');const tree=ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true);const declaration=tree.statements.find(ts.isClassDeclaration).getText(tree);const code=ts.transpileModule(declaration,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;return new vm.Script(code+'\nApexOmni'+(name==='public'?'Public':'Private')+'Websocket').runInNewContext(wsContext);}
const Public=wsClass('public'),Private=wsClass('private');
const publicNames=['class','constructor','connect','open','ping','subscribe','message','instrument','candle','book'];
const privateNames=['class','ordersThrottle','positionsThrottle','connect','open','message','ping','accounts','fills','orders','positions','auth','subscribe','dispose'];
function inputFor(name,variant){
 const instrument={data:[{s:'BTCUSDC',p:'117.25',mp:'121.5',xp:'119.75',pr:'0.0325',o:'2.5',fr:'0.0075',v:'3.5',to:'4.5'}]};
 if(name==='instrument'){if(variant===1)instrument.data=[{s:'BTC-USDC',xp:'108.25'},{s:'BTCUSDC',mp:'110.25'}];if(variant===2)instrument.data=[{s:'missing'},{s:'BTCUSDC',mp:'0',xp:'112.25',p:'0'}];return instrument;}
 const fill={symbol:'BTC-USDC',side:'BUY',createdAt:clock,price:'87.25',size:'2.25',fee:'-0.125',orderId:'fill-1',token:'USDC'};
 const raw={...structuredClone(order),updatedAt:clock,createdAt:clock,status:variant===1?'CANCELED':variant===2?'OPEN':'FILLED',cancelReason:'ACCOUNT_BALANCE_INSUFFICIENT',cumSuccessFillFee:'0.125',latestMatchFillPrice:'99.25',token:'USDC'};
 if(name==='fills')return variant===1?[]:variant===2?[fill,{...fill,orderId:'fill-2',size:'3.25'}]:[fill];
 if(name==='orders')return [raw];
 if(name==='positions')return variant===1?[]:variant===2?[{...rawPosition,symbol:'missing'},rawPosition]:[rawPosition];
 if(name==='accounts')return {contents:{fills:[fill],orders:[raw],positions:[rawPosition]}};
 if(name==='message'&&socketPrivate)return variant===0?JSON.stringify({op:'ping',args:['server-clock',1],pong:null}):variant===1?JSON.stringify({topic:'ws_zk_accounts_v3',contents:{fills:[fill]},success:false,ret_msg:'bad request'}):'bad json';
 if(name==='message')return variant===0?JSON.stringify({...instrument,topic:'instrumentInfo.all'}):variant===1?'op":"ping something pong':'{"topic":"instrumentInfo.all",invalid';
 return undefined;
}
async function fire(){const first=timers.entries().next().value;if(first){timers.delete(first[0]);await first[1]();}}
async function main(){const rows=[];
for(const privacy of ['public','private'])for(const name of privacy==='public'?publicNames:privateNames)for(let variant=0;variant<3;variant++){
 socketPrivate=privacy==='private';connected=true;disposed=false;nextTimer=1;timers=new Map();socketListeners=new Map();pingTimer=undefined;route=variant===1?'wss://tunnel.example/private':undefined;remoteMode=variant;wsContext.ENABLE_RECONNECTING_WEBSOCKET=variant===1;operations=[];
 responses={'/v3/account':{data:structuredClone(account)},'/v3/open-orders':{data:[structuredClone(order)]}};
 const state=store();if(['positions','accounts'].includes(name)){const p=structuredClone(position);p.id='BTC-USDC:short';p.side='short';state.update({positions:[p]});}
 const options={key:'key',secret:'secret-é',passphrase:variant===2?undefined:'pass',testnet:variant===1,publicDataOnly:name==='candle'&&variant===1};
 if(['auth','open'].includes(name)&&socketPrivate&&variant>0){options.remoteSigner={accountName:'remote-account',sign:async request=>{record('remoteSign',request);if(variant===2)return {error:'browser offline'};return {wsAuthSignature:'remote-signature'};}};}
 const parent=new Class('apexomni',options,state);parent.fetchAndSetPrivateSnapshot=()=>{record('spawn');return new Promise(()=>{});};
 operations=[];socket=new (socketPrivate?Private:Public)(parent);record('initialize',socketPrivate,socket.treatAnyMessageAsPong===true);if(name!=='class'&&name!=='constructor')operations=[];
 if(['connect','open','ping'].includes(name)&&variant===2&&!socketPrivate)disposed=true;
 if(name==='orders'||name==='accounts')socket.lastConnectTime=clock-(variant===2?1000:6000);
 if(name==='orders'&&variant===2)parent.recentlyCancelledOrders.set('order-1',clock);
 if(name==='candle'||name==='book')connected=variant!==2;
 const input=inputFor(name,variant);let result;
 if(name==='class'||name==='constructor'){socket.subscribe();if(!socketPrivate)socket.onMessage({data:'pong'});}
 else if(name==='connect'){socket.connectAndSubscribe();if(socketListeners.has('open')){await socketListeners.get('open')();socketListeners.get('message')({data:'{"op":"ping"}'});}}
 else if(name==='open')await socket.onOpen();
 else if(name==='ping'){socket.ping();if(variant===1)socket.ping();}
 else if(name==='subscribe'){if(!socketPrivate&&variant===1){for(let i=0;i<51;i++)socket.listenOrderBook('BOOK'+i,()=>{});operations=[];}socket.subscribe();}
 else if(name==='message')socket.onMessage({data:input});
 else if(name==='instrument')socket.handleInstrumentInfoEvent(input);
 else if(name==='auth')result=await socket.auth();
 else if(name==='fills')socket.handleFillTopic(input);
 else if(name==='orders'){socket.handleOrderTopic(input);if(variant===0)socket.handleOrderTopic(input);}
 else if(name==='positions'){try{socket.handlePositionTopic(input);}catch(error){result={errorName:error.name,errorMessage:error.message};}}
 else if(name==='accounts')socket.handleWsAccountsV2Topic(input);
 else if(name==='ordersThrottle'||name==='positionsThrottle'){
  const method=name==='ordersThrottle'?'throttleFetchOrders':'throttleFetchPositions';
  if(variant===1)responses[name==='ordersThrottle'?'/v3/open-orders':'/v3/account']=new Error('reconcile unavailable');
  if(variant===2){const xhr=parent.xhr;const get=xhr.get;xhr.get=async(...args)=>{const r=await get(...args);disposed=true;return r;};}
  socket[method]();socket[method]();await fire();
 }
 else if(name==='dispose'){socket.throttleFetchOrders();socket.throttleFetchPositions();socket.ping();socket.dispose();}
 else if(name==='candle'){
  const options={symbol:'BTCUSDC',interval:'1m'},cleanup=socket.listenOHLCV(options,value=>record('candle',structuredClone(value)));
  if(variant===2){cleanup();connected=true;await fire();}
  const t=Math.floor(clock/60000)*60000;
  socket.onMessage({data:JSON.stringify({topic:'candle.1.BTCUSDC',data:[{start:t,open:'10.25',high:'12.25',low:'9.25',close:'11.25',volume:'7.25'}]})});
  socket.onMessage({data:JSON.stringify({topic:'recentlyTrade.H.BTCUSDC',data:[{T:t+1000,p:20,v:200}]})});
  socket.onMessage({data:JSON.stringify({topic:'recentlyTrade.H.BTCUSDC',data:[{T:t-1000,p:21,v:210}]})});
  socket.onMessage({data:JSON.stringify({topic:'recentlyTrade.H.BTCUSDC',data:[{T:t+61000,p:22,v:220}]})});
  socket.onMessage({data:JSON.stringify({topic:'recentlyTrade.H.BTCUSDC',data:[{T:t+62000,p:3,v:'1.12345678900'}]})});
  cleanup();socket.subscribe();
 }
 else if(name==='book'){
  const cleanup=socket.listenOrderBook('BTCUSDC',value=>record('book',structuredClone(value)));
  if(variant===2){cleanup();connected=true;await fire();}
  socket.onMessage({data:JSON.stringify({topic:'orderBook200.H.BTCUSDC',type:'snapshot',data:{a:[['102.25','2.5'],['103.25','3.5']],b:[['101.25','4.5'],['100.25','5.5']]}})});
  socket.onMessage({data:JSON.stringify({topic:'orderBook200.H.BTCUSDC',type:'delta',data:{a:[['102.25','0'],['104.25','6.5']],b:[['101.25','7.5']]}})});
  cleanup();socket.subscribe();
 }
 rows.push({privacy,name,variant,input:native(input),result:native(result),operations:native(operations),state:native({tickers:state.tickers.map(context.omitUndefined),positions:state.positions.map(context.omitUndefined),orders:state.orders.map(context.omitUndefined),executions:parent.recentWsExecutions}),lastConnect:socket.lastConnectTime,lastPositions:parent.lastWebsocketPositionsUpdate,timers:timers.size});
}
assert.equal(rows.length,72,'WebSocket actual-source floor');for(const name of publicNames)assert.equal(rows.filter(r=>r.privacy==='public'&&r.name===name).length,3);for(const name of privateNames)assert.equal(rows.filter(r=>r.privacy==='private'&&r.name===name).length,3);
disposed=false;connected=true;operations=[];const chosenValues=[];const chosenParent=new Class('apexomni',{key:'key',secret:'secret'},store());const chosenSocket=new Public(chosenParent);chosenSocket.listenOHLCV({symbol:'BTCUSDC',interval:'1m'},c=>chosenValues.push(structuredClone(c)));const t=Math.floor(clock/60000)*60000;chosenSocket.onMessage({data:JSON.stringify({topic:'candle.1.BTCUSDC',data:[{start:t,open:'10.25',high:'12.25',low:'9.25',close:'11.25',volume:'7.25'}]})});chosenSocket.onMessage({data:JSON.stringify({topic:'recentlyTrade.H.BTCUSDC',data:[{T:t+1000,p:'20.25',v:'202.5'}]})});assert.equal(chosenValues.length,2,'source accepts string close');assert.equal(chosenValues[1].close,'20.25','source retains raw string price');
console.log(JSON.stringify({rows,chosenCandle:native(chosenValues),header:{node:process.version,nodeArch:process.arch,rustArch:process.env.SAFE_CEX_RUST_ARCH}}));}
main().catch(error=>{console.error(error);process.exitCode=1;});
