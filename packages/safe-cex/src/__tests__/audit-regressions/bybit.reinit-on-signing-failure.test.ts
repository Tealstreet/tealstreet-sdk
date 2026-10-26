import { BybitExchange } from '../reference/bybit/bybit.exchange';
import { ENDPOINTS } from '../../exchanges/bybit/bybit.types';
import { BybitInverseExchange } from '../reference/bybit/bybitinverse.exchange';
import { BybitSpotExchange } from '../reference/bybit/bybitspot.exchange';
import { DefaultStore } from '../../store/store.base';

/**
 * `needsReinit` makes the next tick refetch orders, which bybit's tick otherwise
 * never does. It was only set when the VENUE answered an error; a request that
 * THREW — the hosted remote signer unavailable or timed out — left it false, so a
 * failed startup order fetch stayed empty until a private order frame happened to
 * arrive. And a failed reinit was cleared by the tick that ran it, so it retried
 * once. All three bybit variants carry the same helpers.
 */

const signerDown = new Error('Remote signer unavailable — WebSocket disconnected (gone <5s)');

const VARIANTS = [
  {
    label: 'bybit',
    make: (store: DefaultStore) => new BybitExchange('bybit', options(), store),
    method: 'fetchOrdersHelper',
    args: ['linear', 'USDT'],
  },
  {
    label: 'bybitinverse',
    make: (store: DefaultStore) => new BybitInverseExchange('bybitinverse', options(), store),
    method: 'fetchOrdersHelper',
    args: ['inverse'],
  },
  { label: 'bybitspot', make: (store: DefaultStore) => new BybitSpotExchange('bybitspot', options(), store), method: 'fetchOrders', args: [] },
] as const;

function options() {
  return { key: 'k', secret: 's', testnet: false, tuning: { consoleLogging: false } } as any;
}

describe('bybit refetches orders after a request that threw', () => {
  test.each(VARIANTS)('$label: a thrown order fetch sets needsReinit', async ({ make, method, args }) => {
    const exchange = make(new DefaultStore()) as any;
    vi.spyOn(exchange.xhr, 'get').mockImplementation(async (url: unknown) => {
      if (url === ENDPOINTS.UNFILLED_ORDERS) throw signerDown;
      return { data: { retMsg: 'OK', result: { list: [] } } };
    });

    expect(exchange.needsReinit).toBe(false);
    // Spot's fetchOrders may swallow the error; either way the flag must be set.
    await exchange[method](...args).catch(() => undefined);
    expect(exchange.needsReinit).toBe(true);
    exchange.dispose();
  });

  test('a reinit that fails again keeps the flag and retries on a backoff', async () => {
    vi.useFakeTimers();
    try {
      const exchange = new BybitExchange('bybit', options(), new DefaultStore()) as any;
      exchange.needsReinit = true;
      vi.spyOn(exchange, 'fetchOrdersPositionsAndBalance').mockImplementation(async () => {
        exchange.needsReinit = true;
      });
      vi.spyOn(exchange, 'fetchAndSetPrivateSnapshotHelper').mockResolvedValue(undefined);

      const refetch = exchange.fetchOrdersPositionsAndBalance;
      await exchange.tick();
      expect(exchange.needsReinit).toBe(true);
      expect(refetch).toHaveBeenCalledTimes(1);

      // A persistent failure is retried on a backoff, not on every 500ms tick.
      await exchange.tick();
      expect(refetch).toHaveBeenCalledTimes(1);
      vi.advanceTimersByTime(15_000);
      await exchange.tick();
      expect(refetch).toHaveBeenCalledTimes(1);
      vi.advanceTimersByTime(15_000);
      await exchange.tick();
      expect(refetch).toHaveBeenCalledTimes(2);

      // A reinit that succeeds resets the backoff for the next failure.
      refetch.mockImplementation(async () => undefined);
      vi.advanceTimersByTime(60_000);
      await exchange.tick();
      expect(refetch).toHaveBeenCalledTimes(3);
      exchange.needsReinit = true;
      vi.advanceTimersByTime(15_000);
      await exchange.tick();
      expect(refetch).toHaveBeenCalledTimes(4);
      exchange.dispose();
    } finally {
      vi.useRealTimers();
    }
  });
});
