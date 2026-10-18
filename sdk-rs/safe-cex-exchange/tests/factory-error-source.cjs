const assert = require('node:assert/strict');
const path = require('node:path');
const root = path.resolve(__dirname, '../../..');
const load = file => require(path.join(root, 'packages/safe-cex/src', file));
const {AsterDEXExchange} = load('exchanges/asterdex/asterdex.exchange.ts');
const {WOOXExchange} = load('exchanges/woo/woo.exchange.ts');
const axios = require('axios');
axios.defaults.adapter = async config => ({config,status:200,data:{serverTime:Date.now()},headers:{}});
const {DefaultStore} = load('store/store.base.ts');
(async () => {
  for (const [name, Type, client, invoke] of [
    ['asterdex', AsterDEXExchange, 'primaryAPI', venue => venue.syncPositionMode()],
    ['woo', WOOXExchange, 'xhr', venue => venue.xhr.get('/v3/accountinfo')],
  ]) {
    for (const original of ['bare rejection', 42, false]) {
      const venue = new Type(name, {key:'fixture', secret:'fixture', applicationId:'fixture-app', tuning:{consoleLogging:false}}, new DefaultStore());
      try {
        venue[client].defaults.adapter = async () => { throw original; };
        await assert.rejects(invoke(venue), error => {
          assert(error instanceof Error);
          assert.equal(error.name, 'Error');
          assert.equal(error.message, `[${name}] Request setup error: undefined`);
          assert.equal(error.enhancedMessage, error.message);
          assert.equal(error.isAxiosError, true);
          assert.equal(error.originalError, original);
          assert.equal(new Date(error.context.timestamp).toISOString(), error.context.timestamp);
          return true;
        });
      } finally { venue.dispose(); }
    }
  }
  process.stdout.write(JSON.stringify({asterdex:3,woo:3}));
})().catch(error => {console.error(error);process.exitCode=1});
