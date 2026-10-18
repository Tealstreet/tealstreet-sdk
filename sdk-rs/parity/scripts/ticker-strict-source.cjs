'use strict';
const assert = require('node:assert/strict');
const path = require('node:path');
const root = path.resolve(__dirname, '../../..');
const venue = process.argv[2];
assert(['asterdex', 'orderly', 'woo'].includes(venue));
const { DefaultStore } = require(path.join(root, 'packages/safe-cex/src/store/store.base.ts'));
const Exchange = require(path.join(root, `packages/safe-cex/src/exchanges/${venue}/${venue}.exchange.ts`))[
  { asterdex: 'AsterDEXExchange', orderly: 'OrderlyExchange', woo: 'WOOXExchange' }[venue]
];
const fields = ['bid', 'ask', 'last', 'mark', 'index', 'percentage', 'volume', 'quoteVolume', 'fundingRate', 'fundingTime', 'openInterest'];
const tickers = ['BTC', 'ETH', 'ADA'].map(asset => ({
  id: venue === 'orderly' ? `PERP_${asset}_USDC` : `${asset}USDT`,
  symbol: `${asset}${venue === 'orderly' ? 'USDC' : 'USDT'}`,
  ...Object.fromEntries(fields.map((field, i) => [field, 101 + i])),
}));
const store = new DefaultStore();
store.update({ tickers: structuredClone(tickers) });
const exchange = new Exchange(venue === 'orderly' ? 'woofi' : venue, { publicDataOnly: true, applicationId: 'fixture' }, store);
const samples = [
  ['null', null], ['numeric-string', '12.5'], ['prefix-string', '12.5tail'],
  ['nan-string', 'NaN'], ['malformed-object', { bad: true }], ['overflow-string', '1e400'],
  ['signed-zero-string', '-0'], ['false', false], ['number', 12.5],
  ['number-overflow', '__RAW_INFINITY__'], ['number-negative-zero', '__RAW_NEGATIVE_ZERO__'], ['missing', undefined],
];
const channels = venue === 'asterdex' ? {
  'ticker-wrapped': ['c', 'P', 'v', 'q'], 'ticker-bare': ['c', 'P', 'v', 'q'],
  'mark-wrapped': ['p', 'i', 'r', 'T'], 'mark-bare': ['p', 'i', 'r', 'T'],
  trade: ['p'], depth: ['b.price', 'b.amount', 'a.price', 'a.amount'],
} : {
  tickers: ['close', 'amount', 'volume'], bbos: ['bid', 'ask'], markprices: ['price'],
  ...(venue === 'orderly' ? { estfundingrate: ['fundingRate', 'funding_rate', 'fundingTs', 'funding_ts'] } : {}),
};
function wire(value) {
  return JSON.stringify(value).replaceAll('"__RAW_INFINITY__"', '1e400').replaceAll('"__RAW_NEGATIVE_ZERO__"', '-0.0');
}
function bits(value) {
  assert.equal(typeof value, 'number');
  if (Number.isNaN(value)) return '7ff8000000000000';
  const buffer = Buffer.alloc(8); buffer.writeDoubleBE(value); return buffer.toString('hex');
}
const cases = [];
(async () => {
  try {
    for (const [channel, keys] of Object.entries(channels)) for (const field of keys) for (const [label, value] of samples) {
      store.update({ tickers: structuredClone(tickers) });
      const rows = tickers.map((ticker, index) => {
        const offset = index * 11;
        if (venue === 'asterdex') {
          const data = channel.startsWith('ticker') ? { e: '24hrTicker', s: ticker.symbol, c: String(31.5 + offset), P: String(.125 + index), v: String(53.25 + offset), q: String(1091.75 + offset) }
            : channel.startsWith('mark') ? { e: 'markPriceUpdate', s: ticker.symbol, p: String(37.75 + offset), i: String(38.25 + offset), r: String(.0025 + index), T: 1700000000 + offset }
            : channel === 'trade' ? { e: 'aggTrade', s: ticker.symbol, p: String(31.5 + offset), q: '2', T: 1700000000, m: false }
            : { s: ticker.symbol, b: [[String(29.5 + offset), '2']], a: [[String(33.75 + offset), '3']] };
          if (index === 0) {
            if (channel === 'depth') {
              const [side, kind] = field.split('.'); data[side][0][kind === 'price' ? 0 : 1] = value;
            } else data[field] = value;
          }
          return data;
        }
        const data = { symbol: `PERP_${['BTC', 'ETH', 'ADA'][index]}_${venue === 'orderly' ? 'USDC' : 'USDT'}`, close: 31.5 + offset, amount: 1091.75 + offset, volume: 53.25 + offset, bid: 29.5 + offset, ask: 33.75 + offset, price: 37.75 + offset, fundingRate: String(.0025 + index), funding_rate: String(.725 + index), fundingTs: String(1700000000 + offset), funding_ts: String(1800000000 + offset) };
        const chain = [['fundingRate', 'funding_rate'], ['fundingTs', 'funding_ts']].find(keys => keys.includes(field));
        if (chain && chain[1] === field) delete data[chain[0]];
        if (index === 0) data[field] = value;
        return data;
      });
      let frames;
      if (venue !== 'asterdex') frames = [wire({ topic: channel, data: [rows[1], rows[0], rows[2]] })];
      else if (channel.endsWith('wrapped')) frames = [wire({ stream: channel.startsWith('mark') ? '!markPrice@arr@1s' : '!ticker@arr', data: [rows[1], rows[0], rows[2]] })];
      else if (channel.endsWith('bare')) frames = [wire([rows[1], rows[0], rows[2]])];
      else frames = [1, 0, 2].map(i => channel === 'depth' ? wire({ stream: `${tickers[i].symbol.toLowerCase()}@depth20@100ms`, data: rows[i] }) : wire(rows[i]));
      for (const frame of frames) await exchange.publicWebsocket.onMessage({ data: frame });
      assert.equal(store.tickers.length, 3);
      const refusals = [];
      const expected = store.tickers.map(ticker => {
        const invalid = fields.filter(field => typeof ticker[field] !== 'number');
        if (invalid.length) {
          assert.equal(ticker.symbol, tickers[0].symbol, 'only the constructed bad row may require decision-3 refusal');
          assert.equal(invalid.length, 1);
          const field = invalid[0];
          refusals.push({ symbol: ticker.symbol, field, value: ticker[field] === undefined ? 'undefined' : JSON.stringify(ticker[field]) });
        }
        const admitted = invalid.length ? tickers.find(row => row.id === ticker.id) : ticker;
        return { id: ticker.id, symbol: ticker.symbol, fields: Object.fromEntries(fields.map(field => [field, bits(admitted[field])])) };
      });
      for (const index of [1, 2]) assert(fields.some(field => bits(store.tickers[index][field]) !== bits(tickers[index][field])), `good sibling must update: ${channel}/${field}/${label}`);
      cases.push({ channel, field, label, frames, refusals, expected });
    }
  } finally { exchange.dispose(); }
  assert.equal(cases.length, { asterdex: 252, orderly: 120, woo: 72 }[venue]);
  assert.equal(cases.reduce((count, row) => count + row.refusals.length, 0), venue === 'asterdex' ? 12 : 54);
  process.stdout.write(JSON.stringify({ venue, fields, cases }), () => process.exit(0));
})().catch(error => { console.error(error); process.exitCode = 1; });
