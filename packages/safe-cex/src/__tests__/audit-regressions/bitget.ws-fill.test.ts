import { describe, expect, it, vi } from 'vitest';

import { BitgetExchange } from '../reference/bitget/bitget.exchange';
import { DefaultStore } from '../../store/store.base';

const createExchange = () =>
  new BitgetExchange(
    'bitget',
    { key: 'test-key', secret: 'test-secret', passphrase: 'test-passphrase' },
    new DefaultStore(),
  );

describe('audit regression: Bitget private order fills', () => {
  it.each([
    { baseVolume: '0.125', accBaseVolume: '0.75', expected: 0.125 },
    { baseVolume: '', accBaseVolume: '0.75', expected: 0.75 },
  ])('emits partially_filled using the documented fill quantity fields', ({ baseVolume, accBaseVolume, expected }) => {
    const exchange = createExchange();
    vi.spyOn(exchange, 'fetchOrders').mockResolvedValue([]);
    vi.spyOn(exchange, 'fetchBalance').mockResolvedValue({ total: 0, free: 0, used: 0, upnl: 0 });
    const emit = vi.spyOn(exchange.emitter, 'emit');

    exchange.privateWebsocket.handleOrderTopic({
      data: [
        {
          orderId: 'order-1',
          symbol: 'BTCUSDT',
          orderType: 'limit',
          side: 'buy',
          price: '100',
          priceAvg: '101',
          size: '1',
          baseVolume,
          accBaseVolume,
          status: 'partially_filled',
          force: 'gtc',
        },
      ],
    });

    expect(emit).toHaveBeenCalledWith(
      'fill',
      expect.objectContaining({ id: 'order-1', symbol: 'BTCUSDT', price: '101', amount: expected }),
    );
    expect(exchange.store.orders).toHaveLength(1);
  });
});
