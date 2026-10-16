// Live cancelAllOrders, installed Axios response transform, ordered side effects.
const fs=require('node:fs'),path=require('node:path');
const compiler=process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript',ts=require(compiler);
const runtime=require('node:module').createRequire(require.resolve(compiler));
const axios=runtime(process.env.KUCOIN_CANCEL_MISSING_RUNTIME?'missing-kucoin-axios-control':'axios');
const transform=raw=>axios.defaults.transformResponse[0].call(axios.defaults,raw);
const {root,load}=require('./tables.cjs');
const env=load(path.join(root,'packages/safe-cex/src/exchanges/kucoin/kucoinspot.types.ts'));
const file=path.join(root,'packages/safe-cex/src/exchanges/kucoin/kucoinspot.exchange.ts');
const ast=ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true);
const context={},methods={};
for(const name of ['cancelAllOrders','cancelSymbolOrders','cancelOrders']) {
 const member=ast.statements.find(ts.isClassDeclaration).members.find(m=>m.name?.getText(ast)===name);
 if(!member?.initializer)throw Error('missing live '+name);
 let expr=member.initializer.getText(ast);
 if(process.env.KUCOIN_CANCEL_MUTATION==='swallow'){
  if(!expr.includes('throw err;'))throw Error('missing rethrow control');
  expr=expr.replace('throw err;','return;');
 }
 const mod={exports:{}};
 new Function('module',...Object.keys(env),ts.transpileModule('module.exports='+expr,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText).call(context,mod,...Object.values(env));
 methods[name]=mod.exports;
}
const storeFile=path.join(root,'packages/safe-cex/src/store/store.base.ts');
const storeAst=ts.createSourceFile(storeFile,fs.readFileSync(storeFile,'utf8'),ts.ScriptTarget.Latest,true);
const constructor=storeAst.statements.find(ts.isClassDeclaration).members.find(ts.isConstructorDeclaration);
if(!constructor)throw Error('missing DefaultStore constructor');
const storeModule={exports:{}};
new Function('module',ts.transpileModule('module.exports=function()'+constructor.body.getText(storeAst),{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText)(storeModule);
function marketsBySymbol(markets) {
 const store={state:{markets,tickers:[]},tickerIndexMap:new Map(),marketIndexMap:new Map(),tickersMap:{},tickersBySymbolMap:{},marketsMap:{},marketsBySymbolMap:{}};
 storeModule.exports.call(store);return store.marketsBySymbolMap;
}
(async()=>{
 const out=[];
 for(const c of JSON.parse(fs.readFileSync(0,'utf8'))){
  const calls=[],events=[],errors=[];let failure;
  if(c.ordersRaw!==undefined)c.orders=JSON.parse(c.ordersRaw);
  if(c.symbolRaw!==undefined)c.symbol=JSON.parse(c.symbolRaw);
  context.store={marketsBySymbol:marketsBySymbol(c.markets||[])};
  context.unlimitedXHR={delete:async (url,options)=>{
   calls.push({method:'DELETE',url,...(options?{params:options.params}:{})});events.push('request');
   const response=c.responses?c.responses[calls.length-1]:c;
   if(!response)throw Error('missing response fixture');
   if(response.appendOrder)c.orders.push(response.appendOrder);
   if(response.marketsAfter)context.store.marketsBySymbol=marketsBySymbol(response.marketsAfter);
   if(response.failure){failure={...response.failure};if(failure.response?.raw!==undefined)failure.response={...failure.response,data:transform(failure.response.raw)};throw failure;}
   return{data:response.raw===undefined?response.data:transform(response.raw)};
  }};
  context.emitAndLogError=error=>{events.push('emit');errors.push(error===undefined?{$undefined:true}:error);};
  let result;
  try{await methods[c.method||'cancelAllOrders'](c.method==='cancelOrders'?c.orders:c.symbol);events.push('resolved');result={ok:true};}
  catch(error){events.push('rejected');result={ok:false,message:error.message,name:error.name||null,sameFailure:error===failure};}
  out.push(c.symbolRaw!==undefined||c.ordersRaw!==undefined?{callsJson:JSON.stringify(calls),events,errorsJson:JSON.stringify(errors),resultJson:JSON.stringify(result)}:c.surrogate?{calls,events,errorsJson:JSON.stringify(errors),resultJson:JSON.stringify(result)}:{calls,events,errors,result});
 }
 console.log(JSON.stringify(out));
})().catch(e=>{console.error(e);process.exitCode=1;});
