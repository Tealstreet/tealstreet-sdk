// Executes the maintained producer, including its real metadata and queue dependencies.
const fs=require('node:fs'),path=require('node:path'),ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const root=path.resolve(__dirname,'../../..'),cache=new Map();
function load(file){
 if(cache.has(file))return cache.get(file).exports;
 const module={exports:{}};cache.set(file,module);
 const code=ts.transpileModule(fs.readFileSync(file,'utf8'),{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS,esModuleInterop:true}}).outputText;
 const localRequire=specifier=>{
  if(specifier.startsWith('.')||specifier.startsWith('@tealstreet/')){
   const base=specifier.startsWith('.')?path.resolve(path.dirname(file),specifier):path.join(root,'packages',specifier.slice('@tealstreet/'.length));
   return load(fs.existsSync(base+'.ts')?base+'.ts':path.join(base,'index.ts'));
  }
  return require(specifier);
 };
 new Function('require','module','exports',code)(localRequire,module,module.exports);return module.exports;
}
const {DisplayedPositionsPostprocessor}=load(path.join(root,'packages/safe-cex/src/webworker/postProcessors/DisplayedPositionsPostprocessor.ts'));
const {stubStore}=load(path.join(root,'packages/safe-cex/src/store/store.stub.ts'));
const store=structuredClone(stubStore);
const market=symbol=>({...store.markets[0],id:symbol,symbol,base:symbol.slice(0,3),quote:'USDT'});
const ticker=symbol=>({...store.tickers[0],id:symbol,symbol,last:20});
store.markets=[market('BTCUSDT'),market('ETHUSDT')];store.tickers=[ticker('BTCUSDT'),ticker('ETHUSDT')];
store.positions=[{...store.positions[0],id:'real',symbol:'BTCUSDT',side:'long',contracts:2,entryPrice:10,notional:40,unrealizedPnl:0}];
store.activeChasers=[{...store.activeChasers[0],id:'pending',symbol:'ETHUSDT',side:'buy',size:1,isStopped:false,reduceOnly:false}];
store.activeTWAPs=[];store.orders=[];
new DisplayedPositionsPostprocessor('account','binance',{key:'',secret:''}).process(store);
if(store.displayedPositions.length!==2||store.displayedPositions[0].key!=='account:BTCUSDT:long'||'key' in store.displayedPositions[1])throw Error('real/fake producer seam moved');
process.stdout.write(JSON.stringify(store.displayedPositions));
