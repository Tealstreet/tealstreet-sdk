import type { ExchangeName, Ticker } from '@tealstreet/safe-cex/src/types';

import { describe, expect, it } from 'vitest';

import {
  DEFAULT_TICKER_CACHE_MAX_AGE_MS,
  DEFAULT_TICKER_CACHE_TTL_MS,
  getTickerCacheKey,
  readTickerCacheSnapshot,
  writeTickerCacheSnapshot,
} from './tickerCache';

const BYBIT = 'bybit' as ExchangeName;

function fakeRedis() {
  const store = new Map<string, string>();
  const calls: Array<Array<string | number>> = [];

  return {
    store,
    calls,
    client: {
      get: async (key: string) => store.get(key) ?? null,
      set: async (key: string, value: string, ...args: Array<string | number>) => {
        store.set(key, value);
        calls.push([key, ...args]);
        return 'OK';
      },
      del: async (...keys: string[]) => {
        for (const key of keys) store.delete(key);
        return keys.length;
      },
    },
  };
}

const ticker = (symbol: string, last: number) => ({ symbol, last }) as unknown as Ticker;

describe('ticker cache', () => {
  it('round trips a snapshot', async () => {
    const { client } = fakeRedis();

    await writeTickerCacheSnapshot({ client, exchange: BYBIT, tickers: [ticker('BTCUSDT', 100)], refreshedAt: 1_000 });
    const snapshot = await readTickerCacheSnapshot({ client, exchange: BYBIT, now: 1_500 });

    expect(snapshot?.tickers).toHaveLength(1);
    expect(snapshot?.refreshedAt).toBe(1_000);
  });

  // A TTL bounds how long Redis keeps a value, not how stale a value a reader
  // is willing to seed prices from.
  it('refuses a snapshot older than the max age', async () => {
    const { client } = fakeRedis();

    await writeTickerCacheSnapshot({ client, exchange: BYBIT, tickers: [ticker('BTCUSDT', 100)], refreshedAt: 0 });

    expect(await readTickerCacheSnapshot({ client, exchange: BYBIT, now: DEFAULT_TICKER_CACHE_MAX_AGE_MS + 1 })).toBe(
      null,
    );
  });

  // The behaviour the rate cut depends on, stated as an age rather than as a
  // relation between constants: a snapshot far older than the venue's tick has
  // to still be usable, or a writer publishing every 60s leaves the key
  // unreadable for most of every cycle and the seed stops working at all.
  // Reverting the max age to its old 30s makes this fail.
  it('accepts a seed old enough that a 60s writer is still usable', async () => {
    const { client } = fakeRedis();

    await writeTickerCacheSnapshot({ client, exchange: BYBIT, tickers: [ticker('BTCUSDT', 100)], refreshedAt: 0 });

    const snapshot = await readTickerCacheSnapshot({ client, exchange: BYBIT, now: 90_000 });
    expect(snapshot?.tickers).toHaveLength(1);
  });

  // A TTL under the max age deletes the key while a reader would still have
  // taken it. The constant is derived (`MAX_AGE * 2`) so the relation cannot be
  // broken by editing a number — this pins the BEHAVIOUR instead, and fails if
  // anyone replaces the derivation with a hand-set value that is too small.
  it('writes a TTL that outlives the acceptance window of its own reader', async () => {
    const { calls, client } = fakeRedis();

    await writeTickerCacheSnapshot({ client, exchange: BYBIT, tickers: [ticker('BTCUSDT', 100)] });

    const ttl = Number(calls[0][2]);
    expect(ttl).toBeGreaterThanOrEqual(DEFAULT_TICKER_CACHE_MAX_AGE_MS);
  });

  it('writes with a TTL so an abandoned venue expires', async () => {
    const { calls, client } = fakeRedis();

    await writeTickerCacheSnapshot({ client, exchange: BYBIT, tickers: [ticker('BTCUSDT', 100)], ttlMs: 45_000 });

    expect(calls[0]).toEqual([getTickerCacheKey(BYBIT), 'PX', 45_000]);
  });

  it('returns nothing for a missing, corrupt, or empty snapshot', async () => {
    const { client, store } = fakeRedis();

    expect(await readTickerCacheSnapshot({ client, exchange: BYBIT })).toBe(null);

    store.set(getTickerCacheKey(BYBIT), 'not json');
    expect(await readTickerCacheSnapshot({ client, exchange: BYBIT })).toBe(null);

    store.set(
      getTickerCacheKey(BYBIT),
      JSON.stringify({ schemaVersion: 1, exchange: BYBIT, refreshedAt: 1, tickers: [] }),
    );
    expect(await readTickerCacheSnapshot({ client, exchange: BYBIT, now: 2 })).toBe(null);
  });

  it('ignores a snapshot from a future schema', async () => {
    const { client, store } = fakeRedis();
    store.set(
      getTickerCacheKey(BYBIT),
      JSON.stringify({ schemaVersion: 2, exchange: BYBIT, refreshedAt: 1, tickers: [ticker('BTCUSDT', 1)] }),
    );

    expect(await readTickerCacheSnapshot({ client, exchange: BYBIT, now: 2 })).toBe(null);
  });
});
