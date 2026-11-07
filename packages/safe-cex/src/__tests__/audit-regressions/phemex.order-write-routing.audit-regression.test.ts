import { describe, expect, it, vi } from 'vitest';

import { PhemexExchange } from '../reference/phemex/phemex.exchange';
import { ENDPOINTS } from '../../exchanges/phemex/phemex.types';
import { OrderSide, OrderStatus, OrderType } from '../../types';
import { DefaultStore } from '../../store/store.base';

describe('Phemex order-write routing audit regression', () => {
  it('keeps placement and cancellation off the rate-limited account-polling client', async () => {
    const exchange = new PhemexExchange('phemex' as any, { key: 'test', secret: 'test' } as any, new DefaultStore());
    const unlimitedPut = vi.fn().mockResolvedValue({ data: { code: 0, data: { orderID: 'new-order' } } });
    const unlimitedDelete = vi.fn().mockResolvedValue({ data: { code: 0 } });
    exchange.unlimitedXHR = { put: unlimitedPut, delete: unlimitedDelete } as any;
    exchange.xhr = { put: vi.fn(), delete: vi.fn() } as any;

    vi.spyOn(exchange as any, 'formatCreateOrder').mockResolvedValue([{ symbol: 'BTCUSDT' }]);
    vi.spyOn(exchange as any, 'getOrderPositionSide').mockResolvedValue('Long');

    await expect(exchange.placeOrder({} as any)).resolves.toEqual(['new-order']);
    await exchange.cancelOrders([
      {
        id: 'existing-order',
        symbol: 'BTCUSDT',
        side: OrderSide.Buy,
        type: OrderType.Limit,
        status: OrderStatus.Open,
        amount: 1,
        price: 1,
      } as any,
    ]);

    expect(unlimitedPut).toHaveBeenCalledWith(ENDPOINTS.ORDER, undefined, { params: { symbol: 'BTCUSDT' } });
    expect(unlimitedDelete).toHaveBeenCalledWith(ENDPOINTS.CANCEL_ORDER, {
      params: { symbol: 'BTCUSDT', posSide: 'Long', orderID: 'existing-order' },
    });
    expect(exchange.xhr.put).not.toHaveBeenCalled();
    expect(exchange.xhr.delete).not.toHaveBeenCalled();
  });
});
