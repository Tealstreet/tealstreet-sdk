const fs = require('node:fs'), path = require('node:path');
const compiler=process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript',ts=require(compiler);
const runtime=require('node:module').createRequire(require.resolve(compiler));
const axios=runtime(process.env.KUCOIN_READS_MISSING_RUNTIME?'missing-kucoin-axios-control':'axios');
const transform=raw=>axios.defaults.transformResponse[0].call(axios.defaults,raw);
const {root, load} = require('./tables.cjs');
const {context} = require('./sizing.cjs');
const env = {...load(path.join(root,'packages/safe-cex/src/types.ts')), ...load(path.join(root,'packages/safe-cex/src/utils/safe-math.ts')), ...load(path.join(root,'packages/safe-cex/src/exchanges/kucoin/kucoin.types.ts')), virtualClock:{getCurrentTimeMs:()=>1700000000123}, Date:{now:()=>1700000000123}};
const filename=path.join(root,'packages/safe-cex/src/exchanges/kucoin/kucoin.exchange.ts');
const source=ts.createSourceFile(filename,fs.readFileSync(filename,'utf8'),ts.ScriptTarget.Latest,true);
for(const name of ['mapOrder','fetchNormalOrders','fetchAlgoOrders','fetchOrders']) {
 const member=source.statements.find(ts.isClassDeclaration).members.find(m=>m.name?.getText(source)===name);
 if(!member?.initializer)throw Error('missing live '+name);
 let expr=member.initializer.getText(source);
 if(process.env.KUCOIN_ORDER_READ_MUTATION==='empty-failure' && name==='fetchNormalOrders') {
  if(!expr.includes('return null'))throw Error('missing mutation target');
  expr=expr.replaceAll('return null','return []');
 }
 const code=ts.transpileModule('module.exports='+expr,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;
 const module={exports:{}};new Function('module',...Object.keys(env),code).call(context,module,...Object.values(env));context[name]=module.exports;
}
const storeFile=path.join(root,'packages/safe-cex/src/store/store.base.ts');
const storeSource=ts.createSourceFile(storeFile,fs.readFileSync(storeFile,'utf8'),ts.ScriptTarget.Latest,true);
const constructor=storeSource.statements.find(ts.isClassDeclaration).members.find(ts.isConstructorDeclaration);
if(!constructor)throw Error('missing actual Store constructor');
const storeModule={exports:{}};
new Function('module',ts.transpileModule('module.exports=function()'+constructor.body.getText(storeSource),{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText)(storeModule);
(async()=>{
 const results=[];
 for(const c of JSON.parse(fs.readFileSync(0,'utf8'))) {
  const store={state:{markets:c.markets||[],tickers:[]},tickerIndexMap:new Map(),marketIndexMap:new Map(),tickersMap:{},tickersBySymbolMap:{},marketsMap:{},marketsBySymbolMap:{}};
  storeModule.exports.call(store);
  context.store={marketsBySymbol:store.marketsBySymbolMap,orders:c.existing||[]};
  const calls=[],errors=[];
  context.emitAndLogError=message=>errors.push(message===undefined?{$undefined:true}:message);
  context.xhr={async get(url,options){
   calls.push({method:'GET',url,params:options.params});
   if(c.duringSecond&&calls.length===2)context.store.orders=c.duringSecond;
   const response=c.responses[calls.length-1];if(!response)throw Error('unexpected request');
   if(response.failure){const e={...response.failure};if(e.response?.raw!==undefined)e.response={...e.response,data:transform(e.response.raw)};throw e;}
   return {data:response.raw===undefined?response.data:transform(response.raw)};
  }};
  let clockCalls=0;env.virtualClock.getCurrentTimeMs=()=>{clockCalls++;return 1700000000123+(c.clockOffset||0);};
  const orders=await context[c.method]();
  if(c.aliasProbe){
   const gateway=c.responses[0].data.data.items[0], mapped=orders[0];
   const before=JSON.stringify(mapped.info), same=mapped.info===gateway;
   gateway.id='later-label';const label=mapped.info.id;
   gateway.price=NaN;
   results.push({alias:{same,before,label,infoPrice:Number.isNaN(mapped.info.price)?'NaN':mapped.info.price,gatewayPrice:Number.isNaN(gateway.price)?'NaN':gateway.price,mappedPrice:mapped.price}});continue;
  }
  results.push(c.rawOnly?{ordersJSON:JSON.stringify(orders),calls,errors}:{orders,calls,errors,clockCalls,types:orders?.map(o=>({status:typeof o.status,type:typeof o.type,timeInForce:typeof o.timeInForce}))??null});
 }
 process.stdout.write(JSON.stringify(results,(_key,value)=>typeof value==='number'&&!Number.isFinite(value)?{$number:String(value)}:value));
})().catch(error=>{console.error(error);process.exitCode=1});
