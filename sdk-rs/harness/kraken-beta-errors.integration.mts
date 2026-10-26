import test from 'node:test';
import assert from 'node:assert/strict';
import { spotRig, spotResponse } from './spot-host-rig.mjs';
import { krakenSpotPairs, krakenSpotTickers } from './private-tape-krakenspot.mjs';

const credentials = { key: 'replay-key', secret: Buffer.from('replay-secret').toString('base64') };
const order = (spot) => ({ symbol: spot ? 'XBT/USD' : 'PF_XBTUSD', type: 'limit', side: 'buy', amount: 1, price: 99 });
function rig(spot, writeReply) {
  return spotRig(spot ? 'krakenspot' : 'kraken', credentials, (request) => {
    const path = new URL(request.url).pathname;
    if (spot) {
      if (path.endsWith('/AssetPairs')) return spotResponse(krakenSpotPairs);
      if (path.endsWith('/Ticker')) return spotResponse(krakenSpotTickers);
      if (path.endsWith('/Balance')) return spotResponse({ error: [], result: { ZUSD: '1000' } });
      if (path.endsWith('/OpenOrders')) return spotResponse({ error: [], result: { open: {} } });
      if (path.endsWith('/GetWebSocketsToken')) return spotResponse({ error: [], result: { token: 'fixture-token', expires: 900 } });
    } else {
      if (path.endsWith('/instruments')) return spotResponse({ result: 'success', instruments: [{ symbol: 'PF_XBTUSD', base: 'BTC', quote: 'USD', tradeable: true, tickSize: 0.5, contractSize: 1, contractValueTradePrecision: 3, marginLevels: [{ initialMargin: 0.02 }] }] });
      if (path.endsWith('/tickers')) return spotResponse({ result: 'success', tickers: [{ symbol: 'PF_XBTUSD', last: 100, markPrice: 100, indexPrice: 100, bid: 99, ask: 101 }] });
      if (path.endsWith('/openorders')) return spotResponse({ result: 'success', openOrders: [] });
      if (path.endsWith('/openpositions')) return spotResponse({ result: 'success', openPositions: [] });
      if (path.endsWith('/leveragepreferences')) return spotResponse({ result: 'success', leveragePreferences: [] });
      if (path.endsWith('/accounts')) return spotResponse({ result: 'success', accounts: {} });
    }
    return writeReply(request);
  });
}
for (const spot of [false, true]) {
  test(`JS EngineHost/compiled WASM ${spot ? 'Kraken Spot' : 'Kraken'} retains a partial write`, async () => {
    let sends = 0;
    const r = rig(spot, (request) => {
      assert(new URL(request.url).pathname.endsWith(spot ? '/AddOrder' : '/sendorder'));
      sends++;
      return spot ? spotResponse(sends === 1
        ? { error: [], result: { txid: ['accepted'] } }
        : { error: ['EOrder:Insufficient funds'] })
        : spotResponse(sends === 1
          ? { result: 'success', sendStatus: { status: 'placed', order_id: 'accepted' } }
          : { result: 'error', error: 'insufficientFunds' });
    });
    try {
      await r.start();
      const reads = r.requests.filter((request) => request.url.endsWith(spot ? '/OpenOrders' : '/openorders')).length;
      await assert.rejects(r.command({ op: 'placeOrders', orders: [order(spot), order(spot), order(spot)] }), (error) => {
        assert.equal(error.outcome, 'unknown');
        assert.deepEqual(error.partialOrderIds, ['accepted']);
        return true;
      });
      assert.equal(sends, 2);
      assert(r.requests.filter((request) => request.url.endsWith(spot ? '/OpenOrders' : '/openorders')).length > reads);
    } finally { r.dispose(); }
  });
}
for (const [status, kind] of [[401, 'authInvalid'], [403, 'authPermission']]) {
  test(`JS EngineHost/compiled WASM Spot ${status} retains the venue`, async () => {
    let sends = 0;
    const r = rig(true, (request) => {
      assert(new URL(request.url).pathname.endsWith('/AddOrder'));
      return ++sends === 1 ? spotResponse({}, status) : spotResponse({ error: [], result: { txid: ['after-auth'] } });
    });
    try {
      await r.start();
      await assert.rejects(r.command({ op: 'placeOrders', orders: [order(true)] }), (error) => {
        assert.equal(error.kind, kind);
        return true;
      });
      assert.deepEqual(await r.command({ op: 'placeOrders', orders: [order(true)] }), ['after-auth']);
      assert.equal(sends, 2);
    } finally { r.dispose(); }
  });
}
test('JS EngineHost/compiled WASM Futures uses the native code on HTTP 400', async () => {
  const r = rig(false, () => spotResponse({ result: 'error', error: 'apiLimitExceeded' }, 400));
  try {
    await r.start();
    await assert.rejects(r.command({ op: 'placeOrders', orders: [order(false)] }), (error) => {
      assert.equal(error.kind, 'rateLimited');
      assert.equal(error.outcome, 'rejected');
      assert.equal(error.venueCode, 'apiLimitExceeded');
      return true;
    });
  } finally { r.dispose(); }
});
