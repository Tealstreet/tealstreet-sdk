const assert = require('node:assert/strict');
const path = require('node:path');
const root = path.resolve(__dirname, '../../..');
const { DefaultStore } = require(path.join(root, 'packages/safe-cex/src/store/store.base.ts'));
const [venue, profile] = process.argv.slice(2);
assert(['phemex', 'woo'].includes(venue));
assert(['rest', 'stream-new'].includes(profile));
const store = new DefaultStore();
const market = {id: 'BTC/USDT:USDT', symbol: 'BTCUSDT'};
store.update({markets: [market]});
const exported = venue === 'woo' ? 'WOOXExchange' : 'PhemexExchange';
const Exchange = require(path.join(root, `packages/safe-cex/src/exchanges/${venue}/${venue}.exchange.ts`))[exported];
const exchange = new Exchange(venue, {key: '', secret: '', publicDataOnly: true}, store);
if (profile === 'rest') {
  const body = venue === 'woo' ? [{
    symbol: 'PERP_BTC_USDT', '24h_open': 100, '24h_close': 101.2345,
    index_price: 101.1, mark_price: 101.2, last_funding_rate: 0.0001,
    next_funding_time: 1700000000000, '24h_volume': 12, '24h_amount': 1200,
    open_interest: 7,
  }] : {error: null, result: [{
    symbol: 'BTCUSDT', bidRp: '99.5', askRp: '100.5', lastRp: '100',
    markRp: '101', indexRp: '99', openRp: '80', volumeRq: '7',
    openInterestRv: '800', fundingRateRr: '0.001',
  }]};
  const rows = exchange.parseTickers(body);
  assert.equal(rows.length, 1);
  store.update({tickers: rows});
} else {
  assert.equal(venue, 'phemex');
  exchange.publicWebsocket.handleTickerStreamEvents([{data: [[
    'BTCUSDT', '80', '120', '70', '100', '7', '0', '800', '99', '101',
    '0.001', '0', '99.5', '100.5',
  ]]}]);
}
assert.equal(store.tickers.length, 1);
const ticker = store.tickers[0];
const keys = Object.keys(ticker);
assert.deepEqual(keys, venue === 'woo'
  ? ['id', 'symbol', 'bid', 'ask', 'last', 'index', 'mark', 'percentage',
    'fundingRate', 'fundingTime', 'volume', 'quoteVolume', 'openInterest']
  : ['id', 'symbol', 'bid', 'ask', 'last', 'mark', 'index', 'percentage',
    'openInterest', 'fundingRate', 'fundingTime', 'volume', 'quoteVolume']);
assert.equal(typeof ticker.last, 'number');
assert.equal(typeof ticker.volume, 'number');
const bits = value => {
  const buffer = Buffer.alloc(8);
  buffer.writeDoubleBE(value);
  return buffer.toString('hex');
};
process.stdout.write(JSON.stringify({keys, id: ticker.id, symbol: ticker.symbol,
  lastBits: bits(ticker.last), volumeBits: bits(ticker.volume)}));
exchange.dispose();
