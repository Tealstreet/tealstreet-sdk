// Live capture architecture and Node major: captureArchitecture/captureNodeMajor per record.
// Execute live pagination, mapper, discovery and Axios; observe concurrent effects.
const fs=require('node:fs'),path=require('node:path');
const compiler=process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript',ts=require(compiler);
const runtime=require('node:module').createRequire(require.resolve(compiler));
const axios=runtime(process.env.KUCOIN_ORDERS_MISSING_RUNTIME?'missing-kucoin-orders-axios':'axios');
const {root,load}=require('./tables.cjs'),{DefaultStore}=require('./spot-store-notifications.cjs');
const file=path.join(root,'packages/safe-cex/src/exchanges/kucoin/kucoinspot.exchange.ts');
const ast=ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true);
const cls=ast.statements.find(n=>ts.isClassDeclaration(n)&&n.name?.text==='KucoinSpotExchange');
const options={target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS};
const env={...load(path.join(root,'packages/safe-cex/src/exchanges/kucoin/kucoinspot.types.ts')),...load(path.join(root,'packages/safe-cex/src/types.ts'))};
for(const node of ast.statements)if(ts.isVariableStatement(node))for(const d of node.declarationList.declarations){
 if(['KUCOIN_SPOT_OPEN_ORDERS_PAGE_SIZE','KUCOIN_SPOT_OPEN_ORDERS_MAX_PAGES'].includes(d.name.getText(ast))){
  if(!d.initializer)throw Error('missing pagination constant');
  env[d.name.getText(ast)]=new Function('return '+d.initializer.getText(ast))();
 }
}
if(!Object.hasOwn(env,'KUCOIN_SPOT_OPEN_ORDERS_PAGE_SIZE')||!Object.hasOwn(env,'KUCOIN_SPOT_OPEN_ORDERS_MAX_PAGES'))throw Error('missing live pagination constants');
const omit={exports:{}};
new Function('module','exports','require',ts.transpileModule(fs.readFileSync(path.join(root,'packages/safe-cex/src/utils/omitUndefined.ts'),'utf8'),{compilerOptions:options}).outputText)(omit,omit.exports,runtime);
Object.assign(env,omit.exports);
function bind(name,context,clock){
 const member=cls.members.find(m=>m.name?.getText(ast)===name);
 if(!member?.initializer)throw Error('missing live '+name);
 let expression=member.initializer.getText(ast);
 if(name==='fetchOrders'){
  const mutation=process.env.KUCOIN_ORDERS_MUTATION;
  const replace=(from,to)=>{if(!expression.includes(from))throw Error('missing mutation anchor '+from);expression=expression.replace(from,to);};
  if(mutation==='sequential')replace('Promise.all(targetSymbols.map((symbol) => fetchSymbolOrders(symbol)))','(async()=>{const out=[];for(const symbol of targetSymbols)out.push(await fetchSymbolOrders(symbol));return out;})()');
  if(mutation==='empty-fallback')replace('return this.store.orders;','return [];');
  if(mutation==='page-guard')replace('pageNum <= totalPage && ','');
 }
 const mod={exports:{}};
 new Function('module','Date',...Object.keys(env),ts.transpileModule('module.exports='+expression,{compilerOptions:options}).outputText).call(context,mod,{now:clock},...Object.values(env));
 return mod.exports;
}
const snapshot=value=>JSON.parse(JSON.stringify(value));
const {bits}=require('./numeric-bits.cjs');
const numeric=['price','amount','filled','remaining','lastUpdate'];
const inspect=orders=>({wire:snapshot(orders),bits:orders.map(o=>numeric.map(k=>bits(o[k]))),types:orders.map(o=>Object.fromEntries(['id','symbol','status','type','side','timeInForce'].map(k=>[k,typeof o[k]])))});
async function until(predicate,label){for(let i=0;i<1000;i++){if(predicate())return;await Promise.resolve();}throw Error('live Spot orders '+label+' did not settle');}
(async()=>{
 const output=[];
 for(const c of JSON.parse(fs.readFileSync(0,'utf8'))){
  const store=new DefaultStore();store.update({orders:c.existing||[]});
  const events=[],calls=[],errors=[],pending=new Map();let clockCalls=0;
  const context={store,emitAndLogError(error){errors.push(error);events.push(['emit',error]);}};
  const clock=()=>{clockCalls++;return 1700000000123;};
  context.mapOrder=bind('mapOrder',context,clock);
  context.fetchActiveOrderSymbols=bind('fetchActiveOrderSymbols',context,clock);
  context.xhr={get:async(url,options)=>{
   const index=calls.length,params=options?.params;
   calls.push({url,...(params===undefined?{}:{params,keys:Object.keys(params)})});events.push(['request',index]);
   const descriptor=c.responses?.[index];
   if(!descriptor)throw Error('unconfigured Spot order request '+index);
   if(descriptor.defer)await new Promise(resolve=>pending.set(index,resolve));
   if(descriptor.storeOrders)store.update({orders:descriptor.storeOrders});
   if(descriptor.failure){const failure={...descriptor.failure};if(failure.response?.raw!==undefined)failure.response={...failure.response,data:axios.defaults.transformResponse[0].call(axios.defaults,failure.response.raw)};events.push(['reject',index]);throw failure;}
   events.push(['respond',index]);
   return{data:axios.defaults.transformResponse[0].call(axios.defaults,descriptor.raw===undefined?descriptor.data:descriptor.raw)};
  }};
  let returned=false,result;
  const outer=bind('fetchOrders',context,clock)(c.symbols).then(value=>{result=value;returned=true;events.push(['return']);});
  const release=async index=>{await until(()=>pending.has(index),'concurrent request '+index);events.push(['release',index]);const resolve=pending.get(index);pending.delete(index);resolve();};
  for(const index of c.releaseBeforeReturn||[])await release(index);
  await until(()=>returned,'outer completion');await outer;
  const record={captureArchitecture:process.arch,captureNodeMajor:Number(process.versions.node.split(".")[0]),atReturn:{...inspect(result),events:snapshot(events),calls:snapshot(calls),errors:snapshot(errors),storeAlias:result===store.orders,clockCalls}};
  for(const index of c.releaseAfterReturn||[]){await release(index);await until(()=>events.some(e=>(e[0]==='respond'||e[0]==='reject')&&e[1]===index),'released request settlement '+index);}
  if(c.afterCalls!==undefined)await until(()=>calls.length>=c.afterCalls&&errors.length>=(c.afterErrors||0),'sibling continuation');
  record.after={...inspect(result),events:snapshot(events),calls:snapshot(calls),errors:snapshot(errors),pending:pending.size,clockCalls};
  if(c.infoAlias){const wire=c.responses[0].data.data.items[0];if(!result[0])throw Error('missing info alias result');record.infoBefore=snapshot(result[0].info);wire.marker='after';record.infoAfter={result:snapshot(result[0].info),wire:snapshot(wire),same:result[0].info===wire};}
  if(c.alias&&result.length){result[0].price=91;record.mutatedStore=snapshot(store.orders);store.update({orders:c.replaceOrders||[]});record.afterReplacement={result:snapshot(result),store:snapshot(store.orders),sameArray:result===store.orders};}
  if(pending.size)throw Error('unsettled fixture requests');
  output.push(record);
 }
 console.log(JSON.stringify({limits:{pageSize:env.KUCOIN_SPOT_OPEN_ORDERS_PAGE_SIZE,maxPages:env.KUCOIN_SPOT_OPEN_ORDERS_MAX_PAGES},cases:output}));
})().catch(error=>{console.error(error);process.exitCode=1;});
