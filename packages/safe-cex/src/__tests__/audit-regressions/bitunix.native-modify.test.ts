import type { Axios } from 'axios';

import { describe, expect, it, vi } from 'vitest';

import { BitunixExchange } from '../reference/bitunix/bitunix.exchange';
import { ENDPOINTS } from '../reference/bitunix/bitunix.types';
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

    await expect(exchange.updateOrder({ order, update: { price: 50_100.04 } })).resolves.toEqual(['bitunix-order-1']);

    expect(post).toHaveBeenCalledOnce();
    expect(post).toHaveBeenCalledWith(ENDPOINTS.MODIFY_ORDER, {
      orderId: 'bitunix-order-1',
      symbol: MARKET.symbol,
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

  it('amends a sanitized main-thread LIMIT order when raw info was stripped by the worker bridge', async () => {
    const { exchange, post } = createExchange();
    const order = createParentOrder({ info: undefined });

    await expect(exchange.updateOrder({ order, update: { price: 50_100.04 } })).resolves.toEqual(['bitunix-order-1']);

    expect(post).toHaveBeenCalledWith(ENDPOINTS.MODIFY_ORDER, {
      orderId: 'bitunix-order-1',
      symbol: MARKET.symbol,
      qty: '1.25',
      price: '50100',
    });
  });

  it('maps documented pending-order aliases before native modify preflight runs', async () => {
    const { exchange } = createExchange();
    const get = vi.fn().mockResolvedValue({
      data: {
        code: 0,
        data: {
          orderList: [
            {
              orderId: 'bitunix-order-1',
              symbol: MARKET.symbol,
              qty: '1.25',
              tradeQty: '0.25',
              price: '50000',
              side: 'sell',
              type: 'LIMIT',
              effect: 'GTC',
              reduceOnly: false,
              clientId: 'stable-client-id',
              tpPrice: '51000',
              tpStopType: 'LAST',
              slPrice: '49000',
              slStopType: 'MARK',
              mtime: 1_784_505_600_000,
            },
          ],
        },
      },
    });
    exchange.xhr = { get } as unknown as Axios;

    const [parent, takeProfit, stopLoss] = await exchange.fetchRegularOrders();

    expect(parent).toEqual(
      expect.objectContaining({
        id: 'bitunix-order-1',
        type: OrderType.Limit,
        side: OrderSide.Sell,
        tpTriggerType: OrderStopTriggerType.LastPrice,
        slTriggerType: OrderStopTriggerType.MarkPrice,
      }),
    );
    expect(takeProfit).toEqual(
      expect.objectContaining({
        id: 'bitunix-order-1__tp',
        type: OrderType.TakeProfit,
        tpTriggerType: OrderStopTriggerType.LastPrice,
      }),
    );
    expect(stopLoss).toEqual(
      expect.objectContaining({
        id: 'bitunix-order-1__sl',
        type: OrderType.StopLoss,
        slTriggerType: OrderStopTriggerType.MarkPrice,
      }),
    );
  });

  it('sends the requested total base quantity after a partial fill and preserves reduce-only state in place', async () => {
    const { exchange, post } = createExchange();
    const order = createParentOrder({
      reduceOnly: true,
      info: { ...createParentOrder().info, reduceOnly: true, tpPrice: null, slPrice: null },
    });

    await expect(exchange.updateOrder({ order, update: { amount: 0.7504 } })).resolves.toEqual(['bitunix-order-1']);

    expect(post).toHaveBeenCalledWith(ENDPOINTS.MODIFY_ORDER, {
      orderId: 'bitunix-order-1',
      symbol: MARKET.symbol,
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
      takeProfit.id,
    ]);

    expect(post).toHaveBeenCalledWith(
      ENDPOINTS.MODIFY_ORDER,
      expect.objectContaining({
        orderId: parent.id,
        symbol: MARKET.symbol,
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

  it('keeps an attached virtual leg on a replacement parent acknowledgement', async () => {
    const { exchange, post } = createExchange();
    const parent = createParentOrder();
    const takeProfit = createVirtualOrder(parent, 'tp');
    exchange.store.update({ orders: [parent, takeProfit] });
    post.mockResolvedValue({
      data: { code: 0, data: { orderId: 'replacement-parent' }, msg: 'Success' },
    });

    await expect(exchange.updateOrder({ order: takeProfit, update: { price: 51_250 } })).resolves.toEqual([
      'replacement-parent__tp',
    ]);
  });

  it('falls back to cancel/recreate when Bitunix rejects an attached TP modify as unsupported', async () => {
    const { exchange, post } = createExchange();
    const parent = createParentOrder({
      info: {
        ...createParentOrder().info,
        slOrderType: 'MARKET',
        slOrderPrice: undefined,
      },
    });
    const takeProfit = createVirtualOrder(parent, 'tp');
    const stopLoss = createVirtualOrder(parent, 'sl');
    const cancelPost = vi.fn().mockResolvedValue({ data: { code: 0, msg: 'Success' } });
    exchange.xhr = { post: cancelPost } as unknown as Axios;
    exchange.store.update({ orders: [parent, takeProfit, stopLoss] });
    post
      .mockResolvedValueOnce({
        data: { code: 10005, data: null, msg: 'The order does not support this operation' },
      })
      .mockResolvedValueOnce({
        data: { code: 0, data: { orderId: 'replacement-parent' }, msg: 'Success' },
      });

    await expect(exchange.updateOrder({ order: takeProfit, update: { price: 51_250.08 } })).resolves.toEqual([
      'replacement-parent__tp',
    ]);

    expect(post).toHaveBeenNthCalledWith(1, ENDPOINTS.MODIFY_ORDER, expect.any(Object));
    expect(cancelPost).toHaveBeenCalledWith(ENDPOINTS.CANCEL_ORDER, {
      symbol: MARKET.symbol,
      orderList: [{ orderId: parent.id }],
    });
    expect(post).toHaveBeenNthCalledWith(
      2,
      ENDPOINTS.ORDER,
      expect.objectContaining({
        symbol: MARKET.symbol,
        qty: '1.25',
        price: '50000',
        side: 'BUY',
        orderType: 'LIMIT',
        effect: 'GTC',
        tpPrice: '51250.1',
        tpStopType: 'MARK_PRICE',
        tpOrderType: 'MARKET',
        slPrice: '49000',
        slStopType: 'LAST_PRICE',
        slOrderType: 'MARKET',
      }),
    );
    expect(exchange.store.orders.find((candidate) => candidate.id === parent.id)?.status).toBe(OrderStatus.Canceled);
    expect(exchange.store.orders.find((candidate) => candidate.id === takeProfit.id)?.status).toBe(
      OrderStatus.Canceled,
    );
    expect(exchange.store.orders.find((candidate) => candidate.id === stopLoss.id)?.status).toBe(OrderStatus.Canceled);
  });

  it('does not cancel/recreate a regular parent order when Bitunix rejects its amend as unsupported', async () => {
    const { exchange, post } = createExchange();
    const cancelPost = vi.fn().mockResolvedValue({ data: { code: 0, msg: 'Success' } });
    exchange.xhr = { post: cancelPost } as unknown as Axios;
    post.mockResolvedValue({
      data: { code: 10005, data: null, msg: 'The order does not support this operation' },
    });

    await expect(exchange.updateOrder({ order: createParentOrder(), update: { price: 50_100 } })).rejects.toThrow(
      'The order does not support this operation',
    );

    expect(post).toHaveBeenCalledOnce();
    expect(cancelPost).not.toHaveBeenCalled();
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

  it('normalizes private-WebSocket trigger spellings before amending attached protection', async () => {
    const { exchange, post } = createExchange();
    const order = createParentOrder({
      info: {
        ...createParentOrder().info,
        tpStopType: 'Mark',
        slStopType: 'Last',
      },
    });

    await exchange.updateOrder({ order, update: { price: 50_100 } });

    expect(post).toHaveBeenCalledWith(
      ENDPOINTS.MODIFY_ORDER,
      expect.objectContaining({
        tpStopType: 'MARK_PRICE',
        slStopType: 'LAST_PRICE',
      }),
    );
  });

  it('keeps Bitunix default mark-price protection when a private-WebSocket row omits the trigger type', async () => {
    const { exchange, post } = createExchange();
    const order = createParentOrder({
      info: {
        ...createParentOrder().info,
        tpStopType: undefined,
        slPrice: null,
      },
      tpTriggerType: undefined,
    });

    await exchange.updateOrder({ order, update: { price: 50_100 } });

    expect(post).toHaveBeenCalledWith(
      ENDPOINTS.MODIFY_ORDER,
      expect.objectContaining({
        tpStopType: 'MARK_PRICE',
      }),
    );
  });

  it('updates a standalone partial TP/SL display leg through the raw Bitunix TP/SL id', async () => {
    const { exchange, post } = createExchange();
    const takeProfit = createParentOrder({
      id: 'tpsl-order-1__position-1__tp',
      type: OrderType.TakeProfit,
      side: OrderSide.Sell,
      price: 52_000,
      amount: 0.5,
      filled: 0,
      remaining: 0.5,
      reduceOnly: true,
      info: {
        id: 'tpsl-order-1',
        orderId: 'tpsl-order-1',
        positionId: 'position-1',
        tpPrice: '52000',
        tpStopType: 'LAST_PRICE',
        tpOrderType: 'MARKET',
        tpQty: '0.5',
        slPrice: '49000',
        slStopType: 'MARK_PRICE',
        slOrderType: 'MARKET',
        slQty: '0.5',
      },
      tpTriggerType: OrderStopTriggerType.LastPrice,
    });
    const stopLoss = {
      ...takeProfit,
      id: 'tpsl-order-1__position-1__sl',
      type: OrderType.StopLoss,
      price: 49_000,
      slTriggerType: OrderStopTriggerType.MarkPrice,
      tpTriggerType: undefined,
    };
    exchange.store.update({ orders: [takeProfit, stopLoss] });

    await expect(exchange.updateOrder({ order: takeProfit, update: { price: 52_250.04 } })).resolves.toEqual([
      takeProfit.id,
    ]);

    expect(post).toHaveBeenCalledWith(ENDPOINTS.MODIFY_TP_SL_ORDER, {
      orderId: 'tpsl-order-1',
      tpPrice: '52250',
      tpStopType: 'LAST_PRICE',
      tpQty: '0.5',
      tpOrderType: 'MARKET',
      slPrice: '49000',
      slStopType: 'MARK_PRICE',
      slQty: '0.5',
      slOrderType: 'MARKET',
    });
  });

  it('updates a standalone position TP/SL leg through the position modify endpoint while preserving its sibling', async () => {
    const { exchange, post } = createExchange();
    const takeProfit = createParentOrder({
      id: 'position-tpsl-1__position-1__tp',
      type: OrderType.TakeProfit,
      side: OrderSide.Sell,
      price: 52_000,
      amount: 0,
      filled: 0,
      remaining: 0,
      reduceOnly: true,
      info: {
        id: 'position-tpsl-1',
        orderId: 'position-tpsl-1',
        type: 'POSITION_TPSL',
        positionId: 'position-1',
        tpPrice: '52000',
        tpStopType: 'LAST_PRICE',
        slPrice: '49000',
        slStopType: 'MARK_PRICE',
      },
      tpTriggerType: OrderStopTriggerType.LastPrice,
    });
    const stopLoss = {
      ...takeProfit,
      id: 'position-tpsl-1__position-1__sl',
      type: OrderType.StopLoss,
      price: 49_000,
      slTriggerType: OrderStopTriggerType.MarkPrice,
      tpTriggerType: undefined,
    };
    exchange.store.update({ orders: [takeProfit, stopLoss] });

    await expect(exchange.updateOrder({ order: stopLoss, update: { price: 48_750.04 } })).resolves.toEqual([
      stopLoss.id,
    ]);

    expect(post).toHaveBeenCalledWith(ENDPOINTS.MODIFY_POSITION_TP_SL_ORDER, {
      symbol: MARKET.symbol,
      positionId: 'position-1',
      slPrice: '48750',
      slStopType: 'MARK_PRICE',
      tpPrice: '52000',
      tpStopType: 'LAST_PRICE',
    });
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

  it('surfaces an HTTP-200 exchange rejection without canceling or placing a replacement', async () => {
    const { exchange, post } = createExchange();
    post.mockResolvedValue({ data: { code: 10005, data: null, msg: 'Order does not exist' } });
    const cancelOrders = vi.spyOn(exchange, 'cancelOrders');
    const placeOrder = vi.spyOn(exchange, 'placeOrder');

    await expect(exchange.updateOrder({ order: createParentOrder(), update: { price: 50_100 } })).rejects.toThrow(
      'Order does not exist',
    );

    expect(post).toHaveBeenCalledOnce();
    expect(cancelOrders).not.toHaveBeenCalled();
    expect(placeOrder).not.toHaveBeenCalled();
  });

  it('returns a replacement order id acknowledged by Bitunix', async () => {
    const { exchange, post } = createExchange();
    post.mockResolvedValue({
      data: { code: 0, data: { orderId: 'replacement-order' }, msg: 'Success' },
    });

    await expect(exchange.updateOrder({ order: createParentOrder(), update: { price: 50_100 } })).resolves.toEqual([
      'replacement-order',
    ]);
  });

  it('keeps the submitted order id when a successful acknowledgement omits data', async () => {
    const { exchange, post } = createExchange();
    post.mockResolvedValue({ data: { code: 0, data: null, msg: 'Success' } });

    await expect(exchange.updateOrder({ order: createParentOrder(), update: { price: 50_100 } })).resolves.toEqual([
      'bitunix-order-1',
    ]);
  });

  it('preserves amend provenance when a private-WebSocket UPDATE omits immutable fields', () => {
    const { exchange } = createExchange();
    const order = createParentOrder();
    exchange.store.update({ orders: [order] });

    exchange.privateWebsocket.onMessage({
      data: JSON.stringify({
        ch: 'order',
        data: {
          event: 'UPDATE',
          orderId: order.id,
          symbol: order.symbol,
          type: 'LIMIT',
          price: '50100',
          qty: '1.25',
          dealAmount: '0.25',
          clientId: 'stable-client-id',
        },
      }),
    } as MessageEvent);

    expect(exchange.store.orders[0]).toEqual(
      expect.objectContaining({
        price: 50_100,
        reduceOnly: false,
        info: expect.objectContaining({
          orderId: order.id,
          reduceOnly: false,
          tpPrice: '51000.04',
          slPrice: '49000.04',
        }),
      }),
    );
  });

  it('hydrates reduce-only provenance for a fresh private-WebSocket CREATE from its client id', () => {
    const { exchange } = createExchange();
    exchange.orderReduceOnlyCache.set('reduce-client-id', true);

    exchange.privateWebsocket.onMessage({
      data: JSON.stringify({
        ch: 'order',
        data: {
          event: 'CREATE',
          orderId: 'reduce-order-id',
          clientId: 'reduce-client-id',
          symbol: MARKET.symbol,
          type: 'LIMIT',
          side: 'SELL',
          price: '51000',
          qty: '0.25',
          dealAmount: '0',
          effect: 'GTC',
          ctime: '2026-07-23T17:00:00.000Z',
          mtime: '2026-07-23T17:00:00.000Z',
        },
      }),
    } as MessageEvent);

    expect(exchange.store.orders[0]).toEqual(
      expect.objectContaining({
        id: 'reduce-order-id',
        reduceOnly: true,
        info: expect.objectContaining({ reduceOnly: true }),
      }),
    );
  });
});
