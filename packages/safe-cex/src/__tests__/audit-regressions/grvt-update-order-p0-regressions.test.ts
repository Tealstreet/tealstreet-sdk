import { describe, expect, it, vi } from 'vitest';

import { GrvtExchange } from '../reference/grvt/grvt.exchange';
import { DefaultStore } from '../../store/store.base';
import { OrderSide, OrderStatus, OrderStopTriggerType, OrderTimeInForce, OrderType } from '../../types';

const makeExchange = () => {
  const store = new DefaultStore();
  store.update({
    markets: [
      {
        id: 'BTC_USDT_Perp',
        symbol: 'BTC_USDT_PERP',
        base: 'BTC',
        quote: 'USDT',
        active: true,
        info: { ih: '0x030501', bd: 9, qd: 6 },
        precision: { amount: 0.001, price: 0.1 },
        limits: { amount: { min: 0.001, max: 100 }, leverage: { min: 1, max: 100 } },
      },
    ] as any,
    loaded: { ...store.loaded, markets: true },
  });
  const exchange = new GrvtExchange('grvt' as any, {} as any, store);
  exchange.grvtTradingAccountId = '7';
  return exchange;
};

const rawStopOrder = (overrides: Record<string, unknown> = {}) => ({
  order_id: '0xstop',
  is_market: false,
  time_in_force: 'GOOD_TILL_TIME',
  post_only: true,
  reduce_only: true,
  legs: [{ instrument: 'BTC_USDT_Perp', is_buying_asset: false, limit_price: '58000', size: '2' }],
  state: { status: 'OPEN', book_size: ['2'], traded_size: ['0'], update_time: '1700000000000000000' },
  metadata: {
    client_order_id: '501',
    trigger: {
      trigger_type: 'STOP_LOSS',
      tpsl: { trigger_by: 'MARK', trigger_price: '60000', close_position: true, is_split_position: false },
    },
  },
  ...overrides,
});

describe('GRVT P0 audit regressions', () => {
  it('maps REST and private-WS orders with all replacement-critical semantics', () => {
    const exchange = makeExchange();
    const restOrder = (exchange as any).mapOrder(rawStopOrder());

    expect(restOrder).toMatchObject({
      type: OrderType.StopLoss,
      price: 60000,
      reduceOnly: true,
      close: true,
      timeInForce: OrderTimeInForce.PostOnly,
      slTriggerType: OrderStopTriggerType.MarkPrice,
    });

    const wsOrder = (exchange.privateWebsocket as any).parseOrder({
      oi: '0xws',
      ti: 'IMMEDIATE_OR_CANCEL',
      po: false,
      ro: true,
      l: [{ i: 'BTC_USDT_Perp', ib: true, lp: '0', s: '1' }],
      s1: { s: 'OPEN', ts: ['0'], ut: '1700000000000000000' },
      m: {
        co: '502',
        t: { tt: 'TAKE_PROFIT', t: { tb: 'INDEX', tp: '70000', cp: false, is: false } },
      },
    });

    expect(wsOrder).toMatchObject({
      id: '502',
      type: OrderType.TakeProfit,
      price: 70000,
      reduceOnly: true,
      close: false,
      timeInForce: OrderTimeInForce.ImmediateOrCancel,
      tpTriggerType: OrderStopTriggerType.IndexPrice,
    });
  });

  it('preserves post-only, reduce-only, and complete TP/SL semantics through cancel-replace', async () => {
    const exchange = makeExchange();
    const order = (exchange as any).mapOrder(rawStopOrder());
    const cancelOrder = vi.fn().mockResolvedValue(undefined);
    const placeOrder = vi.fn().mockResolvedValue(['replacement-nonce']);
    exchange.cancelOrder = cancelOrder;
    exchange.placeOrder = placeOrder;

    await expect(exchange.updateOrder({ order, update: { price: 59000 } })).resolves.toEqual(['replacement-nonce']);

    expect(cancelOrder).toHaveBeenCalledWith(order);
    expect(placeOrder).toHaveBeenCalledWith(
      {
        symbol: 'BTC_USDT_PERP',
        type: OrderType.StopLoss,
        side: OrderSide.Sell,
        amount: 2,
        price: 59000,
        triggerPrice: 59000,
        reduceOnly: true,
        timeInForce: OrderTimeInForce.PostOnly,
        close: true,
        slTriggerType: OrderStopTriggerType.MarkPrice,
      },
      { isMarket: false, limitPrice: 58000 },
    );
  });

  it('does not cancel an order when its upstream time-in-force or trigger metadata cannot be preserved', async () => {
    const exchange = makeExchange();
    const cancelOrder = vi.fn().mockResolvedValue(undefined);
    exchange.cancelOrder = cancelOrder;
    exchange.placeOrder = vi.fn().mockResolvedValue(['replacement-nonce']);

    const missingTif = {
      ...((exchange as any).mapOrder(rawStopOrder({ time_in_force: 'ALL_OR_NONE', post_only: true })) as object),
      status: OrderStatus.Open,
    };
    await expect(exchange.updateOrder({ order: missingTif as any, update: { price: 59000 } })).rejects.toThrow(
      'missing time-in-force',
    );

    const missingTrigger = {
      ...((exchange as any).mapOrder(rawStopOrder()) as object),
      info: { ...rawStopOrder(), metadata: { client_order_id: '502' } },
    };
    await expect(exchange.updateOrder({ order: missingTrigger as any, update: { price: 59000 } })).rejects.toThrow(
      'missing trigger metadata',
    );
    expect(cancelOrder).not.toHaveBeenCalled();
  });

  it.each(['ALL_OR_NONE', 'RETAIL_PRICE_IMPROVEMENT', 'UNKNOWN_TIF'])(
    'does not collapse unsupported %s maker semantics to PostOnly',
    (timeInForce) => {
      const exchange = makeExchange();
      const order = (exchange as any).mapOrder(rawStopOrder({ time_in_force: timeInForce, post_only: true }));

      expect(order.timeInForce).toBeUndefined();
    },
  );

  it('does not cancel a split TP/SL order that PlaceOrderOpts cannot represent', async () => {
    const exchange = makeExchange();
    const cancelOrder = vi.fn().mockResolvedValue(undefined);
    exchange.cancelOrder = cancelOrder;
    exchange.placeOrder = vi.fn().mockResolvedValue(['replacement-nonce']);
    const splitOrder = (exchange as any).mapOrder(
      rawStopOrder({
        metadata: {
          client_order_id: '501',
          trigger: {
            trigger_type: 'STOP_LOSS',
            tpsl: { trigger_by: 'MARK', trigger_price: '60000', close_position: true, is_split_position: true },
          },
        },
      }),
    );

    await expect(exchange.updateOrder({ order: splitOrder, update: { price: 59000 } })).rejects.toThrow(
      'split-position trigger is not supported',
    );
    expect(cancelOrder).not.toHaveBeenCalled();
  });
});

describe('GRVT replacement quantity and signing preflight', () => {
  it.each([
    ['remaining dust below grid', 0.2004, 100],
    ['native price overflow', 1, 100_000_000_000],
  ])('rejects %s before cancelling the original', async (_label, amount, price) => {
    const exchange = makeExchange();
    const order = {
      id: '42',
      symbol: 'BTC_USDT_PERP',
      type: OrderType.Limit,
      side: OrderSide.Buy,
      amount: 1,
      filled: 0.2,
      remaining: 0.8,
      price: 100,
      reduceOnly: false,
      timeInForce: OrderTimeInForce.GoodTillCancel,
      status: OrderStatus.Open,
      lastUpdate: 1700000000000,
    } as any;
    const cancel = vi.fn().mockResolvedValue(undefined);
    const place = vi.fn().mockResolvedValue(['replacement']);
    exchange.cancelOrder = cancel;
    exchange.placeOrder = place;
    await expect(exchange.updateOrder({ order, update: { amount, price } })).rejects.toThrow(/precision|uint64/);
    expect(cancel).not.toHaveBeenCalled();
    expect(place).not.toHaveBeenCalled();
  });

  it('validates quantized native units while passing the original remaining amount to placement', async () => {
    const exchange = makeExchange();
    const order = {
      id: '42',
      symbol: 'BTC_USDT_PERP',
      type: OrderType.Limit,
      side: OrderSide.Buy,
      amount: 1,
      filled: 0.2,
      remaining: 0.8,
      price: 100,
      reduceOnly: false,
      timeInForce: OrderTimeInForce.GoodTillCancel,
      status: OrderStatus.Open,
      lastUpdate: 1700000000000,
    } as any;
    const cancel = vi.fn().mockResolvedValue(undefined);
    const place = vi.fn().mockResolvedValue(['replacement']);
    exchange.cancelOrder = cancel;
    exchange.placeOrder = place;
    await expect(exchange.updateOrder({ order, update: { amount: 0.5014, price: 110 } })).resolves.toEqual([
      'replacement',
    ]);
    expect(cancel).toHaveBeenCalledWith(order);
    expect(place).toHaveBeenCalledWith(expect.objectContaining({ amount: 0.5014 - 0.2, price: 110 }));
  });
});
