import type { ExchangeName, Market } from '@tealstreet/safe-cex/src/types';

import { DEFAULT_PUBLIC_EXCHANGE_CACHE_NAMESPACE, getMarketCacheKeys } from './buckets';
import {
  buildMarketCacheSnapshot,
  latestPointerFromSnapshot,
  latestPointerMatchesSnapshot,
  parseLatestMarketPointer,
  parseMarketCacheSnapshot,
  serializeLatestMarketPointer,
  serializeMarketCacheSnapshot,
  type SerializedMarketCacheSnapshot,
} from './serialization';

export interface RedisLikeMarketCacheClient {
  get(key: string): Promise<string | null>;
  set(key: string, value: string, ...args: Array<string | number>): Promise<unknown>;
  del(...keys: string[]): Promise<unknown>;
}

export interface RedisLikeMarketCacheLockClient extends RedisLikeMarketCacheClient {
  eval(script: string, numKeys: number, ...args: Array<string | number>): Promise<unknown>;
}

export type WriteMarketCacheSnapshotOptions = {
  client: RedisLikeMarketCacheClient;
  exchange: ExchangeName;
  bucket: string;
  markets: Market[];
  namespace?: string;
  refreshedAt?: number;
  ttlMs?: number;
};

export const DEFAULT_MARKET_CACHE_TTL_MS = 3 * 60 * 60 * 1000;

const setWithOptionalTtl = async (
  client: RedisLikeMarketCacheClient,
  key: string,
  value: string,
  ttlMs?: number,
): Promise<void> => {
  if (ttlMs === undefined) {
    await client.set(key, value);
    return;
  }

  await client.set(key, value, 'PX', ttlMs);
};

export const writeMarketCacheSnapshot = async ({
  client,
  exchange,
  bucket,
  markets,
  namespace = DEFAULT_PUBLIC_EXCHANGE_CACHE_NAMESPACE,
  refreshedAt,
  ttlMs = DEFAULT_MARKET_CACHE_TTL_MS,
}: WriteMarketCacheSnapshotOptions): Promise<SerializedMarketCacheSnapshot> => {
  const snapshot = buildMarketCacheSnapshot({ exchange, bucket, markets, refreshedAt });
  const keys = getMarketCacheKeys(exchange, bucket, namespace);

  await setWithOptionalTtl(client, keys.bucketKey, serializeMarketCacheSnapshot(snapshot), ttlMs);
  await setWithOptionalTtl(
    client,
    keys.latestKey,
    serializeLatestMarketPointer(latestPointerFromSnapshot(snapshot)),
    ttlMs,
  );

  return snapshot;
};

export const readMarketCacheSnapshot = async (
  client: RedisLikeMarketCacheClient,
  exchange: ExchangeName,
  bucket: string,
  namespace = DEFAULT_PUBLIC_EXCHANGE_CACHE_NAMESPACE,
): Promise<SerializedMarketCacheSnapshot | null> => {
  const keys = getMarketCacheKeys(exchange, bucket, namespace);
  return parseMarketCacheSnapshot(await client.get(keys.bucketKey), { exchange, bucket });
};

export const readLatestMarketCacheSnapshot = async (
  client: RedisLikeMarketCacheClient,
  exchange: ExchangeName,
  namespace = DEFAULT_PUBLIC_EXCHANGE_CACHE_NAMESPACE,
): Promise<SerializedMarketCacheSnapshot | null> => {
  const latestKey = getMarketCacheKeys(exchange, '1970010100', namespace).latestKey;
  const pointer = parseLatestMarketPointer(await client.get(latestKey));
  if (!pointer) return null;

  const snapshot = await readMarketCacheSnapshot(client, exchange, pointer.bucket, namespace);
  if (!snapshot) return null;

  return latestPointerMatchesSnapshot(pointer, snapshot) ? snapshot : null;
};
