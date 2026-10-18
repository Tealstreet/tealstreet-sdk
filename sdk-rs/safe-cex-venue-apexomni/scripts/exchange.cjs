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
class Base {constructor(name,opts,store){this.name=name;this.options=opts;this.store=store;this.isDisposed=false;}log(message,severity='info'){record('log',message,String(severity).toLowerCase());}dispose(){this.isDisposed=true;record('base:dispose');}}
const makeApi=()=>Object.fromEntries(['get','post'].map(method=>[method,async(endpoint,dataOrConfig,config)=>{record('request',method,endpoint,method==='get'?dataOrConfig?.params:undefined,method==='post'?dataOrConfig:undefined,method==='post'?config?.headers?.['Content-Type']:undefined);return {data:responses[endpoint]};}]));
class PublicSocket{constructor(parent){assert(parent instanceof Base);record('socket:create',false);}dispose(){record('socket:dispose',false);}}
class PrivateSocket{constructor(parent){assert(parent instanceof Base);record('socket:create',true);}dispose(){record('socket:dispose',true);}}
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
  console:{warn:(...args)=>record('console','warn',...args),error:(...args)=>record('console','error',...args)},
};
const Class=new vm.Script(compile+'\nApexOmniExchange').runInNewContext(context);
function native(value){if(value===undefined)return {kind:'undefined'};if(value===null)return {kind:'null'};if(typeof value==='number'){const b=Buffer.alloc(8);b.writeDoubleBE(value);return {kind:'number',bits:Number.isNaN(value)?'7ff8000000000000':b.toString('hex')};}if(Array.isArray(value))return {kind:'array',values:value.map(native)};if(typeof value==='object')return {kind:'object',values:Object.entries(value).map(([k,v])=>[k,native(v)])};return {kind:typeof value,value};}
const market={id:'BTC-USDC',symbol:'BTCUSDC',base:'BTC',quote:'USDC',active:true,precision:{amount:0.003,price:0.25},limits:{amount:{min:0.006,max:12},leverage:{min:1,max:50}},info:{initialMarginRate:'0.1',defaultInitialMarginRate:'0.05'}};
const ticker={id:'BTC-USDC',symbol:'BTCUSDC',bid:101,ask:102,last:103,mark:104,index:105,percentage:106,volume:107,quoteVolume:108,openInterest:109,fundingRate:110,fundingTime:111};
const position={id:'BTC-USDC:long',symbol:'BTCUSDC',side:'long',entryPrice:91,notional:182,contracts:2,leverage:7,liquidationPrice:23,unrealizedPnl:24};
const rawPosition={symbol:'BTC-USDC',side:'SHORT',avgPrice:'87.25',entryPrice:'89.25',size:'3.25',unrealisedPnl:'4.25',curRealisedPnl:'5.25',liqPrice:'6.25',customImr:'0.2'};
const order={id:'order-1',symbol:'BTC-USDC',type:'LIMIT',status:'OPEN',side:'BUY',price:'101.25',triggerPrice:'0',size:'7.25',cumSuccessFillSize:'2.25',reduceOnly:true,timeInForce:'GOOD_TIL_CANCEL',openSlParams:{triggerPrice:'88.25',triggerPriceType:'INDEX'},openTpParam:{triggerPrice:'122.25',triggerPriceType:'MARKET'}};
const balance={used:31,free:32,total:33,upnl:34};
function store(){const s={markets:[structuredClone(market)],tickers:[structuredClone(ticker)],positions:[structuredClone(position)],orders:[],balance:structuredClone(balance),loaded:{balance:false,orders:false,positions:false,markets:false,tickers:false},zkLinkConfig:{id:'old',l2Key:'old-key'}};s.marketsById={'BTC-USDC':s.markets[0]};s.marketsBySymbol={BTCUSDC:s.markets[0]};s.tickersBySymbol={BTCUSDC:s.tickers[0]};s.updateZkLinkConfig=value=>{s.zkLinkConfig=value;record('store:zk',value);};s.removeOrders=ids=>{record('store:remove',ids);s.orders=s.orders.filter(order=>!ids.some(row=>row.id===order.id));};return s;}
const contract={symbol:'ETH-USDC',crossSymbolName:'ETHUSDC',baseTokenId:'ETH',settleAssetId:'USDC',enableTrade:true,stepSize:'0.007',tickSize:'0.5',minOrderSize:'0.014',maxOrderSize:'28',displayMinLeverage:'2',displayMaxLeverage:'25'};
const account={id:'account',positions:[rawPosition],l2key:'new-key',ethereumAddress:'addr',spotAccount:{a:1},spotWallets:[],contractAccount:{b:2},contractWallets:[]};
async function main(){const rows=[];
const names=['constructor','dispose','accountType','getAccount','fetchRawBalance','fetchBalance','mapPosition','mapOrder','fetchOrders','fetchPositions','fetchMarketsHelper','fetchMarkets','normalizeApexSymbolKey','normalizeExecutionTimestampToSeconds','recordWsExecution','getWsExecutionFallback','cancelOrdersHelper','cancelOrders'];
for(const name of names){for(let variant=0;variant<3;variant++){
 const s=store();operations=[];responses={'/v3/account':structuredClone(account),'/v3/account-balance':{data:{totalEquityValue:'401.25',maintenanceMargin:'51.25',initialMargin:'61.25'}},'/v3/open-orders':{data:[structuredClone(order)]},'/v3/symbols':{data:{contractConfig:{perpetualContract:[contract,{...contract,symbol:'disabled',enableTrade:false}]}}},'/v3/delete-order':{data:{}}};responses['/v3/account']={data:responses['/v3/account']};
 const exchange=new Class('apexomni',{key:'key',secret:'secret',passphrase:'pass'},s);const construction=[...operations];operations=[];let args=[],result;
 try{switch(name){
 case 'constructor':result={margin:exchange.unifiedMarginStatus,network:exchange.networkId,cancelled:exchange.recentlyCancelledOrders.size,executions:exchange.recentWsExecutions.length,accountType:exchange.accountType};operations=construction;break;
 case 'dispose':exchange.dispose();result={disposed:exchange.isDisposed};break;
 case 'accountType':exchange.unifiedMarginStatus=[1,2,NaN][variant];result=exchange.accountType;args=[native(exchange.unifiedMarginStatus)];break;
 case 'getAccount':result=await exchange.getAccount();break;
 case 'fetchRawBalance':case 'fetchBalance':if(variant===1)responses['/v3/account-balance']={};if(variant===2)exchange.isDisposed=true;result=await exchange[name]();break;
 case 'mapPosition':args=[variant===1?{...rawPosition,symbol:'missing'}:variant===2?{...rawPosition,customImr:0,customInitialMarginRate:0,side:'LONG'}:rawPosition];result=exchange.mapPosition(...args);break;
 case 'mapOrder':args=[variant===1?{...order,symbol:'missing'}:variant===2?{...order,side:'sell',postOnly:true,triggerPrice:'91.75'}:order];result=exchange.mapOrder(...args);break;
 case 'fetchOrders':if(variant===1)exchange.recentlyCancelledOrders.set('order-1',clock-9999);if(variant===2)exchange.recentlyCancelledOrders.set('order-1',clock-10001);result=await exchange.fetchOrders();break;
 case 'fetchPositions':if(variant===1)s.zkLinkConfig={id:'old'};if(variant===2)responses['/v3/account'].data.positions=[];result=await exchange.fetchPositions();break;
 case 'fetchMarketsHelper':case 'fetchMarkets':result=await exchange[name]();break;
 case 'normalizeApexSymbolKey':args=[['perp-BTC_usdc-perp','ETH/USDT-SWAP',false][variant]];result=exchange[name](...args);break;
 case 'normalizeExecutionTimestampToSeconds':args=[[1700000000123,'2026-10-13T00:00:00Z','bad'][variant]];result=exchange[name](...args);break;
 case 'recordWsExecution':args=[{symbol:'PERP-BTCUSDC-SWAP',side:'buy',createdAt:1700000000123,price:'83.25',fillPrice:'99.25',size:'2.25',amount:'9.25',fee:'-0.75',orderId:variant===1?'undefined':'row-1'}];if(variant===2)args[0].price='0';exchange.recordWsExecution(...args);result=exchange.recentWsExecutions;break;
 case 'getWsExecutionFallback':exchange.recordWsExecution({symbol:'BTC-USDC',side:'BUY',createdAt:1700000000123,price:'83.25',size:'2.25',fee:'-0.75',orderId:'row-1'});args=[{symbol:'PERP-BTCUSDC-SWAP',orderId:variant===1?'different':undefined,fromMilliseconds:variant===2?1700000001000:undefined,limit:5}];result=exchange[name](...args);break;
 case 'cancelOrdersHelper':case 'cancelOrders':{const mapped=exchange.mapOrder(order);operations=[];args=[variant===0?mapped:variant===1?mapped.slice(1):Array.from({length:12},(_,i)=>({...mapped[0],id:`id-${i}`}))];result=await exchange[name](...args);break;}
 }
 rows.push({name,variant,args:native(args),operations:native(operations),result:{ok:true,value:native(result)},cancelled:native([...exchange.recentlyCancelledOrders]),zk:native(s.zkLinkConfig)});
 }catch(error){throw new Error(`${name}/${variant} source failed: ${error.stack}`);}
}}
assert.equal(rows.length,54,'exchange case floor');console.log(JSON.stringify({rows,fixtures:{market,ticker,position,balance,contract,account,order,rawPosition},header:{node:process.version,nodeArch:process.arch,rustArch:process.env.SAFE_CEX_RUST_ARCH}}));}
async function retainedInfo() {
 const opaque={text:String.fromCharCode(0xd800),nan:NaN,infinity:Infinity,negativeZero:-0,ownUndefined:undefined};
 const assertOpaque=value=>{assert.equal(value.text.charCodeAt(0),0xd800);assert.equal(value.text.length,1);assert.ok(Number.isNaN(value.nan));assert.equal(value.infinity,Infinity);assert.ok(Object.is(value.negativeZero,-0));assert.ok(Object.hasOwn(value,'ownUndefined'));assert.equal(value.ownUndefined,undefined);};
 const s=store(),exchange=new Class('apexomni',{key:'key',secret:'secret',passphrase:'pass'},s);
 responses['/v3/open-orders']={code:0,data:[{...order,openSlParams:{triggerPrice:'0'},openTpParam:{triggerPrice:'0'},opaque},{...order,openSlParams:{triggerPrice:'0'},openTpParam:{triggerPrice:'0'}}]};
 const orders=await exchange.fetchOrders();assert.equal(orders.length,2);assertOpaque(orders[0].info.opaque);assert.ok(!Object.hasOwn(orders[1].info,'opaque'));
 responses['/v3/account-balance']={code:0,data:{totalEquityValue:'100',maintenanceMargin:'2',initialMargin:'10',opaque}};
 assertOpaque((await exchange.fetchRawBalance()).info.opaque);
 responses['/v3/symbols']={code:0,data:{contractConfig:{perpetualContract:[{...contract,opaque},contract],opaque}}};
 const markets=await exchange.fetchMarkets();assert.equal(markets.length,2);assertOpaque(markets[0].info.opaque);assertOpaque(s.zkLinkConfig.contractConfig.opaque);assertOpaque(s.zkLinkConfig.contractConfig.perpetualContract[0].opaque);
 console.log('actual Apex TS retained order/balance/market/config PASS');
}
(process.argv.includes('--retained-info')?retainedInfo():main()).catch(error=>{console.error(error);process.exitCode=1;});
