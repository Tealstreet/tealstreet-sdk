'use strict';
const assert = require('node:assert/strict');
const path = require('node:path');
const root = path.resolve(__dirname, '../../..');
const { DefaultStore } = require(path.join(root, 'packages/safe-cex/src/store/store.base.ts'));
const venue = process.argv[2];
assert(['bitunix', 'bitrue', 'apexomni', 'weex', 'phemex', 'kucoin', 'kucoinspot', 'hyperliquid', 'hyperliquidspot', 'lighter', 'blofin', 'cryptocom', 'coinbase', 'binance', 'binancepm', 'binancespot', 'bybit', 'bybitdemo', 'bybitinverse', 'bybitspot', 'hyrotrader', 'polymarket', 'backpack', 'paradex', 'kraken', 'krakenspot', 'mexc', 'okx', 'bingx', 'bitget', 'grvt'].includes(venue), `unsupported ticker corpus ${venue}`);
const name = { bitunix: 'BitunixExchange', bitrue: 'BitrueExchange', apexomni: 'ApexOmniExchange', weex: 'WeexExchange', phemex: 'PhemexExchange', kucoin: 'KucoinExchange', kucoinspot: 'KucoinSpotExchange', hyperliquid: 'HyperliquidExchange', hyperliquidspot: 'HyperliquidSpotExchange', lighter: 'LighterExchange', blofin: 'BlofinExchange', cryptocom: 'CryptocomExchange', coinbase: 'CoinbaseExchange', binance: 'BinanceExchange', binancepm: 'BinancePMExchange', binancespot: 'BinanceSpotExchange', bybit: 'BybitExchange', bybitdemo: 'BybitExchange', bybitinverse: 'BybitInverseExchange', bybitspot: 'BybitSpotExchange', hyrotrader: 'HyrotraderExchange', polymarket: 'PolymarketExchange', backpack: 'BackpackExchange', paradex: 'ParadexExchange', kraken: 'KrakenExchange', krakenspot: 'KrakenSpotExchange', mexc: 'MexcExchange', okx: 'OKXExchange', bingx: 'BingxExchange', bitget: 'BitgetExchange', grvt: 'GrvtExchange' }[venue];
const directory = { kucoinspot: 'kucoin', hyperliquidspot: 'hyperliquid', krakenspot: 'kraken', binancepm: 'binance', binancespot: 'binance', bybitdemo: 'bybit', bybitinverse: 'bybit', bybitspot: 'bybit', hyrotrader: 'bybit' }[venue] || venue;
if (venue === 'mexc') process.env.TZ = 'UTC';
const Exchange = require(path.join(root, `packages/safe-cex/src/exchanges/${directory}/${venue === 'bybitdemo' ? 'bybit' : venue}.exchange.ts`))[name];
const fields = ['bid', 'ask', 'last', 'mark', 'index', 'percentage', 'volume', 'quoteVolume', 'fundingRate', 'fundingTime', 'openInterest'];
if (venue.startsWith('hyperliquid')) fields.push('midPrice');
const store = new DefaultStore();
const symbols = ['BTC', 'ETH', 'ADA'];
const tickers = symbols.map(asset => ({
  id: { bitunix: `${asset}/USDT`, bitrue: `E-${asset}-USDT`, apexomni: `${asset}-USDC`, weex: `${asset}USDT`, phemex: `${asset}USDT`, kucoin: `${asset === 'BTC' ? 'XBT' : asset}USDTM`, kucoinspot: `${asset}-USDT`, hyperliquid: asset, hyperliquidspot: `@${symbols.indexOf(asset)}`, lighter: String(symbols.indexOf(asset) + 1), blofin: `${asset}-USDT`, cryptocom: `${asset}USD-PERP`, coinbase: `${asset}-USDC`, binance: `${asset}USDT`, binancepm: `${asset}USDT`, binancespot: `${asset}USDT`, bybit: `${asset}USDT`, bybitdemo: `${asset}USDT`, bybitinverse: `${asset}USD`, bybitspot: `${asset}USDT`, hyrotrader: `${asset}USDT`, polymarket: `${asset}-token`, backpack: `${asset}_USDC_PERP`, paradex: `${asset}-USD-PERP`, kraken: `${asset}/USD`, krakenspot: `${asset}-spot-pair`, mexc: `${asset}_USDT`, okx: `${asset}-USDT-SWAP`, bingx: `${asset}-USDT`, bitget: `${asset}-bitget-market`, grvt: `${asset}_USDT_Perp` }[venue],
  symbol: { bitunix: `${asset}USDT`, bitrue: `E-${asset}-USDT`, apexomni: `${asset}USDC`, weex: `${asset}USDT`, phemex: `${asset}USDT`, kucoin: `${asset}USDT`, kucoinspot: `${asset}-USDT`, hyperliquid: `${asset}/USDC`, hyperliquidspot: `${asset}/USDC`, lighter: asset, blofin: `${asset}USDT`, cryptocom: `${asset}USD-PERP`, coinbase: `${asset}-USDC`, binance: `${asset}USDT`, binancepm: `${asset}USDT`, binancespot: `${asset}USDT`, bybit: `${asset}USDT`, bybitdemo: `${asset}USDT`, bybitinverse: `${asset}USD`, bybitspot: `${asset}USDT`, hyrotrader: `${asset}USDT`, polymarket: `${asset}-Yes`, backpack: `${asset}_USDC_PERP`, paradex: `${asset}-USD-PERP`, kraken: `PF_${asset === 'BTC' ? 'XBT' : asset}USD`, krakenspot: `${asset}/USD`, mexc: `${asset}USDT`, okx: `${asset}USDT`, bingx: `${asset}USDT`, bitget: `${asset}USDT`, grvt: `${asset}_USDT_PERP` }[venue],
  ...Object.fromEntries(fields.map((field, index) => [field, 101 + index])),
}));
store.update({ tickers: structuredClone(tickers), markets: tickers.map(t => ({ id: t.id, symbol: t.symbol, ...(venue === 'bingx' ? { precision: { amount: .01, price: .25 } } : {}), ...(venue === 'cryptocom' ? { info: { underlyingSymbol: t.id.split('-')[0] } } : {}), ...(venue === 'mexc' ? { contractSize: (symbols.indexOf(t.symbol.replace('USDT', '')) + 1) * .25 } : {}), ...(venue === 'okx' ? { info: { uly: t.id.replace('-SWAP', '') } } : {}) })) });
const exchange = new Exchange(venue, { publicDataOnly: true, applicationId: 'fixture', enableInverseMarkets: ['bybit', 'bybitdemo'].includes(venue) }, store);
const socket = venue === 'bybitinverse' ? exchange.publicInverseWebsocket : venue.startsWith('kraken')
  ? new (require(path.join(root, `packages/safe-cex/src/exchanges/kraken/${venue}.ws-public.ts`))[venue === 'kraken' ? 'KrakenPublicWebsocket' : 'KrakenSpotPublicWebsocket'])(exchange)
  : exchange.publicWebsocket;
if (venue.startsWith('kraken')) exchange.publicWebsocket = socket;
if (venue === 'bitrue') for (const row of tickers) socket.ensureTickerSubscription(row.symbol);
const samples = [
  ['null', null], ['numeric-string', '12.5'], ['prefix-string', '12.5tail'],
  ['nan-string', 'NaN'], ['malformed-object', { bad: true }],
  ['overflow', '1e400'], ['signed-zero', '-0'], ['false', false],
];
const numericSamples = ['bingx', 'bitget', 'grvt'].includes(venue) ? [...samples, ['large-finite', '1e308'], ['numeric', 12.5]] : samples;
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
const aliasInputs = {
  bingx: { tradePrice: ['tradePrice', 'trade_price'], changePercentage: ['changePercentage', 'change_percentage'], volumeF: ['volumeF', 'volume_f'], valueF: ['valueF', 'value_f'], fairPrice: ['fairPrice', 'fair_price'], indexPrice: ['indexPrice', 'index_price'], fundingRate: ['fundingRate', 'funding_rate'] },
  bitget: { bid: ['bidPr', 'bidPrice', 'bid'], ask: ['askPr', 'askPrice', 'ask'], last: ['lastPr', 'lastPrice', 'last'], mark: ['markPrice', 'markPr', 'mark'], index: ['indexPrice', 'indexPr', 'index'], percentage: ['change24h', 'changeUtc24h', 'change'], fundingRate: ['fundingRate'], fundingTime: ['nextFundingTime', 'fundingTime'], volume: ['baseVolume', 'volume'], quoteVolume: ['quoteVolume'] },
  grvt: { mark: ['mp', 'mark_price'], index: ['ip', 'index_price'], last: ['lp', 'last_price'], bid: ['bb', 'best_bid_price'], ask: ['ba', 'best_ask_price'], fundingRate: ['fr2', 'funding_rate'], openInterest: ['oi', 'open_interest'], buyVolume: ['bv', 'buy_volume_24h_b'], sellVolume: ['sv', 'sell_volume_24h_b'], open: ['op', 'open_price'], fundingTime: ['nf', 'next_funding_time'] },
};
const krakenInputs = {
  bid: ['bid', 'best_bid'], ask: ['ask', 'best_ask'], last: ['last', 'price', 'last_trade'],
  mark: ['mark', 'markPrice'], index: ['index', 'indexPrice'], fundingRate: ['relative_funding_rate'],
  fundingTime: ['next_funding_rate_time', 'next_funding', 'nextFundingTime'], open: ['open24h', 'open_24h'],
};
for (const [field, keys] of Object.entries(krakenInputs)) krakenInputs[field] = [...new Set(keys.flatMap(key => [key, require('lodash/snakeCase')(key), require('lodash/camelCase')(key)]))];
const mexcInputs = {
  last: ['lastPrice', 'last'], mark: ['fairPrice', 'markPrice'], bid: ['bid1', 'bidPrice'], ask: ['ask1', 'askPrice'],
  index: ['indexPrice'], percentage: ['riseFallRate', 'priceChangePercent'], openInterest: ['holdVol', 'openInterest'],
  fundingRate: ['fundingRate'], fundingTime: ['fundingTime', 'nextSettleTime'], volume: ['volume24', 'volume'], quoteVolume: ['amount24', 'quoteVolume'],
};
const paradexSummary = ['bid', 'ask', 'last_traded_price', 'last_price', 'last', 'mark_price', 'oracle_price', 'index_price', 'underlying_price', 'price_change_rate_24h', 'volume_24h', 'volume', 'open_interest', 'funding_rate'];
const bybitInputs = ['bid1Price', 'ask1Price', 'lastPrice', 'markPrice', 'indexPrice', 'price24hPcnt', 'openInterest', 'fundingRate', 'nextFundingTime', 'volume24h', 'turnover24h'];
const channels = ['bybit', 'bybitdemo'].includes(venue) ? { linear: bybitInputs, inverse: bybitInputs }
  : ['bybitinverse', 'bybitspot', 'hyrotrader'].includes(venue) ? { tickers: bybitInputs }
  : venue === 'bingx' ? { contracts: Object.values(aliasInputs.bingx).flat() }
  : venue === 'bitget' ? Object.fromEntries(['primary', 'secondary', 'third'].map(channel => [channel, Object.values(aliasInputs.bitget).flat()]))
  : venue === 'grvt' ? { ...Object.fromEntries(['lite-s', 'lite-d', 'full-s', 'full-d'].map(channel => [channel, Object.values(aliasInputs.grvt).flat()])), ...Object.fromEntries(['mark-zero-lite', 'mark-zero-full', 'mark-stored-lite', 'mark-stored-full'].map(channel => [channel, aliasInputs.grvt.mark])), 'candle-lite': ['c', 'close'], 'candle-full': ['c', 'close'] }
  : venue === 'kraken' ? { ticker: Object.values(krakenInputs).flat() }
  : venue === 'krakenspot' ? { ticker: ['bid', 'ask', 'last', 'volume', 'change_pct'] }
  : venue === 'mexc' ? { 'push.ticker': Object.values(mexcInputs).flat() }
  : venue === 'okx' ? { tickers: ['open24h', 'last', 'bidPx', 'askPx', 'volCcy24h', 'vol24h'], 'mark-price': ['markPx'], 'index-tickers': ['idxPx'], 'open-interest': ['oiCcy'], 'funding-rate': ['fundingRate', 'fundingTime'] }
  : venue === 'backpack' ? { ticker: ['c', 'P', 'v', 'q'], trade: ['p'], bookTicker: ['b', 'a'], depth: ['b', 'a'] }
  : venue === 'paradex' ? { markets_summary: paradexSummary, bbo: ['bid_price', 'ask_price', 'index_price'], 'order_book:inserts': ['bid', 'ask'], 'order_book:updates': ['bid', 'ask'], 'order_book:arrays': ['bid', 'ask'], 'order_book:objects': ['bid', 'ask'], trades: ['price', 'size', 'created_at', 'timestamp'], 'trades:single': ['price', 'size', 'created_at', 'timestamp'] }
  : venue === 'polymarket' ? { price_change: ['best_bid', 'best_ask'], last_trade_price: ['price'] }
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
  const candleCleanups = [];
  let sourceServer;
  let candleCallbacks = 0;
  try {
    if (venue === 'grvt') {
      const { WebSocketServer } = require('ws');
      sourceServer = new WebSocketServer({ host: '127.0.0.1', port: 0 });
      await new Promise((resolve, reject) => { sourceServer.once('listening', resolve); sourceServer.once('error', reject); });
      socket.ws = socket.createWebsocket(`ws://127.0.0.1:${sourceServer.address().port}`);
      await new Promise((resolve, reject) => {
        const timeout = setTimeout(() => reject(Error('ticker source loopback socket did not open')), 10000);
        socket.ws.addEventListener('open', () => { clearTimeout(timeout); resolve(); });
        socket.ws.addEventListener('error', error => { clearTimeout(timeout); reject(error); });
      });
      for (const ticker of tickers) candleCleanups.push(exchange.listenOHLCV({ symbol: ticker.symbol, interval: '1m' }, () => { candleCallbacks++; }));
    }
    for (const [channel, keys] of Object.entries(channels)) for (const key of keys) for (const [label, value] of numericSamples) {
      const priorLast = channel.startsWith('mark-zero') ? 0 : 103;
      const caseTickers = ['bybit', 'bybitdemo'].includes(venue) && channel === 'inverse' ? tickers.map(t => ({ ...t, id: t.id.replace('USDT', 'USD'), symbol: t.symbol.replace('USDT', 'USD') })) : tickers;
      store.update({ tickers: structuredClone(caseTickers).map(ticker => ({ ...ticker, last: priorLast })) });
      candleCallbacks = 0;
      let frames = caseTickers.map((ticker, index) => {
        const offset = index * 11;
        if (['bybit', 'bybitdemo', 'bybitinverse', 'bybitspot', 'hyrotrader'].includes(venue)) {
          const data = { symbol: ticker.symbol, bid1Price: String(29.5 + offset), ask1Price: String(33.75 + offset), lastPrice: String(31.5 + offset), markPrice: String(37.75 + offset), indexPrice: String(38.25 + offset), price24hPcnt: String(.125 + index), openInterest: String(91.25 + offset), fundingRate: String(.0025 + index), nextFundingTime: String(1700000000 + offset), volume24h: String(53.25 + offset), turnover24h: String(1091.75 + offset) };
          if (index === 0) data[key] = value;
          return JSON.stringify({ topic: `tickers.${ticker.symbol}`, type: 'delta', data });
        }
        if (['bingx', 'bitget', 'grvt'].includes(venue)) {
          const groups = aliasInputs[venue];
          const data = {};
          const numbers = { tradePrice: 31.5, last: 31.5, changePercentage: .125, percentage: .125, volumeF: 53.25, volume: 53.25, valueF: 1091.75, quoteVolume: 1091.75, fairPrice: 37.75, mark: 37.75, indexPrice: 38.25, index: 38.25, fundingRate: .0025, fundingTime: 1700000000, bid: 29.5, ask: 33.75, openInterest: 91.25, buyVolume: 23.25, sellVolume: 30.5, open: 19.25 };
          for (const [field, aliases] of Object.entries(groups)) {
            const selected = aliases.includes(key) ? aliases.indexOf(key) : 0;
            for (let alias = selected; alias < aliases.length; alias++) data[aliases[alias]] = String(numbers[field] + offset + alias * 3);
          }
          if (index === 0) data[key] = value;
          if (venue === 'bingx') return { symbol: ticker.id, ...data };
          if (venue === 'bitget') return { instId: ticker.symbol, ...data };
          const full = channel.startsWith('full') || channel.endsWith('full');
          if (channel.startsWith('candle')) {
            const feed = { i: ticker.id, ot: '1700000000000000000', o: '19', h: '45', l: '17', c: String(31.5 + offset), close: String(39.25 + offset), vb: '2' };
            if (full) { feed.instrument = feed.i; delete feed.i; }
            if (key === 'close') delete feed.c;
            if (index === 0) feed[key] = value;
            return JSON.stringify(full ? { stream: 'v1.candle', selector: `${ticker.id}@CI_1_M-TRADE`, feed } : { s: 'v1.candle', s1: `${ticker.id}@CI_1_M-TRADE`, f: feed });
          }
          if (channel.startsWith('mark-')) {
            for (const field of Object.keys(data)) if (!aliasInputs.grvt.mark.includes(field)) delete data[field];
          }
          const stream = `v1.ticker.${channel.endsWith('s') && !channel.startsWith('mark-') ? 's' : 'd'}`;
          return JSON.stringify(full ? { stream, feed: { instrument: ticker.id, ...data } } : { s: stream, f: { i: ticker.id, ...data } });
        }
        if (venue === 'kraken' || venue === 'mexc') {
          const groups = venue === 'kraken' ? krakenInputs : mexcInputs;
          const data = {};
          for (const [field, keys] of Object.entries(groups)) {
            const selected = keys.includes(key) ? key : keys[0];
            const good = { last: 31.5, mark: 37.75, index: 38.25, bid: 29.5, ask: 33.75, volume: 53.25, quoteVolume: 1091.75, openInterest: 91.25, fundingRate: .0025, fundingTime: 1700000000, percentage: .125, open: 19.25 }[field] + offset;
            for (let i = keys.indexOf(selected) + 1; i < keys.length; i++) data[keys[i]] = String(good + (i + 1) * 73.125);
            data[selected] = index === 0 && keys.includes(key) ? value : String(good);
          }
          if (venue === 'kraken') Object.assign(data, { funding_rate: String(.725 + index), fundingRate: String(.875 + index), vol24h: String(53.25 + offset) });
          return JSON.stringify(venue === 'kraken' ? { feed: channel, product_id: ticker.symbol, ...data } : { channel, symbol: ticker.id, data });
        }
        if (venue === 'krakenspot') {
          const data = { symbol: ticker.symbol, bid: String(29.5 + offset), ask: String(33.75 + offset), last: String(31.5 + offset), volume: String(53.25 + offset), vwap: String(42.125 + offset), change_pct: String(.125 + index) };
          if (index === 0) data[key] = value;
          return data;
        }
        if (venue === 'okx') {
          const instId = channel === 'index-tickers' ? ticker.id.replace('-SWAP', '') : ticker.id;
          const data = { instId, open24h: String(19.25 + offset), last: String(31.5 + offset), bidPx: String(29.5 + offset), askPx: String(33.75 + offset), volCcy24h: String(53.25 + offset), vol24h: String(1091.75 + offset), markPx: String(37.75 + offset), idxPx: String(38.25 + offset), oiCcy: String(91.25 + offset), fundingRate: String(.0025 + index), fundingTime: String(1700000000 + offset) };
          if (index === 0) data[key] = value;
          return JSON.stringify({ arg: { channel, instId }, data: [data] });
        }
        if (venue === 'backpack') {
          const data = { e: channel, s: ticker.symbol, c: String(31.5 + offset), P: String(.125 + index), v: String(53.25 + offset), q: String(1091.75 + offset), p: String(37.75 + offset), b: String(29.5 + offset), a: String(33.75 + offset) };
          if (channel === 'depth') {
            data.b = [[index === 0 && key === 'b' ? value : String(29.5 + offset), '1']];
            data.a = [[index === 0 && key === 'a' ? value : String(33.75 + offset), '2']];
          } else if (index === 0) data[key] = value;
          return JSON.stringify({ stream: `${channel}.${ticker.symbol}`, data });
        }
        if (venue === 'paradex') {
          const kind = channel.split(':')[0];
          let data;
          if (kind === 'markets_summary') {
            data = { symbol: ticker.symbol, bid: String(29.5 + offset), ask: String(33.75 + offset), last_traded_price: String(31.5 + offset), mark_price: String(37.75 + offset), oracle_price: String(38.25 + offset), price_change_rate_24h: String(.125 + index), volume_24h: String(53.25 + offset), open_interest: String(91.25 + offset), funding_rate: String(.0025 + index) };
            const chain = [['last_traded_price', 'last_price', 'last'], ['oracle_price', 'index_price', 'underlying_price'], ['volume_24h', 'volume']].find(keys => keys.includes(key));
            if (chain) {
              data[key] = data[chain[0]];
              for (const earlier of chain.slice(0, chain.indexOf(key))) data[earlier] = null;
            }
          } else if (kind === 'bbo') {
            data = { bid_price: String(29.5 + offset), ask_price: String(33.75 + offset), index_price: String(38.25 + offset) };
          } else if (kind === 'trades') {
            data = { market: ticker.symbol, price: String(31.5 + offset), size: String(2.25 + offset), created_at: 1700000000000 + offset };
            if (key === 'timestamp') { data.timestamp = data.created_at; delete data.created_at; }
          } else {
            const bid = index === 0 && key === 'bid' ? value : String(29.5 + offset);
            const ask = index === 0 && key === 'ask' ? value : String(33.75 + offset);
            const bids = [[String(17.25 + offset), '1'], [bid, '2'], [String(21.5 + offset), '3']];
            const asks = [[String(49.25 + offset), '4'], [ask, '5'], [String(41.5 + offset), '6']];
            data = channel.endsWith('arrays') ? { bids, asks }
              : channel.endsWith('objects') ? { bids: bids.map(([price, amount]) => ({ price, amount })), asks: asks.map(([price, amount]) => ({ price, amount })) }
              : { [channel.split(':')[1]]: [...bids.map(([price, size]) => ({ side: 'BUY', price, size })), ...asks.map(([price, size]) => ({ side: 'SELL', price, size }))] };
          }
          if (kind !== 'order_book' && index === 0) data[key] = value;
          return kind === 'markets_summary' || channel === 'trades' ? data : JSON.stringify({ jsonrpc: '2.0', method: 'subscription', params: { channel: `${kind}.${ticker.symbol}`, data } });
        }
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
      if (venue === 'bingx') frames = [JSON.stringify({ dataType: 'market.contracts', data: { contracts: [frames[1], frames[0], frames[2]] } })];
      if (venue === 'bitget') frames = [JSON.stringify({ arg: { channel: 'ticker' }, data: [frames[1], frames[0], frames[2]] })];
      if (venue === 'krakenspot') frames = [JSON.stringify({ channel, type: 'update', data: [frames[1], frames[0], frames[2]] })];
      if (venue === 'paradex' && ['markets_summary', 'trades'].includes(channel)) frames = [JSON.stringify({ jsonrpc: '2.0', method: 'subscription', params: { channel, data: [frames[1], frames[0], frames[2]] } })];
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
        const receiver = ['bybit', 'bybitdemo'].includes(venue) && channel === 'inverse' ? exchange.publicWebsocketSecondary
          : venue === 'bitget' && channel === 'secondary' ? exchange.secondsTickersWebsocket
          : venue === 'bitget' && channel === 'third' ? exchange.thirdTickersWebsocket
          : venue === 'bitunix' && channel === 'price' ? exchange.publicWebsocketSecondary
          : venue.startsWith('binance') && venue !== 'binancespot' && channel !== 'bookTicker' ? socket.marketWebsocket : socket;
        if (['cryptocom', 'polymarket', 'backpack', 'paradex'].includes(venue)) {
          const fresh = new Exchange(venue, { publicDataOnly: true, applicationId: 'fixture', tuning: { consoleLogging: false } }, store);
          if (venue === 'polymarket') for (const ticker of tickers) fresh.tokenToSymbol.set(ticker.id, ticker.symbol);
          try { await fresh.publicWebsocket.onMessage({ data: frame }); } finally {
            const observedTickers = structuredClone(store.tickers);
            fresh.dispose();
            if (venue === 'polymarket') store.update({ tickers: observedTickers });
          }
        } else await receiver.onMessage({ data: frame });
      }
      assert.equal(store.tickers.length, 3, 'constructed ticker corpus retains all rows');
      cases.push({ channel, field: key, label, frames, priorLast, candleCallbacks, expected: store.tickers.map(ticker => ({
        id: ticker.id, symbol: ticker.symbol,
        fields: Object.fromEntries(fields.map(field => [field, bits(ticker[field])])),
      })) });
    }
  } finally {
    for (const cleanup of candleCleanups) cleanup();
    exchange.dispose();
    if (sourceServer) {
      for (const peer of sourceServer.clients) peer.terminate();
      await new Promise(resolve => sourceServer.close(resolve));
    }
  }
  process.stdout.write(JSON.stringify({ venue, fields, capture: { architecture: process.arch, nodeMajor: Number(process.versions.node.split('.')[0]), timezone: Intl.DateTimeFormat().resolvedOptions().timeZone }, cases }), () => process.exit(0));
})().catch(error => { console.error(error); process.exitCode = 1; });
