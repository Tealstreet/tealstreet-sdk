'use strict';
const assert = require('node:assert/strict');
const path = require('node:path');
const root = path.resolve(__dirname, '../../..');
const { DefaultStore } = require(path.join(root, 'packages/safe-cex/src/store/store.base.ts'));
const { KucoinExchange } = require(path.join(root, 'packages/safe-cex/src/exchanges/kucoin/kucoin.exchange.ts'));
const { setTelemetrySink } = require(path.join(root, 'packages/safe-cex/src/telemetry.ts'));
const store = new DefaultStore();
store.update({ markets: [{ id: 'XBTUSDTM', symbol: 'BTCUSDT', contractSize: .001 }] });
const exchange = new KucoinExchange('kucoin', {
  key: 'fixture-key', secret: 'fixture-secret', passphrase: 'fixture-pass', applicationId: 'fixture',
}, store);
const reports = [], events = [], requests = [];
setTelemetrySink({ captureException(error) { reports.push(error); } });
exchange.emitter.on('error', message => events.push(message));
exchange.xhr.defaults.adapter = async config => {
  requests.push([config.method, config.url]);
  assert.equal(config.method, 'get');
  assert.equal(config.url, '/api/v1/positions');
  return { config, status: 200, statusText: 'OK', headers: {}, data: { code: '200000', data: [{
    symbol: 'XBTUSDTM', currentQty: 5, markPrice: '100', avgEntryPrice: '99', leverage: '10',
    unrealisedPnl: '1.5', liquidationPrice: '50', marginMode: 'CROSS', positionSide: 'BOTH',
  }] } };
};
(async () => {
  let rejection;
  try { await exchange.changePositionMode(true); } catch (error) { rejection = error; }
  try {
    assert.equal(reports.length, 2, 'preflight and catch each report');
    assert.equal(events.length, 2, 'preflight and catch each emit');
    assert.equal(requests.length, 1, 'refusal sends only the actual position read');
    for (const error of reports) {
      assert(error instanceof Error);
      assert.equal(error.name, 'Error');
      assert.equal(error.message, 'Cannot update position mode with open positions');
    }
    assert.notEqual(reports[0], reports[1], 'source constructs a fresh thrown Error');
    assert.equal(reports[1], rejection, 'catch reports the original thrown Error');
    process.stdout.write(JSON.stringify({ reports, events, rejection: { name: rejection.name, message: rejection.message } }));
  } finally { exchange.dispose(); setTelemetrySink({}); }
})().catch(error => { console.error(error); process.exitCode = 1; });
