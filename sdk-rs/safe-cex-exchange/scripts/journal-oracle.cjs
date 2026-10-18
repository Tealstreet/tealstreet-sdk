// Execute the full maintained adapter and actual enum dependencies; observe native values.
const fs = require('node:fs'), path = require('node:path'), vm = require('node:vm');
const assert = require('node:assert/strict');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const root = path.resolve(__dirname, '../../..'), cache = new Map();
function load(file) {
  if (cache.has(file)) return cache.get(file);
  const exports = {}; cache.set(file, exports);
  let source = fs.readFileSync(file, 'utf8');
  // Create fixture errors in the source realm so instanceof Error is meaningful.
  if(file.endsWith('buildRecords.ts')) source += '\nexport const sourceError = Error;';
  vm.runInNewContext(ts.transpileModule(source, {compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText, {exports,require(spec) {
    assert(spec.startsWith('.') || spec.startsWith('@tealstreet/'), `dependency ${spec}`);
    const target = spec.startsWith('.') ? path.resolve(path.dirname(file),spec) : path.join(root,'packages',spec.slice('@tealstreet/'.length));
    return load(fs.existsSync(target+'.ts') ? target+'.ts' : path.join(target,'index.ts'));
  }}, {filename:file}); return exports;
}
const api=load(path.join(root,'packages/safe-cex/src/journal/position-intervals.ts'));
const fold=load(path.join(root,'packages/safe-cex/src/journal/fold.position-adapter.ts'));
function observe(value) {
  if (value === undefined) return {undefined:true};
  if (typeof value === 'function') return {inherited:'function'};
  if (typeof value === 'number') { const b=Buffer.alloc(8); b.writeDoubleBE(value); return {number:Number.isNaN(value)?'nan':b.toString('hex')}; }
  if (Array.isArray(value)) return Array.from(value,observe);
  if (value && typeof value === 'object') return Object.fromEntries(Object.entries(value).map(([k,v])=>[k,observe(v)]));
  return value;
}
const fill=(side,qty,price,timestamp,id,over={})=>({side,qty,price,timestamp,id,symbol:'BTC',commission:0.125,commissionAsset:'USDT',realizedPnl:side==='sell'?2.25:0,...over});
const buy=fill('buy',3,11.25,1,'100'),sell=fill('sell',3,12.75,2,'101');
const sequences=[[],[buy],[sell],[sell,buy],
 [buy,{...buy,qty:2,id:'101',timestamp:2}, {...sell,qty:1,timestamp:3,id:'102'},{...sell,qty:4,timestamp:4,id:'103'}],
 [buy,{...sell,qty:5},fill('buy',2,10.5,3,'102')],
 [sell,{...buy,qty:5},fill('sell',2,10.5,3,'102')],
 [buy,{...sell,qty:3-5e-10}],
 [buy,{...sell,qty:0},sell],
 [{...buy,timestamp:0},sell],
 [{...sell,timestamp:1,id:'9999999999999999999999999999'},{...buy,timestamp:1,id:'9999999999999999999999999998'}],
 [buy,{...buy,symbol:'ETH',id:'102'},{...sell,symbol:'ETH',id:'103'},sell]];
const seeds=[{}, {'BTC|both':4}, {'BTC|short':-2,'OUTSIDE|both':1},{'both':2}];
const fundingRows=[{symbol:'BTC',timestamp:999,income:0.5},{symbol:'BTC',timestamp:1000,income:-0.25},{symbol:'BTC',timestamp:2000,income:0.75},{symbol:'BTC',timestamp:2001,income:10},{symbol:'ETH',timestamp:1500,income:3}];
const cases=[];
for(let sequence=0;sequence<12;sequence++)for(let seed=0;seed<4;seed++)for(const missing of [false,true])for(const resolved of [false,true]) {
 const fills=sequences[sequence].map(f=>{const row={...f};if(missing && row.side==='sell')delete row.realizedPnl;return row;});
 const calls=[];
 const options={openingSize:seeds[seed]};if(resolved)options.resolveLeg=e=>{calls.push(e.id);return e.side==='buy'?'long':undefined;};
 const intervals=api.reconstructPositionIntervals(fills,options);
 const funding=intervals.map(i=>[api.attributeFunding(i,fundingRows,{hedged:false}),api.attributeFunding(i,fundingRows,{hedged:true})]);
 const records=intervals.map((i,n)=>fold.mapFoldedInterval(i,{account:'acct',resolveSettleCurrency:s=>'settle:'+s,resolveFunding:()=>funding[n][0]}));
 cases.push({sequence,seed,missing,resolved,fills,opening:seeds[seed],calls,intervals:observe(intervals),funding:observe(funding),records:observe(records)});
}
assert.equal(cases.length,192,'INTERVAL source case floor');
// Direct mapper guards: undefined PnL differs from a reported zero, and NaN exit is absent.
const direct=[];
const interval={symbol:'DIRECT',positionSide:'short',side:'long',openedAt:17.5,closedAt:23.75,maxSize:11.25,closedSize:7.5,complete:false,closedByFlip:false,avgEntryPrice:31.25,avgExitPrice:NaN,realizedPnl:0,fees:0.75,feeCurrencies:['USDT']};
for(const patch of [{},{closedAt:undefined},{closedSize:0},{realizedPnl:undefined}]) {
 const input={...interval,...patch};
 direct.push({input:observe(input),result:observe(fold.mapFoldedInterval(input,{account:'acct',resolveSettleCurrency:s=>'settle:'+s}))});
}
assert.equal(direct.length,4,'FOLD direct case floor');


const build=load(path.join(root,'packages/safe-cex/src/journal/buildRecords.ts'));
async function buildCases() {
 const rows=[buy,{...sell,qty:3}, {...buy,symbol:'ETH',timestamp:0,id:'90'}, {...sell,symbol:'ETH',timestamp:1,id:'91'}, {...buy,symbol:'ETH',timestamp:2,id:'102'},{...sell,symbol:'ETH',timestamp:2.5,id:'103'}];
 const income=[{symbol:'ETH',category:'commission',timestamp:1500,income:7},{symbol:'BTC',category:'funding',timestamp:1500,income:-0.75},{symbol:'ETH',category:'funding',timestamp:2250,income:0.25},{symbol:'BTC',category:'funding',timestamp:2501,income:99}].map((r,i)=>({...r,incomeType:'native',asset:'USDT',tranId:String(i)}));
 const out=[];
 for(const mode of ['absent','success','failure'])for(const scoped of [false,true])for(const failFills of [false,true]){
  const trace=[];
  const exchange={fetchHistoricalFills:async opts=>{trace.push(['fills',opts]);if(failFills){const e=new build.sourceError('fills failed');e.name='FillDenied';throw e;}return opts.symbol?rows.filter(r=>r.symbol===opts.symbol):rows;}};
  if(mode!=='absent')exchange.fetchIncomeHistoryRange=async(from,to)=>{trace.push(['income',from,to]);if(mode==='failure'){const e=new build.sourceError('income failed');e.name='IncomeDenied';throw e;}return income;};
  let result;
  try{result={ok:observe(await build.buildFoldedPositionRecords(exchange,{account:'acct',from:1500,to:2500,symbolScoped:scoped,resolveSettleCurrency:s=>{trace.push(['currency',s]);return 'settle:'+s;}}))};}
  catch(e){result={error:e.name,message:e.message,cause:e.cause?{name:e.cause.name,message:e.cause.message}:null};}
  out.push({mode,scoped,failFills,rows,income,trace:observe(trace),result});
 }
 assert.equal(out.length,12,'BUILD source case floor');return out;
}
const orderingCases=[];
for(const count of [24,32,64,128])for(let seed=1;seed<=32;seed++){
 let state=seed;
 const next=()=>{state=(Math.imul(state,1664525)+1013904223)>>>0;return state;};
 const fills=Array.from({length:count},(_,i)=>fill('buy',1,i+1,next()%3,
  next()%3===0?undefined:String(next()%100),{orderId:String(i)}));
 const result=api.reconstructPositionIntervals(fills);
 assert.equal(result.length,1,'INTERVAL mixed IDs retain the open position');
 assert.equal(result[0].maxSize,count,'INTERVAL mixed IDs retain every fill');
 orderingCases.push({fills,intervals:observe(result)});
}
const nonfiniteCases=[];
for(const count of [24,32,64,128])for(let seed=1;seed<=32;seed++){
 let state=seed;
 const next=()=>{state=(Math.imul(state,1664525)+1013904223)>>>0;return state;};
 const closingTimes=Array.from({length:count},()=>next()%3===0?NaN:next()%1000);
 const fills=[
  ...closingTimes.map((_,i)=>fill('buy',1,1,-1,String(i),{symbol:`CLOSE${i}`})),
  ...closingTimes.map((at,i)=>fill('sell',1,2,at,String(i+count),{symbol:`CLOSE${i}`})),
 ];
 const result=api.reconstructPositionIntervals(fills);
 assert.equal(result.length,count,'INTERVAL retains all nonfinite closes');
 nonfiniteCases.push({fills:fills.map(row=>({...row,timestamp:0})),
  timestamps:fills.map(row=>observe(row.timestamp)),intervals:observe(result)});
}
buildCases().then(buildCases=>process.stdout.write(JSON.stringify({cases,direct,fundingRows,buildCases,orderingCases,nonfiniteCases}))).catch(e=>{console.error(e);process.exitCode=1;});
