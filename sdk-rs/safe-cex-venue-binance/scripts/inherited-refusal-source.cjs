// Actual maintained constructors and inherited methods; terminal IO guard only.
const assert=require('node:assert/strict');
const path=require('node:path');
const {root,load}=require('./inverse-source-loader.cjs');
const {DefaultStore}=load(path.join(root,'packages/safe-cex/src/store/store.base.ts'));
const {setTelemetrySink}=load(path.join(root,'packages/safe-cex/src/telemetry.ts'));
const rows=[];
(async()=>{
 for(const [stem,name,methods] of [
  ['binance','BinanceExchange',['fetchSymbolMaxLeverage']],
  ['binancepm','BinancePMExchange',['fetchSymbolMaxLeverage','fetchPublicTrades']],
  ['binancespot','BinanceSpotExchange',['fetchSymbolMaxLeverage','fetchPublicTrades','setLeverage','changePositionMode','changeMarginType']],
 ]) {
  const Subject=load(path.join(root,`packages/safe-cex/src/exchanges/binance/${stem}.exchange.ts`))[name];
  for(const method of methods)for(const variant of [0,1]) {
   const store=new DefaultStore();
   const subject=new Subject(stem,{key:'synthetic-key',secret:'synthetic-secret',tuning:{consoleLogging:false}},store);
   let requests=0;const events=[],reports=[];
   subject.on('error',value=>events.push(value));setTelemetrySink({captureException:value=>reports.push(value)});
   for(const client of new Set([subject.xhr,subject.papiXhr,subject._primaryXhr,subject._secondaryXhr,subject.publicXhr,subject.unlimitedXHR,subject.fapiXhr].filter(Boolean)))client.defaults.adapter=async()=>{requests++;throw new Error('unexpected inherited-refusal HTTP');};
   const symbol=variant?'ETHUSDT':'BTCUSDT';
   const args=method==='changePositionMode'||method==='changeMarginType'?[!!variant,symbol]:method==='setLeverage'?[symbol,variant?23:17]:method==='fetchPublicTrades'?[symbol,variant?0:17]:[symbol];
   try {
    let caught;try{await subject[method](...args)}catch(error){caught=error}
    assert.ok(caught instanceof Error,`${name}.${method}: actual Error`);
    assert.equal(caught.name,'Error');assert.equal(caught.message,'Not implemented');
    assert.equal(requests,0);assert.equal(events.length,0);assert.equal(reports.length,0);
    rows.push({stem,method,args,error:caught.message,error_name:caught.name,http:requests,events:events.length,reports:reports.length});
   }finally {subject.dispose();setTelemetrySink({});}
  }
 }
 assert.equal(rows.length,16);process.stdout.write(JSON.stringify(rows));
})().catch(error=>{console.error(error);process.exitCode=1});
