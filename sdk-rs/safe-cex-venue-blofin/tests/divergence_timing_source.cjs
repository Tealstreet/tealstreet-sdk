// Full maintained venue and Store; only terminal Axios I/O is scripted.
const assert = require('node:assert/strict');
const path = require('node:path');
const root = path.resolve(__dirname, '../../..');
const {BlofinExchange} = require(path.join(root, 'packages/safe-cex/src/exchanges/blofin/blofin.exchange.ts'));
const {DefaultStore} = require(path.join(root, 'packages/safe-cex/src/store/store.base.ts'));
(async () => {
  const store = new DefaultStore();
  store.update({fetchedLeverage:{BTCUSDT:{buyLeverage:3,sellLeverage:5,marginType:'cross',positionMode:'oneway'}}});
  const venue = new BlofinExchange('blofin', {key:'fixture-key',secret:'fixture-secret',passphrase:'fixture-passphrase',tuning:{consoleLogging:false}}, store);
  const logs = [], requests = [];
  venue.on('log', (message,severity) => logs.push([message,severity]));
  for (const client of new Set([venue.xhr,venue.unlimitedXHR,venue._primaryXhr,venue._secondaryXhr].filter(Boolean))) client.defaults.adapter = async config => {
    requests.push([config.method,config.url]);
    assert.equal(config.url, '/api/v1/account/set-position-mode');
    return {data:{code:'0',msg:'success',data:{positionMode:'long_short_mode'}},status:200,statusText:'OK',headers:{},config};
  };
  try {
    const pending = venue.changePositionMode(true);
    const prefix = logs.map(row => [...row]);
    assert.deepEqual(prefix, [
      ['[blofin] Attempting to change position mode to long_short_mode','info'],
      ['[blofin] Current positions count: 0','info'],
      ['[blofin] Current orders count: 0','info'],
    ]);
    const result = await pending;
    assert.equal(result, undefined, "actual worker producer returns void, not arbitrary positionMode records");
    assert.equal(logs.length,4);
    assert.equal(requests.length,1);
    assert.equal(store.fetchedLeverage.BTCUSDT.positionMode,'hedged');
    process.stdout.write(JSON.stringify({prefix,settled:logs,requests,positionMode:store.fetchedLeverage.BTCUSDT.positionMode}));
  } finally { venue.dispose(); }
})().catch(error=>{console.error(error);process.exitCode=1;});
