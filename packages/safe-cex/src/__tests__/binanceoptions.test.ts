import type { AxiosInstance, InternalAxiosRequestConfig } from 'axios';

import { afterEach, describe, expect, it, vi } from 'vitest';

import { createAPI } from './reference/binance/binanceoptions.api';
import { BinanceOptionsExchange } from './reference/binance/binanceoptions.exchange';
import {
  BINANCE_OPTION_GREEK_UNITS,
  formatBinanceOptionOrder,
  mapBinanceAccountGreeks,
  mapBinanceOptionBalance,
  mapBinanceOptionCandle,
  mapBinanceOptionFill,
  mapBinanceOptionHistoricalOrder,
  mapBinanceOptionMarket,
  mapBinanceOptionOrder,
  mapBinanceOptionPosition,
  mapBinanceOptionQuote,
  mapBinanceOptionTicker,
  optionContractsByUnderlying,
} from './reference/binance/binanceoptions.mappers';
import { ENDPOINTS } from './reference/binance/binanceoptions.types';
import { DefaultStore } from '../store/store.base';
import { OrderSide, OrderStatus, OrderTimeInForce, OrderType, PositionSide } from '../types';

// Recorded from eapi.binance.com/eapi/v1/exchangeInfo, /mark and /ticker on 2026-10-20.
const EXCHANGE_INFO = {
  optionContracts: [
    { baseAsset: 'BTC', quoteAsset: 'USDT', underlying: 'BTCUSDT', settleAsset: 'USDT', nakedSell: true },
    { baseAsset: 'DOGE', quoteAsset: 'USDT', underlying: 'DOGEUSDT', settleAsset: 'USDT', nakedSell: false },
  ],
  optionSymbols: [
    {
      expiryDate: 1798185600000,
      filters: [
        { filterType: 'PRICE_FILTER', minPrice: '775.000', maxPrice: '6940.000', tickSize: '5.000' },
        { filterType: 'LOT_SIZE', minQty: '0.01', maxQty: '200', stepSize: '0.01' },
      ],
      symbol: 'BTC-261225-85000-C',
      side: 'CALL',
      strikePrice: '85000.000',
      underlying: 'BTCUSDT',
      unit: 1,
      minQty: '0.01',
      maxQty: '200',
      priceScale: 3,
      quantityScale: 2,
      quoteAsset: 'USDT',
      status: 'TRADING',
      contractType: 'CRYPTO_OPTIONS',
      underlyingType: 'CRYPTO',
    },
    {
      expiryDate: 1792742400000,
      filters: [
        { filterType: 'PRICE_FILTER', minPrice: '0.2900', maxPrice: '2.5600', tickSize: '0.0100' },
        { filterType: 'LOT_SIZE', minQty: '0.01', maxQty: '4000', stepSize: '0.01' },
      ],
      symbol: 'DOGE-261023-0.08-C',
      side: 'CALL',
      strikePrice: '0.0800',
      underlying: 'DOGEUSDT',
      unit: 1000,
      minQty: '0.01',
      maxQty: '4000',
      priceScale: 4,
      quantityScale: 2,
      quoteAsset: 'USDT',
      status: 'TRADING',
      contractType: 'CRYPTO_OPTIONS',
      underlyingType: 'CRYPTO',
    },
  ],
};
const [BTC_ROW, DOGE_ROW] = EXCHANGE_INFO.optionSymbols;
const BTC = 'BTC-261225-85000-C';
const DOGE = 'DOGE-261023-0.08-C';

const BTC_MARK = {
  symbol: BTC,
  markPrice: '3845.503',
  bidIV: '0.36287904',
  askIV: '0.36615849',
  markIV: '0.36',
  delta: '0.44767474',
  theta: '-37.86983988',
  gamma: '0.00003129',
  vega: '137.16680603',
  highPriceLimit: '6920',
  lowPriceLimit: '770',
  riskFreeInterest: '0.053',
};
const DOGE_MARK = {
  symbol: DOGE,
  markPrice: '1.4147',
  bidIV: '0.56222379',
  askIV: '-1.0',
  markIV: '0.61678767',
  delta: '0.44114908',
  theta: '-0.00029056',
  gamma: '88.85533666',
  vega: '0.00002841',
};
const BTC_TICKER = {
  symbol: BTC,
  priceChange: '705',
  priceChangePercent: '0.185',
  lastPrice: '4520',
  lastQty: '1',
  open: '3815',
  high: '4520',
  low: '3705',
  volume: '2.22',
  amount: '9831.45',
  bidPrice: '3885',
  askPrice: '3930',
  strikePrice: '85000',
  exercisePrice: '81565.34009019',
};

// The rest are the documented examples at developers.binance.com/docs/derivatives/options-trading.
const MARGIN_ACCOUNT = {
  asset: [
    {
      asset: 'USDT',
      marginBalance: '10099.448',
      equity: '10094.44662',
      available: '8725.92524',
      initialMargin: '1084.52138',
      maintMargin: '151.00138',
      unrealizedPNL: '-5.00138',
      adjustedEquity: '34.13282285',
    },
  ],
  greek: [{ underlying: 'BTCUSDT', delta: '-0.05', gamma: '-0.002', theta: '-0.05', vega: '-0.002' }],
  time: 1762843368098,
  canTrade: true,
};
const POSITION = {
  entryPrice: '1000',
  symbol: BTC,
  side: 'SHORT',
  quantity: '-0.1',
  markValue: '105.00138',
  unrealizedPNL: '-5.00138',
  markPrice: '1050.0138',
  strikePrice: '9000',
  expiryDate: 1593511200000,
  optionSide: 'CALL',
  quoteAsset: 'USDT',
};
// bigIntAsStringJsonParse turns the documented 4611875134427365000 into a string.
const OPEN_ORDER = {
  orderId: '4611875134427365000',
  symbol: BTC,
  price: '100',
  quantity: '1',
  executedQty: '0.25',
  side: 'BUY',
  type: 'LIMIT',
  timeInForce: 'GTC',
  reduceOnly: false,
  createTime: 1592465880683,
  updateTime: 1592465880683,
  status: 'ACCEPTED',
  avgPrice: '0',
  clientOrderId: 'mine-1',
  optionSide: 'CALL',
  quoteAsset: 'USDT',
};
const USER_TRADE = {
  id: '4611875134427365000',
  tradeId: 239,
  orderId: '4611875134427365000',
  symbol: BTC,
  price: '100',
  quantity: '1',
  fee: '-1.04378629',
  realizedProfit: '0.00000000',
  side: 'BUY',
  type: 'LIMIT',
  liquidity: 'TAKER',
  time: 1592465880683,
  quoteAsset: 'USDT',
};
const KLINE = [
  1762779600000,
  '1300.000',
  '1310.000',
  '1290.000',
  '1305.000',
  '0.1000',
  1762780499999,
  '130.0000000',
  1,
  '0.1000',
  '130.0000000',
  '0',
];

const contracts = optionContractsByUnderlying(EXCHANGE_INFO);
const btcMarket = mapBinanceOptionMarket(BTC_ROW, contracts)!;
const dogeMarket = mapBinanceOptionMarket(DOGE_ROW, contracts)!;

describe('binanceoptions mappers', () => {
  it('maps an exchangeInfo row to an option market named by its base asset', () => {
    expect(btcMarket).toMatchObject({
      id: BTC,
      symbol: BTC,
      base: 'BTC',
      quote: 'USDT',
      precision: { amount: 0.01, price: 5 },
      limits: { amount: { min: 0.01, max: 200 }, leverage: { min: 1, max: 1 }, price: { min: 775 } },
      option: {
        underlying: 'BTC',
        expiry: 1798185600000,
        strike: 85000,
        right: 'call',
        style: 'european',
        settlement: 'cash',
        premiumAsset: 'USDT',
        settleAsset: 'USDT',
      },
      contractSize: 1,
      isExpiring: true,
    });
    expect(dogeMarket.option).toMatchObject({ underlying: 'DOGE', strike: 0.08 });
  });

  it('drops a row that is not trading or whose underlying it cannot name', () => {
    expect(mapBinanceOptionMarket({ ...BTC_ROW, status: 'CLOSE' }, contracts)).toBeUndefined();
    expect(mapBinanceOptionMarket({ ...BTC_ROW, underlying: 'XYZUSDT' }, contracts)).toBeUndefined();
  });

  it('joins mark and ticker into a quote, reading -1 IV as unavailable', () => {
    expect(mapBinanceOptionQuote(BTC_MARK, BTC_TICKER)).toEqual({
      symbol: BTC,
      bid: 3885,
      ask: 3930,
      mark: 3845.503,
      markIv: 0.36,
      bidIv: 0.36287904,
      askIv: 0.36615849,
      underlyingPrice: 81565.34009019,
      greeks: {
        delta: 0.44767474,
        gamma: 0.00003129,
        vega: 137.16680603,
        theta: -37.86983988,
        units: BINANCE_OPTION_GREEK_UNITS,
      },
    });
    expect(mapBinanceOptionQuote(DOGE_MARK, undefined).askIv).toBe(0);
  });

  it('reads the ticker change as a fraction and reports a percentage', () => {
    expect(mapBinanceOptionTicker(BTC_TICKER, BTC_MARK)).toMatchObject({
      symbol: BTC,
      bid: 3885,
      ask: 3930,
      last: 4520,
      mark: 3845.503,
      index: 81565.34009019,
      percentage: 18.5,
      volume: 2.22,
      quoteVolume: 9831.45,
    });
  });

  it('maps the margin account to a balance and account greeks', () => {
    expect(mapBinanceOptionBalance(MARGIN_ACCOUNT)).toMatchObject({
      total: 10094.44662,
      free: 8725.92524,
      used: 1084.52138,
      upnl: -5.00138,
    });
    expect(mapBinanceAccountGreeks(MARGIN_ACCOUNT, contracts)).toEqual([
      expect.objectContaining({ underlying: 'BTC', delta: -0.05, gamma: -0.002, theta: -0.05, vega: -0.002 }),
    ]);
  });

  it('maps a short position and scales the mark greeks by its size in the underlying', () => {
    const position = mapBinanceOptionPosition(POSITION, btcMarket, BTC_MARK, 1);
    expect(position).toMatchObject({
      symbol: BTC,
      side: PositionSide.Short,
      entryPrice: 1000,
      notional: 105.00138,
      unrealizedPnl: -5.00138,
      contracts: 0.1,
      leverage: 1,
      extra: { contractsNative: 0.1 },
    });
    expect(position.option?.greeks?.delta).toBeCloseTo(-0.044767474, 12);
    expect(position.option?.greeks?.vega).toBeCloseTo(-13.716680603, 9);

    const doge = mapBinanceOptionPosition(
      { ...POSITION, symbol: DOGE, side: 'LONG', quantity: '2' },
      dogeMarket,
      DOGE_MARK,
      1,
    );
    expect(doge.option?.greeks?.delta).toBeCloseTo(2 * 1000 * 0.44114908, 9);
    expect(mapBinanceOptionPosition(POSITION, btcMarket, undefined, 1).option).toEqual({});
  });

  it('maps an open order, keeping the big order ID as a string', () => {
    expect(mapBinanceOptionOrder(OPEN_ORDER, 7)).toMatchObject({
      id: '4611875134427365000',
      status: OrderStatus.Open,
      type: OrderType.Limit,
      side: OrderSide.Buy,
      price: 100,
      amount: 1,
      filled: 0.25,
      remaining: 0.75,
      clientOrderId: 'mine-1',
      timeInForce: OrderTimeInForce.GoodTillCancel,
      lastUpdate: 7,
    });
  });

  it('maps order history statuses', () => {
    expect(mapBinanceOptionHistoricalOrder({ ...OPEN_ORDER, status: 'FILLED', avgPrice: '101' })).toMatchObject({
      id: '4611875134427365000',
      status: OrderStatus.Closed,
      averagePrice: 101,
      createdTime: 1592465880683,
    });
    expect(mapBinanceOptionHistoricalOrder({ ...OPEN_ORDER, status: 'CANCELLED' }).status).toBe(OrderStatus.Canceled);
  });

  it('maps a fill with the fee as a positive commission and no realized PnL', () => {
    const fill = mapBinanceOptionFill(USER_TRADE);
    expect(fill).toEqual({
      id: '239',
      timestamp: 1592465880.683,
      symbol: BTC,
      orderId: '4611875134427365000',
      price: 100,
      qty: 1,
      side: OrderSide.Buy,
      commission: 1.04378629,
      commissionAsset: 'USDT',
      maker: false,
    });
  });

  it('maps a kline row with the timestamp in seconds', () => {
    expect(mapBinanceOptionCandle(KLINE)).toEqual({
      timestamp: 1762779600,
      open: 1300,
      high: 1310,
      low: 1290,
      close: 1305,
      volume: 0.1,
    });
  });

  it('formats a post-only limit order with the flag Binance documents', () => {
    expect(
      formatBinanceOptionOrder(
        {
          symbol: BTC,
          type: OrderType.Limit,
          side: OrderSide.Sell,
          amount: 0.123,
          price: 3847,
          timeInForce: OrderTimeInForce.PostOnly,
          reduceOnly: true,
        },
        btcMarket,
        'cid',
      ),
    ).toEqual({
      symbol: BTC,
      side: 'SELL',
      type: 'LIMIT',
      quantity: '0.12',
      price: '3845',
      timeInForce: 'GTC',
      postOnly: true,
      reduceOnly: true,
      clientOrderId: 'cid',
    });
  });
});

describe('binanceoptions mapper edge cases', () => {
  it('leaves greeks off a quote whose mark row has none', () => {
    expect(mapBinanceOptionQuote({ ...BTC_MARK, delta: '' }, BTC_TICKER)).not.toHaveProperty('greeks');
  });

  it('reads a post-only order as PostOnly', () => {
    expect(mapBinanceOptionOrder({ ...OPEN_ORDER, postOnly: true }, 1).timeInForce).toBe(OrderTimeInForce.PostOnly);
  });

  it('marks a MAKER fill as maker and a TAKER fill as not', () => {
    expect(mapBinanceOptionFill({ ...USER_TRADE, liquidity: 'MAKER' }).maker).toBe(true);
    expect(mapBinanceOptionFill(USER_TRADE).maker).toBe(false);
  });
});

const live: BinanceOptionsExchange[] = [];
afterEach(() => live.splice(0).forEach((e) => e.dispose()));

const create = () => {
  const store = new DefaultStore();
  const exchange = new BinanceOptionsExchange(
    'binanceoptions',
    { key: 'k', secret: 's', tuning: { consoleLogging: false } } as any,
    store,
  );
  live.push(exchange);
  store.update({ markets: [btcMarket, dogeMarket] });
  const calls: Array<[string, string, any]> = [];
  const respond = (method: string, url: string, body: any) => {
    calls.push([method, url, body]);
    if (url === ENDPOINTS.MARK) return Promise.resolve({ data: [BTC_MARK, DOGE_MARK] });
    if (url === ENDPOINTS.TICKERS) return Promise.resolve({ data: [BTC_TICKER] });
    if (url === ENDPOINTS.POSITIONS)
      return Promise.resolve({ data: [POSITION, { ...POSITION, symbol: DOGE, quantity: '0' }] });
    if (url === ENDPOINTS.ORDER && method === 'post')
      return Promise.resolve({ data: { orderId: '4611875134427365001' } });
    return Promise.resolve({ data: {} });
  };
  (exchange.xhr as any) = {
    get: vi.fn((url: string, config?: any) => respond('get', url, config?.params)),
    post: vi.fn((url: string, body?: any) => respond('post', url, body)),
    delete: vi.fn((url: string, config?: any) => respond('delete', url, config?.params)),
  };
  vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => undefined);
  return { exchange, store, calls };
};

const limit = { symbol: BTC, type: OrderType.Limit, side: OrderSide.Buy, amount: 0.5, price: 3800 };

describe('BinanceOptionsExchange requests', () => {
  it('places a limit order by price on /eapi/v1/order with the caller ID', async () => {
    const { exchange, calls } = create();
    expect(await exchange.placeOrder({ ...limit, clientOrderId: 'mine-1' })).toEqual(['4611875134427365001']);
    expect(calls).toEqual([
      [
        'post',
        ENDPOINTS.ORDER,
        {
          symbol: BTC,
          side: 'BUY',
          type: 'LIMIT',
          quantity: '0.5',
          price: '3800',
          timeInForce: 'GTC',
          clientOrderId: 'mine-1',
        },
      ],
    ]);
  });

  it.each([
    [{ impliedVolatility: 0.45 }, /no implied volatility/],
    [{ type: OrderType.Market, price: undefined }, /limit orders only/],
    [{ stopLoss: 3000 }, /do not support stopLoss/],
    [{ price: undefined }, /needs a price/],
    [{ amount: 250 }, /above the 200 maximum/],
    [{ amount: 0.001 }, /rounds to nothing/],
    [{ symbol: 'BTC-261225-1-C' }, /not found/],
  ])('refuses %j before any request', async (change, message) => {
    const { exchange, calls } = create();
    await expect(exchange.placeOrder({ ...limit, ...change } as any)).rejects.toThrow(message);
    expect(calls).toEqual([]);
  });

  it('refuses a whole batch before any request when one order cannot be placed', async () => {
    const { exchange, calls } = create();
    await expect(exchange.placeOrders([limit, { ...limit, impliedVolatility: 0.4 }])).rejects.toThrow(
      /no implied volatility/,
    );
    expect(calls).toEqual([]);
  });

  it('cancels by orderId, and treats an order Binance no longer has as cancelled', async () => {
    const { exchange, store, calls } = create();
    const order = mapBinanceOptionOrder(OPEN_ORDER, 1);
    store.update({ orders: [order] });
    (exchange.xhr.delete as any).mockImplementationOnce((url: string, config: any) => {
      calls.push(['delete', url, config.params]);
      return Promise.reject(Object.assign(new Error('Unknown order sent.'), { code: -2011 }));
    });

    await exchange.cancelOrders([order]);
    expect(calls).toEqual([['delete', ENDPOINTS.ORDER, { symbol: BTC, orderId: '4611875134427365000' }]]);
    expect(store.orders[0].status).toBe(OrderStatus.Canceled);
  });

  it('refuses an amend it cannot place before cancelling anything', async () => {
    const { exchange, store, calls } = create();
    const order = mapBinanceOptionOrder(OPEN_ORDER, 1);
    store.update({ orders: [order] });
    await expect(exchange.updateOrder({ order, update: { amount: 250 } })).rejects.toThrow(/above the 200 maximum/);
    expect(calls).toEqual([]);
    expect(store.orders[0].status).not.toBe(OrderStatus.Canceled);
  });

  it('cancels a symbol through /eapi/v1/allOpenOrders', async () => {
    const { exchange, calls } = create();
    await exchange.cancelSymbolOrders(BTC);
    expect(calls).toEqual([['delete', ENDPOINTS.CANCEL_SYMBOL_ORDERS, { symbol: BTC }]]);
  });

  it('builds the chain for one underlying from /ticker and /mark', async () => {
    const { exchange } = create();
    const rows = await exchange.fetchOptionChain({ underlying: 'BTC' });
    expect(rows.map((r) => [r.market.symbol, r.quote.bid, r.quote.markIv])).toEqual([[BTC, 3885, 0.36]]);
    expect(await exchange.fetchOptionExpiries('DOGE')).toEqual([1792742400000]);
  });

  it('returns open positions with greeks, leaving out flat rows', async () => {
    const { exchange } = create();
    const positions = await exchange.fetchPositions();
    expect(positions.map((p) => p.symbol)).toEqual([BTC]);
    expect(positions[0].option?.greeks?.delta).toBeCloseTo(-0.044767474, 12);
  });

  it('asks for at most the 1000 candles Binance returns, ending at the requested time', async () => {
    const { exchange } = create();
    const to = 1762779600000;
    (exchange.xhr.get as any).mockResolvedValueOnce({ data: [KLINE] });
    await exchange.fetchOHLCV({ symbol: BTC, interval: '1m', limit: 1500, to } as any);
    expect((exchange.xhr.get as any).mock.calls.at(-1)).toEqual([
      ENDPOINTS.KLINE,
      { params: { symbol: BTC, interval: '1m', startTime: to - 1000 * 60_000, endTime: to, limit: 1000 } },
    ]);
  });

  it('keeps an order open when the cancel fails for another reason', async () => {
    const { exchange, store } = create();
    const order = mapBinanceOptionOrder(OPEN_ORDER, 1);
    store.update({ orders: [order] });
    (exchange.xhr.delete as any).mockRejectedValueOnce(
      Object.assign(new Error('Timestamp outside recvWindow'), { code: -1021 }),
    );
    await expect(exchange.cancelOrders([order])).rejects.toThrow(/recvWindow/);
    expect(store.orders[0].status).not.toBe(OrderStatus.Canceled);
  });

  it("marks only the cancelled symbol's orders cancelled", async () => {
    const { exchange, store } = create();
    store.update({
      orders: [
        mapBinanceOptionOrder(OPEN_ORDER, 1),
        mapBinanceOptionOrder({ ...OPEN_ORDER, orderId: '2', symbol: DOGE }, 1),
      ],
    });
    await exchange.cancelSymbolOrders(BTC);
    expect(store.orders.map((o) => [o.symbol, o.status])).toEqual([
      [BTC, OrderStatus.Canceled],
      [DOGE, OrderStatus.Open],
    ]);
  });

  it('amends by cancelling, then placing what is left with the same side, reduce-only and time in force', async () => {
    const { exchange, store, calls } = create();
    const order = mapBinanceOptionOrder({ ...OPEN_ORDER, reduceOnly: true, timeInForce: 'IOC' }, 1);
    store.update({ orders: [order] });
    await exchange.updateOrder({ order, update: { price: 110 } });
    expect(calls).toEqual([
      ['delete', ENDPOINTS.ORDER, { symbol: BTC, orderId: '4611875134427365000' }],
      [
        'post',
        ENDPOINTS.ORDER,
        {
          symbol: BTC,
          side: 'BUY',
          type: 'LIMIT',
          quantity: '0.75',
          price: '110',
          timeInForce: 'IOC',
          reduceOnly: true,
          clientOrderId: expect.any(String),
        },
      ],
    ]);
  });

  it('asks for 500 candles back from the requested time when no limit is given', async () => {
    const { exchange } = create();
    const to = 1762779600000;
    (exchange.xhr.get as any).mockResolvedValueOnce({ data: [KLINE] });
    await exchange.fetchOHLCV({ symbol: BTC, interval: '1m', to } as any);
    expect((exchange.xhr.get as any).mock.calls.at(-1)).toEqual([
      ENDPOINTS.KLINE,
      { params: { symbol: BTC, interval: '1m', startTime: to - 500 * 60_000, endTime: to } },
    ]);
  });

  it('leaves out tickers for symbols it has no market for', async () => {
    const { exchange } = create();
    (exchange.xhr.get as any).mockImplementation((url: string) =>
      Promise.resolve({
        data: url === ENDPOINTS.TICKERS ? [BTC_TICKER, { ...BTC_TICKER, symbol: 'ETH-261225-3000-C' }] : [BTC_MARK],
      }),
    );
    expect((await exchange.fetchTickers()).map((t) => t.symbol)).toEqual([BTC]);
  });

  it('keeps the loaded markets when a refresh fails, and fails a first load', async () => {
    const { exchange, store } = create();
    (exchange.xhr.get as any).mockRejectedValue(new Error('down'));
    expect(await exchange.fetchMarkets()).toBe(store.markets);
    store.update({ markets: [] });
    await expect(exchange.fetchMarkets()).rejects.toThrow('down');
  });

  it('filters fills by order ID and history by status', async () => {
    const { exchange } = create();
    (exchange.xhr.get as any).mockResolvedValueOnce({
      data: [USER_TRADE, { ...USER_TRADE, tradeId: 240, orderId: '9' }],
    });
    expect((await exchange.fetchHistoricalFills({ symbol: BTC, orderId: '9' })).map((f) => f.id)).toEqual(['240']);
    (exchange.xhr.get as any).mockResolvedValueOnce({
      data: [
        { ...OPEN_ORDER, status: 'FILLED' },
        { ...OPEN_ORDER, orderId: '2', status: 'CANCELLED' },
      ],
    });
    const history = await exchange.fetchHistoricalOrders({ symbol: BTC, status: [OrderStatus.Closed] });
    expect(history.map((o) => o.id)).toEqual(['4611875134427365000']);
  });

  it('asks for order history and fills by symbol, and refuses without one', async () => {
    const { exchange, calls } = create();
    await expect(exchange.fetchHistoricalOrders({})).rejects.toThrow(/needs a symbol/);
    await expect(exchange.fetchHistoricalFills({})).rejects.toThrow(/need a symbol/);
    (exchange.xhr.get as any).mockResolvedValueOnce({ data: [USER_TRADE] });
    expect(await exchange.fetchHistoricalFills({ symbol: BTC, from: 1, to: 2 })).toHaveLength(1);
    expect((exchange.xhr.get as any).mock.calls.at(-1)).toEqual([
      ENDPOINTS.USER_TRADES,
      { params: { symbol: BTC, startTime: 1, endTime: 2, limit: undefined } },
    ]);
    expect(calls).toEqual([]);
  });
});

const capture = async (xhr: AxiosInstance, url: string, params?: Record<string, unknown>, body = '{}') => {
  let captured: InternalAxiosRequestConfig | undefined;
  const response = await xhr.request({
    method: 'get',
    url,
    params,
    adapter: (config: any) => {
      captured = config;
      return Promise.resolve({ data: body, status: 200, statusText: 'OK', headers: {}, config });
    },
  });
  return { config: captured!, data: response.data };
};

const stubExchange = {
  name: 'binanceoptions',
  grafanaEndpointsEvery: [],
  grafanaEndpointsPeriodic: [],
  emitter: { emit: () => undefined },
  emitAndLogError: () => undefined,
  recordAuthSuccess: () => undefined,
  recordAuthFailure: () => undefined,
} as any;

describe('binanceoptions API client', () => {
  it('signs a private request through payloadSigner with the Binance query-string scheme', async () => {
    const payloads: string[] = [];
    const xhr = createAPI(
      {
        key: 'api-key',
        secret: '',
        payloadSigner: (request: any) => {
          payloads.push(request.payload);
          return { signature: 'from-signer' };
        },
      } as any,
      stubExchange,
    );
    const { config } = await capture(xhr, ENDPOINTS.POSITIONS, { symbol: BTC });
    expect(config.baseURL).toBe('https://eapi.binance.com');
    expect(config.headers['X-MBX-APIKEY']).toBe('api-key');
    expect(payloads).toEqual([expect.stringMatching(new RegExp(`^symbol=${BTC}&timestamp=\\d+&recvWindow=5000$`))]);
    expect(config.params.signature).toBe('from-signer');
  });

  it('leaves public endpoints unsigned and tagged for hosted mode', async () => {
    const payloads: string[] = [];
    const xhr = createAPI(
      { key: 'k', secret: '', payloadSigner: (r: any) => (payloads.push(r.payload), { signature: 'x' }) } as any,
      stubExchange,
    );
    for (const url of [ENDPOINTS.MARKETS, ENDPOINTS.TICKERS, ENDPOINTS.MARK, ENDPOINTS.KLINE]) {
      const { config } = await capture(xhr, url);
      expect((config as any).safeCexSkipRemoteSigning).toBe(true);
    }
    expect(payloads).toEqual([]);
  });

  it('keeps order IDs above 2^53 exact', async () => {
    const xhr = createAPI({ key: 'k', secret: 's' } as any, stubExchange);
    const { data } = await capture(xhr, ENDPOINTS.ORDER, {}, '{"orderId":4611875134427365001}');
    expect(data.orderId).toBe('4611875134427365001');
  });

  it('rejects a 200 that carries a Binance error code, and reports it unless a cancel expects it', async () => {
    const reported: string[] = [];
    const xhr = createAPI({ key: 'k', secret: 's' } as any, {
      ...stubExchange,
      emitAndLogError: (m: string) => reported.push(m),
    });
    await expect(capture(xhr, ENDPOINTS.POSITIONS, {}, '{"code":-1121,"msg":"Invalid symbol."}')).rejects.toThrow(
      'Invalid symbol.',
    );
    await expect(capture(xhr, ENDPOINTS.ORDER, {}, '{"code":-2011,"msg":"Unknown order sent."}')).rejects.toMatchObject(
      { code: -2011 },
    );
    expect(reported).toEqual(['Invalid symbol.']);
  });

  it('points testnet at the futures testnet host that serves /eapi', async () => {
    const { config } = await capture(
      createAPI({ key: 'k', secret: 's', testnet: true } as any, stubExchange),
      ENDPOINTS.MARKETS,
    );
    expect(config.baseURL).toBe('https://testnet.binancefuture.com');
  });
});
