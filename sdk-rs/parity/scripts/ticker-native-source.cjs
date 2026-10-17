'use strict';
const assert = require('node:assert/strict');
const path = require('node:path');
const root = path.resolve(__dirname, '../../..');
const { DefaultStore } = require(path.join(root, 'packages/safe-cex/src/store/store.base.ts'));
const venue = process.argv[2];
assert(['bitunix', 'bitrue'].includes(venue), `unsupported ticker corpus ${venue}`);
const name = venue === 'bitunix' ? 'BitunixExchange' : 'BitrueExchange';
const Exchange = require(path.join(root, `packages/safe-cex/src/exchanges/${venue}/${venue}.exchange.ts`))[name];
const fields = ['bid', 'ask', 'last', 'mark', 'index', 'percentage', 'volume', 'quoteVolume', 'fundingRate', 'fundingTime', 'openInterest'];
const store = new DefaultStore();
const symbols = ['BTC', 'ETH', 'ADA'];
const tickers = symbols.map(asset => ({
  id: venue === 'bitunix' ? `${asset}/USDT` : `E-${asset}-USDT`,
  symbol: venue === 'bitunix' ? `${asset}USDT` : `E-${asset}-USDT`,
  ...Object.fromEntries(fields.map((field, index) => [field, 101 + index])),
}));
store.update({ tickers: structuredClone(tickers), markets: tickers.map(t => ({ id: t.id, symbol: t.symbol })) });
const exchange = new Exchange(venue, { publicDataOnly: true, applicationId: 'fixture' }, store);
const socket = exchange.publicWebsocket;
if (venue === 'bitrue') for (const row of tickers) socket.ensureTickerSubscription(row.symbol);
const samples = [
  ['null', null], ['numeric-string', '12.5'], ['prefix-string', '12.5tail'],
  ['nan-string', 'NaN'], ['malformed-object', { bad: true }],
  ['overflow', '1e400'], ['signed-zero', '-0'], ['false', false],
];
function bits(number) {
  assert.equal(typeof number, 'number');
  if (Number.isNaN(number)) return '7ff8000000000000';
  const buffer = Buffer.alloc(8); buffer.writeDoubleBE(number); return buffer.toString('hex');
}
const channels = venue === 'bitunix'
  ? { ticker: ['la', 'o', 'b', 'q'], price: ['mp', 'ip', 'fr', 'ft'], depth_book1: ['b', 'a'] }
  : { ticker: ['close', 'rose', 'vol', 'amount'] };
(async () => {
  const cases = [];
  try {
    for (const [channel, keys] of Object.entries(channels)) for (const key of keys) for (const [label, value] of samples) {
      store.update({ tickers: structuredClone(tickers) });
      const frames = tickers.map((ticker, index) => {
        const offset = index * 11;
        if (venue === 'bitrue') {
          const tick = { close: String(31.5 + offset), rose: String(.125 + index), vol: String(53.25 + offset), amount: String(1091.75 + offset) };
          if (index === 0) tick[key] = value;
          return JSON.stringify({ channel: `market_${socket.convertToWsSymbol(ticker.symbol)}_ticker`, tick });
        }
        let data = { la: String(31.5 + offset), o: String(19.25 + offset), b: String(53.25 + offset), q: String(1091.75 + offset),
          mp: String(37.75 + offset), ip: String(38.25 + offset), fr: String(.125 + index), ft: String(1700000000 + offset) };
        if (channel === 'depth_book1') {
          data = { b: [[String(29.5 + offset), '1']], a: [[String(33.75 + offset), '2']] };
          if (index === 0) data[key][0][0] = value;
        } else if (index === 0) data[key] = value;
        return JSON.stringify({ ch: channel, symbol: ticker.symbol, data });
      });
      for (const frame of frames) {
        const receiver = venue === 'bitunix' && channel === 'price' ? exchange.publicWebsocketSecondary : socket;
        await receiver.onMessage({ data: frame });
      }
      cases.push({ channel, field: key, label, frames, expected: store.tickers.map(ticker => ({
        id: ticker.id, symbol: ticker.symbol,
        fields: Object.fromEntries(fields.map(field => [field, bits(ticker[field])])),
      })) });
    }
  } finally { exchange.dispose(); }
  process.stdout.write(JSON.stringify({ venue, fields, cases }));
})().catch(error => { console.error(error); process.exitCode = 1; });
