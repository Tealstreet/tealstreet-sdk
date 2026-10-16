// Live capture architecture and Node major: captureArchitecture/captureNodeMajor per record.
// Actual venue method and Store constructor/getter; no Object.fromEntries surrogate.
const fs=require('node:fs'),path=require('node:path');
const compiler=process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript',ts=require(compiler);
const runtime=require('node:module').createRequire(require.resolve(compiler));
const axios=runtime(process.env.KUCOIN_MARKETS_MISSING_RUNTIME?'missing-kucoin-axios-control':'axios');
const transform=raw=>axios.defaults.transformResponse[0].call(axios.defaults,raw);
const {root,load}=require('./tables.cjs');
const env=load(path.join(root,'packages/safe-cex/src/exchanges/kucoin/kucoinspot.types.ts'));
const context={};
const file=path.join(root,'packages/safe-cex/src/exchanges/kucoin/kucoinspot.exchange.ts');
const ast=ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true);
const node=ast.statements.find(ts.isClassDeclaration).members.find(m=>m.name?.getText(ast)==='fetchMarkets');
if(!node?.initializer)throw Error('missing fetchMarkets source');
let expr=node.initializer.getText(ast);
if(process.env.KUCOIN_SPOT_MARKETS_MUTATION==='number'){
 if(!expr.includes('parseFloat('))throw Error('missing parseFloat control');
 expr=expr.replaceAll('parseFloat(', 'Number(');
}
const mod={exports:{}};new Function('module',...Object.keys(env),ts.transpileModule('module.exports='+expr,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText).call(context,mod,...Object.values(env));
const sf=path.join(root,'packages/safe-cex/src/store/store.base.ts');
const sa=ts.createSourceFile(sf,fs.readFileSync(sf,'utf8'),ts.ScriptTarget.Latest,true),sc=sa.statements.find(ts.isClassDeclaration);
function body(member){if(!member?.body)throw Error('missing Store member');const m={exports:{}};new Function('module',ts.transpileModule('module.exports=function()'+member.body.getText(sa),{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText)(m);return m.exports;}
const construct=body(sc.members.find(ts.isConstructorDeclaration));
const getMarkets=body(sc.members.find(m=>ts.isGetAccessor(m)&&m.name.getText(sa)==='markets'));
const {bits}=require('./numeric-bits.cjs');
(async()=>{
 const out=[];
 for(const c of JSON.parse(fs.readFileSync(0,'utf8'))){
  const store={state:{markets:c.existing||[],tickers:[]},tickerIndexMap:new Map(),marketIndexMap:new Map(),tickersMap:{},tickersBySymbolMap:{},marketsMap:{},marketsBySymbolMap:{}};
  construct.call(store);Object.defineProperty(store,'markets',{get:getMarkets});context.store=store;
  const calls=[],errors=[];let payload=c.raw===undefined?c.data:transform(c.raw);
  context.emitAndLogError=message=>errors.push(message);
  context.xhr={get:async url=>{calls.push({method:'GET',url});if(c.duringResponse)store.state.markets=c.duringResponse;if(c.failure){const failure={...c.failure};if(failure.response?.raw!==undefined)failure.response={...failure.response,data:transform(failure.response.raw)};throw failure;}return{data:payload};}};
  const markets=await mod.exports();
  const numericBits=markets.map(m=>[m.precision.amount,m.precision.price,m.limits.amount.min,m.limits.amount.max].map(bits));
  const result={calls,errors,markets,bits:numericBits};
  if(c.alias){const before=JSON.parse(JSON.stringify(markets));if(c.alias==='info')payload.data[0].marker='later';else store.state.markets[0].symbol='LATER';result.alias={before,source:payload,store:store.markets};}
  if(c.surrogate)result.markets=JSON.stringify(markets);
  if(c.failureSurrogate){result.errorJson=JSON.stringify(errors);result.errors=[];}
  out.push(result);
 }
 console.log(JSON.stringify(out.map(row=>({...row,captureArchitecture:process.arch,captureNodeMajor:Number(process.versions.node.split(".")[0])})),(_k,v)=>typeof v==='number'&&!Number.isFinite(v)?{$number:String(v)}:v));
})().catch(e=>{console.error(e);process.exitCode=1;});
