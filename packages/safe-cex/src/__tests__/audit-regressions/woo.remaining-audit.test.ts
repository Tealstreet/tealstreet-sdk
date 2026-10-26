import { describe, expect, it, vi } from 'vitest';

import { WOOXExchange } from '../reference/woo/woo.exchange';
import { ENDPOINTS } from '../reference/woo/woo.types';
import { DefaultStore } from '../../store/store.base';
import {
  MarginType,
  OrderSide,
  OrderStatus,
  OrderTimeInForce,
  OrderType,
  PositionMode,
  PositionSide,
} from '../../types';

const makeExchange = () => {
  const store = new DefaultStore();
  const exchange = new WOOXExchange(
    'woo' as any,
    {
      key: 'key',
      secret: 'secret',
      applicationId: 'application-id',
    } as any,
    store,
  );

  store.update({
    markets: [
      {
        id: 'BTC/USDT:USDT',
        symbol: 'BTCUSDT',
        precision: { amount: 0.001, price: 0.1 },
        limits: { amount: { min: 0.001, max: 10 } },
        info: { price_range: 0.01 },
      } as any,
    ],
    tickers: [
      {
        id: 'BTC/USDT:USDT',
        symbol: 'BTCUSDT',
        last: 100,
      } as any,
    ],
  });
  vi.spyOn(exchange, 'getPositionMode').mockResolvedValue(PositionMode.OneWay);

  return { exchange, store };
};

describe('WooX remaining July audit regressions', () => {
  it('returns camelCase algo order IDs for partial TP/SL placement', async () => {
    const { exchange } = makeExchange();
    const post = vi.fn().mockResolvedValue({
      data: {
        success: true,
        data: { rows: [{ orderId: 101 }, { orderId: 102 }] },
        timestamp: 1,
      },
    });
    exchange.xhr = { post } as any;

    const ids = await exchange.placeStopMarket({
      symbol: 'BTCUSDT',
      side: OrderSide.Sell,
      type: OrderType.StopLoss,
      amount: 1,
      price: 90,
    } as any);

    expect(post).toHaveBeenCalledWith(
      ENDPOINTS.ALGO_ORDER,
      expect.objectContaining({ algoType: 'STOP', triggerPrice: '90', quantity: '1' }),
    );
    expect(ids).toEqual(['101', '102']);
  });

  it('emulates market buys with a crossing IOC order', async () => {
    const { exchange } = makeExchange();
    const post = vi.fn().mockResolvedValue({
      data: { success: true, order_id: 301 },
    });
    exchange.xhr = { post } as any;

    const ids = await exchange.placeOrder({
      symbol: 'BTCUSDT',
      side: OrderSide.Buy,
      type: OrderType.Market,
      amount: 2,
    });

    expect(post).toHaveBeenCalledWith(
      ENDPOINTS.PLACE_ORDER,
      expect.objectContaining({
        symbol: 'PERP_BTC_USDT',
        order_type: 'IOC',
        order_price: 100.9,
        side: 'BUY',
        order_quantity: 2,
      }),
    );
    expect(post.mock.calls[0]?.[1]).not.toHaveProperty('order_amount');
    expect(ids).toEqual(['301']);
  });

  it('uses mark price for market buys when legacy ticker ask is stale', async () => {
    const { exchange, store } = makeExchange();
    store.update({
      tickers: [
        {
          id: 'BTC/USDT:USDT',
          symbol: 'BTCUSDT',
          bid: 1989.9,
          ask: 1989.9,
          last: 1989.9,
          index: 1886.1,
          mark: 1886.1,
        } as any,
      ],
    });
    const post = vi.fn().mockResolvedValue({
      data: { success: true, order_id: 303 },
    });
    exchange.xhr = { post } as any;

    const ids = await exchange.placeOrder({
      symbol: 'BTCUSDT',
      side: OrderSide.Buy,
      type: OrderType.Market,
      amount: 2,
    });

    expect(post).toHaveBeenCalledWith(
      ENDPOINTS.PLACE_ORDER,
      expect.objectContaining({
        symbol: 'PERP_BTC_USDT',
        order_type: 'IOC',
        order_price: 1903.1,
        side: 'BUY',
        order_quantity: 2,
      }),
    );
    expect(ids).toEqual(['303']);
  });

  it('uses the current orderbook ask when fair-price slippage would not cross the WOO book', async () => {
    const { exchange, store } = makeExchange();
    store.update({
      tickers: [
        {
          id: 'BTC/USDT:USDT',
          symbol: 'BTCUSDT',
          bid: 1882.3,
          ask: 1882.3,
          last: 1882.3,
          index: 1882.3,
          mark: 1882.3,
        } as any,
      ],
    });
    const post = vi.fn().mockResolvedValue({
      data: { success: true, order_id: 304 },
    });
    const get = vi.fn().mockResolvedValue({
      data: {
        success: true,
        asks: [{ price: 1900.6, quantity: 0.1782 }],
        bids: [{ price: 1880, quantity: 0.0159 }],
      },
    });
    exchange.xhr = { get, post } as any;

    const ids = await exchange.placeOrder({
      symbol: 'BTCUSDT',
      side: OrderSide.Buy,
      type: OrderType.Market,
      amount: 2,
    });

    expect(get).toHaveBeenCalledWith(`${ENDPOINTS.ORDERBOOK}/PERP_BTC_USDT`);
    expect(post).toHaveBeenCalledWith(
      ENDPOINTS.PLACE_ORDER,
      expect.objectContaining({
        symbol: 'PERP_BTC_USDT',
        order_type: 'IOC',
        order_price: 1900.6,
        side: 'BUY',
        order_quantity: 2,
      }),
    );
    expect(ids).toEqual(['304']);
  });

  it('does not use an orderbook ask outside WOO market price range for market IOC emulation', async () => {
    const { exchange, store } = makeExchange();
    store.update({
      tickers: [
        {
          id: 'BTC/USDT:USDT',
          symbol: 'BTCUSDT',
          bid: 1876.1,
          ask: 1900.6,
          last: 1881.2,
          index: 1881.2,
          mark: 1881.2,
        } as any,
      ],
    });
    const post = vi.fn().mockResolvedValue({
      data: { success: true, order_id: 305 },
    });
    const get = vi.fn().mockResolvedValue({
      data: {
        success: true,
        asks: [{ price: 1900.6, quantity: 0.1782 }],
        bids: [{ price: 1876.1, quantity: 0.0159 }],
      },
    });
    exchange.xhr = { get, post } as any;

    const ids = await exchange.placeOrder({
      symbol: 'BTCUSDT',
      side: OrderSide.Buy,
      type: OrderType.Market,
      amount: 2,
    });

    expect(get).toHaveBeenCalledWith(`${ENDPOINTS.ORDERBOOK}/PERP_BTC_USDT`);
    expect(post).toHaveBeenCalledWith(
      ENDPOINTS.PLACE_ORDER,
      expect.objectContaining({
        symbol: 'PERP_BTC_USDT',
        order_type: 'IOC',
        order_price: 1898.1,
        side: 'BUY',
        order_quantity: 2,
      }),
    );
    expect(ids).toEqual(['305']);
  });

  it('treats WOO position-mode already-set errors as readback-confirmed no-ops', async () => {
    const { exchange, store } = makeExchange();
    vi.mocked(exchange.getPositionMode).mockRestore();
    const emitError = vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => {});
    const post = vi.fn().mockRejectedValue({
      response: {
        data: {
          success: false,
          code: -1005,
          error_code: 111000,
          message: 'Position mode remains unchanged as it is already set to your selection.',
        },
      },
      message: 'Bad Request',
    });
    const get = vi.fn(async (endpoint: string) => {
      if (endpoint === ENDPOINTS.ACCOUNT) {
        return {
          data: {
            success: true,
            data: {
              positionMode: 'ONE_WAY',
              accountMode: 'FUTURES',
              freeCollateral: 10,
              totalCollateral: 10,
              totalAccountValue: 10,
            },
          },
        };
      }
      if (endpoint === ENDPOINTS.POSITIONS) {
        return { data: { success: true, data: { positions: [] } } };
      }
      throw new Error(`unexpected endpoint ${endpoint}`);
    });
    exchange.xhr = { post, get } as any;

    await expect(exchange.changePositionMode(false)).resolves.toBeUndefined();

    expect(post).toHaveBeenCalledWith(ENDPOINTS.POSITION_MODE, { position_mode: 'ONE_WAY' });
    expect(get).toHaveBeenCalledWith(ENDPOINTS.ACCOUNT);
    expect(get).toHaveBeenCalledWith(ENDPOINTS.POSITIONS);
    expect(store.positions).toEqual([]);
    expect(emitError).not.toHaveBeenCalled();
  });

  it('rejects crossing post-only limits before WOO can fill them', async () => {
    const { exchange, store } = makeExchange();
    store.update({
      tickers: [
        {
          id: 'BTC/USDT:USDT',
          symbol: 'BTCUSDT',
          bid: 99,
          ask: 101,
          last: 102,
          index: 102,
          mark: 102,
        } as any,
      ],
    });
    const get = vi.fn().mockResolvedValue({
      data: {
        success: true,
        asks: [{ price: 101, quantity: 1 }],
        bids: [{ price: 99, quantity: 1 }],
      },
    });
    const post = vi.fn();
    exchange.xhr = { get, post } as any;

    await expect(
      exchange.placeOrder({
        symbol: 'BTCUSDT',
        side: OrderSide.Buy,
        type: OrderType.Limit,
        timeInForce: OrderTimeInForce.PostOnly,
        price: 101,
        amount: 2,
      }),
    ).rejects.toThrow('WOO post-only buy order would cross current ask 101');

    expect(get).toHaveBeenCalledWith(`${ENDPOINTS.ORDERBOOK}/PERP_BTC_USDT`);
    expect(post).not.toHaveBeenCalled();
  });

  it('rejects post-only limits that can cross hidden WOO liquidity near fair price', async () => {
    const { exchange, store } = makeExchange();
    store.update({
      tickers: [
        {
          id: 'BTC/USDT:USDT',
          symbol: 'BTCUSDT',
          bid: 99,
          ask: 101,
          last: 100,
          index: 100,
          mark: 100,
        } as any,
      ],
    });
    const get = vi.fn();
    const post = vi.fn();
    exchange.xhr = { get, post } as any;

    await expect(
      exchange.placeOrder({
        symbol: 'BTCUSDT',
        side: OrderSide.Buy,
        type: OrderType.Limit,
        timeInForce: OrderTimeInForce.PostOnly,
        price: 100.5,
        amount: 2,
      }),
    ).rejects.toThrow('WOO post-only buy order would cross current fair price 100');

    expect(get).not.toHaveBeenCalled();
    expect(post).not.toHaveBeenCalled();
  });

  it('caches zero-position leverage metadata instead of refetching every positions poll', async () => {
    const { exchange } = makeExchange();
    const get = vi.fn(async (endpoint: string) => {
      if (endpoint === ENDPOINTS.POSITIONS) {
        return { data: { success: true, data: { positions: [] } } };
      }
      if (endpoint === ENDPOINTS.FUTURES_LEVERAGE) {
        return {
          data: {
            success: true,
            data: {
              symbol: 'PERP_BTC_USDT',
              default_margin_mode: 'CROSS',
              position_mode: 'ONE_WAY',
              details: [{ position_side: 'BOTH', leverage: '10' }],
            },
            timestamp: 1,
          },
        };
      }
      throw new Error(`unexpected endpoint ${endpoint}`);
    });
    exchange.xhr = { get } as any;

    await expect(exchange.fetchPositions(['BTCUSDT'])).resolves.toEqual([
      expect.objectContaining({ symbol: 'BTCUSDT', contracts: 0, leverage: 10 }),
    ]);
    await expect(exchange.fetchPositions(['BTCUSDT'])).resolves.toEqual([
      expect.objectContaining({ symbol: 'BTCUSDT', contracts: 0, leverage: 10 }),
    ]);

    expect(get.mock.calls.filter(([endpoint]) => endpoint === ENDPOINTS.FUTURES_LEVERAGE)).toHaveLength(1);
  });

  it('keeps cached zero-position leverage metadata through Woo rate limits', async () => {
    const { exchange } = makeExchange();
    (exchange as any)._positionMetadataCache.set(`BTCUSDT:${PositionMode.OneWay}`, {
      metadata: [
        {
          symbol: 'BTCUSDT',
          side: PositionSide.Long,
          leverage: 10,
          positionMode: PositionMode.OneWay,
          marginType: MarginType.Cross,
        },
      ],
      updatedAt: 0,
    });
    const get = vi.fn(async (endpoint: string) => {
      if (endpoint === ENDPOINTS.POSITIONS) {
        return { data: { success: true, data: { positions: [] } } };
      }
      if (endpoint === ENDPOINTS.FUTURES_LEVERAGE) {
        throw {
          response: {
            status: 429,
            data: {
              code: -1003,
              error_code: 90003,
              message: 'You have exceeded the rate limit, please try again in 10 seconds.',
            },
          },
          message: 'Too Many Requests',
        };
      }
      throw new Error(`unexpected endpoint ${endpoint}`);
    });
    exchange.xhr = { get } as any;

    await expect(exchange.fetchPositions(['BTCUSDT'])).resolves.toEqual([
      expect.objectContaining({ symbol: 'BTCUSDT', contracts: 0, leverage: 10 }),
    ]);
    expect(get.mock.calls.filter(([endpoint]) => endpoint === ENDPOINTS.FUTURES_LEVERAGE)).toHaveLength(1);
  });

  it('emulates market sells with a crossing IOC order', async () => {
    const { exchange } = makeExchange();
    const post = vi.fn().mockResolvedValue({
      data: { success: true, order_id: 302 },
    });
    exchange.xhr = { post } as any;

    const ids = await exchange.placeOrder({
      symbol: 'BTCUSDT',
      side: OrderSide.Sell,
      type: OrderType.Market,
      amount: 2,
    });

    expect(post).toHaveBeenCalledWith(
      ENDPOINTS.PLACE_ORDER,
      expect.objectContaining({
        symbol: 'PERP_BTC_USDT',
        order_type: 'IOC',
        order_price: 99.1,
        side: 'SELL',
        order_quantity: 2,
      }),
    );
    expect(post.mock.calls[0]?.[1]).not.toHaveProperty('order_amount');
    expect(ids).toEqual(['302']);
  });

  it('returns camelCase algo order IDs for trailing-stop placement', async () => {
    const { exchange, store } = makeExchange();
    store.update({
      positions: [
        {
          id: 'BTCUSDT-Long',
          symbol: 'BTCUSDT',
          side: PositionSide.Long,
          contracts: 2,
        } as any,
      ],
    });
    const post = vi.fn().mockResolvedValue({
      data: {
        success: true,
        data: { rows: [{ orderId: 201 }] },
        timestamp: 1,
      },
    });
    exchange.xhr = { post } as any;

    const ids = await exchange.placeTrailingStopLossOrder({
      symbol: 'BTCUSDT',
      side: OrderSide.Sell,
      type: OrderType.TrailingStopLoss,
      price: 95,
    } as any);

    expect(ids).toEqual(['201']);
  });

  it('does not recreate a stale sibling before replacing a positional bracket', async () => {
    const { exchange, store } = makeExchange();
    const takeProfit = {
      id: 'tp-child',
      parentId: 'bracket-parent',
      symbol: 'BTCUSDT',
      side: OrderSide.Sell,
      type: OrderType.TakeProfit,
      status: OrderStatus.Open,
      price: 120,
    } as any;
    const oldStopLoss = {
      ...takeProfit,
      id: 'sl-child',
      type: OrderType.StopLoss,
      price: 90,
    } as any;
    store.update({ orders: [takeProfit, oldStopLoss] });

    const remove = vi.fn().mockResolvedValue({ data: { success: true } });
    exchange.xhr = { delete: remove } as any;
    const placePositional = vi.spyOn(exchange, 'placePositionalAlgoOrder').mockResolvedValue(['new-parent']);

    const ids = await exchange.placeAlgoOrder({
      symbol: 'BTCUSDT',
      side: OrderSide.Sell,
      type: OrderType.StopLoss,
      price: 92,
    } as any);

    expect(remove).toHaveBeenCalledTimes(1);
    expect(remove).toHaveBeenCalledWith(`${ENDPOINTS.ALGO_ORDER}/bracket-parent`);
    expect(placePositional).toHaveBeenCalledTimes(1);
    expect(placePositional).toHaveBeenCalledWith(expect.objectContaining({ stopLoss: 92, takeProfit: 120 }));
    expect(ids).toEqual(['new-parent']);
  });

  it('selects the same closing side when replacing a hedged positional bracket', async () => {
    const { exchange, store } = makeExchange();
    const shortTakeProfit = {
      id: 'short-tp',
      parentId: 'short-parent',
      symbol: 'BTCUSDT',
      side: OrderSide.Buy,
      type: OrderType.TakeProfit,
      status: OrderStatus.Open,
      price: 80,
    } as any;
    const longTakeProfit = {
      ...shortTakeProfit,
      id: 'long-tp',
      parentId: 'long-parent',
      side: OrderSide.Sell,
      price: 120,
    } as any;
    store.update({ orders: [shortTakeProfit, longTakeProfit] });

    const remove = vi.fn().mockResolvedValue({ data: { success: true } });
    exchange.xhr = { delete: remove } as any;
    const placePositional = vi.spyOn(exchange, 'placePositionalAlgoOrder').mockResolvedValue(['new-parent']);

    await exchange.placeAlgoOrder({
      symbol: 'BTCUSDT',
      side: OrderSide.Sell,
      type: OrderType.StopLoss,
      price: 92,
    } as any);

    expect(remove).toHaveBeenCalledWith(`${ENDPOINTS.ALGO_ORDER}/long-parent`);
    expect(placePositional).toHaveBeenCalledWith(expect.objectContaining({ stopLoss: 92, takeProfit: 120 }));
  });

  it('rejects when replacement bracket placement fails after cancellation', async () => {
    const { exchange, store } = makeExchange();
    store.update({
      orders: [
        {
          id: 'tp-child',
          parentId: 'bracket-parent',
          symbol: 'BTCUSDT',
          side: OrderSide.Sell,
          type: OrderType.TakeProfit,
          status: OrderStatus.Open,
          price: 120,
        } as any,
      ],
    });
    const replacementError = {
      response: { data: { message: 'replacement rejected' } },
      message: 'Request failed',
    };
    const remove = vi.fn().mockResolvedValue({ data: { success: true } });
    const post = vi.fn().mockRejectedValue(replacementError);
    exchange.xhr = { delete: remove, post } as any;
    const emitError = vi.spyOn(exchange, 'emitAndLogError').mockReturnValue(undefined as any);

    await expect(
      exchange.placeAlgoOrder({
        symbol: 'BTCUSDT',
        side: OrderSide.Sell,
        type: OrderType.StopLoss,
        price: 92,
      } as any),
    ).rejects.toBe(replacementError);

    expect(remove).toHaveBeenCalledWith(`${ENDPOINTS.ALGO_ORDER}/bracket-parent`);
    expect(post).toHaveBeenCalledWith(ENDPOINTS.ALGO_ORDER, expect.objectContaining({ algoType: 'POSITIONAL_TP_SL' }));
    expect(emitError.mock.calls[0]?.[0]).toBe('replacement rejected');
  });

  it('surfaces private websocket authentication failures', () => {
    const { exchange } = makeExchange();
    const emitError = vi.spyOn(exchange, 'emitAndLogError').mockReturnValue(undefined as any);

    exchange.privateWebsocket.onMessage({
      data: JSON.stringify({ event: 'auth', success: false, message: 'invalid signature' }),
    } as MessageEvent);

    expect(emitError.mock.calls[0]?.[0]).toBe('invalid signature');
  });

  it('returns the WooX response message when account validation fails', async () => {
    const { exchange } = makeExchange();
    exchange._primaryXhr = {
      get: vi.fn().mockRejectedValue({
        response: { data: { message: 'API key is invalid' } },
        message: 'Request failed',
      }),
    } as any;
    vi.spyOn(exchange, 'emitAndLogError').mockReturnValue(undefined as any);

    await expect(exchange.validateAccount()).resolves.toEqual({
      error: 'API key is invalid',
    });
  });

  it('uses the current woox.io domain for legacy historical klines', () => {
    expect(ENDPOINTS.KLINE_HISTORY).toBe('https://api-pub.woox.io/v1/hist/kline');
  });
});
