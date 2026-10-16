import type { ExchangeName, Ticker } from '@tealstreet/safe-cex/src/types';

import { DEFAULT_PUBLIC_EXCHANGE_CACHE_NAMESPACE } from './buckets';
import type { RedisLikeMarketCacheClient } from './marketCache';

/**
 * Last-known tickers for a venue, so a starting exchange has prices before its
 * own feed produces any.
 *
 * Deliberately not bucketed like markets: a ticker is only interesting while
 * it is current, so this is a single key. Reads carry a max age too, because a
 * TTL only bounds how long Redis keeps a value, not how stale the value a
 * reader accepts may be.
 *
 * THIS IS A BOOTSTRAP SEED, NOT A QUOTE. Its whole job is to make
 * `loaded.tickers` true at init so private accounts do not queue behind a
 * blocking `fetchTickers` (measured at 4.6s before it existed); the venue's
 * public WebSocket overwrites every value within a second or so of the feed
 * starting. So the acceptable age is set by "will this unblock startup", not
 * by "is this a price someone could trade on".
 *
 * MAX AGE IS THE DIAL, AND IT SETS TWO THINGS AT ONCE:
 *
 *  - The write cadence. A writer that publishes less often than the reader
 *    accepts leaves the key unusable for the difference, so
 *    `TICKER_WRITE_THROTTLE_MS` in exchange-server's `public-data-cache.ts`
 *    must stay comfortably under this.
 *  - How stale a CONNECTING CLIENT's first ticker render can be. The seed is
 *    not as invisible as it looks: `ExchangeWorker._prepareStorePayload` emits
 *    the full `tickers` array on the first payload after init, so the seeded
 *    values land in the server's `lastStoreData` and are replayed to every
 *    session that connects before the venue's first real update overwrites
 *    them (ordinarily ~1s after an exchange restart). The browser's own 20s
 *    frozen-ticker tripwire is the backstop.
 *
 * That second point is why this is 150s rather than the 5 minutes the write
 * rate alone would justify: the ~6x cut in writes comes from the CADENCE, and
 * max age only has to clear it, so there is no reason to buy staleness we do
 * not need.
 */

export const TICKER_CACHE_VERSION = 'v1';
export const DEFAULT_TICKER_CACHE_MAX_AGE_MS = 150_000;
/**
 * Derived, never hand-set: a TTL below the max age deletes the key while a
 * reader would still have accepted it, which silently turns every raise of the
 * max age into a no-op. Expressing it as a multiple makes that unbreakable
 * rather than something a test has to keep catching.
 */
export const DEFAULT_TICKER_CACHE_TTL_MS = DEFAULT_TICKER_CACHE_MAX_AGE_MS * 2;

export type SerializedTickerCacheSnapshot = {
  schemaVersion: 1;
  exchange: ExchangeName;
  refreshedAt: number;
  tickers: Ticker[];
};

export const getTickerCacheKey = (
  exchange: ExchangeName,
  namespace: string = DEFAULT_PUBLIC_EXCHANGE_CACHE_NAMESPACE,
): string => `${namespace}:tickers:${TICKER_CACHE_VERSION}:${exchange}`;

export const writeTickerCacheSnapshot = async ({
  client,
  exchange,
  tickers,
  namespace = DEFAULT_PUBLIC_EXCHANGE_CACHE_NAMESPACE,
  refreshedAt = Date.now(),
  ttlMs = DEFAULT_TICKER_CACHE_TTL_MS,
}: {
  client: RedisLikeMarketCacheClient;
  exchange: ExchangeName;
  tickers: Ticker[];
  namespace?: string;
  refreshedAt?: number;
  ttlMs?: number;
}): Promise<SerializedTickerCacheSnapshot> => {
  const snapshot: SerializedTickerCacheSnapshot = { schemaVersion: 1, exchange, refreshedAt, tickers };

  await client.set(getTickerCacheKey(exchange, namespace), JSON.stringify(snapshot), 'PX', ttlMs);

  return snapshot;
};

export const parseTickerCacheSnapshot = (raw: string | null): SerializedTickerCacheSnapshot | null => {
  if (!raw) return null;

  try {
    const parsed = JSON.parse(raw) as Partial<SerializedTickerCacheSnapshot>;
    if (parsed?.schemaVersion !== 1) return null;
    if (!Array.isArray(parsed.tickers) || parsed.tickers.length === 0) return null;
    if (typeof parsed.refreshedAt !== 'number' || !parsed.exchange) return null;

    return parsed as SerializedTickerCacheSnapshot;
  } catch {
    return null;
  }
};

export const readTickerCacheSnapshot = async ({
  client,
  exchange,
  namespace = DEFAULT_PUBLIC_EXCHANGE_CACHE_NAMESPACE,
  maxAgeMs = DEFAULT_TICKER_CACHE_MAX_AGE_MS,
  now = Date.now(),
}: {
  client: RedisLikeMarketCacheClient;
  exchange: ExchangeName;
  namespace?: string;
  maxAgeMs?: number;
  now?: number;
}): Promise<SerializedTickerCacheSnapshot | null> => {
  const snapshot = parseTickerCacheSnapshot(await client.get(getTickerCacheKey(exchange, namespace)));
  if (!snapshot) return null;
  if (now - snapshot.refreshedAt > maxAgeMs) return null;

  return snapshot;
};
