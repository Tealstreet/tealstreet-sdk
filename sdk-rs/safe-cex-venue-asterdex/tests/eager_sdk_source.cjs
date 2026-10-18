const assert = require('node:assert/strict');
const path = require('node:path');
const root = path.resolve(__dirname, '../../..');
const { AsterDEXExchange } = require(path.join(root, 'packages/safe-cex/src/exchanges/asterdex/asterdex.exchange.ts'));
const { DefaultStore } = require(path.join(root, 'packages/safe-cex/src/store/store.base.ts'));
(async () => {
  const store = new DefaultStore();
  const venue = new AsterDEXExchange('asterdex', { tuning: { consoleLogging: false } }, store);
  const logs = [];
  venue.on('log', message => logs.push(message));
  try {
    const pending = venue.validateAccount();
    const prefix = [...logs];
    await assert.rejects(pending, /Missing required credentials: API key/);
    assert.deepEqual(prefix, ['[asterdex] Account validation failed: Missing required credentials: API key']);
    venue.options.key = 'synthetic-key';
    venue.options.secret = 'synthetic-secret';
    let release;
    const held = new Promise(resolve => { release = resolve; });
    const requests = [];
    venue.primaryAPI.defaults.adapter = async config => {
      requests.push([config.method, config.url]);
      if (config.url.endsWith('positionSide/dual')) await held;
      return { data: config.url.endsWith('account') ? { accountAlias: 'native-account' } : [], status: 200, statusText: 'OK', headers: {}, config };
    };
    void venue.getAccount();
    void venue.changePositionMode(true);
    for (let turn = 0; turn < 200 && requests.length < 2; turn++) await new Promise(resolve => setTimeout(resolve, 5));
    assert.equal(store.options.isHedged, false);
    release();
    for (let turn = 0; turn < 200 && requests.length < 3; turn++) await new Promise(resolve => setTimeout(resolve, 5));
    assert.equal(store.options.isHedged, true);
    assert.deepEqual(requests, [['get', '/fapi/v4/account'], ['post', '/fapi/v1/positionSide/dual'], ['get', '/fapi/v1/positionRisk']]);
    process.stdout.write(JSON.stringify({ prefix, requests, hedged: store.options.isHedged }));
  } finally { venue.dispose(); }
})().catch(error => { console.error(error); process.exitCode = 1; });
