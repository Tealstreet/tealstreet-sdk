import type { ExchangeOptions, Market, Ticker } from '../../types';

import { afterEach, describe, expect, it, vi } from 'vitest';

import { ENDPOINTS } from '../../exchanges/okx/okx.types';
import { OKXEUExchange } from '../reference/okx/okxeu.exchange';
import { DefaultStore } from '../../store/store.base';

const options: ExchangeOptions = {
  key: 'test-key',
  secret: 'test-secret',
  passphrase: 'test-passphrase',
  testnet: false,
  tuning: { consoleLogging: false },
};

const swapInstrument = {
  instType: 'SWAP',
  instId: 'BTC-USDT-SWAP',
  instFamily: 'BTC-USDT',
  uly: 'BTC-USDT',
  ctType: 'linear',
  ctVal: '0.01',
  ctValCcy: 'BTC',
  settleCcy: 'USDT',
  state: 'live',
  lotSz: '0.01',
  tickSz: '0.1',
  minSz: '0.01',
  lever: '100',
  maxIcebergSz: '1000',
  maxLmtSz: '1000',
  maxMktSz: '1000',
  maxStopSz: '1000',
  maxTriggerSz: '1000',
  maxTwapSz: '1000',
  ruleType: 'normal',
};

const xperpInstrument = {
  instType: 'FUTURES',
  instId: 'ETH-USD_UM_XPERP-310404',
  instFamily: 'ETH-USD_UM_XPERP',
  uly: 'ETH-USD',
  ctType: 'linear',
  ctVal: '0.001',
  ctValCcy: 'ETH',
  settleCcy: 'USD',
  state: 'live',
  lotSz: '1',
  tickSz: '0.01',
  minSz: '1',
  lever: '50',
  maxIcebergSz: '1000',
  maxLmtSz: '1000',
  maxMktSz: '1000',
  maxStopSz: '1000',
  maxTriggerSz: '1000',
  maxTwapSz: '1000',
  ruleType: 'xperp',
};

const regularFutureInstrument = {
  ...xperpInstrument,
  instId: 'ETH-USD-260925',
  instFamily: 'ETH-USD',
  ruleType: 'normal',
};

const createExchange = () => new OKXEUExchange('okxeu', options, new DefaultStore());

const ticker = (instId: string, instType = 'SWAP') => ({
  instType,
  instId,
  last: '100',
  bidPx: '99',
  askPx: '101',
  open24h: '90',
  volCcy24h: '10',
  vol24h: '1000',
});

const storedTicker = (market: Market): Ticker => ({
  id: market.id,
  symbol: market.symbol,
  bid: 0,
  ask: 0,
  last: 0,
  mark: 0,
  index: 0,
  percentage: 0,
  openInterest: 0,
  fundingRate: 0,
  fundingTime: 0,
  volume: 0,
  quoteVolume: 0,
});

afterEach(() => {
  vi.restoreAllMocks();
});

describe('OKX EU X-Perp market support', () => {
  it('loads private markets from account-available instruments', async () => {
    const exchange = createExchange();
    const get = vi.fn(async (endpoint: string, opts?: { params?: Record<string, string> }) => {
      if (endpoint === ENDPOINTS.ACCOUNT_INSTRUMENTS && opts?.params?.instType === 'SWAP') {
        return { data: { data: [] } };
      }
      if (endpoint === ENDPOINTS.ACCOUNT_INSTRUMENTS && opts?.params?.instType === 'FUTURES') {
        return { data: { data: [xperpInstrument, regularFutureInstrument] } };
      }
      return { data: { data: [] } };
    });
    exchange.xhr.get = get as any;

    const markets = await exchange.fetchMarkets();

    expect(get).toHaveBeenCalledWith(ENDPOINTS.ACCOUNT_INSTRUMENTS, { params: { instType: 'SWAP' } });
    expect(get).toHaveBeenCalledWith(ENDPOINTS.ACCOUNT_INSTRUMENTS, { params: { instType: 'FUTURES' } });
    expect(markets.map((market) => market.id)).toEqual(['ETH-USD_UM_XPERP-310404']);
    expect(markets[0]).toMatchObject({
      symbol: 'ETHUSD_UM_XPERP310404',
      prettySymbol: 'ETH/USD XPERP',
      base: 'ETH',
      quote: 'USD',
      active: true,
      isInverse: false,
    });
    expect('isExpiring' in markets[0]).toBe(false);
  });

  it('keeps public OKX EU market discovery available for public-only sessions', async () => {
    const exchange = new OKXEUExchange(
      'okxeu',
      { ...options, publicDataOnly: true },
      new DefaultStore(),
    );
    const get = vi.fn(async (endpoint: string, opts?: { params?: Record<string, string> }) => {
      if (endpoint === ENDPOINTS.MARKETS && opts?.params?.instType === 'SWAP') {
        return { data: { data: [swapInstrument] } };
      }
      if (endpoint === ENDPOINTS.MARKETS && opts?.params?.instType === 'FUTURES') {
        return { data: { data: [xperpInstrument, regularFutureInstrument] } };
      }
      return { data: { data: [] } };
    });
    exchange.xhr.get = get as any;

    const markets = await exchange.fetchMarkets();

    expect(get).toHaveBeenCalledWith(ENDPOINTS.MARKETS, { params: { instType: 'SWAP' } });
    expect(get).toHaveBeenCalledWith(ENDPOINTS.MARKETS, { params: { instType: 'FUTURES' } });
    expect(markets.map((market) => market.id)).toEqual(['BTC-USDT-SWAP', 'ETH-USD_UM_XPERP-310404']);
  });

  it('fetches and parses FUTURES X-Perp tickers for stored OKX EU markets', async () => {
    const exchange = createExchange();
    const markets = await (async () => {
      exchange.xhr.get = vi.fn(async (endpoint: string, opts?: { params?: Record<string, string> }) => {
        if (endpoint === ENDPOINTS.ACCOUNT_INSTRUMENTS && opts?.params?.instType === 'SWAP') {
          return { data: { data: [swapInstrument] } };
        }
        if (endpoint === ENDPOINTS.ACCOUNT_INSTRUMENTS && opts?.params?.instType === 'FUTURES') {
          return { data: { data: [xperpInstrument] } };
        }
        return { data: { data: [] } };
      }) as any;
      return exchange.fetchMarkets();
    })();
    exchange.store.update({ markets, loaded: { ...exchange.store.loaded, markets: true } });

    const get = vi.fn(async (endpoint: string, opts?: { params?: Record<string, string> }) => {
      if (endpoint === ENDPOINTS.TICKERS && opts?.params?.instType === 'SWAP') {
        return { data: { data: [ticker('BTC-USDT-SWAP')] } };
      }
      if (endpoint === ENDPOINTS.TICKERS && opts?.params?.instType === 'FUTURES') {
        return { data: { data: [ticker('ETH-USD_UM_XPERP-310404', 'FUTURES'), ticker('ETH-USD-260925', 'FUTURES')] } };
      }
      return { data: { data: [] } };
    });
    exchange.xhr.get = get as any;

    const rawTickers = await exchange.fetchTickersRaw();
    const parsed = exchange.parseTickers(rawTickers);

    expect(get).toHaveBeenCalledWith(ENDPOINTS.TICKERS, { params: { instType: 'SWAP' } });
    expect(get).toHaveBeenCalledWith(ENDPOINTS.TICKERS, { params: { instType: 'FUTURES' } });
    expect(parsed.map((item) => item.symbol)).toEqual(['BTCUSDT', 'ETHUSD_UM_XPERP310404']);
  });

  it('routes one index ticker update to all markets using the same underlying', () => {
    const exchange = createExchange();
    const swapMarket = {
      ...swapInstrument,
      instId: 'ETH-USD-SWAP',
      instFamily: 'ETH-USD',
      uly: 'ETH-USD',
      ctType: 'inverse',
      ctValCcy: 'USD',
      settleCcy: 'ETH',
    };
    const markets: Market[] = [
      {
        id: swapMarket.instId,
        symbol: 'ETHUSD',
        base: 'ETH',
        quote: 'USD',
        active: true,
        isInverse: true,
        precision: { amount: 1, price: 0.1 },
        limits: { amount: { min: 1, max: 1000 }, leverage: { min: 1, max: 100 } },
        contractSize: 10,
        info: swapMarket,
      },
      {
        id: xperpInstrument.instId,
        symbol: 'ETHUSD_UM_XPERP310404',
        prettySymbol: 'ETH/USD XPERP',
        base: 'ETH',
        quote: 'USD',
        active: true,
        isInverse: false,
        precision: { amount: 1, price: 0.01 },
        limits: { amount: { min: 0.001, max: 1000 }, leverage: { min: 1, max: 50 } },
        contractSize: 0.001,
        info: xperpInstrument,
      },
    ];
    exchange.store.update({
      markets,
      tickers: markets.map(storedTicker),
      loaded: { ...exchange.store.loaded, markets: true, tickers: true },
    });

    exchange.publicWebsocket.handleIndexTickerEvents({ data: [{ instId: 'ETH-USD', idxPx: '2500' }] });

    expect(exchange.store.tickersById['ETH-USD-SWAP'].index).toBe(2500);
    expect(exchange.store.tickersById['ETH-USD_UM_XPERP-310404'].index).toBe(2500);
    expect(exchange.hasIndexPriceLoaded('ETH-USD-SWAP')).toBe(true);
    expect(exchange.hasIndexPriceLoaded('ETH-USD_UM_XPERP-310404')).toBe(true);
  });

  it('subscribes private and business websocket topics for SWAP and FUTURES', () => {
    const exchange = createExchange();
    const privateSend = vi.spyOn(exchange.privateWebsocket, 'sendWithLog').mockImplementation(() => {});
    const businessSend = vi.spyOn(exchange.businessWebsocket, 'sendWithLog').mockImplementation(() => {});

    exchange.privateWebsocket.subscribe();
    exchange.businessWebsocket.subscribe();

    expect(JSON.parse(privateSend.mock.calls[0][0])).toMatchObject({
      op: 'subscribe',
      args: [
        { channel: 'account' },
        { channel: 'positions', instType: 'SWAP' },
        { channel: 'orders', instType: 'SWAP' },
        { channel: 'positions', instType: 'FUTURES' },
        { channel: 'orders', instType: 'FUTURES' },
      ],
    });
    expect(JSON.parse(businessSend.mock.calls[0][0])).toMatchObject({
      op: 'subscribe',
      args: [
        { channel: 'orders-algo', instType: 'SWAP' },
        { channel: 'algo-advance', instType: 'SWAP' },
        { channel: 'orders-algo', instType: 'FUTURES' },
        { channel: 'algo-advance', instType: 'FUTURES' },
      ],
    });
  });
});
