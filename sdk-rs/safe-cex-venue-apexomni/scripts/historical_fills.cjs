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
const baseFile=path.join(root,'exchanges/base.ts');
const baseTree=ts.createSourceFile(baseFile,fs.readFileSync(baseFile,'utf8'),ts.ScriptTarget.Latest,true);
const baseClass=baseTree.statements.find(n=>ts.isClassDeclaration(n)&&n.name?.text==='BaseExchange');
const getterDeclarations=baseClass.members.filter(n=>ts.isGetAccessorDeclaration(n)&&['canSign','isPublicDataOnly'].includes(n.name.getText(baseTree))).map(n=>n.getText(baseTree));
assert.equal(getterDeclarations.length,2,'actual Base authentication getters');
const gettersCode=ts.transpileModule(`class AuthGetters{${getterDeclarations.join('\n')}}`,{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText;
const getterDescriptors=Object.getOwnPropertyDescriptors(new vm.Script(gettersCode+'\nAuthGetters').runInNewContext().prototype);delete getterDescriptors.constructor;Object.defineProperties(Base.prototype,getterDescriptors);
const makeApi=()=>Object.fromEntries(['get','post'].map(method=>[method,async(endpoint,dataOrConfig,config)=>{record('request',method,endpoint,method==='get'?dataOrConfig?.params:undefined,method==='post'?dataOrConfig:undefined,method==='post'?config?.headers?.['Content-Type']:undefined,method==='get'?dataOrConfig?.timeout:undefined);if(responses[endpoint] instanceof Error)throw responses[endpoint];return {data:responses[endpoint]};}]));
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
async function main(){const cases=[];
const fill={symbol:'BTC-USDC',side:'BUY',createdAt:1700000000000,fillPrice:'83.25',fillSize:'2.25',fee:'-0.75',orderId:'rest',token:'USDC'};
const cached={symbol:'BTC-USDC',side:'BUY',createdAt:1700000000000,price:'19.25',size:'0.25',fee:'-0.05',orderId:'ws',token:'USDC'};
for(let variant=0;variant<13;variant++){
 const s=store();operations=[];let options={symbol:'BTCUSDC',limit:100},configured;
 responses={'/v3/fills':{data:{fills:[structuredClone(fill)]}},'/v3/history-orders':{data:{orders:[]}}};
 if(variant===1){responses['/v3/fills']=new Error('fills unavailable');responses['/v3/history-orders']={body:{result:{rows:[{...fill,side:'\uFEFFSHORT\uFEFF',cumSuccessFillSize:'4.25',fillSize:'1.25',avgPrice:'91.25',fillPrice:'',cumSuccessFillValue:'425',fillFee:'-0.125'}]}}};configured='4321.5';}
 if(variant===2){responses['/v3/fills']={data:{fills:[]}};}
 if(variant===3){options={symbol:'BTCUSDC',from:1700000000,to:1700001000,limit:2};responses['/v3/fills']={result:{list:[fill,{...fill,createdAt:1700000001000,orderId:'rest-2'}]}};}
 if(variant===4)options={};
 if(variant===5)options={symbol:'missing'};
 if(variant===6){options={symbol:'BTCUSDC',orderId:'ws'};}
 if(variant===7){responses['/v3/fills']=new Error('fills down');responses['/v3/history-orders']=new Error('history down');}
 if(variant===8){responses['/v3/fills']={data:[null]};}
 if(variant===9){options={symbol:'BTCUSDC',limit:0};responses['/v3/fills']={data:{rows:[{...fill,side:'\uFEFFBUY\uFEFF'},fill]}};configured=Infinity;}
 if(variant===12)responses['/v3/fills']=null;
 const exchange=new Class('apexomni',{key:variant===11?'':'key',secret:'secret',passphrase:'pass',publicDataOnly:variant===10,extra:{executionHistoryTimeoutMs:configured}},s);exchange.recordWsExecution(cached);operations=[];
 const result=await exchange.fetchHistoricalFills(options);
 cases.push({variant,options:native(options),configured:native(configured),result:native(result),operations:native(operations),cached:native(cached)});
}
assert.equal(cases.length,13,'historical source floor');assert.equal(cases[0].result.values.length,1,'REST baseline');assert.equal(cases[0].result.values[0].values.find(([k])=>k==='orderId')[1].value,'rest','REST excludes cache');for(const i of [10,11]){assert.equal(cases[i].result.values.length,0,'public-only source result');assert.equal(cases[i].operations.values.length,0,'public-only source makes no requests');}console.log(JSON.stringify({cases,header:{node:process.version,nodeArch:process.arch,rustArch:process.env.SAFE_CEX_RUST_ARCH}}));}
main().catch(error=>{console.error(error);process.exitCode=1;});
