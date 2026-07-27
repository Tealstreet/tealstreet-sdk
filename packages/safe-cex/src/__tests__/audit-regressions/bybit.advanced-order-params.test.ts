import { afterEach, describe, expect, it, vi } from 'vitest';

import type {
  BybitBboOrderOpts,
  BybitTickSizeMarketOrderOpts,
} from '../../index';

import { BybitExchange } from '../../exchanges/bybit/bybit.exchange';
import { getBybitOrderExecutionParams } from '../../exchanges/bybit/bybit.order-params';
import { ENDPOINTS } from '../../exchanges/bybit/bybit.types';
import { BybitDemoExchange } from '../../exchanges/bybit/bybitdemo.exchange';
import { BybitInverseExchange } from '../../exchanges/bybit/bybitinverse.exchange';
import { BybitSpotExchange } from '../../exchanges/bybit/bybitspot.exchange';
import { OrderSide, OrderTimeInForce, OrderType, PositionMode } from '../../types';

const bboOrder = (symbol = 'BTCUSDT'): BybitBboOrderOpts => ({
  symbol,
  type: OrderType.Limit,
  side: OrderSide.Buy,
  amount: symbol === 'BTCUSD' ? 25 : 0.5,
  bybitBbo: { sideType: 'Counterparty', level: 3 },
});

const tickSizeOrder = (symbol = 'BTCUSDT'): BybitTickSizeMarketOrderOpts => ({
  symbol,
  type: OrderType.Market,
  side: OrderSide.Buy,
  amount: symbol === 'BTCUSD' ? 25 : 0.5,
  bybitSlippageToleranceTicks: 125,
});

const createStore = (symbol: string, quote: string, isInverse = false) => {
  const market = {
    id: symbol,
    symbol,
    base: 'BTC',
    quote,
    active: true,
    precision: { amount: isInverse ? 1 : 0.001, price: 0.1 },
    limits: { amount: { min: isInverse ? 1 : 0.001, max: 100 }, leverage: { min: 1, max: 100 } },
    isInverse,
  };
  return {
    subscribe: vi.fn(),
    markets: [market],
    marketsBySymbol: { [symbol]: market },
    tickersBySymbol: {},
    orders: [],
    positions: [],
  } as any;
};

const mockPositionMode = (exchange: BybitExchange | BybitDemoExchange | BybitInverseExchange) => {
  (exchange as any).fetchPositionMode = vi.fn().mockResolvedValue(PositionMode.OneWay);
};

afterEach(() => {
  vi.useRealTimers();
  vi.restoreAllMocks();
});

describe('Bybit adapter-only advanced order parameters', () => {
  it('serializes explicit TickSize and BBO intent without defaults', () => {
    expect(getBybitOrderExecutionParams(tickSizeOrder(), 'linear')).toEqual({
      slippageToleranceType: 'TickSize',
      slippageTolerance: '125',
    });
    expect(getBybitOrderExecutionParams(bboOrder(), 'linear')).toEqual({
      bboSideType: 'Counterparty',
      bboLevel: '3',
    });
    expect(
      getBybitOrderExecutionParams(
        { symbol: 'BTCUSDT', type: OrderType.Market, side: OrderSide.Buy, amount: 0.5 },
        'linear',
      ),
    ).toEqual({});
  });

  it.each([0, -1, 1.5, 10_001, Number.POSITIVE_INFINITY])(
    'rejects invalid TickSize tolerance %s',
    (bybitSlippageToleranceTicks) => {
      expect(() =>
        getBybitOrderExecutionParams(
          { ...tickSizeOrder(), bybitSlippageToleranceTicks } as any,
          'linear',
        ),
      ).toThrow('Bybit TickSize slippage must be an integer from 1 to 10000');
    },
  );

  it('rejects non-plain and ambiguous TickSize market orders', () => {
    expect(() =>
      getBybitOrderExecutionParams({ ...tickSizeOrder(), type: OrderType.Limit } as any, 'linear'),
    ).toThrow('Bybit TickSize slippage is only supported for plain market orders');
    expect(() =>
      getBybitOrderExecutionParams({ ...tickSizeOrder(), triggerPrice: 30_000 } as any, 'linear'),
    ).toThrow('Bybit TickSize slippage is only supported for plain market orders');
    expect(() =>
      getBybitOrderExecutionParams({ ...tickSizeOrder(), maxSlippagePct: 1 } as any, 'linear'),
    ).toThrow('Bybit TickSize slippage is only supported for plain market orders');
  });

  it('rejects malformed, incompatible, and unsupported-category BBO intent', () => {
    expect(() =>
      getBybitOrderExecutionParams({ ...bboOrder(), price: 30_000 } as any, 'linear'),
    ).toThrow('Bybit BBO cannot include price, conditional, attached TP/SL, or slippage fields');
    expect(() =>
      getBybitOrderExecutionParams(
        { ...bboOrder(), timeInForce: OrderTimeInForce.PostOnly } as any,
        'linear',
      ),
    ).toThrow('Bybit BBO currently supports only GoodTillCancel time in force');
    expect(() =>
      getBybitOrderExecutionParams({ ...bboOrder(), bybitBbo: { sideType: 'Queue', level: 6 } } as any, 'linear'),
    ).toThrow('Bybit BBO level must be an integer from 1 to 5');
    expect(() =>
      getBybitOrderExecutionParams(
        { ...bboOrder(), bybitBbo: { sideType: 'Nearest', level: 1 } } as any,
        'linear',
      ),
    ).toThrow('Bybit BBO sideType must be Queue or Counterparty');
    expect(() => getBybitOrderExecutionParams(bboOrder(), 'spot')).toThrow(
      'Bybit BBO orders are only supported for linear and inverse products',
    );
  });

  it('rejects BBO and TickSize controls together', () => {
    expect(() =>
      getBybitOrderExecutionParams(
        { ...bboOrder(), bybitSlippageToleranceTicks: 10 } as any,
        'linear',
      ),
    ).toThrow('Bybit BBO and TickSize slippage options are mutually exclusive');
  });

  it('sends TickSize through linear, inverse, demo, and spot single-order endpoints', async () => {
    const variants = [
      {
        exchange: new BybitExchange('bybit', { key: 'key', secret: 'secret' }, createStore('BTCUSDT', 'USDT')),
        order: tickSizeOrder(),
      },
      {
        exchange: new BybitInverseExchange(
          'bybitinverse',
          { key: 'key', secret: 'secret' },
          createStore('BTCUSD', 'USD', true),
        ),
        order: tickSizeOrder('BTCUSD'),
      },
      {
        exchange: new BybitDemoExchange(
          'bybitdemo',
          { key: 'key', secret: 'secret' },
          createStore('BTCUSDT', 'USDT'),
        ),
        order: tickSizeOrder(),
      },
      {
        exchange: new BybitSpotExchange('bybitspot', { key: 'key', secret: 'secret' }, createStore('BTCUSDT', 'USDT')),
        order: tickSizeOrder(),
      },
    ];

    for (const { exchange, order } of variants) {
      if (!(exchange instanceof BybitSpotExchange)) mockPositionMode(exchange);
      const post = vi.fn().mockResolvedValue({ data: { retMsg: 'OK', result: { orderId: 'order-1' } } });
      exchange.xhr = { post } as any;

      await expect(exchange.placeOrder(order)).resolves.toEqual(['order-1']);
      expect(post).toHaveBeenCalledWith(
        ENDPOINTS.CREATE_ORDER,
        expect.objectContaining({
          orderType: 'Market',
          slippageToleranceType: 'TickSize',
          slippageTolerance: '125',
        }),
      );
    }
  });

  it('sends no BBO or slippage fields when the caller sets no execution control', async () => {
    const exchange = new BybitExchange('bybit', { key: 'key', secret: 'secret' }, createStore('BTCUSDT', 'USDT'));
    mockPositionMode(exchange);
    const post = vi.fn().mockResolvedValue({ data: { retMsg: 'OK', result: { orderId: 'order-1' } } });
    exchange.xhr = { post } as any;

    await exchange.placeOrder({
      symbol: 'BTCUSDT',
      type: OrderType.Market,
      side: OrderSide.Buy,
      amount: 0.5,
    });

    const payload = post.mock.calls[0][1];
    expect(payload).not.toHaveProperty('slippageToleranceType');
    expect(payload).not.toHaveProperty('slippageTolerance');
    expect(payload).not.toHaveProperty('bboSideType');
    expect(payload).not.toHaveProperty('bboLevel');
  });

  it('sends price-less BBO orders through linear, inverse, and demo single-order endpoints', async () => {
    const variants = [
      {
        exchange: new BybitExchange('bybit', { key: 'key', secret: 'secret' }, createStore('BTCUSDT', 'USDT')),
        order: bboOrder(),
      },
      {
        exchange: new BybitInverseExchange(
          'bybitinverse',
          { key: 'key', secret: 'secret' },
          createStore('BTCUSD', 'USD', true),
        ),
        order: bboOrder('BTCUSD'),
      },
      {
        exchange: new BybitDemoExchange(
          'bybitdemo',
          { key: 'key', secret: 'secret' },
          createStore('BTCUSDT', 'USDT'),
        ),
        order: bboOrder(),
      },
    ];

    for (const { exchange, order } of variants) {
      mockPositionMode(exchange);
      const post = vi.fn().mockResolvedValue({ data: { retMsg: 'OK', result: { orderId: 'order-1' } } });
      exchange.xhr = { post } as any;

      await expect(exchange.placeOrder(order)).resolves.toEqual(['order-1']);
      const payload = post.mock.calls[0][1];
      expect(post.mock.calls[0][0]).toBe(ENDPOINTS.CREATE_ORDER);
      expect(payload).toEqual(
        expect.objectContaining({ orderType: 'Limit', bboSideType: 'Counterparty', bboLevel: '3' }),
      );
      expect(payload).not.toHaveProperty('price');
    }
  });

  it('keeps BBO and TickSize off the batch endpoint', async () => {
    const exchange = new BybitExchange('bybit', { key: 'key', secret: 'secret' }, createStore('BTCUSDT', 'USDT'));
    mockPositionMode(exchange);
    const post = vi.fn().mockResolvedValue({ data: { retMsg: 'OK', result: { orderId: 'order-1' } } });
    exchange.xhr = { post } as any;

    await expect(exchange.placeOrders([bboOrder(), tickSizeOrder()])).resolves.toEqual(['order-1', 'order-1']);
    expect(post).toHaveBeenCalledTimes(2);
    expect(post).toHaveBeenNthCalledWith(1, ENDPOINTS.CREATE_ORDER, expect.objectContaining({ bboLevel: '3' }));
    expect(post).toHaveBeenNthCalledWith(
      2,
      ENDPOINTS.CREATE_ORDER,
      expect.objectContaining({ slippageToleranceType: 'TickSize' }),
    );
  });

  it('preflights an entire batch before I/O and propagates explicit-control validation errors', async () => {
    const exchange = new BybitExchange('bybit', { key: 'key', secret: 'secret' }, createStore('BTCUSDT', 'USDT'));
    const post = vi.fn();
    const get = vi.fn();
    const emitAndLogError = vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => {});
    exchange.xhr = { get, post } as any;

    const invalid = { ...bboOrder(), bybitBbo: { sideType: 'Queue', level: 6 } } as any;
    await expect(exchange.placeOrders([tickSizeOrder(), invalid])).rejects.toThrow(
      'Bybit BBO level must be an integer from 1 to 5',
    );
    expect(emitAndLogError).toHaveBeenCalledTimes(1);
    expect(emitAndLogError).toHaveBeenCalledWith('Bybit BBO level must be an integer from 1 to 5');
    expect(get).not.toHaveBeenCalled();
    expect(post).not.toHaveBeenCalled();
  });

  it('preflights the sequential spot bulk path before placing an earlier valid order', async () => {
    const exchange = new BybitSpotExchange(
      'bybitspot',
      { key: 'key', secret: 'secret' },
      createStore('BTCUSDT', 'USDT'),
    );
    const post = vi.fn();
    exchange.xhr = { post } as any;

    await expect(
      exchange.placeOrders([
        tickSizeOrder(),
        { ...tickSizeOrder(), bybitSlippageToleranceTicks: 10_001 } as any,
      ]),
    ).rejects.toThrow('Bybit TickSize slippage must be an integer from 1 to 10000');
    expect(post).not.toHaveBeenCalled();
  });

  it('forces an order snapshot after position-bound TP/SL placement returns no order id', async () => {
    vi.useFakeTimers();
    const exchange = new BybitExchange('bybit', { key: 'key', secret: 'secret' }, createStore('BTCUSDT', 'USDT'));
    mockPositionMode(exchange);
    const post = vi.fn().mockResolvedValue({ data: { retMsg: 'OK', result: {} } });
    const fetchAndSetPrivateSnapshotHelper = vi
      .spyOn(exchange, 'fetchAndSetPrivateSnapshotHelper')
      .mockResolvedValue(undefined);
    exchange.xhr = { post } as any;

    await expect(
      exchange.placeStopLossOrTakeProfit({
        symbol: 'BTCUSDT',
        type: OrderType.StopLoss,
        side: OrderSide.Sell,
        price: 65_000,
        amount: 0.03,
        reduceOnly: true,
      }),
    ).resolves.toEqual([]);

    expect(post).toHaveBeenCalledWith(
      ENDPOINTS.SET_TRADING_STOP,
      expect.objectContaining({
        category: 'linear',
        symbol: 'BTCUSDT',
        tpslMode: 'Partial',
        stopLoss: '65000',
        slSize: '0.03',
      }),
    );
    expect(fetchAndSetPrivateSnapshotHelper).not.toHaveBeenCalled();

    await vi.advanceTimersByTimeAsync(251);

    expect(fetchAndSetPrivateSnapshotHelper).toHaveBeenCalledOnce();
  });
});
