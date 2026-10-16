// Live capture architecture and Node major: captureArchitecture/captureNodeMajor per record.
// Live venue method, production Store writes, and installed Axios response transform.
const fs=require('node:fs'),path=require('node:path');
const compiler=process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript',ts=require(compiler);
const runtime=require('node:module').createRequire(require.resolve(compiler));
const axios=runtime(process.env.KUCOIN_TICKERS_MISSING_RUNTIME?'missing-kucoin-axios-control':'axios'),{root,load}=require('./tables.cjs');
const {DefaultStore}=require('./spot-store-notifications.cjs');
const env=load(path.join(root,'packages/safe-cex/src/exchanges/kucoin/kucoinspot.types.ts'));
const file=path.join(root,'packages/safe-cex/src/exchanges/kucoin/kucoinspot.exchange.ts');
const ast=ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true);
const member=ast.statements.find(ts.isClassDeclaration).members.find(m=>m.name?.getText(ast)==='fetchTickers');
if(!member?.initializer)throw Error('missing live fetchTickers');
let expr=member.initializer.getText(ast);
if(process.env.KUCOIN_TICKERS_MUTATION==='number'){
 if(!expr.includes('parseFloat('))throw Error('missing parseFloat control');
 expr=expr.replaceAll('parseFloat(','Number(');
}
const context={},mod={exports:{}};
new Function('module',...Object.keys(env),ts.transpileModule('module.exports='+expr,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText).call(context,mod,...Object.values(env));
const {bits}=require('./numeric-bits.cjs');
const numeric=['bid','ask','last','mark','index','percentage','fundingRate','fundingTime','openInterest','volume','quoteVolume'];
const transform=raw=>axios.defaults.transformResponse[0].call(axios.defaults,raw);
(async()=>{
 const out=[];
 for(const c of JSON.parse(fs.readFileSync(0,'utf8'))){
  const store=new DefaultStore();store.update({markets:c.markets||[],tickers:c.existing||[]});
  context.store=store;const calls=[],errors=[];
  let payload=c.raw===undefined?c.data:transform(c.raw);
  context.xhr={get:async url=>{calls.push({method:'GET',url});if(c.duringResponse)store.update({tickers:c.duringResponse});if(c.failure){const e={...c.failure};if(e.response?.raw!==undefined)e.response={...e.response,data:transform(e.response.raw)};throw e;}return{data:payload};}};
  context.emitAndLogError=error=>errors.push(error);
  const tickers=await mod.exports();
  const captured={tickers,errors,calls,bits:tickers.map(t=>numeric.map(k=>bits(t[k]))),fields:tickers.map(t=>({id:typeof t.id,symbol:typeof t.symbol}))};
  if(c.alias){captured.before=JSON.parse(JSON.stringify(tickers));if(c.alias==='info')payload.data.ticker[0].marker='later';else store.tickers[0].last=77;captured.source=payload;captured.store=store.tickers;}
  if(c.surrogate){captured.tickersJson=JSON.stringify(tickers);captured.errorsJson=JSON.stringify(errors);captured.tickers=[];captured.errors=[];}
  out.push(captured);
 }
 console.log(JSON.stringify(out.map(row=>({...row,captureArchitecture:process.arch,captureNodeMajor:Number(process.versions.node.split(".")[0])}))));
})().catch(e=>{console.error(e);process.exitCode=1;});
