// Supplement the existing loaders; execute the maintained declarations unchanged.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const Module = require('node:module');
const history = process.argv[2] === 'history';
const filename = path.resolve(__dirname, history ? 'historical_fills.cjs' : 'exchange.cjs');
const driver = 'main().catch(error=>{console.error(error);process.exitCode=1;});';
let code = require('./controlled-exchange-loader.cjs')(fs.readFileSync(filename, 'utf8'));
assert.equal(code.split(driver).length, 2, 'APEX-BOUNDARY maintained driver selector');
code = code.replace(driver, () => String.raw`
(async () => {
 const rows = [];
 const assets = [['string','USDC'],['number',17],['array',['USDC']],['object',{}],
                 ['empty-string',''],['empty-array',[]],['boolean',true],['zero',0]];
 const fill = {symbol:'BTC-USDC',side:'BUY',createdAt:1700000000000,
               price:'83.25',fillPrice:'83.25',size:'2.25',fillSize:'2.25',fee:'-0.75',orderId:'rest'};
 const cached = {...fill,price:'19.25',size:'0.25',orderId:'ws',token:'USDC'};
 const history = process.argv[2] === 'history';
 const assetRows=[];
 if (!history) for(const [field,wireField] of [['id','symbol'],['symbol','crossSymbolName'],['base','baseTokenId'],['quote','settleAssetId'],['active','enableTrade']]){
  const values=field==='active'?[{asset:'wrong'},'truthy',17,[]]:[{asset:'wrong'},false,null,17,undefined];
  for(const value of values){
   const s=store();const venue=new Class('apexomni',{key:'key',secret:'secret'},s);
   const bad={...contract,symbol:'BAD-USDC',crossSymbolName:'BADUSDC',[wireField]:value};
   responses={'/v3/symbols':{data:{contractConfig:{perpetualContract:[contract,bad,{...contract,symbol:'LAST-USDC',crossSymbolName:'LASTUSDC'}]}}}};operations=[];
   const result=await venue.fetchMarketsHelper();
   assert.equal(result.length,3,'source keeps malformed field and siblings');
   assert.equal(result[1][field],value,'source preserves raw field identity');
   assert(!operations.some(op=>op[0]==='log'),'source has no typed refusal');
   assetRows.push({field,value:native(value),result:native(result)});
  }
 }
 assert.equal(assetRows.length,history?0:24,'strict market source cases');
 const marketCases = [
  ['valid',[contract,{...contract,symbol:'disabled',enableTrade:false}]],
  ['late-null',[{...contract,enableTrade:'truthy'},null]],
  ['nonboolean',[{...contract,enableTrade:'truthy'}]],
  ['numeric-id',[{...contract,symbol:17}]],
  ['nonboolean-bad-step',[{...contract,enableTrade:'truthy',stepSize:{toString:null}}]],
  ['nonboolean-before-later-bad-step',[{...contract,enableTrade:'truthy'},{...contract,stepSize:{toString:null}}]],
  ['numeric-id-before-later-bad-step',[{...contract,symbol:17},{...contract,stepSize:{toString:null}}]],
  ['missing-config',undefined],['null-config',undefined],['null-data',undefined],
  ['null-root',undefined],['missing-data',undefined],
  ['null-contracts',null],['number-contracts',17],['empty',[]],['null-row',[null]],
  ['primitive-prefix',[17,contract]],
 ];
 if (!history) for (const [label,contracts] of marketCases) {
  const s = store(); const venue = new Class('apexomni',{key:'key',secret:'secret'},s);
  const payload = label==='missing-config'?{data:{}}:label==='null-config'?{data:{contractConfig:null}}:
                  label==='null-data'?{data:null}:label==='null-root'?null:label==='missing-data'?{}:
                  {data:{contractConfig:{perpetualContract:contracts}}};
  if(label==='missing-config')s.zkLinkConfig.contractConfig={stale:true};
  responses = {'/v3/symbols':payload}; operations=[];
  let result;
  try {result={ok:true,value:native(await venue.fetchMarketsHelper())};}
  catch(error){result={ok:false,name:error.name,message:error.message};}
  rows.push({method:'fetchMarketsHelper',label,payload:native(payload),result,
             operations:native(operations),zk:native(s.zkLinkConfig)});
 }
 if (!history) for (const field of ['symbol','side','createdAt','price','fillSize','orderId','fee']) {
  const s=store();const venue=new Class('apexomni',{key:'key',secret:'secret'},s);
  const input={...fill,token:'USDC',[field]:{toString:null}};operations=[];
  let result;
  try{venue.recordWsExecution(input);result={ok:true};}
  catch(error){result={ok:false,name:error.name,message:error.message};}
  rows.push({method:'recordWsExecutionError',label:field,input:native(input),result,
             operations:native(operations),cache:native(venue.recentWsExecutions)});
 }
 for (const [label,asset] of assets) for (const hasCache of history ? [false,true] : [false]) {
  const s=store(); const venue=new Class('apexomni',{key:'key',secret:'secret'},s);
  if(hasCache)venue.recordWsExecution(cached);
  const input={...fill,token:asset};
  responses={'/v3/fills':{data:{fills:[input]}},'/v3/history-orders':{data:{orders:[]}}};operations=[];
  const result=history ? await venue.fetchHistoricalFills({symbol:'BTCUSDC',limit:100})
                       : (venue.recordWsExecution(input),venue.recentWsExecutions);
  assert.equal(result.length,1,'APEX-BOUNDARY source keeps one raw execution');
  const selected=asset || 'USDC';
  assert.deepEqual(result[0].commissionAsset,selected,'APEX-BOUNDARY source asset identity');
  rows.push({method:history?'fetchHistoricalFills':'recordWsExecution',label,hasCache,
             input:native(input),cached:native(cached),result:native(result),
             operations:native(operations),cache:native(venue.recentWsExecutions)});
 }
 if (history) for (const [label,inputs,options] of [
  ['raw-before-null',[{...fill,token:17},null],{symbol:'BTCUSDC',limit:100}],
  ['raw-before-coercion-error',[{...fill,token:17},{...fill,fillPrice:{toString:null}}],{symbol:'BTCUSDC',limit:100}],
  ['raw-outside-window',[{...fill,token:17}],{symbol:'BTCUSDC',limit:100,from:1700000010}],
  ['raw-overwritten',[{...fill,token:17},{...fill,token:'USDC'}],{symbol:'BTCUSDC',limit:100}],
  ['raw-truncated',[{...fill,token:17},{...fill,token:'USDC',createdAt:1700000001000}],{symbol:'BTCUSDC',limit:1}],
 ]) {
  const s=store(); const venue=new Class('apexomni',{key:'key',secret:'secret'},s);
  venue.recordWsExecution(cached);
  responses={'/v3/fills':{data:{fills:inputs}},'/v3/history-orders':{data:{orders:[]}}};operations=[];
  const result=await venue.fetchHistoricalFills(options);
  rows.push({method:'fetchHistoricalFills',label,hasCache:true,inputs:native(inputs),options,
             cached:native(cached),result:native(result),operations:native(operations),cache:native(venue.recentWsExecutions)});
 }
 assert.equal(rows.length,history?21:32,'APEX-BOUNDARY independent source floor');
 console.log(JSON.stringify({rows,assetRows,header:{node:process.version,arch:process.arch}}));
})().catch(error=>{console.error(error);process.exitCode=1;});
`);
const m = new Module(filename, module);
m.filename = filename;
m.paths = Module._nodeModulePaths(path.dirname(filename));
m._compile(code, filename);
