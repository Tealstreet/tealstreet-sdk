import { afterEach, expect, it, vi } from 'vitest';

import { BitrueExchange } from '../reference/bitrue/bitrue.exchange';
import { DefaultStore } from '../../store/store.base';
import { OrderSide, OrderStatus, OrderStopTriggerType, OrderType } from '../../types';

const fixture = () => {
  vi.useFakeTimers();
  const store = new DefaultStore();
  store.update({
    markets: [
      {
        id: 'E-BTC-USDT',
        symbol: 'E-BTC-USDT',
        base: 'BTC',
        quote: 'USDT',
        active: true,
        contractSize: 10,
        precision: { amount: 10, price: 0.1 },
        limits: { amount: { min: 10, max: 10000 }, leverage: { min: 1, max: 100 } },
      },
    ],
  });
  const exchange = new BitrueExchange('bitrue', { key: 'key', secret: 'secret' }, store);
  vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => undefined);
  return { exchange, store };
};
const parent = {
  orderId: 'parent',
  type: 'LIMIT',
  side: 'BUY',
  status: 'NEW',
  action: 'OPEN',
  price: 100,
  origQty: 100,
  executedQty: 0,
};
const child = {
  orderId: '2334151155877155282',
  clientOrderId: '2334151155877155283',
  type: 'MARKET',
  conditionOrder: true,
  triggerType: 1,
  triggerPrice: 90,
  triggerPriceType: 1,
  side: 'BUY',
  status: 'NEW',
  action: 'OPEN',
  origQty: 100,
  executedQty: 0,
};
afterEach(() => vi.useRealTimers());
it('flattens authoritative protective children and keeps their identity on private deltas', async () => {
  const { exchange, store } = fixture();
  exchange.xhr = {
    get: vi.fn().mockResolvedValue({ data: { code: '0', data: [{ ...parent, childOrders: [child] }] } }),
  } as any;
  const rows = await exchange.fetchOrdersForSymbol('E-BTC-USDT');
  expect(rows).toHaveLength(2);
  expect(rows[0].stopLoss).toBe(90);
  expect(rows[1]).toMatchObject({
    id: child.orderId,
    parentId: parent.orderId,
    type: OrderType.StopLoss,
    side: OrderSide.Sell,
    reduceOnly: true,
    amount: 100,
    slTriggerType: OrderStopTriggerType.LastPrice,
  });
  store.update({ orders: rows });
  exchange.privateWebsocket.handleOrderTradeUpdate({
    e: 'ORDER_TRADE_UPDATE',
    E: 1,
    o: {
      s: 'E_BTCUSDT',
      i: child.orderId,
      S: 'BUY',
      o: 'MARKET',
      x: 'NEW',
      X: 'NEW',
      q: '100',
      z: '0',
      p: '0',
    },
  } as any);
  expect(store.orders.find((order) => order.id === child.orderId)).toMatchObject({
    parentId: parent.orderId,
    side: OrderSide.Sell,
    type: OrderType.StopLoss,
    price: 90,
    reduceOnly: true,
    slTriggerType: OrderStopTriggerType.LastPrice,
  });
});
it('classifies standalone conditionals before their MARKET execution type', async () => {
  const { exchange } = fixture();
  exchange.xhr = {
    get: vi
      .fn()
      .mockResolvedValue({
        data: { code: '0', data: [{ ...child, action: 'CLOSE', origQty: undefined, executedQty: 2 }] },
      }),
  } as any;
  expect(await exchange.fetchOrdersForSymbol('E-BTC-USDT')).toMatchObject([
    { type: OrderType.StopLoss, amount: 2, filled: 0, remaining: 2 },
  ]);
});
it('retains the complete previous symbol snapshot when any child is malformed', async () => {
  const { exchange, store } = fixture();
  const previous = {
    id: 'old',
    symbol: 'E-BTC-USDT',
    type: OrderType.Limit,
    status: OrderStatus.Open,
    side: OrderSide.Buy,
    price: 100,
    amount: 100,
    filled: 0,
    remaining: 100,
    reduceOnly: false,
    lastUpdate: 0,
  };
  store.update({ orders: [previous] });
  exchange.xhr = {
    get: vi
      .fn()
      .mockResolvedValue({
        data: { code: '0', data: [{ ...parent, childOrders: [{ ...child, orderId: undefined }] }] },
      }),
  } as any;
  expect(await exchange.fetchOrdersForSymbol('E-BTC-USDT')).toEqual([previous]);
});
