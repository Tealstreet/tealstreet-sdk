import { describe, expect, it, vi } from 'vitest';

import { OrderlyExchange } from '../reference/orderly/orderly.exchange';
import { ENDPOINTS } from '../reference/orderly/orderly.types';
import { DefaultStore } from '../../store/store.base';
import { OrderSide, OrderStatus, OrderType } from '../../types';

const createExchange = () => {
  const store = new DefaultStore();
  store.update({
    orders: [
      {
        id: 'take-profit-child',
        parentId: 'positional-parent',
        symbol: 'BTCUSDC',
        type: OrderType.TakeProfit,
        side: OrderSide.Sell,
        status: OrderStatus.Open,
        price: 110,
      },
    ] as any,
  });

  return new OrderlyExchange(
    'woofi',
    { key: 'test-key', secret: 'test-secret', applicationId: 'test-account' },
    store,
  );
};

describe('audit regression: Orderly bulk algo cancellation', () => {
  it('awaits direct normal and algo cancellation endpoints without recreating TP/SL siblings', async () => {
    const exchange = createExchange();
    const deleteRequest = vi.fn().mockResolvedValue({ data: { success: true } });
    const recreateSibling = vi.spyOn(exchange, 'placePositionalAlgoOrder');
    exchange.xhr = { delete: deleteRequest } as any;

    await exchange.cancelAllOrders();
    await exchange.cancelAllOrders();

    expect(deleteRequest).toHaveBeenNthCalledWith(1, ENDPOINTS.CANCEL_ORDERS);
    expect(deleteRequest).toHaveBeenNthCalledWith(2, ENDPOINTS.CANCEL_ALGO_ORDERS);
    expect(deleteRequest).toHaveBeenNthCalledWith(3, ENDPOINTS.CANCEL_ORDERS);
    expect(deleteRequest).toHaveBeenNthCalledWith(4, ENDPOINTS.CANCEL_ALGO_ORDERS);
    expect(recreateSibling).not.toHaveBeenCalled();
  });

  it('uses the documented algo endpoint and Orderly symbol encoding for symbol-scoped cancellation', async () => {
    const exchange = createExchange();
    const deleteRequest = vi.fn().mockResolvedValue({ data: { success: true } });
    exchange.xhr = { delete: deleteRequest } as any;

    await exchange.cancelSymbolOrders('BTCUSDC');

    const request = { params: { symbol: 'PERP_BTC_USDC' } };
    expect(deleteRequest).toHaveBeenNthCalledWith(1, ENDPOINTS.CANCEL_SYMBOL_ORDERS, request);
    expect(deleteRequest).toHaveBeenNthCalledWith(2, ENDPOINTS.CANCEL_ALGO_ORDERS, request);
  });

  it.each([
    ['normal orders', new Error('normal cancellation failed'), { data: { success: true } }],
    ['algo orders', { data: { success: true } }, new Error('algo cancellation failed')],
  ])('attempts the other cancellation path when %s cancellation fails', async (_path, firstResult, secondResult) => {
    const exchange = createExchange();
    const deleteRequest = vi
      .fn()
      .mockImplementationOnce(() => (firstResult instanceof Error ? Promise.reject(firstResult) : Promise.resolve(firstResult)))
      .mockImplementationOnce(() => (secondResult instanceof Error ? Promise.reject(secondResult) : Promise.resolve(secondResult)));
    const emitAndLogError = vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => {});
    exchange.xhr = { delete: deleteRequest } as any;

    await exchange.cancelAllOrders();

    expect(deleteRequest).toHaveBeenNthCalledWith(1, ENDPOINTS.CANCEL_ORDERS);
    expect(deleteRequest).toHaveBeenNthCalledWith(2, ENDPOINTS.CANCEL_ALGO_ORDERS);
    expect(emitAndLogError).toHaveBeenCalledTimes(1);
  });
});
