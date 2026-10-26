import { beforeEach, describe, expect, it, vi } from 'vitest';

import { AsterDEXExchange } from './reference/asterdex/asterdex.exchange';
import { OrderSide, OrderStatus, OrderType, PositionMode, PositionSide } from '../types';

const createAPIInstancesMock = vi.fn();

vi.mock('../exchanges/asterdex/asterdex.api', () => ({
  createAPIInstances: (...args: any[]) => createAPIInstancesMock(...args),
}));

vi.mock('./reference/asterdex/asterdex.ws-public', () => ({
  AsterDEXPublicWebsocket: class {
    connectAndSubscribe = vi.fn();
    dispose = vi.fn();
  },
}));

vi.mock('./reference/asterdex/asterdex.ws-private', () => ({
  AsterDEXPrivateWebsocket: class {
    connectAndSubscribe = vi.fn();
    dispose = vi.fn();
  },
}));

const createMockStore = () => {
  const options = {
    isHedged: false,
  };
  const store = {
    subscribe: vi.fn(() => vi.fn()),
    update: vi.fn(),
    reset: vi.fn(),
    removeOrders: vi.fn(),
    updateSocketHealth: vi.fn(),
    setOrdersForSymbol: vi.fn(),
    positions: [],
    orders: [],
    markets: [],
    tickers: [],
    options,
    setSetting: vi.fn((key: 'isHedged', value: boolean) => {
      options[key] = value;
    }),
    loaded: {
      balance: false,
      orders: false,
      markets: false,
      tickers: false,
      positions: false,
    },
    fetchedLeverage: {},
    get marketsBySymbol() {
      return Object.fromEntries(store.markets.map((market: any) => [market.symbol, market]));
    },
  } as any;

  return store;
};

const makeRawMarket = () => ({
  symbol: 'BTCUSDT',
  baseAsset: 'BTC',
  quoteAsset: 'USDT',
  contractType: 'PERPETUAL',
  status: 'TRADING',
  maintMarginPercent: '2.5',
  requiredMarginPercent: '5',
  pricePrecision: 2,
  quantityPrecision: 3,
  baseAssetPrecision: 8,
  quotePrecision: 8,
  triggerProtect: '0.15',
  filters: [
    {
      filterType: 'PRICE_FILTER',
      minPrice: '0.10',
      maxPrice: '1000000',
      tickSize: '0.10',
    },
    {
      filterType: 'LOT_SIZE',
      minQty: '0.001',
      maxQty: '1000',
      stepSize: '0.001',
    },
  ],
  orderTypes: ['LIMIT', 'MARKET'],
  timeInForce: ['GTC'],
  liquidationFee: '0.01',
  marketTakeBound: '0.05',
});

describe('AsterDEX leverage brackets', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('fetches leverage brackets with signed primary API and applies first-tier leverage to markets', async () => {
    const publicGet = vi.fn().mockResolvedValue({
      data: { symbols: [makeRawMarket()] },
    });
    const primaryGet = vi.fn().mockResolvedValue({
      data: [
        {
          symbol: 'BTCUSDT',
          brackets: [
            {
              bracket: 1,
              initialLeverage: 75,
              notionalCap: 10000,
              notionalFloor: 0,
              maintMarginRatio: 0.0065,
            },
            {
              bracket: 2,
              initialLeverage: 50,
              notionalCap: 50000,
              notionalFloor: 10000,
              maintMarginRatio: 0.01,
            },
          ],
        },
      ],
    });

    createAPIInstancesMock.mockReturnValue({
      primary: { get: primaryGet },
      secondary: {},
      unlimited: {},
      public: { get: publicGet },
    });

    const exchange = new AsterDEXExchange('asterdex' as any, { key: 'k', secret: 's' } as any, createMockStore());
    vi.spyOn(exchange, 'log').mockImplementation(() => undefined);

    const markets = await exchange.fetchMarkets();

    expect(publicGet).toHaveBeenCalledWith('/fapi/v1/exchangeInfo');
    expect(primaryGet).toHaveBeenCalledWith('/fapi/v1/leverageBracket');
    expect(publicGet).not.toHaveBeenCalledWith('/fapi/v1/leverageBracket');
    expect(markets[0].limits?.leverage?.max).toBe(75);
  });

  it('handles object leverageBracket responses and still maps max leverage', async () => {
    const publicGet = vi.fn().mockResolvedValue({
      data: { symbols: [makeRawMarket()] },
    });
    const primaryGet = vi.fn().mockResolvedValue({
      data: {
        symbol: 'BTCUSDT',
        brackets: [
          {
            bracket: 1,
            initialLeverage: 66,
            notionalCap: 10000,
            notionalFloor: 0,
            maintMarginRatio: 0.0065,
          },
        ],
      },
    });

    createAPIInstancesMock.mockReturnValue({
      primary: { get: primaryGet },
      secondary: {},
      unlimited: {},
      public: { get: publicGet },
    });

    const exchange = new AsterDEXExchange('asterdex' as any, { key: 'k', secret: 's' } as any, createMockStore());
    vi.spyOn(exchange, 'log').mockImplementation(() => undefined);

    const markets = await exchange.fetchMarkets();
    expect(markets[0].limits?.leverage?.max).toBe(66);
  });

  it('falls back to default leverage when leverage bracket request fails', async () => {
    const publicGet = vi.fn().mockResolvedValue({
      data: { symbols: [makeRawMarket()] },
    });
    const primaryGet = vi.fn().mockRejectedValue(new Error('Auth error'));

    createAPIInstancesMock.mockReturnValue({
      primary: { get: primaryGet },
      secondary: {},
      unlimited: {},
      public: { get: publicGet },
    });

    const exchange = new AsterDEXExchange('asterdex' as any, { key: 'k', secret: 's' } as any, createMockStore());
    vi.spyOn(exchange, 'log').mockImplementation(() => undefined);

    const markets = await exchange.fetchMarkets();
    expect(markets[0].limits?.leverage?.max).toBe(10);
  });

  it('uses positionRisk leverage for fake zero-size positions', async () => {
    const primaryGet = vi.fn().mockImplementation((path: string) => {
      if (path === '/fapi/v1/positionRisk') {
        return Promise.resolve({
          data: [
            {
              symbol: 'BTCUSDT',
              positionAmt: '0',
              leverage: '200',
              entryPrice: '0',
              markPrice: '0',
              unRealizedProfit: '0',
              liquidationPrice: '0',
              marginType: 'cross',
              isolatedMargin: '0',
              isAutoAddMargin: 'false',
              positionSide: 'BOTH',
              notional: '0',
              updateTime: 0,
            },
          ],
        });
      }
      return Promise.resolve({ data: [] });
    });

    createAPIInstancesMock.mockReturnValue({
      primary: { get: primaryGet },
      secondary: {},
      unlimited: {},
      public: {},
    });

    const store = createMockStore();
    store.markets = [
      {
        symbol: 'BTCUSDT',
        active: true,
        isSpot: false,
      },
    ];

    const exchange = new AsterDEXExchange('asterdex' as any, { key: 'k', secret: 's' } as any, store);
    vi.spyOn(exchange, 'log').mockImplementation(() => undefined);

    const positions = await exchange.fetchPositions();
    const longPosition = positions.find((position) => position.symbol === 'BTCUSDT' && position.side === 'long');
    const shortPosition = positions.find((position) => position.symbol === 'BTCUSDT' && position.side === 'short');

    expect(positions).toHaveLength(1);
    expect(longPosition?.leverage).toBe(200);
    expect(longPosition?.positionMode).toBe(PositionMode.OneWay);
    expect(shortPosition).toBeUndefined();
  });

  it('reads current hedge mode from Aster position mode endpoint', async () => {
    const primaryGet = vi.fn().mockResolvedValue({ data: { dualSidePosition: true } });

    createAPIInstancesMock.mockReturnValue({
      primary: { get: primaryGet },
      secondary: {},
      unlimited: {},
      public: {},
    });

    const store = createMockStore();
    const exchange = new AsterDEXExchange('asterdex' as any, { key: 'k', secret: 's' } as any, store);

    await expect(exchange.fetchPositionMode()).resolves.toBe(true);

    expect(primaryGet).toHaveBeenCalledWith('/fapi/v1/positionSide/dual');
  });

  it('treats unknown-order cancels as already complete', async () => {
    const secondaryDelete = vi.fn().mockRejectedValue(new Error('AsterDEX API Error: Unknown order sent. (code: -2011)'));

    createAPIInstancesMock.mockReturnValue({
      primary: {},
      secondary: { delete: secondaryDelete },
      unlimited: {},
      public: {},
    });

    const exchange = new AsterDEXExchange('asterdex' as any, { key: 'k', secret: 's' } as any, createMockStore());
    vi.spyOn(exchange, 'log').mockImplementation(() => undefined);

    await expect(
      exchange.cancelOrders([
        {
          id: '12345',
          symbol: 'BTCUSDT',
          type: OrderType.StopLoss,
          side: OrderSide.Sell,
          price: 50000,
          amount: 1,
          filled: 0,
          remaining: 1,
          status: OrderStatus.Open,
          reduceOnly: true,
          lastUpdate: 0,
        },
      ]),
    ).resolves.toBeUndefined();

    expect(secondaryDelete).toHaveBeenCalledWith(
      '/fapi/v1/order',
      expect.objectContaining({
        __suppressAsterUnknownOrderError: true,
        params: {
          symbol: 'BTCUSDT',
          orderId: '12345',
        },
      }),
    );
  });

  it('uses the fetched account mode for fake zero-size positions', async () => {
    const primaryGet = vi.fn().mockResolvedValue({
      data: [
        {
          symbol: 'BTCUSDT',
          positionAmt: '0',
          leverage: '50',
          entryPrice: '0',
          markPrice: '0',
          unRealizedProfit: '0',
          liquidationPrice: '0',
          marginType: 'cross',
          isolatedMargin: '0',
          isAutoAddMargin: 'false',
          positionSide: 'BOTH',
          notional: '0',
          updateTime: 0,
        },
      ],
    });

    createAPIInstancesMock.mockReturnValue({
      primary: { get: primaryGet },
      secondary: {},
      unlimited: {},
      public: {},
    });

    const store = createMockStore();
    store.options.isHedged = true;
    store.markets = [
      {
        symbol: 'BTCUSDT',
        active: true,
        isSpot: false,
      },
    ];

    const exchange = new AsterDEXExchange('asterdex' as any, { key: 'k', secret: 's' } as any, store);
    vi.spyOn(exchange, 'log').mockImplementation(() => undefined);

    const positions = await exchange.fetchPositions();

    expect(positions).toHaveLength(2);
    expect(positions.every((position) => position.positionMode === PositionMode.Hedged)).toBe(true);
  });

  it('marks real LONG and SHORT positionRisk rows as hedged positions', async () => {
    const primaryGet = vi.fn().mockResolvedValue({
      data: [
        {
          symbol: 'BTCUSDT',
          positionAmt: '1.5',
          leverage: '20',
          entryPrice: '50000',
          markPrice: '50100',
          unRealizedProfit: '150',
          liquidationPrice: '40000',
          marginType: 'cross',
          isolatedMargin: '0',
          isAutoAddMargin: 'false',
          positionSide: 'LONG',
          notional: '75150',
          updateTime: 0,
        },
      ],
    });

    createAPIInstancesMock.mockReturnValue({
      primary: { get: primaryGet },
      secondary: {},
      unlimited: {},
      public: {},
    });

    const store = createMockStore();
    const exchange = new AsterDEXExchange('asterdex' as any, { key: 'k', secret: 's' } as any, store);
    vi.spyOn(exchange, 'log').mockImplementation(() => undefined);

    const positions = await exchange.fetchPositions();

    expect(positions[0]).toMatchObject({
      symbol: 'BTCUSDT',
      side: PositionSide.Long,
      positionMode: PositionMode.Hedged,
    });
  });

  it('sends positionSide and omits reduceOnly when placing reduce-only orders in hedge mode', async () => {
    const secondaryPost = vi.fn().mockResolvedValue({ data: { orderId: 12345 } });

    createAPIInstancesMock.mockReturnValue({
      primary: {},
      secondary: { post: secondaryPost },
      unlimited: {},
      public: {},
    });

    const store = createMockStore();
    store.options.isHedged = true;
    store.markets = [
      {
        symbol: 'BTCUSDT',
        precision: {
          amount: 0.001,
          price: 0.1,
        },
        limits: {
          amount: {
            max: 100,
          },
        },
        info: {
          filters: [
            {
              filterType: 'PRICE_FILTER',
              tickSize: '0.1',
            },
          ],
        },
      },
    ];

    const exchange = new AsterDEXExchange('asterdex' as any, { key: 'k', secret: 's' } as any, store);
    vi.spyOn(exchange, 'log').mockImplementation(() => undefined);

    await exchange.placeOrder({
      symbol: 'BTCUSDT',
      type: OrderType.Limit,
      side: OrderSide.Sell,
      amount: 1,
      price: 50000,
      reduceOnly: true,
    });

    expect(secondaryPost).toHaveBeenCalledWith(
      '/fapi/v1/order',
      expect.objectContaining({
        symbol: 'BTCUSDT',
        side: 'SELL',
        type: 'LIMIT',
        positionSide: 'LONG',
      }),
    );
    expect(secondaryPost.mock.calls[0][1]).not.toHaveProperty('reduceOnly');
  });
});
