'use strict';
const assert = require('node:assert/strict');
const path = require('node:path');
const root = path.resolve(__dirname, '../../..');
const { DefaultStore } = require(path.join(root, 'packages/safe-cex/src/store/store.base.ts'));
const venue = process.argv[2];
assert(['bitunix', 'bitrue', 'apexomni', 'weex', 'phemex', 'kucoin', 'kucoinspot'].includes(venue), `unsupported ticker corpus ${venue}`);
const name = { bitunix: 'BitunixExchange', bitrue: 'BitrueExchange', apexomni: 'ApexOmniExchange', weex: 'WeexExchange', phemex: 'PhemexExchange', kucoin: 'KucoinExchange', kucoinspot: 'KucoinSpotExchange' }[venue];
const directory = venue === 'kucoinspot' ? 'kucoin' : venue;
const Exchange = require(path.join(root, `packages/safe-cex/src/exchanges/${directory}/${venue}.exchange.ts`))[name];
const fields = ['bid', 'ask', 'last', 'mark', 'index', 'percentage', 'volume', 'quoteVolume', 'fundingRate', 'fundingTime', 'openInterest'];
const store = new DefaultStore();
const symbols = ['BTC', 'ETH', 'ADA'];
const tickers = symbols.map(asset => ({
  id: { bitunix: `${asset}/USDT`, bitrue: `E-${asset}-USDT`, apexomni: `${asset}-USDC`, weex: `${asset}USDT`, phemex: `${asset}USDT`, kucoin: `${asset === 'BTC' ? 'XBT' : asset}USDTM`, kucoinspot: `${asset}-USDT` }[venue],
  symbol: { bitunix: `${asset}USDT`, bitrue: `E-${asset}-USDT`, apexomni: `${asset}USDC`, weex: `${asset}USDT`, phemex: `${asset}USDT`, kucoin: `${asset}USDT`, kucoinspot: `${asset}-USDT` }[venue],
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
  : venue === 'bitrue' ? { ticker: ['close', 'rose', 'vol', 'amount'] }
    : venue === 'apexomni' ? { instrumentInfo: ['p', 'mp', 'xp', 'pr', 'o', 'fr', 'v', 'to'] }
      : venue === 'weex' ? { ticker: ['c', 'm', 'i', 'P', 'v', 'q'], 'ticker-array': ['c', 'm', 'i', 'P', 'v', 'q'] }
        : venue === 'phemex' ? { packed: ['openRp', 'lastRp', 'volumeRq', 'openInterestRv', 'indexRp', 'markRp', 'fundingRateRr', 'bid', 'ask'] }
          : venue === 'kucoin' ? { tickerV2: ['bestBidPrice', 'bestAskPrice'], snapshot: ['lastPrice', 'priceChgPct', 'fundingRate', 'volume', 'turnover'], instrument: ['markPrice', 'indexPrice'] }
            : { ticker: ['bestBid', 'bestBidPrice', 'buy', 'bestAsk', 'bestAskPrice', 'sell', 'price', 'last', 'lastPrice', 'changeRate', 'volume', 'vol', 'volume24h', 'turnover', 'volValue', 'turnover24h'], 'ticker-all': ['bestBid', 'bestBidPrice', 'buy', 'bestAsk', 'bestAskPrice', 'sell', 'price', 'last', 'lastPrice', 'changeRate', 'volume', 'vol', 'volume24h', 'turnover', 'volValue', 'turnover24h'] };
(async () => {
  const cases = [];
  try {
    for (const [channel, keys] of Object.entries(channels)) for (const key of keys) for (const [label, value] of samples) {
      store.update({ tickers: structuredClone(tickers) });
      let frames = tickers.map((ticker, index) => {
        const offset = index * 11;
        if (venue === 'phemex') {
          const data = { openRp: String(19.25 + offset), lastRp: String(31.5 + offset), volumeRq: String(53.25 + offset),
            openInterestRv: String(91.25 + offset), indexRp: String(38.25 + offset), markRp: String(37.75 + offset), fundingRateRr: String(.0025 + index), bid: String(29.5 + offset), ask: String(33.75 + offset) };
          if (index === 0) data[key] = value;
          return [ticker.symbol, data.openRp, 'unused', 'unused', data.lastRp, data.volumeRq, 'unused', data.openInterestRv, data.indexRp, data.markRp, data.fundingRateRr, 'unused', data.bid, data.ask];
        }
        if (venue === 'kucoin') {
          const data = { symbol: ticker.id, bestBidPrice: String(29.5 + offset), bestAskPrice: String(33.75 + offset), lastPrice: String(31.5 + offset), priceChgPct: String(.125 + index), fundingRate: String(.0025 + index), volume: String(53.25 + offset), turnover: String(1091.75 + offset), markPrice: String(37.75 + offset), indexPrice: String(38.25 + offset) };
          if (index === 0) data[key] = value;
          if (channel === 'instrument') delete data.symbol;
          const prefix = channel === 'instrument' ? '/contract/instrument' : `/contractMarket/${channel}`;
          return JSON.stringify({ type: 'message', topic: `${prefix}:${ticker.id}`, data });
        }
        if (venue === 'kucoinspot') {
          const data = { bestBid: String(29.5 + offset), bestAsk: String(33.75 + offset), price: String(31.5 + offset), changeRate: String(.125 + index), volume: String(53.25 + offset), turnover: String(1091.75 + offset) };
          const chain = [['bestBid', 'bestBidPrice', 'buy'], ['bestAsk', 'bestAskPrice', 'sell'], ['price', 'last', 'lastPrice'], ['volume', 'vol', 'volume24h'], ['turnover', 'volValue', 'turnover24h']].find(keys => keys.includes(key));
          if (chain) {
            data[key] = data[chain[0]];
            for (const earlier of chain.slice(0, chain.indexOf(key))) {
              if (channel === 'ticker-all') data[earlier] = null;
              else delete data[earlier];
            }
          }
          if (index === 0) data[key] = value;
          return JSON.stringify({ type: 'message', topic: `/market/ticker:${channel === 'ticker-all' ? 'all' : ticker.symbol}`, ...(channel === 'ticker-all' ? { subject: ticker.symbol } : {}), data });
        }
        if (venue === 'apexomni') {
          const row = { s: ticker.symbol, p: String(31.5 + offset), mp: String(37.75 + offset), xp: String(38.25 + offset),
            pr: String(.125 + index), o: String(91.25 + offset), fr: String(.0025 + index), v: String(53.25 + offset), to: String(1091.75 + offset) };
          if (index === 0) row[key] = value;
          return row;
        }
        if (venue === 'weex') {
          const row = { c: String(31.5 + offset), m: String(37.75 + offset), i: String(38.25 + offset),
            P: String(.125 + index), v: String(53.25 + offset), q: String(1091.75 + offset) };
          if (index === 0) row[key] = value;
          return JSON.stringify({ e: 'ticker', s: ticker.symbol, d: channel === 'ticker-array' ? [row] : row });
        }
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
      if (venue === 'phemex') frames = [JSON.stringify({ method: 'perp_market24h_pack_p.update', data: [frames[1], frames[0], frames[2]] })];
      if (venue === 'apexomni') frames = [JSON.stringify({ topic: 'instrumentInfo.all', data: [frames[1], frames[0], frames[2]] })];
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
