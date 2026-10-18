'use strict';
const assert = require('node:assert/strict');
const path = require('node:path');
const {root, load, external} = require('./inverse-source-loader.cjs');
const {BinanceExchange} = load(path.join(root, 'packages/safe-cex/src/exchanges/binance/binance.exchange.ts'));
const {DefaultStore} = load(path.join(root, 'packages/safe-cex/src/store/store.base.ts'));
const axios = external('axios');
const tick = () => new Promise(resolve => setTimeout(resolve, 5));
(async () => {
  const output = [];
  for (const algoStatus of [200, 500]) {
    const store = new DefaultStore();
    const venue = new BinanceExchange('binance', {key:'alias-key',secret:'alias-secret'}, store);
    const errors = [], calls = new Map();
    let original, settled = false;
    venue.on('error', error => errors.push(error));
    try {
      const regular = {id:'9007199254740993',symbol:'BTCUSDT',type:'limit',side:'buy',price:91.2,
        amount:3,filled:0,remaining:3,status:'open',reduceOnly:false,lastUpdate:0};
      store.update({orders:[regular,{...regular,id:'algo-order',type:'stop_market'},
        {...regular,id:'other-symbol',symbol:'ETHUSDT'}]});
      for (const client of new Set([venue.xhr,venue._primaryXhr,venue._secondaryXhr])) {
        if (!client) continue;
        client.defaults.adapter = config => new Promise((resolve,reject) => {
          assert.equal(config.method,'delete');
          assert.equal(config.params.symbol,'BTCUSDT');
          assert.match(config.params.signature,/^[a-f0-9]{64}$/);
          assert.equal(config.headers.get('X-MBX-APIKEY'),'alias-key');
          assert(['/fapi/v1/allOpenOrders','/fapi/v1/algoOpenOrders'].includes(config.url));
          assert(!calls.has(config.url),'each signed DELETE starts once');
          calls.set(config.url, () => {
            const status = config.url.includes('algoOpenOrders') ? algoStatus : 200;
            const response = {status,statusText:'',headers:{},config,data:JSON.stringify({msg:'algo refused'})};
            if (status === 200) resolve(response);
            else {
              original = new axios.AxiosError('Request failed with status code 500','ERR_BAD_RESPONSE',config,undefined,response);
              reject(original);
            }
          });
        });
      }
      const pending = venue.cancelSymbolOrders('BTCUSDT').then(
        () => { settled = true; return {ok:true}; },
        error => { settled = true; assert.equal(error,original); return {ok:false,status:error.response.status,msg:error.response.data.msg}; });
      const deadline = Date.now() + 2000;
      while (calls.size < 2 && Date.now() < deadline) await tick();
      assert.equal(calls.size,2,'both actual source HTTP requests started');
      assert.equal(settled,false);
      calls.get('/fapi/v1/allOpenOrders')();
      await tick();
      assert.equal(settled,false,'algo reply remains pending');
      assert(store.orders.every(order => order.status === 'open'));
      calls.get('/fapi/v1/algoOpenOrders')();
      const result = await pending;
      output.push({...result,orders:store.orders.map(order => [order.id,order.status]),errors:errors.length});
    } finally { venue.dispose(); }
  }
  process.stdout.write(JSON.stringify(output));
})().catch(error => { console.error(error); process.exitCode = 1; });
