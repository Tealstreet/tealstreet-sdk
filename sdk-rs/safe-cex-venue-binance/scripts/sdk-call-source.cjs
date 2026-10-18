// Complete maintained SDK constructors and fetchOHLCV; terminal Axios only.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const {root, load, external} = require('./inverse-source-loader.cjs');
const {DefaultStore} = load(path.join(root, 'packages/safe-cex/src/store/store.base.ts'));
const axios = external('axios');
const {setTelemetrySink}=load(path.join(root,'packages/safe-cex/src/telemetry.ts'));
const inputs = JSON.parse(fs.readFileSync(path.join(__dirname, '../fixtures/sdk-call-inputs.json')));
function number(value) {
  if (Number.isNaN(value)) return {kind:'NaN'};
  const bytes=Buffer.alloc(8);bytes.writeDoubleBE(value);return {bits:bytes.toString('hex')};
}
(async()=>{
  const outputs=[];
  for(const input of inputs.filter(input=>!input.argumentError)) {
    const [stem,name] = {futures:['binance','BinanceExchange'],pm:['binancepm','BinancePMExchange'],spot:['binancespot','BinanceSpotExchange']}[input.family];
    const Subject = load(path.join(root, `packages/safe-cex/src/exchanges/binance/${stem}.exchange.ts`))[name];
    const reports=[];setTelemetrySink({captureException:error=>reports.push(error)});
    const subject = new Subject(stem,{key:'synthetic-key',secret:'synthetic-secret',tuning:{consoleLogging:false}},new DefaultStore());
    const requests=[];
    const adapter=async config=>{
      const uri=axios.getUri(config),url=new URL(uri);
      requests.push({method:config.method,base:config.baseURL,path:url.pathname,query:url.search.slice(1).split('&').filter(Boolean).filter(field=>!['timestamp=','recvWindow=','signature='].some(key=>field.startsWith(key))),signed:url.search.includes('signature=')});
      assert.equal(url.pathname,input.family==='spot'?'/api/v3/klines':'/fapi/v1/klines');
      return {data:input.data,status:200,statusText:'OK',headers:{'content-type':'application/json'},config};
    };
    for(const client of new Set([subject.xhr,subject.publicXhr,subject.papiXhr,subject.fapiXhr,subject.unlimitedXHR].filter(Boolean)))client.defaults.adapter=adapter;
    try {
      const rows=await subject.fetchOHLCV(input.options);
      if(input.profile==='nonfinite')assert.ok(Number.isNaN(rows[0].volume));else assert.ok(Object.is(rows[0].open,-0));
      const candles=rows.map(row=>({timestamp:number(row.timestamp),open:number(row.open),high:number(row.high),low:number(row.low),close:number(row.close),volume:number(row.volume),lastSide:row.lastSide??null}));
      assert.equal(rows.length,1);assert.equal(requests.length,1);assert.equal(reports.length,0);
      outputs.push({input,output:{result:{candles},requests,reports:reports.length}});
    }finally{subject.dispose();setTelemetrySink({});}
  }
  assert.equal(outputs.length,6);process.stdout.write(JSON.stringify(outputs));
})().catch(error=>{console.error(error);process.exitCode=1});
