'use strict';
const assert = require('node:assert/strict');
const path = require('node:path');
const root = path.resolve(__dirname, '../../..');
const { DefaultStore } = require(path.join(root, 'packages/safe-cex/src/store/store.base.ts'));
const venue = process.argv[2];
assert(['bitunix', 'bitrue', 'apexomni', 'weex', 'phemex', 'kucoin', 'kucoinspot', 'hyperliquid', 'hyperliquidspot', 'lighter', 'blofin', 'cryptocom', 'coinbase', 'binance', 'binancepm', 'binancespot', 'polymarket'].includes(venue), `unsupported ticker corpus ${venue}`);
const name = { bitunix: 'BitunixExchange', bitrue: 'BitrueExchange', apexomni: 'ApexOmniExchange', weex: 'WeexExchange', phemex: 'PhemexExchange', kucoin: 'KucoinExchange', kucoinspot: 'KucoinSpotExchange', hyperliquid: 'HyperliquidExchange', hyperliquidspot: 'HyperliquidSpotExchange', lighter: 'LighterExchange', blofin: 'BlofinExchange', cryptocom: 'CryptocomExchange', coinbase: 'CoinbaseExchange', binance: 'BinanceExchange', binancepm: 'BinancePMExchange', binancespot: 'BinanceSpotExchange', polymarket: 'PolymarketExchange' }[venue];
const directory = { kucoinspot: 'kucoin', hyperliquidspot: 'hyperliquid', binancepm: 'binance', binancespot: 'binance' }[venue] || venue;
const Exchange = require(path.join(root, `packages/safe-cex/src/exchanges/${directory}/${venue}.exchange.ts`))[name];
const fields = ['bid', 'ask', 'last', 'mark', 'index', 'percentage', 'volume', 'quoteVolume', 'fundingRate', 'fundingTime', 'openInterest'];
if (venue.startsWith('hyperliquid')) fields.push('midPrice');
const store = new DefaultStore();
const symbols = ['BTC', 'ETH', 'ADA'];
const tickers = symbols.map(asset => ({
  id: { bitunix: `${asset}/USDT`, bitrue: `E-${asset}-USDT`, apexomni: `${asset}-USDC`, weex: `${asset}USDT`, phemex: `${asset}USDT`, kucoin: `${asset === 'BTC' ? 'XBT' : asset}USDTM`, kucoinspot: `${asset}-USDT`, hyperliquid: asset, hyperliquidspot: `@${symbols.indexOf(asset)}`, lighter: String(symbols.indexOf(asset) + 1), blofin: `${asset}-USDT`, cryptocom: `${asset}USD-PERP`, coinbase: `${asset}-USDC`, binance: `${asset}USDT`, binancepm: `${asset}USDT`, binancespot: `${asset}USDT`, polymarket: `${asset}-token` }[venue],
  symbol: { bitunix: `${asset}USDT`, bitrue: `E-${asset}-USDT`, apexomni: `${asset}USDC`, weex: `${asset}USDT`, phemex: `${asset}USDT`, kucoin: `${asset}USDT`, kucoinspot: `${asset}-USDT`, hyperliquid: `${asset}/USDC`, hyperliquidspot: `${asset}/USDC`, lighter: asset, blofin: `${asset}USDT`, cryptocom: `${asset}USD-PERP`, coinbase: `${asset}-USDC`, binance: `${asset}USDT`, binancepm: `${asset}USDT`, binancespot: `${asset}USDT`, polymarket: `${asset}-Yes` }[venue],
  ...Object.fromEntries(fields.map((field, index) => [field, 101 + index])),
}));
store.update({ tickers: structuredClone(tickers), markets: tickers.map(t => ({ id: t.id, symbol: t.symbol, ...(venue === 'cryptocom' ? { info: { underlyingSymbol: t.id.split('-')[0] } } : {}) })) });
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
const lighterTickerInputs = {
  last: ['lastPrice', 'last_price', 'lastTradePrice', 'last_trade_price', 'last', 'price'],
  mark: ['markPrice', 'mark_price', 'mark'], index: ['indexPrice', 'index_price', 'index'],
  bid: ['bidPrice', 'bid_price', 'bestBidPrice', 'best_bid_price', 'bestBid', 'best_bid', 'b', 'bid.0', 'b.0', 'b.price', 'bid.price', 'bid.p', 'bid.bestPrice', 'bid.best_price', 'b.p', 'b.bestPrice', 'b.best_price', 'bid'],
  ask: ['askPrice', 'ask_price', 'bestAskPrice', 'best_ask_price', 'bestAsk', 'best_ask', 'a', 'ask.0', 'a.0', 'a.price', 'ask.price', 'ask.p', 'ask.bestPrice', 'ask.best_price', 'a.p', 'a.bestPrice', 'a.best_price', 'ask'],
  volume: ['volume24h', 'volume_24h', 'volume'], quoteVolume: ['quoteVolume24h', 'quote_volume_24h', 'quoteVolume', 'quote_volume'],
  openInterest: ['openInterest', 'open_interest', 'oi', 'openInterestUsd', 'open_interest_usd'],
  fundingRate: ['fundingRate', 'funding_rate', 'currentFundingRate'], percentage: ['priceChange24h', 'price_change_24h'],
};
const lighterStatsInputs = {
  mark: ['mark_price', 'markPrice', 'mark'], index: ['index_price', 'indexPrice', 'index'],
  last: ['last_trade_price', 'lastTradePrice', 'last', 'price'], volume: ['daily_base_token_volume', 'volume'],
  quoteVolume: ['daily_quote_token_volume', 'quote_volume'], openInterest: ['open_interest', 'openInterest'],
  fundingRate: ['current_funding_rate', 'funding_rate'], percentage: ['daily_price_change'],
  bid: ['best_bid_price', 'bid_price', 'bid'], ask: ['best_ask_price', 'ask_price', 'ask'],
};
function putInput(data, key, value) {
  const [name, member] = key.split('.');
  if (member === undefined) data[name] = value;
  else data[name] = member === '0' ? [value, '2'] : { [member]: value };
}
const channels = venue === 'polymarket' ? { price_change: ['best_bid', 'best_ask'], last_trade_price: ['price'] }
  : venue.startsWith('binance') ? { '24hrTicker': ['c', 'P', 'v', 'q'], ...(venue === 'binancespot' ? {} : { markPriceUpdate: ['p', 'i', 'r', 'T'] }), bookTicker: ['b', 'a'] }
  : venue === 'blofin' ? { tickers: ['open24h', 'last', 'bidPrice', 'askPrice', 'volCurrency24h', 'vol24h'], 'funding-rate': ['fundingRate', 'fundingTime'] }
  : venue === 'cryptocom' ? { ticker: ['b', 'k', 'a', 'c', 'v', 'vv', 'oi'], mark: ['v'], index: ['v'], funding: ['v'], book: ['bid', 'ask'] }
  : venue === 'coinbase' ? { ticker_batch: ['price', 'price_percent_chg_24_h', 'volume_24_h', 'best_bid', 'best_ask', 'high_24_h', 'low_24_h'] }
  : venue === 'lighter' ? { ticker: Object.values(lighterTickerInputs).flat(), tickers: Object.values(lighterTickerInputs).flat(), 'market_stats:all': Object.values(lighterStatsInputs).flat() }
  : venue === 'hyperliquid' ? { allMids: ['mid'], fastAssetCtxs: ['markPx', 'midPx'] }
  : venue === 'hyperliquidspot' ? { allMids: ['mid'] }
  : venue === 'bitunix'
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
        if (venue === 'polymarket') {
          const data = { asset_id: ticker.id, best_bid: String(29.5 + offset), best_ask: String(33.75 + offset), price: String(31.5 + offset) };
          if (index === 0) data[key] = value;
          return channel === 'price_change' ? data : JSON.stringify({ event_type: channel, ...data });
        }
        if (venue.startsWith('binance')) {
          const data = { e: channel, s: ticker.symbol, c: String(31.5 + offset), P: String(.125 + index), v: String(53.25 + offset), q: String(1091.75 + offset), p: String(37.75 + offset), i: String(38.25 + offset), r: String(.0025 + index), T: String(1700000000 + offset), b: String(29.5 + offset), a: String(33.75 + offset) };
          if (index === 0) data[key] = value;
          return data;
        }
        if (venue === 'blofin') {
          const data = { instId: ticker.id, open24h: String(19.25 + offset), last: String(31.5 + offset), bidPrice: String(29.5 + offset), askPrice: String(33.75 + offset), volCurrency24h: String(53.25 + offset), vol24h: String(1091.75 + offset), fundingRate: String(.0025 + index), fundingTime: String(1700000000 + offset) };
          if (index === 0) data[key] = value;
          return JSON.stringify({ arg: { channel, instId: ticker.id }, data: [data] });
        }
        if (venue === 'cryptocom') {
          const data = channel === 'ticker' ? { b: String(29.5 + offset), k: String(33.75 + offset), a: String(31.5 + offset), c: String(.125 + index), v: String(53.25 + offset), vv: String(1091.75 + offset), oi: String(91.25 + offset) } : { v: String(37.75 + offset) };
          if (index === 0) data[key] = value;
          const row = channel === 'book' ? { bids: [[index === 0 && key === 'bid' ? value : String(29.5 + offset), '1']], asks: [[index === 0 && key === 'ask' ? value : String(33.75 + offset), '2']] } : data;
          return JSON.stringify({ method: 'subscribe', code: 0, result: { channel, instrument_name: channel === 'index' ? ticker.id.split('-')[0] : ticker.id, data: [row] } });
        }
        if (venue === 'coinbase') {
          const data = { product_id: ticker.symbol, price: String(31.5 + offset), price_percent_chg_24_h: String(.125 + index), volume_24_h: String(53.25 + offset), best_bid: String(29.5 + offset), best_ask: String(33.75 + offset), high_24_h: String(45.25 + offset), low_24_h: String(19.75 + offset) };
          if (index === 0) data[key] = value;
          return data;
        }
        if (venue === 'lighter') {
          const groups = channel === 'market_stats:all' ? lighterStatsInputs : lighterTickerInputs;
          const data = { symbol: ticker.symbol };
          for (const [field, aliases] of Object.entries(groups)) {
            const selected = aliases.includes(key) ? key : aliases[0];
            const good = { last: 31.5, mark: 37.75, index: 38.25, bid: 29.5, ask: 33.75, volume: 53.25, quoteVolume: 1091.75, openInterest: 91.25, fundingRate: .0025, percentage: .125 }[field] + offset;
            putInput(data, selected, index === 0 && aliases.includes(key) ? value : String(good));
          }
          if (channel === 'market_stats:all') { delete data.symbol; data.market_id = ticker.id; }
          return channel === 'ticker' ? JSON.stringify({ channel: `ticker/${ticker.id}`, data: { ...data, symbol: undefined } }) : data;
        }
        if (venue.startsWith('hyperliquid')) {
          const data = channel === 'allMids' ? { mid: String(31.5 + offset) } : { markPx: String(37.75 + offset), midPx: String(31.5 + offset) };
          if (index === 0) data[key] = value;
          return [ticker.id, channel === 'allMids' ? data.mid : data];
        }
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
      if (venue === 'polymarket' && channel === 'price_change') frames = [JSON.stringify({ event_type: channel, price_changes: [frames[1], frames[0], frames[2]] })];
      if (venue.startsWith('binance')) frames = [JSON.stringify([frames[1], frames[0], frames[2]])];
      if (venue === 'coinbase') frames = [JSON.stringify({ channel, events: [{ type: 'update', tickers: [frames[1], frames[0], frames[2]] }] })];
      if (venue === 'lighter' && channel === 'tickers') frames = [JSON.stringify({ channel, data: [frames[1], frames[0], frames[2]] })];
      if (venue === 'lighter' && channel === 'market_stats:all') frames = [JSON.stringify({ channel, market_stats: Object.fromEntries([1, 0, 2].map(i => [`market-${tickers[i].id}`, frames[i]])) })];
      if (venue.startsWith('hyperliquid')) {
        const data = Object.fromEntries([frames[1], frames[0], frames[2]]);
        frames = [JSON.stringify(channel === 'allMids' ? { channel, data: { mids: data } } : { channel, data: require('node:zlib').deflateRawSync(JSON.stringify(data)).toString('base64') })];
      }
      if (venue === 'phemex') frames = [JSON.stringify({ method: 'perp_market24h_pack_p.update', data: [frames[1], frames[0], frames[2]] })];
      if (venue === 'apexomni') frames = [JSON.stringify({ topic: 'instrumentInfo.all', data: [frames[1], frames[0], frames[2]] })];
      for (const frame of frames) {
        const receiver = venue === 'bitunix' && channel === 'price' ? exchange.publicWebsocketSecondary
          : venue.startsWith('binance') && venue !== 'binancespot' && channel !== 'bookTicker' ? socket.marketWebsocket : socket;
        if (['cryptocom', 'polymarket'].includes(venue)) {
          const fresh = new Exchange(venue, { publicDataOnly: true, applicationId: 'fixture' }, store);
          if (venue === 'polymarket') for (const ticker of tickers) fresh.tokenToSymbol.set(ticker.id, ticker.symbol);
          try { await fresh.publicWebsocket.onMessage({ data: frame }); } finally {
            const observedTickers = structuredClone(store.tickers);
            fresh.dispose();
            if (venue === 'polymarket') store.update({ tickers: observedTickers });
          }
        } else await receiver.onMessage({ data: frame });
      }
      assert.equal(store.tickers.length, 3, 'constructed ticker corpus retains all rows');
      cases.push({ channel, field: key, label, frames, expected: store.tickers.map(ticker => ({
        id: ticker.id, symbol: ticker.symbol,
        fields: Object.fromEntries(fields.map(field => [field, bits(ticker[field])])),
      })) });
    }
  } finally { exchange.dispose(); }
  process.stdout.write(JSON.stringify({ venue, fields, cases }), () => process.exit(0));
})().catch(error => { console.error(error); process.exitCode = 1; });
