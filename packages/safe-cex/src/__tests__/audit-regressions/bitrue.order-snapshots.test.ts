import { afterEach, expect, it, vi } from 'vitest';

import { FrameStore } from '../../store/frameStore';
import { OrderSide, OrderStatus, OrderStopTriggerType, OrderType } from '../../types';
import { nativeFixture } from '../cutover/nativeFixture';
import { BitrueExchange } from '../reference/bitrue/bitrue.exchange';

const fixture = () => {
  vi.useFakeTimers();
  const store = new FrameStore();
  store.apply(
    nativeFixture(
      {
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
      },
      store.markets.map((row) => row.symbol),
    ),
  );
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
  store.apply(
    nativeFixture(
      { orders: rows },
      store.markets.map((row) => row.symbol),
    ),
  );
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
    get: vi.fn().mockResolvedValue({
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
  store.apply(
    nativeFixture(
      { orders: [previous] },
      store.markets.map((row) => row.symbol),
    ),
  );
  exchange.xhr = {
    get: vi.fn().mockResolvedValue({
      data: { code: '0', data: [{ ...parent, childOrders: [{ ...child, orderId: undefined }] }] },
    }),
  } as any;
  expect(await exchange.fetchOrdersForSymbol('E-BTC-USDT')).toEqual([previous]);
});

// Rule 5 / #3640: the retained oracle must publish REST/cancel facts as SCXF.
it('publishes scoped REST censuses and clears terminal rows without changing another symbol', async () => {
  const { exchange, store } = fixture();
  const market = store.markets[0];
  store.apply(nativeFixture({ markets: [market, { ...market, id: 'E-ETH-USDT', symbol: 'E-ETH-USDT', base: 'ETH' }] }));
  vi.spyOn(exchange.xhr, 'get').mockResolvedValue({ data: { code: '0', data: [parent] } });
  const [order] = await exchange.fetchOrdersForSymbol('E-BTC-USDT');
  const other = { ...order, id: 'other', symbol: 'E-ETH-USDT' };
  store.apply(
    nativeFixture(
      { orders: [{ ...order, id: 'terminal', status: OrderStatus.Closed }, other] },
      store.markets.map((row) => row.symbol),
    ),
  );
  const result = await exchange.fetchAndSetOrders(['E-BTC-USDT']);
  expect(result).toHaveLength(1);
  expect(store.orders.map((row) => row.id)).toEqual(['other', parent.orderId]);
  expect(store.orders[1]).toMatchObject({ id: result[0].id, status: OrderStatus.Open });
  vi.spyOn(exchange.xhr, 'get').mockResolvedValue({ data: { code: '0', data: [] } });
  await exchange.fetchAndSetOrders(['E-BTC-USDT']);
  expect(store.orders.map((row) => row.id)).toEqual(['other']);
  exchange.dispose();
});

it('publishes acknowledged cancellations without the deleted mutable-store worker method', async () => {
  const { exchange, store } = fixture();
  vi.spyOn(exchange.xhr, 'get').mockResolvedValue({ data: { code: '0', data: [parent] } });
  const [order] = await exchange.fetchOrdersForSymbol('E-BTC-USDT');
  const other = { ...order, id: 'other' };
  store.apply(
    nativeFixture(
      { orders: [order, other] },
      store.markets.map((row) => row.symbol),
    ),
  );
  const post = vi.spyOn(exchange.xhr, 'post').mockResolvedValue({ data: { code: '0', data: { orderId: order.id } } });
  await exchange.cancelOrders([order]);
  expect(post).toHaveBeenCalledExactlyOnceWith('/fapi/v2/cancel', {
    orderId: order.id,
    contractName: order.symbol,
    conditionOrder: false,
  });
  expect(store.orders.map((row) => row.id)).toEqual(['other']);
  exchange.dispose();
});

it('retains order state when a scoped read or cancellation fails', async () => {
  const { exchange, store } = fixture();
  vi.spyOn(exchange.xhr, 'get').mockResolvedValue({ data: { code: '0', data: [parent] } });
  const [order] = await exchange.fetchOrdersForSymbol('E-BTC-USDT');
  store.apply(
    nativeFixture(
      { orders: [order] },
      store.markets.map((row) => row.symbol),
    ),
  );
  const before = store.orders;
  vi.spyOn(exchange.xhr, 'get').mockRejectedValue(new Error('read failed'));
  await exchange.fetchAndSetOrders(['E-BTC-USDT']);
  expect(store.orders).toEqual(before);
  vi.spyOn(exchange.xhr, 'post').mockResolvedValue({ data: { code: '1', msg: 'cancel refused' } });
  await expect(exchange.cancelOrders([order])).rejects.toThrow('cancel refused');
  expect(store.orders).toEqual(before);
  exchange.dispose();
});

it('publishes empty per-symbol censuses during the paced account sweep', async () => {
  const { exchange, store } = fixture();
  vi.spyOn(exchange.xhr, 'get').mockResolvedValue({ data: { code: '0', data: [parent] } });
  const [order] = await exchange.fetchOrdersForSymbol('E-BTC-USDT');
  store.apply(
    nativeFixture(
      { orders: [{ ...order, status: OrderStatus.Closed }] },
      store.markets.map((row) => row.symbol),
    ),
  );
  vi.spyOn(exchange.xhr, 'get').mockResolvedValue({ data: { code: '0', data: [] } });
  const sweep = exchange.fetchAndEmitOrders();
  await vi.runAllTimersAsync();
  await sweep;
  expect(store.orders).toEqual([]);
  exchange.dispose();
});
