import { beforeEach, describe, expect, it, vi } from 'vitest';

import { KucoinSpotExchange } from './reference/kucoin/kucoinspot.exchange';
import { KUCOIN_SPOT_ENDPOINTS } from './reference/kucoin/kucoinspot.types';
import { DefaultStore } from '../store/store.base';
import {
  ExchangeOptions,
  MarginType,
  Market,
  OrderSide,
  OrderStatus,
  OrderTimeInForce,
  OrderType,
  PositionMode,
  PositionSide,
  Ticker,
} from '../types';

const options: ExchangeOptions = {
  key: 'test-key',
  secret: 'test-secret',
  passphrase: 'test-passphrase',
};

const btcMarket: Market = {
  id: 'BTC-USDT',
  symbol: 'BTC-USDT',
  prettySymbol: 'BTCUSDT',
  base: 'BTC',
  quote: 'USDT',
  active: true,
  precision: {
    amount: 0.000001,
    price: 0.01,
  },
  limits: {
    amount: {
      min: 0.00001,
      max: 10_000,
    },
    leverage: {
      min: 1,
      max: 1,
    },
  },
  contractSize: 1,
  isSpot: true,
  isInverse: false,
};

const btcTicker: Ticker = {
  id: 'BTC-USDT',
  symbol: 'BTC-USDT',
  bid: 50_000,
  ask: 50_010,
  last: 50_000,
  mark: 50_000,
  index: 50_000,
  percentage: 1,
  fundingRate: 0,
  fundingTime: 0,
  openInterest: 0,
  volume: 100,
  quoteVolume: 5_000_000,
};

const createExchange = () => {
  const store = new DefaultStore();
  const exchange = new KucoinSpotExchange('kucoinspot', options, store);

  return { exchange, store };
};

describe('KucoinSpotExchange', () => {
  let exchange: KucoinSpotExchange;
  let store: DefaultStore;

  beforeEach(() => {
    ({ exchange, store } = createExchange());
    vi.restoreAllMocks();
  });

  it('maps KuCoin spot symbols into active spot markets', async () => {
    exchange.xhr = {
      get: vi.fn().mockResolvedValue({
        data: {
          code: '200000',
          data: [
            {
              symbol: 'BTC-USDT',
              baseCurrency: 'BTC',
              quoteCurrency: 'USDT',
              enableTrading: true,
              baseIncrement: '0.000001',
              priceIncrement: '0.01',
              baseMinSize: '0.00001',
              baseMaxSize: '10000',
            },
          ],
        },
      }),
    } as any;

    const markets = await exchange.fetchMarkets();

    expect(exchange.xhr.get).toHaveBeenCalledWith('/api/v2/symbols');
    expect(markets).toHaveLength(1);
    expect(markets[0]).toMatchObject({
      id: 'BTC-USDT',
      symbol: 'BTC-USDT',
      prettySymbol: 'BTCUSDT',
      base: 'BTC',
      quote: 'USDT',
      active: true,
      precision: {
        amount: 0.000001,
        price: 0.01,
      },
      limits: {
        amount: {
          min: 0.00001,
          max: 10_000,
        },
        leverage: {
          min: 1,
          max: 1,
        },
      },
      isSpot: true,
      isInverse: false,
    });
  });

  it('maps KuCoin all-ticker rows into spot tickers', async () => {
    store.update({ markets: [btcMarket] });
    exchange.xhr = {
      get: vi.fn().mockResolvedValue({
        data: {
          code: '200000',
          data: {
            ticker: [
              {
                symbol: 'BTC-USDT',
                buy: '50000',
                sell: '50010',
                last: '50005',
                changeRate: '0.025',
                vol: '12.5',
                volValue: '625062.5',
              },
              {
                symbol: 'UNKNOWN-USDT',
                last: '1',
              },
            ],
          },
        },
      }),
    } as any;

    const tickers = await exchange.fetchTickers();

    expect(exchange.xhr.get).toHaveBeenCalledWith('/api/v1/market/allTickers');
    expect(tickers).toHaveLength(1);
    expect(tickers[0]).toMatchObject({
      id: 'BTC-USDT',
      symbol: 'BTC-USDT',
      bid: 50_000,
      ask: 50_010,
      last: 50_005,
      mark: 50_005,
      index: 50_005,
      percentage: 2.5,
      fundingRate: 0,
      fundingTime: 0,
      openInterest: 0,
      volume: 12.5,
      quoteVolume: 625_062.5,
    });
  });

  it('applies /market/ticker:all updates by resolving the symbol from json.subject', () => {
    // Clone: updateTicker mutates the ticker in place, so don't taint the shared fixture.
    store.update({ tickers: [{ ...btcTicker }] });
    const updateTicker = vi.spyOn(store, 'updateTicker');

    // The all-symbols feed puts the symbol in `subject`; `data` has none and the
    // topic is literally ":all". Without subject the update is dropped.
    exchange.publicWebsocket.handleTickerSnapshot({
      topic: '/market/ticker:all',
      subject: 'BTC-USDT',
      data: { bestBid: '51000', bestAsk: '51001', price: '51000.5' },
    });

    expect(updateTicker).toHaveBeenCalledWith(
      expect.objectContaining({ symbol: 'BTC-USDT' }),
      expect.objectContaining({ bid: 51_000, ask: 51_001, last: 51_000.5 }),
    );
  });

  it('aggregates trade account balances with ticker-derived USD values', async () => {
    store.update({
      markets: [btcMarket],
      tickers: [btcTicker],
    });
    exchange.xhr = {
      get: vi.fn().mockResolvedValue({
        data: {
          code: '200000',
          data: [
            {
              currency: 'BTC',
              available: '0.08',
              holds: '0.02',
              balance: '0.1',
            },
            {
              currency: 'USDT',
              available: '100',
              holds: '5',
              balance: '105',
            },
          ],
        },
      }),
    } as any;

    const balance = await exchange.fetchBalance();

    expect(exchange.xhr.get).toHaveBeenCalledWith('/api/v1/accounts', {
      params: { type: 'trade' },
    });
    expect(balance.free).toBe(4100);
    expect(balance.used).toBe(1005);
    expect(balance.total).toBe(5105);
    expect(balance.currencies?.BTC).toMatchObject({
      free: 0.08,
      used: 0.02,
      total: 0.1,
      upnl: 0,
    });
    expect(balance.coins?.BTC).toMatchObject({
      base: 'BTC',
      amount: 0.1,
      notional: 5000,
    });
  });

  it('maps KuCoin HF active orders into internal open orders', async () => {
    exchange.xhr = {
      get: vi.fn().mockResolvedValue({
        data: {
          code: '200000',
          data: {
            currentPage: 1,
            pageSize: 50,
            totalNum: 1,
            totalPage: 1,
            items: [
              {
                id: 'order-1',
                symbol: 'BTC-USDT',
                type: 'limit',
                side: 'buy',
                price: '50000',
                size: '0.2',
                dealSize: '0.05',
                remainSize: '0.15',
                status: 'active',
                postOnly: true,
                lastUpdatedAt: '1710000000000',
              },
            ],
          },
        },
      }),
    } as any;

    const orders = await exchange.fetchOrders(['BTC-USDT']);

    expect(exchange.xhr.get).toHaveBeenCalledWith('/api/v1/hf/orders/active/page', {
      params: { symbol: 'BTC-USDT', pageNum: 1, pageSize: 50 },
    });
    expect(orders).toHaveLength(1);
    expect(orders[0]).toMatchObject({
      id: 'order-1',
      status: OrderStatus.Open,
      symbol: 'BTC-USDT',
      type: OrderType.Limit,
      side: OrderSide.Buy,
      price: 50_000,
      amount: 0.2,
      filled: 0.05,
      remaining: 0.15,
      reduceOnly: false,
      timeInForce: OrderTimeInForce.PostOnly,
      lastUpdate: 1_710_000_000_000,
    });
  });

  it('paginates KuCoin HF active orders until the last open-order page', async () => {
    exchange.xhr = {
      get: vi
        .fn()
        .mockResolvedValueOnce({
          data: {
            code: '200000',
            data: {
              currentPage: 1,
              pageSize: 50,
              totalNum: 2,
              totalPage: 2,
              items: [
                {
                  id: 'order-1',
                  symbol: 'BTC-USDT',
                  type: 'limit',
                  side: 'buy',
                  price: '50000',
                  size: '0.2',
                  dealSize: '0',
                  remainSize: '0.2',
                  active: true,
                },
              ],
            },
          },
        })
        .mockResolvedValueOnce({
          data: {
            code: '200000',
            data: {
              currentPage: 2,
              pageSize: 50,
              totalNum: 2,
              totalPage: 2,
              items: [
                {
                  id: 'order-2',
                  symbol: 'BTC-USDT',
                  type: 'limit',
                  side: 'sell',
                  price: '60000',
                  size: '0.1',
                  dealSize: '0',
                  remainSize: '0.1',
                  active: true,
                },
              ],
            },
          },
        }),
    } as any;

    // Explicit symbol — the HF open-orders endpoint always needs one; the
    // no-symbol active-symbols discovery is covered by its own test.
    const orders = await exchange.fetchOrders(['BTC-USDT']);

    expect(exchange.xhr.get).toHaveBeenNthCalledWith(1, '/api/v1/hf/orders/active/page', {
      params: { symbol: 'BTC-USDT', pageNum: 1, pageSize: 50 },
    });
    expect(exchange.xhr.get).toHaveBeenNthCalledWith(2, '/api/v1/hf/orders/active/page', {
      params: { symbol: 'BTC-USDT', pageNum: 2, pageSize: 50 },
    });
    expect(orders.map((order) => order.id)).toEqual(['order-1', 'order-2']);
  });

  it('formats KuCoin HF limit order placement payloads', async () => {
    store.update({ markets: [btcMarket] });
    exchange.unlimitedXHR = {
      post: vi.fn().mockResolvedValue({
        data: {
          code: '200000',
          data: {
            orderId: 'created-order-1',
          },
        },
      }),
    } as any;

    const orderIds = await exchange.placeOrder({
      symbol: 'BTC-USDT',
      type: OrderType.Limit,
      side: OrderSide.Buy,
      amount: 0.1,
      price: 50_000.01,
      timeInForce: OrderTimeInForce.PostOnly,
    });

    expect(orderIds).toEqual(['created-order-1']);
    expect(exchange.unlimitedXHR.post).toHaveBeenCalledTimes(1);

    const [path, payload] = (exchange.unlimitedXHR.post as any).mock.calls[0];
    expect(path).toBe('/api/v1/hf/orders');
    expect(payload).toMatchObject({
      symbol: 'BTC-USDT',
      type: 'limit',
      side: 'buy',
      price: '50000.01',
      size: '0.1',
      timeInForce: 'GTC',
      postOnly: true,
    });
    expect(payload.clientOid).toEqual(expect.any(String));
  });

  it('branches spot order WS events on the event type, not the collapsed status', () => {
    store.update({ markets: [btcMarket] });
    // any background refetch is a harmless mock
    exchange.xhr = { get: vi.fn().mockResolvedValue({ data: { code: '200000', data: { symbols: [] } } }) } as any;
    const ws = exchange.privateWebsocket;
    const updateStatus = vi.spyOn(store, 'updateOrderStatus');
    const addOrUpdate = vi.spyOn(store, 'addOrUpdateOrder');
    const fills: any[] = [];
    exchange.emitter.on('fill', (f: any) => fills.push(f));

    const base = {
      orderId: 'o1',
      symbol: 'BTC-USDT',
      side: 'buy',
      orderType: 'limit',
      price: '50000',
      size: '0.1',
      filledSize: '0',
    };

    ws.handleOrderTopic({ data: { ...base, type: 'open', status: 'open' } });
    expect(addOrUpdate).toHaveBeenCalledWith(expect.objectContaining({ id: 'o1', status: OrderStatus.Open }));

    // Cancel arrives as type:canceled, status:done — MUST be Canceled, not Closed.
    ws.handleOrderTopic({ data: { ...base, type: 'canceled', status: 'done' } });
    expect(updateStatus).toHaveBeenCalledWith(expect.objectContaining({ id: 'o1' }), OrderStatus.Canceled);

    // Match → one fill in base units.
    ws.handleOrderTopic({ data: { ...base, type: 'match', status: 'match', matchSize: '0.04', matchPrice: '50001' } });
    expect(fills).toContainEqual(expect.objectContaining({ id: 'o1', amount: 0.04, price: 50001 }));

    // Terminal filled arrives as type:filled, status:done → Closed.
    ws.handleOrderTopic({ data: { ...base, type: 'filled', status: 'done', filledSize: '0.1' } });
    expect(updateStatus).toHaveBeenCalledWith(expect.objectContaining({ id: 'o1' }), OrderStatus.Closed);
  });

  it('emits before throwing on unsupported spot orders (TP/SL)', async () => {
    store.update({ markets: [btcMarket] });
    const emitSpy = vi.spyOn(exchange, 'emitAndLogError');
    await expect(
      exchange.placeOrder({
        symbol: 'BTC-USDT',
        type: OrderType.Limit,
        side: OrderSide.Buy,
        amount: 0.1,
        price: 50_000,
        stopLoss: 40_000,
      } as any),
    ).rejects.toThrow(/TP\/SL/);
    expect(emitSpy).toHaveBeenCalled();
  });

  it('fetches all open orders by first discovering active-order symbols (HF requires a symbol)', async () => {
    const get = vi.fn(async (path: string) => {
      if (path === KUCOIN_SPOT_ENDPOINTS.ACTIVE_ORDER_SYMBOLS) {
        return { data: { code: '200000', data: { symbols: ['BTC-USDT'] } } };
      }
      return { data: { code: '200000', data: { items: [], totalPage: 1 } } };
    });
    exchange.xhr = { get } as any;

    await exchange.fetchOrders();

    expect(get).toHaveBeenCalledWith(KUCOIN_SPOT_ENDPOINTS.ACTIVE_ORDER_SYMBOLS);
    expect(get).toHaveBeenCalledWith(
      KUCOIN_SPOT_ENDPOINTS.OPEN_ORDERS,
      expect.objectContaining({ params: expect.objectContaining({ symbol: 'BTC-USDT' }) }),
    );
  });

  it('maps KuCoin HF fills into execution history', async () => {
    exchange.xhr = {
      get: vi.fn().mockResolvedValue({
        data: {
          code: '200000',
          data: {
            items: [
              {
                createdAt: '1710000000000',
                symbol: 'BTC-USDT',
                orderId: 'order-1',
                price: '50000',
                size: '0.02',
                side: 'sell',
                fee: '0.5',
                feeCurrency: 'USDT',
              },
            ],
          },
        },
      }),
    } as any;

    const executions = await exchange.fetchHistoricalFills({
      symbol: 'BTC-USDT',
      orderId: 'order-1',
      limit: 250,
      from: 1_700_000_000_000,
      to: 1_710_000_000_000,
    });

    expect(exchange.xhr.get).toHaveBeenCalledWith('/api/v1/hf/fills', {
      params: {
        symbol: 'BTC-USDT',
        orderId: 'order-1',
        limit: 100,
        startAt: 1_700_000_000_000,
        endAt: 1_710_000_000_000,
      },
    });
    expect(executions).toEqual([
      {
        timestamp: 1_710_000_000_000,
        symbol: 'BTC-USDT',
        orderId: 'order-1',
        price: 50_000,
        qty: 0.02,
        side: OrderSide.Sell,
        commission: 0.5,
        commissionAsset: 'USDT',
      },
    ]);
  });

  it('does not call the symbol-required HF fills endpoint without a symbol', async () => {
    exchange.xhr = { get: vi.fn() } as any;

    await expect(exchange.fetchHistoricalFills({ from: 1_700_000_000_000 })).resolves.toEqual([]);
    expect(exchange.xhr.get).not.toHaveBeenCalled();
  });

  it('generates zero-size one-way positions for active spot markets', async () => {
    store.update({
      markets: [
        btcMarket,
        {
          ...btcMarket,
          id: 'ETH-USDT',
          symbol: 'ETH-USDT',
          base: 'ETH',
          active: false,
        },
      ],
    });

    const positions = await exchange.fetchPositions();

    expect(positions).toHaveLength(1);
    expect(positions[0]).toMatchObject({
      id: 'BTC-USDT:long',
      symbol: 'BTC-USDT',
      side: PositionSide.Long,
      entryPrice: 0,
      notional: 0,
      leverage: 1,
      unrealizedPnl: 0,
      contracts: 0,
      liquidationPrice: 0,
      marginType: MarginType.Isolated,
      positionMode: PositionMode.OneWay,
      info: { spot: true },
    });
  });
});
