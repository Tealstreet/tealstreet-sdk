import { createHash } from 'node:crypto';

import type { ExchangeName, Market } from '@tealstreet/safe-cex/src/types';

import { isUtcHourMarketBucket } from './buckets';

export const MARKET_CACHE_SCHEMA_VERSION = 1;

export type MarketCacheSchemaVersion = typeof MARKET_CACHE_SCHEMA_VERSION;

export type SerializedMarketCacheSnapshot = {
  schemaVersion: MarketCacheSchemaVersion;
  exchange: ExchangeName;
  bucket: string;
  refreshedAt: number;
  marketCount: number;
  contentHash: string;
  markets: Market[];
};

export type LatestMarketPointer = {
  schemaVersion: MarketCacheSchemaVersion;
  bucket: string;
  refreshedAt: number;
  marketCount: number;
  contentHash: string;
};

type BuildSnapshotOptions = {
  exchange: ExchangeName;
  bucket: string;
  markets: Market[];
  refreshedAt?: number;
};

const isRecord = (value: unknown): value is Record<string, unknown> =>
  typeof value === 'object' && value !== null && !Array.isArray(value);

const isFiniteNumber = (value: unknown): value is number => typeof value === 'number' && Number.isFinite(value);

export const computeMarketContentHash = (markets: Market[]): string =>
  createHash('sha256').update(JSON.stringify(markets)).digest('hex');

export const buildMarketCacheSnapshot = ({
  exchange,
  bucket,
  markets,
  refreshedAt = Date.now(),
}: BuildSnapshotOptions): SerializedMarketCacheSnapshot => {
  if (!isUtcHourMarketBucket(bucket)) {
    throw new Error(`Invalid UTC-hour market bucket: ${bucket}`);
  }

  return {
    schemaVersion: MARKET_CACHE_SCHEMA_VERSION,
    exchange,
    bucket,
    refreshedAt,
    marketCount: markets.length,
    contentHash: computeMarketContentHash(markets),
    markets,
  };
};

export const latestPointerFromSnapshot = (snapshot: SerializedMarketCacheSnapshot): LatestMarketPointer => ({
  schemaVersion: MARKET_CACHE_SCHEMA_VERSION,
  bucket: snapshot.bucket,
  refreshedAt: snapshot.refreshedAt,
  marketCount: snapshot.marketCount,
  contentHash: snapshot.contentHash,
});

export const serializeMarketCacheSnapshot = (snapshot: SerializedMarketCacheSnapshot): string =>
  JSON.stringify(snapshot);

export const serializeLatestMarketPointer = (pointer: LatestMarketPointer): string => JSON.stringify(pointer);

const parseJsonRecord = (raw: string): Record<string, unknown> | null => {
  try {
    const parsed: unknown = JSON.parse(raw);
    return isRecord(parsed) ? parsed : null;
  } catch {
    return null;
  }
};

export const parseMarketCacheSnapshot = (
  raw: string | null,
  expected?: { exchange?: ExchangeName; bucket?: string },
): SerializedMarketCacheSnapshot | null => {
  if (!raw) return null;

  const parsed = parseJsonRecord(raw);
  if (!parsed) return null;
  if (parsed.schemaVersion !== MARKET_CACHE_SCHEMA_VERSION) return null;
  if (typeof parsed.exchange !== 'string') return null;
  if (!isUtcHourMarketBucket(parsed.bucket)) return null;
  if (!isFiniteNumber(parsed.refreshedAt)) return null;
  if (!isFiniteNumber(parsed.marketCount)) return null;
  if (typeof parsed.contentHash !== 'string' || parsed.contentHash.length === 0) return null;
  if (!Array.isArray(parsed.markets)) return null;
  if (parsed.marketCount !== parsed.markets.length) return null;

  const snapshot = parsed as SerializedMarketCacheSnapshot;
  if (expected?.exchange && snapshot.exchange !== expected.exchange) return null;
  if (expected?.bucket && snapshot.bucket !== expected.bucket) return null;
  if (computeMarketContentHash(snapshot.markets) !== snapshot.contentHash) return null;

  return snapshot;
};

export const parseLatestMarketPointer = (raw: string | null): LatestMarketPointer | null => {
  if (!raw) return null;

  const parsed = parseJsonRecord(raw);
  if (!parsed) return null;
  if (parsed.schemaVersion !== MARKET_CACHE_SCHEMA_VERSION) return null;
  if (!isUtcHourMarketBucket(parsed.bucket)) return null;
  if (!isFiniteNumber(parsed.refreshedAt)) return null;
  if (!isFiniteNumber(parsed.marketCount)) return null;
  if (typeof parsed.contentHash !== 'string' || parsed.contentHash.length === 0) return null;

  return parsed as LatestMarketPointer;
};

export const latestPointerMatchesSnapshot = (
  pointer: LatestMarketPointer,
  snapshot: SerializedMarketCacheSnapshot,
): boolean =>
  pointer.schemaVersion === snapshot.schemaVersion &&
  pointer.bucket === snapshot.bucket &&
  pointer.refreshedAt === snapshot.refreshedAt &&
  pointer.marketCount === snapshot.marketCount &&
  pointer.contentHash === snapshot.contentHash;
