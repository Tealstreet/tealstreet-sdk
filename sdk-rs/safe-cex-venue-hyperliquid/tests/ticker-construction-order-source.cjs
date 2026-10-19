const assert = require('node:assert/strict');
const path = require('node:path');
const root = path.resolve(__dirname, '../../..');
const { DefaultStore } = require(path.join(root, 'packages/safe-cex/src/store/store.base.ts'));
const { HyperliquidExchange } = require(path.join(root, 'packages/safe-cex/src/exchanges/hyperliquid/hyperliquid.exchange.ts'));
const store = new DefaultStore();
const exchange = new HyperliquidExchange('hyperliquid', { publicDataOnly: true }, store);
const meta = { universe: [
  { name: 'BTC', szDecimals: 5, maxLeverage: 40 },
  { name: 'ETH', szDecimals: 4, maxLeverage: 25 },
] };
const assets = ['61000.0', '3000.0'].map(mark => ({
  funding: '0.0000125', openInterest: '100.5', prevDayPx: '60000.0',
  dayNtlVlm: '1000000.0', premium: '0.0001', oraclePx: mark,
  markPx: mark, midPx: mark, impactPxs: [mark, mark], dayBaseVlm: '16.4',
}));
const mapped = exchange.fetchMarketsHelper(meta, assets, { makerFee: 0.0001, takerFee: 0.00035 });
assert.equal(mapped.tickers.length, 2);
const expected = [
  'id', 'symbol', 'bid', 'ask', 'last', 'mark', 'index', 'midPrice', 'percentage',
  'fundingRate', 'fundingTime', 'volume', 'quoteVolume', 'openInterest', 'info',
];
for (let i = 0; i < 2; ++i) {
  assert.deepEqual(Object.keys(mapped.tickers[i]), expected);
  assert.equal(mapped.tickers[i].info, assets[i]);
}
store.update({ tickers: mapped.tickers });
for (const ticker of store.tickers) assert.deepEqual(Object.keys(ticker), expected.filter(k => k !== 'info'));
process.stdout.write(JSON.stringify({root: Object.keys(store.serialize()), tickers: store.tickers.map(Object.keys)}));
