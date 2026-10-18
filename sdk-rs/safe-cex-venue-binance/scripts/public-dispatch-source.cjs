'use strict';
const assert = require('node:assert/strict');
const path = require('node:path');
const root = path.resolve(__dirname, '../../..');
const { DefaultStore } = require(path.join(root, 'packages/safe-cex/src/store/store.base.ts'));
const fields = ['bid', 'ask', 'last', 'mark', 'index', 'percentage', 'volume',
  'quoteVolume', 'fundingRate', 'fundingTime', 'openInterest'];
function bits(value) {
  const buffer = Buffer.alloc(8);
  buffer.writeDoubleBE(value);
  return buffer.toString('hex');
}
const describe = ticker => Object.fromEntries(fields.map(field => [field, bits(ticker[field])]));
(async () => {
  const cases = [];
  for (const [venue, name] of [['binance', 'BinanceExchange'],
    ['binancepm', 'BinancePMExchange'], ['binancespot', 'BinanceSpotExchange']]) {
    const Exchange = require(path.join(root, `packages/safe-cex/src/exchanges/binance/${venue}.exchange.ts`))[name];
    for (const key of ['constructor', 'toString', '__proto__', 'not-owned']) {
      const store = new DefaultStore();
      store.update({ tickers: [{ id: 'BTCUSDT', symbol: 'BTCUSDT', bid: 99, ask: 101,
        last: 100, mark: 100, index: 100, percentage: 0, volume: 0, quoteVolume: 0,
        fundingRate: 0, fundingTime: 0, openInterest: 0 }] });
      const exchange = new Exchange(venue, { publicDataOnly: true, applicationId: 'fixture' }, store);
      try {
        const captured = store.tickersBySymbol.BTCUSDT;
        const before = describe(captured);
        let error = null;
        try {
          await exchange.publicWebsocket.onMessage({ data: JSON.stringify({
            e: 'bookTicker', s: key, b: { toString: null }, a: '123.25',
          }) });
        } catch (failure) {
          assert(failure instanceof TypeError);
          error = { name: failure.name, message: failure.message };
        }
        assert.equal(error !== null, key !== 'not-owned', `${venue} inherited lookup coercion ${key}`);
        const afterBad = describe(store.tickers[0]);
        assert.deepEqual(afterBad, before);
        await exchange.publicWebsocket.onMessage({ data: JSON.stringify({
          e: 'bookTicker', s: 'BTCUSDT', b: '-0', a: '123.25',
        }) });
        assert.equal(bits(captured.bid), '8000000000000000');
        assert.equal(captured.ask, 123.25);
        cases.push({ venue, key, error, before, afterBad, afterValid: describe(store.tickers[0]),
          capturedBid: bits(captured.bid), capturedAsk: bits(captured.ask) });
      } finally {
        exchange.dispose();
      }
    }
  }
  assert.equal(cases.length, 12);
  process.stdout.write(JSON.stringify(cases));
})().catch(error => { console.error(error); process.exitCode = 1; });
