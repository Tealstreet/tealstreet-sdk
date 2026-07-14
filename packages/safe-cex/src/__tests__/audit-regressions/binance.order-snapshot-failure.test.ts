import { afterEach, describe, expect, it, vi } from 'vitest';

import { BinanceExchange } from '../../exchanges/binance/binance.exchange';
import { BinancePrivateWebsocket } from '../../exchanges/binance/binance.ws-private';
import { DefaultStore } from '../../store/store.base';

const preservedOrders = [{ id: 'regular-order' }, { id: 'algo-order' }] as any[];

const transientFailures = [
  ['429 rate limit', { message: 'Too many requests', response: { status: 429 } }],
  ['5xx response', { message: 'Internal server error', response: { status: 503 } }],
  ['network failure', new Error('socket hang up')],
] as const;

const createExchange = () => {
  const store = new DefaultStore();
  store.update({ orders: preservedOrders as any });
  const exchange = new BinanceExchange('binance', { key: 'test-key', secret: 'test-secret' }, store);
  return { exchange, store };
};

const runStartupSnapshot = async (exchange: BinanceExchange) => {
  Object.assign(exchange, {
    isDisposed: false,
    privateWebsocket: { connectAndSubscribe: vi.fn() },
    fetchPositionMode: vi.fn().mockResolvedValue(false),
    tick: vi.fn().mockResolvedValue(undefined),
    fetchPositions: vi.fn(),
  });
  await exchange._startPrivate();
};

const snapshotCommitters = [
  ['private startup', (exchange: BinanceExchange) => runStartupSnapshot(exchange)],
  ['private-WS throttled refresh', (exchange: BinanceExchange) => {
    const websocket = new BinancePrivateWebsocket(exchange);
    return (websocket as any).fetchThrottledOrders();
  }],
  ['private-WS reconnect snapshot', (exchange: BinanceExchange) => exchange.fetchAndSetPrivateSnapshot()],
  ['worker-triggered full refresh', (exchange: BinanceExchange) => exchange.fetchAndSetOrders()],
] as const;

describe('Binance order snapshot audit regression', () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it.each(transientFailures)(
    'returns the live order snapshot after a %s from either open-orders endpoint',
    async (_name, error) => {
      vi.spyOn(console, 'error').mockImplementation(() => undefined);
      for (const failedEndpoint of ['regular', 'algo'] as const) {
        const { exchange, store } = createExchange();
        const get = vi.spyOn(exchange.xhr, 'get');
        if (failedEndpoint === 'regular') {
          get.mockRejectedValueOnce(error).mockResolvedValueOnce({ data: [] } as any);
        } else {
          get.mockResolvedValueOnce({ data: [] } as any).mockRejectedValueOnce(error);
        }

        await expect(exchange.fetchOrders()).resolves.toBe(store.orders);
        expect(store.orders).toBe(preservedOrders);
        expect(get).toHaveBeenCalledTimes(2);
      }
    },
  );

  it.each(snapshotCommitters)(
    '%s cannot turn a transient fetch failure into an empty authoritative snapshot',
    async (_name, commitSnapshot) => {
      const { exchange, store } = createExchange();
      vi.spyOn(console, 'error').mockImplementation(() => undefined);
      vi.spyOn(exchange.xhr, 'get').mockRejectedValue({ message: 'Too many requests', response: { status: 429 } });

      await commitSnapshot(exchange);

      expect(store.orders).toBe(preservedOrders);
      expect(store.loaded.orders).toBe(true);
    },
  );

  it.each(snapshotCommitters)('%s clears orders after a successful genuinely empty response', async (_name, commitSnapshot) => {
    const { exchange, store } = createExchange();
    vi.spyOn(exchange.xhr, 'get').mockResolvedValue({ data: [] } as any);

    await commitSnapshot(exchange);

    expect(store.orders).toEqual([]);
    expect(store.loaded.orders).toBe(true);
  });
});
