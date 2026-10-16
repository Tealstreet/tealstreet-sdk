const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict'),vm=require('node:vm');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const file=path.resolve(__dirname,'../../../packages/safe-cex/src/exchanges/woo/woo.v3.public-parity.ts');
const source=fs.readFileSync(file,'utf8'),ast=ts.createSourceFile(file,source,ts.ScriptTarget.Latest,true);
for(const[name,fields]of [['WooV3PublicRequestContext',['symbol']],['WooV3KlineRequestContext',['symbol','interval']]]){
 const declaration=ast.statements.find(n=>ts.isTypeAliasDeclaration(n)&&n.name.text===name);assert(declaration);
 const inner=declaration.type.typeArguments[0];assert(ts.isTypeLiteralNode(inner));
 assert.deepEqual(inner.members.map(m=>m.name.getText(ast)),fields);assert(inner.members.every(m=>m.type.kind===ts.SyntaxKind.StringKeyword));
}
const api={};const output=ts.transpileModule(source+'\nexport {canonicalString,canonicalMarket,canonicalLegacyTickerStats,canonicalV3TickerStats,canonicalOrderBookLevel,canonicalizeOrderBookValues,canonicalCandle,canonicalizeCandlesForRequest,normalizeKlineInterval,compareOrderBookSide,unsupportedIntervalBlocker,validateKlineContext};',{reportDiagnostics:true,compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}});
assert.equal(output.diagnostics.filter(d=>d.category===ts.DiagnosticCategory.Error).length,0);
const v3={};const v3file=path.resolve(path.dirname(file),'woo.v3.public.ts');
vm.runInNewContext(ts.transpileModule(fs.readFileSync(v3file,'utf8'),{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText,{exports:v3,require:name=>{assert.equal(name,'../../utils/orderbook');return {calcOrderBookTotal(){throw Error('out-of-scope source book total');},sortOrderBook(){throw Error('out-of-scope source book sort');}};}});
const context=vm.createContext({exports:api,require:name=>{assert.equal(name,'./woo.v3.public');return v3;}});
vm.runInContext('const __initialization=[]; const __map=Array.prototype.map; Array.prototype.map=function(...args){__initialization.push("map");return __map.apply(this,args);}; const __freeze=Object.freeze; Object.freeze=function(value){__initialization.push("freeze");return __freeze(value);};',context);
vm.runInContext(output.outputText,context);assert.deepEqual(Array.from(vm.runInContext('__initialization',context)),['map','freeze','map']);api.mapWooV3Candles=v3.mapWooV3Candles;
function observe(v){if(v===undefined)return{undefined:true};if(typeof v==='number'&&(Number.isNaN(v)||Object.is(v,-0)))return{number:Number.isNaN(v)?'NaN':'-0'};if(v&&typeof v==='object'){if(Array.isArray(v))return v.map(observe);return{fields:Object.entries(v).map(([k,v])=>[k,observe(v)])};}return v;}
function revive(v){if(v&&typeof v==='object'){if(Object.keys(v).length===1&&'$nativeNumber' in v){assert(['NaN','-0'].includes(v.$nativeNumber));return v.$nativeNumber==='NaN'?NaN:-0;}return Array.isArray(v)?v.map(revive):Object.fromEntries(Object.entries(v).map(([k,v])=>[k,revive(v)]));}return v;}
const check=process.argv.includes('--check');
const checks=[
 {op:'canonicalString',input:undefined,expected:{undefined:true}},
 {op:'canonicalV3TickerStats',input:{id:'row',symbol:'ETH/USDT',index:1,mark:2,percentage24h:3,openInterest:4,estimatedFundingRate:5,fundingTime:6,volume24h:7,quoteVolume24h:8},expected:{fields:[['id','row'],['symbol','ETH/USDT'],['index',1],['mark',2],['percentage',3],['openInterest',4],['fundingRate',5],['fundingTime',6],['volume',7],['quoteVolume',8]]}},
 {op:'normalizeKlineInterval',input:'1m',expected:{fields:[['status','ready'],['interval','1m'],['durationMs',60000]]}},
];
const cases=check?checks:revive(JSON.parse(fs.readFileSync(0,'utf8')));
if(check)assert.equal(cases.length,3,'independent root oracle check floor');
const results=cases.map(c=>{
 assert.equal(typeof api[c.op],'function');
 try {
  const args=c.args||[c.input],value=api[c.op](...args);assert(!c.throws,'expected source throw');
  if(c.op.startsWith('compareWooV3')){
   let legacy,observed;
   if(c.op==='compareWooV3Markets'||c.op==='compareWooV3TickerStats'){[legacy,observed]=args;}
   else if(c.op==='compareWooV3OrderBook'){legacy=args[0].legacyOrderBook;observed=args[0].v3OrderBook;}
   else if(c.op==='compareWooV3Candles'){legacy=args[0].legacyCandles;observed=args[0].v3Rows;}
   else throw Error('unaccounted source comparison');
   assert.equal(value.authoritative,legacy,'source authoritative identity');assert.equal(value.observed,observed,'source observed identity');
   assert(Object.isFrozen(value));assert(Object.isFrozen(value.mismatches));assert(Object.isFrozen(value.blockers));
   for(const item of value.mismatches){assert(Object.isFrozen(item));assert(Object.isFrozen(item.provenance));}
   if(c.mutateAfter){assert.equal(c.op,'compareWooV3OrderBook');legacy.bids.push({price:23,amount:29,total:31});assert.equal(value.authoritative.bids.length,2);}
  }
  const observed=observe(value);
  if(check)assert.deepEqual(observed,c.expected,`WOO public canonical root check ${c.op}`);
  return observed;
 }catch(error){if(!c.throws)throw error;return {throwMessage:error.message};}
});
if(check)console.log('WOO public canonical oracle: 3 source checks');
else process.stdout.write(JSON.stringify(results));
