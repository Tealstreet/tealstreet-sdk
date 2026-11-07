import type { Axios } from 'axios';

import { describe, expect, it, vi } from 'vitest';

import { BinanceExchange } from '../reference/binance/binance.exchange';
import { ENDPOINTS } from '../../exchanges/binance/binance.types';
import { DefaultStore } from '../../store/store.base';
import { OrderSide, OrderStatus, OrderType } from '../../types';

const createSubject = () => {
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
