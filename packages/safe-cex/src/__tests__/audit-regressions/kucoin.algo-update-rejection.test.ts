import type { Axios } from 'axios';

import { describe, expect, it, vi } from 'vitest';

import { KucoinExchange } from '../reference/kucoin/kucoin.exchange';
import { DefaultStore } from '../../store/store.base';
import { Market, OrderType } from '../../types';

const MARKET: Market = {
  id: 'XBTUSDTM',
  symbol: 'BTCUSDT',
  base: 'BTC',
  quote: 'USDT',
  active: true,
  precision: { amount: 1, price: 0.1 },
  limits: {
    amount: { min: 1, max: 1_000_000 },
    leverage: { min: 1, max: 100 },
  },
  contractSize: 0.001,
};

describe('KuCoin algo cancel-replace rejection', () => {
  it('surfaces a replacement failure after the original protection was cancelled', async () => {
    const store = new DefaultStore();
    store.update({ markets: [MARKET] });
    const exchange = new KucoinExchange(
      'kucoin',
      {
        key: 'test-key',
        secret: 'test-secret',
        passphrase: 'test-passphrase',
        tealstreetTunnel: false,
      },
      store,
    );
    const cancellation = vi.fn().mockResolvedValue({ data: { code: '200000' } });
    const replacementError = new Error('replacement rejected');
    const replacement = vi.fn().mockRejectedValue(replacementError);
    exchange.unlimitedXHR = {
      delete: cancellation,
      post: replacement,
    } as unknown as Axios;
    const emitAndLogError = vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => undefined as any);
    const [order] = exchange.mapOrder({
      id: 'stop-order-id',
      symbol: MARKET.id,
      status: 'open',
      type: 'market',
      side: 'sell',
      price: '0',
      size: '5',
      filledSize: '0',
      reduceOnly: true,
      stop: 'down',
      stopPrice: '49000',
    });

    expect(order.type).toBe(OrderType.StopLoss);
    await expect(exchange.updateOrder({ order, update: { price: 48_000 } })).rejects.toBe(replacementError);

    expect(cancellation).toHaveBeenCalledWith('/api/v1/orders/stop-order-id', {
      params: { symbol: MARKET.id },
    });
    expect(replacement).toHaveBeenCalledWith(
      '/api/v1/orders',
      expect.objectContaining({ symbol: MARKET.id, stopPrice: '48000', size: '5' }),
    );
    expect(cancellation.mock.invocationCallOrder[0]).toBeLessThan(replacement.mock.invocationCallOrder[0]);
    expect(emitAndLogError).toHaveBeenCalledWith('replacement rejected');

    exchange.dispose();
  });
});
