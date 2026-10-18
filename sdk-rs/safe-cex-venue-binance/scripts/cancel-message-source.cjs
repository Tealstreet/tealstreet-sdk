'use strict';
const assert = require('node:assert/strict');
const path = require('node:path');
const {root, load, external} = require('./inverse-source-loader.cjs');
const {BinanceExchange} = load(path.join(root, 'packages/safe-cex/src/exchanges/binance/binance.exchange.ts'));
const {DefaultStore} = load(path.join(root, 'packages/safe-cex/src/store/store.base.ts'));
const axios = external('axios');
const units = value => Array.from({length:value.length}, (_,index) => value.charCodeAt(index));
(async () => {
  const output = [];
  for (const msg of ['\ud800raw\udfff', 'ordinary failure']) {
    const venue = new BinanceExchange('binance', {key:'alias-key',secret:'alias-secret'}, new DefaultStore());
    const errors = [], calls = [];
    let original;
    venue.on('error', (...args) => errors.push(args));
    try {
      for (const client of new Set([venue.xhr,venue._primaryXhr,venue._secondaryXhr])) {
        if (!client) continue;
        client.defaults.adapter = async config => {
          assert.equal(config.method,'delete');
          assert.equal(config.params.symbol,'BTCUSDT');
          assert.match(config.params.signature,/^[a-f0-9]{64}$/);
          calls.push(config.url);
          const status = config.url.includes('algoOpenOrders') ? 400 : 200;
          const response = {status,statusText:'',headers:{},config,data:JSON.stringify(status===400?{msg}:{})};
          if (status===200) return response;
          original = new axios.AxiosError('Request failed with status code 400','ERR_BAD_REQUEST',config,undefined,response);
          throw original;
        };
      }
      const caught = await venue.cancelSymbolOrders('BTCUSDT').then(() => assert.fail('must reject'), error => error);
      assert.equal(caught,original);
      assert.deepEqual(calls.sort(),['/fapi/v1/algoOpenOrders','/fapi/v1/allOpenOrders'].sort());
      assert.equal(errors.length,2);
      for (const args of errors) { assert.equal(args.length,2); assert.equal(typeof args[0],'string'); }
      assert.equal(caught.response.data.msg,msg);
      output.push({response:units(caught.response.data.msg),arity:errors.map(args=>args.length),emitted:errors.map(args=>units(args[0])),json:JSON.stringify(caught.response.data)});
    } finally { venue.dispose(); }
  }
  process.stdout.write(JSON.stringify(output));
})().catch(error => { console.error(error); process.exitCode=1; });
