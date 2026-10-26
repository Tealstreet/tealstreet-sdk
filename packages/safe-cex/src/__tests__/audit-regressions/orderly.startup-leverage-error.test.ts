import { describe, expect, it, vi } from 'vitest';

import { OrderlyExchange } from '../reference/orderly/orderly.exchange';
import { DefaultStore } from '../../store/store.base';

describe('audit regression: Orderly startup leverage fetch', () => {
  it('observes and emits a fire-and-forget account leverage failure', async () => {
    const exchange = new OrderlyExchange(
      'woofi',
      { key: 'test-key', secret: 'test-secret', applicationId: 'test-account' },
      new DefaultStore(),
    );
    exchange.xhr = { get: vi.fn().mockResolvedValue({ data: { data: {} } }) } as any;
    vi.spyOn(exchange, 'fetchMarkets').mockResolvedValue([]);
    vi.spyOn(exchange, 'fetchTickers').mockResolvedValue([]);
    vi.spyOn(exchange, 'fetchAndUpdateAccountLeverage').mockRejectedValue(new Error('leverage unavailable'));
    vi.spyOn(exchange, 'tick').mockResolvedValue(undefined);
    vi.spyOn(exchange, 'fetchOrders').mockResolvedValue([]);
    vi.spyOn(exchange.publicWebsocket, 'connectAndSubscribe').mockImplementation(() => {});
    vi.spyOn(exchange.privateWebsocket, 'connectAndSubscribe').mockImplementation(() => {});
    const emitAndLogError = vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => {});

    await expect(exchange.start()).resolves.toBeUndefined();
    await vi.waitFor(() => expect(emitAndLogError).toHaveBeenCalledWith('leverage unavailable'));
  });
});
