'use strict';
const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const root=path.resolve(__dirname,'../../../packages/safe-cex/src/exchanges/woo');
function load(file,names,context={}) {
 const ast=ts.createSourceFile(file,fs.readFileSync(path.join(root,file),'utf8'),ts.ScriptTarget.Latest,true);
 const found=ast.statements.filter(ts.isVariableStatement).filter(n=>n.declarationList.declarations.some(d=>names.includes(d.name.getText(ast))));
 assert.equal(found.length,names.length,'WS-TOPIC actual source declarations/'+file);
 const exports={};vm.runInNewContext(ts.transpileModule(found.map(n=>n.getText(ast)).join('\n')+'\nexport { '+names.join(',')+' };',{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText,{exports,...context});return exports;
}
const contracts=load('woo.v3.contracts.ts',['WOO_V3_WS_URL','WOO_V3_ENDPOINTS']);
const api=load('woo.v3.websocket.ts',['WOO_V3_WS_CHANNELS','WOO_V3_WS_LIMITS','WOO_V3_WS_PUBLIC_TOPIC_KINDS','WOO_V3_WS_PRIVATE_TOPICS','WS_SYMBOL_PATTERN','KLINE_INTERVALS','requireString','requireSymbol','buildWooV3PublicTopic','buildWooV3PrivateTopic','parseWooV3WsTopic'],contracts);
const publicKinds=['orderbookupdate','orderbookupdaterpi','orderbook10','bbo','bborpi','trade','ticker','kline','indexprice','markprice','estfundingrate','openinterest'];
const privateKinds=['account','balance','position','executionreport','algoexecutionreport'];
const intervals=['1m','3m','5m','15m','30m','1h','2h','4h','6h','12h','1d','3d','1w','1M'];
assert.deepEqual([...api.WOO_V3_WS_PUBLIC_TOPIC_KINDS],publicKinds);
assert.deepEqual([...api.WOO_V3_WS_PRIVATE_TOPICS],privateKinds);
assert.deepEqual([...api.KLINE_INTERVALS],intervals);
function capture(fn,...args){try{return {ok:fn(...args)};}catch(e){return {error:{name:e.name,message:e.message}};}}
const builds=[];
for(const kind of publicKinds){
 if(kind.startsWith('orderbookupdate'))for(const option of [50,200,500])builds.push({kind,symbol:'PERP_BTC_USDT',option});
 else if(kind==='kline')for(const option of intervals)builds.push({kind,symbol:'SPOT_ETH_USDC',option});
 else builds.push({kind,symbol:'PERP_10BTC_USDT'});
}
const buildErrors=[
 {kind:'bbo',symbol:null},{kind:'bbo',symbol:''},{kind:'bbo',symbol:'BTCUSDT'},
 {kind:'bbo',symbol:'PERP_btc_USDT'},{kind:'bbo',symbol:'PERP_BTC_USDT\n'},
 {kind:'bbo',symbol:'PERP_BTC_USDT',option:null},{kind:'bbo',symbol:'PERP_BTC_USDT',option:50},
 {kind:'orderbookupdate',symbol:'PERP_BTC_USDT'},{kind:'orderbookupdate',symbol:'PERP_BTC_USDT',option:'50'},
 {kind:'orderbookupdate',symbol:'PERP_BTC_USDT',option:51},{kind:'orderbookupdaterpi',symbol:'PERP_BTC_USDT',option:0},
 {kind:'kline',symbol:'PERP_BTC_USDT'},{kind:'kline',symbol:'PERP_BTC_USDT',option:1},
 {kind:'kline',symbol:'PERP_BTC_USDT',option:'1s'},{kind:'kline',symbol:'bad',option:'bad'},
];
const buildRows=[...builds,...buildErrors].map(spec=>({...spec,...capture(api.buildWooV3PublicTopic,spec.kind,spec.symbol,spec.option)}));
for(const row of buildRows)if(row.ok)assert.equal(Object.isFrozen(row.ok),true,'WS-TOPIC source descriptor frozen');
const privateRows=[...privateKinds,'unknown','',null,3,[],{toString:null}].map(input=>({input,...capture(api.buildWooV3PrivateTopic,input)}));
const parseInputs=[
 ...builds.map(spec=>`${spec.kind}@${spec.symbol}${spec.option===undefined?'':'@'+spec.option}`),...privateKinds,
 ...['0x32','050','+50','5e1',' 50 ','50.0','0b110010','\uFEFF50\uFEFF'].map(depth=>'orderbookupdate@PERP_BTC_USDT@'+depth),
 null,3,'','unknown@PERP_BTC_USDT','bbo','bbo@PERP_BTC_USDT@50','orderbookupdate@PERP_BTC_USDT',
 'orderbookupdate@PERP_BTC_USDT@bad','orderbookupdate@PERP_BTC_USDT@Infinity','orderbookupdate@PERP_BTC_USDT@50@x',
 'orderbookupdaterpi@bad@200','kline@PERP_BTC_USDT','kline@PERP_BTC_USDT@bad','kline@PERP_BTC_USDT@1m@x',
];
const parseRows=parseInputs.map(input=>({input,...capture(api.parseWooV3WsTopic,input)}));
const stringRows=[{}, {input:null},{input:3},{input:[]},{input:''},{input:'',allowEmpty:true},{input:'é𐐀'},{input:'text'}].map(spec=>({...spec,...capture(api.requireString,spec.input,'test.path',spec.allowEmpty)}));
const symbolRows=[{}, {input:null},{input:'PERP_BTC_USDT'},{input:'SPOT_10BTC_USDC'},{input:'PERP_BTC_USDT\n'},{input:'PERP_BTC_USDT_EXTRA'},{input:'PERP__USDT'},{input:'SPOT_É_USDC'}].map(spec=>({...spec,...capture(api.requireSymbol,spec.input,'test.symbol')}));
assert.equal(buildRows.length,44,'WS-TOPIC build floor');assert.equal(privateRows.length,11,'WS-TOPIC private floor');
assert.equal(parseRows.length,56,'WS-TOPIC parse floor');assert.equal(stringRows.length,8);assert.equal(symbolRows.length,8);
console.log(JSON.stringify({channels:api.WOO_V3_WS_CHANNELS,limits:api.WOO_V3_WS_LIMITS,publicKinds,privateKinds,intervals,buildRows,privateRows,parseRows,stringRows,symbolRows}));
