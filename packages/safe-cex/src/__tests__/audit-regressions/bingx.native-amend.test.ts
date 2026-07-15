import type { Axios } from 'axios';

import { describe, expect, it, vi } from 'vitest';

import { BingxExchange } from '../../exchanges/bingx/bingx.exchange';
import { ENDPOINTS } from '../../exchanges/bingx/bingx.types';
import { DefaultStore } from '../../store/store.base';
import {
  Market,
  Order,
  OrderSide,
  OrderStatus,
  OrderTimeInForce,
  OrderType,
} from '../../types';

const MARKET: Market = {
  id: 'BTC-USDT',
  symbol: 'BTCUSDT',
  base: 'BTC',
  quote: 'USDT',
  active: true,
  precision: { amount: 0.001, price: 0.1 },
  limits: { amount: { min: 0.001, max: 100 }, leverage: { min: 1, max: 100 } },
};

const createOrder = (overrides: Partial<Order> = {}): Order => ({
  id: '1989246372640980993',
  status: OrderStatus.Open,
  symbol: MARKET.symbol,
  type: OrderType.Limit,
  side: OrderSide.Buy,
  price: 50_000,
  amount: 1.25,
  filled: 0,
  remaining: 1.25,
  reduceOnly: false,
  timeInForce: OrderTimeInForce.GoodTillCancel,
  lastUpdate: 1_784_505_600_000,
  info: {
    orderId: '1989246372640980993',
    symbol: 'BTC-USDT',
    side: 'BUY',
    positionSide: 'BOTH',
    type: 'LIMIT',
    origQty: '1.250',
    executedQty: '0',
    reduceOnly: false,
    timeInForce: 'GTC',
    postOnly: false,
    status: 'NEW',
  },
  ...overrides,
});

const createExchange = () => {
  const store = new DefaultStore();
  store.update({ markets: [MARKET] });
  const exchange = new BingxExchange('bingx', { key: 'key', secret: 'secret' }, store);
  const amendPost = vi.fn().mockImplementation(async (_endpoint, payload) => ({
    data: {
      code: 0,
      msg: '',
      data: {
        orderId: '1989246372640980993',
        clientOrderId: '',
        symbol: 'BTC-USDT',
        quantity: payload.quantity,
      },
    },
  }));
  const replacementPost = vi.fn();
  exchange.xhr = { post: amendPost } as unknown as Axios;
  exchange.unlimitedXHR = { post: replacementPost } as unknown as Axios;

  return { exchange, amendPost, replacementPost };
};

const expectNoReplacementWrites = (exchange: BingxExchange, replacementPost: ReturnType<typeof vi.fn>) => {
  const cancelOrders = vi.spyOn(exchange, 'cancelOrders');
  const placeOrder = vi.spyOn(exchange, 'placeOrder');
  return () => {
    expect(replacementPost).not.toHaveBeenCalled();
    expect(cancelOrders).not.toHaveBeenCalled();
    expect(placeOrder).not.toHaveBeenCalled();
  };
};

describe('BingX native amend safety boundary', () => {
  it('amends an exactly reconciled unfilled LIMIT quantity in place and preserves the stable order id', async () => {
    const { exchange, amendPost, replacementPost } = createExchange();
    const assertNoReplacementWrites = expectNoReplacementWrites(exchange, replacementPost);

    await expect(exchange.updateOrder({ order: createOrder(), update: { amount: 0.7504 } })).resolves.toEqual([
      '1989246372640980993',
    ]);

    expect(amendPost).toHaveBeenCalledOnce();
    expect(amendPost).toHaveBeenCalledWith(ENDPOINTS.AMEND_ORDER, {
      orderId: '1989246372640980993',
      quantity: 0.75,
      symbol: 'BTC-USDT',
    });
    expect(ENDPOINTS.AMEND_ORDER).toBe('/openApi/swap/v1/trade/amend');
    assertNoReplacementWrites();
  });

  it('preserves post-only, reduce-only, side, and hedged position-side attributes by amending only identity and size', async () => {
    const { exchange, amendPost } = createExchange();
    const order = createOrder({
      side: OrderSide.Sell,
      reduceOnly: true,
      timeInForce: OrderTimeInForce.PostOnly,
      info: {
        ...createOrder().info,
        side: 'SELL',
        positionSide: 'SHORT',
        reduceOnly: true,
        timeInForce: 'PostOnly',
        postOnly: true,
      },
    });

    await exchange.updateOrder({ order, update: { amount: 1 } });

    expect(amendPost).toHaveBeenCalledWith(ENDPOINTS.AMEND_ORDER, {
      orderId: order.id,
      quantity: 1,
      symbol: MARKET.id,
    });
    expect(amendPost.mock.calls[0][1]).not.toHaveProperty('side');
    expect(amendPost.mock.calls[0][1]).not.toHaveProperty('positionSide');
    expect(amendPost.mock.calls[0][1]).not.toHaveProperty('reduceOnly');
    expect(amendPost.mock.calls[0][1]).not.toHaveProperty('timeInForce');
  });

  it.each([
    {
      name: 'price edit',
      order: createOrder(),
      update: { price: 50_100 },
      error: 'cannot change price',
    },
    {
      name: 'combined price and quantity edit',
      order: createOrder(),
      update: { price: 50_100, amount: 1 },
      error: 'cannot change price',
    },
    {
      name: 'unchanged normalized quantity',
      order: createOrder(),
      update: { amount: 1.2504 },
      error: 'must change after market-precision normalization',
    },
    {
      name: 'partially filled total-size ambiguity',
      order: createOrder({
        filled: 0.25,
        remaining: 1,
        info: { ...createOrder().info, status: 'PARTIALLY_FILLED', executedQty: '0.25' },
      }),
      update: { amount: 1 },
      error: 'disabled until quantity is documented as total or remaining size',
    },
    {
      name: 'trigger order',
      order: createOrder({ type: OrderType.StopLoss }),
      update: { amount: 1 },
      error: 'supports only exactly reconciled open LIMIT orders',
    },
    {
      name: 'LIMIT with attached protection',
      order: createOrder({
        info: {
          ...createOrder().info,
          takeProfit: { type: 'TAKE_PROFIT_MARKET', stopPrice: '51000', quantity: '1.25' },
        },
      }),
      update: { amount: 1 },
      error: 'cannot prove attached TP/SL quantity preservation',
    },
  ])('rejects $name before any amend, cancel-replace, cancel, or placement write', async ({ order, update, error }) => {
    const { exchange, amendPost, replacementPost } = createExchange();
    const assertNoReplacementWrites = expectNoReplacementWrites(exchange, replacementPost);

    await expect(exchange.updateOrder({ order, update } as any)).rejects.toThrow(error);

    expect(amendPost).not.toHaveBeenCalled();
    assertNoReplacementWrites();
  });

  it.each([
    {
      name: 'mismatched order id',
      info: { ...createOrder().info, orderId: 'different-order' },
      error: 'immutable order identity is not reconciled',
    },
    {
      name: 'mismatched side',
      info: { ...createOrder().info, side: 'SELL' },
      error: 'immutable order identity is not reconciled',
    },
    {
      name: 'invalid position side',
      info: { ...createOrder().info, positionSide: 'UNKNOWN' },
      error: 'immutable order identity is not reconciled',
    },
    {
      name: 'missing side provenance',
      info: { ...createOrder().info, side: undefined },
      error: 'immutable order identity is not reconciled',
    },
    {
      name: 'missing position-side provenance',
      info: { ...createOrder().info, positionSide: undefined },
      error: 'immutable order identity is not reconciled',
    },
    {
      name: 'mismatched reduce-only',
      info: { ...createOrder().info, reduceOnly: true },
      error: 'reduce-only state is not reconciled',
    },
    {
      name: 'missing reduce-only provenance',
      info: { ...createOrder().info, reduceOnly: undefined },
      error: 'reduce-only state is not reconciled',
    },
    {
      name: 'mismatched time in force',
      info: { ...createOrder().info, timeInForce: 'PostOnly', postOnly: true },
      error: 'time in force is not reconciled',
    },
    {
      name: 'missing time-in-force provenance',
      info: { ...createOrder().info, timeInForce: undefined, postOnly: undefined },
      error: 'time in force is not reconciled',
    },
    {
      name: 'unknown time-in-force with post-only false',
      info: { ...createOrder().info, timeInForce: 'UNKNOWN', postOnly: false },
      error: 'time in force is not reconciled',
    },
    {
      name: 'omitted time-in-force with post-only false',
      info: { ...createOrder().info, timeInForce: undefined, postOnly: false },
      error: 'time in force is not reconciled',
    },
    {
      name: 'mismatched original quantity',
      info: { ...createOrder().info, origQty: '2' },
      error: 'disabled until quantity is documented as total or remaining size',
    },
    {
      name: 'missing original quantity',
      info: { ...createOrder().info, origQty: undefined },
      error: 'disabled until quantity is documented as total or remaining size',
    },
    {
      name: 'missing executed quantity',
      info: { ...createOrder().info, executedQty: undefined },
      error: 'disabled until quantity is documented as total or remaining size',
    },
    {
      name: 'missing pending status',
      info: { ...createOrder().info, status: undefined },
      error: 'disabled until quantity is documented as total or remaining size',
    },
  ])('rejects $name before any write', async ({ info, error }) => {
    const { exchange, amendPost, replacementPost } = createExchange();
    const assertNoReplacementWrites = expectNoReplacementWrites(exchange, replacementPost);

    await expect(
      exchange.updateOrder({ order: createOrder({ info }), update: { amount: 0.75 } }),
    ).rejects.toThrow(error);

    expect(amendPost).not.toHaveBeenCalled();
    assertNoReplacementWrites();
  });

  it.each([
    {
      name: 'HTTP-200 exchange rejection',
      response: { data: { code: 100400, msg: 'Invalid parameter', data: {} } },
      error: 'Invalid parameter',
    },
    {
      name: 'mismatched acknowledgement id',
      response: {
        data: { code: 0, msg: '', data: { orderId: 'different-order', symbol: 'BTC-USDT', quantity: 0.75 } },
      },
      error: 'did not confirm order 1989246372640980993',
    },
    {
      name: 'missing acknowledged quantity',
      response: {
        data: { code: 0, msg: '', data: { orderId: '1989246372640980993', symbol: 'BTC-USDT' } },
      },
      error: 'did not confirm order 1989246372640980993',
    },
  ])('surfaces $name without canceling or placing a replacement', async ({ response, error }) => {
    const { exchange, amendPost, replacementPost } = createExchange();
    amendPost.mockResolvedValue(response);
    const assertNoReplacementWrites = expectNoReplacementWrites(exchange, replacementPost);

    await expect(exchange.updateOrder({ order: createOrder(), update: { amount: 0.75 } })).rejects.toThrow(error);

    expect(amendPost).toHaveBeenCalledOnce();
    assertNoReplacementWrites();
  });
});
