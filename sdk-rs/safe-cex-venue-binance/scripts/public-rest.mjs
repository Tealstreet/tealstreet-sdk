import assert from 'node:assert/strict';
import fs from 'node:fs';import vm from 'node:vm';import {createRequire} from 'node:module';
const require=createRequire(import.meta.url),ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript'),dayjs=require('dayjs');
const [venue,family,mode]=process.argv.slice(2);assert.ok(['futures','pm','spot'].includes(venue));assert.ok(['markets','tickers','ohlcv'].includes(family));
const file=venue==='futures'?'binance':venue==='pm'?'binancepm':'binancespot',className=venue==='futures'?'BinanceExchange':venue==='pm'?'BinancePMExchange':'BinanceSpotExchange';
function tree(path){return ts.createSourceFile(path,fs.readFileSync(new URL(path,import.meta.url),'utf8'),ts.ScriptTarget.Latest,true);}
function declaration(t,name){const n=t.statements.find(n=>ts.isVariableStatement(n)&&n.declarationList.declarations.some(d=>d.name.getText(t)===name));assert.ok(n,name);return n.getText(t).replace(/^export /,'');}
const t=tree(`../../../packages/safe-cex/src/exchanges/binance/${file}.exchange.ts`),types=tree(`../../../packages/safe-cex/src/exchanges/binance/${file}.types.ts`),cls=t.statements.find(n=>ts.isClassDeclaration(n)&&n.name?.text===className);
const method=family==='markets'?'fetchMarkets':family==='tickers'?'fetchTickers':'fetchOHLCV',member=cls.members.find(n=>n.name?.getText(t)===method);assert.ok(member,method);
const vCode=ts.transpileModule(fs.readFileSync(new URL('../../../packages/safe-cex/src/utils/get-key.ts',import.meta.url),'utf8'),{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022,esModuleInterop:true}}).outputText;
let helpers='';if(venue!=='spot'){const stem=venue==='futures'?'Binance':'BinancePM';helpers=['describe'+stem+'ResponseShape','get'+stem+'ResponseMessage','require'+stem+'ArrayResponse'].map(n=>declaration(t,n)).join('\n');}
if(venue==='futures')helpers+='\n'+declaration(t,'emitBinanceExchangeError');
function normalize(v){if(v===undefined)return {$undefined:true};if(typeof v==='number'){if(Number.isNaN(v))return 'nan';if(v===Infinity)return 'infinity';if(v===-Infinity)return '-infinity';if(Object.is(v,-0))return '-zero';return v;}if(Array.isArray(v))return v.map(normalize);if(v&&typeof v==='object')return Object.fromEntries(Object.entries(v).map(([k,v])=>[k,normalize(v)]));return v;}
const assetRows=[];
if(mode==='asset-rows'||mode==='field-rows'){
 assert.equal(family,'markets');
 const market=(symbol,base)=>({symbol,baseAsset:base,quoteAsset:'USDT',marginAsset:'USDT',contractType:'PERPETUAL',status:'TRADING',filters:[{filterType:'PRICE_FILTER',tickSize:'0.1'},{filterType:'LOT_SIZE',stepSize:'0.01',minQty:'0.01',maxQty:'100'},{filterType:'MARKET_LOT_SIZE',stepSize:'0.01',minQty:'0.01',maxQty:'100'}]});
 if(mode==='asset-rows') for(const field of ['baseAsset','quoteAsset'])for(const value of [{asset:'wrong'},false,null,17,undefined]){
  const bad={...market('BADUSDT','BAD'),[field]:value};
  assetRows.push({name:'public-only',payloads:[{data:{symbols:[market('BTCUSDT','BTC'),bad,market('ETHUSDT','ETH')]}}]});
 }
 if(mode==='field-rows'){
  for(const value of [{symbol:'wrong'},false,null,17,undefined]){
   assetRows.push({name:'public-only',payloads:[{data:{symbols:[market('BTCUSDT','BTC'),{...market('BADUSDT','BAD'),symbol:value},market('ETHUSDT','ETH')]}}]});
  }
  if(venue!=='spot')for(const value of ['125',{leverage:125},[125],true]){
   assetRows.push({name:'field-refusal',payloads:[{data:{symbols:[market('BTCUSDT','BTC'),market('BADUSDT','BAD'),market('ETHUSDT','ETH')]}},{data:[{symbol:'BADUSDT',brackets:[{initialLeverage:value}]}]}]});
  }
 }
}
const inputs=mode==='asset-rows'||mode==='field-rows'?assetRows:JSON.parse(fs.readFileSync(new URL('../tests/fixtures/public-rest.json',import.meta.url),'utf8')).filter(c=>c.family===family);assert.equal(inputs.length,mode==='asset-rows'?10:mode==='field-rows'?(venue==='spot'?5:9):{markets:17,tickers:15,ohlcv:18}[family],'independent input floor');const results=[];
for(const fixture of inputs){
 const c=structuredClone(fixture),trace=[],emissions=[],exports={};let calls=0,publicOnly=c.name==='public-only';
 function clock(value){trace.push(['dayjs',value===undefined?null:normalize(value)]);if(c.name==='date-fail')throw error;const dt=dayjs(value===undefined?200000000:value);return {valueOf:()=>dt.valueOf(),subtract(amount,unit){trace.push(['subtract',dt.valueOf(),normalize(amount),unit]);if(c.name==='subtract-fail')throw error;const out=dt.subtract(amount,unit);return {valueOf:()=>out.valueOf()};}};}
 const code=`${declaration(types,'ENDPOINTS')}\n${helpers}\nexports.Subject=class {${member.getText(t)}};exports.failure=()=>new RangeError('public fixture');exports.isError=e=>e instanceof Error;`;
 const context=vm.createContext({exports,require,dayjs:clock});vm.runInContext(vCode,context);context.v=exports.v;
 vm.runInContext(ts.transpileModule(code,{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText,context);
 const error=exports.failure();error.code='fixture-code';error.response={status:503,data:{msg:'venue detail'}};
 const subject=new exports.Subject();
 const get=client=>(endpoint,opts)=>{calls++;trace.push(['get',client,endpoint,normalize(opts?.params??null)]);if(c.name==='get-sync-fail')throw error;return Promise.resolve().then(()=>{trace.push(['resolve',calls]);if(c.name==='get-reject')throw error;if(c.name==='late-public')publicOnly=true;return c.payloads[calls-1];});};
 subject.publicXhr={get:get('public')};subject.xhr={get:get('private')};subject.papiXhr={get:get('papi')};
 Object.defineProperty(subject,'isPublicDataOnly',{get(){trace.push(['public-only']);return publicOnly;}});
 const cachedMarkets=[{id:'cached',symbol:'cached',base:'C',quote:'Q',active:false,precision:{amount:1,price:2},limits:{amount:{min:3,max:4},leverage:{min:1,max:5}}}],cachedTickers=[{id:'cached',symbol:'cached',bid:1,ask:2,last:3,mark:4,index:5,percentage:6,fundingRate:7,fundingTime:8,volume:9,quoteVolume:10,openInterest:0}];
 const map={BTCUSDT:{id:'BTC/USDT',symbol:'BTCUSDT'},undefined:{id:'undefined-id',symbol:'undefined'}};
 subject.store={get markets(){trace.push(['fallback-markets']);if(c.name==='fallback-fail')throw error;return cachedMarkets;},get tickers(){trace.push(['fallback-tickers']);if(c.name==='fallback-fail')throw error;return cachedTickers;},get marketsBySymbol(){trace.push(['map']);if(['map-fail','map-null-order'].includes(c.name))throw error;return map;}};
 subject.emitAndLogError=(arg,prefix)=>{trace.push(['emit']);const isError=exports.isError(arg);emissions.push({kind:isError?'error':'message',original:isError?arg===error:false,prefix:prefix===undefined?null:prefix,message:isError?null:normalize(arg),errorName:isError?arg.name:null,errorMessage:isError?arg.message:null,errorCode:isError?normalize(arg.code):null,errorResponse:isError?normalize(arg.response):null});if(c.name==='emit-fail')throw error;};
 let value,failure;const opts=c.opts;if(c.nan_limit)opts.limit=NaN;
 try{value=await subject[method](opts);}catch(e){failure=e;}
 results.push({name:c.name,trace,emissions,fallback:value===cachedMarkets||value===cachedTickers,value:value===undefined?null:normalize(value),failure:failure?{original:failure===error,name:failure.name,message:failure.message,code:normalize(failure.code),response:normalize(failure.response)}:null});
}
if(mode==='asset-rows'||mode==='field-rows'){
 for(const [index,result] of results.entries()){
  assert.equal(result.failure,null,'source accepts raw field');
  assert.equal(result.fallback,false,'source keeps fresh mixed response');
  assert.equal(result.emissions.length,0,'source emits no typed refusal');
  assert.equal(result.value.length,3,'source retains all rows');
  if(mode==='asset-rows'){
   const field=index<5?'base':'quote';
   assert.deepEqual(result.value[1][field],normalize([{asset:'wrong'},false,null,17,undefined][index%5]),'source raw asset value');
  }else if(index<5){
   assert.deepEqual(result.value[1].symbol,normalize([{symbol:'wrong'},false,null,17,undefined][index]),'source raw symbol');
  }else{
   assert.deepEqual(result.value[1].limits.leverage.max,normalize(['125',{leverage:125},[125],true][index-5]),'source raw leverage');
  }
 }
}
process.stdout.write(JSON.stringify(results));
