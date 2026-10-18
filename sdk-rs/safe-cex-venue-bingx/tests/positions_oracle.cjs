const assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
assert(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH),root=path.resolve(__dirname,'../../..'),cache=new Map();
function compile(text,file){const r=ts.transpileModule(text,{fileName:file,reportDiagnostics:true,compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}});assert(!r.diagnostics.some(d=>d.category===ts.DiagnosticCategory.Error));return r.outputText;}
function load(file){if(cache.has(file))return cache.get(file);const module={exports:{}};cache.set(file,module.exports);const req=spec=>{
 if(spec.startsWith('@tealstreet/'))return load(path.join(root,'packages',spec.slice('@tealstreet/'.length)+'.ts'));
 if(spec.endsWith('/virtualClock'))return {virtualClock:{getCurrentTimeMs:()=>1234}};
 if(spec.startsWith('.'))return load(path.resolve(path.dirname(file),spec+'.ts'));
 return require(require.resolve(spec,{paths:[path.join(root,'node_modules')]}));
};vm.runInThisContext(`(function(require,module,exports){${compile(fs.readFileSync(file,'utf8'),file)}\n})`,{filename:file})(req,module,module.exports);cache.set(file,module.exports);return module.exports;}
const types=load(path.join(root,'packages/safe-cex/src/types.ts'));
const constants=load(path.join(root,'packages/safe-cex/src/exchanges/bingx/bingx.types.ts'));
const {DefaultStore}=load(path.join(root,'packages/safe-cex/src/store/store.base.ts'));
const {v}=load(path.join(root,'packages/safe-cex/src/utils/get-key.ts'));
const {createPositionId}=load(path.join(root,'packages/safe-cex/src/utils/createPositionId.ts'));
const {mapSeries,forEachSeries}=require(require.resolve('p-iteration',{paths:[path.join(root,'node_modules')]}));
const flatten=require(require.resolve('lodash/flatten',{paths:[path.join(root,'node_modules')]}));
const groupBy=require(require.resolve('lodash/groupBy',{paths:[path.join(root,'node_modules')]}));
const names=['fetchOpenPositions','fetchPositions','fetchPositionMetadata','changePositionMode','changeMarginType','setLeverage','cancelOrders','cancelSymbolOrders'];
const file=path.join(root,'packages/safe-cex/src/exchanges/bingx/bingx.exchange.ts'),src=ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true);
const cls=src.statements.filter(s=>ts.isClassDeclaration(s)&&s.name?.text==='BingxExchange');assert.equal(cls.length,1);
const members=[...names,'getOrFetchPositionMode','fetchLeverageForSymbol','fetchMarginModeForSymbol','toBingxSymbol'].map(name=>{const found=cls[0].members.filter(m=>m.name?.getText(src)===name);assert.equal(found.length,1,name+' census');return found[0].getText(src);});
const Reads=vm.runInNewContext(compile(`class Reads {${members.join('\n')}} Reads;`,file),{...types,...constants,v,createPositionId,mapSeries,forEachSeries,flatten,groupBy},{filename:file});
const market=(base,quote,max)=>({id:`${base}-${quote}`,symbol:`${base}${quote}`,base,quote,active:true,precision:{amount:0.01,price:0.25},limits:{amount:{min:0.1,max:999},leverage:{min:1,max}}});
const markets=[market('BTC','USDT',50),market('ETH','USDC',20)];
const cached=[{id:'ETHUSDC:long',symbol:'ETHUSDC',side:'long',entryPrice:41,notional:83,leverage:11,marginType:'cross',positionMode:'hedged',unrealizedPnl:1,contracts:2,liquidationPrice:31,marginSize:7}];
const order=(id,symbol)=>({id,symbol,status:'open',type:'limit',side:'buy',price:13,amount:3,filled:0,remaining:3,reduceOnly:false,lastUpdate:1234});
const orders=[order('a','BTCUSDT'),order('b','ETHUSDC'),order('c','BTCUSDT'),order('n','10'),order('k','2')];
function fixtures(method,variant){
 const rows=[{symbol:'BTC-USDT',avgPrice:'11',positionAmt:'-3',unrealizedProfit:'2.25',positionSide:'BOTH',isolated:'false',leverage:'7',isolatedMargin:'0',initialMargin:'123',liquidationPrice:'5.5'}, {symbol:'BTC-USDT',avgPrice:'17',positionAmt:'-2',unrealizedProfit:'-1.5',positionSide:'LONG',isolated:false,leverage:4,isolatedMargin:'4.5',liquidationPrice:'9.25'}, {symbol:'OTHER-USDT',avgPrice:'99'}];
 const input={markets,cached,orders,rows,mode:variant===1?'false':'true',symbols:['BTCUSDT','ETHUSDC','ETHUSDC'],symbol:'BTCUSDT',flag:variant!==1,leverage:variant===1?-7:123,side:variant===1?null:'short',disposed:false,error:null,postCode:0,postMsg:'',cancelCodes:{a:0,b:0,c:80016,n:0,k:0}};
 if(variant===1){input.rows=[{...rows[0],positionAmt:'2.5',avgPrice:'10',unrealizedProfit:'-1',isolated:0,leverage:3.5,isolatedMargin:0,initialMargin:'9',liquidationPrice:'NaN'}];if(method==='fetchPositions')input.symbols=[];if(method==='fetchPositionMetadata'){input.symbols=['ETHUSDC','BTCUSDT'];input.leverageFailure='BTC-USDT';}if(method==='cancelOrders'){input.cancelCodes.b=7;input.cancelCodes.c='0';}}
 if(variant===2){
  if(['fetchOpenPositions','fetchPositions'].includes(method))input.error={scope:'all',message:'network cause',msg:'wire denied'};
  if(method==='fetchPositionMetadata')input.error={scope:'margin',message:'margin cause',msg:'margin denied'};
  if(['changePositionMode','changeMarginType'].includes(method)){input.postCode=109401;input.postMsg='pending orders';}
  if(method==='setLeverage')input.side=null;
  if(method==='cancelOrders')input.error={scope:'order-b',message:'cancel cause',msg:'cancel denied'};
  if(method==='cancelSymbolOrders')input.error={scope:'all',message:'cancel cause',msg:'cancel denied'};
 }
 if(variant===3){
  if(method==='fetchOpenPositions')input.positionCode='0';
  if(method==='fetchPositions')input.symbols=null;
  if(method==='fetchPositionMetadata')input.disposed=true;
  if(method==='changePositionMode'){input.postCode='0';input.postMsg='strict config code';}
  if(method==='changeMarginType')input.symbol='';
  if(method==='setLeverage')input.symbol='UNKNOWN';
  if(method==='cancelOrders'){input.orders=[];}
  if(method==='cancelSymbolOrders')input.cancelAllCode=109401;
 }
 return structuredClone(input);
}
function observe(value){if(typeof value==='number'){if(Number.isNaN(value))return {number:'NaN'};const b=Buffer.alloc(8);b.writeDoubleBE(value);return {number:b.toString('hex')};}if(typeof value==='function')return {callable:String(value)};if(value===undefined)return {undefined:true};if(Array.isArray(value))return value.map(observe);if(value&&typeof value==='object')return Object.fromEntries(Object.entries(value).map(([k,v])=>[k,observe(v)]));return value;}
function setup(input){const venue=new Reads(),calls=[],reports=[];const store=new DefaultStore();store.update({markets:input.markets,positions:input.cached,orders:input.orders,fetchedLeverage:{OLD:{buyLeverage:2,sellLeverage:3}}});venue.store=store;venue.isDisposed=input.disposed;if(input.disposed)store.reset();venue.emitAndLogError=(...args)=>reports.push(args);let mode=input.mode;
 const dispatch=async(method,url,config)=>{
  calls.push({method,url,config});const params=method==='POST'?config:config?.params;
  const e=input.error;
  if(e&&(e.scope==='all'||(e.scope==='margin'&&url===constants.ENDPOINTS.MARGIN_MODE&&method==='GET')||(e.scope==='order-b'&&params?.orderId==='b'))){const error=new Error(e.message);error.response={data:{msg:e.msg}};throw error;}
  if(method==='POST'){if(url===constants.ENDPOINTS.HEDGE_MODE&&input.postCode===0)mode=config.dualSidePosition;return {data:{code:input.postCode,msg:input.postMsg}};}
  if(method==='DELETE')return {data:{code:url===constants.ENDPOINTS.ORDER?input.cancelCodes[params.orderId]:input.cancelAllCode??0,msg:'cancel rejected'}};
  if(url===constants.ENDPOINTS.POSITIONS)return {data:{code:input.positionCode??0,msg:'position rejected',data:input.rows}};
  if(url===constants.ENDPOINTS.HEDGE_MODE)return {data:{code:0,data:{dualSidePosition:mode}}};
  if(url===constants.ENDPOINTS.LEVERAGE){if(params.symbol===input.leverageFailure)return {data:{code:9,msg:'no leverage'}};return {data:{code:0,data:{longLeverage:params.symbol==='ETH-USDC'?11:17,shortLeverage:params.symbol==='ETH-USDC'?31:23}}};}
  if(url===constants.ENDPOINTS.MARGIN_MODE)return {data:{code:0,data:{marginType:params.symbol==='ETH-USDC'?'CROSSED':'ISOLATED'}}};
  assert.fail('unexpected request '+method+' '+url);
 };
 venue.xhr={get:(url,config)=>dispatch('GET',url,config),post:(url,body)=>dispatch('POST',url,body),delete:(url,config)=>dispatch('DELETE',url,config)};
 return {venue,store,calls,reports};
}
async function invoke(venue,method,input){switch(method){case 'fetchOpenPositions':return venue.fetchOpenPositions();case 'fetchPositions':return venue.fetchPositions(input.symbols??undefined);case 'fetchPositionMetadata':return venue.fetchPositionMetadata(input.symbols);case 'changePositionMode':return venue.changePositionMode(input.flag);case 'changeMarginType':return venue.changeMarginType(input.flag,input.symbol);case 'setLeverage':return venue.setLeverage(input.symbol,input.leverage,input.side??undefined);case 'cancelOrders':return venue.cancelOrders(input.orders);case 'cancelSymbolOrders':return venue.cancelSymbolOrders(input.symbol);default:assert.fail(method);}}
(async()=>{const cases=[];for(const method of names)for(let variant=0;variant<4;variant++){
 const input=fixtures(method,variant),working=structuredClone(input),{venue,store,calls,reports}=setup(working);let output;
 try{output={ok:observe(await invoke(venue,method,working))};}catch(e){output={error:{name:e.name,message:e.message}};}
 await Promise.resolve();
 cases.push({method,variant,input,output,calls:observe(calls),reports:observe(reports),state:observe({positions:store.positions,orders:store.orders,fetchedLeverage:store.fetchedLeverage,updateIds:store.updateIds})});
 }
 const inherited=fixtures('fetchOpenPositions',0);inherited.rows=[{...inherited.rows[0],positionSide:'constructor'}];const i=setup(inherited),value=await i.venue.fetchOpenPositions();
 const prefix=structuredClone(inherited);prefix.rows[0].liquidationPrice={toString:0};const p=setup(prefix);await p.venue.fetchOpenPositions();
 const lateInput=fixtures('fetchPositionMetadata',2),late=setup(lateInput),get=late.venue.xhr.get;let release;
 late.venue.xhr.get=async(url,config)=>{if(url===constants.ENDPOINTS.LEVERAGE)await new Promise(resolve=>{release=resolve;});return get(url,config);};
 const early=await late.venue.fetchPositionMetadata(['BTCUSDT']);const before=!!late.store.fetchedLeverage.BTCUSDT;release();for(let i=0;i<5;i++)await Promise.resolve();
 const lateSibling={early:observe(early),before,afterBuyLeverage:late.store.fetchedLeverage.BTCUSDT.buyLeverage};
 const nan=fixtures('setLeverage',1);nan.leverage=NaN;const n=setup(nan);await n.venue.setLeverage(nan.symbol,nan.leverage);const post=n.calls.find(c=>c.method==='POST');
 process.stdout.write(JSON.stringify({runtime:{node:process.version,arch:process.arch,platform:process.platform},names,cases,boundary:{lateSibling,inherited:{input:inherited,side:typeof value[0].side,id:value[0].id},prefix:{input:prefix,reports:p.reports},nanBody:observe(post.config),nanCalls:observe(n.calls),nanReports:observe(n.reports)}}));
})().catch(e=>{console.error(e);process.exitCode=1;});
