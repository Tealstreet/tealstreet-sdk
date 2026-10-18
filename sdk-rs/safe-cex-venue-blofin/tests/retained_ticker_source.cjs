const assert=require('node:assert/strict');
const path=require('node:path');
const root=path.resolve(__dirname,'../../..');
const {BlofinExchange}=require(path.join(root,'packages/safe-cex/src/exchanges/blofin/blofin.exchange.ts'));
const {DefaultStore}=require(path.join(root,'packages/safe-cex/src/store/store.base.ts'));
(async()=>{
  const output=[];
  for(const native of [false,true]) {
    const store=new DefaultStore();
    store.update({markets:[{id:'BTC-USDT',symbol:'BTCUSDT',base:'BTC',quote:'USDT',active:true,precision:{amount:0.001,price:0.1},limits:{amount:{min:0.001,max:1000},leverage:{min:1,max:50}}}]});
    const venue=new BlofinExchange('blofin',{key:'fixture-key',secret:'fixture-secret',passphrase:'fixture-passphrase'},store);
    const raw='{"code":"0","data":[{"instId":"BTC-USDT","open24h":"100","last":"110","bidPrice":"109","askPrice":"111","volCurrency24h":"12","vol24h":"1320","opaque":{"text":"\\ud800","\\udc00":"opaque-key-value","overflow":1e400,"negativeZero":-0}}]}';
    const body=JSON.parse(raw);
    if(native) {body.data[0].opaque.nan=NaN;body.data[0].opaque.ownUndefined=undefined;}
    let calls=0;
    for(const client of new Set([venue.xhr,venue.unlimitedXHR,venue._primaryXhr,venue._secondaryXhr].filter(Boolean))) client.defaults.adapter=async config=>{
      assert.equal(config.url,'/api/v1/market/tickers');calls++;
      return {status:200,statusText:'OK',headers:{'content-type':'application/json'},config,data:native?body:raw};
    };
    try {
      const tickers=await venue.fetchTickers();
      assert.equal(tickers.length,1);assert.equal(tickers[0].last,110);assert.equal(calls,1);
      const opaque=tickers[0].info.opaque;
      assert.equal(opaque.text.charCodeAt(0),0xd800);assert.equal(opaque['\udc00'],'opaque-key-value');
      assert.equal(opaque.overflow,Infinity);assert.ok(Object.is(opaque.negativeZero,-0));
      if(native) {assert.ok(Number.isNaN(opaque.nan));assert.ok(Object.hasOwn(opaque,'ownUndefined'));assert.equal(opaque.ownUndefined,undefined);}
      output.push({native,textUnits:[opaque.text.charCodeAt(0)],keyUnits:[Object.keys(opaque).find(k=>k.charCodeAt(0)===0xdc00).charCodeAt(0)],overflow:'infinity',negativeZero:true,ownUndefined:Object.hasOwn(opaque,'ownUndefined'),nan:native?Number.isNaN(opaque.nan):false});
    } finally {venue.dispose();}
  }
  process.stdout.write(JSON.stringify(output));
})().catch(error=>{console.error(error);process.exitCode=1;});
