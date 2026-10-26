import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { AxiosError } from 'axios';

import {
  BreakoutpropExchange,
  parseDxSessionTimeoutMs,
} from '../reference/breakoutprop/breakoutprop.exchange';
import { ENDPOINTS } from '../reference/breakoutprop/breakoutprop.types';
import { DefaultStore } from '../../store/store.base';
import { ExchangeOptions, OrderSide, OrderType } from '../../types';

const symbol = 'ETHUSD';

afterEach(() => vi.restoreAllMocks());

describe('Breakout Prop official DX REST order contract', () => {
  let exchange: BreakoutpropExchange;
  let post: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    const store = new DefaultStore();
    store.update({
      markets: [
        {
          id: symbol,
          symbol,
          base: 'ETH',
          quote: 'USD',
          active: true,
          precision: { amount: 0.001, price: 0.01 },
          limits: { amount: { min: 0.01, max: 100 }, leverage: { min: 1, max: 5 } },
        },
      ],
    });
    exchange = new BreakoutpropExchange(
      'breakoutprop',
      { key: 'user', secret: 'password' } as ExchangeOptions,
      store,
    );
    vi.spyOn(exchange, 'getSessionToken').mockResolvedValue('session');
    vi.spyOn(exchange, 'getAccountCode').mockResolvedValue('default:123');
    post = vi.fn().mockResolvedValue({ data: { orderId: 42 } });
    exchange.xhr = { post } as any;
  });

  it('sends a step-adjusted reduce-only limit as CLOSE', async () => {
    await exchange.placeOrder({
      symbol,
      type: OrderType.Limit,
      side: OrderSide.Sell,
      amount: 0.0126,
      price: 3656.019,
      reduceOnly: true,
    });

    expect(post).toHaveBeenCalledWith(
      ENDPOINTS.PLACE_ORDER,
      expect.objectContaining({
        type: 'LIMIT',
        quantity: 0.013,
        side: 'SELL',
        limitPrice: 3656.02,
        positionEffect: 'CLOSE',
        tif: 'GTC',
      }),
      { params: { accountCode: 'default:123' } },
    );
    expect(post.mock.calls[0][1]).not.toHaveProperty('stopPrice');
  });

  it('omits price fields for MARKET', async () => {
    await exchange.placeOrder({ symbol, type: OrderType.Market, side: OrderSide.Buy, amount: 0.01 });

    expect(post.mock.calls[0][1]).toMatchObject({ type: 'MARKET', positionEffect: 'OPEN' });
    expect(post.mock.calls[0][1]).not.toHaveProperty('limitPrice');
    expect(post.mock.calls[0][1]).not.toHaveProperty('stopPrice');
  });

  it.each([OrderType.StopLoss, OrderType.TakeProfit])('maps %s to STOP with stopPrice', async (type) => {
    await exchange.placeOrder({
      symbol,
      type,
      side: OrderSide.Sell,
      amount: 0.01,
      triggerPrice: 3500.019,
      reduceOnly: true,
    });

    expect(post.mock.calls[0][1]).toMatchObject({ type: 'STOP', stopPrice: 3500.02, positionEffect: 'CLOSE' });
    expect(post.mock.calls[0][1]).not.toHaveProperty('limitPrice');
  });

  it('fails closed for unsupported order types and missing required prices', async () => {
    await expect(
      exchange.placeOrder({ symbol, type: OrderType.TrailingStopLoss, side: OrderSide.Sell, amount: 0.01 }),
    ).rejects.toThrow('does not support trailing_stop_market orders');
    await expect(
      exchange.placeOrder({ symbol, type: OrderType.Limit, side: OrderSide.Buy, amount: 0.01 }),
    ).rejects.toThrow('limit orders require a price');
    await expect(
      exchange.placeOrder({ symbol, type: OrderType.StopLoss, side: OrderSide.Sell, amount: 0.01 }),
    ).rejects.toThrow('stop orders require a trigger price');
    expect(post).not.toHaveBeenCalled();
  });

  it('maps market precision to the API step fields while retaining the minimum', async () => {
    vi.spyOn(exchange, 'fetchCombinedInstruments').mockResolvedValue([
      {
        instrument: {
          type: 'FOREX',
          symbol,
          version: 1,
          description: 'ETH',
          priceIncrement: 0.01,
          pipSize: 0.01,
          currency: 'USD',
          lotSize: 1,
          multiplier: 1,
          firstCurrency: 'ETH',
        },
        instrumentDetails: {
          account: 'default:123',
          type: 'FOREX',
          symbol,
          tradingStatus: 'FULL',
          minOrderSize: 0.01,
          minOrderSizeIncrement: 0.001,
          maxOrderSize: { value: 100, limitType: 'LOTS' },
          marginRate: { rateType: 'FLAT', tiers: [{ endVolume: 'Infinity', value: 0.2 }] },
        },
      },
    ]);

    await expect(exchange.fetchMarkets()).resolves.toEqual([
      expect.objectContaining({
        precision: { amount: 0.001, price: 0.01 },
        limits: expect.objectContaining({ amount: { min: 0.01, max: 100 } }),
      }),
    ]);
  });

  it('awaits cancellation before placing an update replacement', async () => {
    let releaseCancel!: () => void;
    const cancelPending = new Promise<void>((resolve) => {
      releaseCancel = resolve;
    });
    vi.spyOn(exchange, 'cancelOrder').mockReturnValue(cancelPending.then(() => 'old-order'));
    const placeOrder = vi.spyOn(exchange, 'placeOrder').mockResolvedValue(['43']);
    const update = exchange.updateOrder({
      order: {
        id: 'old-order',
        status: 'open',
        symbol,
        type: OrderType.Limit,
        side: OrderSide.Buy,
        price: 3656,
        amount: 0.01,
        filled: 0,
        remaining: 0.01,
        reduceOnly: false,
        lastUpdate: 1,
      } as any,
      update: { price: 3657 },
    });

    await Promise.resolve();
    expect(placeOrder).not.toHaveBeenCalled();
    releaseCancel();
    await expect(update).resolves.toEqual(['43']);
    expect(placeOrder).toHaveBeenCalledWith(expect.objectContaining({ price: 3657 }));
  });

  it('propagates cancellation failure and does not place a replacement', async () => {
    vi.spyOn(exchange, 'cancelOrder').mockRejectedValue(new Error('cancel rejected'));
    const placeOrder = vi.spyOn(exchange, 'placeOrder');

    await expect(
      exchange.updateOrder({
        order: {
          id: 'old-order',
          status: 'open',
          symbol,
          type: OrderType.Limit,
          side: OrderSide.Buy,
          price: 3656,
          amount: 0.01,
          filled: 0,
          remaining: 0.01,
          reduceOnly: false,
          lastUpdate: 1,
        } as any,
        update: { price: 3657 },
      }),
    ).rejects.toThrow('cancel rejected');
    expect(placeOrder).not.toHaveBeenCalled();
  });

  it('normalizes REST candles to seconds and live candles to interval starts', async () => {
    post.mockResolvedValue({
      data: {
        events: [
          {
            symbol,
            type: 'Candle',
            candleType: '5m',
            open: 3500,
            high: 3510,
            low: 3490,
            close: 3505,
            volume: 12,
            time: '2026-07-15T10:02:03.456Z',
          },
        ],
      },
    });
    const expectedRestTimestamp = new Date('2026-07-15T10:02:03.456Z').getTime() / 1000;
    await expect(exchange.fetchOHLCV({ symbol, interval: '5m' })).resolves.toEqual([
      expect.objectContaining({ timestamp: expectedRestTimestamp }),
    ]);

    vi.spyOn(Date, 'now').mockReturnValue(new Date('2026-07-15T10:02:03.456Z').getTime());
    const listener = vi.fn();
    exchange.listenOHLCV({ symbol, interval: '5m' }, listener);
    exchange.publicWebsocket.messageHandlers[`poll_kline.${symbol}`][0]({ last: 3505 });

    const expectedLiveTimestamp = Math.floor(expectedRestTimestamp / 300) * 300;
    expect(listener).toHaveBeenCalledWith(expect.objectContaining({ timestamp: expectedLiveTimestamp }));
  });

  it('publishes one synthetic quote level on each side', () => {
    const listener = vi.fn();
    exchange.listenOrderBook(symbol, listener);
    exchange.publicWebsocket.messageHandlers[`poll_book.${symbol}`][0]({ bid: 3500, ask: 3501 });
    expect(listener).toHaveBeenCalledWith({ bids: [{ price: 3500, amount: 1, total: 1 }], asks: [{ price: 3501, amount: 1, total: 1 }] });
  });

  it('keeps public-only quote listeners polling without portfolio reads', async () => {
    vi.spyOn(exchange, 'isPublicDataOnly', 'get').mockReturnValue(true);
    const portfolio = vi.spyOn(exchange, 'fetchPortfolioRaw');
    vi.spyOn(exchange, 'fetchTickers').mockResolvedValue([{ id: symbol, symbol, bid: 3500, ask: 3501, last: 3500 } as any]);
    const listener = vi.fn();
    exchange.listenOrderBook(symbol, listener);
    vi.spyOn(exchange, 'getTickInterval' as any).mockReturnValue(Number.POSITIVE_INFINITY);
    await exchange.tick();
    exchange.dispose();
    expect(portfolio).not.toHaveBeenCalled();
    expect(listener).toHaveBeenCalledOnce();
  });

  it('uses the portfolio snapshot for orders and maps close/update fields', async () => {
    const portfolio = {
      portfolios: [
        {
          account: 'default:123',
          version: 1,
          balances: [],
          positions: [],
          orders: [
            {
              account: 'default:123',
              version: 2,
              orderId: 42,
              orderCode: 'client-order',
              clientOrderId: 'client-order',
              actionCode: 'action',
              legCount: 1,
              type: 'LIMIT',
              instrument: symbol,
              status: 'WORKING',
              finalStatus: false,
              legs: [
                {
                  instrument: symbol,
                  positionEffect: 'CLOSE',
                  positionCode: 'position-1',
                  side: 'SELL',
                  price: 3500,
                  legRatio: 1,
                  quantity: 0.01,
                  filledQuantity: 0.002,
                  remainingQuantity: 0.008,
                  averagePrice: 3500,
                },
              ],
              side: 'SELL',
              tif: 'GTC',
              marginRate: 0.2,
              issueTime: '2026-07-15T10:02:03.456Z',
              transactionTime: '2026-07-15T10:02:04.567Z',
              executions: [],
              cashTransactions: [],
            },
          ],
        },
      ],
    };
    vi.spyOn(exchange, 'fetchPortfolioRaw').mockResolvedValue(portfolio);

    await expect(exchange.fetchOrders()).resolves.toEqual([
      expect.objectContaining({
        id: 'client-order',
        reduceOnly: true,
        lastUpdate: new Date('2026-07-15T10:02:04.567Z').getTime(),
      }),
    ]);
  });

  it('derives used margin and unrealized PnL from portfolio positions', () => {
    exchange.store.update({
      tickers: [
        {
          id: symbol,
          symbol,
          bid: 4099,
          ask: 4101,
          last: 4100,
          mark: 4100,
          index: 4100,
          percentage: 0,
          openInterest: 0,
          fundingRate: 0,
          fundingTime: 0,
          volume: 0,
          quoteVolume: 0,
        },
      ],
    });
    const portfolio = {
      portfolios: [
        {
          account: 'default:123',
          version: 1,
          balances: [{ account: 'default:123', version: 1, value: 10_000, currency: 'USD' }],
          positions: [
            {
              account: 'default:123',
              version: 1,
              positionCode: 'position-1',
              symbol,
              quantity: 0.25,
              side: 'BUY',
              quantityNotional: 1_000,
              openTime: '2026-07-15T09:00:00Z',
              openPrice: 4_000,
              lastUpdateTime: '2026-07-15T10:00:00Z',
              marginRate: 0.2,
            },
          ],
          orders: [],
        },
      ],
    };

    expect(exchange.formatOpenPositions(portfolio)).toEqual([
      expect.objectContaining({ notional: 1_000, marginSize: 200, unrealizedPnl: 25 }),
    ]);
    expect(exchange.formatBalance(portfolio)).toMatchObject({ total: 10_025, used: 200, upnl: 25, free: 9_825 });
  });
});

describe('Breakout Prop official DX REST session contract', () => {
  it('parses the server-provided inactivity timeout', async () => {
    expect(parseDxSessionTimeoutMs('01:02:03')).toBe(3_723_000);
    expect(() => parseDxSessionTimeoutMs('00:60:00')).toThrow('Invalid DXtrade session timeout');

    const exchange = new BreakoutpropExchange(
      'breakoutprop',
      { key: 'user', secret: 'password' } as ExchangeOptions,
      new DefaultStore(),
    );
    exchange.xhr = {
      post: vi.fn().mockResolvedValue({ data: { sessionToken: 'fresh', timeout: '00:01:30' } }),
    } as any;
    vi.spyOn(Date, 'now').mockReturnValue(1_000_000);

    await expect(exchange.getSessionToken()).resolves.toBe('fresh');
    expect(exchange.sessionTokenExpiresAt).toBe(1_090_000);
  });

  it('re-authenticates and retries a 401 exactly once', async () => {
    const exchange = new BreakoutpropExchange(
      'breakoutprop',
      { key: 'user', secret: 'password' } as ExchangeOptions,
      new DefaultStore(),
    );
    exchange.sessionToken = 'stale';
    exchange.sessionTokenExpiresAt = Date.now() + 60_000;
    let privateCalls = 0;
    let loginCalls = 0;
    exchange.xhr.defaults.adapter = vi.fn(async (config) => {
      if (config.url === ENDPOINTS.LOGIN) {
        loginCalls += 1;
        return {
          data: { sessionToken: 'fresh', timeout: '00:30:00' },
          status: 200,
          statusText: 'OK',
          headers: {},
          config,
        };
      }
      privateCalls += 1;
      if (privateCalls === 1) {
        throw new AxiosError('unauthorized', 'ERR_BAD_REQUEST', config, undefined, {
          data: {},
          status: 401,
          statusText: 'Unauthorized',
          headers: {},
          config,
        });
      }
      return { data: { ok: true }, status: 200, statusText: 'OK', headers: {}, config };
    });

    await expect(exchange.xhr.get('accounts/default%3A123/portfolio')).resolves.toMatchObject({ data: { ok: true } });
    expect(loginCalls).toBe(1);
    expect(privateCalls).toBe(2);
    expect(exchange.sessionToken).toBe('fresh');
  });
});

describe('Breakout Prop documented order prices', () => {
  it.each([
    ['LIMIT', { limitPrice: 3529.47 }, undefined, 3529.47],
    ['STOP', { stopPrice: 3500 }, undefined, 3500],
    ['STOP', { stopPrice: 3500 }, 0, 3500],
    ['STOP', {}, 3500, 3500],
    ['MARKET', {}, undefined, 0],
  ] as const)('reads %s prices with documented and legacy payloads', (type, prices, legacy, expected) => {
    const exchange = new BreakoutpropExchange('breakoutprop', { key: 'user', secret: 'password' } as ExchangeOptions, new DefaultStore());
    try {
      const orders = exchange.formatOrders({ portfolios: [{ orders: [{
        orderCode: 'native:1', instrument: 'ETHUSD', type, status: 'WORKING', ...prices,
        transactionTime: '2026-07-15T10:00:00Z',
        legs: [{ side: 'SELL', positionEffect: 'CLOSE', quantity: 0, filledQuantity: 0, remainingQuantity: 0,
          ...(legacy === undefined ? {} : { price: legacy }) }],
      }] }] } as any);
      expect(orders).toHaveLength(1);
      expect(orders[0].price).toBe(expected);
    } finally {
      exchange.dispose();
    }
  });
});
