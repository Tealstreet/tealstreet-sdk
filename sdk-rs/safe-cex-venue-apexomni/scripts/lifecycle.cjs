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
async function main(){const rows=[];
const names=['validateAccount','start','fetchAndSetPrivateSnapshot','fetchAndSetPrivateSnapshotHelper','tick','fetchOHLCV','listenOHLCV','listenOrderBook','setLeverage'];
for(const name of names)for(let variant=0;variant<(name==='fetchOHLCV'?4:3);variant++){
 external=variant===1;publicOnly=variant===2;tasks=[];listeners=[];operations=[];responses={'/v3/user':{data:{}},'/v3/account':{data:structuredClone(account)},'/v3/account-balance':{data:{totalEquityValue:'401.25',maintenanceMargin:'51.25',initialMargin:'61.25'}},'/v3/open-orders':{data:[structuredClone(order)]},'/v3/symbols':{data:{contractConfig:{perpetualContract:[contract]}}},'/v3/klines':{data:{BTCUSDC:[{t:2000,o:'10.25',h:'12.25',l:'9.25',c:'11.25',v:'7.25'},{t:1000,o:'20.25',h:'22.25',l:'19.25',c:'21.25',v:'8.25'}]}},'/v3/set-initial-margin-rate':{data:{}}};
 if(name==='validateAccount'&&variant===1){const error=new Error('network cause');error.response={data:{msg:'bad key'}};responses['/v3/user']=error;}signerFailure=name==='validateAccount'&&variant===2;
 const storeValue=store(),exchange=new Class('apexomni',{key:'key',secret:'secret',passphrase:'pass'},storeValue);operations=[];let args=[],result;
 if(variant===2&&['fetchAndSetPrivateSnapshot','fetchAndSetPrivateSnapshotHelper','tick'].includes(name))exchange.isDisposed=true;
 if(variant===1&&name.includes('Snapshot')){exchange.lastWebsocketPositionsUpdate=clock-500;storeValue.positions[0].lastUpdate=clock-500;storeValue.positions[0].id='BTC-USDC:short';storeValue.positions[0].side='short';}
 let notifications=0;storeValue.subscribe(()=>{notifications++;});
 switch(name){
 case 'validateAccount':result=await exchange.validateAccount();break;
 case 'start':await exchange.start();result=undefined;break;
 case 'fetchAndSetPrivateSnapshot':case 'fetchAndSetPrivateSnapshotHelper':await exchange[name]();break;
 case 'tick':await exchange.tick();break;
 case 'fetchOHLCV':args=[{symbol:'BTCUSDC',interval:variant===1?'1d':'1m',limit:variant===2?0:variant===3?0.000013333333333333333:3,from:variant===2?1700000000000:undefined,to:variant===2?undefined:variant===3?1000.9:1700001234567}];result=await exchange.fetchOHLCV(...args);break;
 case 'listenOHLCV':{args=[{symbol:'BTCUSDC',interval:'1m'}];const cleanup=exchange.listenOHLCV(args[0],value=>record('callback:candle',value));listeners[0]({timestamp:1,open:2,high:3,low:4,close:5,volume:6});cleanup();break;}
 case 'listenOrderBook':{args=['BTCUSDC'];const cleanup=exchange.listenOrderBook(args[0],value=>record('callback:book',value));listeners[0]({bids:[],asks:[]});cleanup();break;}
 case 'setLeverage':args=['BTCUSDC',[12,100,7][variant]];await exchange.setLeverage(...args);break;
 }
 await new Promise(resolve=>setImmediate(resolve));
 rows.push({name,variant,args:native(args),operations:native(operations),result:native(result),state:native({markets:storeValue.markets,tickers:storeValue.tickers,positions:storeValue.positions.map(context.omitUndefined),orders:storeValue.orders.map(context.omitUndefined),balance:storeValue.balance,loaded:storeValue.loaded}),tasks:tasks.length,notifications});
}
assert.equal(rows.length,28,'lifecycle source floor');console.log(JSON.stringify({rows,header:{node:process.version,nodeArch:process.arch,rustArch:process.env.SAFE_CEX_RUST_ARCH}}));}
main().catch(error=>{console.error(error);process.exitCode=1;});
