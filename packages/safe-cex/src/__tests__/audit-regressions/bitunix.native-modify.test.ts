import type { Axios } from 'axios';

import { describe, expect, it, vi } from 'vitest';

import { BitunixExchange } from '../../exchanges/bitunix/bitunix.exchange';
import { ENDPOINTS } from '../../exchanges/bitunix/bitunix.types';
import { DefaultStore } from '../../store/store.base';
import {
  ExchangeOptions,
  Market,
  Order,
  OrderSide,
  OrderStatus,
  OrderStopTriggerType,
  OrderTimeInForce,
  OrderType,
} from '../../types';

const MARKET: Market = {
  id: 'BTC/USDT',
  symbol: 'BTCUSDT',
  base: 'BTC',
  quote: 'USDT',
  active: true,
  precision: { amount: 0.001, price: 0.1 },
  limits: { amount: { min: 0.001, max: 100 }, leverage: { min: 1, max: 125 } },
};

const createParentOrder = (overrides: Partial<Order> = {}): Order => ({
  id: 'bitunix-order-1',
  status: OrderStatus.Open,
  symbol: MARKET.symbol,
  type: OrderType.Limit,
  side: OrderSide.Buy,
  price: 50_000,
  amount: 1.25,
  filled: 0.25,
  remaining: 1,
  reduceOnly: false,
  timeInForce: OrderTimeInForce.GoodTillCancel,
  lastUpdate: 1_784_505_600_000,
  info: {
    orderId: 'bitunix-order-1',
    orderType: 'LIMIT',
    qty: '1.25',
    price: '50000',
    reduceOnly: false,
    tpPrice: '51000.04',
    tpStopType: 'MARK_PRICE',
    tpOrderType: 'MARKET',
    slPrice: '49000.04',
    slStopType: 'LAST_PRICE',
    slOrderType: 'LIMIT',
    slOrderPrice: '48950.04',
  },
  ...overrides,
});

const createVirtualOrder = (parent: Order, leg: 'tp' | 'sl'): Order => ({
  id: `${parent.id}__${leg}`,
  status: OrderStatus.Open,
  symbol: parent.symbol,
  type: leg === 'tp' ? OrderType.TakeProfit : OrderType.StopLoss,
  side: OrderSide.Sell,
  price: leg === 'tp' ? 51_000 : 49_000,
  amount: parent.amount,
  filled: 0,
  remaining: parent.amount,
  reduceOnly: true,
  timeInForce: OrderTimeInForce.GoodTillCancel,
  lastUpdate: parent.lastUpdate,
  info: parent.info,
  ...(leg === 'tp'
    ? { tpTriggerType: OrderStopTriggerType.MarkPrice }
    : { slTriggerType: OrderStopTriggerType.LastPrice }),
});

const createExchange = () => {
  const store = new DefaultStore();
  store.update({ markets: [MARKET] });
  const exchange = new BitunixExchange(
    'bitunix',
    {
      key: 'test-key',
      secret: 'test-secret',
      testnet: false,
      tuning: { consoleLogging: false },
    } as ExchangeOptions,
    store,
  );
  const post = vi.fn().mockResolvedValue({
    data: { code: 0, data: { orderId: 'bitunix-order-1', clientId: 'stable-client-id' }, msg: 'Success' },
  });
  exchange.unlimitedXHR = { post } as unknown as Axios;

  return { exchange, post };
};

describe('Bitunix native modify-order safety boundary', () => {
  it('amends a reconciled LIMIT by exact order id while preserving attached protection', async () => {
    const { exchange, post } = createExchange();
    const order = createParentOrder();
    const cancelOrders = vi.spyOn(exchange, 'cancelOrders');
    const placeOrder = vi.spyOn(exchange, 'placeOrder');

    await expect(exchange.updateOrder({ order, update: { price: 50_100.04 } })).resolves.toEqual([
      'bitunix-order-1',
    ]);

    expect(post).toHaveBeenCalledOnce();
    expect(post).toHaveBeenCalledWith(ENDPOINTS.MODIFY_ORDER, {
      orderId: 'bitunix-order-1',
      qty: '1.25',
      price: '50100',
      tpPrice: '51000',
      tpStopType: 'MARK_PRICE',
      tpOrderType: 'MARKET',
      slPrice: '49000',
      slStopType: 'LAST_PRICE',
      slOrderType: 'LIMIT',
      slOrderPrice: '48950',
    });
    expect(cancelOrders).not.toHaveBeenCalled();
    expect(placeOrder).not.toHaveBeenCalled();
  });

  it('sends the requested total base quantity after a partial fill and preserves reduce-only state in place', async () => {
    const { exchange, post } = createExchange();
    const order = createParentOrder({
      reduceOnly: true,
      info: { ...createParentOrder().info, reduceOnly: true, tpPrice: null, slPrice: null },
    });

    await expect(exchange.updateOrder({ order, update: { amount: 0.7504 } })).resolves.toEqual([
      'bitunix-order-1',
    ]);

    expect(post).toHaveBeenCalledWith(ENDPOINTS.MODIFY_ORDER, {
      orderId: 'bitunix-order-1',
      qty: '0.75',
      price: '50000',
    });
    expect(post.mock.calls[0][1]).not.toHaveProperty('reduceOnly');
  });

  it('updates an attached virtual TP on its parent id and preserves the sibling SL atomically', async () => {
    const { exchange, post } = createExchange();
    const parent = createParentOrder();
    const takeProfit = createVirtualOrder(parent, 'tp');
    const stopLoss = createVirtualOrder(parent, 'sl');
    exchange.store.update({ orders: [parent, takeProfit, stopLoss] });

    await expect(exchange.updateOrder({ order: takeProfit, update: { price: 51_250.08 } })).resolves.toEqual([
      parent.id,
    ]);

    expect(post).toHaveBeenCalledWith(
      ENDPOINTS.MODIFY_ORDER,
      expect.objectContaining({
        orderId: parent.id,
        qty: '1.25',
        price: '50000',
        tpPrice: '51250.1',
        tpStopType: 'MARK_PRICE',
        slPrice: '49000',
        slStopType: 'LAST_PRICE',
        slOrderType: 'LIMIT',
        slOrderPrice: '48950',
      }),
    );
  });

  it('rejects a malformed combined price-and-amount virtual TP update instead of ignoring its quantity', async () => {
    const { exchange, post } = createExchange();
    const parent = createParentOrder();
    const takeProfit = createVirtualOrder(parent, 'tp');
    exchange.store.update({ orders: [parent, takeProfit] });

    await expect(
      exchange.updateOrder({ order: takeProfit, update: { price: 51_250, amount: 0.5 } } as any),
    ).rejects.toThrow('attached TP/SL quantity cannot be amended independently');

    expect(post).not.toHaveBeenCalled();
  });

  it('repeating the same desired amend reuses the stable order id and exact payload', async () => {
    const { exchange, post } = createExchange();
    const order = createParentOrder({ info: { ...createParentOrder().info, tpPrice: null, slPrice: null } });

    await expect(exchange.updateOrder({ order, update: { price: 50_200 } })).resolves.toEqual([order.id]);
    await expect(exchange.updateOrder({ order, update: { price: 50_200 } })).resolves.toEqual([order.id]);

    expect(post).toHaveBeenCalledTimes(2);
    expect(post.mock.calls[1]).toEqual(post.mock.calls[0]);
  });

  it.each([
    {
      name: 'standalone TP/SL order with ambiguous route',
      order: createParentOrder({
        id: 'tpsl-order-1',
        type: OrderType.StopLoss,
        info: { id: 'tpsl-order-1', positionId: 'position-1', slPrice: '49000', slQty: '1' },
      }),
      update: { price: 48_500 },
      error: 'supports only reconciled pending LIMIT orders',
    },
    {
      name: 'standalone TP/SL display leg with raw exchange id in info',
      order: createParentOrder({
        id: 'tpsl-order-1__position-1__sl',
        type: OrderType.StopLoss,
        info: { id: 'tpsl-order-1', orderId: 'tpsl-order-1', positionId: 'position-1', slPrice: '49000', slQty: '1' },
      }),
      update: { price: 48_500 },
      error: 'supports only reconciled pending LIMIT orders',
    },
    {
      name: 'unreconciled regular order id',
      order: createParentOrder({ info: { ...createParentOrder().info, orderId: 'different-order-id' } }),
      update: { price: 50_100 },
      error: 'exact pending LIMIT order identity is not reconciled',
    },
    {
      name: 'unreconciled reduce-only state',
      order: createParentOrder({ reduceOnly: true }),
      update: { price: 50_100 },
      error: 'reduce-only state is not reconciled',
    },
    {
      name: 'quantity below the already-filled amount',
      order: createParentOrder(),
      update: { amount: 0.1 },
      error: 'must exceed the already-filled quantity',
    },
  ])('rejects $name before any exchange write', async ({ order, update, error }) => {
    const { exchange, post } = createExchange();
    const emitAndLogError = vi.spyOn(exchange, 'emitAndLogError');

    await expect(exchange.updateOrder({ order, update } as any)).rejects.toThrow(error);

    expect(emitAndLogError).toHaveBeenCalledWith(expect.stringContaining(error));
    expect(post).not.toHaveBeenCalled();
  });

  it.each([
    {
      name: 'HTTP-200 exchange rejection',
      response: { data: { code: 10005, data: null, msg: 'Order does not exist' } },
      error: 'Order does not exist',
    },
    {
      name: 'mismatched acknowledgement id',
      response: { data: { code: 0, data: { orderId: 'different-order' }, msg: 'Success' } },
      error: 'did not confirm order bitunix-order-1',
    },
  ])('surfaces $name without canceling or placing a replacement', async ({ response, error }) => {
    const { exchange, post } = createExchange();
    post.mockResolvedValue(response);
    const cancelOrders = vi.spyOn(exchange, 'cancelOrders');
    const placeOrder = vi.spyOn(exchange, 'placeOrder');

    await expect(
      exchange.updateOrder({ order: createParentOrder(), update: { price: 50_100 } }),
    ).rejects.toThrow(error);

    expect(post).toHaveBeenCalledOnce();
    expect(cancelOrders).not.toHaveBeenCalled();
    expect(placeOrder).not.toHaveBeenCalled();
  });
});
