import type { Axios } from 'axios';

import { describe, expect, it, vi } from 'vitest';

import { OrderlyExchange } from '../reference/orderly/orderly.exchange';
import { ENDPOINTS } from '../reference/orderly/orderly.types';
import { DefaultStore } from '../../store/store.base';
import { ExchangeOptions, Market, Order, OrderSide, OrderStatus, OrderTimeInForce, OrderType } from '../../types';

const MARKET: Market = {
  id: 'BTC/USDC:USDC',
  symbol: 'BTCUSDC',
  base: 'BTC',
  quote: 'USDC',
  active: true,
  precision: { amount: 0.001, price: 0.01 },
  limits: { amount: { min: 0.001, max: 100 }, leverage: { min: 1, max: 100 } },
};

const rawRestOrder = (overrides: Record<string, unknown> = {}) => ({
  order_id: 78_151,
  symbol: 'PERP_BTC_USDC',
  type: 'LIMIT',
  side: 'BUY',
  status: 'NEW',
  price: 50_000,
  quantity: 1.25,
  total_executed_quantity: 0,
  executed_quantity: 0,
  executed: 0,
  visible_quantity: 1.25,
  reduce_only: false,
  margin_mode: 'CROSS',
  order_tag: 'tealstreet',
  ...overrides,
});

const createExchange = (
  rawOverrides: Record<string, unknown> = {},
  orderOverrides: Partial<Order> = {},
  response: unknown = { success: true, data: { status: 'EDIT_SENT' }, timestamp: 1_784_505_600_000 },
) => {
  const store = new DefaultStore();
  store.update({ markets: [MARKET] });
  const exchange = new OrderlyExchange(
    'woofi',
    {
      key: 'test-key',
      secret: 'test-secret',
      applicationId: 'test-account',
      tuning: { consoleLogging: false },
    } as ExchangeOptions,
    store,
  );
  const mapped = exchange.mapLimitOrder(rawRestOrder(rawOverrides));
  if (!mapped) throw new Error('fixture did not map');
  const order: Order = {
    ...mapped,
    timeInForce: OrderTimeInForce.GoodTillCancel,
    ...orderOverrides,
  };
  store.update({ orders: [order] });

  const put = vi.fn().mockResolvedValue({ data: response });
  exchange.xhr = { put } as unknown as Axios;
  const cancelOrders = vi.spyOn(exchange, 'cancelOrders');
  const placeOrder = vi.spyOn(exchange, 'placeOrder');

  return { exchange, store, order, put, cancelOrders, placeOrder };
};

const expectNoCancelReplace = (cancelOrders: ReturnType<typeof vi.spyOn>, placeOrder: ReturnType<typeof vi.spyOn>) => {
  expect(cancelOrders).not.toHaveBeenCalled();
  expect(placeOrder).not.toHaveBeenCalled();
};

describe('audit regression: Orderly stable-ID native LIMIT amend', () => {
  it('sends the exact required identity plus only a changed price and returns the stable ID', async () => {
    const { exchange, order, put, cancelOrders, placeOrder } = createExchange();

    await expect(exchange.updateOrder({ order, update: { price: 50_100.25 } })).resolves.toEqual(['78151']);

    expect(put).toHaveBeenCalledOnce();
    expect(put).toHaveBeenCalledWith(ENDPOINTS.UPDATE_ORDER, {
      order_id: '78151',
      symbol: 'PERP_BTC_USDC',
      order_type: 'LIMIT',
      side: 'BUY',
      order_price: 50_100.25,
    });
    expect(put.mock.calls[0][1]).not.toHaveProperty('order_quantity');
    expect(put.mock.calls[0][1]).not.toHaveProperty('reduce_only');
    expectNoCancelReplace(cancelOrders, placeOrder);
  });

  it('sends the exact required identity plus only a changed quantity and preserves reduce-only in place', async () => {
    const { exchange, order, put, cancelOrders, placeOrder } = createExchange({
      side: 'SELL',
      reduce_only: true,
    });

    expect(order.reduceOnly).toBe(true);
    await expect(exchange.updateOrder({ order, update: { amount: 0.75 } })).resolves.toEqual(['78151']);

    expect(put).toHaveBeenCalledWith(ENDPOINTS.UPDATE_ORDER, {
      order_id: '78151',
      symbol: 'PERP_BTC_USDC',
      order_type: 'LIMIT',
      side: 'SELL',
      order_quantity: 0.75,
    });
    expect(put.mock.calls[0][1]).not.toHaveProperty('order_price');
    expectNoCancelReplace(cancelOrders, placeOrder);
  });

  it('preserves a proven isolated-margin order while editing by stable ID', async () => {
    const { exchange, order, put, cancelOrders, placeOrder } = createExchange({ margin_mode: 'ISOLATED' });

    await expect(exchange.updateOrder({ order, update: { price: 50_100 } })).resolves.toEqual(['78151']);

    expect(put).toHaveBeenCalledWith(ENDPOINTS.UPDATE_ORDER, {
      order_id: '78151',
      symbol: 'PERP_BTC_USDC',
      order_type: 'LIMIT',
      side: 'BUY',
      order_price: 50_100,
    });
    expectNoCancelReplace(cancelOrders, placeOrder);
  });

  it('prefers an explicit REST reduce_only false over a conflicting camel-case value in the mapper', () => {
    const { exchange } = createExchange({ reduce_only: false, reduceOnly: true });

    expect(exchange.mapLimitOrder(rawRestOrder({ reduce_only: false, reduceOnly: true }))?.reduceOnly).toBe(false);
  });

  it.each([
    {
      name: 'success false',
      response: { success: false, code: -1000, message: 'edit rejected', data: { status: 'EDIT_REJECTED' } },
      error: 'edit rejected',
    },
    {
      name: 'unexpected success status',
      response: { success: true, data: { status: 'PENDING' } },
      error: 'Orderly native amend returned PENDING',
    },
  ])('surfaces $name instead of reporting a successful amend', async ({ response, error }) => {
    const { exchange, order, put, cancelOrders, placeOrder } = createExchange({}, {}, response);

    await expect(exchange.updateOrder({ order, update: { price: 50_100 } })).rejects.toThrow(error);

    expect(put).toHaveBeenCalledOnce();
    expectNoCancelReplace(cancelOrders, placeOrder);
  });

  it('surfaces signing or network failures without cancel-replace fallback', async () => {
    const { exchange, order, put, cancelOrders, placeOrder } = createExchange();
    put.mockRejectedValueOnce(new Error('signed request failed'));

    await expect(exchange.updateOrder({ order, update: { amount: 1 } })).rejects.toThrow('signed request failed');

    expectNoCancelReplace(cancelOrders, placeOrder);
  });

  it.each([
    {
      name: 'partial fill status',
      raw: { status: 'PARTIAL_FILLED', total_executed_quantity: 0.25, executed_quantity: 0.25, executed: 0.25 },
      update: { price: 50_100 },
      error: 'requires an unfilled pending REST order',
    },
    {
      name: 'subnormal cumulative fill hidden by ordinary decimal rounding',
      raw: { total_executed_quantity: Number.MIN_VALUE },
      update: { price: 50_100 },
      error: 'requires exact zero-fill state',
    },
    {
      name: 'unsafe numeric order identity',
      raw: { order_id: Number.MAX_SAFE_INTEGER + 1 },
      update: { price: 50_100 },
      error: 'requires exact immutable order identity parity',
    },
    {
      name: 'non-canonical string order identity',
      raw: { order_id: '078151' },
      update: { price: 50_100 },
      error: 'requires exact immutable order identity parity',
    },
    {
      name: 'websocket-only row missing REST reduce-only and margin provenance',
      raw: { reduce_only: undefined },
      deleteRawKeys: ['reduce_only'],
      update: { price: 50_100 },
      error: 'requires explicit REST identity, fill, visibility, reduce-only, and margin-mode provenance',
    },
    {
      name: 'websocket camel-case margin mode without REST provenance',
      raw: { marginMode: 'ISOLATED' },
      deleteRawKeys: ['margin_mode'],
      update: { price: 50_100 },
      error: 'requires explicit REST identity, fill, visibility, reduce-only, and margin-mode provenance',
    },
    {
      name: 'unknown REST margin mode',
      raw: { margin_mode: 'PORTFOLIO' },
      update: { price: 50_100 },
      error: 'requires proven CROSS or ISOLATED margin provenance',
    },
    {
      name: 'iceberg visibility',
      raw: { visible_quantity: 0.5 },
      update: { price: 50_100 },
      error: 'requires exact ordinary LIMIT price, quantity, visibility, and precision parity',
    },
    {
      name: 'off-grid current raw price',
      raw: { price: 50_000.001 },
      update: { price: 50_100 },
      error: 'requires exact ordinary LIMIT price, quantity, visibility, and precision parity',
    },
    {
      name: 'trigger marker on an otherwise regular LIMIT row',
      raw: { trigger_price: 49_000 },
      update: { price: 50_100 },
      error: 'cannot preserve an unsupported time-in-force or trigger shape',
    },
    {
      name: 'post-only current order',
      raw: { time_in_force: 'POST_ONLY' },
      order: { timeInForce: OrderTimeInForce.PostOnly },
      update: { price: 50_100 },
      error: 'supports only ordinary open GTC LIMIT orders',
    },
    {
      name: 'no-op price',
      raw: {},
      update: { price: 50_000 },
      error: 'requires a real change after market-precision validation',
    },
    {
      name: 'off-grid amended value',
      raw: {},
      update: { price: 50_100.001 },
      error: 'requires an amended value aligned exactly to market precision',
    },
    {
      name: 'positive value that underflows market precision',
      raw: {},
      update: { amount: Number.MIN_VALUE },
      error: 'requires an amended value aligned exactly to market precision',
    },
  ])('rejects $name before every write', async ({ raw, order: orderOverrides, update, error, deleteRawKeys }) => {
    const { exchange, store, put, cancelOrders, placeOrder } = createExchange(raw, orderOverrides);
    const order = store.orders[0];
    for (const key of deleteRawKeys || []) delete order.info[key];

    await expect(exchange.updateOrder({ order, update } as any)).rejects.toThrow(error);

    expect(put).not.toHaveBeenCalled();
    expectNoCancelReplace(cancelOrders, placeOrder);
  });

  it('rejects an unreconciled and malformed combined update before every write', async () => {
    const { exchange, store, order, put, cancelOrders, placeOrder } = createExchange();
    const emitAndLogError = vi.spyOn(exchange, 'emitAndLogError');
    store.update({ orders: [] });

    await expect(exchange.updateOrder({ order, update: { price: 50_100, amount: 1 } } as any)).rejects.toThrow(
      'requires exactly one price or quantity change',
    );
    await expect(exchange.updateOrder({ order, update: { price: 50_100 } })).rejects.toThrow(
      'requires an exactly reconciled store order',
    );

    expect(put).not.toHaveBeenCalled();
    expect(emitAndLogError).toHaveBeenCalledTimes(2);
    expect(emitAndLogError).toHaveBeenNthCalledWith(
      1,
      expect.objectContaining({ message: 'Orderly native amend requires exactly one price or quantity change' }),
    );
    expectNoCancelReplace(cancelOrders, placeOrder);
  });

  it('preserves the existing native algo trigger update path', async () => {
    const { exchange, put } = createExchange();
    const triggerOrder: Order = {
      id: '99',
      parentId: '88',
      status: OrderStatus.Open,
      symbol: MARKET.symbol,
      type: OrderType.StopLoss,
      side: OrderSide.Sell,
      price: 49_000,
      amount: 1,
      filled: 0,
      remaining: 1,
      reduceOnly: true,
      lastUpdate: 1_784_505_600_000,
    };

    await expect(exchange.updateOrder({ order: triggerOrder, update: { price: 48_500 } })).resolves.toEqual(['99']);
    expect(put).toHaveBeenCalledWith(ENDPOINTS.ALGO_ORDER, {
      order_id: 88,
      child_orders: [{ order_id: '99', trigger_price: 48_500 }],
    });
  });
});
