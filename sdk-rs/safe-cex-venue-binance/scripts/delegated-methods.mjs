import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import {createRequire} from 'node:module';
const require=createRequire(import.meta.url),ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const results=[];
for(const [variant,klass,methods] of [['binance','BinanceExchange',['listenOrderBook','fetchHistoricalPositions']],['binancepm','BinancePMExchange',['listenOHLCV','listenOrderBook']],['binancespot','BinanceSpotExchange',['listenOHLCV','listenOrderBook']]]){
 const file=new URL(`../../../packages/safe-cex/src/exchanges/binance/${variant}.exchange.ts`,import.meta.url),ast=ts.createSourceFile(file.pathname,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true),decl=ast.statements.find(n=>ts.isClassDeclaration(n)&&n.name?.text===klass);
 for(const method of methods){
  const field=decl.members.find(n=>n.name?.getText(ast)===method);assert.ok(field?.initializer);
  const exports={};vm.runInNewContext(ts.transpileModule(`exports.fn=function(...args){return (${field.initializer.getText(ast)})(...args);};`,{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText,{exports});
  if(method==='fetchHistoricalPositions'){
   for(const mode of ['ok','sync-reject','reject']){
    const opts={account:'account-A',from:100,to:200},records=[{symbol:'BTCUSDT',size:3.6},{symbol:'ETHUSDT',size:7}],failure=RangeError('fold failure');let calls=0,received=null,error=null,value=null;
    try{value=await exports.fn.call({foldClosedPositionRecords:(arg,config)=>{calls++;received={sameOptions:arg===opts,account:arg.account,from:arg.from,to:arg.to,symbolScoped:config.symbolScoped};if(mode==='sync-reject')throw failure;return mode==='reject'?Promise.reject(failure):Promise.resolve(records);}},opts);}catch(e){assert.equal(e,failure);error='fold';}
    results.push({variant,method,mode,calls,received,error,sameResult:value===records,value});
   }
   continue;
  }
  for(const external of variant==='binance'?[false,true]:[false])for(const mode of ['ok','listen-reject','callback-reject','cleanup-reject']){
   const candle=method==='listenOHLCV',opts={symbol:'BTCUSDT',interval:'1m',from:100,to:200,limit:7},payload=candle?{timestamp:1000,open:11,high:22,low:3,close:12.3,volume:3.6}:{bids:[{price:12.3,amount:3.6,total:3.6}],asks:[{price:12.4,amount:7,total:7}]};
   const errors=Object.fromEntries(['listen','callback','cleanup'].map(role=>[role,RangeError(`${role} failure`)]));let calls=0,callbacks=0,cleanups=0,received=null,error=null,sameCleanup=null,samePayload=null;
   const callback=arg=>{callbacks++;samePayload=arg===payload;if(candle)arg.close=20;else arg.bids[0].price=18;if(mode==='callback-reject')throw errors.callback;};
   const cleanup=()=>{cleanups++;if(mode==='cleanup-reject')throw errors.cleanup;};
   const listen=role=>(arg,cb)=>{calls++;received={role,sameOptions:candle?arg===opts:null,symbol:candle?arg.symbol:arg,sameCallback:cb===callback};if(mode==='listen-reject')throw errors.listen;if(candle)arg.limit=9;cb(payload);return cleanup;};
   const instance={publicWebsocket:{[method]:listen('public')},externalPublicDataProvider:external?{subscribeOrderbook:listen('external')}:null};
   try{const returned=exports.fn.call(instance,candle?opts:'BTCUSDT',callback);sameCleanup=returned===cleanup;returned();returned();}catch(e){error=Object.entries(errors).find(([,original])=>e===original)?.[0];assert.ok(error);}
   results.push({variant,method,mode,external,calls,callbacks,cleanups,received,error,sameCleanup,samePayload,opts,payload});
  }
 }
}
assert.equal(results.length,27);
process.stdout.write(JSON.stringify(results));
