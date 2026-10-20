import type { Axios } from 'axios';

import { describe, expect, it, vi } from 'vitest';

import { BinanceExchange } from '../../exchanges/binance/binance.exchange';
import { ENDPOINTS } from '../../exchanges/binance/binance.types';
import { BinancePMExchange } from '../../exchanges/binance/binancepm.exchange';
import { DefaultStore } from '../../store/store.base';
import { OrderSide, OrderStatus, OrderType } from '../../types';

const createStore = () => {
  const store = new DefaultStore();
  store.update({
    orders: [
      {
        id: 'regular-order',
        symbol: 'BTCUSDT',
        type: OrderType.Limit,
        side: OrderSide.Buy,
        status: OrderStatus.Open,
      },
      {
        id: 'algo-order',
        symbol: 'BTCUSDT',
        type: OrderType.StopLoss,
        side: OrderSide.Sell,
        status: OrderStatus.Open,
      },
      {
        id: 'other-symbol-order',
        symbol: 'ETHUSDT',
        type: OrderType.TakeProfit,
        side: OrderSide.Sell,
        status: OrderStatus.Open,
      },
    ] as any,
  });
  return store;
};

const createSubject = () => {
  const store = createStore();
  const exchange = new BinanceExchange('binance', { key: 'key', secret: 'secret' }, store);
  return { exchange, store };
};

describe('Binance USD-M symbol cancellation', () => {
  it('uses both documented cancel-all endpoints for regular and Algo Service orders', async () => {
    const { exchange, store } = createSubject();
    const deleteRequest = vi.fn().mockResolvedValue({ data: { code: 200 } });
    exchange.xhr = { delete: deleteRequest } as unknown as Axios;

    await exchange.cancelSymbolOrders('BTCUSDT');

    const request = { params: { symbol: 'BTCUSDT' } };
    expect(ENDPOINTS.CANCEL_SYMBOL_ORDERS).toBe('/fapi/v1/allOpenOrders');
    expect(ENDPOINTS.CANCEL_SYMBOL_ALGO_ORDERS).toBe('/fapi/v1/algoOpenOrders');
    expect(deleteRequest).toHaveBeenNthCalledWith(1, ENDPOINTS.CANCEL_SYMBOL_ORDERS, request);
    expect(deleteRequest).toHaveBeenNthCalledWith(2, ENDPOINTS.CANCEL_SYMBOL_ALGO_ORDERS, request);
    expect(store.orders.find((order) => order.id === 'regular-order')?.status).toBe(OrderStatus.Canceled);
    expect(store.orders.find((order) => order.id === 'algo-order')?.status).toBe(OrderStatus.Canceled);
    expect(store.orders.find((order) => order.id === 'other-symbol-order')?.status).toBe(OrderStatus.Open);
  });

  it.each([
    ['regular', new Error('regular cancel failed'), { data: { code: 200 } }, 'regular-order', 'algo-order'],
    ['algo', { data: { code: 200 } }, new Error('algo cancel failed'), 'algo-order', 'regular-order'],
  ])(
    'attempts both paths and preserves the %s family when only that cancellation fails',
    async (_family, regularResult, algoResult, openOrderId, canceledOrderId) => {
      const { exchange, store } = createSubject();
      const deleteRequest = vi
        .fn()
        .mockImplementationOnce(() =>
          regularResult instanceof Error ? Promise.reject(regularResult) : Promise.resolve(regularResult),
        )
        .mockImplementationOnce(() =>
          algoResult instanceof Error ? Promise.reject(algoResult) : Promise.resolve(algoResult),
        );
      const emitAndLogError = vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => undefined);
      exchange.xhr = { delete: deleteRequest } as unknown as Axios;

      await expect(exchange.cancelSymbolOrders('BTCUSDT')).rejects.toThrow(`${_family} cancel failed`);

      expect(deleteRequest).toHaveBeenCalledTimes(2);
      expect(store.orders.find((order) => order.id === openOrderId)?.status).toBe(OrderStatus.Open);
      expect(store.orders.find((order) => order.id === canceledOrderId)?.status).toBe(OrderStatus.Canceled);
      expect(emitAndLogError).toHaveBeenCalledWith(`${_family} cancel failed`);
    },
  );
});

describe('Binance PM symbol cancellation', () => {
  const regularPath = '/papi/v1/um/allOpenOrders';
  const conditionalPath = '/papi/v1/um/conditional/allOpenOrders';
  const createPMSubject = () => {
    const store = createStore();
    const exchange = new BinancePMExchange('binancepm', { key: 'key', secret: 'secret' }, store);
    return { exchange, store };
  };

  it('cancels both native families even when the local symbol has no orders', async () => {
    const { exchange, store } = createPMSubject();
    const deleteRequest = vi.fn().mockResolvedValue({ data: { code: 200 } });
    exchange.papiXhr = { delete: deleteRequest } as unknown as Axios;

    await exchange.cancelSymbolOrders('BTCUSDT');

    expect(deleteRequest.mock.calls).toEqual([
      [regularPath, { params: { symbol: 'BTCUSDT' } }],
      [conditionalPath, { params: { symbol: 'BTCUSDT' } }],
    ]);
    expect(store.orders.find((order) => order.id === 'regular-order')?.status).toBe(OrderStatus.Canceled);
    expect(store.orders.find((order) => order.id === 'algo-order')?.status).toBe(OrderStatus.Canceled);
    expect(store.orders.find((order) => order.id === 'other-symbol-order')?.status).toBe(OrderStatus.Open);

    deleteRequest.mockClear();
    await exchange.cancelSymbolOrders('SOLUSDT');
    expect(deleteRequest.mock.calls).toEqual([
      [regularPath, { params: { symbol: 'SOLUSDT' } }],
      [conditionalPath, { params: { symbol: 'SOLUSDT' } }],
    ]);
  });

  it.each(['regular', 'conditional'])(
    'preserves the failed %s family and still cancels its sibling',
    async (failedFamily) => {
      const { exchange, store } = createPMSubject();
      const failure = new Error(`${failedFamily} cancel failed`);
      const deleteRequest = vi.fn().mockImplementation((path: string) =>
        path === (failedFamily === 'regular' ? regularPath : conditionalPath)
          ? Promise.reject(failure)
          : Promise.resolve({ data: { code: 200 } }),
      );
      const emit = vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => undefined);
      exchange.papiXhr = { delete: deleteRequest } as unknown as Axios;

      await expect(exchange.cancelSymbolOrders('BTCUSDT')).rejects.toBe(failure);

      expect(deleteRequest).toHaveBeenCalledTimes(2);
      expect(store.orders.find((order) => order.id === 'regular-order')?.status).toBe(
        failedFamily === 'regular' ? OrderStatus.Open : OrderStatus.Canceled,
      );
      expect(store.orders.find((order) => order.id === 'algo-order')?.status).toBe(
        failedFamily === 'conditional' ? OrderStatus.Open : OrderStatus.Canceled,
      );
      expect(store.orders.find((order) => order.id === 'other-symbol-order')?.status).toBe(OrderStatus.Open);
      expect(emit).toHaveBeenCalledWith(failure.message);
    },
  );

  it('preserves a racing fill and orders added after cancellation began', async () => {
    const { exchange, store } = createPMSubject();
    const releases: Array<() => void> = [];
    exchange.papiXhr = {
      delete: vi.fn(() => new Promise((resolve) => releases.push(() => resolve({ data: { code: 200 } })))),
    } as unknown as Axios;
    const pending = exchange.cancelSymbolOrders('BTCUSDT');
    store.updateOrdersStatus([{ id: 'regular-order' }], OrderStatus.Closed);
    store.addOrUpdateOrders([{ ...store.orders[0], id: 'new-order', status: OrderStatus.Open }]);
    for (const release of releases) release();
    await pending;

    expect(store.orders.find((order) => order.id === 'regular-order')?.status).toBe(OrderStatus.Closed);
    expect(store.orders.find((order) => order.id === 'algo-order')?.status).toBe(OrderStatus.Canceled);
    expect(store.orders.find((order) => order.id === 'new-order')?.status).toBe(OrderStatus.Open);
  });
});

it.each([
  ['USD-M', 'before'],
  ['USD-M', 'during'],
  ['PM', 'before'],
  ['PM', 'during'],
])('%s preserves a complete fill %s cancellation and a later placement', async (profile, timing) => {
  const store = createStore();
  const exchange = profile === 'PM'
    ? new BinancePMExchange('binancepm', { key: 'key', secret: 'secret' }, store)
    : new BinanceExchange('binance', { key: 'key', secret: 'secret' }, store);
  const releases: Array<() => void> = [];
  const deleteRequest = vi.fn(() => new Promise((resolve) => releases.push(() => resolve({ data: { code: 200 } }))));
  const client = { delete: deleteRequest } as unknown as Axios;
  if (exchange instanceof BinancePMExchange) exchange.papiXhr = client;
  else exchange.xhr = client;
  const fill = () => store.updateOrder({ id: 'regular-order' }, {
    status: OrderStatus.Closed,
    amount: 0.002,
    filled: 0.002,
    remaining: 0,
  });
  if (timing === 'before') fill();
  const pending = exchange.cancelSymbolOrders('BTCUSDT');
  if (timing === 'during') fill();
  store.addOrUpdateOrders([{ ...store.orders[0], id: 'later-order', status: OrderStatus.Open, filled: 0, remaining: 0.002 }]);
  for (const release of releases) release();
  await pending;

  expect(deleteRequest).toHaveBeenCalledTimes(2);
  expect(store.orders.find((order) => order.id === 'regular-order')).toMatchObject({
    status: OrderStatus.Closed, amount: 0.002, filled: 0.002, remaining: 0,
  });
  expect(store.orders.find((order) => order.id === 'algo-order')?.status).toBe(OrderStatus.Canceled);
  expect(store.orders.find((order) => order.id === 'later-order')?.status).toBe(OrderStatus.Open);
  expect(store.orders.find((order) => order.id === 'other-symbol-order')?.status).toBe(OrderStatus.Open);
});
