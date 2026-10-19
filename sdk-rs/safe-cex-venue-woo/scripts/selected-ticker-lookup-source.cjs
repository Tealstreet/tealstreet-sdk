const assert = require('node:assert/strict');
const root = require('node:path').resolve(__dirname, '../../..');
const { DefaultStore } = require(root + '/packages/safe-cex/src/store/store.base.ts');
const { WOOXExchange } = require(root + '/packages/safe-cex/src/exchanges/woo/woo.exchange.ts');
const scenes = [
  ['tickers', 'last', { close: 117.5, amount: 9001.5, volume: 21.25 }],
  ['bbos', 'bid', { bid: 116.25, ask: 118.75 }],
  ['markprices', 'mark', { price: 119.125 }],
];
const observed = [];
for (const [topic, field, fields] of scenes) {
  const store = new DefaultStore();
  store.update({ tickers: [{ id: 'BTCUSDT', symbol: 'BTCUSDT', bid: 1, ask: 2, last: 3,
    mark: 4, index: 5, percentage: 6, openInterest: 7, fundingRate: 8,
    fundingTime: 9, volume: 10, quoteVolume: 20 }] });
  const exchange = new WOOXExchange('woo', { publicDataOnly: true, applicationId: 'fixture-app' }, store);
  const selected = store.tickersBySymbol.BTCUSDT;
  let unrelatedReads = 0;
  const original = { label: 'unrelated lookup must stay cold' };
  Object.defineProperty(store.tickersBySymbol, 'UNRELATED', {
    enumerable: true, configurable: true,
    get() { unrelatedReads++; throw original; },
  });
  try {
    exchange.publicWebsocket.onMessage({ data: JSON.stringify({ topic, data: [{ symbol: 'PERP_BTC_USDT', ...fields }] }) });
    assert.equal(unrelatedReads, 0, topic);
    assert.strictEqual(store.tickersBySymbol.BTCUSDT, selected, topic + ' row alias');
    const expected = { tickers: 117.5, bbos: 116.25, markprices: 119.125 }[topic];
    assert(Object.is(selected[field], expected), topic + ' selected update');
    observed.push({ topic, unrelatedReads, selectedIdentity: true, value: selected[field] });
  } finally { exchange.dispose(); }
}
assert.equal(observed.length, 3);
process.stdout.write(JSON.stringify({ observations: observed }), () => process.exit(0));
