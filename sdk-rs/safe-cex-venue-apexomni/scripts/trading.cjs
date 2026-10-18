const assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
const {createRequire}=require('node:module');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const dependency=createRequire(path.join(process.env.SAFE_CEX_TYPESCRIPT_PATH,'package.json'));
const root=path.resolve(__dirname,'../../../packages/safe-cex/src');
const cache=new Map();
function load(file){if(cache.has(file))return cache.get(file);const module={exports:{}};cache.set(file,module.exports);const code=ts.transpileModule(fs.readFileSync(file,'utf8'),{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;new vm.Script(`(function(require,exports){${code}\n})`,{filename:file}).runInNewContext()((name)=>name==='@tealstreet/logging/src/LogSeverity'?load(path.resolve(root,'../../logging/src/LogSeverity.ts')):name.startsWith('.')?load(path.resolve(path.dirname(file),name+'.ts')):dependency(name),module.exports);return module.exports;}
const enums=load(path.join(root,'types.ts'));const {OrderType,OrderSide,OrderStopTriggerType}=enums;
const tables=load(path.join(root,'exchanges/apexomni/apexomni.types.ts'));
const file=path.join(root,'exchanges/apexomni/apexomni.exchange.ts');
const tree=ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true);
const declarations=tree.statements.filter(n=>ts.isFunctionDeclaration(n)||ts.isVariableStatement(n)||ts.isClassDeclaration(n)).map(n=>n.getText(tree)).join('\n');
const compile=ts.transpileModule(declarations,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;
let operations=[],responses={},clock=1700001234567;
const record=(name,...args)=>operations.push([name,...args]);
let external=false,publicOnly=false,signerFailure=false,tasks=[],listeners=[];
class Base {constructor(name,opts,store){this.name=name;this.options=opts;this.store=store;this.isDisposed=false;this.isPublicDataOnly=publicOnly;}log(message,severity='info'){record('log',message,String(severity).toLowerCase());}dispose(){this.isDisposed=true;}async initExternalPublicData(){record('external:init');return external;}resolveCallerClientOrderId(opts){record('caller:id',opts.clientOrderId);return opts.clientOrderId;}async placeOrders(orders){record('place:orders',orders.map(o=>Object.fromEntries(['symbol','type','side','amount','price','triggerPrice','stopLoss','takeProfit','reduceOnly','timeInForce','triggerType','tpTriggerType','slTriggerType','clientOrderId','maxSlippagePct','close'].filter(k=>o[k]!==undefined).map(k=>[k,o[k]]))));return [];}getTickInterval(){return 321;}emitAndLogError(error){record('error',error instanceof Error?error.message:error);}}
let requestId=0;const makeApi=()=>Object.fromEntries(['get','post'].map(method=>[method,async(endpoint,dataOrConfig,config)=>{record('request',method,endpoint,method==='get'?dataOrConfig?.params:undefined,method==='post'?dataOrConfig:undefined,method==='post'?config?.headers?.['Content-Type']:undefined);if(responses[endpoint] instanceof Error)throw responses[endpoint];if(endpoint==='/v3/order')return {data:{data:{id:'placed-'+(++requestId)}}};return {data:responses[endpoint]};}]));
class PublicSocket{constructor(parent){assert(parent instanceof Base);record('socket:create',false);}dispose(){record('socket:dispose',false);}connectAndSubscribe(){record('socket:connect',false);}listenOHLCV(opts,callback){record('listen:candle',opts);listeners.push(callback);return ()=>record('cleanup:candle');}listenOrderBook(symbol,callback){record('listen:book',symbol);listeners.push(callback);return ()=>record('cleanup:book');}}
class PrivateSocket{constructor(parent){assert(parent instanceof Base);record('socket:create',true);}dispose(){record('socket:dispose',true);}connectAndSubscribe(){record('socket:connect',true);}throttleFetchOrders(){record('socket:throttle-orders');}}
const actualV=load(path.join(root,'utils/get-key.ts'));
// get-key imports lodash, so execute its unchanged functions with actual dependencies.
function v(obj,key){return actualV.v(obj,key);}
const actualMathFile=path.join(root,'utils/safe-math.ts');
const mathModule={exports:{}};const mathCode=ts.transpileModule(fs.readFileSync(actualMathFile,'utf8'),{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;
new vm.Script(`(function(require,exports){${mathCode}\n})`).runInNewContext()(name=>{assert.equal(name,'@tealstreet/math/src/isNotNumber');return load(path.resolve(root,'../../math/src/isNotNumber.ts'));},mathModule.exports);
const sourceModel=load(path.join(root,'utils/createPositionId.ts'));
const context={exports:{},...enums,...tables,BaseExchange:Base,ApexOmniPublicWebsocket:PublicSocket,ApexOmniPrivateWebsocket:PrivateSocket,TextEncoder,
  rateLimit:(api,opts)=>{record('rateLimit',opts.maxRPS);return api;},createAPI:(opts,parent)=>{record('api:create');return makeApi();},patternClientOrderIdRule:(...args)=>args,
  virtualClock:{getCurrentTimeMs:()=>Number(process.env.SAFE_CEX_VIRTUAL_TIME ?? clock)},Date:class extends Date{static now(){return clock;}},v,
  createPositionId:sourceModel.createPositionId,...mathModule.exports,
  uniqBy:dependency('lodash/uniqBy'),chunk:dependency('lodash/chunk'),forEachSeries:dependency('p-iteration').forEachSeries,sleep:async ms=>{record('sleep',ms);},
  omitUndefined:load(path.join(root,'utils/omitUndefined.ts')).omitUndefined,
  BigNumber:dependency('bignumber.js'),toHex:bytes=>Buffer.from(bytes).toString('hex'),sha256:dependency('@noble/hashes/sha2.js').sha256,times:dependency('lodash/times'),inverseObj:Object.fromEntries?obj=>Object.fromEntries(Object.entries(obj).map(([k,v])=>[v,k])):undefined,mapSeries:dependency('p-iteration').mapSeries,qs:dependency('qs'),
  signContract:undefined,
  Math:Object.assign(Object.create(Math),{random:()=>{record('random');return 0.5;}}),
  initSdk:async()=>record('sdk:init'),sdk:{newRpcSignerWithProvider:opts=>{record('signer:create',opts);return {createSignedContract:tx=>{record('contract:sign');return {signature:{signature:'contract-signature'}};},initZklinkSigner:async seed=>{record('signer:init',seed);if(signerFailure)throw new Error('bad seed');}};}},
  loop:(callback,interval)=>{record('loop',interval);tasks.push(callback);},dayjs:(date)=>dependency('dayjs')(date===undefined?clock:date),orderBy:dependency('lodash/orderBy'),
  mergeUpdatesUsingLastUpdate:undefined,
  console:{warn:(...args)=>record('console','warn',...args),error:(...args)=>record('console','error',...args)},
};

const mergeFile=path.join(root,'utils/mergeUpdatesUsingLastUpdate.ts');const mergeCode=ts.transpileModule(fs.readFileSync(mergeFile,'utf8'),{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;const mergeExports={};new vm.Script(`(function(require,exports){${mergeCode}\n})`).runInNewContext()(name=>name==='./virtualClock'?{virtualClock:context.virtualClock}:dependency(name),mergeExports);context.mergeUpdatesUsingLastUpdate=mergeExports.mergeUpdatesUsingLastUpdate;
const realm=vm.createContext(context);const walletArray=vm.runInContext('Array',realm);
context.sdk.ContractBuilder=class{constructor(...args){record('contract:builder',...args);this.args=args;}};context.sdk.newContract=builder=>{assert(builder instanceof context.sdk.ContractBuilder);record('contract:new');return builder;};
const signerDir=path.join(root,'exchanges/apexomni/zklink/signer');const signerSource=['utils.ts','transactions/contract.ts'].map(file=>{const text=fs.readFileSync(path.join(signerDir,file),'utf8'),tree=ts.createSourceFile(file,text,ts.ScriptTarget.Latest,true);return tree.statements.filter(n=>ts.isFunctionDeclaration(n)||ts.isVariableStatement(n)).map(n=>n.getText(tree)).join('\n');}).join('\n');const signerCode=ts.transpileModule(signerSource,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;context.signContract=new vm.Script(signerCode+'\nsignContract').runInNewContext({exports:{},Array:walletArray,sdk:context.sdk,BigNumber:dependency('@ethersproject/bignumber').BigNumber,isAddress:dependency('ethers').isAddress});
const Class=new vm.Script(compile+'\nApexOmniExchange').runInContext(realm);
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
const base={symbol:'BTCUSDC',type:OrderType.Limit,side:OrderSide.Buy,amount:0.006,price:101.25};
const zk={id:'123456',l2Key:'seed',contractAccount:{makerFeeRate:'0.0001',takerFeeRate:'0.0002'},contractConfig:{perpetualContract:[{symbol:'BTC-USDC',l2PairId:'55'}]}};
for(const name of ['placeOrder','updateOrder','cancelSymbolOrders'])for(let variant=0;variant<(name==='placeOrder'?7:3);variant++){
 const storeValue=store();storeValue.updateZkLinkConfig(zk);storeValue.markets[0].limits.amount.max=0.009;requestId=0;operations=[];signerFailure=false;
 responses={'/v3/delete-order':{data:{}},'/v3/delete-open-orders':{data:{}}};const exchange=new Class('apexomni',{key:'key',secret:'secret',passphrase:'pass',omniKeySeed:'seed'},storeValue);operations=[];let args=[],result;
 try{
 if(name==='placeOrder'){
  let opts={...base};if(variant===1)opts={...opts,type:OrderType.Market,side:OrderSide.Sell};if(variant===2)opts={...opts,stopLoss:90,takeProfit:120,tpTriggerType:OrderStopTriggerType.IndexPrice,slTriggerType:OrderStopTriggerType.LastPrice};if(variant===3)opts={...opts,amount:0.012};if(variant===4)opts={...opts,type:OrderType.StopLoss,price:90,triggerPrice:91};if(variant===5)opts={...opts,clientOrderId:'caller-1'};if(variant===6)opts={...opts,amount:0.012,takeProfit:120};args=[opts];result=await exchange.placeOrder(opts);
 }else{
   const mapped=exchange.mapOrder({...order,size:'0.006',cumSuccessFillSize:'0'});storeValue.addOrUpdateOrdersFull(mapped);operations=[];
   if(name==='updateOrder'){const selected=variant===0?mapped[0]:variant===1?mapped.find(o=>o.id.includes('__take_profit')):mapped.find(o=>o.id.includes('__stop_loss'));args=[{order:selected,update:variant===0?{amount:0.003}:{price:variant===1?121:89}}];result=await exchange.updateOrder(...args);}
   else{args=['BTCUSDC',variant!==1];result=await exchange.cancelSymbolOrders(...args);}
 }
 rows.push({name,variant,args:native(args),raw:JSON.stringify(args),result:{ok:true,value:native(result)},operations:native(operations)});
 }catch(error){rows.push({name,variant,args:native(args),raw:JSON.stringify(args),result:{ok:false,message:error.message,name:error.name},operations:native(operations)});}
}
assert.equal(rows.length,13,'trading source floor');for(const row of rows)assert.equal(row.result.ok,!(row.name==='placeOrder'&&row.variant===6),'source trading baseline '+row.name+'/'+row.variant+' '+JSON.stringify(row.result));console.log(JSON.stringify({rows,zk,header:{node:process.version,nodeArch:process.arch,rustArch:process.env.SAFE_CEX_RUST_ARCH}}));}
main().catch(error=>{console.error(error);process.exitCode=1;});
